//! Length-prefix framing for the Host Protocol (ADR 0006, ADR 0022).
//!
//! Each frame is a 4-byte big-endian u32 length prefix followed by that many
//! bytes of MessagePack body. The length counts the body only.

use std::io;

use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Reject any single frame body larger than this many bytes. ADR 0006 frames
/// carry protocol messages, extension UI buffers, and tool output; 16 MiB is
/// well above any plausible single message while bounding allocation against a
/// malicious or buggy peer that advertises a near-4 GiB length. Without this
/// bound, `read_frame` would allocate `vec![0u8; len]` straight off the wire.
pub const MAX_FRAME_SIZE: usize = 16 * 1024 * 1024;

/// Write a frame: 4-byte BE u32 length, then the body.
pub async fn write_frame<W>(writer: &mut W, body: &[u8]) -> io::Result<()>
where
    W: AsyncWriteExt + Unpin,
{
    let len = u32::try_from(body.len()).map_err(|_| {
        io::Error::new(io::ErrorKind::InvalidInput, "frame body exceeds u32 length")
    })?;
    writer.write_all(&len.to_be_bytes()).await?;
    writer.write_all(body).await?;
    Ok(())
}

/// Read a frame: read 4-byte BE u32 length, then read that many body bytes.
/// Rejects a length exceeding [`MAX_FRAME_SIZE`] before allocating, so a peer
/// cannot drive the reader into an oversized allocation.
pub async fn read_frame<R>(reader: &mut R) -> io::Result<Vec<u8>>
where
    R: AsyncReadExt + Unpin,
{
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf).await?;
    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_FRAME_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "frame body exceeds maximum allowed size",
        ));
    }
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body).await?;
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::DuplexStream;

    #[tokio::test]
    async fn write_frame_emits_be_length_then_body() {
        let (mut client, mut server) = tokio::io::duplex(64);
        let body = b"hello";
        write_frame(&mut client, body).await.unwrap();

        // Read the raw bytes the writer produced, from the other end of the
        // duplex. A duplex feeds writes on one side to reads on the other.
        let mut raw = [0u8; 9];
        server.read_exact(&mut raw).await.unwrap();
        assert_eq!(&raw[0..4], &[0, 0, 0, 5], "length prefix is BE u32 of body");
        assert_eq!(&raw[4..9], body);
    }

    #[tokio::test]
    async fn read_frame_round_trips_write_frame() {
        let (mut a, mut b) = tokio::io::duplex(64);
        let body = b"\x82\xa3foo\x01";
        write_frame(&mut a, body).await.unwrap();
        let got = read_frame(&mut b).await.unwrap();
        assert_eq!(got, body);
    }

    #[tokio::test]
    async fn read_frame_handles_empty_body() {
        let (mut a, mut b): (DuplexStream, DuplexStream) = tokio::io::duplex(64);
        write_frame(&mut a, b"").await.unwrap();
        let got = read_frame(&mut b).await.unwrap();
        assert!(got.is_empty());
    }

    #[tokio::test]
    async fn read_frame_rejects_oversized_length_before_allocating() {
        // A peer advertises a length one byte over the cap. read_frame must
        // reject it without allocating (and without waiting for the body).
        let (mut a, mut b): (DuplexStream, DuplexStream) = tokio::io::duplex(64);
        let oversized = (MAX_FRAME_SIZE as u32 + 1).to_be_bytes();
        a.write_all(&oversized).await.unwrap();
        let err = read_frame(&mut b).await.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
        assert!(err.to_string().contains("maximum allowed size"));
    }
}
