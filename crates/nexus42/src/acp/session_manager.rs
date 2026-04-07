//! ACP session persistence across CLI invocations (ACP-R6).
//!
//! This module provides session save/load functionality so that ACP sessions
//! persist across CLI exits. Sessions are stored as JSON in a global cache
//! location ($HOME/.nexus42/acp/sessions.json).

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::acp::client::SessionId;

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
/// load/save/delete operations for session entries.
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
    pub fn new(sessions_file: PathBuf) -> Self {
        Self { sessions_file }
    }

    /// Get the default sessions file path: `$HOME/.nexus42/acp/sessions.json`
    pub fn default_sessions_file() -> PathBuf {
        let home = dirs::home_dir().expect("Failed to get home directory");
        home.join(".nexus42").join("acp").join("sessions.json")
    }

    /// Load all sessions from the sessions file.
    ///
    /// Returns an empty vector if the file doesn't exist.
    pub fn load_sessions(&self) -> crate::acp::AcpResult<Vec<SessionEntry>> {
        if !self.sessions_file.exists() {
            return Ok(Vec::new());
        }

        let content = std::fs::read_to_string(&self.sessions_file)?;
        let file: SessionsFile = serde_json::from_str(&content)?;
        Ok(file.sessions)
    }

    /// Save a session entry to the sessions file.
    ///
    /// If a session with the same session_id exists, it will be updated.
    /// Otherwise, the session is appended to the list.
    #[allow(dead_code)]
    pub fn save_session(&self, entry: &SessionEntry) -> crate::acp::AcpResult<()> {
        // Load existing sessions
        let mut sessions = self.load_sessions()?;

        // Find and update existing session, or append new one
        if let Some(existing) = sessions
            .iter_mut()
            .find(|s| s.session_id == entry.session_id)
        {
            *existing = entry.clone();
        } else {
            sessions.push(entry.clone());
        }

        // Write back to file
        let file = SessionsFile { sessions };
        let content = serde_json::to_string_pretty(&file)?;

        // Ensure parent directory exists
        if let Some(parent) = self.sessions_file.parent() {
            std::fs::create_dir_all(parent)?;
        }

        std::fs::write(&self.sessions_file, content)?;
        Ok(())
    }

    /// Find the most recent session matching the given agent and workspace.
    ///
    /// Returns the most recently used session (by `last_used_at`) that matches
    /// both the agent_id and workspace_hint. Returns None if no matching session exists.
    #[allow(dead_code)]
    pub fn find_recent_session(
        &self,
        agent_id: &str,
        workspace_hint: &PathBuf,
    ) -> crate::acp::AcpResult<Option<SessionEntry>> {
        let sessions = self.load_sessions()?;

        // Filter by agent_id and workspace_hint, then find the most recent
        let matching = sessions
            .into_iter()
            .filter(|s| s.agent_id == agent_id && s.workspace_hint == *workspace_hint)
            .max_by_key(|s| s.last_used_at);

        Ok(matching)
    }

    /// Remove sessions that have expired (>24 hours old).
    ///
    /// Sessions with `last_used_at` older than 24 hours are removed from the
    /// sessions file. Returns the list of removed sessions.
    pub fn cleanup_expired(&self) -> crate::acp::AcpResult<Vec<SessionEntry>> {
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
    pub fn delete_session(
        &self,
        session_id: &SessionId,
    ) -> crate::acp::AcpResult<Option<SessionEntry>> {
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

        // Deserialize back
        let decoded: SessionEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.session_id, entry.session_id);
        assert_eq!(decoded.agent_id, entry.agent_id);
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
    fn save_session_creates_new_file() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let sessions_file = temp_dir.path().join("sessions.json");

        // File doesn't exist yet
        assert!(!sessions_file.exists());

        let manager = SessionManager::new(sessions_file.clone());
        let entry = SessionEntry {
            session_id: SessionId::new("sess-new"),
            agent_id: "claude-acp".to_string(),
            workspace_hint: PathBuf::from("/tmp/workspace"),
            created_at: Utc::now(),
            last_used_at: Utc::now(),
        };

        manager.save_session(&entry).unwrap();

        // File should now exist
        assert!(sessions_file.exists());

        // Verify content
        let sessions = manager.load_sessions().unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].session_id, entry.session_id);
    }

    #[test]
    fn save_session_appends_to_existing() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let sessions_file = temp_dir.path().join("sessions.json");

        // Create initial session
        let manager = SessionManager::new(sessions_file.clone());
        let entry1 = SessionEntry {
            session_id: SessionId::new("sess-001"),
            agent_id: "claude-acp".to_string(),
            workspace_hint: PathBuf::from("/tmp/ws1"),
            created_at: Utc::now(),
            last_used_at: Utc::now(),
        };
        manager.save_session(&entry1).unwrap();

        // Add another session
        let entry2 = SessionEntry {
            session_id: SessionId::new("sess-002"),
            agent_id: "codex-acp".to_string(),
            workspace_hint: PathBuf::from("/tmp/ws2"),
            created_at: Utc::now(),
            last_used_at: Utc::now(),
        };
        manager.save_session(&entry2).unwrap();

        // Should have 2 sessions
        let sessions = manager.load_sessions().unwrap();
        assert_eq!(sessions.len(), 2);
    }

    #[test]
    fn save_session_updates_existing() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let sessions_file = temp_dir.path().join("sessions.json");

        let manager = SessionManager::new(sessions_file.clone());

        // Create initial session
        let entry = SessionEntry {
            session_id: SessionId::new("sess-001"),
            agent_id: "claude-acp".to_string(),
            workspace_hint: PathBuf::from("/tmp/ws"),
            created_at: Utc::now(),
            last_used_at: Utc::now(),
        };
        manager.save_session(&entry).unwrap();

        // Update the same session (same session_id)
        let updated = SessionEntry {
            session_id: entry.session_id.clone(),
            agent_id: entry.agent_id.clone(),
            workspace_hint: entry.workspace_hint.clone(),
            created_at: entry.created_at,
            last_used_at: Utc::now(), // Updated timestamp
        };
        manager.save_session(&updated).unwrap();

        // Should still have only 1 session
        let sessions = manager.load_sessions().unwrap();
        assert_eq!(sessions.len(), 1);

        // Timestamp should be updated
        assert!(sessions[0].last_used_at > entry.last_used_at);
    }

    #[test]
    fn find_recent_session_matches_agent_and_workspace() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let sessions_file = temp_dir.path().join("sessions.json");

        let manager = SessionManager::new(sessions_file.clone());

        // Create multiple sessions
        let entry1 = SessionEntry {
            session_id: SessionId::new("sess-001"),
            agent_id: "claude-acp".to_string(),
            workspace_hint: PathBuf::from("/tmp/workspace1"),
            created_at: Utc::now(),
            last_used_at: Utc::now() - chrono::Duration::hours(2),
        };
        manager.save_session(&entry1).unwrap();

        let entry2 = SessionEntry {
            session_id: SessionId::new("sess-002"),
            agent_id: "claude-acp".to_string(),
            workspace_hint: PathBuf::from("/tmp/workspace2"),
            created_at: Utc::now(),
            last_used_at: Utc::now(), // More recent
        };
        manager.save_session(&entry2).unwrap();

        // Find session for claude-acp + workspace2
        let found = manager
            .find_recent_session("claude-acp", &PathBuf::from("/tmp/workspace2"))
            .unwrap();

        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.session_id, SessionId::new("sess-002"));
        assert_eq!(found.agent_id, "claude-acp");
    }

    #[test]
    fn find_recent_session_returns_none_if_no_match() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let sessions_file = temp_dir.path().join("sessions.json");

        let manager = SessionManager::new(sessions_file.clone());

        // Create a session for a different agent
        let entry = SessionEntry {
            session_id: SessionId::new("sess-001"),
            agent_id: "codex-acp".to_string(),
            workspace_hint: PathBuf::from("/tmp/workspace"),
            created_at: Utc::now(),
            last_used_at: Utc::now(),
        };
        manager.save_session(&entry).unwrap();

        // Search for claude-acp (should not find)
        let found = manager
            .find_recent_session("claude-acp", &PathBuf::from("/tmp/workspace"))
            .unwrap();

        assert!(found.is_none());
    }

    #[test]
    fn find_recent_session_returns_most_recent() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let sessions_file = temp_dir.path().join("sessions.json");

        let manager = SessionManager::new(sessions_file.clone());

        // Create two sessions for same agent + workspace
        let entry1 = SessionEntry {
            session_id: SessionId::new("sess-old"),
            agent_id: "claude-acp".to_string(),
            workspace_hint: PathBuf::from("/tmp/workspace"),
            created_at: Utc::now() - chrono::Duration::hours(2),
            last_used_at: Utc::now() - chrono::Duration::hours(2),
        };
        manager.save_session(&entry1).unwrap();

        let entry2 = SessionEntry {
            session_id: SessionId::new("sess-new"),
            agent_id: "claude-acp".to_string(),
            workspace_hint: PathBuf::from("/tmp/workspace"),
            created_at: Utc::now(),
            last_used_at: Utc::now(), // More recent
        };
        manager.save_session(&entry2).unwrap();

        // Should return the more recent one
        let found = manager
            .find_recent_session("claude-acp", &PathBuf::from("/tmp/workspace"))
            .unwrap()
            .unwrap();

        assert_eq!(found.session_id, SessionId::new("sess-new"));
    }

    #[test]
    fn cleanup_expired_removes_old_sessions() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let sessions_file = temp_dir.path().join("sessions.json");

        let manager = SessionManager::new(sessions_file.clone());

        // Create an old session (30 hours ago)
        let old_entry = SessionEntry {
            session_id: SessionId::new("sess-old"),
            agent_id: "claude-acp".to_string(),
            workspace_hint: PathBuf::from("/tmp/workspace"),
            created_at: Utc::now() - chrono::Duration::hours(30),
            last_used_at: Utc::now() - chrono::Duration::hours(30),
        };
        manager.save_session(&old_entry).unwrap();

        // Create a recent session (1 hour ago)
        let recent_entry = SessionEntry {
            session_id: SessionId::new("sess-recent"),
            agent_id: "codex-acp".to_string(),
            workspace_hint: PathBuf::from("/tmp/workspace"),
            created_at: Utc::now() - chrono::Duration::hours(1),
            last_used_at: Utc::now() - chrono::Duration::hours(1),
        };
        manager.save_session(&recent_entry).unwrap();

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

        // Create a recent session (1 hour ago)
        let entry = SessionEntry {
            session_id: SessionId::new("sess-recent"),
            agent_id: "claude-acp".to_string(),
            workspace_hint: PathBuf::from("/tmp/workspace"),
            created_at: Utc::now() - chrono::Duration::hours(1),
            last_used_at: Utc::now() - chrono::Duration::hours(1),
        };
        manager.save_session(&entry).unwrap();

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

        // Create a session exactly 24 hours old (boundary case)
        let boundary_entry = SessionEntry {
            session_id: SessionId::new("sess-boundary"),
            agent_id: "claude-acp".to_string(),
            workspace_hint: PathBuf::from("/tmp/workspace"),
            created_at: Utc::now() - chrono::Duration::hours(24),
            last_used_at: Utc::now() - chrono::Duration::hours(24),
        };
        manager.save_session(&boundary_entry).unwrap();

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

        // Create two sessions
        let entry1 = SessionEntry {
            session_id: SessionId::new("sess-001"),
            agent_id: "claude-acp".to_string(),
            workspace_hint: PathBuf::from("/tmp/ws1"),
            created_at: Utc::now(),
            last_used_at: Utc::now(),
        };
        manager.save_session(&entry1).unwrap();

        let entry2 = SessionEntry {
            session_id: SessionId::new("sess-002"),
            agent_id: "codex-acp".to_string(),
            workspace_hint: PathBuf::from("/tmp/ws2"),
            created_at: Utc::now(),
            last_used_at: Utc::now(),
        };
        manager.save_session(&entry2).unwrap();

        // Delete one session
        manager.delete_session(&SessionId::new("sess-001")).unwrap();

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
        assert!(path.file_name().unwrap() == "sessions.json");
    }
}
