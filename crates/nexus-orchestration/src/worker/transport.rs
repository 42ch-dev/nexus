//! RpcTransport trait + concrete impls for worker IPC.
//!
//! The `RpcTransport` trait is the **insurance layer** — callers depend on
//! `&mut dyn RpcTransport`, never on `jsonrpsee::*` directly. This allows
//! swapping jsonrpsee-core for an alternative implementation without
//! propagating changes through the codebase.
//!
//! Design: `.agents/plans/knowledge/crate-selection-best-practices-v1.md` §3.1.

use async_trait::async_trait;
use futures_util::SinkExt; // for FramedWrite::send
use futures_util::StreamExt; // for FramedRead::next
use std::io;
use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec};

// ---------------------------------------------------------------------------
// Trait
// ---------------------------------------------------------------------------

/// Abstraction over a line-delimited transport used by the JSON-RPC client.
///
/// Implementations MUST frame data as newline-delimited JSON (NDJSON):
/// each `send` appends `'\n'`, each `recv` reads until `'\n'`.
#[async_trait]
pub trait RpcTransport: Send + 'static {
    /// Receive the next newline-delimited message. Returns `None` on EOF.
    async fn recv(&mut self) -> Option<String>;

    /// Send a complete line (newline is appended automatically).
    async fn send(&mut self, line: String) -> io::Result<()>;
}

// ---------------------------------------------------------------------------
// StdioTransport — wraps child process stdin/stdout
// ---------------------------------------------------------------------------

/// NDJSON transport over a child process's stdin + stdout pipes.
///
/// Uses `LinesCodec` for framing. The caller owns the
/// `tokio::process::ChildStdin` / `ChildStdout` halves.
pub struct StdioTransport {
    reader: FramedRead<tokio::process::ChildStdout, LinesCodec>,
    writer: FramedWrite<tokio::process::ChildStdin, LinesCodec>,
}

impl StdioTransport {
    /// Create a new stdio transport from a child process's pipe halves.
    pub fn new(
        stdin: tokio::process::ChildStdin,
        stdout: tokio::process::ChildStdout,
    ) -> Self {
        // Use LinesCodec with a generous max-line-length (1 MiB).
        let codec = LinesCodec::new_with_max_length(1024 * 1024);
        Self {
            reader: FramedRead::new(stdout, codec.clone()),
            writer: FramedWrite::new(stdin, codec),
        }
    }
}

#[async_trait]
impl RpcTransport for StdioTransport {
    async fn recv(&mut self) -> Option<String> {
        self.reader
            .next()
            .await
            .map(|r| r.expect("LinesCodec error"))
    }

    async fn send(&mut self, line: String) -> io::Result<()> {
        self.writer
            .send(line)
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}

// ---------------------------------------------------------------------------
// DuplexTransport — in-memory mock for tests
// ---------------------------------------------------------------------------

/// In-memory NDJSON transport backed by `tokio::io::duplex`.
///
/// Each half (`client` / `server`) can `recv` what the other `send`s.
/// Used in tests to avoid spawning real child processes.
pub struct DuplexTransport {
    read_half: FramedRead<tokio::io::BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>, LinesCodec>,
    write_half: FramedWrite<tokio::io::BufWriter<tokio::io::WriteHalf<tokio::io::DuplexStream>>, LinesCodec>,
}

impl DuplexTransport {
    /// Create a connected pair: `(client, server)`.
    pub fn new_pair() -> (Self, Self) {
        let (client_stream, server_stream) = tokio::io::duplex(64 * 1024);
        let client = Self::from_stream(client_stream);
        let server = Self::from_stream(server_stream);
        (client, server)
    }

    fn from_stream(stream: tokio::io::DuplexStream) -> Self {
        let codec = LinesCodec::new_with_max_length(1024 * 1024);
        let (read_half, write_half) = tokio::io::split(stream);
        Self {
            read_half: FramedRead::new(tokio::io::BufReader::new(read_half), codec.clone()),
            write_half: FramedWrite::new(tokio::io::BufWriter::new(write_half), codec),
        }
    }
}

#[async_trait]
impl RpcTransport for DuplexTransport {
    async fn recv(&mut self) -> Option<String> {
        self.read_half
            .next()
            .await
            .map(|r| r.expect("LinesCodec error"))
    }

    async fn send(&mut self, line: String) -> io::Result<()> {
        self.write_half
            .send(line)
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn duplex_roundtrip() {
        let (mut client, mut server) = DuplexTransport::new_pair();

        client.send(r#"{"hello":"world"}"#.to_string()).await.unwrap();
        let msg = server.recv().await.expect("server receives message");
        assert_eq!(msg, r#"{"hello":"world"}"#);

        server
            .send(r#"{"reply":42}"#.to_string())
            .await
            .unwrap();
        let reply = client.recv().await.expect("client receives reply");
        assert_eq!(reply, r#"{"reply":42}"#);
    }
}
