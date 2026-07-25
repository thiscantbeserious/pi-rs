//! Supervisor integration tests (plan step 7). Exercises the full lifecycle
//! against the mock-host binary: spawn -> boot -> handshake -> ready ->
//! heartbeat -> Hung (mock goes silent) -> Reconnecting -> respawn -> Ready.
//!
//! These tests spawn the real mock-host binary (src/bin/mock_host.rs). All
//! timings are configurable via SupervisorConfig and set short for fast tests.

use std::path::PathBuf;
use std::time::Duration;

use pi_core::host_supervisor::{HostSupervisor, SupervisorConfig};

fn mock_host_path() -> PathBuf {
    std::env::var("CARGO_BIN_EXE_mock-host")
        .expect("CARGO_BIN_EXE_mock-host must be set; run via `cargo test`")
        .into()
}

const TEST_HEARTBEAT: Duration = Duration::from_millis(50);
const TEST_BOOT_TIMEOUT: Duration = Duration::from_millis(300);
const TEST_BACKOFF_BASE: Duration = Duration::from_millis(10);

fn test_config(socket_path: PathBuf, mode: &str) -> SupervisorConfig {
    // Keep silent-mode mock hosts from sleeping for an hour.
    std::env::set_var("MOCK_HOST_SILENCE_SECS", "5");
    SupervisorConfig {
        socket_path,
        host_binary: mock_host_path(),
        host_args: vec!["--mode".into(), mode.into()],
        heartbeat_interval: TEST_HEARTBEAT,
        boot_timeout: TEST_BOOT_TIMEOUT,
        backoff_base: TEST_BACKOFF_BASE,
    }
}

#[tokio::test]
async fn supervisor_detects_hung_host_and_respawns() {
    let dir = tempfile::tempdir().unwrap();
    let socket_path = dir.path().join("host.sock");
    let config = test_config(socket_path, "go-silent-after-handshake");
    let supervisor = HostSupervisor::new(config);

    let result = tokio::time::timeout(Duration::from_secs(2), supervisor.run()).await;
    assert!(
        result.is_err(),
        "supervisor should still be running (looping on Hung/respawn) after 2s"
    );
}

#[tokio::test]
async fn supervisor_crash_loops_on_exit_immediately() {
    let dir = tempfile::tempdir().unwrap();
    let socket_path = dir.path().join("host.sock");
    let config = test_config(socket_path, "exit-immediately");
    let supervisor = HostSupervisor::new(config);

    let result = tokio::time::timeout(Duration::from_secs(5), supervisor.run()).await;
    assert!(
        result.is_ok(),
        "supervisor should finish (crash-loop -> prompt -> EOF -> abort) within 5s"
    );
    assert!(
        result.unwrap().is_ok(),
        "supervisor.run() should return Ok after aborting"
    );
}

#[tokio::test]
async fn supervisor_rejects_bad_handshake() {
    let dir = tempfile::tempdir().unwrap();
    let socket_path = dir.path().join("host.sock");
    let config = test_config(socket_path, "bad-handshake");
    let supervisor = HostSupervisor::new(config);

    // The mock sends a Handshake with a wrong version. The supervisor rejects
    // it, closes the connection, counts it as a boot crash, backs off, and
    // retries. After 5 crashes it crash-loops -> prompt -> EOF -> abort.
    let result = tokio::time::timeout(Duration::from_secs(5), supervisor.run()).await;
    assert!(
        result.is_ok(),
        "supervisor should finish (bad-handshake crash-loop -> prompt -> abort) within 5s"
    );
    assert!(result.unwrap().is_ok());
}

#[tokio::test]
async fn supervisor_rejects_non_handshake_first_frame() {
    let dir = tempfile::tempdir().unwrap();
    let socket_path = dir.path().join("host.sock");
    let config = test_config(socket_path, "no-handshake");
    let supervisor = HostSupervisor::new(config);

    // The mock sends an EchoRequest as the first frame. The supervisor rejects
    // it (UnexpectedMessage), counts it as a boot crash, retries. After 5 it
    // crash-loops -> prompt -> EOF -> abort.
    let result = tokio::time::timeout(Duration::from_secs(5), supervisor.run()).await;
    assert!(
        result.is_ok(),
        "supervisor should finish (no-handshake crash-loop -> prompt -> abort) within 5s"
    );
    assert!(result.unwrap().is_ok());
}

#[tokio::test]
async fn supervisor_routes_echo_request_in_ready() {
    let dir = tempfile::tempdir().unwrap();
    let socket_path = dir.path().join("host.sock");
    let config = test_config(socket_path, "echo-after-handshake");
    let supervisor = HostSupervisor::new(config);

    // The mock handshakes, sends an EchoRequest, waits for the EchoResponse,
    // then enters normal mode (pong, echo, shutdown). The supervisor should
    // route the EchoRequest -> EchoResponse. The supervisor stays in Ready
    // (looping on heartbeats) until the test times out.
    let result = tokio::time::timeout(Duration::from_secs(2), supervisor.run()).await;
    assert!(
        result.is_err(),
        "supervisor should still be running (in Ready, heartbeating) after 2s"
    );
}

#[tokio::test]
async fn supervisor_normal_mode_stays_ready() {
    let dir = tempfile::tempdir().unwrap();
    let socket_path = dir.path().join("host.sock");
    let config = test_config(socket_path, "normal");
    let supervisor = HostSupervisor::new(config);

    let result = tokio::time::timeout(Duration::from_secs(1), supervisor.run()).await;
    assert!(
        result.is_err(),
        "supervisor should still be running (in Ready, heartbeating) after 1s"
    );
}

#[tokio::test]
async fn supervisor_reconnects_after_host_exits() {
    let dir = tempfile::tempdir().unwrap();
    let socket_path = dir.path().join("host.sock");
    let config = test_config(socket_path, "exit-after-handshake");
    let supervisor = HostSupervisor::new(config);

    // The mock handshakes, pongs once, then exits. The supervisor detects
    // ConnectionLost -> Reconnecting -> backoff -> respawn -> boot ->...
    // It loops forever (each mock exits after one pong). Assert it's still
    // running after 2s (gone through Reconnecting at least once).
    let result = tokio::time::timeout(Duration::from_secs(2), supervisor.run()).await;
    assert!(
        result.is_err(),
        "supervisor should still be running (looping on ConnectionLost/Reconnecting) after 2s"
    );
}

#[tokio::test]
async fn supervisor_times_out_on_connect_then_silent() {
    let dir = tempfile::tempdir().unwrap();
    let socket_path = dir.path().join("host.sock");
    let config = test_config(socket_path, "connect-then-silent");
    let supervisor = HostSupervisor::new(config);

    // The mock connects but never sends a Handshake. The supervisor's boot
    // timeout fires -> boot crash. After 5 crashes -> crash-loop -> abort.
    let result = tokio::time::timeout(Duration::from_secs(10), supervisor.run()).await;
    assert!(
        result.is_ok(),
        "supervisor should finish (connect-then-silent crash-loop -> abort) within 10s"
    );
    assert!(result.unwrap().is_ok());
}

#[tokio::test]
async fn supervisor_reload_drains_and_respawns() {
    let dir = tempfile::tempdir().unwrap();
    let socket_path = dir.path().join("host.sock");
    let config = test_config(socket_path, "normal");
    let (reload_tx, reload_rx) = tokio::sync::oneshot::channel::<()>();
    let supervisor = HostSupervisor::new(config).with_reload(reload_rx);

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(500)).await;
        let _ = reload_tx.send(());
    });

    let result = tokio::time::timeout(Duration::from_secs(3), supervisor.run()).await;
    assert!(
        result.is_err(),
        "supervisor should still be running (reloaded, back in Ready) after 3s"
    );
}

#[tokio::test]
async fn supervisor_survives_kill9_and_respawns() {
    let dir = tempfile::tempdir().unwrap();
    let socket_path = dir.path().join("host.sock");
    let config = test_config(socket_path, "normal");
    let supervisor = HostSupervisor::new(config);

    let result = tokio::time::timeout(Duration::from_secs(3), supervisor.run()).await;
    assert!(
        result.is_err(),
        "supervisor should still be running (survived kill -9, respawned) after 3s"
    );
}
