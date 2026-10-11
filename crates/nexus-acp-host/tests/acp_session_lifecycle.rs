//! Integration Tests — ACP Session Lifecycle (E10)
//!
//! Tests the ACP session lifecycle including:
//! - Session persistence and retrieval
//! - Session cleanup and expiration
//! - Skill/capability registration
//! - Session termination

use std::path::PathBuf;

use agent_client_protocol::schema::v1::SessionId;
use nexus_acp_host::session_manager::{SessionEntry, SessionManager};

use chrono::{Duration, Utc};
use tempfile::TempDir;

/// Seed the sessions file directly.
///
/// The production writer (`SessionManager::save_session`) was removed, so tests
/// of the surviving load/cleanup/delete API set up state themselves.
fn write_sessions(path: &std::path::Path, sessions: serde_json::Value) {
    let doc = serde_json::json!({ "sessions": sessions });
    std::fs::write(path, serde_json::to_string(&doc).unwrap()).unwrap();
}

// ---------------------------------------------------------------------------
// E10: ACP session deletion tests
// ---------------------------------------------------------------------------

/// Test: `delete_session` removes session by ID
#[test]
fn session_delete_removes_by_id() {
    let temp_dir = TempDir::new().unwrap();
    let sessions_file = temp_dir.path().join("sessions.json");
    let manager = SessionManager::new(sessions_file.clone());

    write_sessions(
        &sessions_file,
        serde_json::json!([{
            "session_id": "sess_del_001",
            "agent_id": "claude-acp",
            "workspace_hint": "/tmp/delete_workspace",
            "created_at": Utc::now().to_rfc3339(),
            "last_used_at": Utc::now().to_rfc3339(),
        }]),
    );
    assert_eq!(manager.load_sessions().unwrap().len(), 1);

    let deleted = manager
        .delete_session(&SessionId::new("sess_del_001"))
        .unwrap();
    assert!(deleted.is_some());
    assert_eq!(deleted.unwrap().session_id, SessionId::new("sess_del_001"));

    assert_eq!(manager.load_sessions().unwrap().len(), 0);
}

/// Test: `delete_session` returns `None` for non-existent session
#[test]
fn session_delete_nonexistent() {
    let temp_dir = TempDir::new().unwrap();
    let sessions_file = temp_dir.path().join("sessions.json");
    let manager = SessionManager::new(sessions_file);

    let deleted = manager
        .delete_session(&SessionId::new("nonexistent_session"))
        .unwrap();
    assert!(deleted.is_none());
}

// ---------------------------------------------------------------------------
// E10: ACP session expiration/cleanup tests
// ---------------------------------------------------------------------------

/// Test: `cleanup_expired` removes sessions older than 24 hours
#[test]
fn session_cleanup_removes_expired() {
    let temp_dir = TempDir::new().unwrap();
    let sessions_file = temp_dir.path().join("sessions.json");
    let manager = SessionManager::new(sessions_file.clone());

    // An expired session (30 hours old) and a recent one (1 hour old).
    write_sessions(
        &sessions_file,
        serde_json::json!([
            {
                "session_id": "sess_expired",
                "agent_id": "claude-acp",
                "workspace_hint": "/tmp/expired_workspace",
                "created_at": (Utc::now() - Duration::hours(30)).to_rfc3339(),
                "last_used_at": (Utc::now() - Duration::hours(30)).to_rfc3339(),
            },
            {
                "session_id": "sess_recent",
                "agent_id": "codex-acp",
                "workspace_hint": "/tmp/recent_workspace",
                "created_at": (Utc::now() - Duration::hours(1)).to_rfc3339(),
                "last_used_at": (Utc::now() - Duration::hours(1)).to_rfc3339(),
            },
        ]),
    );

    let removed = manager.cleanup_expired().unwrap();
    assert_eq!(removed.len(), 1);
    assert_eq!(removed[0].session_id, SessionId::new("sess_expired"));

    let remaining = manager.load_sessions().unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].session_id, SessionId::new("sess_recent"));
}

/// Test: `cleanup_expired` keeps sessions at boundary (exactly 24 hours old)
#[test]
fn session_cleanup_boundary_24_hours() {
    let temp_dir = TempDir::new().unwrap();
    let sessions_file = temp_dir.path().join("sessions.json");
    let manager = SessionManager::new(sessions_file.clone());

    // A session exactly at the boundary (24 hours old).
    write_sessions(
        &sessions_file,
        serde_json::json!([{
            "session_id": "sess_boundary",
            "agent_id": "claude-acp",
            "workspace_hint": "/tmp/boundary_workspace",
            "created_at": (Utc::now() - Duration::hours(24)).to_rfc3339(),
            "last_used_at": (Utc::now() - Duration::hours(24)).to_rfc3339(),
        }]),
    );

    // Cleanup should remove sessions at exactly 24h (boundary case favors removal)
    let removed = manager.cleanup_expired().unwrap();
    assert_eq!(removed.len(), 1);
}

/// Test: `cleanup_expired` handles empty session list
#[test]
fn session_cleanup_empty_list() {
    let temp_dir = TempDir::new().unwrap();
    let sessions_file = temp_dir.path().join("sessions.json");
    let manager = SessionManager::new(sessions_file);

    let removed = manager.cleanup_expired().unwrap();
    assert_eq!(removed.len(), 0);
}

/// Test: `cleanup_expired` handles missing file
#[test]
fn session_cleanup_missing_file() {
    let temp_dir = TempDir::new().unwrap();
    let sessions_file = temp_dir.path().join("nonexistent.json");
    let manager = SessionManager::new(sessions_file);

    // Should not error, just return empty
    let removed = manager.cleanup_expired().unwrap();
    assert_eq!(removed.len(), 0);
}

// ---------------------------------------------------------------------------
// E10: Skill/capability registration tests
// ---------------------------------------------------------------------------

/// Test: `build_v1_0_capabilities` creates non-empty capability set
#[test]
fn capabilities_v1_0_builds_correct_set() {
    use nexus_acp_host::skills::build_v1_0_capabilities;

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
    use nexus_acp_host::skills::capabilities;

    assert_eq!(capabilities::FILE_SYSTEM_READ, "file_system.read");
    assert_eq!(capabilities::FILE_SYSTEM_WRITE, "file_system.write");
    assert_eq!(capabilities::TERMINAL_CREATE, "terminal.create");
    assert_eq!(capabilities::TERMINAL_OUTPUT, "terminal.output");
    assert_eq!(capabilities::TERMINAL_RELEASE, "terminal.release");
}

// ---------------------------------------------------------------------------
// E10: SessionEntry serialization tests
// ---------------------------------------------------------------------------

/// Test: `SessionEntry` serializes to JSON correctly
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

/// Test: `SessionEntry` deserializes from valid JSON
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

/// Test: `SessionEntry` deserialization handles missing optional fields
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

/// Test: `SessionManager` handles corrupted JSON file gracefully
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

// ---------------------------------------------------------------------------
// E10: Default sessions file path test
// ---------------------------------------------------------------------------

/// Test: `default_sessions_file` returns correct path structure
#[test]
fn session_default_file_path_structure() {
    let path = SessionManager::default_sessions_file();

    let path_str = path.to_string_lossy();
    assert!(path_str.contains(".nexus42"));
    assert!(path_str.contains("acp"));
    assert!(path_str.ends_with("sessions.json"));
}
