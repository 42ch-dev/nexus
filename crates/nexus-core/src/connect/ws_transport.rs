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
//! - `send`: resolves when the socket has accepted the bytes. A `close` (or a
//!   failed receive) cancels a send parked on a peer that stopped reading, and
//!   that send reports [`TransportError::Closed`] instead of pinning the half.
//! - `recv`: fails fast with [`TransportError::Closed`] on connection loss
//!   (never parks past the peer's disconnect).
//! - `close`: signals cancellation BEFORE any lock or flush, so a parked
//!   `send`/`recv` releases its half; the halves are then moved out of the
//!   transport, flushed with a bounded best-effort close frame and dropped so
//!   the peer observes the FIN. Concurrent and repeated callers share the ONE
//!   settled outcome: a close that cannot flush within the bound is reported as
//!   an I/O error rather than as success, and a cancelled caller's attempt
//!   leaves its already-taken halves released while a waiting caller finishes
//!   the settlement (I-000259).
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
//! `recv` or `send` wakes with [`TransportError::Closed`] even before the
//! socket halves are released; both I/O paths re-check the flag under their
//! half's lock, so a notification that races the first check still ends the
//! call before it parks. The physical teardown is owned once
//! ([`tokio::sync::OnceCell`]), which is what makes a cancelled closer safe:
//! the halves it took drop with it, and a waiting closer carries the attempt.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{SinkExt, StreamExt};
use spoke_connect::remote::{Transport, TransportError};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{Mutex, Notify, OnceCell};
use tokio_tungstenite::tungstenite::protocol::{Message, WebSocketConfig};
use tokio_tungstenite::tungstenite::Error as WsError;
use tokio_tungstenite::WebSocketStream;

/// Default maximum inbound WS message size (bytes) — AR-57 #3.
pub const DEFAULT_MAX_ENVELOPE_BYTES: usize = 2 * 1024 * 1024;

/// Bound on the graceful close-frame flush in [`Transport::close`]. The socket
/// halves are always released, so a peer that stopped reading cannot hold the
/// teardown open.
const CLOSE_BOUND: Duration = Duration::from_secs(2);

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
    /// Wakes a parked `recv`/`send` when the transport closes (fail-fast).
    closed_notify: Notify,
    /// The settled outcome of the ONE physical teardown. The first `close`
    /// runs it; concurrent and repeated callers await the same result, so a
    /// caller never reports a close it merely started, and a cancelled owner
    /// hands the attempt to a waiting caller. The halves themselves are taken
    /// out of the slots above, so a cancelled attempt drops (releases) them.
    teardown: OnceCell<Result<(), TransportError>>,
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
            teardown: OnceCell::new(),
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
                // A close can race the fail-fast check above; the lock is where
                // the two meet, so re-check it before parking on the socket.
                if self.closed.load(Ordering::SeqCst) {
                    return Err(TransportError::Closed);
                }
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
        if self.closed.swap(true, Ordering::SeqCst) {
            return;
        }
        // Signal BEFORE taking a half: a parked `recv`/`send` holds its half's
        // mutex across the native poll, so it can only release it after it
        // observes the close.
        self.closed_notify.notify_waiters();
        self.stream.lock().await.take();
        self.sink.lock().await.take();
    }

    /// Take both socket halves out of the shared slots and dispose of them.
    ///
    /// Taking first is what makes teardown cancellation-safe: the halves move
    /// into this future, so dropping it (a cancelled `close`) drops them and
    /// the socket is released; a later attempt finds them gone instead of
    /// reviving a half another caller already released. The close-frame flush
    /// is best-effort and bounded — a peer that stopped reading cannot hold the
    /// teardown open, and that terminal condition is surfaced rather than
    /// reported as a clean close.
    async fn dispose(&self) -> Result<(), TransportError> {
        let sink = self.sink.lock().await.take();
        let stream = self.stream.lock().await.take();
        drop(stream);
        let Some(mut sink) = sink else {
            // The write half was already taken (and released) by an earlier or
            // cancelled attempt: there is nothing left for this one to dispose.
            return Ok(());
        };
        let outcome = match tokio::time::timeout(CLOSE_BOUND, sink.close()).await {
            // The peer completed the handshake, or the write reported a
            // connection that is already gone. The close frame is best-effort
            // by this transport's retained contract (a peer that already
            // closed resets the write, and the existing consumer contract
            // requires that close to settle as success), while the disposal
            // below is what this call owes.
            Ok(Ok(())) | Ok(Err(_)) => Ok(()),
            // The peer did not accept the close within the bound. The halves
            // still go away below; the refusal is the close's real outcome.
            Err(_) => Err(TransportError::Io(format!(
                "ws close frame was not flushed within {CLOSE_BOUND:?}"
            ))),
        };
        drop(sink);
        outcome
    }
}

#[async_trait]
impl<S> Transport for WsTransport<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + Sync,
{
    async fn send(&self, envelope: &[u8]) -> Result<(), TransportError> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(TransportError::Closed);
        }
        tokio::select! {
            biased;
            // A close releases this send instead of waiting for the sink: the
            // cancelled send drops its sink guard, so `close` can take the half
            // even while the peer is not reading.
            () = self.closed_notify.notified() => Err(TransportError::Closed),
            outcome = async {
                let mut sink_guard = self.sink.lock().await;
                // The notification can race the fail-fast check above; the lock
                // is where the two meet, so re-check it before writing.
                if self.closed.load(Ordering::SeqCst) {
                    return Err(TransportError::Closed);
                }
                let sink = sink_guard.as_mut().ok_or(TransportError::Closed)?;
                let outcome = sink
                    .send(Message::Binary(envelope.to_vec().into()))
                    .await
                    .map_err(map_ws_error);
                drop(sink_guard);
                outcome
            } => outcome,
        }
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
        // Cancellation must be visible BEFORE any lock or flush: a parked
        // `send`/`recv` holds its half's mutex across the native poll, so it can
        // only release that half once it observes this signal.
        self.closed.store(true, Ordering::SeqCst);
        self.closed_notify.notify_waiters();
        // One physical teardown per transport: every concurrent or repeated
        // caller returns this settled outcome, never the fact that a close was
        // merely started, and a cancelled owner hands the attempt to a waiter.
        self.teardown.get_or_init(|| self.dispose()).await.clone()
    }
}

/// The [`WsTransport`] end type alias used by the accept/dial helpers.
pub type WsTcpTransport = WsTransport<tokio::net::TcpStream>;
