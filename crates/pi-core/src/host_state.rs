//! Host lifecycle state machine (ADR 0023 Q4, PHILOSOPHY.md §4).
//!
//! Pattern A typestate: each state is a struct carrying its invariants
//! (`Booting` holds the child handle, `Ready` holds the connection + miss
//! count, etc.). Transition methods live on the inner types and consume
//! `self`. `enum HostState` wraps them for supervisor storage. Illegal
//! transitions don't compile because the method does not exist on that inner
//! type. The single match site in the supervisor is the only mutation point
//! (ADR 0023 Q5: single writer).
//!
//! 8 states per ADR 0023 Q4: Stopped, Booting, Ready, Hung, Draining,
//! BackingOff, Reconnecting, CrashLooping. `Hung` is a transient named state
//! for log clarity even though it holds no data (ADR 0023 Q4, grill Q1).

use std::time::Duration;

use pi_protocol::{Handshake, HandshakeAck, Shutdown, PROTOCOL_VERSION};
use tokio::process::Child;
use tokio::sync::mpsc;
use tokio::time::Instant;

/// The connection triple: the channels + task handle + child process for one
/// live host connection. Held by `Ready` and `Draining` (ADR 0023 Q4/Q5, grill
/// Q8). The supervisor owns the heartbeat timer + miss count; this triple is
/// the I/O surface the supervisor routes messages through.
pub struct ConnTriple {
    pub upstream: mpsc::Receiver<pi_protocol::Message>,
    pub downstream: mpsc::Sender<pi_protocol::Message>,
    pub conn_task: tokio::task::JoinHandle<std::io::Result<()>>,
    pub child: Child,
}

impl ConnTriple {
    /// Drop the downstream sender (so the connection task exits), await the
    /// task, and kill the child if still alive. Used on Hung / connection loss.
    pub async fn shutdown(mut self) {
        // Drop the sender first so the connection task's downstream.recv()
        // returns None and the task exits cleanly.
        self.downstream.closed().await;
        // Cancel the task if it hasn't exited.
        self.conn_task.abort();
        let _ = self.conn_task.await;
        // Kill the child if still alive.
        let _ = self.child.start_kill();
        let _ = self.child.wait().await;
    }
}

/// A spawned host process handle. Held by `Booting` (before the connection is
/// established). Once the handshake completes, the child moves into the
/// `ConnTriple` held by `Ready`.
pub struct HostProcess {
    pub child: Child,
}

/// Stopped: no host process, no pending action. Initial and
/// terminal-after-graceful-shutdown.
pub struct Stopped;

/// Booting: Core spawned the host, waiting for Handshake. A crash here is a
/// "failed boot" (ADR 0023 Q2's crash count increments).
pub struct Booting {
    pub process: HostProcess,
}

/// Ready: handshake accepted, HandshakeAck sent. Host is live. Holds the
/// `ConnTriple` (I/O surface) and the miss count. The supervisor owns the
/// heartbeat timer and sends `Heartbeat` downstream on the interval (ADR 0023
/// Q5, grill Q7).
pub struct Ready {
    pub conn: ConnTriple,
    /// Consecutive missed Pongs. 3 → Hung (ADR 0022 Q7).
    pub missed_pongs: u32,
}

/// Hung: 3 consecutive missed Pongs. Transient: the Core closes the socket
/// and moves to Reconnecting. Kept as a named state for log clarity (ADR
/// 0023 Q4, grill Q1) even though it holds no data and immediately
/// transitions.
pub struct Hung;

/// Draining: Core sent Shutdown{drain:true} (for /reload), waiting for
/// ShutdownAck or host exit. Holds the `ConnTriple` so the supervisor can
/// keep reading the ack and detect host exit.
pub struct Draining {
    pub conn: ConnTriple,
}

/// BackingOff: a boot crash occurred (crash count < 5); wait the backoff
/// timer, then respawn. Distinct from Stopped so the pending timer is
/// explicit state, and from Reconnecting because the reason differs (boot
/// failure vs runtime death) for logging/metrics (ADR 0023 Q4).
pub struct BackingOff {
    pub deadline: Instant,
    pub crash_count: u32,
}

/// Reconnecting: the host died at runtime (via Hung); wait the backoff timer,
/// then respawn. Distinct from BackingOff for the reason, shared timer logic
/// (ADR 0023 Q4).
pub struct Reconnecting {
    pub deadline: Instant,
}

/// CrashLooping: 5 consecutive failed boots; supervisor gave up
/// auto-respawn, surfacing the native prompt (ADR 0023 Q2).
pub struct CrashLooping {
    pub crash_count: u32,
}

/// The supervisor's held state. The enum wrap is the runtime boundary; the
/// inner types' transition methods enforce legal transitions at compile time.
pub enum HostState {
    Stopped(Stopped),
    Booting(Booting),
    Ready(Ready),
    Hung(Hung),
    Draining(Draining),
    BackingOff(BackingOff),
    Reconnecting(Reconnecting),
    CrashLooping(CrashLooping),
}

impl HostState {
    pub fn stopped() -> Self {
        Self::Stopped(Stopped)
    }
}

// --- Transitions on inner types. Methods consume self, return the next
// state. Illegal transitions (e.g. Ready::on_handshake) do not exist as
// methods, so they do not compile (PHILOSOPHY.md §4). ---

impl Stopped {
    /// Stopped → Booting: spawn the host.
    pub fn spawn(self, process: HostProcess) -> Booting {
        Booting { process }
    }
}

/// The outcome of a boot crash. The supervisor decides BackingOff vs
/// CrashLooping based on the crash count (ADR 0023 Q2).
pub enum BootCrashOutcome {
    /// Crash count < 5: wait the backoff, then respawn.
    BackingOff(BackingOff),
    /// Crash count >= 5: give up, surface the prompt.
    CrashLooping(CrashLooping),
}

impl Booting {
    /// Booting → Ready: valid Handshake received, version matches.
    /// Takes the ConnTriple built during the boot phase (the connection task,
    /// channels, and child). Crash count resets on successful handshake
    /// (ADR 0023 Q2).
    pub fn on_handshake_ok(self, conn: ConnTriple) -> Ready {
        Ready {
            conn,
            missed_pongs: 0,
        }
    }

    /// Booting → BackingOff | CrashLooping: host exited before handshake.
    /// `crash_count` is the count *including* this crash.
    pub fn on_boot_crash(self, crash_count: u32, backoff: Duration) -> BootCrashOutcome {
        if crash_count >= 5 {
            BootCrashOutcome::CrashLooping(CrashLooping { crash_count })
        } else {
            BootCrashOutcome::BackingOff(BackingOff {
                deadline: Instant::now() + backoff,
                crash_count,
            })
        }
    }
}

impl Ready {
    /// Ready → Hung: 3 consecutive missed Pongs (ADR 0022 Q7).
    /// Consumes self and returns the ConnTriple for the supervisor to shut
    /// down (cancel the task, kill the child). A hung host won't process
    /// Shutdown, so the connection is torn down, not drained.
    pub fn into_hung(self) -> (Hung, ConnTriple) {
        (Hung, self.conn)
    }

    /// Ready → Draining: Core sends Shutdown{drain:true} for /reload.
    /// Moves the ConnTriple into Draining so the supervisor can keep reading
    /// the ShutdownAck.
    pub fn on_drain(self) -> Draining {
        Draining { conn: self.conn }
    }

    /// Record a missed Pong. Returns Ready with incremented count.
    pub fn on_pong_missed(self) -> Ready {
        Ready {
            conn: self.conn,
            missed_pongs: self.missed_pongs + 1,
        }
    }

    /// Record a received Pong. Resets the miss count.
    pub fn on_pong(self) -> Ready {
        Ready {
            conn: self.conn,
            missed_pongs: 0,
        }
    }

    /// Whether the miss threshold is reached (3 per ADR 0022 Q7).
    pub fn is_hung(&self) -> bool {
        self.missed_pongs >= 3
    }
}

impl Hung {
    /// Hung → Reconnecting: Core closed the socket, wait backoff then respawn.
    pub fn on_close(self, backoff: Duration) -> Reconnecting {
        Reconnecting {
            deadline: Instant::now() + backoff,
        }
    }
}

impl Draining {
    /// Draining → Stopped: ShutdownAck received or host exited. Returns the
    /// ConnTriple for the supervisor to shut down cleanly.
    pub fn on_drained(self) -> (Stopped, ConnTriple) {
        (Stopped, self.conn)
    }
}

impl BackingOff {
    /// BackingOff → Booting: backoff timer elapsed, respawn.
    pub fn on_deadline(self, process: HostProcess) -> Booting {
        Booting { process }
    }

    pub fn is_ready(&self) -> bool {
        Instant::now() >= self.deadline
    }
}

impl Reconnecting {
    /// Reconnecting → Booting: backoff timer elapsed, respawn.
    pub fn on_deadline(self, process: HostProcess) -> Booting {
        Booting { process }
    }

    pub fn is_ready(&self) -> bool {
        Instant::now() >= self.deadline
    }
}

impl CrashLooping {
    /// CrashLooping → Booting: human picked Restart (resets backoff + crash
    /// count, ADR 0023 Q2/Q6).
    pub fn on_restart(self, process: HostProcess) -> Booting {
        Booting { process }
    }

    /// CrashLooping → Stopped: human picked BypassOnce or AbortTurn.
    pub fn on_abort(self) -> Stopped {
        Stopped
    }
}

// --- Handshake validation (ADR 0022 Q3: exact-match-or-refuse). ---

/// Validate a Handshake against the protocol version. Returns HandshakeAck on
/// match, or the rejection (caller closes the socket + logs).
pub fn validate_handshake(hs: &Handshake) -> Result<HandshakeAck, HandshakeRejection> {
    if hs.protocol_version == PROTOCOL_VERSION {
        Ok(HandshakeAck)
    } else {
        Err(HandshakeRejection::VersionMismatch {
            expected: PROTOCOL_VERSION,
            got: hs.protocol_version,
        })
    }
}

/// A handshake rejection reason (ADR 0022 Q3).
#[derive(Debug, PartialEq, Eq)]
pub enum HandshakeRejection {
    VersionMismatch { expected: u32, got: u32 },
}

// --- Backoff calculator (ADR 0023 Q2: exponential 1->2->4->8->30 cap). ---

/// The backoff multiplier for a given crash count (1-indexed: the 1st crash
/// waits 1x, the 2nd 2x, ... capped at 30x). Resets on successful handshake.
/// The actual duration is `base * multiplier` where `base` is passed by the
/// caller (1s in production, 100ms in tests).
pub fn backoff_multiplier(crash_count: u32) -> u64 {
    match crash_count {
        0 => 0,
        1 => 1,
        2 => 2,
        3 => 4,
        4 => 8,
        _ => 30,
    }
}

/// The backoff duration for a given crash count, using the default 1s base.
/// Tests should use `backoff_for_scaled` with a shorter base.
pub fn backoff_for(crash_count: u32) -> Duration {
    Duration::from_secs(backoff_multiplier(crash_count))
}

/// The backoff duration for a given crash count, scaled by `base`.
pub fn backoff_for_scaled(crash_count: u32, base: Duration) -> Duration {
    base * backoff_multiplier(crash_count) as u32
}

/// What to send for a graceful shutdown (ADR 0022 Q8).
pub fn graceful_shutdown() -> Shutdown {
    Shutdown { drain: true }
}

/// What to send for an immediate shutdown (ADR 0022 Q8).
pub fn immediate_shutdown() -> Shutdown {
    Shutdown { drain: false }
}

/// The ack expected from a graceful shutdown (ADR 0022 Q8).
pub fn is_shutdown_ack(msg: &pi_protocol::Message) -> bool {
    matches!(msg, pi_protocol::Message::ShutdownAck)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stopped_starts_empty() {
        let s = HostState::stopped();
        assert!(matches!(s, HostState::Stopped(_)));
    }

    #[tokio::test]
    async fn stopped_to_booting_via_spawn() {
        let process = HostProcess {
            child: spawn_nothing(),
        };
        let stopped = Stopped;
        let booting = stopped.spawn(process);
        assert!(booting.process.child.id().is_some());
    }

    #[tokio::test]
    async fn booting_to_ready_on_handshake_ok() {
        let booting = Booting {
            process: HostProcess {
                child: spawn_nothing(),
            },
        };
        let conn = test_conn_triple().await;
        let ready = booting.on_handshake_ok(conn);
        assert_eq!(
            ready.missed_pongs, 0,
            "miss count resets on successful handshake"
        );
    }

    #[tokio::test]
    async fn booting_crash_under_5_yields_backing_off() {
        let booting = Booting {
            process: HostProcess {
                child: spawn_nothing(),
            },
        };
        let outcome = booting.on_boot_crash(1, backoff_for(1));
        assert!(matches!(outcome, BootCrashOutcome::BackingOff(_)));
    }

    #[tokio::test]
    async fn booting_crash_at_5_yields_crash_looping() {
        let booting = Booting {
            process: HostProcess {
                child: spawn_nothing(),
            },
        };
        let outcome = booting.on_boot_crash(5, backoff_for(5));
        assert!(matches!(outcome, BootCrashOutcome::CrashLooping(_)));
    }

    #[tokio::test]
    async fn ready_to_hung_after_3_misses() {
        let mut ready = Ready {
            conn: test_conn_triple().await,
            missed_pongs: 0,
        };
        ready = ready.on_pong_missed();
        assert!(!ready.is_hung(), "1 miss is not hung");
        ready = ready.on_pong_missed();
        assert!(!ready.is_hung(), "2 misses are not hung");
        ready = ready.on_pong_missed();
        assert!(ready.is_hung(), "3 misses is hung");
        let (_hung, conn) = ready.into_hung();
        conn.shutdown().await;
    }

    #[tokio::test]
    async fn ready_pong_resets_miss_count() {
        let ready = Ready {
            conn: test_conn_triple().await,
            missed_pongs: 2,
        };
        let ready = ready.on_pong();
        assert_eq!(ready.missed_pongs, 0, "Pong resets the miss count");
        ready.conn.shutdown().await;
    }

    #[test]
    fn validate_handshake_matches() {
        let hs = Handshake {
            protocol_version: PROTOCOL_VERSION,
            host_pid: 1234,
        };
        assert!(validate_handshake(&hs).is_ok());
    }

    #[test]
    fn validate_handshake_rejects_mismatch() {
        let hs = Handshake {
            protocol_version: PROTOCOL_VERSION + 1,
            host_pid: 1234,
        };
        let err = validate_handshake(&hs).unwrap_err();
        assert_eq!(
            err,
            HandshakeRejection::VersionMismatch {
                expected: PROTOCOL_VERSION,
                got: PROTOCOL_VERSION + 1
            }
        );
    }

    #[test]
    fn backoff_sequence_is_exponential_capped_at_30() {
        assert_eq!(backoff_for(1), Duration::from_secs(1));
        assert_eq!(backoff_for(2), Duration::from_secs(2));
        assert_eq!(backoff_for(3), Duration::from_secs(4));
        assert_eq!(backoff_for(4), Duration::from_secs(8));
        assert_eq!(backoff_for(5), Duration::from_secs(30));
        assert_eq!(backoff_for(6), Duration::from_secs(30), "capped at 30");
        assert_eq!(backoff_for(100), Duration::from_secs(30), "capped at 30");
    }

    /// A dummy child for testing state transitions without spawning a real
    /// process. The supervisor tests use the real mock-host binary.
    fn spawn_nothing() -> Child {
        // A child that exits immediately; we only care it has a pid for the
        // state-struct tests. Drop kills it.
        let mut cmd = tokio::process::Command::new("true");
        cmd.spawn().expect("spawn true")
    }

    /// A ConnTriple for testing state transitions. Spawns a connection task
    /// over a real UDS pair (the connection task expects a UnixStream).
    /// The task exits immediately when the downstream channel closes.
    async fn test_conn_triple() -> ConnTriple {
        use tokio::net::UnixListener;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let _client = tokio::net::UnixStream::connect(&path).await.unwrap();
        let (server, _) = listener.accept().await.unwrap();
        let (upstream_tx, upstream_rx) = mpsc::channel(8);
        let (downstream_tx, downstream_rx) = mpsc::channel(8);
        let conn_task = tokio::spawn(crate::host_connection::run_connection(
            server,
            upstream_tx,
            downstream_rx,
        ));
        // Leak the dir so the socket file survives; cleaned up on test exit.
        std::mem::forget(dir);
        ConnTriple {
            upstream: upstream_rx,
            downstream: downstream_tx,
            conn_task,
            child: spawn_nothing(),
        }
    }
}
