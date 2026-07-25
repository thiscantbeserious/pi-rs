//! The connection task: dumb I/O between a `HostConnection` and two mpsc
//! channels (upstream: Message to supervisor; downstream: Message from
//! supervisor). No logic, no heartbeat timer (that's the supervisor's per
//! ADR 0023 Q5 / grill Q3). ~15 lines of select!.
//!
//! The task exits when the downstream channel closes (supervisor dropped the
//! sender) or the socket read fails (host closed/EOF/error). On exit it
//! returns the connection so the supervisor can decide what to do.

use pi_protocol::framing::{read_frame, write_frame};
use pi_protocol::Message;
use tokio::net::UnixStream;
use tokio::sync::mpsc;

/// Run the connection I/O loop until the downstream channel closes or the
/// socket breaks. Reads frames upstream, writes downstream frames.
pub async fn run_connection(
    stream: UnixStream,
    upstream: mpsc::Sender<Message>,
    mut downstream: mpsc::Receiver<Message>,
) -> std::io::Result<()> {
    let (mut reader, mut writer) = stream.into_split();
    loop {
        tokio::select! {
            // Downstream message to write to the host. None means the
            // supervisor closed the channel (e.g. on Hung): stop the task.
            msg = downstream.recv() => {
                let Some(msg) = msg else { break };
                let body = rmp_serde::to_vec(&msg)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                write_frame(&mut writer, &body).await?;
            }
            // Frame arrived from the host.
            res = read_frame(&mut reader) => {
                let frame = res?;
                let msg: Message = rmp_serde::from_slice(&frame)
                    .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
                // If the upstream sender is gone (supervisor dropped it),
                // the host is no longer needed: stop.
                if upstream.send(msg).await.is_err() {
                    break;
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pi_protocol::{EchoRequest, EchoResponse};

    #[tokio::test]
    async fn connection_reads_frame_upstream_and_writes_downstream() {
        // Real UDS pair: the host side writes a frame, the connection reads it
        // upstream; the supervisor side sends a frame downstream, the
        // connection writes it to the host.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("conn.sock");
        let listener = tokio::net::UnixListener::bind(&path).unwrap();

        let (upstream_tx, mut upstream_rx) = mpsc::channel::<Message>(8);
        let (downstream_tx, downstream_rx) = mpsc::channel::<Message>(8);

        let server = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            run_connection(stream, upstream_tx, downstream_rx).await
        });

        let mut host = tokio::net::UnixStream::connect(&path).await.unwrap();

        // Host sends an EchoRequest.
        let req = Message::EchoRequest {
            inner: EchoRequest {
                request_id: 1,
                payload: b"hi".to_vec(),
            },
        };
        write_frame(&mut host, &rmp_serde::to_vec(&req).unwrap())
            .await
            .unwrap();

        // Supervisor receives it upstream.
        let got = upstream_rx.recv().await.expect("frame arrived upstream");
        assert_eq!(got, req);

        // Supervisor sends an EchoResponse downstream.
        let resp = Message::EchoResponse {
            inner: EchoResponse {
                request_id: 1,
                payload: b"hi".to_vec(),
            },
        };
        downstream_tx.send(resp.clone()).await.unwrap();

        // Host receives the response frame.
        let frame = read_frame(&mut host).await.unwrap();
        let back: Message = rmp_serde::from_slice(&frame).unwrap();
        assert_eq!(back, resp);

        // Drop downstream sender to end the connection task cleanly.
        drop(downstream_tx);
        server.await.unwrap().unwrap();
    }
}
