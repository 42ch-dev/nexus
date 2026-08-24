//! Peer session manager (V1.174 P0, AR-67 §3.2-§3.4).
//!
//! Tracks live Connect sessions per peer id:
//!
//! - `sessions`: `peer_id → SessionRecord { responder, admitted_ids,
//!   connected_at }` — the responder handle the daemon's dispatch path
//!   reverse-invokes, the tool ids the session admitted at registration
//!   (T2 granularity: the authenticated manifest's `tools[]`; the T3
//!   admission filter chain narrows this set), and the establishment
//!   timestamp.
//! - `reverse`: `tool_id → peer_id` — the per-session id index used for
//!   eviction. T3 replaces this surface with the process-global
//!   `PeerToolTable` (AR-68); this manager stays the session-record owner.
//!
//! Lifecycle:
//! - `register` is the deterministic last-wins replacement point: a second
//!   session with the same peer id replaces the first (old responder
//!   closed, old reverse entries evicted, fresh admission) — no two live
//!   sessions per peer id (AR-67 #4).
//! - `evict` (with the expected-responder guard) is the close-observation
//!   teardown: the session's monitor calls it when the transport drops; the
//!   guard prevents a stale monitor from evicting a replacement session.
//!
//! The manager is lock-light: `std::sync::Mutex` critical sections are
//! synchronous (no `.await` under a guard) — `ConnectResponder::close` is
//! synchronous, so replacement/eviction never parks. Poisoned locks are
//! recovered via [`std::sync::PoisonError::into_inner`] (daemon-wide mutex
//! policy), so public methods never panic.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use chrono::{DateTime, Utc};
use spoke_connect::remote::ConnectResponder;

/// Default maximum concurrent peer sessions (AR-67 #4; config-gated).
pub const DEFAULT_MAX_SESSIONS: usize = 8;

/// One established peer session.
#[derive(Clone)]
pub struct SessionRecord {
    /// The authenticated dialer peer id.
    pub peer_id: String,
    /// The responder handle (reverse-invoke face for this session).
    pub responder: Arc<ConnectResponder>,
    /// Tool ids admitted by this session (manifest `tools[]` at T2
    /// granularity; the T3 admission chain narrows this).
    pub admitted_ids: Vec<String>,
    /// Wall-clock establishment time.
    pub connected_at: DateTime<Utc>,
}

/// Process-scoped session registry (one per accept loop).
pub struct PeerSessionManager {
    sessions: Mutex<HashMap<String, SessionRecord>>,
    /// `tool_id → peer_id` reverse index (session-scoped; T3 replaces this
    /// surface with the `PeerToolTable`).
    reverse: Mutex<HashMap<String, String>>,
}

impl Default for PeerSessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PeerSessionManager {
    /// Create an empty session manager.
    #[must_use]
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            reverse: Mutex::new(HashMap::new()),
        }
    }

    /// Number of live sessions.
    #[must_use]
    pub fn session_count(&self) -> usize {
        self.sessions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    /// Read-only clone of one session record.
    #[must_use]
    pub fn get(&self, peer_id: &str) -> Option<SessionRecord> {
        self.sessions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(peer_id)
            .cloned()
    }

    /// All live peer ids.
    #[must_use]
    pub fn peer_ids(&self) -> Vec<String> {
        self.sessions
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .keys()
            .cloned()
            .collect()
    }

    /// Reverse-index lookup: which live peer owns `tool_id`?
    #[must_use]
    pub fn tool_owner(&self, tool_id: &str) -> Option<String> {
        self.reverse
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(tool_id)
            .cloned()
    }

    /// All tool ids currently indexed by live sessions.
    #[must_use]
    pub fn indexed_tool_ids(&self) -> Vec<String> {
        self.reverse
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .keys()
            .cloned()
            .collect()
    }

    /// Register a fresh established session.
    ///
    /// Deterministic last-wins (AR-67 #4): a session already registered for
    /// the same peer id is closed and its reverse entries evicted before the
    /// new session is admitted. Returns `true` when a previous session was
    /// replaced.
    ///
    /// The old responder is closed OUTSIDE the mutex guard (synchronous
    /// `ConnectResponder::close` spawns the transport teardown) so no lock is
    /// held across a scheduling point.
    pub fn register(
        &self,
        peer_id: &str,
        responder: Arc<ConnectResponder>,
        admitted_ids: &[String],
    ) -> bool {
        let replaced_record = {
            let mut sessions = self.sessions.lock().unwrap_or_else(PoisonError::into_inner);
            let record = SessionRecord {
                peer_id: peer_id.to_owned(),
                connected_at: Utc::now(),
                responder,
                admitted_ids: admitted_ids.to_vec(),
            };
            sessions.insert(peer_id.to_owned(), record)
        };
        // Rebuild the reverse index for this peer (evict old entries, admit
        // the fresh set).
        let mut reverse = self.reverse.lock().unwrap_or_else(PoisonError::into_inner);
        reverse.retain(|_tool_id, owner| owner != peer_id);
        for tool_id in admitted_ids {
            reverse.insert(tool_id.clone(), peer_id.to_owned());
        }
        drop(reverse);
        if let Some(old) = replaced_record {
            // Deterministic last-wins: the replaced session is closed (old
            // entries evicted above; the old responder is torn down outside
            // the lock — `close()` is synchronous and spawns the transport
            // teardown, so no lock is held across a scheduling point).
            old.responder.close();
            tracing::info!(%peer_id, "peer session replaced (same peer id)");
            true
        } else {
            false
        }
    }

    /// Evict a session, optionally guarded by the expected responder.
    ///
    /// `expected` prevents a stale monitor (whose transport closed while the
    /// session was already replaced) from evicting the replacement session.
    /// Returns `false` when nothing was evicted (missing / guarded-out).
    pub fn evict(&self, peer_id: &str, expected: Option<&Arc<ConnectResponder>>) -> bool {
        let removed = {
            let mut sessions = self.sessions.lock().unwrap_or_else(PoisonError::into_inner);
            match sessions.get(peer_id) {
                Some(record) if expected.is_none_or(|e| Arc::ptr_eq(e, &record.responder)) => {
                    sessions.remove(peer_id)
                }
                _ => return false,
            }
        };
        if let Some(record) = removed {
            let mut reverse = self.reverse.lock().unwrap_or_else(PoisonError::into_inner);
            reverse.retain(|_tool_id, owner| owner != peer_id);
            drop(reverse);
            record.responder.close();
        }
        true
    }
}
