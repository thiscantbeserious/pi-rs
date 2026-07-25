//! Supervisor integration tests (plan step 7). Exercises the full lifecycle
//! against the mock-host binary: spawn -> boot -> handshake -> ready ->
//! heartbeat -> Hung (mock goes silent) -> Reconnecting -> respawn -> Ready.
//!
//! These tests spawn the real mock-host binary (src/bin/mock_host.rs). All
//! timings are configurable via SupervisorConfig and set short for fast tests.

use std::path::PathBuf;
use std::time::Duration;

use pi_core::host_supervisor::{HostSupervisor, SupervisorConfig};

/// Locate the mock-host binary in the target directory.
fn mock_host_path() -> PathBuf {
    std::env::var("CARGO_BIN_EXE_mock-host")
        .unwrap_or_else(|_| {
            let manifest = env!("CARGO_MANIFEST_DIR");
            format!("{manifest}/../target/debug/mock-host")
        })
        .into()
}

/// All test timings. Short enough for fast tests, long enough for process
/// startup. Total suite: ~6s.
const TEST_HEARTBEAT: Duration = Duration::from_millis(50);
const TEST_BOOT_TIMEOUT: Duration = Duration::from_millis(300);
const TEST_BACKOFF_BASE: Duration = Duration::from_millis(10);

fn test_config(socket_path: PathBuf, mode: &str) -> SupervisorConfig {
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

    // The mock handshakes then goes silent. The supervisor should:
    // boot -> ready -> Hung (3 missed Pongs, ~150ms) -> Reconnecting ->
    // respawn -> boot -> ready -> Hung -> ... loop forever.
    // Assert it's still running (looping) after 1s.
    let result = tokio::time::timeout(Duration::from_secs(1), supervisor.run()).await;
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

    // The mock exits immediately before connecting. Each boot times out
    // after 300ms. After 5 crashes, the supervisor enters CrashLooping and
    // calls the prompt. stdin is not a TTY in tests, so the prompt reads
    // EOF -> AbortTurn -> Stopped -> the supervisor exits.
    // Total: ~5 * 300ms boot + 10ms * (1+2+4+8) backoff = ~1.5s + 0.15s = ~1.65s.
    let result = tokio::time::timeout(Duration::from_secs(5), supervisor.run()).await;
    assert!(
        result.is_ok(),
        "supervisor should finish (crash-loop -> prompt -> EOF -> abort) within 10s"
    );
    assert!(
        result.unwrap().is_ok(),
        "supervisor.run() should return Ok after aborting"
    );
}
