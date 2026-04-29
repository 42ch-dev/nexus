//! `RpcTransport` trait + concrete impls for worker IPC.
//!
//! The `RpcTransport` trait is the **insurance layer** — callers depend on
//! `&mut dyn RpcTransport`, never on `jsonrpsee::*` directly. This allows
//! swapping jsonrpsee-core for an alternative implementation without
//! propagating changes through the codebase.
//!
//! For `IpcClient` (multiplexed concurrent requests), the transport can be
//! split into read and write halves via [`RpcTransport::split`].
//!
//! Design: `.agents/plans/knowledge/crate-selection-best-practices-v1.md` §3.1.

use async_trait::async_trait;
use futures_util::SinkExt; // for FramedWrite::send
use futures_util::StreamExt; // for FramedRead::next
use std::io;
use tokio_util::codec::{FramedRead, FramedWrite, LinesCodec};

// ---------------------------------------------------------------------------
// Split transport traits (for IpcClient concurrent read/write)
// ---------------------------------------------------------------------------

/// Read half of a line-delimited transport.
///
/// Used by [`IpcClient`](crate::worker::IpcClient)'s background reader task
/// which is the sole consumer of the read side.
pub trait RpcTransportRead: Send + 'static {
    /// Receive the next newline-delimited message. Returns `None` on EOF.
    fn recv(
        &mut self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<String>> + Send + '_>>;
}

/// Write half of a line-delimited transport.
///
/// Shared via `Mutex` inside [`IpcClient`](crate::worker::IpcClient) so
/// that multiple concurrent `call()` requests can serialise writes.
pub trait RpcTransportWrite: Send + 'static {
    /// Send a complete line (newline is appended automatically).
    fn send(
        &mut self,
        line: String,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = io::Result<()>> + Send + '_>>;
}

// ---------------------------------------------------------------------------
// Combined trait (backward compat for one-shot call_json_rpc)
// ---------------------------------------------------------------------------

/// Abstraction over a line-delimited transport used by the JSON-RPC client.
///
/// Implementations MUST frame data as newline-delimited JSON (NDJSON):
/// each `send` appends `'\n'`, each `recv` reads until `'\n'`.
///
/// For multiplexed use, call [`RpcTransport::split`] to obtain separate
/// read and write halves.
#[async_trait]
pub trait RpcTransport: Send + 'static {
    /// Receive the next newline-delimited message. Returns `None` on EOF.
    async fn recv(&mut self) -> Option<String>;

    /// Send a complete line (newline is appended automatically).
    async fn send(&mut self, line: String) -> io::Result<()>;

    /// Split this transport into separate read and write halves.
    ///
    /// Used by [`IpcClient`](crate::worker::IpcClient) to allow concurrent
    /// reads (background task) and writes (multiple callers).
    fn split(self: Box<Self>) -> (Box<dyn RpcTransportRead>, Box<dyn RpcTransportWrite>);
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
    #[must_use]
    pub fn new(stdin: tokio::process::ChildStdin, stdout: tokio::process::ChildStdout) -> Self {
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
        match self.reader.next().await {
            Some(Ok(line)) => Some(line),
            Some(Err(e)) => {
                tracing::warn!(error = %e, "LinesCodec read error; closing transport");
                None
            }
            None => None,
        }
    }

    async fn send(&mut self, line: String) -> io::Result<()> {
        self.writer
            .send(line)
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    fn split(self: Box<Self>) -> (Box<dyn RpcTransportRead>, Box<dyn RpcTransportWrite>) {
        let reader = StdioReadHalf(self.reader);
        let writer = StdioWriteHalf(self.writer);
        (Box::new(reader), Box::new(writer))
    }
}

// Split halves for StdioTransport.

struct StdioReadHalf(FramedRead<tokio::process::ChildStdout, LinesCodec>);

impl RpcTransportRead for StdioReadHalf {
    fn recv(
        &mut self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<String>> + Send + '_>> {
        Box::pin(async move {
            match self.0.next().await {
                Some(Ok(line)) => Some(line),
                Some(Err(e)) => {
                    tracing::warn!(error = %e, "LinesCodec read error; closing transport");
                    None
                }
                None => None,
            }
        })
    }
}

struct StdioWriteHalf(FramedWrite<tokio::process::ChildStdin, LinesCodec>);

impl RpcTransportWrite for StdioWriteHalf {
    fn send(
        &mut self,
        line: String,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = io::Result<()>> + Send + '_>> {
        Box::pin(async move {
            self.0
                .send(line)
                .await
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
        })
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
    read_half:
        FramedRead<tokio::io::BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>, LinesCodec>,
    write_half: FramedWrite<
        tokio::io::BufWriter<tokio::io::WriteHalf<tokio::io::DuplexStream>>,
        LinesCodec,
    >,
}

impl DuplexTransport {
    /// Create a connected pair: `(client, server)`.
    #[must_use]
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
        match self.read_half.next().await {
            Some(Ok(line)) => Some(line),
            Some(Err(e)) => {
                tracing::warn!(error = %e, "LinesCodec read error; closing transport");
                None
            }
            None => None,
        }
    }

    async fn send(&mut self, line: String) -> io::Result<()> {
        self.write_half
            .send(line)
            .await
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    fn split(self: Box<Self>) -> (Box<dyn RpcTransportRead>, Box<dyn RpcTransportWrite>) {
        let reader = DuplexReadHalf(self.read_half);
        let writer = DuplexWriteHalf(self.write_half);
        (Box::new(reader), Box::new(writer))
    }
}

// Split halves for DuplexTransport.

struct DuplexReadHalf(
    FramedRead<tokio::io::BufReader<tokio::io::ReadHalf<tokio::io::DuplexStream>>, LinesCodec>,
);

impl RpcTransportRead for DuplexReadHalf {
    fn recv(
        &mut self,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Option<String>> + Send + '_>> {
        Box::pin(async move {
            match self.0.next().await {
                Some(Ok(line)) => Some(line),
                Some(Err(e)) => {
                    tracing::warn!(error = %e, "LinesCodec read error; closing transport");
                    None
                }
                None => None,
            }
        })
    }
}

struct DuplexWriteHalf(
    FramedWrite<tokio::io::BufWriter<tokio::io::WriteHalf<tokio::io::DuplexStream>>, LinesCodec>,
);

impl RpcTransportWrite for DuplexWriteHalf {
    fn send(
        &mut self,
        line: String,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = io::Result<()>> + Send + '_>> {
        Box::pin(async move {
            self.0
                .send(line)
                .await
                .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
        })
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

        client
            .send(r#"{"hello":"world"}"#.to_string())
            .await
            .unwrap();
        let msg = server.recv().await.expect("server receives message");
        assert_eq!(msg, r#"{"hello":"world"}"#);

        server.send(r#"{"reply":42}"#.to_string()).await.unwrap();
        let reply = client.recv().await.expect("client receives reply");
        assert_eq!(reply, r#"{"reply":42}"#);
    }

    #[tokio::test]
    async fn duplex_split_concurrent() {
        // Verify that split halves can do concurrent read and write.
        let (client, mut server) = DuplexTransport::new_pair();
        let (mut reader, mut writer) = Box::new(client).split();

        let write_handle = tokio::spawn(async move {
            writer
                .send(r#"{"method":"test"}"#.to_string())
                .await
                .expect("send");
        });

        // Server reads and replies.
        let server_handle = tokio::spawn(async move {
            let msg = server.recv().await.expect("server receives");
            assert_eq!(msg, r#"{"method":"test"}"#);
            server
                .send(r#"{"jsonrpc":"2.0","id":1,"result":{"ok":true}}"#.to_string())
                .await
                .expect("server send");
        });

        let read_handle = tokio::spawn(async move {
            let msg = reader.recv().await.expect("client receives reply");
            msg
        });

        write_handle.await.expect("write task");
        server_handle.await.expect("server task");
        let reply = read_handle.await.expect("read task");
        assert_eq!(reply, r#"{"jsonrpc":"2.0","id":1,"result":{"ok":true}}"#);
    }
}
