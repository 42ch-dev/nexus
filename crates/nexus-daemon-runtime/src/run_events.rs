//! Bounded per-run workflow event rings and SSE replay (v1.188 P4 §6.3).

use nexus_agent_host::capability::model::HostEvent;
use nexus_orchestration::run_state::RunRecord;
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, Weak};
use std::time::{SystemTime, UNIX_EPOCH};

pub const MAX_RECORDS_PER_RUN: usize = 256;
pub const MAX_BYTES_PER_RUN: usize = 1024 * 1024;
pub const MAX_FRAME_BYTES: usize = 512 * 1024;
pub const MAX_LIVE_RINGS: usize = 64;
pub const MAX_TERMINAL_RINGS: usize = 64;

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
}

struct StoredRecord {
    sequence: u64,
    frame: SseFrame,
    byte_len: usize,
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
        if let Some(ring) = state.live.remove(run_id) {
            state.terminal.insert(run_id.to_string(), ring);
            state.terminal_order.push_back(run_id.to_string());
            while state.terminal.len() > MAX_TERMINAL_RINGS {
                if let Some(evicted) = state.terminal_order.pop_front() {
                    state.terminal.remove(&evicted);
                }
            }
        }
    }

    pub fn subscribe(
        &self,
        run_id: &str,
        last_event_id: Option<&str>,
        inspect_url: String,
    ) -> Result<Vec<SseFrame>, SubscribeError> {
        let state = self
            .inner
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let ring = state
            .live
            .get(run_id)
            .or_else(|| state.terminal.get(run_id));
        let Some(ring) = ring else {
            return Err(SubscribeError::HistoryUnavailable(HistoryUnavailableWire {
                run_id: run_id.to_string(),
                inspect_url,
            }));
        };
        if let Some(cursor) = last_event_id {
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
            return Ok(collect_from(ring, run_id, Some(seq)));
        }
        Ok(collect_from(ring, run_id, None))
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
            self.append_gap(run_id);
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
            frame,
            byte_len,
        });
        ring.total_bytes += byte_len;
        while ring.records.len() > MAX_RECORDS_PER_RUN || ring.total_bytes > MAX_BYTES_PER_RUN {
            if let Some(front) = ring.records.pop_front() {
                ring.total_bytes = ring.total_bytes.saturating_sub(front.byte_len);
            }
        }
    }

    fn append_gap(&self, run_id: &str) {
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
        ring.records.push_back(StoredRecord {
            sequence,
            frame: SseFrame {
                id: format!("{}:{}", ring.epoch, sequence),
                event: "gap".to_string(),
                data,
            },
            byte_len,
        });
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
    SseFrame {
        id: format!("{}:gap", epoch),
        event: "gap".to_string(),
        data: serde_json::to_string(&gap).unwrap_or_default(),
    }
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
