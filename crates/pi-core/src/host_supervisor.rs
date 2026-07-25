//! The host supervisor (ADR 0023 Q5). Owns the `HostState`, the crash count,
//! and the backoff timer. Spawns the host process and a connection task. Runs
//! the heartbeat timer (5s, ADR 0022 Q7) and owns the miss count (grill Q7:
//! supervisor owns the heartbeat, NOT the connection task). Drives state
//! transitions on events from the connection. Single writer of lifecycle
//! state.
//!
//! The connection triple (`ConnTriple`: upstream receiver, downstream sender,
//! connection task handle, child process) lives in the `Ready` and `Draining`
//! state structs (grill Q8). Transitions move or drop it.
//!
//! Structure: `run()` is a thin loop that calls `transition(state)` per
//! iteration. Each transition method does the I/O for one state and returns
//! the next state. Policy decisions (which state to go to) are extracted into
//! pure functions with unit tests.

use std::path::PathBuf;
use std::time::Duration;

use pi_protocol::{EchoResponse, Message, ProtocolError, ProtocolErrorCode};
use tokio::net::UnixListener;
use tokio::process::Command;
use tokio::sync::{mpsc, oneshot};
use tokio::time::{interval, Instant};

use crate::host_connection::run_connection;
use crate::host_state::{
    backoff_for_scaled, validate_handshake, ConnTriple, HostProcess, HostState,
};

/// Consecutive missed Pongs before declaring the host hung (ADR 0022 Q7: 3).
const MISSED_PONG_THRESHOLD: u32 = 3;
/// Consecutive failed boots before crash-loop (ADR 0023 Q2: 5).
const CRASH_LOOP_THRESHOLD: u32 = 5;

/// A decision from the native restart prompt (ADR 0023 Q3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureDecision {
    Restart,
    BypassOnce,
    AbortTurn,
}

/// Configuration for the supervisor.
pub struct SupervisorConfig {
    pub socket_path: PathBuf,
    pub host_binary: PathBuf,
    pub host_args: Vec<String>,
    pub heartbeat_interval: Duration,
    pub boot_timeout: Duration,
    pub backoff_base: Duration,
}

/// The host supervisor. Construct with `new`, run with `run`.
pub struct HostSupervisor {
    config: SupervisorConfig,
    /// A signal the caller fires to trigger /reload. The ready_phase's select!
    /// listens on this; when it fires, the supervisor sends Shutdown{drain:true},
    /// waits for ShutdownAck, and transitions to Stopped -> respawn.
    reload_signal: Option<oneshot::Receiver<()>>,
}

impl HostSupervisor {
    pub fn new(config: SupervisorConfig) -> Self {
        Self {
            config,
            reload_signal: None,
        }
    }

    /// Set the reload signal. The returned sender triggers /reload when fired.
    /// The supervisor's ready_phase listens on the receiver.
    pub fn with_reload(mut self, reload_signal: oneshot::Receiver<()>) -> Self {
        self.reload_signal = Some(reload_signal);
        self
    }

    /// Run the supervisor loop. Returns when the host reaches a terminal
    /// Stopped state (after AbortTurn, or a fatal bind error).
    pub async fn run(mut self) -> std::io::Result<()> {
        let listener = UnixListener::bind(&self.config.socket_path)?;
        let mut state = HostState::stopped();
        let mut crash_count: u32 = 0;
        let mut reload = self.reload_signal.take();

        loop {
            // Only pass the reload signal when entering Ready; it stays
            // None for all other transitions so it survives Stopped -> Booting.
            let reload_for_ready = if matches!(state, HostState::Ready(_)) {
                reload.take()
            } else {
                None
            };
            state = self
                .transition(state, &mut crash_count, &listener, reload_for_ready)
                .await?;
            // Drained means /reload completed: respawn, not terminate.
            if matches!(state, HostState::Stopped(_)) && reload.is_none() {
                break;
            }
        }

        let _ = std::fs::remove_file(&self.config.socket_path);
        Ok(())
    }

    async fn transition(
        &self,
        state: HostState,
        crash_count: &mut u32,
        listener: &UnixListener,
        reload_signal: Option<oneshot::Receiver<()>>,
    ) -> std::io::Result<HostState> {
        match state {
            HostState::Stopped(_) => self.transition_from_stopped().await,
            HostState::Booting(booting) => {
                self.transition_from_booting(listener, booting, crash_count)
                    .await
            }
            HostState::Ready(ready) => {
                self.transition_from_ready(ready, crash_count, reload_signal)
                    .await
            }
            HostState::Hung(_) => Ok(self.transition_from_hung()),
            HostState::Draining(drain) => self.transition_from_draining(drain, crash_count).await,
            HostState::BackingOff(bo) => self.transition_from_backing_off(bo).await,
            HostState::Reconnecting(re) => self.transition_from_reconnecting(re).await,
            HostState::CrashLooping(cl) => {
                self.transition_from_crash_looping(cl, crash_count).await
            }
        }
    }

    async fn transition_from_stopped(&self) -> std::io::Result<HostState> {
        let process = self.spawn_host().await?;
        Ok(HostState::Booting(crate::host_state::Booting { process }))
    }

    async fn transition_from_booting(
        &self,
        listener: &UnixListener,
        booting: crate::host_state::Booting,
        crash_count: &mut u32,
    ) -> std::io::Result<HostState> {
        match self.boot_phase(listener, booting).await? {
            BootResult::Ready(conn) => {
                *crash_count = 0;
                Ok(HostState::Ready(crate::host_state::Ready {
                    conn,
                    missed_pongs: 0,
                }))
            }
            BootResult::Crash => {
                *crash_count += 1;
                Ok(decide_boot_crash(*crash_count, self.config.backoff_base))
            }
        }
    }

    async fn transition_from_ready(
        &self,
        ready: crate::host_state::Ready,
        crash_count: &mut u32,
        reload_signal: Option<oneshot::Receiver<()>>,
    ) -> std::io::Result<HostState> {
        match self.ready_phase(ready, reload_signal).await? {
            ReadyResult::Hung(backoff) => {
                Ok(HostState::Reconnecting(crate::host_state::Reconnecting {
                    deadline: Instant::now() + backoff,
                }))
            }
            ReadyResult::Drained => {
                // /reload completed: respawn immediately (ADR 0017).
                *crash_count = 0;
                let process = self.spawn_host().await?;
                Ok(HostState::Booting(crate::host_state::Booting { process }))
            }
            ReadyResult::ConnectionLost => {
                Ok(HostState::Reconnecting(crate::host_state::Reconnecting {
                    deadline: Instant::now() + backoff_for_scaled(1, self.config.backoff_base),
                }))
            }
        }
    }

    fn transition_from_hung(&self) -> HostState {
        // Transient: the Core closed the socket in ready_phase. First death
        // auto-respawns (ADR 0023 Q6).
        HostState::Reconnecting(crate::host_state::Reconnecting {
            deadline: Instant::now() + backoff_for_scaled(1, self.config.backoff_base),
        })
    }

    async fn transition_from_draining(
        &self,
        drain: crate::host_state::Draining,
        crash_count: &mut u32,
    ) -> std::io::Result<HostState> {
        let (_, conn) = drain.on_drained();
        conn.shutdown().await;
        *crash_count = 0;
        Ok(HostState::Stopped(crate::host_state::Stopped))
    }

    async fn transition_from_backing_off(
        &self,
        bo: crate::host_state::BackingOff,
    ) -> std::io::Result<HostState> {
        tokio::time::sleep_until(bo.deadline).await;
        let process = self.spawn_host().await?;
        Ok(HostState::Booting(crate::host_state::Booting { process }))
    }

    async fn transition_from_reconnecting(
        &self,
        re: crate::host_state::Reconnecting,
    ) -> std::io::Result<HostState> {
        tokio::time::sleep_until(re.deadline).await;
        let process = self.spawn_host().await?;
        Ok(HostState::Booting(crate::host_state::Booting { process }))
    }

    async fn transition_from_crash_looping(
        &self,
        cl: crate::host_state::CrashLooping,
        crash_count: &mut u32,
    ) -> std::io::Result<HostState> {
        let decision = prompt_failure(cl.crash_count);
        match decision {
            FailureDecision::Restart => {
                *crash_count = 0;
                let process = self.spawn_host().await?;
                Ok(HostState::Booting(crate::host_state::Booting { process }))
            }
            FailureDecision::BypassOnce | FailureDecision::AbortTurn => {
                cl.on_abort();
                Ok(HostState::Stopped(crate::host_state::Stopped))
            }
        }
    }

    async fn spawn_host(&self) -> std::io::Result<HostProcess> {
        let mut cmd = Command::new(&self.config.host_binary);
        cmd.args(&self.config.host_args);
        cmd.env("PI_RS_HOST_SOCKET", &self.config.socket_path);
        let child = cmd.spawn()?;
        Ok(HostProcess { child })
    }

    /// The boot phase: accept the connection, validate the handshake, send
    /// HandshakeAck. Returns the ConnTriple on success, Crash if the host
    /// exits or sends a bad handshake before completing.
    async fn boot_phase(
        &self,
        listener: &UnixListener,
        booting: crate::host_state::Booting,
    ) -> std::io::Result<BootResult> {
        let process = booting.process;
        let stream = match tokio::time::timeout(self.config.boot_timeout, listener.accept()).await {
            Ok(Ok((stream, _))) => stream,
            Ok(Err(e)) => return Err(e),
            Err(_) => return Ok(BootResult::Crash),
        };

        let (upstream_tx, mut upstream_rx) = mpsc::channel::<Message>(32);
        let (downstream_tx, downstream_rx) = mpsc::channel::<Message>(32);
        let conn_task = tokio::spawn(run_connection(stream, upstream_tx, downstream_rx));

        let first = tokio::time::timeout(self.config.boot_timeout, upstream_rx.recv()).await;
        let first = match first {
            Ok(Some(msg)) => msg,
            _ => {
                kill_boot_child(process.child, conn_task).await;
                return Ok(BootResult::Crash);
            }
        };

        let hs = match first {
            Message::Handshake { inner: hs } => hs,
            _ => {
                send_protocol_error(
                    &downstream_tx,
                    ProtocolErrorCode::UnexpectedMessage,
                    "first frame was not a Handshake",
                )
                .await;
                kill_boot_child(process.child, conn_task).await;
                return Ok(BootResult::Crash);
            }
        };

        match validate_handshake(&hs) {
            Ok(_ack) => {
                if downstream_tx.send(Message::HandshakeAck).await.is_err() {
                    kill_boot_child(process.child, conn_task).await;
                    return Ok(BootResult::Crash);
                }
                Ok(BootResult::Ready(ConnTriple {
                    upstream: upstream_rx,
                    downstream: downstream_tx,
                    conn_task,
                    child: process.child,
                }))
            }
            Err(rejection) => {
                send_protocol_error(
                    &downstream_tx,
                    ProtocolErrorCode::HandshakeRejected,
                    &format!("{rejection:?}"),
                )
                .await;
                kill_boot_child(process.child, conn_task).await;
                Ok(BootResult::Crash)
            }
        }
    }

    /// The ready phase: heartbeat loop + message routing. Returns when the host
    /// is declared Hung (3 missed Pongs), drained (ShutdownAck), or the
    /// connection drops. Owns the heartbeat timer and miss count (grill Q7).
    async fn ready_phase(
        &self,
        mut ready: crate::host_state::Ready,
        mut reload_signal: Option<oneshot::Receiver<()>>,
    ) -> std::io::Result<ReadyResult> {
        let mut heartbeat = interval(self.config.heartbeat_interval);
        heartbeat.reset();
        let _ = heartbeat.tick().await;

        let result = loop {
            tokio::select! {
                _ = heartbeat.tick() => {
                    if ready.conn.downstream.send(Message::Heartbeat).await.is_err() {
                        break ReadyResult::ConnectionLost;
                    }
                    match tokio::time::timeout(self.config.heartbeat_interval, ready.conn.upstream.recv()).await {
                        Ok(Some(Message::Pong)) => ready.missed_pongs = 0,
                        Ok(Some(Message::EchoRequest { inner: req })) => {
                            let resp = Message::EchoResponse { inner: EchoResponse {
                                request_id: req.request_id, payload: req.payload,
                            }};
                            let _ = ready.conn.downstream.send(resp).await;
                            ready.missed_pongs += 1;
                        }
                        Ok(Some(_)) => ready.missed_pongs += 1,
                        Ok(None) => break ReadyResult::ConnectionLost,
                        Err(_) => {
                            ready.missed_pongs += 1;
                            if ready.missed_pongs >= MISSED_PONG_THRESHOLD {
                                break ReadyResult::Hung(backoff_for_scaled(1, self.config.backoff_base));
                            }
                        }
                    }
                }
                msg = ready.conn.upstream.recv() => {
                    match msg {
                        Some(Message::Pong) => ready.missed_pongs = 0,
                        Some(Message::EchoRequest { inner: req }) => {
                            let resp = Message::EchoResponse { inner: EchoResponse {
                                request_id: req.request_id, payload: req.payload,
                            }};
                            let _ = ready.conn.downstream.send(resp).await;
                        }
                        // ShutdownAck from the host: graceful drain complete.
                        Some(Message::ShutdownAck) => {
                            break ReadyResult::Drained;
                        }
                        Some(_) => {}
                        None => break ReadyResult::ConnectionLost,
                    }
                }
                // /reload signal: send Shutdown{drain:true}, wait for
                // ShutdownAck, then transition to Drained (ADR 0017).
                _ = async {
                    if let Some(ref mut rx) = reload_signal {
                        let _ = rx.await;
                    } else {
                        // No reload signal: park forever so this branch is
                        // never selected when there's no signal.
                        std::future::pending::<()>().await;
                    }
                } => {
                    // Send Shutdown{drain:true} (ADR 0022 Q8).
                    let shutdown = Message::Shutdown { inner: pi_protocol::Shutdown { drain: true } };
                    if ready.conn.downstream.send(shutdown).await.is_err() {
                        break ReadyResult::ConnectionLost;
                    }
                    // Wait for ShutdownAck with a timeout (don't hang
                    // /reload on a live but unresponsive host).
                    match tokio::time::timeout(self.config.boot_timeout, ready.conn.upstream.recv()).await {
                        Ok(Some(Message::ShutdownAck)) => break ReadyResult::Drained,
                        _ => break ReadyResult::ConnectionLost,
                    }
                }
                _ = ready.conn.child.wait() => break ReadyResult::ConnectionLost,
            }
        };

        // Tear down the connection: cancel the task, kill the child.
        // Destructure ready to move conn out without a placeholder spawn.
        let crate::host_state::Ready { conn, .. } = ready;
        conn.shutdown().await;
        Ok(result)
    }
}

// --- Pure policy functions (unit-testable without I/O) ---

/// Decide the next state after a boot crash. Crash count includes this crash.
/// Returns BackingOff (crash_count < 5) or CrashLooping (crash_count >= 5).
fn decide_boot_crash(crash_count: u32, backoff_base: Duration) -> HostState {
    let backoff = backoff_for_scaled(crash_count, backoff_base);
    if crash_count >= CRASH_LOOP_THRESHOLD {
        HostState::CrashLooping(crate::host_state::CrashLooping { crash_count })
    } else {
        HostState::BackingOff(crate::host_state::BackingOff {
            deadline: Instant::now() + backoff,
            crash_count,
        })
    }
}

/// Send a ProtocolError downstream and await it.
async fn send_protocol_error(
    downstream: &mpsc::Sender<Message>,
    code: ProtocolErrorCode,
    message: &str,
) {
    let _ = downstream
        .send(Message::ProtocolError {
            inner: ProtocolError {
                code,
                message: message.into(),
            },
        })
        .await;
}

enum BootResult {
    Ready(ConnTriple),
    Crash,
}

/// Kill the child process and clean up the connection task on a boot crash.
/// Without this, a mock that connects but never handshakes stays alive,
/// holding the socket and blocking the next respawn.
async fn kill_boot_child(
    mut child: tokio::process::Child,
    conn_task: tokio::task::JoinHandle<std::io::Result<()>>,
) {
    let _ = child.start_kill();
    let _ = child.wait().await;
    conn_task.abort();
    let _ = conn_task.await;
}

enum ReadyResult {
    Hung(Duration),
    Drained,
    ConnectionLost,
}

/// The native restart prompt stub (ADR 0023 Q3). Logs to stderr, reads a
/// line from stdin: r=restart, b=bypass once, a=abort turn.
fn prompt_failure(crash_count: u32) -> FailureDecision {
    eprintln!("host crash-looped {crash_count} times. [r]estart / [b]ypass once / [a]bort turn?");
    let mut line = String::new();
    if std::io::stdin().read_line(&mut line).is_err() {
        return FailureDecision::AbortTurn;
    }
    match line.trim() {
        "r" => FailureDecision::Restart,
        "b" => FailureDecision::BypassOnce,
        _ => FailureDecision::AbortTurn,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn decide_boot_crash_under_threshold_yields_backing_off() {
        let state = decide_boot_crash(1, Duration::from_millis(100));
        assert!(matches!(state, HostState::BackingOff(_)));
    }

    #[test]
    fn decide_boot_crash_at_threshold_yields_crash_looping() {
        let state = decide_boot_crash(5, Duration::from_millis(100));
        assert!(matches!(state, HostState::CrashLooping(_)));
    }

    #[test]
    fn decide_boot_crash_above_threshold_yields_crash_looping() {
        let state = decide_boot_crash(10, Duration::from_millis(100));
        assert!(matches!(state, HostState::CrashLooping(_)));
    }

    #[test]
    fn prompt_failure_returns_abort_on_eof() {
        // stdin in tests is typically EOF or closed.
        let decision = prompt_failure(5);
        // On EOF, read_line returns 0 bytes, trim() is "", matches _ -> AbortTurn.
        // But if stdin is a TTY this would block. In cargo test, stdin is
        // usually /dev/null, so this should return AbortTurn.
        assert_eq!(decision, FailureDecision::AbortTurn);
    }
}
