//! Bounded per-run workflow event rings and live SSE replay (v1.188 P4 §6.3).

use nexus_agent_host::capability::model::HostEvent;
use nexus_orchestration::run_state::RunRecord;
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use tokio::sync::mpsc;
use uuid::Uuid;

pub const MAX_RECORDS_PER_RUN: usize = 256;
pub const MAX_BYTES_PER_RUN: usize = 1024 * 1024;
pub const MAX_FRAME_BYTES: usize = 512 * 1024;
pub const MAX_LIVE_RINGS: usize = 64;
pub const MAX_TERMINAL_RINGS: usize = 64;
pub const MAX_SUBSCRIBERS_PER_RUN: usize = 16;
/// Data frames a live subscriber may lag behind before it is evicted as
/// lagging.
pub const MAX_PENDING_FRAMES_PER_SUB: usize = 16;
pub const MAX_PENDING_BYTES_PER_SUB: usize = 1024 * 1024;
/// Live-subscriber channel capacity: the data-frame cap PLUS one reserved
/// control slot (QC2 F-005).
///
/// The eviction path must ALWAYS be able to enqueue its terminal lag gap.
/// Parking the gap in a reserved slot (never handed out to data frames)
/// means a lagging reader drains its buffered data frames, then reads the
/// explicit gap, then observes a clean close — never a silent EOF on a
/// saturated channel.
const SUBSCRIBER_CHANNEL_CAPACITY: usize = MAX_PENDING_FRAMES_PER_SUB + 1;

/// Shared map of per-run sinks registered before drive; the coordinator and
/// [`crate::prompt_executor::HostPromptExecutor`] share this handle.
pub type RunEventSinkMap = Arc<tokio::sync::Mutex<std::collections::HashMap<String, RunEventSink>>>;

/// Root run id of any session id.
///
/// Nested prompt steps execute against CHILD sessions minted as
/// `<parent>:child:<uuid>` (recursively), but every host event belongs to the
/// one SSE ring owned by the driven ROOT run. Stripping at the FIRST
/// `:child:` yields that root for any descendant depth.
#[must_use]
pub fn root_run_id(session_id: &str) -> &str {
    session_id
        .split_once(CHILD_SESSION_INFIX)
        .map_or(session_id, |(root, _)| root)
}

/// Child-session id infix (mirrors `spawn_child_session_internal`).
pub const CHILD_SESSION_INFIX: &str = ":child:";

/// Resolve the sink that owns `run_id`: an exact match, else the sink of the
/// ROOT run that admitted it.
///
/// Without the root fallback a nested prompt (whose `run_id` is a child
/// session id) finds no sink and its `OpStarted`/content/`OpFinished` events
/// are silently dropped from the root ring — the very frames a late
/// subscriber replays from a retained terminal ring.
pub async fn sink_for_run(sinks: &RunEventSinkMap, run_id: &str) -> Option<RunEventSink> {
    let guard = sinks.lock().await;
    guard.get(run_id).cloned().or_else(|| {
        let root = root_run_id(run_id);
        (root != run_id).then(|| guard.get(root).cloned()).flatten()
    })
}

#[derive(Clone)]
pub struct RunEventSink {
    run_id: String,
    registry: Weak<RunEventRegistryInner>,
}

impl RunEventSink {
    pub fn publish_host_event(&self, step_id: &str, attempt_id: &str, host_event: &HostEvent) {
        let Some(registry) = self.registry.upgrade() else {
            return;
        };
        registry.publish_host_event(&self.run_id, step_id, attempt_id, host_event);
    }

    #[must_use]
    pub fn run_id(&self) -> &str {
        &self.run_id
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct HostEventWire<'a> {
    pub run_id: &'a str,
    pub epoch: Uuid,
    pub sequence: u64,
    pub step_id: &'a str,
    pub attempt_id: &'a str,
    pub host_event: &'a HostEvent,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunStateWire<'a> {
    pub run_id: &'a str,
    pub epoch: Uuid,
    pub sequence: u64,
    pub state_revision: u64,
    pub status: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct GapWire {
    pub run_id: String,
    pub epoch: Uuid,
    pub from_sequence: u64,
    pub to_sequence: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct HistoryUnavailableWire {
    pub run_id: String,
    pub inspect_url: String,
}

#[derive(Debug, Clone)]
pub struct SseFrame {
    pub id: String,
    pub event: String,
    pub data: String,
}

/// Live subscriber queue entry: carries the exact wire-byte reservation used
/// for backpressure accounting so dequeue/drop releases the same count.
#[derive(Debug, Clone)]
struct QueuedFrame {
    frame: SseFrame,
    wire_bytes: usize,
}

#[derive(Debug)]
pub enum SubscribeError {
    MalformedCursor,
    FutureCursor,
    TooManySubscribers,
    HistoryUnavailable(HistoryUnavailableWire),
}

pub struct RunEventRegistry {
    inner: Arc<RunEventRegistryInner>,
}

struct RunEventRegistryInner {
    state: Mutex<RegistryState>,
    next_subscriber_id: AtomicU64,
}

struct RegistryState {
    live: HashMap<String, RunRing>,
    terminal_order: VecDeque<String>,
    terminal: HashMap<String, RunRing>,
}

struct RunRing {
    run_id: String,
    epoch: Uuid,
    records: VecDeque<StoredRecord>,
    total_bytes: usize,
    next_sequence: u64,
    subscribers: HashMap<u64, Subscriber>,
    closed: bool,
}

struct StoredRecord {
    sequence: u64,
    frame: SseFrame,
    byte_len: usize,
}

struct Subscriber {
    tx: mpsc::Sender<QueuedFrame>,
    pending_frames: usize,
    pending_bytes: usize,
}

struct SubscriberRelease {
    inner: Weak<RunEventRegistryInner>,
    run_id: String,
    pub(crate) subscriber_id: u64,
}

impl Drop for SubscriberRelease {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.upgrade() {
            inner.unregister_subscriber(&self.run_id, self.subscriber_id);
        }
    }
}

/// Live SSE subscription: replay is delivered first; subsequent frames arrive
/// on [`recv`](Self::recv). Dropping releases the subscriber permit.
pub struct LiveSubscription {
    replay: VecDeque<SseFrame>,
    rx: mpsc::Receiver<QueuedFrame>,
    inner: Weak<RunEventRegistryInner>,
    run_id: String,
    subscriber_id: u64,
    _release: SubscriberRelease,
}

impl LiveSubscription {
    pub async fn recv(&mut self) -> Option<SseFrame> {
        if let Some(frame) = self.replay.pop_front() {
            return Some(frame);
        }
        let queued = self.rx.recv().await?;
        if let Some(inner) = self.inner.upgrade() {
            inner.decrement_subscriber_pending(&self.run_id, self.subscriber_id, queued.wire_bytes);
        }
        Some(queued.frame)
    }
}

impl Drop for LiveSubscription {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.upgrade() {
            while let Ok(queued) = self.rx.try_recv() {
                inner.decrement_subscriber_pending(
                    &self.run_id,
                    self.subscriber_id,
                    queued.wire_bytes,
                );
            }
        }
    }
}

impl Default for RunEventRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl RunEventRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RunEventRegistryInner {
                state: Mutex::new(RegistryState {
                    live: HashMap::new(),
                    terminal_order: VecDeque::new(),
                    terminal: HashMap::new(),
                }),
                next_subscriber_id: AtomicU64::new(1),
            }),
        }
    }

    // The `MAX_LIVE_RINGS` capacity check and the ring insertion MUST stay in
    // one critical section: splitting them (as the lint's early `drop`
    // suggests) would let two concurrent registrations both observe capacity
    // and both insert, exceeding the cap. The guard is therefore held to the
    // end of the atomic section on purpose.
    #[allow(clippy::significant_drop_tightening)]
    pub fn try_register_live(&self, run_id: &str) -> Option<RunEventSink> {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.live.len() >= MAX_LIVE_RINGS && !state.live.contains_key(run_id) {
            return None;
        }
        let epoch = current_epoch();
        state
            .live
            .entry(run_id.to_string())
            .or_insert_with(|| RunRing {
                run_id: run_id.to_string(),
                epoch,
                records: VecDeque::new(),
                total_bytes: 0,
                next_sequence: 1,
                subscribers: HashMap::new(),
                closed: false,
            });
        Some(RunEventSink {
            run_id: run_id.to_string(),
            registry: Arc::downgrade(&self.inner),
        })
    }

    pub fn publish_host_event_for_run(
        &self,
        run_id: &str,
        step_id: &str,
        attempt_id: &str,
        host_event: &HostEvent,
    ) {
        self.inner
            .publish_host_event(run_id, step_id, attempt_id, host_event);
    }

    pub fn publish_run_state(&self, run_id: &str, record: &RunRecord) {
        let reason = record
            .state
            .as_ref()
            .and_then(|s| s.failure.as_ref())
            .map(|f| f.message.clone())
            .or_else(|| {
                record
                    .state
                    .as_ref()
                    .and_then(|s| s.failure.as_ref())
                    .map(|f| f.code.clone())
            });
        let wire = RunStateWire {
            run_id,
            epoch: Uuid::nil(),
            sequence: 0,
            state_revision: record.state_revision,
            status: record.status.as_db_str(),
            reason,
        };
        self.inner.append_json(run_id, "run_state", &wire);
    }

    /// Close a run's live ring after its durable **authoritative terminal**
    /// winner was published (Completed/Failed/Cancelled).
    ///
    /// Callers must NOT use this for `Interrupted` (QC2 F-001): an
    /// unconfirmed cancel/failure cleanup leaves the run actionable, so its
    /// ring stays live for the retry that may still settle it. Closing it
    /// would turn the actionable run into a replay-only history and drop
    /// every live subscriber.
    // Lock/ring-move semantics preserved deliberately: the live-ring removal,
    // terminal insertion and terminal-order eviction are one critical section
    // (the lint's suggested early `drop` would sit after the last use and is a
    // no-op, so an explicit allow beats a meaningless statement).
    #[allow(clippy::significant_drop_tightening)]
    pub fn mark_terminal(&self, run_id: &str) {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(mut ring) = state.live.remove(run_id) {
            ring.closed = true;
            ring.subscribers.clear();
            state.terminal.insert(run_id.to_string(), ring);
            state.terminal_order.push_back(run_id.to_string());
            while state.terminal.len() > MAX_TERMINAL_RINGS {
                if let Some(evicted) = state.terminal_order.pop_front() {
                    state.terminal.remove(&evicted);
                }
            }
        } else if let Some(ring) = state.terminal.get_mut(run_id) {
            ring.closed = true;
        }
    }

    /// Live SSE: atomically replay from `Last-Event-ID`, then register for live tail.
    ///
    /// # Errors
    ///
    /// Returns [`SubscribeError::HistoryUnavailable`] when the run has no live
    /// or terminal ring, or when the cursor's epoch does not match the ring's;
    /// [`SubscribeError::MalformedCursor`] / [`SubscribeError::FutureCursor`]
    /// for an unparsable or future cursor; and
    /// [`SubscribeError::TooManySubscribers`] when the run's subscriber cap is
    /// already reached.
    ///
    /// # Panics
    ///
    /// Never in practice: the ring lookup is `.expect`ed only after the same
    /// map was checked to contain `run_id` under the same guard.
    pub fn subscribe_live(
        &self,
        run_id: &str,
        last_event_id: Option<&str>,
        inspect_url: String,
    ) -> Result<LiveSubscription, SubscribeError> {
        let (tx, rx) = mpsc::channel(SUBSCRIBER_CHANNEL_CAPACITY);
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let is_terminal = if state.live.contains_key(run_id) {
            false
        } else if state.terminal.contains_key(run_id) {
            true
        } else {
            return Err(SubscribeError::HistoryUnavailable(HistoryUnavailableWire {
                run_id: run_id.to_string(),
                inspect_url,
            }));
        };
        let ring = if is_terminal {
            state.terminal.get_mut(run_id)
        } else {
            state.live.get_mut(run_id)
        }
        .expect("ring exists");
        if !is_terminal && ring.subscribers.len() >= MAX_SUBSCRIBERS_PER_RUN {
            return Err(SubscribeError::TooManySubscribers);
        }
        let after_seq = if let Some(cursor) = last_event_id {
            let (epoch, seq) = parse_cursor(cursor)?;
            if epoch != ring.epoch {
                return Err(SubscribeError::HistoryUnavailable(HistoryUnavailableWire {
                    run_id: run_id.to_string(),
                    inspect_url,
                }));
            }
            if seq >= ring.next_sequence {
                return Err(SubscribeError::FutureCursor);
            }
            Some(seq)
        } else {
            None
        };
        let replay = collect_from(ring, run_id, after_seq);
        if is_terminal {
            return Ok(LiveSubscription {
                replay: replay.into(),
                rx,
                inner: Arc::downgrade(&self.inner),
                run_id: run_id.to_string(),
                subscriber_id: 0,
                _release: SubscriberRelease {
                    inner: Weak::new(),
                    run_id: run_id.to_string(),
                    subscriber_id: 0,
                },
            });
        }
        let subscriber_id = self.inner.next_subscriber_id.fetch_add(1, Ordering::SeqCst);
        ring.subscribers.insert(
            subscriber_id,
            Subscriber {
                tx,
                pending_frames: 0,
                pending_bytes: 0,
            },
        );
        Ok(LiveSubscription {
            replay: replay.into(),
            rx,
            inner: Arc::downgrade(&self.inner),
            run_id: run_id.to_string(),
            subscriber_id,
            _release: SubscriberRelease {
                inner: Arc::downgrade(&self.inner),
                run_id: run_id.to_string(),
                subscriber_id,
            },
        })
    }

    /// Current live subscriber count for a run (test/diagnostic).
    #[cfg(test)]
    pub fn live_subscriber_count(&self, run_id: &str) -> usize {
        self.inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .live
            .get(run_id)
            .map_or(0, |r| r.subscribers.len())
    }

    /// Pending backpressure counters for a live subscriber (test/diagnostic).
    #[cfg(test)]
    pub fn subscriber_pending(&self, run_id: &str, subscriber_id: u64) -> (usize, usize) {
        self.inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .live
            .get(run_id)
            .and_then(|r| r.subscribers.get(&subscriber_id))
            .map_or((0, 0), |s| (s.pending_frames, s.pending_bytes))
    }
}

impl RunEventRegistryInner {
    fn unregister_subscriber(&self, run_id: &str, subscriber_id: u64) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(ring) = state.live.get_mut(run_id) {
            ring.subscribers.remove(&subscriber_id);
        }
    }

    fn decrement_subscriber_pending(&self, run_id: &str, subscriber_id: u64, byte_len: usize) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(ring) = state.live.get_mut(run_id) {
            if let Some(sub) = ring.subscribers.get_mut(&subscriber_id) {
                sub.pending_frames = sub.pending_frames.saturating_sub(1);
                sub.pending_bytes = sub.pending_bytes.saturating_sub(byte_len);
            }
        }
    }

    fn ring_closed(&self, run_id: &str) -> bool {
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(ring) = state.live.get(run_id) {
            return ring.closed;
        }
        state.terminal.get(run_id).is_some_and(|ring| ring.closed)
    }

    fn publish_host_event(
        &self,
        run_id: &str,
        step_id: &str,
        attempt_id: &str,
        host_event: &HostEvent,
    ) {
        if self.ring_closed(run_id) {
            return;
        }
        let wire = HostEventWire {
            run_id,
            epoch: Uuid::nil(),
            sequence: 0,
            step_id,
            attempt_id,
            host_event,
        };
        self.append_json(run_id, "host_event", &wire);
    }

    fn append_json(&self, run_id: &str, event: &str, value: &impl Serialize) {
        if self.ring_closed(run_id) {
            return;
        }
        let Ok(data) = serde_json::to_string(value) else {
            return;
        };
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let ring = if let Some(r) = state.live.get_mut(run_id) {
            if r.closed {
                return;
            }
            r
        } else if let Some(r) = state.terminal.get_mut(run_id) {
            r
        } else {
            return;
        };
        let sequence = ring.next_sequence;
        ring.next_sequence += 1;
        let payload = serde_json::from_str::<serde_json::Value>(&data)
            .ok()
            .and_then(|mut v| {
                if let Some(obj) = v.as_object_mut() {
                    obj.insert("epoch".to_string(), serde_json::json!(ring.epoch));
                    obj.insert("sequence".to_string(), serde_json::json!(sequence));
                }
                serde_json::to_string(&v).ok()
            })
            .unwrap_or(data);
        let frame = SseFrame {
            id: format!("{}:{}", ring.epoch, sequence),
            event: event.to_string(),
            data: payload,
        };
        if encoded_sse_frame_bytes(&frame) > MAX_FRAME_BYTES {
            Self::append_gap_record_locked(ring, run_id, sequence);
            return;
        }
        Self::push_record_locked(ring, &frame);
    }

    fn append_gap_record_locked(ring: &mut RunRing, run_id: &str, skipped_sequence: u64) {
        let gap_sequence = ring.next_sequence;
        ring.next_sequence += 1;
        let gap = GapWire {
            run_id: run_id.to_string(),
            epoch: ring.epoch,
            from_sequence: skipped_sequence,
            to_sequence: skipped_sequence,
        };
        let data = serde_json::to_string(&gap).unwrap_or_default();
        let frame = SseFrame {
            id: format!("{}:{}", ring.epoch, gap_sequence),
            event: "gap".to_string(),
            data,
        };
        Self::push_record_locked(ring, &frame);
    }

    fn push_record_locked(ring: &mut RunRing, frame: &SseFrame) {
        let byte_len = encoded_sse_frame_bytes(frame);
        ring.records.push_back(StoredRecord {
            sequence: ring.next_sequence.saturating_sub(1),
            frame: frame.clone(),
            byte_len,
        });
        ring.total_bytes += byte_len;
        while ring.records.len() > MAX_RECORDS_PER_RUN || ring.total_bytes > MAX_BYTES_PER_RUN {
            if let Some(front) = ring.records.pop_front() {
                ring.total_bytes = ring.total_bytes.saturating_sub(front.byte_len);
            }
        }
        fanout(ring, frame);
    }
}

const fn encoded_sse_frame_bytes(frame: &SseFrame) -> usize {
    // Wire estimate: `id:` + id + `\n` + `event:` + event + `\n` + `data:` + data + `\n\n`
    frame.id.len() + frame.event.len() + frame.data.len() + 16
}

/// Fan a frame out to every live subscriber.
///
/// A subscriber that has fallen behind (data-frame or pending-byte cap) is
/// evicted with an explicit terminal lag gap (QC2 F-005). The gap is written
/// into the channel's reserved control slot, so it is deliverable even when
/// the data queue is full; the subscriber entry is then removed, closing the
/// channel after the gap.
fn fanout(ring: &mut RunRing, frame: &SseFrame) {
    let byte_len = encoded_sse_frame_bytes(frame);
    let lag_sequence = ring.next_sequence.saturating_sub(1);
    let mut remove = Vec::new();
    for (id, sub) in &mut ring.subscribers {
        if sub.pending_frames >= MAX_PENDING_FRAMES_PER_SUB
            || sub.pending_bytes.saturating_add(byte_len) > MAX_PENDING_BYTES_PER_SUB
        {
            let lag = lag_gap_frame(&ring.run_id, ring.epoch, lag_sequence);
            let lag_bytes = encoded_sse_frame_bytes(&lag);
            let _ = sub.tx.try_send(QueuedFrame {
                frame: lag,
                wire_bytes: lag_bytes,
            });
            remove.push(*id);
            continue;
        }
        let queued = QueuedFrame {
            frame: frame.clone(),
            wire_bytes: byte_len,
        };
        match sub.tx.try_send(queued) {
            Ok(()) => {
                sub.pending_frames += 1;
                sub.pending_bytes += byte_len;
            }
            Err(mpsc::error::TrySendError::Full(_)) => {
                let lag = lag_gap_frame(&ring.run_id, ring.epoch, lag_sequence);
                let lag_bytes = encoded_sse_frame_bytes(&lag);
                let _ = sub.tx.try_send(QueuedFrame {
                    frame: lag,
                    wire_bytes: lag_bytes,
                });
                remove.push(*id);
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                remove.push(*id);
            }
        }
    }
    for id in remove {
        ring.subscribers.remove(&id);
    }
}

fn gap_wire_from_frame(frame: &SseFrame) -> Option<GapWire> {
    if frame.event != "gap" {
        return None;
    }
    serde_json::from_str(&frame.data).ok()
}

fn range_covered_by_explicit_gap(ring: &RunRing, from: u64, to: u64) -> bool {
    if from > to {
        return true;
    }
    ring.records.iter().any(|rec| {
        gap_wire_from_frame(&rec.frame)
            .is_some_and(|gap| gap.from_sequence <= from && gap.to_sequence >= to)
    })
}

fn collect_from(ring: &RunRing, run_id: &str, after: Option<u64>) -> Vec<SseFrame> {
    let mut out = Vec::new();
    let first_retained = ring.records.front().map_or(1, |r| r.sequence);
    let after_seq = after.unwrap_or(0);
    if after_seq == 0 && first_retained > 1 {
        if !range_covered_by_explicit_gap(ring, 1, first_retained - 1) {
            out.push(gap_frame(run_id, ring.epoch, 1, first_retained - 1));
        }
    } else if after_seq > 0
        && after_seq < first_retained
        && !range_covered_by_explicit_gap(ring, after_seq + 1, first_retained - 1)
    {
        out.push(gap_frame(
            run_id,
            ring.epoch,
            after_seq + 1,
            first_retained - 1,
        ));
    }
    for rec in &ring.records {
        if rec.sequence > after_seq {
            out.push(rec.frame.clone());
        }
    }
    out
}

fn gap_frame(run_id: &str, epoch: Uuid, from: u64, to: u64) -> SseFrame {
    let gap = GapWire {
        run_id: run_id.to_string(),
        epoch,
        from_sequence: from,
        to_sequence: to,
    };
    let sequence = to.max(from);
    SseFrame {
        id: format!("{epoch}:{sequence}"),
        event: "gap".to_string(),
        data: serde_json::to_string(&gap).unwrap_or_default(),
    }
}

fn lag_gap_frame(run_id: &str, epoch: Uuid, sequence: u64) -> SseFrame {
    gap_frame(run_id, epoch, sequence, sequence)
}

fn parse_cursor(cursor: &str) -> Result<(Uuid, u64), SubscribeError> {
    let Some((epoch, seq)) = cursor.rsplit_once(':') else {
        return Err(SubscribeError::MalformedCursor);
    };
    let epoch = Uuid::parse_str(epoch).map_err(|_| SubscribeError::MalformedCursor)?;
    let seq = seq.parse().map_err(|_| SubscribeError::MalformedCursor)?;
    Ok((epoch, seq))
}

fn current_epoch() -> Uuid {
    Uuid::new_v4()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_orchestration::engine::{SessionId, SessionStatus};

    fn mk_record(run_id: &str, status: SessionStatus, revision: u64) -> RunRecord {
        RunRecord {
            session_id: SessionId(run_id.into()),
            status,
            state_revision: revision,
            execution_version: 1,
            descriptor: None,
            state: None,
            graph_version: 1,
        }
    }

    fn frame_sequence(frame: &SseFrame) -> u64 {
        frame
            .id
            .rsplit(':')
            .next()
            .expect("cursor id")
            .parse()
            .expect("numeric sequence")
    }

    fn assert_monotonic_no_duplicates(frames: &[SseFrame]) {
        let mut last = 0u64;
        let mut seen = std::collections::HashSet::new();
        for frame in frames {
            let seq = frame_sequence(frame);
            assert!(seq > last, "sequences must increase: {seq} after {last}");
            assert!(seen.insert(frame.id.clone()), "duplicate id {}", frame.id);
            last = seq;
        }
    }

    #[test]
    fn gap_cursor_is_numeric_sequence() {
        let epoch = Uuid::parse_str("00000000-0000-0000-0000-00000000002a").expect("epoch");
        let frame = gap_frame("run-1", epoch, 3, 7);
        assert!(frame.id.starts_with(&format!("{epoch}:")));
        let seq = frame.id.rsplit(':').next().unwrap();
        assert!(seq.parse::<u64>().is_ok());
        assert_ne!(seq, "gap");
    }

    #[test]
    fn pre_restart_cursor_yields_history_unavailable_after_registry_restart() {
        let before = RunEventRegistry::new();
        let _sink = before.try_register_live("run-1").expect("register");
        before.publish_run_state("run-1", &mk_record("run-1", SessionStatus::Running, 1));
        let pre_restart_cursor = {
            let state = before
                .inner
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let ring = state.live.get("run-1").expect("ring");
            format!("{}:1", ring.epoch)
        };

        let after = RunEventRegistry::new();
        let _sink = after
            .try_register_live("run-1")
            .expect("re-register after restart");
        after.publish_run_state("run-1", &mk_record("run-1", SessionStatus::Running, 2));

        let result = after.subscribe_live("run-1", Some(&pre_restart_cursor), "/inspect".into());
        let Err(err) = result else {
            panic!("pre-restart cursor must not subscribe live");
        };
        let SubscribeError::HistoryUnavailable(wire) = err else {
            panic!("pre-restart cursor must map to history_unavailable, not invalid_cursor or silent skip");
        };
        assert_eq!(wire.run_id, "run-1");
        assert_eq!(wire.inspect_url, "/inspect");
    }

    /// Nested prompt steps run under child session ids; the root must be
    /// recoverable at any depth so their lifecycle events land on the driven
    /// root run's single ring.
    #[test]
    fn root_run_id_strips_every_child_level() {
        assert_eq!(root_run_id("root-run"), "root-run");
        assert_eq!(root_run_id("root-run:child:a"), "root-run");
        assert_eq!(root_run_id("root-run:child:a:child:b"), "root-run");
    }

    /// A descendant prompt resolves to the OWNING root ring; an unknown run
    /// resolves to nothing (never a wrong ring).
    #[tokio::test]
    async fn sink_for_run_resolves_descendant_to_owning_root_ring() {
        let registry = RunEventRegistry::new();
        let sink = registry.try_register_live("root-run").expect("register");
        let mut map = std::collections::HashMap::new();
        map.insert("root-run".to_string(), sink);
        let sinks: RunEventSinkMap = Arc::new(tokio::sync::Mutex::new(map));

        let exact = sink_for_run(&sinks, "root-run").await.expect("exact match");
        assert_eq!(exact.run_id(), "root-run");

        let nested = sink_for_run(&sinks, "root-run:child:a:child:b")
            .await
            .expect("descendant resolves to the owning root ring");
        assert_eq!(nested.run_id(), "root-run");

        assert!(
            sink_for_run(&sinks, "unrelated-run").await.is_none(),
            "an unowned run must not borrow another run's ring"
        );
    }

    #[test]
    fn encoded_frame_cap_counts_wire_overhead() {
        let frame = SseFrame {
            id: "00000000-0000-0000-0000-000000000001:1".into(),
            event: "host_event".into(),
            data: "x".repeat(MAX_FRAME_BYTES),
        };
        assert!(encoded_sse_frame_bytes(&frame) > MAX_FRAME_BYTES);
    }

    #[tokio::test]
    async fn live_subscriber_receives_post_replay_events() {
        let registry = RunEventRegistry::new();
        let _sink = registry.try_register_live("run-1").expect("register");
        let mut sub = registry
            .subscribe_live("run-1", None, "/inspect".into())
            .expect("subscribe");
        registry.publish_run_state("run-1", &mk_record("run-1", SessionStatus::Running, 1));
        let frame = tokio::time::timeout(std::time::Duration::from_secs(1), sub.recv())
            .await
            .expect("timeout")
            .expect("frame");
        assert_eq!(frame.event, "run_state");
    }

    #[tokio::test]
    async fn subscriber_permit_released_on_drop() {
        let registry = RunEventRegistry::new();
        let _sink = registry.try_register_live("run-1").expect("register");
        let sub = registry
            .subscribe_live("run-1", None, "/inspect".into())
            .expect("subscribe");
        assert_eq!(registry.live_subscriber_count("run-1"), 1);
        drop(sub);
        assert_eq!(registry.live_subscriber_count("run-1"), 0);
    }

    #[tokio::test]
    async fn draining_subscriber_survives_many_events() {
        let registry = RunEventRegistry::new();
        let _sink = registry.try_register_live("run-1").expect("register");
        let mut sub = registry
            .subscribe_live("run-1", None, "/inspect".into())
            .expect("subscribe");
        let mut frames = Vec::with_capacity(32);
        for i in 0_u64..32 {
            registry.publish_run_state("run-1", &mk_record("run-1", SessionStatus::Running, i + 1));
            let frame = tokio::time::timeout(std::time::Duration::from_secs(1), sub.recv())
                .await
                .expect("timed out waiting for live frame while publishing")
                .expect("subscriber closed while draining");
            assert_eq!(frame.event, "run_state");
            frames.push(frame);
        }
        assert_eq!(frames.len(), 32);
        assert_monotonic_no_duplicates(&frames);
        assert_eq!(registry.live_subscriber_count("run-1"), 1);
    }

    #[tokio::test]
    async fn replay_dedupes_with_last_event_id() {
        let registry = RunEventRegistry::new();
        let _sink = registry.try_register_live("run-1").expect("register");
        registry.publish_run_state("run-1", &mk_record("run-1", SessionStatus::Running, 1));
        registry.publish_run_state("run-1", &mk_record("run-1", SessionStatus::Running, 2));
        let mut first = registry
            .subscribe_live("run-1", None, "/inspect".into())
            .expect("subscribe");
        let f1 = first.recv().await.expect("first");
        let f2 = first.recv().await.expect("second");
        assert_monotonic_no_duplicates(&[f1.clone(), f2.clone()]);
        drop(first);
        let mut second = registry
            .subscribe_live("run-1", Some(&f1.id), "/inspect".into())
            .expect("reconnect");
        let only = tokio::time::timeout(std::time::Duration::from_secs(1), second.recv())
            .await
            .expect("replay handoff must not hang")
            .expect("replay tail frame");
        assert_eq!(only.id, f2.id);
        assert_eq!(frame_sequence(&only), frame_sequence(&f2));
        let more = tokio::time::timeout(std::time::Duration::from_millis(200), second.recv()).await;
        assert!(
            more.is_err(),
            "must not wait for events already covered by the replay snapshot"
        );
    }

    #[tokio::test]
    async fn terminal_ring_replay_then_closes() {
        let registry = RunEventRegistry::new();
        let _sink = registry.try_register_live("run-1").expect("register");
        registry.publish_run_state("run-1", &mk_record("run-1", SessionStatus::Completed, 2));
        registry.mark_terminal("run-1");
        let mut sub = registry
            .subscribe_live("run-1", None, "/inspect".into())
            .expect("subscribe");
        let first = sub.recv().await.expect("run_state");
        assert_eq!(first.event, "run_state");
        assert!(sub.recv().await.is_none());
    }

    #[tokio::test]
    async fn post_terminal_publish_is_rejected() {
        let registry = RunEventRegistry::new();
        let sink = registry.try_register_live("run-1").expect("register");
        registry.publish_run_state("run-1", &mk_record("run-1", SessionStatus::Completed, 1));
        registry.mark_terminal("run-1");
        sink.publish_host_event(
            "step",
            "attempt",
            &nexus_agent_host::capability::model::HostEvent::MessageDelta(
                nexus_agent_host::capability::model::TextDeltaEvent {
                    session_id: nexus_agent_host::HostSessionId::new(),
                    op_id: nexus_agent_host::HostOperationId::new(),
                    text: "late".into(),
                },
            ),
        );
        let mut sub = registry
            .subscribe_live("run-1", None, "/inspect".into())
            .expect("subscribe");
        let only = sub.recv().await.expect("run_state");
        assert_eq!(only.event, "run_state");
        assert!(sub.recv().await.is_none());
    }

    #[tokio::test]
    async fn subscriber_limit_returns_too_many_subscribers() {
        let registry = RunEventRegistry::new();
        let _sink = registry.try_register_live("run-1").expect("register");
        let mut subs = Vec::new();
        for _ in 0..MAX_SUBSCRIBERS_PER_RUN {
            subs.push(
                registry
                    .subscribe_live("run-1", None, "/inspect".into())
                    .expect("subscriber permit"),
            );
        }
        assert!(matches!(
            registry.subscribe_live("run-1", None, "/inspect".into()),
            Err(SubscribeError::TooManySubscribers)
        ));
        drop(subs);
    }

    #[tokio::test]
    async fn replay_exceeds_pending_cap_replays_without_subscriber_error() {
        let registry = RunEventRegistry::new();
        let _sink = registry.try_register_live("run-1").expect("register");
        for i in 0_u64..20 {
            registry.publish_run_state("run-1", &mk_record("run-1", SessionStatus::Running, i + 1));
        }
        let mut sub = registry
            .subscribe_live("run-1", None, "/inspect".into())
            .expect("subscribe must not fail when replay exceeds pending cap");
        let mut frames = Vec::with_capacity(20);
        for _ in 0..20 {
            let frame = tokio::time::timeout(std::time::Duration::from_secs(1), sub.recv())
                .await
                .expect("replay frame timeout")
                .expect("replay frame");
            frames.push(frame);
        }
        assert_eq!(frames.len(), 20);
        assert_monotonic_no_duplicates(&frames);
    }

    #[tokio::test]
    async fn replay_after_eviction_emits_explicit_gap_for_skipped_cursor() {
        let registry = RunEventRegistry::new();
        let _sink = registry.try_register_live("run-1").expect("register");
        for i in 0..(MAX_RECORDS_PER_RUN + 5) {
            registry
                .inner
                .append_json("run-1", "host_event", &serde_json::json!({ "n": i }));
        }
        let (first_retained, cursor_epoch) = registry
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .live
            .get("run-1")
            .map(|ring| (ring.records.front().expect("retained").sequence, ring.epoch))
            .expect("ring");
        assert!(first_retained > 1, "ring must have evicted early sequences");
        let cursor = format!("{}:{}", cursor_epoch, 1);
        let mut sub = registry
            .subscribe_live("run-1", Some(&cursor), "/inspect".into())
            .expect("reconnect");
        let gap = tokio::time::timeout(std::time::Duration::from_secs(1), sub.recv())
            .await
            .expect("gap frame timeout")
            .expect("gap frame");
        assert_eq!(gap.event, "gap");
        let payload: serde_json::Value = serde_json::from_str(&gap.data).expect("gap json");
        assert_eq!(payload["from_sequence"].as_u64(), Some(2));
        assert_eq!(payload["to_sequence"].as_u64(), Some(first_retained - 1));
    }

    #[tokio::test]
    async fn oversize_skipped_sequence_emits_explicit_gap_on_subscribe() {
        let registry = RunEventRegistry::new();
        let _sink = registry.try_register_live("run-1").expect("register");
        registry.inner.append_json(
            "run-1",
            "host_event",
            &serde_json::json!({ "blob": "x".repeat(MAX_FRAME_BYTES) }),
        );
        registry.publish_run_state("run-1", &mk_record("run-1", SessionStatus::Running, 1));
        let mut sub = registry
            .subscribe_live("run-1", None, "/inspect".into())
            .expect("subscribe");
        let gap = tokio::time::timeout(std::time::Duration::from_secs(1), sub.recv())
            .await
            .expect("gap frame timeout")
            .expect("gap frame");
        assert_eq!(gap.event, "gap");
        assert_eq!(
            frame_sequence(&gap),
            2,
            "gap keeps its own monotonic sequence id"
        );
        let payload: serde_json::Value = serde_json::from_str(&gap.data).expect("gap json");
        assert_eq!(payload["from_sequence"].as_u64(), Some(1));
        assert_eq!(payload["to_sequence"].as_u64(), Some(1));
        let run_state = tokio::time::timeout(std::time::Duration::from_secs(1), sub.recv())
            .await
            .expect("run_state timeout")
            .expect("run_state");
        assert_eq!(run_state.event, "run_state");
        assert_eq!(frame_sequence(&run_state), 3);
        let mut replay = registry
            .subscribe_live("run-1", Some(&gap.id), "/inspect".into())
            .expect("reconnect after gap cursor");
        let tail = tokio::time::timeout(std::time::Duration::from_secs(1), replay.recv())
            .await
            .expect("strict-later replay timeout")
            .expect("strict-later replay");
        assert_eq!(tail.event, "run_state");
        assert_eq!(frame_sequence(&tail), 3);
        let more = tokio::time::timeout(std::time::Duration::from_millis(200), replay.recv()).await;
        assert!(more.is_err(), "cursor at gap must not replay gap again");
    }

    #[tokio::test]
    async fn oversize_payload_emits_gap_and_respects_ring_cap() {
        let registry = RunEventRegistry::new();
        let _sink = registry.try_register_live("run-1").expect("register");
        let huge = "x".repeat(MAX_FRAME_BYTES);
        for _ in 0..300 {
            registry
                .inner
                .append_json("run-1", "host_event", &serde_json::json!({ "blob": huge }));
        }
        let state = registry
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let ring = state.live.get("run-1").expect("ring");
        assert!(ring.records.len() <= MAX_RECORDS_PER_RUN);
        assert!(ring.total_bytes <= MAX_BYTES_PER_RUN);
        assert!(ring.records.iter().any(|r| r.frame.event == "gap"));
        drop(state);
    }
    #[tokio::test]
    async fn dequeue_releases_wire_byte_reservation_not_payload_only() {
        let registry = RunEventRegistry::new();
        let _sink = registry.try_register_live("run-1").expect("register");
        let mut sub = registry
            .subscribe_live("run-1", None, "/inspect".into())
            .expect("subscribe");
        let subscriber_id = sub.subscriber_id;
        registry.publish_run_state("run-1", &mk_record("run-1", SessionStatus::Running, 1));
        let (frames_before, bytes_before) = registry.subscriber_pending("run-1", subscriber_id);
        assert!(frames_before > 0, "publish must reserve pending frames");
        assert!(bytes_before > 0, "publish must reserve pending bytes");
        let frame = tokio::time::timeout(std::time::Duration::from_secs(1), sub.recv())
            .await
            .expect("frame timeout")
            .expect("frame");
        assert_eq!(frame.event, "run_state");
        let (frames_after, bytes_after) = registry.subscriber_pending("run-1", subscriber_id);
        assert_eq!(frames_after, 0, "dequeue must release frame reservation");
        assert_eq!(
            bytes_after, 0,
            "dequeue must release full wire-byte reservation"
        );
    }

    #[tokio::test]
    async fn draining_subscriber_never_false_gaps_under_wire_overhead() {
        let registry = RunEventRegistry::new();
        let _sink = registry.try_register_live("run-1").expect("register");
        let mut sub = registry
            .subscribe_live("run-1", None, "/inspect".into())
            .expect("subscribe");
        let subscriber_id = sub.subscriber_id;
        for i in 0_u64..512 {
            registry.publish_run_state("run-1", &mk_record("run-1", SessionStatus::Running, i + 1));
            let frame = tokio::time::timeout(std::time::Duration::from_secs(1), sub.recv())
                .await
                .expect("timed out while draining")
                .expect("subscriber closed while draining");
            assert_ne!(
                frame.event, "gap",
                "draining subscriber must not false-gap at frame {i}"
            );
            let (_, bytes_after) = registry.subscriber_pending("run-1", subscriber_id);
            assert_eq!(
                bytes_after, 0,
                "pending bytes must not leak wire overhead at frame {i}"
            );
        }
        assert_eq!(registry.live_subscriber_count("run-1"), 1);
    }

    /// QC2 F-005: a lagging subscriber must ALWAYS receive the explicit lag
    /// gap, even when its data channel is completely full — the gap is
    /// parked in the channel's reserved control slot. The reader drains the
    /// buffered frames, reads the gap, then observes a clean close; a silent
    /// EOF (or a hang) fails this test.
    #[tokio::test]
    async fn lag_gap_is_delivered_even_when_data_channel_is_full() {
        let registry = RunEventRegistry::new();
        let _sink = registry.try_register_live("run-1").expect("register");
        let mut sub = registry
            .subscribe_live("run-1", None, "/inspect".into())
            .expect("subscribe");
        // Never drain while the ring fans out: the data queue fills to its
        // cap and the subscriber is evicted as lagging.
        for i in 0..=MAX_PENDING_FRAMES_PER_SUB {
            registry.publish_run_state(
                "run-1",
                &mk_record("run-1", SessionStatus::Running, i as u64 + 1),
            );
        }
        assert_eq!(
            registry.live_subscriber_count("run-1"),
            0,
            "lagging subscriber must be evicted once the pending cap is exceeded"
        );
        // Drain buffered data frames until the explicit gap arrives.
        let mut drained = 0usize;
        loop {
            match tokio::time::timeout(std::time::Duration::from_secs(1), sub.recv()).await {
                Ok(Some(frame)) if frame.event == "gap" => break,
                Ok(Some(frame)) => {
                    assert_eq!(frame.event, "run_state");
                    drained += 1;
                    assert!(
                        drained <= MAX_PENDING_FRAMES_PER_SUB,
                        "gap must arrive within the data cap"
                    );
                }
                Ok(None) => panic!("channel closed with silent EOF before the lag gap"),
                Err(error) => panic!("timed out waiting for the explicit lag gap: {error}"),
            }
        }
        // After the gap the evicted subscriber's channel closes.
        let closed = tokio::time::timeout(std::time::Duration::from_secs(1), sub.recv()).await;
        assert!(
            matches!(closed, Ok(None)),
            "the lag gap must be followed by a clean close, got {closed:?}"
        );
    }

    /// QC2 F-005 (control): a slow-but-draining subscriber must never be
    /// evicted — eviction is for readers that actually stop consuming.
    #[tokio::test]
    async fn draining_subscriber_is_never_evicted_with_reserved_control_slot() {
        let registry = RunEventRegistry::new();
        let _sink = registry.try_register_live("run-1").expect("register");
        let mut sub = registry
            .subscribe_live("run-1", None, "/inspect".into())
            .expect("subscribe");
        for i in 0..(MAX_PENDING_FRAMES_PER_SUB * 4) {
            registry.publish_run_state(
                "run-1",
                &mk_record("run-1", SessionStatus::Running, i as u64 + 1),
            );
            let frame = tokio::time::timeout(std::time::Duration::from_secs(1), sub.recv())
                .await
                .expect("draining recv timed out")
                .expect("draining subscriber must stay open");
            assert_eq!(frame.event, "run_state");
        }
        assert_eq!(registry.live_subscriber_count("run-1"), 1);
    }
}
