//! WebSocket `Transport` for the peer-tools Connect face (AR-57).
//!
//! `WsTransport` implements [`spoke_connect::remote::Transport`] over
//! `tokio-tungstenite` (server + client ends) behind the `connect-client`
//! feature. Framing follows the frozen contract §2: **one connect envelope =
//! one WebSocket message** — `send` emits exactly one Binary message carrying
//! the full envelope bytes, `recv` returns the next complete inbound message.
//! Envelope bytes are opaque; this transport MUST NOT parse or re-serialize
//! them.
//!
//! Semantics (AR-57):
//! - `send`: resolves when the socket has accepted the bytes.
//! - `recv`: fails fast with [`TransportError::Closed`] on connection loss
//!   (never parks past the peer's disconnect).
//! - `close`: idempotent teardown; both directions close together (the WS
//!   close frame is flushed, then the socket halves are dropped so the peer
//!   observes the FIN immediately).
//! - Socket/protocol errors → [`TransportError::Io`].
//! - Max inbound WS message size default **2 MiB** (config-gated); oversized
//!   or non-Binary message → [`TransportError::Io`] and the session is
//!   self-closed (fail-closed, AR-57 #3). Mirrors the spoke 2 MiB
//!   response-cap convention.
//!
//! The socket is split into `SplitSink`/`SplitStream` halves held behind
//! `tokio::sync::Mutex` (the [`Transport`] trait is `&self`-based while
//! `SinkExt`/`StreamExt` need `&mut`), so a pending `recv` never blocks
//! `send` and vice versa — the same concurrency the loopback pair provides
//! for free. `close()` signals a [`tokio::sync::Notify`] so any parked
//! `recv` wakes with [`TransportError::Closed`] even before the socket
//! halves are released.

use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use spoke_connect::remote::{Transport, TransportError};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{Mutex, Notify};
use tokio_tungstenite::tungstenite::protocol::{Message, WebSocketConfig};
use tokio_tungstenite::tungstenite::Error as WsError;
use tokio_tungstenite::WebSocketStream;

/// Default maximum inbound WS message size (bytes) — AR-57 #3.
pub const DEFAULT_MAX_ENVELOPE_BYTES: usize = 2 * 1024 * 1024;

/// Build the tungstenite config enforcing the 2 MiB inbound cap.
#[must_use]
pub fn ws_config(max_envelope_bytes: usize) -> WebSocketConfig {
    WebSocketConfig::default()
        .max_message_size(Some(max_envelope_bytes))
        .max_frame_size(Some(max_envelope_bytes))
}

/// Error-mapping helper: a tungstenite failure → [`TransportError`].
///
/// Connection loss (normal close / already closed) maps to
/// [`TransportError::Closed`] so the adapter's pending invokes fail fast;
/// protocol violations, capacity overruns and IO faults are transport-level
/// IO errors (frozen contract §2: socket/protocol errors → `Io`).
fn map_ws_error(error: WsError) -> TransportError {
    match error {
        WsError::ConnectionClosed | WsError::AlreadyClosed => TransportError::Closed,
        WsError::Io(io_error) => TransportError::Io(format!("ws io error: {io_error}")),
        other => TransportError::Io(format!("ws error: {other}")),
    }
}

/// One end of a WebSocket connection implementing the spoke message-oriented
/// [`Transport`] seam.
pub struct WsTransport<S> {
    sink: Mutex<Option<SplitSink<WebSocketStream<S>, Message>>>,
    stream: Mutex<Option<SplitStream<WebSocketStream<S>>>>,
    /// Idempotent-close guard + fail-fast flag.
    closed: AtomicBool,
    /// Wakes a parked `recv` when the transport closes (fail-fast).
    closed_notify: Notify,
}

impl<S> WsTransport<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    /// Wrap an established (post-handshake) WebSocket stream into a
    /// [`Transport`] end.
    #[must_use]
    pub fn new(stream: WebSocketStream<S>) -> Self {
        let (sink, stream) = stream.split();
        Self {
            sink: Mutex::new(Some(sink)),
            stream: Mutex::new(Some(stream)),
            closed: AtomicBool::new(false),
            closed_notify: Notify::new(),
        }
    }

    /// Pull the next inbound message from the socket half, mapping stream
    /// end and socket errors per the contract. `None` from the fused stream
    /// means the peer disconnected (close frame or FIN) ⇒ `Closed`.
    async fn next_message(&self) -> Result<Message, TransportError> {
        tokio::select! {
            biased;
            // Fail fast when the transport is closed (peer close or local
            // `close`) even if the socket would otherwise park.
            () = self.closed_notify.notified() => Err(TransportError::Closed),
            message = async {
                let mut stream_guard = self.stream.lock().await;
                let stream = stream_guard.as_mut().ok_or(TransportError::Closed)?;
                let outcome = match stream.next().await {
                    Some(Ok(message)) => Ok(message),
                    Some(Err(error)) => Err(map_ws_error(error)),
                    // Stream fused: peer disconnected.
                    None => Err(TransportError::Closed),
                };
                drop(stream_guard);
                outcome
            } => message,
        }
    }

    /// Fail-closed teardown: mark closed, wake any parked `recv`, release
    /// both socket halves. Idempotent.
    async fn tear_down(&self) {
        if self.closed.swap(true, Ordering::Relaxed) {
            return;
        }
        self.closed_notify.notify_waiters();
        self.stream.lock().await.take();
        self.sink.lock().await.take();
    }
}

#[async_trait]
impl<S> Transport for WsTransport<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + Sync,
{
    async fn send(&self, envelope: &[u8]) -> Result<(), TransportError> {
        if self.closed.load(Ordering::Relaxed) {
            return Err(TransportError::Closed);
        }
        let mut sink_guard = self.sink.lock().await;
        let sink = sink_guard.as_mut().ok_or(TransportError::Closed)?;
        let outcome = sink
            .send(Message::Binary(envelope.to_vec().into()))
            .await
            .map_err(map_ws_error);
        drop(sink_guard);
        outcome
    }

    async fn recv(&self) -> Result<Vec<u8>, TransportError> {
        if self.closed.load(Ordering::Relaxed) {
            return Err(TransportError::Closed);
        }
        loop {
            let message = match self.next_message().await {
                Ok(message) => message,
                // Clean connection loss: fail fast, nothing to tear down.
                Err(TransportError::Closed) => return Err(TransportError::Closed),
                // Session poisoned (e.g. inbound envelope over the 2 MiB cap
                // ⇒ tungstenite Capacity): fail-closed — no further traffic
                // may flow on this transport.
                Err(io_error) => {
                    self.tear_down().await;
                    return Err(io_error);
                }
            };
            match message {
                Message::Binary(bytes) => return Ok(bytes.to_vec()),
                Message::Text(_) => {
                    // Non-Binary data message: fail-closed (AR-57 #3) —
                    // tear the session down so no further traffic can flow.
                    self.tear_down().await;
                    return Err(TransportError::Io(
                        "non-Binary WS message received; session closed (fail-closed)".into(),
                    ));
                }
                // Control frames are answered internally by tungstenite
                // (auto-pong); a close frame surfaces as stream end → Closed.
                Message::Ping(_) | Message::Pong(_) | Message::Close(_) | Message::Frame(_) => {}
            }
        }
    }

    async fn close(&self) -> Result<(), TransportError> {
        if self.closed.swap(true, Ordering::Relaxed) {
            // Already closed: idempotent no-op.
            return Ok(());
        }
        // Best-effort WS close handshake (flush the close frame), then
        // release both socket halves so the peer observes the FIN and any
        // parked `recv` on this end wakes with `Closed`.
        {
            let mut sink_guard = self.sink.lock().await;
            if let Some(sink) = sink_guard.as_mut() {
                let _ = sink.close().await;
            }
        }
        self.closed_notify.notify_waiters();
        self.stream.lock().await.take();
        self.sink.lock().await.take();
        Ok(())
    }
}

/// The [`WsTransport`] end type alias used by the accept/dial helpers.
pub type WsTcpTransport = WsTransport<tokio::net::TcpStream>;
