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

use std::path::PathBuf;
use std::time::Duration;

use pi_protocol::{EchoResponse, Message, ProtocolError, ProtocolErrorCode};
use tokio::net::UnixListener;
use tokio::process::Command;
use tokio::sync::mpsc;
use tokio::time::{interval, Instant};

use crate::host_connection::run_connection;
use crate::host_state::{
    backoff_for_scaled, validate_handshake, ConnTriple, HostProcess, HostState,
};

/// The default heartbeat interval for production (ADR 0022 Q7: 5s).
/// Tests override this via SupervisorConfig.heartbeat_interval.
#[allow(dead_code)]
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
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
    /// The UDS path to bind and pass to the host via PI_RS_HOST_SOCKET.
    pub socket_path: PathBuf,
    /// The host binary to spawn.
    pub host_binary: PathBuf,
    /// Args to pass to the host binary.
    pub host_args: Vec<String>,
    /// The heartbeat interval (ADR 0022 Q7: 5s in production; shorter for
    /// tests). The supervisor sends Heartbeat on this interval and declares
    /// Hung after 3 consecutive missed Pongs (3 * interval).
    pub heartbeat_interval: Duration,
    /// How long to wait for the host to connect and send a Handshake before
    /// declaring a boot crash (30s in production; shorter for tests).
    pub boot_timeout: Duration,
    /// The base unit for exponential backoff (1s in production; shorter for
    /// tests). The sequence is base * [1, 2, 4, 8, 30-cap] (ADR 0023 Q2).
    pub backoff_base: Duration,
}

/// The host supervisor. Construct with `new`, run with `run`.
pub struct HostSupervisor {
    config: SupervisorConfig,
}

impl HostSupervisor {
    pub fn new(config: SupervisorConfig) -> Self {
        Self { config }
    }

    /// Run the supervisor loop. Returns when the host reaches a terminal
    /// Stopped state (after AbortTurn, or a fatal bind error).
    pub async fn run(self) -> std::io::Result<()> {
        let listener = UnixListener::bind(&self.config.socket_path)?;
        let mut state = HostState::stopped();
        let mut crash_count: u32 = 0;

        loop {
            state = match state {
                HostState::Stopped(_) => {
                    let process = self.spawn_host().await?;
                    HostState::Booting(crate::host_state::Booting { process })
                }
                HostState::Booting(booting) => match self.boot_phase(&listener, booting).await? {
                    BootResult::Ready(conn) => {
                        crash_count = 0;
                        HostState::Ready(crate::host_state::Ready {
                            conn,
                            missed_pongs: 0,
                        })
                    }
                    BootResult::Crash => {
                        crash_count += 1;
                        let backoff = backoff_for_scaled(crash_count, self.config.backoff_base);
                        if crash_count >= CRASH_LOOP_THRESHOLD {
                            HostState::CrashLooping(crate::host_state::CrashLooping { crash_count })
                        } else {
                            HostState::BackingOff(crate::host_state::BackingOff {
                                deadline: Instant::now() + backoff,
                                crash_count,
                            })
                        }
                    }
                },
                HostState::Ready(ready) => match self.ready_phase(ready).await? {
                    ReadyResult::Hung(backoff) => {
                        HostState::Reconnecting(crate::host_state::Reconnecting {
                            deadline: Instant::now() + backoff,
                        })
                    }
                    ReadyResult::Drained => {
                        crash_count = 0;
                        HostState::Stopped(crate::host_state::Stopped)
                    }
                    ReadyResult::ConnectionLost => {
                        HostState::Reconnecting(crate::host_state::Reconnecting {
                            deadline: Instant::now()
                                + backoff_for_scaled(1, self.config.backoff_base),
                        })
                    }
                },
                HostState::Hung(_hung) => {
                    // Transient: the Core closed the socket in ready_phase
                    // (via ConnTriple::shutdown), now move to Reconnecting.
                    // First death auto-respawns (ADR 0023 Q6).
                    HostState::Reconnecting(crate::host_state::Reconnecting {
                        deadline: Instant::now() + backoff_for_scaled(1, self.config.backoff_base),
                    })
                }
                HostState::Draining(drain) => {
                    // Should not normally be reached (drain completes in
                    // ready_phase), but handle it: the host exited during drain.
                    let (_, conn) = drain.on_drained();
                    conn.shutdown().await;
                    crash_count = 0;
                    HostState::Stopped(crate::host_state::Stopped)
                }
                HostState::BackingOff(bo) => {
                    tokio::time::sleep_until(bo.deadline).await;
                    let process = self.spawn_host().await?;
                    HostState::Booting(crate::host_state::Booting { process })
                }
                HostState::Reconnecting(re) => {
                    tokio::time::sleep_until(re.deadline).await;
                    let process = self.spawn_host().await?;
                    HostState::Booting(crate::host_state::Booting { process })
                }
                HostState::CrashLooping(cl) => {
                    let decision = prompt_failure(cl.crash_count);
                    match decision {
                        FailureDecision::Restart => {
                            crash_count = 0;
                            let process = self.spawn_host().await?;
                            HostState::Booting(crate::host_state::Booting { process })
                        }
                        FailureDecision::BypassOnce | FailureDecision::AbortTurn => {
                            cl.on_abort();
                            HostState::Stopped(crate::host_state::Stopped)
                        }
                    }
                }
            };

            if matches!(state, HostState::Stopped(_)) {
                break;
            }
        }

        let _ = std::fs::remove_file(&self.config.socket_path);
        Ok(())
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
        // Accept the connection (with a timeout; if the host exits before
        // connecting, the accept hangs and we time out -> Crash, not an
        // error that exits the supervisor).
        let stream = match tokio::time::timeout(self.config.boot_timeout, listener.accept()).await {
            Ok(Ok((stream, _))) => stream,
            Ok(Err(e)) => return Err(e),
            Err(_) => return Ok(BootResult::Crash), // accept timeout = boot crash
        };
        {
            let (upstream_tx, mut upstream_rx) = mpsc::channel::<Message>(32);
            let (downstream_tx, downstream_rx) = mpsc::channel::<Message>(32);
            let conn_task = tokio::spawn(run_connection(stream, upstream_tx, downstream_rx));

            // Wait for the Handshake (or a crash, or timeout).
            let first = tokio::time::timeout(self.config.boot_timeout, upstream_rx.recv()).await;
            let first = match first {
                Ok(Some(msg)) => msg,
                _ => {
                    let _ = conn_task.await;
                    return Ok(BootResult::Crash);
                }
            };
            let hs = match first {
                Message::Handshake { inner: hs } => hs,
                _ => {
                    let _ = downstream_tx
                        .send(Message::ProtocolError {
                            inner: ProtocolError {
                                code: ProtocolErrorCode::UnexpectedMessage,
                                message: "first frame was not a Handshake".into(),
                            },
                        })
                        .await;
                    let _ = conn_task.await;
                    return Ok(BootResult::Crash);
                }
            };
            match validate_handshake(&hs) {
                Ok(_ack) => {
                    if downstream_tx.send(Message::HandshakeAck).await.is_err() {
                        let _ = conn_task.await;
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
                    let _ = downstream_tx
                        .send(Message::ProtocolError {
                            inner: ProtocolError {
                                code: ProtocolErrorCode::HandshakeRejected,
                                message: format!("{rejection:?}"),
                            },
                        })
                        .await;
                    let _ = conn_task.await;
                    Ok(BootResult::Crash)
                }
            }
        }
        // (The block scope ends here; process.child is free to move below.)
    }

    /// The ready phase: heartbeat loop + message routing. Returns when the host
    /// is declared Hung (3 missed Pongs), drained (ShutdownAck), or the
    /// connection drops. Owns the heartbeat timer and miss count (grill Q7).
    async fn ready_phase(
        &self,
        mut ready: crate::host_state::Ready,
    ) -> std::io::Result<ReadyResult> {
        let mut heartbeat = interval(self.config.heartbeat_interval);
        // The first tick fires immediately; consume it so we wait one full
        // interval before the first heartbeat.
        heartbeat.reset();
        let _ = heartbeat.tick().await;

        let result = loop {
            tokio::select! {
                _ = heartbeat.tick() => {
                    // Send a Heartbeat downstream.
                    if ready.conn.downstream.send(Message::Heartbeat).await.is_err() {
                        break ReadyResult::ConnectionLost;
                    }
                    // Wait for the Pong within one interval.
                    match tokio::time::timeout(self.config.heartbeat_interval, ready.conn.upstream.recv()).await {
                        Ok(Some(Message::Pong)) => ready.missed_pongs = 0,
                        Ok(Some(Message::EchoRequest { inner: req })) => {
                            let resp = Message::EchoResponse { inner: EchoResponse {
                                request_id: req.request_id, payload: req.payload,
                            }};
                            let _ = ready.conn.downstream.send(resp).await;
                            // Non-Pong during the pong wait: count as a miss.
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
                        Some(_) => {}
                        None => break ReadyResult::ConnectionLost,
                    }
                }
                status = ready.conn.child.wait() => {
                    let _ = status;
                    break ReadyResult::ConnectionLost;
                }
            }
        };

        // Tear down the connection: cancel the task, kill the child.
        // On Hung, the host is unresponsive (won't process Shutdown); on
        // ConnectionLost, it's already gone. On Drained, the host acked and
        // exited, so shutdown is a no-op cleanup.
        let conn = std::mem::replace(
            &mut ready.conn,
            ConnTriple {
                upstream: mpsc::channel(1).1,
                downstream: mpsc::channel(1).0,
                conn_task: tokio::spawn(async { Ok(()) }),
                child: Command::new("true").spawn()?,
            },
        );
        conn.shutdown().await;
        Ok(result)
    }
}

enum BootResult {
    Ready(ConnTriple),
    Crash,
}

enum ReadyResult {
    /// 3 missed Pongs. The arg is the backoff to wait before reconnecting.
    Hung(Duration),
    /// Graceful Shutdown{drain:true} -> ShutdownAck -> Stopped.
    /// (Wired when reload() lands; the ready_phase currently returns
    /// ConnectionLost on drain, which is corrected in the reload step.)
    #[allow(dead_code)]
    Drained,
    /// The connection dropped unexpectedly (not via heartbeat timeout).
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
