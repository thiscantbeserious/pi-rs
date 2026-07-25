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
//! - `bad-handshake`: connect, send a Handshake with a wrong protocol_version.
//!   Exercises the version-mismatch rejection path.
//! - `no-handshake`: connect, send an EchoRequest as the first frame instead
//!   of a Handshake. Exercises the unexpected-first-frame rejection path.
//! - `echo-after-handshake`: connect, handshake, then send an EchoRequest
//!   and wait for the EchoResponse. Exercises message routing in Ready.
//!
//! The socket path comes from `PI_RS_HOST_SOCKET`. Run:
//!   mock-host [--mode <mode>]

use std::env;
use std::process::ExitCode;

use pi_protocol::framing::{read_frame, write_frame};
use pi_protocol::{EchoRequest, EchoResponse, Handshake, Message, PROTOCOL_VERSION};
use tokio::net::UnixStream;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Normal,
    ExitImmediately,
    GoSilentAfterHandshake,
    BadHandshake,
    NoHandshake,
    EchoAfterHandshake,
}

fn parse_mode(args: &[String]) -> Mode {
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--mode" {
            if let Some(val) = iter.next() {
                return match val.as_str() {
                    "exit-immediately" => Mode::ExitImmediately,
                    "go-silent-after-handshake" => Mode::GoSilentAfterHandshake,
                    "bad-handshake" => Mode::BadHandshake,
                    "no-handshake" => Mode::NoHandshake,
                    "echo-after-handshake" => Mode::EchoAfterHandshake,
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

    match mode {
        Mode::BadHandshake => {
            // Send a Handshake with a wrong protocol_version.
            let hs = Message::Handshake {
                inner: Handshake {
                    protocol_version: PROTOCOL_VERSION + 1,
                    host_pid: std::process::id(),
                },
            };
            send(&mut stream, &hs).await;
            // Wait for the supervisor to reject and close.
            let _ = read_frame(&mut stream).await;
            return ExitCode::SUCCESS;
        }
        Mode::NoHandshake => {
            // Send an EchoRequest as the first frame (not a Handshake).
            let req = Message::EchoRequest {
                inner: EchoRequest {
                    request_id: 1,
                    payload: b"not-a-handshake".to_vec(),
                },
            };
            send(&mut stream, &req).await;
            let _ = read_frame(&mut stream).await;
            return ExitCode::SUCCESS;
        }
        _ => {}
    }

    // Normal handshake.
    let hs = Message::Handshake {
        inner: Handshake {
            protocol_version: PROTOCOL_VERSION,
            host_pid: std::process::id(),
        },
    };
    send(&mut stream, &hs).await;

    // Wait for HandshakeAck.
    let ack_frame = match read_frame(&mut stream).await {
        Ok(f) => f,
        Err(_) => return ExitCode::from(5),
    };
    let _ack: Message = match rmp_serde::from_slice(&ack_frame) {
        Ok(m) => m,
        Err(_) => return ExitCode::from(6),
    };

    if mode == Mode::GoSilentAfterHandshake {
        tokio::time::sleep(std::time::Duration::from_secs(3600)).await;
        return ExitCode::SUCCESS;
    }

    if mode == Mode::EchoAfterHandshake {
        // Send an EchoRequest, wait for the EchoResponse.
        let req = Message::EchoRequest {
            inner: EchoRequest {
                request_id: 42,
                payload: b"echo-test".to_vec(),
            },
        };
        send(&mut stream, &req).await;
        let _ = read_frame(&mut stream).await;
        // Then enter normal message loop.
    }

    message_loop(&mut stream).await
}

async fn send(stream: &mut UnixStream, msg: &Message) {
    let body = rmp_serde::to_vec(msg).expect("encode");
    let _ = write_frame(stream, &body).await;
}

/// Handle incoming messages until the socket closes or a Shutdown is received.
async fn message_loop(stream: &mut UnixStream) -> ExitCode {
    loop {
        let frame = match read_frame(stream).await {
            Ok(f) => f,
            Err(_) => return ExitCode::SUCCESS,
        };
        let msg: Message = match rmp_serde::from_slice(&frame) {
            Ok(m) => m,
            Err(_) => continue,
        };
        match handle_message(stream, msg).await {
            HandleResult::Continue => {}
            HandleResult::Exit(code) => return code,
        }
    }
}

enum HandleResult {
    Continue,
    Exit(ExitCode),
}

async fn handle_message(stream: &mut UnixStream, msg: Message) -> HandleResult {
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
                let ack = Message::ShutdownAck;
                send(stream, &ack).await;
            }
            return HandleResult::Exit(ExitCode::SUCCESS);
        }
        _ => None,
    };
    if let Some(reply) = reply {
        send(stream, &reply).await;
    }
    HandleResult::Continue
}
