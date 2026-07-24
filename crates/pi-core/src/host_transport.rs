//! Core-side Host Protocol transport (ADR 0006, ADR 0022).
//!
//! The Core binds a Unix domain socket before spawning the host. The host
//! connects and sends the first frame (Handshake). The listener survives
//! `kill -9` of the host (exit gate 2): the host only holds a connection, not
//! the listener, so respawn lands on the same path with zero rebind race.

use std::path::Path;

use pi_protocol::framing::{read_frame, write_frame};
use pi_protocol::Message;
use tokio::net::{UnixListener, UnixStream};

/// A bound Host Protocol listener. Owns the UDS path and cleans it up on drop.
pub struct HostListener {
    listener: UnixListener,
    path: std::path::PathBuf,
}

impl HostListener {
    /// Bind a UDS at `path`. Removes a stale socket file first.
    pub fn bind(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let path = path.as_ref().to_path_buf();
        // A stale socket file from a crashed previous run blocks bind.
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path)?;
        Ok(Self { listener, path })
    }

    /// The UDS path, to pass to the spawned host via PI_RS_HOST_SOCKET.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Accept one host connection.
    pub async fn accept(&self) -> std::io::Result<HostConnection> {
        let (stream, _peer) = self.listener.accept().await?;
        Ok(HostConnection::new(stream))
    }
}

impl Drop for HostListener {
    fn drop(&mut self) {
        // Clean up the socket file so the next bind succeeds.
        let _ = std::fs::remove_file(&self.path);
    }
}

/// One accepted host connection. Reads and writes framed Messages.
pub struct HostConnection {
    stream: UnixStream,
}

impl HostConnection {
    pub fn new(stream: UnixStream) -> Self {
        Self { stream }
    }

    /// Read one framed Message from the host.
    pub async fn read_message(&mut self) -> std::io::Result<Message> {
        let frame = read_frame(&mut self.stream).await?;
        rmp_serde::from_slice(&frame)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }

    /// Write one framed Message to the host.
    pub async fn write_message(&mut self, msg: &Message) -> std::io::Result<()> {
        let body = rmp_serde::to_vec(msg)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        write_frame(&mut self.stream, &body).await
    }
}

impl AsRef<UnixStream> for HostConnection {
    fn as_ref(&self) -> &UnixStream {
        &self.stream
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pi_protocol::{EchoRequest, Message};

    #[tokio::test]
    async fn listener_accepts_a_connection_and_round_trips_a_message() {
        let dir = tempfile::tempdir().unwrap();
        let socket_path = dir.path().join("host.sock");

        let listener = HostListener::bind(&socket_path).unwrap();
        assert!(socket_path.exists(), "bind creates the socket file");

        // Simulate the host: connect, send a Message, read the echo back.
        let host_side = tokio::spawn(async move {
            let mut sock = UnixStream::connect(&socket_path).await.unwrap();
            let msg = Message::EchoRequest {
                inner: EchoRequest {
                    request_id: 7,
                    payload: b"ping".to_vec(),
                },
            };
            let body = rmp_serde::to_vec(&msg).unwrap();
            write_frame(&mut sock, &body).await.unwrap();

            let frame = read_frame(&mut sock).await.unwrap();
            let echo: Message = rmp_serde::from_slice(&frame).unwrap();
            echo
        });

        let mut conn = listener.accept().await.unwrap();
        let got = conn.read_message().await.unwrap();
        assert_eq!(
            got,
            Message::EchoRequest {
                inner: EchoRequest {
                    request_id: 7,
                    payload: b"ping".to_vec()
                }
            }
        );

        // Echo it back.
        conn.write_message(&got).await.unwrap();

        let echo = host_side.await.unwrap();
        assert_eq!(
            echo,
            Message::EchoRequest {
                inner: EchoRequest {
                    request_id: 7,
                    payload: b"ping".to_vec()
                }
            }
        );
    }

    #[tokio::test]
    async fn listener_survives_client_drop_without_removing_the_socket() {
        // Exit gate 2 property: the listener stays bound when a connected host
        // vanishes. A new host can connect to the same path.
        let dir = tempfile::tempdir().unwrap();
        let socket_path = dir.path().join("host.sock");
        let listener = HostListener::bind(&socket_path).unwrap();

        {
            let _sock = UnixStream::connect(&socket_path).await.unwrap();
            let _conn = listener.accept().await.unwrap();
            // Drop the connection and the client socket, simulating host death.
        }

        // A new host can still connect to the same listener.
        let _sock2 = UnixStream::connect(&socket_path).await.unwrap();
        // Connection established; the listener survived the previous host's death.
    }
}
