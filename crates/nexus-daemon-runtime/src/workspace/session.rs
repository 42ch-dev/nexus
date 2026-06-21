//! Workspace session management (DF-31 skeleton).
//!
//! In-memory session store for `workspace.open` / `workspace.commit`.
//! Sessions are daemon-scoped and expire after a configurable TTL.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// A workspace session identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SessionId(pub String);

impl SessionId {
    /// Generate a new session ID.
    #[must_use]
    pub fn new() -> Self {
        let id = uuid::Uuid::new_v4().to_string();
        Self(format!("ws_{id}"))
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

/// Snapshot of the workspace state at session open time.
///
/// # Future expansion (DF-31 → DF-42)
///
/// Currently contains only the workspace root and resolved path.
/// Future iterations may add:
/// - File listing with hashes (for conflict detection)
/// - Git tree-ish reference
/// - Manifest version
#[derive(Debug, Clone)]
pub struct WorkspaceSnapshot {
    /// Absolute path to the workspace creative root.
    pub workspace_root: String,
    /// Relative path within the workspace that was opened.
    pub relative_path: String,
    /// Whether the target path already existed.
    pub existed: bool,
}

/// Information about an active workspace session.
#[derive(Debug, Clone)]
pub struct SessionInfo {
    /// The resolved workspace path.
    pub workspace_path: String,
    /// Snapshot of workspace state at open time.
    pub snapshot: WorkspaceSnapshot,
    /// When this session was created.
    pub created_at: Instant,
    /// Whether the session has been consumed (committed) and is now stale.
    pub consumed: bool,
}

impl SessionInfo {
    /// Whether this session has expired (older than `ttl`).
    #[must_use]
    fn is_expired(&self, ttl: Duration) -> bool {
        self.created_at.elapsed() > ttl
    }
}

/// In-memory workspace session manager.
///
/// Manages the lifecycle of workspace sessions:
/// - **open**: Create a new session with a snapshot of workspace state.
/// - **validate**: Check that a session exists, is not consumed, and is not expired.
/// - **consume**: Mark a session as consumed after a successful commit.
/// - **cleanup**: Remove expired sessions.
///
/// # Conflict model (DF-31 skeleton)
///
/// - Each `workspace.open` creates a **new** session with a unique `session_id`.
/// - A `workspace.commit` references a `session_id`. If the session has already
///   been consumed (committed), the commit is **rejected** with a stale-session
///   error rather than silently overwriting.
/// - Expired sessions are rejected on both open and commit paths.
///
/// # Future expansion (DF-31 → DF-42)
///
/// The current skeleton uses simple in-memory sessions without file-level
/// conflict detection. Future iterations may add:
/// - File-level checksums in the snapshot for true OCC (optimistic concurrency)
/// - Persistent session storage
/// - Cross-daemon session negotiation
/// - Branch/merge session semantics
pub struct WorkspaceSessionManager {
    sessions: Mutex<HashMap<SessionId, SessionInfo>>,
    /// Default session TTL.
    ttl: Duration,
}

impl WorkspaceSessionManager {
    /// Default session TTL (5 minutes).
    pub const DEFAULT_TTL: Duration = Duration::from_mins(5);

    /// Create a new session manager with the default TTL.
    #[must_use]
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            ttl: Self::DEFAULT_TTL,
        }
    }

    /// Open a new workspace session.
    ///
    /// Creates a session with a unique ID and a snapshot of the given
    /// workspace path. The caller is responsible for validating the path
    /// before calling this method.
    ///
    /// # Panics
    ///
    /// Panics if the mutex is poisoned.
    pub fn open_session(
        &self,
        workspace_root: &str,
        relative_path: &str,
        existed: bool,
    ) -> SessionId {
        cleanup_expired(&self.sessions, self.ttl);
        let session_id = SessionId::new();
        let info = SessionInfo {
            workspace_path: relative_path.to_string(),
            snapshot: WorkspaceSnapshot {
                workspace_root: workspace_root.to_string(),
                relative_path: relative_path.to_string(),
                existed,
            },
            created_at: Instant::now(),
            consumed: false,
        };
        self.sessions
            .lock()
            .expect("session manager mutex poisoned")
            .insert(session_id.clone(), info);
        session_id
    }

    /// Validate that a session exists and is usable.
    ///
    /// Returns `Ok(&SessionInfo)` if the session exists, is not consumed,
    /// and is not expired. Returns an error string otherwise.
    ///
    /// # Errors
    ///
    /// Returns an error string if:
    /// - The session ID is not found in the session store
    /// - The session has already been consumed (committed)
    /// - The session has exceeded its TTL
    ///
    /// # Panics
    ///
    /// Panics if the mutex is poisoned.
    pub fn validate_session(&self, session_id: &SessionId) -> Result<SessionInfo, String> {
        cleanup_expired(&self.sessions, self.ttl);
        let info = {
            let sessions = self
                .sessions
                .lock()
                .expect("session manager mutex poisoned");
            sessions.get(session_id).cloned()
        };
        let info = info.ok_or_else(|| format!("session {session_id} not found"))?;
        if info.consumed {
            return Err(format!(
                "session {session_id} has already been committed (stale session)"
            ));
        }
        if info.is_expired(self.ttl) {
            return Err(format!("session {session_id} has expired"));
        }
        Ok(info)
    }

    /// Mark a session as consumed after a successful commit.
    ///
    /// Returns the session info if successful, or an error if the session
    /// was already consumed or not found.
    ///
    /// # Errors
    ///
    /// Returns an error string if:
    /// - The session ID is not found in the session store
    /// - The session has already been consumed (committed)
    /// - The session has exceeded its TTL
    ///
    /// # Panics
    ///
    /// Panics if the mutex is poisoned.
    pub fn consume_session(&self, session_id: &SessionId) -> Result<SessionInfo, String> {
        cleanup_expired(&self.sessions, self.ttl);
        let info = {
            let sessions = self
                .sessions
                .lock()
                .expect("session manager mutex poisoned");
            sessions.get(session_id).cloned()
        };
        let info = info.ok_or_else(|| format!("session {session_id} not found"))?;
        if info.consumed {
            return Err(format!(
                "session {session_id} has already been committed (stale session)"
            ));
        }
        if info.is_expired(self.ttl) {
            return Err(format!("session {session_id} has expired"));
        }
        // Mark consumed in a separate lock acquisition
        {
            let mut sessions = self
                .sessions
                .lock()
                .expect("session manager mutex poisoned");
            if let Some(entry) = sessions.get_mut(session_id) {
                entry.consumed = true;
            }
        }
        Ok(info)
    }
}

impl Default for WorkspaceSessionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Remove sessions that have exceeded the TTL.
fn cleanup_expired(sessions: &Mutex<HashMap<SessionId, SessionInfo>>, ttl: Duration) {
    let mut guard = sessions.lock().expect("session manager mutex poisoned");
    guard.retain(|_id, info| !info.is_expired(ttl));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn open_session_returns_unique_ids() {
        let mgr = WorkspaceSessionManager::new();
        let id1 = mgr.open_session("/ws", "path1", true);
        let id2 = mgr.open_session("/ws", "path2", false);
        assert_ne!(id1.0, id2.0);
    }

    #[test]
    fn validate_session_succeeds_for_fresh_session() {
        let mgr = WorkspaceSessionManager::new();
        let id = mgr.open_session("/ws", "test", true);
        let result = mgr.validate_session(&id);
        assert!(result.is_ok(), "expected Ok, got {result:?}");
    }

    #[test]
    fn validate_session_fails_for_unknown_id() {
        let mgr = WorkspaceSessionManager::new();
        let fake_id = SessionId("ws_nonexistent".to_string());
        let result = mgr.validate_session(&fake_id);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[test]
    fn consume_session_marks_session_as_consumed() {
        let mgr = WorkspaceSessionManager::new();
        let id = mgr.open_session("/ws", "test", true);
        let result = mgr.consume_session(&id);
        assert!(result.is_ok());

        // Second consume should fail
        let result2 = mgr.consume_session(&id);
        assert!(result2.is_err());
        assert!(result2.unwrap_err().contains("already been committed"));
    }

    #[test]
    fn validate_after_consume_fails() {
        let mgr = WorkspaceSessionManager::new();
        let id = mgr.open_session("/ws", "test", true);
        mgr.consume_session(&id).unwrap();

        let result = mgr.validate_session(&id);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already been committed"));
    }

    #[test]
    fn session_snapshot_preserves_path_info() {
        let mgr = WorkspaceSessionManager::new();
        let id = mgr.open_session("/home/user/workspace", "Works/my-novel", true);
        let info = mgr.validate_session(&id).unwrap();
        assert_eq!(info.snapshot.workspace_root, "/home/user/workspace");
        assert_eq!(info.snapshot.relative_path, "Works/my-novel");
        assert!(info.snapshot.existed);
    }

    #[test]
    fn expired_sessions_are_cleaned_up() {
        // Create a session manager with a very short TTL
        let mgr = WorkspaceSessionManager {
            sessions: Mutex::new(HashMap::new()),
            ttl: Duration::from_millis(1),
        };
        let id = mgr.open_session("/ws", "test", true);

        // Wait for expiration
        thread::sleep(Duration::from_millis(10));

        // Session should be cleaned up — validate should fail
        let result = mgr.validate_session(&id);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }
}
