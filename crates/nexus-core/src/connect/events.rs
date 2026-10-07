//! Bounded event retention and ack-gated Connect subscriptions.
use std::collections::{HashMap, VecDeque};
use std::future::Future as _;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{Notify, Semaphore};
use uuid::Uuid;

const MAX_EVENTS: usize = 1024;
// Ring histories persist across session close for reconnect replay. Once full,
// publish and subscribe evict an inactive ring; active rings are never evicted.
const MAX_STREAMS: usize = 4096;
const MAX_BATCH: usize = 64;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct EventFrame {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub event: String,
    pub data: Value,
}

#[derive(Clone, Debug)]
struct StoredEvent {
    sequence: u64,
    event: String,
    data: Value,
}

struct StreamRing {
    epoch: Uuid,
    next_sequence: u64,
    events: VecDeque<StoredEvent>,
}

/// Per-(session, stream) registration. `active` gates delivery until the
/// subscribe response write is observed; `changed` wakes on new frames and
/// activation; `cancelled`/`cancelled_flag` interrupt an in-flight delivery,
/// ack wait or send-slot wait on replacement or session close. The
/// per-(session, stream) gate (`ConnectEventRegistry`'s `gates`) is held
/// from registration until activation so a second subscribe for the same
/// slot cannot register a replacement generation before the previous
/// generation's response write was observed.
struct Subscriber {
    active: Arc<AtomicBool>,
    changed: Arc<Notify>,
    cancelled: Arc<Notify>,
    cancelled_flag: Arc<AtomicBool>,
}

impl Subscriber {
    fn cancel(&self) {
        self.cancelled_flag.store(true, Ordering::Release);
        self.cancelled.notify_one();
    }
}

struct AdmissionGate {
    semaphore: Arc<Semaphore>,
    users: AtomicUsize,
}

impl Default for AdmissionGate {
    fn default() -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(1)),
            users: AtomicUsize::new(0),
        }
    }
}

/// A subscribe call's reference to its gate. Failed or cancelled admissions
/// remove unregistered gates once no competing waiter can still use them.
struct AdmissionGateUse {
    registry: ConnectEventRegistry,
    key: (String, String),
    gate: Arc<AdmissionGate>,
}

impl Drop for AdmissionGateUse {
    fn drop(&mut self) {
        let mut state = self
            .registry
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if self.gate.users.fetch_sub(1, Ordering::Relaxed) == 1 {
            remove_idle_gate(&mut state, &self.key, &self.gate);
        }
    }
}

#[derive(Default)]
struct RegistryState {
    streams: HashMap<String, StreamRing>,
    subscribers: HashMap<(String, String), Subscriber>,
    gates: HashMap<(String, String), Arc<AdmissionGate>>,
    subscriber_counts: HashMap<String, usize>,
    sessions: HashMap<String, Arc<AtomicBool>>,
    #[cfg(test)]
    gate_waiter: Option<Arc<Notify>>,
}

fn decrement_subscriber_count(state: &mut RegistryState, stream: &str) {
    let remove_count = if let Some(count) = state.subscriber_counts.get_mut(stream) {
        *count -= 1;
        *count == 0
    } else {
        false
    };
    if remove_count {
        state.subscriber_counts.remove(stream);
    }
}

fn remove_idle_gate(state: &mut RegistryState, key: &(String, String), gate: &Arc<AdmissionGate>) {
    if !state.subscribers.contains_key(key)
        && state.gates.get(key).is_some_and(|current| {
            Arc::ptr_eq(current, gate) && current.users.load(Ordering::Relaxed) == 0
        })
    {
        state.gates.remove(key);
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SubscribeError {
    InvalidCursor,
    UnknownEpoch,
    FutureCursor,
    ClosedSession,
    StreamLimit,
}

#[derive(Clone, Default)]
pub struct ConnectEventRegistry(Arc<Mutex<RegistryState>>);

static CONNECT_EVENT_REGISTRY: LazyLock<ConnectEventRegistry> =
    LazyLock::new(ConnectEventRegistry::default);

/// Shared event registry used by Connect publishers and accepted sessions.
#[must_use]
pub fn connect_event_registry() -> &'static ConnectEventRegistry {
    &CONNECT_EVENT_REGISTRY
}

/// An atomic replay cut followed by a live tail. `ack` advances the cursor
/// only after the reverse invocation for the preceding batch succeeds.
pub struct EventSubscription {
    pub replay: Vec<EventFrame>,
    registry: ConnectEventRegistry,
    session: String,
    stream: String,
    cursor: u64,
    sent_through: Option<u64>,
    acked: Arc<Semaphore>,
    active: Arc<AtomicBool>,
    changed: Arc<Notify>,
    cancelled: Arc<Notify>,
    cancelled_flag: Arc<AtomicBool>,
}

impl EventSubscription {
    /// True while this subscription is still the live registration for its
    /// (session, stream) — false once replaced or the session is closed.
    #[must_use]
    pub fn is_alive(&self) -> bool {
        let state = self
            .registry
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state
            .subscribers
            .get(&(self.session.clone(), self.stream.clone()))
            .is_some_and(|subscriber| Arc::ptr_eq(&subscriber.active, &self.active))
    }

    /// True once this generation was cancelled (replaced or session close),
    /// independent of the registry lookup.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled_flag.load(Ordering::Acquire) || !self.is_alive()
    }

    /// Wakeup fired when this generation is cancelled. Delivery code selects
    /// on it to interrupt ack waits, send-slot waits and reverse invokes.
    #[must_use]
    pub fn cancellation(&self) -> Arc<Notify> {
        Arc::clone(&self.cancelled)
    }

    /// The next bounded, contiguous batch, or an empty vector once the
    /// subscription has been cancelled (replaced, or its session closed).
    pub async fn next_batch(&mut self) -> Vec<EventFrame> {
        if self.sent_through.is_some() && !self.wait_for_ack().await {
            return Vec::new();
        }
        let cancelled = Arc::clone(&self.cancelled);
        let changed = Arc::clone(&self.changed);
        loop {
            if self.is_cancelled() {
                return Vec::new();
            }
            if self.active.load(Ordering::Acquire) {
                if !self.replay.is_empty() {
                    let frames = std::mem::take(&mut self.replay);
                    self.sent_through = last_sequence(&frames);
                    return frames;
                }
                let frames = self.live_page();
                if !frames.is_empty() {
                    self.sent_through = last_sequence(&frames);
                    return frames;
                }
            }
            tokio::select! {
                biased;
                () = cancelled.notified() => {}
                () = changed.notified() => {}
            }
        }
    }

    /// Await the ack for the outstanding batch, or cancellation.
    async fn wait_for_ack(&mut self) -> bool {
        let acked = Arc::clone(&self.acked);
        let cancelled = Arc::clone(&self.cancelled);
        loop {
            if self.is_cancelled() {
                return false;
            }
            tokio::select! {
                biased;
                () = cancelled.notified() => {}
                Ok(permit) = acked.acquire() => {
                    permit.forget();
                    self.sent_through = None;
                    return true;
                }
            }
        }
    }

    fn live_page(&self) -> Vec<EventFrame> {
        let state = self
            .registry
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state
            .streams
            .get(&self.stream)
            .map_or_else(Vec::new, |ring| {
                ring.events
                    .iter()
                    .filter(|event| event.sequence > self.cursor)
                    .take(MAX_BATCH)
                    .map(|event| EventFrame {
                        id: Some(format!("{}:{}", ring.epoch, event.sequence)),
                        event: event.event.clone(),
                        data: event.data.clone(),
                    })
                    .collect()
            })
    }

    /// Advance the read cursor for the outstanding batch. A cancellation
    /// never advances it: `ack` is a no-op once the batch was dropped.
    pub fn ack(&mut self) {
        if let Some(sequence) = self.sent_through {
            self.cursor = sequence;
            self.acked.add_permits(1);
        }
    }

    pub fn control(event: impl Into<String>, data: Value) -> EventFrame {
        EventFrame {
            id: None,
            event: event.into(),
            data,
        }
    }

    pub fn unregister(&self, session: &str) {
        let key = (session.to_owned(), self.stream.clone());
        let mut state = self
            .registry
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state
            .subscribers
            .get(&key)
            .is_some_and(|subscriber| Arc::ptr_eq(&subscriber.active, &self.active))
        {
            state.subscribers.remove(&key);
            decrement_subscriber_count(&mut state, &key.1);
            if let Some(gate) = state.gates.get(&key).cloned() {
                remove_idle_gate(&mut state, &key, &gate);
            }
        }
    }
}

fn last_sequence(frames: &[EventFrame]) -> Option<u64> {
    frames
        .last()?
        .id
        .as_deref()?
        .rsplit_once(':')?
        .1
        .parse()
        .ok()
}

/// Validate a `<UUID epoch>:<decimal sequence>` cursor against `ring`.
fn parse_cursor(ring: &StreamRing, cursor: &str) -> Result<u64, SubscribeError> {
    let (epoch, sequence) = cursor
        .rsplit_once(':')
        .ok_or(SubscribeError::InvalidCursor)?;
    if epoch != ring.epoch.to_string() {
        return Err(SubscribeError::UnknownEpoch);
    }
    let sequence = sequence
        .parse::<u64>()
        .map_err(|_| SubscribeError::InvalidCursor)?;
    if sequence >= ring.next_sequence {
        return Err(SubscribeError::FutureCursor);
    }
    Ok(sequence)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublishError {
    StreamLimit,
}

fn make_room_for_stream(state: &mut RegistryState, stream: &str) -> bool {
    if state.streams.contains_key(stream) || state.streams.len() < MAX_STREAMS {
        return true;
    }
    let evictable = state
        .streams
        .keys()
        .find(|candidate| !state.subscriber_counts.contains_key(candidate.as_str()))
        .cloned();
    evictable.is_some_and(|key| state.streams.remove(&key).is_some())
}

impl ConnectEventRegistry {
    /// Track the lifetime of an accepted transport session without retaining
    /// closed-session tombstones.
    pub fn open_session(&self, session: &str) {
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state
            .sessions
            .insert(session.to_owned(), Arc::new(AtomicBool::new(false)));
    }
    /// Publish retains at most `MAX_STREAMS` rings, evicting an inactive ring
    /// when at capacity; active rings are never evicted.
    pub fn publish(
        &self,
        stream: &str,
        event: &str,
        data: Value,
    ) -> Result<EventFrame, PublishError> {
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !make_room_for_stream(&mut state, stream) {
            return Err(PublishError::StreamLimit);
        }
        let ring = state
            .streams
            .entry(stream.to_owned())
            .or_insert_with(|| StreamRing {
                epoch: Uuid::new_v4(),
                next_sequence: 1,
                events: VecDeque::new(),
            });
        let sequence = ring.next_sequence;
        ring.next_sequence = ring.next_sequence.saturating_add(1);
        ring.events.push_back(StoredEvent {
            sequence,
            event: event.to_owned(),
            data: data.clone(),
        });
        while ring.events.len() > MAX_EVENTS {
            ring.events.pop_front();
        }
        let epoch = ring.epoch;
        for ((_, subscribed_stream), subscriber) in &state.subscribers {
            if subscribed_stream == stream {
                subscriber.changed.notify_one();
            }
        }
        drop(state);
        Ok(EventFrame {
            id: Some(format!("{epoch}:{sequence}")),
            event: event.to_owned(),
            data,
        })
    }

    /// Atomically captures the replay page and registers its live tail.
    ///
    /// Registration is serialized per (session, stream): a subscribe only
    /// proceeds once the previous subscription's response write has been
    /// observed (`activate` releases its gate permit). Because of that, the
    /// subscriber currently occupying a stream slot is exactly the
    /// generation whose response write is still outstanding, so `activate`
    /// can never enable a replacement generation early (§A.2a(c)/(f)5). A
    /// pre-existing activated subscription for the same slot is replaced and
    /// its delivery cancelled.
    ///
    /// # Errors
    /// Returns [`SubscribeError::InvalidCursor`] when `cursor` is malformed,
    /// [`SubscribeError::UnknownEpoch`] when it names a foreign ring epoch,
    /// [`SubscribeError::FutureCursor`] when it is ahead of the ring,
    /// [`SubscribeError::ClosedSession`] when the owning session has closed,
    /// and [`SubscribeError::StreamLimit`] when every retained ring is active.
    pub async fn subscribe(
        &self,
        session: &str,
        stream: &str,
        cursor: Option<&str>,
    ) -> Result<(String, Option<String>, EventSubscription), SubscribeError> {
        let key = (session.to_owned(), stream.to_owned());
        let (gate, lifetime) = {
            let mut state = self
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let lifetime = state
                .sessions
                .get(session)
                .cloned()
                .filter(|lifetime| !lifetime.load(Ordering::Acquire))
                .ok_or(SubscribeError::ClosedSession)?;
            let gate = Arc::clone(
                state
                    .gates
                    .entry(key.clone())
                    .or_insert_with(|| Arc::new(AdmissionGate::default())),
            );
            gate.users.fetch_add(1, Ordering::Relaxed);
            (gate, lifetime)
        };
        let _gate_use = AdmissionGateUse {
            registry: self.clone(),
            key: key.clone(),
            gate: Arc::clone(&gate),
        };
        let acquire = Arc::clone(&gate.semaphore).acquire_owned();
        tokio::pin!(acquire);
        let permit = std::future::poll_fn(|cx| {
            let result = acquire.as_mut().poll(cx);
            #[cfg(test)]
            if result.is_pending() {
                let waiter = self
                    .0
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .gate_waiter
                    .take();
                if let Some(waiter) = waiter {
                    waiter.notify_one();
                }
            }
            result
        })
        .await
        .map_err(|_| SubscribeError::ClosedSession)?;
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if lifetime.load(Ordering::Acquire)
            || !state
                .sessions
                .get(session)
                .is_some_and(|current| Arc::ptr_eq(current, &lifetime))
        {
            return Err(SubscribeError::ClosedSession);
        }
        if !make_room_for_stream(&mut state, stream) {
            return Err(SubscribeError::StreamLimit);
        }
        permit.forget();
        let ring = state
            .streams
            .entry(stream.to_owned())
            .or_insert_with(|| StreamRing {
                epoch: Uuid::new_v4(),
                next_sequence: 1,
                events: VecDeque::new(),
            });
        let mut gap_reason = None;
        let sequence = match cursor {
            Some(cursor) => match parse_cursor(ring, cursor) {
                Ok(sequence) => {
                    if ring
                        .events
                        .front()
                        .is_some_and(|event| sequence < event.sequence.saturating_sub(1))
                    {
                        gap_reason = Some("history_unavailable");
                        0
                    } else {
                        sequence
                    }
                }
                Err(_) => {
                    gap_reason = Some("stale_cursor");
                    0
                }
            },
            None => 0,
        };
        let mut replay: Vec<EventFrame> = ring
            .events
            .iter()
            .filter(|event| event.sequence > sequence)
            .take(MAX_BATCH)
            .map(|event| EventFrame {
                id: Some(format!("{}:{}", ring.epoch, event.sequence)),
                event: event.event.clone(),
                data: event.data.clone(),
            })
            .collect();
        if let Some(reason) = gap_reason {
            replay.insert(
                0,
                EventSubscription::control(
                    "gap",
                    serde_json::json!({
                        "reason": reason,
                        "requires_transcript_reconciliation": true,
                        "operation_id": null,
                        "resync_required": true,
                        "inspect_url": ""
                    }),
                ),
            );
        }
        let epoch = ring.epoch.to_string();
        let active = Arc::new(AtomicBool::new(false));
        let changed = Arc::new(Notify::new());
        let cancelled = Arc::new(Notify::new());
        let cancelled_flag = Arc::new(AtomicBool::new(false));
        let subscriber = Subscriber {
            active: Arc::clone(&active),
            changed: Arc::clone(&changed),
            cancelled: Arc::clone(&cancelled),
            cancelled_flag: Arc::clone(&cancelled_flag),
        };
        if let Some(previous) = state.subscribers.insert(key, subscriber) {
            previous.cancel();
        } else {
            *state
                .subscriber_counts
                .entry(stream.to_owned())
                .or_default() += 1;
        }
        drop(state);
        Ok((
            epoch,
            cursor.map(str::to_owned),
            EventSubscription {
                replay,
                registry: self.clone(),
                session: session.to_owned(),
                stream: stream.to_owned(),
                cursor: sequence,
                sent_through: None,
                acked: Arc::new(Semaphore::new(0)),
                active,
                changed,
                cancelled,
                cancelled_flag,
            },
        ))
    }

    /// Enable delivery for the current (session, stream) generation and admit
    /// the next subscribe for that slot. Idempotent: a repeat observation of
    /// an already-activated generation changes nothing.
    pub fn activate(&self, session: &str, stream: &str) {
        let state = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let key = (session.to_owned(), stream.to_owned());
        let activated = state
            .subscribers
            .get(&key)
            .is_some_and(|subscriber| !subscriber.active.swap(true, Ordering::AcqRel));
        if activated {
            if let Some(subscriber) = state.subscribers.get(&key) {
                subscriber.changed.notify_one();
            }
            if let Some(gate) = state.gates.get(&key) {
                gate.semaphore.add_permits(1);
            }
        }
    }

    /// End every subscription owned by `session`.
    pub fn remove_session(&self, session: &str) {
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(lifetime) = state.sessions.remove(session) {
            lifetime.store(true, Ordering::Release);
        }
        let keys: Vec<_> = state
            .gates
            .keys()
            .filter(|(owner, _)| owner == session)
            .cloned()
            .collect();
        for key in &keys {
            if let Some(gate) = state.gates.remove(key) {
                gate.semaphore.close();
            }
        }
        let subscriber_keys: Vec<_> = state
            .subscribers
            .keys()
            .filter(|(owner, _)| owner == session)
            .cloned()
            .collect();
        for key in subscriber_keys {
            if let Some(subscriber) = state.subscribers.remove(&key) {
                subscriber.cancel();
                decrement_subscriber_count(&mut state, &key.1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::Duration;

    fn assert_generated_gap(frame: &EventFrame, expected_reason: &str) {
        assert_eq!(frame.event, "gap");
        assert_eq!(frame.id, None);
        assert_eq!(frame.data["reason"], expected_reason);
        let gap: nexus_contracts::generated::core::core_connect_gap_event::CoreConnectGapEvent =
            serde_json::from_value(frame.data.clone()).expect("emitted gap matches generated DTO");
        assert!(gap.requires_transcript_reconciliation);
        assert!(gap.resync_required);
        assert_eq!(
            serde_json::to_value(gap).expect("generated gap serializes"),
            frame.data
        );
    }

    #[tokio::test]
    async fn replay_then_live_is_ack_gated() {
        let registry = ConnectEventRegistry::default();
        registry.open_session("p");
        let first = registry.publish("s", "one", json!(1)).unwrap();
        let (_, _, mut subscription) = registry.subscribe("p", "s", None).await.unwrap();
        registry.activate("p", "s");
        assert_eq!(subscription.replay, vec![first.clone()]);
        assert_eq!(subscription.next_batch().await, vec![first]);
        let next = registry.publish("s", "two", json!(2)).unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(10), subscription.next_batch())
                .await
                .is_err()
        );
        subscription.ack();
        assert_eq!(subscription.next_batch().await, vec![next]);
    }

    #[tokio::test]
    async fn replacement_cancels_the_previous_delivery() {
        let registry = ConnectEventRegistry::default();
        registry.open_session("p");
        let (_, _, mut first) = registry.subscribe("p", "s", None).await.unwrap();
        registry.activate("p", "s");
        let (_, _, mut second) = registry.subscribe("p", "s", None).await.unwrap();
        assert!(first.next_batch().await.is_empty());
        assert!(!first.is_alive());
        assert!(second.is_alive());
        let frame = registry.publish("s", "one", json!(1)).unwrap();
        registry.activate("p", "s");
        assert_eq!(second.next_batch().await, vec![frame]);
    }

    /// C2: a replacement must interrupt an outstanding (unacknowledged) batch
    /// so a cancelled generation cannot wedge the reader.
    #[tokio::test]
    async fn replacement_cancels_an_outstanding_unacked_batch() {
        let registry = ConnectEventRegistry::default();
        registry.open_session("p");
        registry.publish("s", "one", json!(1)).unwrap();
        let (_, _, mut first) = registry.subscribe("p", "s", None).await.unwrap();
        registry.activate("p", "s");
        // Take a batch, then withhold the ack: `next_batch` would otherwise
        // park on the ack semaphore forever.
        assert_eq!(first.next_batch().await.len(), 1);
        assert!(
            tokio::time::timeout(Duration::from_millis(10), first.next_batch())
                .await
                .is_err()
        );
        let (_, _, mut second) = registry.subscribe("p", "s", None).await.unwrap();
        assert!(
            tokio::time::timeout(Duration::from_millis(50), first.next_batch())
                .await
                .expect("cancellation must release the ack wait")
                .is_empty()
        );
        registry.activate("p", "s");
        // Drain `second`'s replay (the frame published before it registered),
        // then prove the new generation delivers fresh live frames.
        let replay = second.next_batch().await;
        assert_eq!(replay.len(), 1);
        second.ack();
        let frame = registry.publish("s", "two", json!(2)).unwrap();
        assert_eq!(second.next_batch().await, vec![frame]);
    }

    /// C2: session close must also interrupt an outstanding unacked batch.
    #[tokio::test]
    async fn session_close_cancels_an_outstanding_unacked_batch() {
        let registry = ConnectEventRegistry::default();
        registry.open_session("p");
        registry.publish("s", "one", json!(1)).unwrap();
        let (_, _, mut subscription) = registry.subscribe("p", "s", None).await.unwrap();
        registry.activate("p", "s");
        assert_eq!(subscription.next_batch().await.len(), 1);
        registry.remove_session("p");
        assert!(
            tokio::time::timeout(Duration::from_millis(50), subscription.next_batch())
                .await
                .expect("session close must release the ack wait")
                .is_empty()
        );
    }

    /// C1: a second subscribe for the same (session, stream) is admitted only
    /// after the first generation's response write is observed, so an older
    /// response can never enable the replacement.
    #[tokio::test]
    async fn same_stream_subscribe_serializes_on_activation() {
        let registry = ConnectEventRegistry::default();
        registry.open_session("p");
        let (_, _, first) = registry.subscribe("p", "s", None).await.unwrap();
        let mut replacement = tokio::spawn({
            let registry = registry.clone();
            async move { registry.subscribe("p", "s", None).await }
        });
        assert!(
            tokio::time::timeout(Duration::from_millis(20), &mut replacement)
                .await
                .is_err(),
            "the replacement must not register before the first response"
        );
        registry.activate("p", "s");
        let (_, _, second) = tokio::time::timeout(Duration::from_secs(1), &mut replacement)
            .await
            .expect("activation must admit the replacement")
            .expect("subscribe task must not panic")
            .expect("replacement subscribe succeeds");
        assert!(!first.is_alive());
        assert!(second.is_alive());
    }

    #[tokio::test]
    async fn cursor_shape_verbatim_echo_and_control_frames() {
        let registry = ConnectEventRegistry::default();
        registry.open_session("p");
        let frame = registry.publish("s", "one", json!({})).unwrap();
        let id = frame.id.unwrap();
        let (epoch, sequence) = id.rsplit_once(':').unwrap();
        assert!(!epoch.is_empty());
        assert_eq!(sequence, "1");
        assert!(sequence.parse::<u64>().is_ok());
        let (_, resumed_from, _) = registry.subscribe("p", "s", Some(&id)).await.unwrap();
        assert_eq!(resumed_from.as_deref(), Some(id.as_str()));
        assert_eq!(EventSubscription::control("gap", json!({})).id, None);
    }

    #[test]
    fn cursor_parser_preserves_typed_malformed_foreign_and_future_errors() {
        let registry = ConnectEventRegistry::default();
        registry.publish("s", "event", json!({})).unwrap();
        let state = registry
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let ring = state.streams.get("s").unwrap();
        assert_eq!(
            parse_cursor(ring, "malformed"),
            Err(SubscribeError::InvalidCursor)
        );
        assert_eq!(
            parse_cursor(ring, &format!("{}:1", Uuid::new_v4())),
            Err(SubscribeError::UnknownEpoch)
        );
        assert_eq!(
            parse_cursor(ring, &format!("{}:2", ring.epoch)),
            Err(SubscribeError::FutureCursor)
        );
    }
    #[tokio::test]
    async fn stale_cursor_and_trimmed_history_emit_cursorless_gap_first() {
        let registry = ConnectEventRegistry::default();
        registry.open_session("p");
        registry.publish("s", "one", json!(1)).unwrap();
        let (_, _, stale) = registry
            .subscribe("p", "s", Some("not-a-cursor"))
            .await
            .unwrap();
        assert_generated_gap(&stale.replay[0], "stale_cursor");

        registry.publish("foreign", "event", json!(1)).unwrap();
        let foreign_cursor = format!("{}:0", Uuid::new_v4());
        let (_, _, foreign) = registry
            .subscribe("p", "foreign", Some(&foreign_cursor))
            .await
            .unwrap();
        assert_generated_gap(&foreign.replay[0], "stale_cursor");

        let future_frame = registry.publish("future", "event", json!(1)).unwrap();
        let future_epoch = future_frame
            .id
            .unwrap()
            .split_once(':')
            .unwrap()
            .0
            .to_owned();
        let future_cursor = format!("{future_epoch}:2");
        let (_, _, future) = registry
            .subscribe("p", "future", Some(&future_cursor))
            .await
            .unwrap();
        assert_generated_gap(&future.replay[0], "stale_cursor");

        let oldest = registry
            .publish("trimmed", "event", json!(0))
            .unwrap()
            .id
            .unwrap();
        let epoch = oldest.split_once(':').unwrap().0;
        for n in 1..=MAX_EVENTS {
            registry.publish("trimmed", "event", json!(n)).unwrap();
        }
        let cursor = format!("{epoch}:0");
        let (_, _, trimmed) = registry
            .subscribe("p", "trimmed", Some(&cursor))
            .await
            .unwrap();
        assert_generated_gap(&trimmed.replay[0], "history_unavailable");
    }
    #[test]
    fn generated_gap_contract_rejects_false_literal_flags() {
        let valid = json!({
            "reason": "stale_cursor",
            "requires_transcript_reconciliation": true,
            "operation_id": null,
            "resync_required": true,
            "inspect_url": ""
        });
        let parse = |value| {
            serde_json::from_value::<
                nexus_contracts::generated::core::core_connect_gap_event::CoreConnectGapEvent,
            >(value)
        };
        let gap = parse(valid.clone()).unwrap();
        assert_eq!(serde_json::to_value(&gap).unwrap(), valid);
        for field in ["requires_transcript_reconciliation", "resync_required"] {
            let mut invalid = valid.clone();
            invalid[field] = json!(false);
            assert!(parse(invalid).is_err(), "false {field} must be rejected");
            let mut invalid = gap.clone();
            match field {
                "requires_transcript_reconciliation" => {
                    invalid.requires_transcript_reconciliation = false;
                }
                "resync_required" => invalid.resync_required = false,
                _ => unreachable!(),
            }
            assert!(
                serde_json::to_value(invalid).is_err(),
                "false {field} must not serialize"
            );
        }
    }

    #[tokio::test]
    async fn close_rejects_queued_admission_and_churn_keeps_streams_bounded() {
        let registry = ConnectEventRegistry::default();
        registry.open_session("p");
        let _a = registry.subscribe("p", "s", None).await.unwrap();
        let gate_waiter = Arc::new(Notify::new());
        registry
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .gate_waiter = Some(Arc::clone(&gate_waiter));
        let queued = tokio::spawn({
            let registry = registry.clone();
            async move { registry.subscribe("p", "s", None).await }
        });
        gate_waiter.notified().await;
        registry.remove_session("p");
        assert!(matches!(
            queued.await.unwrap(),
            Err(SubscribeError::ClosedSession)
        ));
        assert!(!registry
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .subscribers
            .contains_key(&("p".to_owned(), "s".to_owned())));
        assert!(matches!(
            registry.subscribe("p", "after-close", None).await,
            Err(SubscribeError::ClosedSession)
        ));
        assert!(!registry
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .gates
            .contains_key(&("p".to_owned(), "after-close".to_owned())));
        for n in 0..MAX_STREAMS + 10 {
            let session = format!("session-{n}");
            registry.open_session(&session);
            let stream = format!("stream-{n}");
            let _ = registry.subscribe(&session, &stream, None).await;
            registry.remove_session(&session);
        }
        assert!(registry.0.lock().unwrap().streams.len() <= MAX_STREAMS);
        assert!(registry.0.lock().unwrap().sessions.is_empty());
    }
    #[tokio::test]
    async fn publish_uses_stream_cap_and_preserves_subscribed_rings() {
        let registry = ConnectEventRegistry::default();
        registry.open_session("active");
        registry.publish("protected", "event", json!(0)).unwrap();
        let _active = registry
            .subscribe("active", "protected", None)
            .await
            .unwrap();
        registry.activate("active", "protected");

        for n in 0..MAX_STREAMS - 1 {
            registry
                .publish(&format!("published-{n}"), "event", json!(n))
                .unwrap();
        }
        registry.publish("overflow", "event", json!(1)).unwrap();

        let state = registry
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(state.streams.len(), MAX_STREAMS);
        assert!(state.streams.contains_key("protected"));
    }

    #[tokio::test]
    async fn all_active_streams_reject_publish_and_subscribe_overflow() {
        let registry = ConnectEventRegistry::default();
        registry.open_session("active");
        {
            let mut state = registry
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            for n in 0..MAX_STREAMS {
                let stream = format!("active-{n}");
                state.streams.insert(
                    stream.clone(),
                    StreamRing {
                        epoch: Uuid::new_v4(),
                        next_sequence: 1,
                        events: VecDeque::new(),
                    },
                );
                state.subscriber_counts.insert(stream.clone(), 1);
                state.subscribers.insert(
                    ("active".to_owned(), stream),
                    Subscriber {
                        active: Arc::new(AtomicBool::new(true)),
                        changed: Arc::new(Notify::new()),
                        cancelled: Arc::new(Notify::new()),
                        cancelled_flag: Arc::new(AtomicBool::new(false)),
                    },
                );
            }
        }

        assert_eq!(
            registry.publish("overflow", "event", json!({})),
            Err(PublishError::StreamLimit)
        );
        registry.open_session("overflow-owner");
        for n in 0..256 {
            let stream = format!("overflow-{n}");
            assert!(matches!(
                registry.subscribe("overflow-owner", &stream, None).await,
                Err(SubscribeError::StreamLimit)
            ));
            let state = registry
                .0
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            assert!(state.sessions.contains_key("overflow-owner"));
            assert!(
                state.gates.is_empty(),
                "rejected admission left gate metadata"
            );
        }
        let state = registry
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        assert_eq!(state.streams.len(), MAX_STREAMS);
        assert!(!state.streams.contains_key("overflow"));
    }
}
