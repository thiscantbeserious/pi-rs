//! Mock host binary for the supervisor + chaos tests (ADR 0023, plan step 9).
//!
//! A minimal host that speaks the 9-message Phase 1 protocol with
//! controllable misbehavior, so the tests can exercise the supervisor's
//! lifecycle paths without the real Deno host binary.
//!
//! Modes (passed as `--mode <mode>`):
//! - `normal` (default): connect, handshake, pong heartbeats, ack Shutdown,
//!   echo EchoRequest. The well-behaved host.
//! - `exit-immediately`: exit before connecting. For the crash-loop test
//!   (5 failed boots -> CrashLooping -> prompt).
//! - `go-silent-after-handshake`: connect, handshake, then stop replying to
//!   heartbeats. For the Hung test without kill -9.
//!
//! The socket path comes from `PI_RS_HOST_SOCKET`. Run:
//!   mock-host [--mode normal|exit-immediately|go-silent-after-handshake]

use std::env;
use std::process::ExitCode;

use pi_protocol::framing::{read_frame, write_frame};
use pi_protocol::{EchoResponse, Handshake, Message, PROTOCOL_VERSION};
use tokio::net::UnixStream;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Normal,
    ExitImmediately,
    GoSilentAfterHandshake,
}

fn parse_mode(args: &[String]) -> Mode {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--mode" {
            if let Some(val) = iter.next() {
                return match val.as_str() {
                    "exit-immediately" => Mode::ExitImmediately,
                    "go-silent-after-handshake" => Mode::GoSilentAfterHandshake,
                    _ => Mode::Normal,
                };
            }
        }
    }
    Mode::Normal
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let mode = parse_mode(&args);

    if mode == Mode::ExitImmediately {
        return ExitCode::from(1);
    }

    let socket_path = match env::var("PI_RS_HOST_SOCKET") {
        Ok(p) => p,
        Err(_) => {
            eprintln!("mock-host: PI_RS_HOST_SOCKET not set");
            return ExitCode::from(2);
        }
    };

    let mut stream = match UnixStream::connect(&socket_path).await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("mock-host: connect failed: {e}");
            return ExitCode::from(3);
        }
    };

    // Host speaks first: send Handshake (ADR 0022 Q2).
    let hs = Message::Handshake {
        inner: Handshake {
            protocol_version: PROTOCOL_VERSION,
            host_pid: std::process::id(),
        },
    };
    let body = rmp_serde::to_vec(&hs).expect("encode handshake");
    if write_frame(&mut stream, &body).await.is_err() {
        return ExitCode::from(4);
    }

    // Wait for HandshakeAck before entering the message loop.
    let ack_frame = match read_frame(&mut stream).await {
        Ok(f) => f,
        Err(_) => return ExitCode::from(5),
    };
    let _ack: Message = match rmp_serde::from_slice(&ack_frame) {
        Ok(m) => m,
        Err(_) => return ExitCode::from(6),
    };
    // (Real validation that it's a HandshakeAck happens in the supervisor test;
    // the mock just needs the dance to complete.)

    if mode == Mode::GoSilentAfterHandshake {
        // Stop replying. The supervisor's heartbeat will time out 3x -> Hung.
        // Sleep forever (until killed).
        tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
        return ExitCode::SUCCESS;
    }

    // Normal mode: handle messages.
    loop {
        let frame = match read_frame(&mut stream).await {
            Ok(f) => f,
            Err(_) => return ExitCode::SUCCESS, // socket closed
        };
        let msg: Message = match rmp_serde::from_slice(&frame) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let reply = match msg {
            Message::Heartbeat => Some(Message::Pong),
            Message::EchoRequest { inner: req } => Some(Message::EchoResponse {
                inner: EchoResponse {
                    request_id: req.request_id,
                    payload: req.payload,
                },
            }),
            Message::Shutdown { inner: shutdown } => {
                if shutdown.drain {
                    // Graceful: ack then exit (ADR 0022 Q8).
                    let ack = Message::ShutdownAck;
                    let body = rmp_serde::to_vec(&ack).expect("encode ack");
                    let _ = write_frame(&mut stream, &body).await;
                }
                return ExitCode::SUCCESS;
            }
            _ => None,
        };
        if let Some(reply) = reply {
            let body = rmp_serde::to_vec(&reply).expect("encode reply");
            if write_frame(&mut stream, &body).await.is_err() {
                return ExitCode::from(7);
            }
        }
    }
}
