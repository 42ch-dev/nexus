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
//! - `in_flight`: an atomic count of connections accepted but not yet
//!   registered (in-handshake) or torn down. The accept loop gates on
//!   `in_flight + sessions.len() >= max_sessions` so a dial flood of
//!   incomplete handshakes cannot exceed the cap (QC-fix W-A; the
//!   registered count alone left in-handshake connections uncounted).
//!
//! Lifecycle:
//! - `reserve_in_flight` is called at accept, BEFORE the WS upgrade +
//!   handshake (which run inside the spawned connection task). It fails
//!   closed when the budget is exhausted — a refused connection never
//!   reserves a slot.
//! - `register` is the deterministic last-wins replacement point: a second
//!   session with the same peer id replaces the first (old responder
//!   closed, fresh admission) — no two live sessions per peer id
//!   (AR-67 #4). It also converts the connection's in-flight reservation
//!   into the registered session (releases the in-flight slot).
//! - `evict` (with the expected-responder guard) is the close-observation
//!   teardown: the session's monitor calls it when the transport drops; the
//!   guard prevents a stale monitor from evicting a replacement session.
//! - `release_in_flight` is the failure/close fallback: a connection task
//!   that ends without registering (handshake rejection, timeout, WS
//!   upgrade failure) releases its reservation.
//!
//! The manager is lock-light: `std::sync::Mutex` critical sections are
//! synchronous (no `.await` under a guard) — `ConnectResponder::close` is
//! synchronous, so replacement/eviction never parks. Poisoned locks are
//! recovered via [`std::sync::PoisonError::into_inner`] (daemon-wide mutex
//! policy), so public methods never panic.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
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
    /// In-handshake / pre-registration connection count (accepted but not
    /// yet registered). Together with the sessions map it bounds the total
    /// accepted connections at the accept gate (QC-fix W-A).
    in_flight: AtomicUsize,
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
            in_flight: AtomicUsize::new(0),
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

    /// Total connection budget used: registered sessions + in-flight
    /// (in-handshake) connections.
    #[must_use]
    pub fn connection_count(&self) -> usize {
        self.session_count() + self.in_flight.load(Ordering::Relaxed)
    }

    /// Reserve one in-flight slot for an accepted connection.
    ///
    /// Called by the accept loop BEFORE spawning the connection task (the
    /// WS upgrade + handshake run inside that task). Returns `false` when
    /// the budget is exhausted — the caller refuses the connection. The
    /// slot is released via [`PeerSessionManager::release_in_flight`] when
    /// the connection task finishes without registering (handshake failure
    /// or timeout) and converted to a registered session by `register`.
    #[must_use]
    pub fn reserve_in_flight(&self, max_sessions: usize) -> bool {
        let mut in_flight = self.in_flight.load(Ordering::Relaxed);
        loop {
            if self.session_count() + in_flight >= max_sessions {
                return false;
            }
            match self.in_flight.compare_exchange_weak(
                in_flight,
                in_flight + 1,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return true,
                Err(observed) => in_flight = observed,
            }
        }
    }

    /// Release an in-flight reservation (handshake failed / timed out, or
    /// the connection task ended without registering). Saturating — a
    /// defensive double-release can never underflow the counter.
    pub fn release_in_flight(&self) {
        let _ = self
            .in_flight
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |v| {
                Some(v.saturating_sub(1))
            });
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
        // QC-fix W-A: the accept reservation converts into a registered
        // session here. Release the in-flight slot AFTER the map insert —
        // releasing first would open a transient window where a concurrent
        // accept sees a freed slot and over-admits past the cap. A call
        // without a prior reservation (defensive; e.g. direct unit usage)
        // is a saturating no-op.
        self.release_in_flight();
        if let Some(old) = replaced_record {
            // Deterministic last-wins: the replaced session is closed (the
            // old responder is torn down outside the lock — `close()` is
            // synchronous and spawns the transport teardown, so no lock is
            // held across a scheduling point).
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
            record.responder.close();
        }
        true
    }
}
