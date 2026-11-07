//! WebSocket `Transport` for the peer-tools Connect face (AR-57).
//!
//! `WsTransport` implements [`spoke_connect::remote::Transport`] over
//! `tokio-tungstenite` (server + client ends) behind the `connect-client` /
//! `memory-remote` features. Framing follows the frozen contract §2: **one
//! connect envelope = one WebSocket message** — `send` emits exactly one Binary
//! message carrying the full envelope bytes, `recv` returns the next complete
//! inbound message. Envelope bytes are opaque; this transport MUST NOT parse or
//! re-serialize them.
//!
//! Semantics (AR-57):
//! - `send`: resolves when the socket has accepted the bytes. A `close` (or a
//!   failed receive) cancels a send parked on a peer that stopped reading, and
//!   that send reports [`TransportError::Closed`] instead of pinning the half.
//! - `recv`: fails fast with [`TransportError::Closed`] on connection loss
//!   (never parks past the peer's disconnect).
//! - `close`: signals cancellation BEFORE any lock or flush, so a parked
//!   `send`/`recv` releases its half. The ONE physical teardown then runs as its
//!   own owner — not in the caller's future — so cancelling the closer cannot
//!   abandon it: it moves both halves out of the transport, flushes a bounded
//!   close frame and drops them. Concurrent and repeated callers await the one
//!   settled result, which is `Ok` only when the close completed or the peer was
//!   already gone; a flush that cannot complete is reported as an I/O error and
//!   retained for later callers (I-000259).
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
//! for free. Each I/O step polls its half ONCE inside the lock and then
//! releases it before awaiting a socket/close wake, so a caller that is polled
//! `Pending` and then retained without being polled again holds no physical
//! half: the teardown can still take the halves, release the socket and settle
//! the close. `close()` wakes every parked step with [`TransportError::Closed`],
//! and each step re-checks the flag under its half's lock, so a notification
//! that races the first check still ends the call instead of parking it. The
//! whole-operation `send`/`recv` slots are acquired close-aware as well: a
//! same-direction call that queued behind a retained holder, or one started
//! after the close, returns [`TransportError::Closed`] promptly instead of
//! waiting for that slot. That wait listens to the close signal only — ordinary
//! socket readiness in either direction must never evict a queued caller. The
//! halves and the result are owned by a dedicated teardown task (`Arc`ed slots
//! only — never the transport itself), which is what keeps the disposal alive
//! when the initiating caller is cancelled.

use std::io::ErrorKind;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex, PoisonError};
use std::task::{Context, Poll, Wake, Waker};
use std::time::Duration;

use async_trait::async_trait;
use futures_util::stream::{SplitSink, SplitStream};
use futures_util::{Sink, SinkExt, Stream, StreamExt};
use spoke_connect::remote::{Transport, TransportError};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::{Mutex, Notify};
use tokio_tungstenite::tungstenite::protocol::{Message, WebSocketConfig};
use tokio_tungstenite::tungstenite::Error as WsError;
use tokio_tungstenite::WebSocketStream;

/// Default maximum inbound WS message size (bytes) — AR-57 #3.
pub const DEFAULT_MAX_ENVELOPE_BYTES: usize = 2 * 1024 * 1024;

/// Bound on the graceful close-frame flush in [`Transport::close`]. The socket
/// halves are always released, so a peer that stopped reading cannot hold the
/// teardown open.
const CLOSE_BOUND: Duration = Duration::from_secs(2);

type SinkHalf<S> = SplitSink<WebSocketStream<S>, Message>;
type StreamHalf<S> = SplitStream<WebSocketStream<S>>;

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

/// Whether a write failure means the peer connection is already gone.
///
/// Only these conditions are "already closed"; every other write failure (TLS,
/// protocol, capacity, permission or arbitrary I/O) is a real close error that
/// must be surfaced instead of being reported as a clean close.
const fn is_peer_gone(kind: ErrorKind) -> bool {
    matches!(
        kind,
        ErrorKind::ConnectionReset
            | ErrorKind::ConnectionAborted
            | ErrorKind::BrokenPipe
            | ErrorKind::NotConnected
    )
}

/// Classify the outcome of the best-effort close-frame write.
///
/// A connection that is already closed (or whose peer went away mid-write) has
/// reached the state `close` asks for, so it settles successfully — that is the
/// retained existing consumer contract. Anything else is the close's real
/// terminal error and is surfaced.
fn close_frame_outcome(error: WsError) -> Result<(), TransportError> {
    match error {
        WsError::Io(io_error) if is_peer_gone(io_error.kind()) => Ok(()),
        WsError::ConnectionClosed
        | WsError::AlreadyClosed
        | WsError::Protocol(
            tokio_tungstenite::tungstenite::error::ProtocolError::ResetWithoutClosingHandshake,
        ) => Ok(()),
        other => Err(map_ws_error(other)),
    }
}

/// One end of a WebSocket connection implementing the spoke message-oriented
/// [`Transport`] seam.
pub struct WsTransport<S> {
    /// The write half. `Arc`ed so the teardown task can own its disposal
    /// without holding the transport (no self-cycle).
    sink: Arc<Mutex<Option<SinkHalf<S>>>>,
    /// The read half, shared with the teardown task the same way.
    stream: Arc<Mutex<Option<StreamHalf<S>>>>,
    /// Serialises whole `send` calls. It owns no physical half, so a caller
    /// that is polled `Pending` and then retained never keeps the teardown
    /// from taking the socket halves.
    send_slot: Mutex<()>,
    /// The same for whole `recv` calls.
    recv_slot: Mutex<()>,
    /// Idempotent-close guard + fail-fast flag.
    closed: AtomicBool,
    /// Wakes a parked I/O step for a socket event or a close.
    io_wake: Arc<IoWake>,
    /// Wakes only a close: ordinary socket readiness must never be mistaken
    /// for one.
    closed_signal: Notify,
    /// The one physical teardown and its settled result.
    teardown: Arc<Teardown>,
}

/// Wakes a parked `send`/`recv` step for a socket event or a close.
///
/// Each I/O step holds its half only for one synchronous poll and then parks on
/// this notification, so a caller that keeps the future alive without polling
/// it again holds no socket half. The close wakes it so the step can observe the
/// close instead of staying parked.
struct IoWake {
    notify: Notify,
}

impl Wake for IoWake {
    fn wake(self: Arc<Self>) {
        self.notify.notify_waiters();
    }

    fn wake_by_ref(self: &Arc<Self>) {
        self.notify.notify_waiters();
    }
}

/// The single physical teardown of one transport.
///
/// The first `close` (or a poisoned receive) signals and starts
/// [`Self::run`] as its own task, so the disposal is owned independently of the
/// initiating waiter: cancelling that caller cannot abandon it, and a later
/// caller does not have to continue it. Every caller awaits the one settled
/// result instead of inferring success from the close having started.
struct Teardown {
    /// Settled synchronously: the receiving side may write it from a `Drop`
    /// guard, which cannot await.
    state: StdMutex<TeardownState>,
    /// Wakes every caller waiting for the settled result.
    settled: Notify,
}

enum TeardownState {
    Idle,
    Running,
    Settled(Result<(), TransportError>),
}

impl Teardown {
    fn new() -> Self {
        Self {
            state: StdMutex::new(TeardownState::Idle),
            settled: Notify::new(),
        }
    }

    /// Record the one terminal outcome and wake every waiter.
    fn settle(&self, outcome: Result<(), TransportError>) {
        {
            let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            if !matches!(*state, TeardownState::Running) {
                return;
            }
            *state = TeardownState::Settled(outcome);
        }
        self.settled.notify_waiters();
    }
}

/// Move both socket halves out of their slots, flush a bounded close frame and
/// release them. Owned by the teardown task, never by a caller's future.
async fn dispose<S>(
    sink_half: Arc<Mutex<Option<SinkHalf<S>>>>,
    stream_half: Arc<Mutex<Option<StreamHalf<S>>>>,
) -> Result<(), TransportError>
where
    S: AsyncRead + AsyncWrite + Unpin + Send,
{
    // The signal already woke any parked I/O, so these waits are bounded by the
    // runtime scheduling that woken call: taking first moves the halves into
    // this task, where the flush below and the final drops release them.
    let sink = sink_half.lock().await.take();
    let stream = stream_half.lock().await.take();
    drop(stream);
    let Some(mut sink) = sink else {
        // Already taken by a previous attempt: nothing left to dispose.
        return Ok(());
    };
    let outcome = match tokio::time::timeout(CLOSE_BOUND, sink.close()).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => close_frame_outcome(error),
        Err(_) => Err(TransportError::Io(
            "ws close frame was not flushed within the close bound".to_string(),
        )),
    };
    drop(sink);
    outcome
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
            sink: Arc::new(Mutex::new(Some(sink))),
            stream: Arc::new(Mutex::new(Some(stream))),
            send_slot: Mutex::new(()),
            recv_slot: Mutex::new(()),
            closed: AtomicBool::new(false),
            io_wake: Arc::new(IoWake {
                notify: Notify::new(),
            }),
            closed_signal: Notify::new(),
            teardown: Arc::new(Teardown::new()),
        }
    }

    /// Acquire one whole-operation slot, giving up as soon as the transport
    /// closes.
    ///
    /// The slot owns no physical half, but a retained `Pending` holder can keep
    /// it for an unbounded time. Pre-close callers still serialise behind it
    /// (wire order is unchanged); a caller already queued when the close lands,
    /// or one arriving afterwards, returns [`TransportError::Closed`] promptly
    /// instead of waiting for that slot — the close is what settles it, not a
    /// timeout.
    async fn lock_until_closed<'a>(
        &'a self,
        slot: &'a Mutex<()>,
    ) -> Result<tokio::sync::MutexGuard<'a, ()>, TransportError> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(TransportError::Closed);
        }
        // Subscribe to the close signal ONLY: the I/O notification also fires
        // for ordinary socket readiness, which must not evict a queued caller.
        let notified = self.closed_signal.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();
        if self.closed.load(Ordering::SeqCst) {
            return Err(TransportError::Closed);
        }
        tokio::select! {
            biased;
            () = notified.as_mut() => Err(TransportError::Closed),
            guard = slot.lock() => {
                // A slot granted at the same moment as the close still loses
                // to the close.
                if self.closed.load(Ordering::SeqCst) {
                    Err(TransportError::Closed)
                } else {
                    Ok(guard)
                }
            }
        }
    }
}

#[async_trait]
impl<S> Transport for WsTransport<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static,
{
    #[allow(clippy::significant_drop_tightening)] // the sink guard spans one poll, never an await
    async fn send(&self, envelope: &[u8]) -> Result<(), TransportError> {
        // Serialise whole sends without owning a physical half, and never wait
        // on that serialisation past a close.
        let _slot = self.lock_until_closed(&self.send_slot).await?;
        if self.closed.load(Ordering::SeqCst) {
            return Err(TransportError::Closed);
        }
        let mut message = Some(Message::Binary(envelope.to_vec().into()));
        let mut staged = false;
        loop {
            // Register the wake before the poll so a socket wake that lands in
            // between is never lost.
            let notified = self.io_wake.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let outcome = {
                // The physical half is held only across this one synchronous
                // poll and released before the await below; a retained caller
                // therefore never keeps it from the teardown.
                let mut sink_guard = self.sink.lock().await;
                if self.closed.load(Ordering::SeqCst) {
                    return Err(TransportError::Closed);
                }
                let sink = sink_guard.as_mut().ok_or(TransportError::Closed)?;
                let waker = Waker::from(Arc::clone(&self.io_wake));
                let mut context = Context::from_waker(&waker);
                let mut pinned = Pin::new(sink);
                // Named positively: the already-staged path below is the
                // exceptional one, and the comment reads top-down.
                let unstaged = !staged;
                if unstaged {
                    match pinned.as_mut().poll_ready(&mut context) {
                        Poll::Ready(Ok(())) => {
                            let message = message.take().expect("one message per send");
                            if let Err(error) = pinned.as_mut().start_send(message) {
                                Poll::Ready(Err(map_ws_error(error)))
                            } else {
                                staged = true;
                                poll_flush(&mut pinned, &mut context)
                            }
                        }
                        Poll::Ready(Err(error)) => Poll::Ready(Err(map_ws_error(error))),
                        Poll::Pending => Poll::Pending,
                    }
                } else {
                    poll_flush(&mut pinned, &mut context)
                }
            };
            if let Poll::Ready(outcome) = outcome {
                return outcome;
            }
            notified.await;
        }
    }

    #[allow(clippy::significant_drop_tightening)] // the stream guard spans one poll, never an await
    async fn recv(&self) -> Result<Vec<u8>, TransportError> {
        // Serialise whole receives without owning a physical half, and never
        // wait on that serialisation past a close.
        let _slot = self.lock_until_closed(&self.recv_slot).await?;
        if self.closed.load(Ordering::SeqCst) {
            return Err(TransportError::Closed);
        }
        loop {
            let notified = self.io_wake.notify.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            let polled = {
                // The read half is held only across this one synchronous poll.
                let mut stream_guard = self.stream.lock().await;
                if self.closed.load(Ordering::SeqCst) {
                    return Err(TransportError::Closed);
                }
                let stream = stream_guard.as_mut().ok_or(TransportError::Closed)?;
                let waker = Waker::from(Arc::clone(&self.io_wake));
                let mut context = Context::from_waker(&waker);
                Pin::new(stream).poll_next(&mut context)
            };
            match polled {
                Poll::Ready(Some(Ok(Message::Binary(bytes)))) => return Ok(bytes.to_vec()),
                Poll::Ready(Some(Ok(Message::Text(_)))) => {
                    // Non-Binary data message: fail-closed (AR-57 #3) —
                    // tear the session down so no further traffic can flow.
                    self.signal();
                    return Err(TransportError::Io(
                        "non-Binary WS message received; session closed (fail-closed)".into(),
                    ));
                }
                // Control frames are answered internally by tungstenite
                // (auto-pong); a close frame surfaces as stream end → Closed.
                Poll::Ready(Some(Ok(
                    Message::Ping(_) | Message::Pong(_) | Message::Close(_) | Message::Frame(_),
                ))) => {}
                Poll::Ready(Some(Err(error))) => {
                    // Session poisoned (e.g. inbound envelope over the 2 MiB
                    // cap ⇒ tungstenite Capacity): fail-closed — no further
                    // traffic may flow on this transport.
                    self.signal();
                    return Err(map_ws_error(error));
                }
                // Stream fused: peer disconnected.
                Poll::Ready(None) => return Err(TransportError::Closed),
                Poll::Pending => notified.await,
            }
        }
    }

    async fn close(&self) -> Result<(), TransportError> {
        self.signal();
        self.settled_result().await
    }
}

impl<S> WsTransport<S>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + Sync + 'static,
{
    /// Signal the close and start the one physical teardown, if it has not
    /// started yet.
    ///
    /// The signal runs before any lock or flush so parked `send`/`recv` calls
    /// release their halves. The teardown then runs as its own task owning the
    /// two half slots: it survives this caller being cancelled, settles its
    /// result once, and never depends on a later caller taking over.
    fn signal(&self) {
        self.closed.store(true, Ordering::SeqCst);
        self.closed_signal.notify_waiters();
        self.io_wake.notify.notify_waiters();
        let mut state = self
            .teardown
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if !matches!(*state, TeardownState::Idle) {
            return;
        }
        *state = TeardownState::Running;
        drop(state);
        let sink = Arc::clone(&self.sink);
        let stream = Arc::clone(&self.stream);
        let teardown = Arc::clone(&self.teardown);
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                handle.spawn(async move {
                    let outcome = dispose(sink, stream).await;
                    teardown.settle(outcome);
                });
            }
            Err(_) => {
                // No runtime to own the disposal. Settle truthfully instead of
                // leaving callers on a teardown that can never run; the halves
                // are released when the transport drops.
                teardown.settle(Err(TransportError::Io(
                    "ws close requires a Tokio runtime".to_string(),
                )));
            }
        }
    }

    /// Await the one settled teardown result.
    async fn settled_result(&self) -> Result<(), TransportError> {
        loop {
            // Register before reading the state so a settlement that lands in
            // between cannot be missed.
            let notified = self.teardown.settled.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            {
                let state = self
                    .teardown
                    .state
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner);
                if let TeardownState::Settled(outcome) = &*state {
                    return outcome.clone();
                }
            }
            notified.await;
        }
    }
}

/// Poll one flush step of a `SplitSink` and map its error.
fn poll_flush<S>(
    sink: &mut Pin<&mut S>,
    context: &mut Context<'_>,
) -> Poll<Result<(), TransportError>>
where
    S: Sink<Message, Error = WsError> + Unpin,
{
    match sink.as_mut().poll_flush(context) {
        Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
        Poll::Ready(Err(error)) => Poll::Ready(Err(map_ws_error(error))),
        Poll::Pending => Poll::Pending,
    }
}

/// The [`WsTransport`] end type alias used by the accept/dial helpers.
pub type WsTcpTransport = WsTransport<tokio::net::TcpStream>;

#[cfg(test)]
mod tests {
    use super::*;

    fn io_error(kind: ErrorKind) -> WsError {
        WsError::Io(std::io::Error::from(kind))
    }

    /// Only a genuinely closed/peer-gone connection settles the best-effort
    /// close frame as success.
    #[test]
    fn close_frame_outcome_accepts_only_peer_gone_conditions() {
        assert_eq!(close_frame_outcome(WsError::ConnectionClosed), Ok(()));
        assert_eq!(close_frame_outcome(WsError::AlreadyClosed), Ok(()));
        assert_eq!(
            close_frame_outcome(WsError::Protocol(
                tokio_tungstenite::tungstenite::error::ProtocolError::ResetWithoutClosingHandshake,
            )),
            Ok(())
        );
        for kind in [
            ErrorKind::ConnectionReset,
            ErrorKind::ConnectionAborted,
            ErrorKind::BrokenPipe,
            ErrorKind::NotConnected,
        ] {
            assert_eq!(close_frame_outcome(io_error(kind)), Ok(()), "{kind:?}");
        }
    }

    /// Every other close-frame failure is a real terminal error and must reach
    /// the caller instead of being cached as a clean close.
    #[test]
    fn close_frame_outcome_surfaces_other_failures() {
        for kind in [
            ErrorKind::PermissionDenied,
            ErrorKind::WouldBlock,
            ErrorKind::TimedOut,
            ErrorKind::Other,
        ] {
            let outcome = close_frame_outcome(io_error(kind));
            assert!(
                matches!(outcome, Err(TransportError::Io(_))),
                "{kind:?} must be surfaced, got {outcome:?}"
            );
        }
        let protocol = close_frame_outcome(WsError::Protocol(
            tokio_tungstenite::tungstenite::error::ProtocolError::SendAfterClosing,
        ));
        assert!(
            matches!(protocol, Err(TransportError::Io(_))),
            "a protocol failure must be surfaced, got {protocol:?}"
        );
    }
}
