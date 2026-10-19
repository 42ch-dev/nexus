//! ACP session persistence across CLI invocations (ACP-R6).
//!
//! This module provides session load/cleanup functionality so that ACP session
//! metadata persists across CLI exits. Sessions are stored as JSON in a global
//! cache location ($HOME/.nexus42/acp/sessions.json).

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use agent_client_protocol::schema::v1::SessionId;

/// A persisted ACP session entry.
///
/// Minimal session state for V1.0: just enough to call `session/load`
/// on the agent. The agent holds the actual conversation context.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEntry {
    /// The ACP session ID (from `session/new` response).
    pub session_id: SessionId,
    /// Agent identifier (e.g., "claude-acp").
    pub agent_id: String,
    /// Workspace hint: cwd at session creation time.
    pub workspace_hint: PathBuf,
    /// When the session was created.
    pub created_at: DateTime<Utc>,
    /// When the session was last used (updated on each `session/load`).
    pub last_used_at: DateTime<Utc>,
}

/// Manages persisted ACP sessions.
///
/// Sessions are stored as JSON in a single file. The manager provides
/// load/delete/cleanup operations for session entries.
#[derive(Debug, Clone)]
pub struct SessionManager {
    /// Path to the sessions JSON file.
    sessions_file: PathBuf,
}

/// JSON structure for sessions file.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct SessionsFile {
    sessions: Vec<SessionEntry>,
}

impl SessionManager {
    /// Create a new session manager with the given sessions file path.
    #[must_use]
    pub const fn new(sessions_file: PathBuf) -> Self {
        Self { sessions_file }
    }

    /// Get the default sessions file path: `$HOME/.nexus42/acp/sessions.json`
    ///
    /// # Panics
    ///
    /// Panics if the home directory cannot be determined.
    #[must_use]
    pub fn default_sessions_file() -> PathBuf {
        let home = dirs::home_dir().expect("Failed to get home directory");
        home.join(".nexus42").join("acp").join("sessions.json")
    }

    /// Load all sessions from the sessions file.
    ///
    /// Returns an empty vector if the file doesn't exist.
    ///
    /// # Errors
    ///
    /// Returns an error if the file exists but cannot be read or parsed.
    pub fn load_sessions(&self) -> crate::AcpResult<Vec<SessionEntry>> {
        if !self.sessions_file.exists() {
            return Ok(Vec::new());
        }

        let content = std::fs::read_to_string(&self.sessions_file)?;
        let file: SessionsFile = serde_json::from_str(&content)?;
        Ok(file.sessions)
    }

    /// Remove sessions that have expired (>24 hours old).
    ///
    /// Sessions with `last_used_at` older than 24 hours are removed from the
    /// sessions file. Returns the list of removed sessions.
    ///
    /// # Errors
    ///
    /// Returns an error if the sessions file cannot be read or written.
    pub fn cleanup_expired(&self) -> crate::AcpResult<Vec<SessionEntry>> {
        let sessions = self.load_sessions()?;

        let now = Utc::now();
        let max_age = chrono::Duration::hours(24);

        // Partition into kept and removed
        let (kept, removed): (Vec<_>, Vec<_>) = sessions
            .into_iter()
            .partition(|s| now.signed_duration_since(s.last_used_at) <= max_age);

        // Write back the kept sessions
        let file = SessionsFile { sessions: kept };
        let content = serde_json::to_string_pretty(&file)?;

        if let Some(parent) = self.sessions_file.parent() {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::write(&self.sessions_file, content)?;

        Ok(removed)
    }

    /// Delete a session by its session ID.
    ///
    /// Returns the deleted session if it existed, or None if it didn't.
    ///
    /// # Errors
    ///
    /// Returns an error if the sessions file cannot be read or written.
    pub fn delete_session(&self, session_id: &SessionId) -> crate::AcpResult<Option<SessionEntry>> {
        let sessions = self.load_sessions()?;

        // Find and remove the session
        let (kept, removed): (Vec<_>, Vec<_>) = sessions
            .into_iter()
            .partition(|s| s.session_id != *session_id);

        // Write back the kept sessions
        let file = SessionsFile { sessions: kept };
        let content = serde_json::to_string_pretty(&file)?;

        if let Some(parent) = self.sessions_file.parent() {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::write(&self.sessions_file, content)?;

        // Return the removed session (if any)
        Ok(removed.into_iter().next())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// Seed the sessions file directly.
    ///
    /// The production writer (`SessionManager::save_session`) was removed, so
    /// tests of the surviving load/cleanup/delete API set up state themselves.
    fn write_sessions(path: &std::path::Path, sessions: Vec<SessionEntry>) {
        let file = SessionsFile { sessions };
        std::fs::write(path, serde_json::to_string(&file).unwrap()).unwrap();
    }

    #[test]
    fn session_entry_construction() {
        let session_id = SessionId::new("test-session-123");
        let entry = SessionEntry {
            session_id: session_id.clone(),
            agent_id: "claude-acp".to_string(),
            workspace_hint: PathBuf::from("/tmp/test-workspace"),
            created_at: Utc::now(),
            last_used_at: Utc::now(),
        };

        assert_eq!(entry.session_id, session_id);
        assert_eq!(entry.agent_id, "claude-acp");
        assert_eq!(entry.workspace_hint, PathBuf::from("/tmp/test-workspace"));
    }

    #[test]
    fn session_entry_serialization() {
        let entry = SessionEntry {
            session_id: SessionId::new("sess-abc"),
            agent_id: "codex-acp".to_string(),
            workspace_hint: PathBuf::from("/home/user/project"),
            created_at: "2026-04-08T10:00:00Z".parse().unwrap(),
            last_used_at: "2026-04-08T15:00:00Z".parse().unwrap(),
        };

        // Serialize to JSON
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("sess-abc"));
        assert!(json.contains("codex-acp"));
        assert!(json.contains("/home/user/project"));

        // Deserialize back
        let decoded: SessionEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.session_id, entry.session_id);
        assert_eq!(decoded.agent_id, entry.agent_id);
        assert_eq!(decoded.workspace_hint, entry.workspace_hint);
    }

    #[test]
    fn load_sessions_from_file() {
        use tempfile::TempDir;

        // Create a temporary directory for testing
        let temp_dir = TempDir::new().unwrap();
        let sessions_file = temp_dir.path().join("sessions.json");

        // Write test session data
        let test_data = serde_json::json!({
            "sessions": [
                {
                    "session_id": "sess-001",
                    "agent_id": "claude-acp",
                    "workspace_hint": "/tmp/workspace",
                    "created_at": "2026-04-08T10:00:00Z",
                    "last_used_at": "2026-04-08T15:00:00Z"
                }
            ]
        });
        std::fs::write(&sessions_file, test_data.to_string()).unwrap();

        // Load sessions (this will fail because SessionManager doesn't exist)
        let manager = SessionManager::new(sessions_file);
        let sessions = manager.load_sessions().unwrap();

        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].agent_id, "claude-acp");
    }

    #[test]
    fn load_sessions_empty_file() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let sessions_file = temp_dir.path().join("sessions.json");

        // Write empty sessions array
        let test_data = serde_json::json!({ "sessions": [] });
        std::fs::write(&sessions_file, test_data.to_string()).unwrap();

        let manager = SessionManager::new(sessions_file);
        let sessions = manager.load_sessions().unwrap();

        assert_eq!(sessions.len(), 0);
    }

    #[test]
    fn load_sessions_missing_file() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let sessions_file = temp_dir.path().join("missing.json");

        // Don't create the file
        let manager = SessionManager::new(sessions_file);
        let sessions = manager.load_sessions().unwrap();

        // Should return empty list if file doesn't exist
        assert_eq!(sessions.len(), 0);
    }

    #[test]
    fn cleanup_expired_removes_old_sessions() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let sessions_file = temp_dir.path().join("sessions.json");

        let manager = SessionManager::new(sessions_file.clone());
        // An old session (30 hours ago) and a recent one (1 hour ago).
        write_sessions(
            &sessions_file,
            vec![
                SessionEntry {
                    session_id: SessionId::new("sess-old"),
                    agent_id: "claude-acp".to_string(),
                    workspace_hint: PathBuf::from("/tmp/workspace"),
                    created_at: Utc::now() - chrono::Duration::hours(30),
                    last_used_at: Utc::now() - chrono::Duration::hours(30),
                },
                SessionEntry {
                    session_id: SessionId::new("sess-recent"),
                    agent_id: "codex-acp".to_string(),
                    workspace_hint: PathBuf::from("/tmp/workspace"),
                    created_at: Utc::now() - chrono::Duration::hours(1),
                    last_used_at: Utc::now() - chrono::Duration::hours(1),
                },
            ],
        );

        // Cleanup expired sessions (>24h old)
        let removed = manager.cleanup_expired().unwrap();

        // Should have removed 1 session (the old one)
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].session_id, SessionId::new("sess-old"));

        // Verify remaining sessions
        let remaining = manager.load_sessions().unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].session_id, SessionId::new("sess-recent"));
    }

    #[test]
    fn cleanup_expired_keeps_recent_sessions() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let sessions_file = temp_dir.path().join("sessions.json");

        let manager = SessionManager::new(sessions_file.clone());
        // A recent session (1 hour ago).
        write_sessions(
            &sessions_file,
            vec![SessionEntry {
                session_id: SessionId::new("sess-recent"),
                agent_id: "claude-acp".to_string(),
                workspace_hint: PathBuf::from("/tmp/workspace"),
                created_at: Utc::now() - chrono::Duration::hours(1),
                last_used_at: Utc::now() - chrono::Duration::hours(1),
            }],
        );

        // Cleanup expired sessions
        let removed = manager.cleanup_expired().unwrap();

        // Should not remove anything
        assert_eq!(removed.len(), 0);

        // Session should still be there
        let remaining = manager.load_sessions().unwrap();
        assert_eq!(remaining.len(), 1);
    }

    #[test]
    fn cleanup_expired_boundary_24_hours() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let sessions_file = temp_dir.path().join("sessions.json");

        let manager = SessionManager::new(sessions_file.clone());
        // A session exactly 24 hours old (boundary case).
        write_sessions(
            &sessions_file,
            vec![SessionEntry {
                session_id: SessionId::new("sess-boundary"),
                agent_id: "claude-acp".to_string(),
                workspace_hint: PathBuf::from("/tmp/workspace"),
                created_at: Utc::now() - chrono::Duration::hours(24),
                last_used_at: Utc::now() - chrono::Duration::hours(24),
            }],
        );

        // Cleanup expired sessions
        let removed = manager.cleanup_expired().unwrap();

        // Sessions at exactly 24h should be removed
        assert_eq!(removed.len(), 1);
    }

    #[test]
    fn delete_session_removes_by_id() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let sessions_file = temp_dir.path().join("sessions.json");

        let manager = SessionManager::new(sessions_file.clone());
        // Two sessions to delete from.
        write_sessions(
            &sessions_file,
            vec![
                SessionEntry {
                    session_id: SessionId::new("sess-001"),
                    agent_id: "claude-acp".to_string(),
                    workspace_hint: PathBuf::from("/tmp/ws1"),
                    created_at: Utc::now(),
                    last_used_at: Utc::now(),
                },
                SessionEntry {
                    session_id: SessionId::new("sess-002"),
                    agent_id: "codex-acp".to_string(),
                    workspace_hint: PathBuf::from("/tmp/ws2"),
                    created_at: Utc::now(),
                    last_used_at: Utc::now(),
                },
            ],
        );

        // Delete one session
        let removed = manager.delete_session(&SessionId::new("sess-001")).unwrap();
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().session_id, SessionId::new("sess-001"));

        // Verify only sess-002 remains
        let remaining = manager.load_sessions().unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].session_id, SessionId::new("sess-002"));
    }

    #[test]
    fn default_sessions_file_path() {
        let path = SessionManager::default_sessions_file();

        // Should end with .nexus42/acp/sessions.json
        assert!(path.to_string_lossy().contains(".nexus42"));
        assert!(path.to_string_lossy().contains("acp"));
        assert_eq!(path.file_name().unwrap(), "sessions.json");
    }

    #[test]
    fn delete_session_returns_none_for_nonexistent() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let sessions_file = temp_dir.path().join("sessions.json");
        let manager = SessionManager::new(sessions_file);

        let deleted = manager
            .delete_session(&SessionId::new("nonexistent_session"))
            .unwrap();

        assert!(deleted.is_none());
    }

    #[test]
    fn cleanup_expired_empty_list() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let sessions_file = temp_dir.path().join("sessions.json");

        let manager = SessionManager::new(sessions_file.clone());
        write_sessions(&sessions_file, Vec::new());

        let removed = manager.cleanup_expired().unwrap();

        assert_eq!(removed.len(), 0);
    }

    #[test]
    fn cleanup_expired_missing_file() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let sessions_file = temp_dir.path().join("nonexistent.json");
        let manager = SessionManager::new(sessions_file);

        // Should not error, just return empty
        let removed = manager.cleanup_expired().unwrap();

        assert_eq!(removed.len(), 0);
    }

    #[test]
    fn load_sessions_corrupted_json_errors() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let sessions_file = temp_dir.path().join("corrupted.json");

        // Write invalid JSON
        std::fs::write(&sessions_file, "not valid json {").unwrap();

        let manager = SessionManager::new(sessions_file);

        // Should return error for corrupted JSON
        assert!(manager.load_sessions().is_err());
    }
}
