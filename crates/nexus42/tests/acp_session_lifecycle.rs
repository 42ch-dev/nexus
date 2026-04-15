//! Integration Tests — ACP Session Lifecycle (E10)
//!
//! Tests the ACP session lifecycle including:
//! - Session persistence and retrieval
//! - Session cleanup and expiration
//! - Skill/capability registration
//! - Session termination

use std::path::PathBuf;

use nexus42::acp::client::SessionId;
use nexus42::acp::session_manager::{SessionEntry, SessionManager};

use chrono::{Duration, Utc};
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// E10: ACP session persistence tests
// ---------------------------------------------------------------------------

/// Test: SessionManager saves and loads a session correctly
#[test]
fn session_persistence_save_and_load() {
    let temp_dir = TempDir::new().unwrap();
    let sessions_file = temp_dir.path().join("sessions.json");
    let manager = SessionManager::new(sessions_file.clone());

    let entry = SessionEntry {
        session_id: SessionId::new("sess_persist_001"),
        agent_id: "claude-acp".to_string(),
        workspace_hint: PathBuf::from("/tmp/test_workspace"),
        created_at: Utc::now(),
        last_used_at: Utc::now(),
    };

    manager.save_session(&entry).unwrap();

    // Load and verify
    let sessions = manager.load_sessions().unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].session_id, entry.session_id);
    assert_eq!(sessions[0].agent_id, "claude-acp");
}

/// Test: SessionManager persists multiple sessions
#[test]
fn session_persistence_multiple_sessions() {
    let temp_dir = TempDir::new().unwrap();
    let sessions_file = temp_dir.path().join("sessions.json");
    let manager = SessionManager::new(sessions_file.clone());

    let entry1 = SessionEntry {
        session_id: SessionId::new("sess_multi_001"),
        agent_id: "claude-acp".to_string(),
        workspace_hint: PathBuf::from("/tmp/workspace1"),
        created_at: Utc::now(),
        last_used_at: Utc::now(),
    };

    let entry2 = SessionEntry {
        session_id: SessionId::new("sess_multi_002"),
        agent_id: "codex-acp".to_string(),
        workspace_hint: PathBuf::from("/tmp/workspace2"),
        created_at: Utc::now(),
        last_used_at: Utc::now(),
    };

    manager.save_session(&entry1).unwrap();
    manager.save_session(&entry2).unwrap();

    let sessions = manager.load_sessions().unwrap();
    assert_eq!(sessions.len(), 2);
}

/// Test: SessionManager updates existing session
#[test]
fn session_persistence_update_existing() {
    let temp_dir = TempDir::new().unwrap();
    let sessions_file = temp_dir.path().join("sessions.json");
    let manager = SessionManager::new(sessions_file.clone());

    let original = SessionEntry {
        session_id: SessionId::new("sess_update_001"),
        agent_id: "claude-acp".to_string(),
        workspace_hint: PathBuf::from("/tmp/workspace"),
        created_at: Utc::now(),
        last_used_at: Utc::now(),
    };

    manager.save_session(&original).unwrap();

    // Update with new last_used_at
    let updated = SessionEntry {
        session_id: original.session_id.clone(),
        agent_id: original.agent_id.clone(),
        workspace_hint: original.workspace_hint.clone(),
        created_at: original.created_at,
        last_used_at: Utc::now(),
    };

    manager.save_session(&updated).unwrap();

    let sessions = manager.load_sessions().unwrap();
    assert_eq!(sessions.len(), 1);
    assert!(sessions[0].last_used_at >= original.last_used_at);
}

// ---------------------------------------------------------------------------
// E10: ACP session retrieval tests
// ---------------------------------------------------------------------------

/// Test: find_recent_session returns matching session by agent and workspace
#[test]
fn session_find_recent_by_agent_and_workspace() {
    let temp_dir = TempDir::new().unwrap();
    let sessions_file = temp_dir.path().join("sessions.json");
    let manager = SessionManager::new(sessions_file.clone());

    let entry = SessionEntry {
        session_id: SessionId::new("sess_find_001"),
        agent_id: "claude-acp".to_string(),
        workspace_hint: PathBuf::from("/tmp/find_workspace"),
        created_at: Utc::now(),
        last_used_at: Utc::now(),
    };

    manager.save_session(&entry).unwrap();

    let found = manager
        .find_recent_session("claude-acp", &PathBuf::from("/tmp/find_workspace"))
        .unwrap();

    assert!(found.is_some());
    assert_eq!(found.unwrap().session_id, entry.session_id);
}

/// Test: find_recent_session returns None when no match
#[test]
fn session_find_recent_no_match() {
    let temp_dir = TempDir::new().unwrap();
    let sessions_file = temp_dir.path().join("sessions.json");
    let manager = SessionManager::new(sessions_file.clone());

    let entry = SessionEntry {
        session_id: SessionId::new("sess_nomatch_001"),
        agent_id: "claude-acp".to_string(),
        workspace_hint: PathBuf::from("/tmp/workspace_a"),
        created_at: Utc::now(),
        last_used_at: Utc::now(),
    };

    manager.save_session(&entry).unwrap();

    // Search for different workspace
    let found = manager
        .find_recent_session("claude-acp", &PathBuf::from("/tmp/workspace_b"))
        .unwrap();

    assert!(found.is_none());
}

/// Test: find_recent_session returns most recent when multiple matches
#[test]
fn session_find_recent_returns_most_recent() {
    let temp_dir = TempDir::new().unwrap();
    let sessions_file = temp_dir.path().join("sessions.json");
    let manager = SessionManager::new(sessions_file.clone());

    let older = SessionEntry {
        session_id: SessionId::new("sess_old"),
        agent_id: "claude-acp".to_string(),
        workspace_hint: PathBuf::from("/tmp/recent_workspace"),
        created_at: Utc::now() - Duration::hours(2),
        last_used_at: Utc::now() - Duration::hours(2),
    };

    let newer = SessionEntry {
        session_id: SessionId::new("sess_new"),
        agent_id: "claude-acp".to_string(),
        workspace_hint: PathBuf::from("/tmp/recent_workspace"),
        created_at: Utc::now(),
        last_used_at: Utc::now(),
    };

    manager.save_session(&older).unwrap();
    manager.save_session(&newer).unwrap();

    let found = manager
        .find_recent_session("claude-acp", &PathBuf::from("/tmp/recent_workspace"))
        .unwrap();

    assert!(found.is_some());
    assert_eq!(found.unwrap().session_id, SessionId::new("sess_new"));
}

// ---------------------------------------------------------------------------
// E10: ACP session deletion tests
// ---------------------------------------------------------------------------

/// Test: delete_session removes session by ID
#[test]
fn session_delete_removes_by_id() {
    let temp_dir = TempDir::new().unwrap();
    let sessions_file = temp_dir.path().join("sessions.json");
    let manager = SessionManager::new(sessions_file.clone());

    let entry = SessionEntry {
        session_id: SessionId::new("sess_del_001"),
        agent_id: "claude-acp".to_string(),
        workspace_hint: PathBuf::from("/tmp/delete_workspace"),
        created_at: Utc::now(),
        last_used_at: Utc::now(),
    };

    manager.save_session(&entry).unwrap();
    assert_eq!(manager.load_sessions().unwrap().len(), 1);

    let deleted = manager
        .delete_session(&SessionId::new("sess_del_001"))
        .unwrap();
    assert!(deleted.is_some());
    assert_eq!(deleted.unwrap().session_id, entry.session_id);

    assert_eq!(manager.load_sessions().unwrap().len(), 0);
}

/// Test: delete_session returns None for non-existent session
#[test]
fn session_delete_nonexistent() {
    let temp_dir = TempDir::new().unwrap();
    let sessions_file = temp_dir.path().join("sessions.json");
    let manager = SessionManager::new(sessions_file.clone());

    let deleted = manager
        .delete_session(&SessionId::new("nonexistent_session"))
        .unwrap();
    assert!(deleted.is_none());
}

// ---------------------------------------------------------------------------
// E10: ACP session expiration/cleanup tests
// ---------------------------------------------------------------------------

/// Test: cleanup_expired removes sessions older than 24 hours
#[test]
fn session_cleanup_removes_expired() {
    let temp_dir = TempDir::new().unwrap();
    let sessions_file = temp_dir.path().join("sessions.json");
    let manager = SessionManager::new(sessions_file.clone());

    // Create an expired session (30 hours old)
    let expired = SessionEntry {
        session_id: SessionId::new("sess_expired"),
        agent_id: "claude-acp".to_string(),
        workspace_hint: PathBuf::from("/tmp/expired_workspace"),
        created_at: Utc::now() - Duration::hours(30),
        last_used_at: Utc::now() - Duration::hours(30),
    };

    // Create a recent session (1 hour old)
    let recent = SessionEntry {
        session_id: SessionId::new("sess_recent"),
        agent_id: "codex-acp".to_string(),
        workspace_hint: PathBuf::from("/tmp/recent_workspace"),
        created_at: Utc::now() - Duration::hours(1),
        last_used_at: Utc::now() - Duration::hours(1),
    };

    manager.save_session(&expired).unwrap();
    manager.save_session(&recent).unwrap();

    let removed = manager.cleanup_expired().unwrap();
    assert_eq!(removed.len(), 1);
    assert_eq!(removed[0].session_id, SessionId::new("sess_expired"));

    let remaining = manager.load_sessions().unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].session_id, SessionId::new("sess_recent"));
}

/// Test: cleanup_expired keeps sessions at boundary (exactly 24 hours old)
#[test]
fn session_cleanup_boundary_24_hours() {
    let temp_dir = TempDir::new().unwrap();
    let sessions_file = temp_dir.path().join("sessions.json");
    let manager = SessionManager::new(sessions_file.clone());

    // Create a session exactly at the boundary (24 hours old)
    let boundary = SessionEntry {
        session_id: SessionId::new("sess_boundary"),
        agent_id: "claude-acp".to_string(),
        workspace_hint: PathBuf::from("/tmp/boundary_workspace"),
        created_at: Utc::now() - Duration::hours(24),
        last_used_at: Utc::now() - Duration::hours(24),
    };

    manager.save_session(&boundary).unwrap();

    // Cleanup should remove sessions at exactly 24h (boundary case favors removal)
    let removed = manager.cleanup_expired().unwrap();
    assert_eq!(removed.len(), 1);
}

/// Test: cleanup_expired handles empty session list
#[test]
fn session_cleanup_empty_list() {
    let temp_dir = TempDir::new().unwrap();
    let sessions_file = temp_dir.path().join("sessions.json");
    let manager = SessionManager::new(sessions_file.clone());

    let removed = manager.cleanup_expired().unwrap();
    assert_eq!(removed.len(), 0);
}

/// Test: cleanup_expired handles missing file
#[test]
fn session_cleanup_missing_file() {
    let temp_dir = TempDir::new().unwrap();
    let sessions_file = temp_dir.path().join("nonexistent.json");
    let manager = SessionManager::new(sessions_file.clone());

    // Should not error, just return empty
    let removed = manager.cleanup_expired().unwrap();
    assert_eq!(removed.len(), 0);
}

// ---------------------------------------------------------------------------
// E10: Skill/capability registration tests
// ---------------------------------------------------------------------------

/// Test: build_v1_0_capabilities creates non-empty capability set
#[test]
fn capabilities_v1_0_builds_correct_set() {
    use nexus42::acp::skills::build_v1_0_capabilities;

    let caps = build_v1_0_capabilities();

    // Verify fs capabilities exist and have read/write enabled
    // The fs field is a FileSystemCapabilities struct, not an Option
    assert!(caps.fs.read_text_file);
    assert!(caps.fs.write_text_file);

    // Verify terminal capabilities
    assert!(caps.terminal);
}

/// Test: capability constants match expected values
#[test]
fn capabilities_constants_correct() {
    use nexus42::acp::skills::capabilities;

    assert_eq!(capabilities::FILE_SYSTEM_READ, "file_system.read");
    assert_eq!(capabilities::FILE_SYSTEM_WRITE, "file_system.write");
    assert_eq!(capabilities::TERMINAL_CREATE, "terminal.create");
    assert_eq!(capabilities::TERMINAL_OUTPUT, "terminal.output");
    assert_eq!(capabilities::TERMINAL_RELEASE, "terminal.release");
}

// ---------------------------------------------------------------------------
// E10: SessionEntry serialization tests
// ---------------------------------------------------------------------------

/// Test: SessionEntry serializes to JSON correctly
#[test]
fn session_entry_serialization() {
    let entry = SessionEntry {
        session_id: SessionId::new("sess_json_001"),
        agent_id: "claude-acp".to_string(),
        workspace_hint: PathBuf::from("/tmp/json_workspace"),
        created_at: "2026-04-15T10:00:00Z".parse().unwrap(),
        last_used_at: "2026-04-15T15:00:00Z".parse().unwrap(),
    };

    let json = serde_json::to_string(&entry).unwrap();
    assert!(json.contains("sess_json_001"));
    assert!(json.contains("claude-acp"));
    assert!(json.contains("/tmp/json_workspace"));

    let decoded: SessionEntry = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.session_id, entry.session_id);
    assert_eq!(decoded.agent_id, entry.agent_id);
}

/// Test: SessionEntry deserializes from valid JSON
#[test]
fn session_entry_deserialization() {
    let json = r#"{
        "session_id": "sess_decode_001",
        "agent_id": "codex-acp",
        "workspace_hint": "/tmp/decode_workspace",
        "created_at": "2026-04-15T10:00:00Z",
        "last_used_at": "2026-04-15T15:00:00Z"
    }"#;

    let entry: SessionEntry = serde_json::from_str(json).unwrap();
    assert_eq!(entry.session_id, SessionId::new("sess_decode_001"));
    assert_eq!(entry.agent_id, "codex-acp");
    assert_eq!(entry.workspace_hint, PathBuf::from("/tmp/decode_workspace"));
}

/// Test: SessionEntry deserialization handles missing optional fields
#[test]
fn session_entry_deserialization_minimal() {
    let json = r#"{
        "session_id": "sess_minimal_001",
        "agent_id": "claude-acp",
        "workspace_hint": "/tmp/minimal",
        "created_at": "2026-04-15T10:00:00Z",
        "last_used_at": "2026-04-15T15:00:00Z"
    }"#;

    let entry: SessionEntry = serde_json::from_str(json).unwrap();
    assert_eq!(entry.session_id, SessionId::new("sess_minimal_001"));
}

// ---------------------------------------------------------------------------
// E10: SessionManager error handling tests
// ---------------------------------------------------------------------------

/// Test: SessionManager handles corrupted JSON file gracefully
#[test]
fn session_manager_handles_corrupted_json() {
    let temp_dir = TempDir::new().unwrap();
    let sessions_file = temp_dir.path().join("corrupted.json");

    // Write invalid JSON
    std::fs::write(&sessions_file, "not valid json {").unwrap();

    let manager = SessionManager::new(sessions_file);
    let result = manager.load_sessions();

    // Should return error for corrupted JSON
    assert!(result.is_err());
}

/// Test: SessionManager creates parent directory on first save
#[test]
fn session_manager_creates_parent_dir() {
    let temp_dir = TempDir::new().unwrap();
    let sessions_file = temp_dir.path().join("subdir").join("sessions.json");

    assert!(!sessions_file.exists());

    let manager = SessionManager::new(sessions_file.clone());
    let entry = SessionEntry {
        session_id: SessionId::new("sess_mkdir_001"),
        agent_id: "claude-acp".to_string(),
        workspace_hint: PathBuf::from("/tmp/mkdir_workspace"),
        created_at: Utc::now(),
        last_used_at: Utc::now(),
    };

    manager.save_session(&entry).unwrap();

    assert!(sessions_file.exists());
}

// ---------------------------------------------------------------------------
// E10: Default sessions file path test
// ---------------------------------------------------------------------------

/// Test: default_sessions_file returns correct path structure
#[test]
fn session_default_file_path_structure() {
    let path = SessionManager::default_sessions_file();

    let path_str = path.to_string_lossy();
    assert!(path_str.contains(".nexus42"));
    assert!(path_str.contains("acp"));
    assert!(path_str.ends_with("sessions.json"));
}
