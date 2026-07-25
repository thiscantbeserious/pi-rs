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

use std::time::{Duration, Instant};

use pi_protocol::{Handshake, HandshakeAck, Shutdown, PROTOCOL_VERSION};
use tokio::process::Child;

/// A spawned host process handle. Held by `Booting`.
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

/// Ready: handshake accepted, HandshakeAck sent. Host is live.
pub struct Ready {
    pub process: HostProcess,
    /// Consecutive missed Pongs. 3 → Hung (ADR 0022 Q7).
    pub missed_pongs: u32,
}

/// Hung: 3 consecutive missed Pongs. Transient: the Core closes the socket
/// and moves to Reconnecting. Kept as a named state for log clarity (ADR
/// 0023 Q4, grill Q1) even though it holds no data and immediately
/// transitions.
pub struct Hung;

/// Draining: Core sent Shutdown{drain:true} (for /reload), waiting for
/// ShutdownAck or host exit.
pub struct Draining {
    pub process: HostProcess,
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
    /// Crash count resets on successful handshake (ADR 0023 Q2).
    pub fn on_handshake_ok(self) -> Ready {
        Ready {
            process: self.process,
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
    /// The connection is dropped here; the host process is killed by the
    /// supervisor closing the socket (a hung host won't process Shutdown).
    pub fn on_hung(self) -> Hung {
        Hung
    }

    /// Ready → Draining: Core sends Shutdown{drain:true} for /reload.
    pub fn on_drain(self) -> Draining {
        Draining {
            process: self.process,
        }
    }

    /// Record a missed Pong. Returns Ready with incremented count, or Hung
    /// if the count hit 3 (caller then calls on_hung).
    pub fn on_pong_missed(self) -> Ready {
        Ready {
            process: self.process,
            missed_pongs: self.missed_pongs + 1,
        }
    }

    /// Record a received Pong. Resets the miss count.
    pub fn on_pong(self) -> Ready {
        Ready {
            process: self.process,
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
    /// Draining → Stopped: ShutdownAck received or host exited.
    pub fn on_drained(self) -> Stopped {
        Stopped
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

/// The backoff duration for a given crash count (1-indexed: the 1st crash
/// waits 1s, the 2nd 2s, ... capped at 30s). Resets on successful handshake.
pub fn backoff_for(crash_count: u32) -> Duration {
    let secs = match crash_count {
        0 => 0,
        1 => 1,
        2 => 2,
        3 => 4,
        4 => 8,
        _ => 30,
    };
    Duration::from_secs(secs)
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
        let ready = booting.on_handshake_ok();
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
            process: HostProcess {
                child: spawn_nothing(),
            },
            missed_pongs: 0,
        };
        ready = ready.on_pong_missed();
        assert!(!ready.is_hung(), "1 miss is not hung");
        ready = ready.on_pong_missed();
        assert!(!ready.is_hung(), "2 misses are not hung");
        ready = ready.on_pong_missed();
        assert!(ready.is_hung(), "3 misses is hung");
        let _hung = ready.on_hung();
    }

    #[tokio::test]
    async fn ready_pong_resets_miss_count() {
        let ready = Ready {
            process: HostProcess {
                child: spawn_nothing(),
            },
            missed_pongs: 2,
        };
        let ready = ready.on_pong();
        assert_eq!(ready.missed_pongs, 0, "Pong resets the miss count");
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
}
