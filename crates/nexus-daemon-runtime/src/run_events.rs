//! Bounded per-run workflow event rings and live SSE replay (v1.188 P4 §6.3).

use nexus_agent_host::capability::model::HostEvent;
use nexus_orchestration::run_state::RunRecord;
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, Weak};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;

pub const MAX_RECORDS_PER_RUN: usize = 256;
pub const MAX_BYTES_PER_RUN: usize = 1024 * 1024;
pub const MAX_FRAME_BYTES: usize = 512 * 1024;
pub const MAX_LIVE_RINGS: usize = 64;
pub const MAX_TERMINAL_RINGS: usize = 64;
pub const MAX_SUBSCRIBERS_PER_RUN: usize = 16;
pub const MAX_PENDING_FRAMES_PER_SUB: usize = 16;
pub const MAX_PENDING_BYTES_PER_SUB: usize = 1024 * 1024;

#[derive(Clone)]
pub struct RunEventSink {
    run_id: String,
    registry: Weak<RunEventRegistryInner>,
}

impl RunEventSink {
    pub fn publish_host_event(
        &self,
        step_id: &str,
        attempt_id: &str,
        host_event: &HostEvent,
    ) {
        let Some(registry) = self.registry.upgrade() else {
            return;
        };
        registry.publish_host_event(&self.run_id, step_id, attempt_id, host_event);
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct HostEventWire<'a> {
    pub run_id: &'a str,
    pub epoch: u64,
    pub sequence: u64,
    pub step_id: &'a str,
    pub attempt_id: &'a str,
    pub host_event: &'a HostEvent,
}

#[derive(Debug, Clone, Serialize)]
pub struct RunStateWire<'a> {
    pub run_id: &'a str,
    pub epoch: u64,
    pub sequence: u64,
    pub state_revision: u64,
    pub status: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GapWire {
    pub run_id: String,
    pub epoch: u64,
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
}

struct RegistryState {
    live: HashMap<String, RunRing>,
    terminal_order: VecDeque<String>,
    terminal: HashMap<String, RunRing>,
}

struct RunRing {
    run_id: String,
    epoch: u64,
    records: VecDeque<StoredRecord>,
    total_bytes: usize,
    next_sequence: u64,
    subscribers: Vec<Subscriber>,
}

struct StoredRecord {
    sequence: u64,
    frame: SseFrame,
    byte_len: usize,
}

struct Subscriber {
    tx: mpsc::Sender<SseFrame>,
    pending_frames: usize,
    pending_bytes: usize,
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
            }),
        }
    }

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
        state.live.entry(run_id.to_string()).or_insert_with(|| RunRing {
            run_id: run_id.to_string(),
            epoch,
            records: VecDeque::new(),
            total_bytes: 0,
            next_sequence: 1,
            subscribers: Vec::new(),
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
        self.inner.publish_host_event(run_id, step_id, attempt_id, host_event);
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
            epoch: 0,
            sequence: 0,
            state_revision: record.state_revision,
            status: record.status.as_db_str(),
            reason,
        };
        self.inner.append_json(run_id, "run_state", &wire);
    }

    pub fn mark_terminal(&self, run_id: &str) {
        let mut state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(mut ring) = state.live.remove(run_id) {
            ring.subscribers.clear();
            state.terminal.insert(run_id.to_string(), ring);
            state.terminal_order.push_back(run_id.to_string());
            while state.terminal.len() > MAX_TERMINAL_RINGS {
                if let Some(evicted) = state.terminal_order.pop_front() {
                    state.terminal.remove(&evicted);
                }
            }
        }
    }

    /// Live SSE: atomically replay from `Last-Event-ID`, then register for live tail.
    pub fn subscribe_live(
        &self,
        run_id: &str,
        last_event_id: Option<&str>,
        inspect_url: String,
    ) -> Result<mpsc::Receiver<SseFrame>, SubscribeError> {
        let (tx, rx) = mpsc::channel(MAX_PENDING_FRAMES_PER_SUB);
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
        let pending_frames = replay.len();
        let pending_bytes = replay.iter().map(|f| f.data.len()).sum();
        for frame in replay {
            if tx.try_send(frame).is_err() {
                return Err(SubscribeError::TooManySubscribers);
            }
        }
        if is_terminal {
            return Ok(rx);
        }
        if ring.subscribers.len() >= MAX_SUBSCRIBERS_PER_RUN {
            return Err(SubscribeError::TooManySubscribers);
        }
        ring.subscribers.push(Subscriber {
            tx,
            pending_frames,
            pending_bytes,
        });
        Ok(rx)
    }
}

impl RunEventRegistryInner {
    fn publish_host_event(
        &self,
        run_id: &str,
        step_id: &str,
        attempt_id: &str,
        host_event: &HostEvent,
    ) {
        let wire = HostEventWire {
            run_id,
            epoch: 0,
            sequence: 0,
            step_id,
            attempt_id,
            host_event,
        };
        self.append_json(run_id, "host_event", &wire);
    }

    fn append_json(&self, run_id: &str, event: &str, value: &impl Serialize) {
        let data = match serde_json::to_string(value) {
            Ok(s) => s,
            Err(_) => return,
        };
        if data.len() > MAX_FRAME_BYTES {
            self.append_gap_record(run_id);
            return;
        }
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let ring = if let Some(r) = state.live.get_mut(run_id) {
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
        let byte_len = frame.data.len();
        ring.records.push_back(StoredRecord {
            sequence,
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

    fn append_gap_record(&self, run_id: &str) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let ring = if let Some(r) = state.live.get_mut(run_id) {
            r
        } else if let Some(r) = state.terminal.get_mut(run_id) {
            r
        } else {
            return;
        };
        let sequence = ring.next_sequence;
        ring.next_sequence += 1;
        let gap = GapWire {
            run_id: run_id.to_string(),
            epoch: ring.epoch,
            from_sequence: sequence,
            to_sequence: sequence,
        };
        let data = serde_json::to_string(&gap).unwrap_or_default();
        let byte_len = data.len();
        let frame = SseFrame {
            id: format!("{}:{}", ring.epoch, sequence),
            event: "gap".to_string(),
            data,
        };
        ring.records.push_back(StoredRecord {
            sequence,
            frame: frame.clone(),
            byte_len,
        });
        fanout(ring, frame);
    }
}

fn fanout(ring: &mut RunRing, frame: SseFrame) {
    let byte_len = frame.data.len();
    let mut i = 0;
    while i < ring.subscribers.len() {
        let sub = &mut ring.subscribers[i];
        if sub.pending_frames >= MAX_PENDING_FRAMES_PER_SUB
            || sub.pending_bytes.saturating_add(byte_len) > MAX_PENDING_BYTES_PER_SUB
        {
            let lag = lag_gap_frame(&ring.run_id, ring.epoch, ring.next_sequence.saturating_sub(1));
            let _ = sub.tx.try_send(lag);
            ring.subscribers.remove(i);
            continue;
        }
        match sub.tx.try_send(frame.clone()) {
            Ok(()) => {
                sub.pending_frames += 1;
                sub.pending_bytes += byte_len;
                i += 1;
            }
            Err(mpsc::error::TrySendError::Full(_)) => {
                let lag = lag_gap_frame(&ring.run_id, ring.epoch, ring.next_sequence.saturating_sub(1));
                let _ = sub.tx.try_send(lag);
                ring.subscribers.remove(i);
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                ring.subscribers.remove(i);
            }
        }
    }
}

fn collect_from(ring: &RunRing, run_id: &str, after: Option<u64>) -> Vec<SseFrame> {
    let mut out = Vec::new();
    let first_retained = ring.records.front().map(|r| r.sequence).unwrap_or(1);
    let after_seq = after.unwrap_or(0);
    if after_seq == 0 && first_retained > 1 {
        out.push(gap_frame(run_id, ring.epoch, 1, first_retained - 1));
    } else if after_seq > 0 && after_seq < first_retained {
        out.push(gap_frame(run_id, ring.epoch, after_seq + 1, first_retained - 1));
    }
    for rec in &ring.records {
        if rec.sequence > after_seq {
            out.push(rec.frame.clone());
        }
    }
    out
}

fn gap_frame(run_id: &str, epoch: u64, from: u64, to: u64) -> SseFrame {
    let gap = GapWire {
        run_id: run_id.to_string(),
        epoch,
        from_sequence: from,
        to_sequence: to,
    };
    let sequence = to.max(from);
    SseFrame {
        id: format!("{}:{}", epoch, sequence),
        event: "gap".to_string(),
        data: serde_json::to_string(&gap).unwrap_or_default(),
    }
}

fn lag_gap_frame(run_id: &str, epoch: u64, sequence: u64) -> SseFrame {
    gap_frame(run_id, epoch, sequence, sequence)
}

fn parse_cursor(cursor: &str) -> Result<(u64, u64), SubscribeError> {
    let Some((epoch, seq)) = cursor.split_once(':') else {
        return Err(SubscribeError::MalformedCursor);
    };
    let epoch = epoch.parse().map_err(|_| SubscribeError::MalformedCursor)?;
    let seq = seq.parse().map_err(|_| SubscribeError::MalformedCursor)?;
    Ok((epoch, seq))
}

fn current_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_orchestration::engine::SessionStatus;

    #[test]
    fn gap_cursor_is_numeric_sequence() {
        let frame = gap_frame("run-1", 42, 3, 7);
        assert!(frame.id.starts_with("42:"));
        let seq = frame.id.split(':').nth(1).unwrap();
        assert!(seq.parse::<u64>().is_ok());
        assert_ne!(seq, "gap");
    }

    #[tokio::test]
    async fn live_subscriber_receives_post_replay_events() {
        let registry = RunEventRegistry::new();
        let _sink = registry.try_register_live("run-1").expect("register");
        let mut rx = registry
            .subscribe_live("run-1", None, "/inspect".into())
            .expect("subscribe");
        let record = nexus_orchestration::run_state::RunRecord {
            session_id: nexus_orchestration::engine::SessionId("run-1".into()),
            status: SessionStatus::Running,
            state_revision: 1,
            execution_version: 1,
            descriptor: None,
            state: None,
            graph_version: 1,
        };
        registry.publish_run_state("run-1", &record);
        let frame = tokio::time::timeout(std::time::Duration::from_secs(1), rx.recv())
            .await
            .expect("timeout")
            .expect("frame");
        assert_eq!(frame.event, "run_state");
    }

    #[tokio::test]
    async fn terminal_ring_replay_then_closes() {
        let registry = RunEventRegistry::new();
        let _sink = registry.try_register_live("run-1").expect("register");
        let record = nexus_orchestration::run_state::RunRecord {
            session_id: nexus_orchestration::engine::SessionId("run-1".into()),
            status: SessionStatus::Completed,
            state_revision: 2,
            execution_version: 1,
            descriptor: None,
            state: None,
            graph_version: 1,
        };
        registry.publish_run_state("run-1", &record);
        registry.mark_terminal("run-1");
        let mut rx = registry
            .subscribe_live("run-1", None, "/inspect".into())
            .expect("subscribe");
        let first = rx.recv().await.expect("run_state");
        assert_eq!(first.event, "run_state");
        assert!(rx.recv().await.is_none());
    }
}
