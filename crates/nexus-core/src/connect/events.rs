//! Bounded event retention and ack-gated Connect subscriptions.
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{Notify, Semaphore};
use uuid::Uuid;

const MAX_EVENTS: usize = 1024;
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
/// subscribe response write is observed; `changed` is the single wakeup for
/// every lifecycle transition (activation, new frames, replacement, close),
/// so a cancelled subscription always wakes and observes its own liveness.
struct Subscriber {
    active: Arc<AtomicBool>,
    changed: Arc<Notify>,
}

#[derive(Default)]
struct RegistryState {
    streams: HashMap<String, StreamRing>,
    subscribers: HashMap<(String, String), Subscriber>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SubscribeError {
    InvalidCursor,
    UnknownEpoch,
    FutureCursor,
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
    acked: Semaphore,
    active: Arc<AtomicBool>,
    changed: Arc<Notify>,
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

    /// The next bounded, contiguous batch, or an empty vector once the
    /// subscription has been cancelled (replaced, or its session closed).
    pub async fn next_batch(&mut self) -> Vec<EventFrame> {
        if self.sent_through.is_some() {
            if let Ok(permit) = self.acked.acquire().await {
                permit.forget();
                self.sent_through = None;
            }
        }
        loop {
            if !self.is_alive() {
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
            self.changed.notified().await;
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

impl ConnectEventRegistry {
    pub fn publish(&self, stream: &str, event: &str, data: Value) -> EventFrame {
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
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
        EventFrame {
            id: Some(format!("{epoch}:{sequence}")),
            event: event.to_owned(),
            data,
        }
    }

    /// Atomically captures the replay page and registers its live tail. A
    /// pre-existing subscription for the same (session, stream) is replaced:
    /// its delivery is cancelled (woken so it observes it is no longer live).
    ///
    /// # Errors
    /// Returns [`SubscribeError::InvalidCursor`] when `cursor` is malformed,
    /// [`SubscribeError::UnknownEpoch`] when it names a foreign ring epoch,
    /// and [`SubscribeError::FutureCursor`] when it is ahead of the ring.
    pub fn subscribe(
        &self,
        session: &str,
        stream: &str,
        cursor: Option<&str>,
    ) -> Result<(String, Option<String>, EventSubscription), SubscribeError> {
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let ring = state
            .streams
            .entry(stream.to_owned())
            .or_insert_with(|| StreamRing {
                epoch: Uuid::new_v4(),
                next_sequence: 1,
                events: VecDeque::new(),
            });
        let sequence = if let Some(cursor) = cursor {
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
            sequence
        } else {
            0
        };
        let epoch = ring.epoch.to_string();
        let replay = ring
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
        let active = Arc::new(AtomicBool::new(false));
        let changed = Arc::new(Notify::new());
        if let Some(previous) = state.subscribers.insert(
            (session.to_owned(), stream.to_owned()),
            Subscriber {
                active: Arc::clone(&active),
                changed: Arc::clone(&changed),
            },
        ) {
            previous.active.store(true, Ordering::Release);
            previous.changed.notify_one();
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
                acked: Semaphore::new(0),
                active,
                changed,
            },
        ))
    }

    /// Enable delivery only after the subscribe response write is observed.
    pub fn activate(&self, session: &str, stream: &str) {
        let state = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(subscriber) = state
            .subscribers
            .get(&(session.to_owned(), stream.to_owned()))
        {
            subscriber.active.store(true, Ordering::Release);
            subscriber.changed.notify_one();
        }
    }

    /// End every subscription owned by `session` (its drivers wake and stop).
    pub fn remove_session(&self, session: &str) {
        let mut state = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for ((owner, _), subscriber) in &state.subscribers {
            if owner == session {
                subscriber.active.store(true, Ordering::Release);
                subscriber.changed.notify_one();
            }
        }
        state.subscribers.retain(|(owner, _), _| owner != session);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn replay_then_live_is_ack_gated() {
        let registry = ConnectEventRegistry::default();
        let first = registry.publish("s", "one", json!(1));
        let (_, _, mut subscription) = registry.subscribe("p", "s", None).unwrap();
        registry.activate("p", "s");
        assert_eq!(subscription.replay, vec![first.clone()]);
        assert_eq!(subscription.next_batch().await, vec![first]);
        let next = registry.publish("s", "two", json!(2));
        assert!(tokio::time::timeout(
            std::time::Duration::from_millis(10),
            subscription.next_batch()
        )
        .await
        .is_err());
        subscription.ack();
        assert_eq!(subscription.next_batch().await, vec![next]);
    }

    #[tokio::test]
    async fn replacement_cancels_the_previous_delivery() {
        let registry = ConnectEventRegistry::default();
        let (_, _, mut first) = registry.subscribe("p", "s", None).unwrap();
        registry.activate("p", "s");
        let (_, _, mut second) = registry.subscribe("p", "s", None).unwrap();
        assert!(first.next_batch().await.is_empty());
        assert!(!first.is_alive());
        assert!(second.is_alive());
        let frame = registry.publish("s", "one", json!(1));
        registry.activate("p", "s");
        assert_eq!(second.next_batch().await, vec![frame]);
    }

    #[tokio::test]
    async fn session_close_cancels_deliveries() {
        let registry = ConnectEventRegistry::default();
        let (_, _, mut subscription) = registry.subscribe("p", "s", None).unwrap();
        registry.activate("p", "s");
        registry.remove_session("p");
        assert!(subscription.next_batch().await.is_empty());
    }

    #[test]
    fn cursor_shape_verbatim_echo_and_control_frames() {
        let registry = ConnectEventRegistry::default();
        let frame = registry.publish("s", "one", json!({}));
        let id = frame.id.unwrap();
        let (epoch, sequence) = id.rsplit_once(':').unwrap();
        assert!(!epoch.is_empty());
        assert_eq!(sequence, "1");
        assert!(sequence.parse::<u64>().is_ok());
        let (_, resumed_from, _) = registry.subscribe("p", "s", Some(&id)).unwrap();
        assert_eq!(resumed_from.as_deref(), Some(id.as_str()));
        assert_eq!(EventSubscription::control("gap", json!({})).id, None);
    }
}
