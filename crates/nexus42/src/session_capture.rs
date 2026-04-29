//! Session-end capture for memory pipeline.
//!
//! Collects session context when an ACP session ends and submits
//! a `PendingReviewRecord` to the daemon API for later review/promotion.
//!
//! See creator-memory-soul-lifecycle-v1.md §6.2.

// These functions are public API for future integration - not yet wired into session flow
#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tracing::{debug, info, warn};

use crate::api::daemon_client::DaemonClient;

/// Structured raw digest for session-end capture.
///
/// Contains session statistics extracted from the ACP interaction.
/// This is stored in `raw_digest` as a JSON string.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionDigest {
    /// Session duration in seconds.
    pub duration_secs: u64,
    /// Number of user/assistant message exchanges.
    pub message_count: usize,
    /// Number of tool calls made.
    pub tool_calls: usize,
    /// Last context snippet (truncated to 200 chars).
    pub last_context: String,
}

impl SessionDigest {
    /// Create a new session digest with collected metrics.
    #[must_use]
    pub fn new(
        duration_secs: u64,
        message_count: usize,
        tool_calls: usize,
        last_context: &str,
    ) -> Self {
        // Truncate last_context to 200 chars (char-safe for UTF-8)
        let last_context = if last_context.chars().count() > 200 {
            let truncated: String = last_context.chars().take(197).collect();
            format!("{truncated}...")
        } else {
            last_context.to_string()
        };

        Self {
            duration_secs,
            message_count,
            tool_calls,
            last_context,
        }
    }

    /// Render digest as JSON string for storage in `raw_digest` field.
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// Request body for daemon API create pending review.
#[derive(Debug, Serialize)]
struct CreatePendingReviewRequest {
    pending_id: String,
    session_id: String,
    creator_id: String,
    world_id: Option<String>,
    task_kind: Option<String>,
    raw_digest: String,
    created_at: Option<String>,
}

/// Response body for daemon API create pending review.
#[derive(Debug, Deserialize)]
struct CreatePendingReviewResponse {
    #[allow(dead_code)]
    success: bool,
    #[allow(dead_code)]
    pending_id: String,
}

/// Session capture state for tracking metrics.
#[derive(Debug)]
pub struct SessionCapture {
    /// Session ID.
    session_id: String,
    /// Agent ID (used for `task_kind` heuristic).
    agent_id: String,
    /// Creator ID.
    creator_id: String,
    /// Optional world ID.
    world_id: Option<String>,
    /// Session start time.
    start_time: Instant,
    /// Message count.
    message_count: usize,
    /// Tool call count.
    tool_calls: usize,
    /// Last context snippet.
    last_context: String,
}

impl SessionCapture {
    /// Create a new session capture tracker.
    #[must_use]
    pub fn new(
        session_id: String,
        agent_id: String,
        creator_id: String,
        world_id: Option<String>,
    ) -> Self {
        Self {
            session_id,
            agent_id,
            creator_id,
            world_id,
            start_time: Instant::now(),
            message_count: 0,
            tool_calls: 0,
            last_context: String::new(),
        }
    }

    /// Record a message exchange.
    pub const fn record_message(&mut self) {
        self.message_count += 1;
    }

    /// Record a tool call.
    pub const fn record_tool_call(&mut self) {
        self.tool_calls += 1;
    }

    /// Update the last context snippet.
    pub fn update_context(&mut self, context: &str) {
        self.last_context = context.to_string();
    }

    /// Capture session-end data and build a `PendingReviewRecord`.
    ///
    /// Returns the structured digest ready for submission to daemon.
    #[must_use]
    pub fn capture_session_end(&self) -> SessionDigest {
        let duration_secs = self.start_time.elapsed().as_secs();

        SessionDigest::new(
            duration_secs,
            self.message_count,
            self.tool_calls,
            &self.last_context,
        )
    }

    /// Determine task kind heuristic from `agent_id`.
    ///
    /// Agent IDs may contain hints like:
    /// - "brainstorm-agent" → "brainstorm"
    /// - "outline" → "outline"
    /// - "chapter-writer" → "chapter"
    /// - "research" → "research"
    ///
    /// Default is "unknown".
    #[must_use]
    pub fn detect_task_kind(&self) -> String {
        let agent_lower = self.agent_id.to_lowercase();

        if agent_lower.contains("brainstorm") {
            return "brainstorm".to_string();
        }
        if agent_lower.contains("outline") {
            return "outline".to_string();
        }
        if agent_lower.contains("chapter") {
            return "chapter".to_string();
        }
        if agent_lower.contains("research") {
            return "research".to_string();
        }

        "unknown".to_string()
    }

    /// Submit pending review to daemon API.
    ///
    /// This is a fire-and-forget operation — if the daemon is unavailable,
    /// log a warning and continue without blocking session shutdown.
    ///
    /// The timeout is intentionally short (5s) to avoid delaying shutdown.
    ///
    /// If `pending_id` is provided, it is used as-is; otherwise a new one is
    /// generated internally. This allows callers to guarantee the same ID is
    /// used for both daemon submission and local fallback.
    pub async fn submit_to_daemon(
        &self,
        daemon_client: &DaemonClient,
        digest: &SessionDigest,
        pending_id: Option<&str>,
    ) -> (bool, String) {
        let pending_id = pending_id.map_or_else(
            || {
                format!(
                    "pending_{}",
                    uuid::Uuid::new_v4().to_string().replace('-', "")
                )
            },
            ToString::to_string,
        );
        let task_kind = self.detect_task_kind();
        let raw_digest = digest.to_json();
        let created_at = chrono::Utc::now().to_rfc3339();

        let request = CreatePendingReviewRequest {
            pending_id: pending_id.clone(),
            session_id: self.session_id.clone(),
            creator_id: self.creator_id.clone(),
            world_id: self.world_id.clone(),
            task_kind: Some(task_kind),
            raw_digest,
            created_at: Some(created_at),
        };

        debug!(
            session_id = %self.session_id,
            pending_id = %request.pending_id,
            "Submitting session-end capture to daemon"
        );

        // Use short timeout client for fire-and-forget
        let timeout_client = DaemonClient::with_timeouts(
            daemon_client.base_url(),
            Duration::from_secs(2),
            Duration::from_secs(5),
        );

        let result: Result<CreatePendingReviewResponse, crate::errors::CliError> = timeout_client
            .post("/v1/local/memory/pending-review", &request)
            .await;

        match result {
            Ok(response) => {
                debug!(
                    session_id = %self.session_id,
                    pending_id = %response.pending_id,
                    "Session-end capture submitted successfully"
                );
                (true, pending_id)
            }
            Err(e) => {
                warn!(
                    session_id = %self.session_id,
                    error = %e,
                    "Failed to submit session-end capture — daemon unavailable, continuing"
                );
                (false, pending_id)
            }
        }
    }

    /// Get the session ID.
    #[must_use]
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    /// Get the creator ID.
    #[must_use]
    pub fn creator_id(&self) -> &str {
        &self.creator_id
    }
}

/// Serialized pending capture record for local file fallback.
///
/// When the daemon is unavailable, session capture data is persisted to
/// `~/.nexus42/pending_captures/<pending_id>.json` for later processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingCaptureFile {
    /// Unique pending ID.
    pub pending_id: String,
    /// Session ID.
    pub session_id: String,
    /// Creator ID.
    pub creator_id: String,
    /// Optional world ID.
    pub world_id: Option<String>,
    /// Task kind heuristic.
    pub task_kind: String,
    /// The raw session digest JSON.
    pub raw_digest: String,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
}

/// Save a pending capture to a specified directory.
///
/// Creates the directory if it doesn't exist.
/// Returns `Ok(path)` on success, `Err` if file I/O fails.
///
/// # Errors
///
/// Returns an I/O error if directory creation or file write fails.
pub fn save_capture_to_dir(
    dir: &std::path::Path,
    pending_id: &str,
    capture: &SessionCapture,
    digest: &SessionDigest,
) -> std::result::Result<PathBuf, std::io::Error> {
    std::fs::create_dir_all(dir)?;

    let file = PendingCaptureFile {
        pending_id: pending_id.to_string(),
        session_id: capture.session_id.clone(),
        creator_id: capture.creator_id.clone(),
        world_id: capture.world_id.clone(),
        task_kind: capture.detect_task_kind(),
        raw_digest: digest.to_json(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    let file_path = dir.join(format!("{pending_id}.json"));
    let json = serde_json::to_string_pretty(&file)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;

    std::fs::write(&file_path, json)?;
    Ok(file_path)
}

/// Save a pending capture to local file when daemon is unavailable.
///
/// Creates `~/.nexus42/pending_captures/` directory if it doesn't exist.
/// Returns `Ok(path)` on success, `Err` if file I/O fails.
///
/// # Errors
///
/// Returns an I/O error if nexus home cannot be resolved or file write fails.
pub fn save_capture_locally(
    pending_id: &str,
    capture: &SessionCapture,
    digest: &SessionDigest,
) -> std::result::Result<PathBuf, std::io::Error> {
    let nexus_home = crate::config::nexus_home()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::NotFound, e.to_string()))?;

    let pending_dir = nexus_home.join("pending_captures");
    save_capture_to_dir(&pending_dir, pending_id, capture, digest)
}

/// Resolve the pending captures directory path.
///
/// Returns `None` if nexus home cannot be determined.
#[must_use]
pub fn pending_captures_dir() -> Option<PathBuf> {
    crate::config::nexus_home()
        .ok()
        .map(|home| home.join("pending_captures"))
}

/// Fire-and-forget session capture submitter.
///
/// Spawns a background task to submit the capture without blocking.
/// If the daemon is unavailable, persists to a local file for later processing.
///
/// The `pending_id` is generated once and used for both daemon submission
/// and local fallback, ensuring consistent identity.
pub fn spawn_submit_capture(
    daemon_client: DaemonClient,
    capture: SessionCapture,
    digest: SessionDigest,
) {
    tokio::spawn(async move {
        let pending_id = format!(
            "pending_{}",
            uuid::Uuid::new_v4().to_string().replace('-', "")
        );

        let (success, returned_id) = capture
            .submit_to_daemon(&daemon_client, &digest, Some(&pending_id))
            .await;

        if !success {
            // Daemon unavailable — persist locally for later processing (R8)
            // Use the same pending_id that was sent to the daemon
            match save_capture_locally(&returned_id, &capture, &digest) {
                Ok(path) => {
                    info!(
                        session_id = %capture.session_id(),
                        pending_id = %returned_id,
                        path = %path.display(),
                        "Session capture persisted locally — daemon was unavailable"
                    );
                }
                Err(e) => {
                    warn!(
                        session_id = %capture.session_id(),
                        pending_id = %returned_id,
                        error = %e,
                        "Failed to persist session capture locally — data may be lost"
                    );
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_digest_new_truncates_long_context() {
        // Create a context that exceeds 200 chars
        let long_context = "This is a very long context that exceeds the maximum length of 200 characters and should be truncated properly with an ellipsis at the end. We need to make sure this string is at least 201 characters long so that the truncation logic kicks in and adds the ellipsis marker to indicate truncation occurred.";
        assert!(long_context.len() > 200, "Test context must be > 200 chars");
        let digest = SessionDigest::new(60, 10, 5, long_context);

        assert!(digest.last_context.len() <= 200);
        assert!(digest.last_context.ends_with("..."));
    }

    #[test]
    fn session_digest_to_json_produces_valid_json() {
        let digest = SessionDigest::new(120, 15, 3, "Test context");
        let json = digest.to_json();

        let parsed: SessionDigest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.duration_secs, 120);
        assert_eq!(parsed.message_count, 15);
        assert_eq!(parsed.tool_calls, 3);
        assert_eq!(parsed.last_context, "Test context");
    }

    #[test]
    fn session_capture_detects_task_kind_from_agent_id() {
        let capture = SessionCapture::new(
            "sess_test".to_string(),
            "brainstorm-agent".to_string(),
            "ctr_test".to_string(),
            None,
        );
        assert_eq!(capture.detect_task_kind(), "brainstorm");

        let capture = SessionCapture::new(
            "sess_test".to_string(),
            "outline-writer".to_string(),
            "ctr_test".to_string(),
            None,
        );
        assert_eq!(capture.detect_task_kind(), "outline");

        let capture = SessionCapture::new(
            "sess_test".to_string(),
            "random-agent".to_string(),
            "ctr_test".to_string(),
            None,
        );
        assert_eq!(capture.detect_task_kind(), "unknown");
    }

    #[test]
    fn session_capture_records_metrics() {
        let mut capture = SessionCapture::new(
            "sess_test".to_string(),
            "agent".to_string(),
            "ctr_test".to_string(),
            None,
        );

        capture.record_message();
        capture.record_message();
        capture.record_tool_call();
        capture.update_context("New context");

        assert_eq!(capture.message_count, 2);
        assert_eq!(capture.tool_calls, 1);
        assert_eq!(capture.last_context, "New context");
    }

    #[tokio::test]
    async fn submit_to_daemon_returns_false_on_connection_refused() {
        let capture = SessionCapture::new(
            "sess_test".to_string(),
            "agent".to_string(),
            "ctr_test".to_string(),
            None,
        );
        let digest = SessionDigest::new(60, 5, 2, "Context");

        // Use a port that nothing is listening on
        let daemon_client = DaemonClient::with_timeouts(
            "http://127.0.0.1:19999",
            Duration::from_millis(100),
            Duration::from_millis(200),
        );

        let (success, _pending_id) = capture
            .submit_to_daemon(&daemon_client, &digest, None)
            .await;
        assert!(!success);
    }

    #[test]
    fn save_capture_to_dir_writes_valid_json() {
        let capture = SessionCapture::new(
            "sess_test".to_string(),
            "brainstorm-agent".to_string(),
            "ctr_test".to_string(),
            Some("wld_testworld".to_string()),
        );
        let digest = SessionDigest::new(120, 15, 3, "Test context");

        let tmp = tempfile::tempdir().expect("failed to create temp dir");
        let pending_id = "pending_test123";

        let path =
            save_capture_to_dir(tmp.path(), pending_id, &capture, &digest).expect("save failed");

        // File should exist
        assert!(path.exists());

        // File should end with pending_id.json
        assert!(path
            .file_name()
            .expect("no file name")
            .to_str()
            .expect("non-utf8")
            .starts_with(pending_id));

        // Content should be valid JSON and deserialize back
        let content = std::fs::read_to_string(&path).expect("failed to read file");
        let parsed: PendingCaptureFile =
            serde_json::from_str(&content).expect("file should contain valid JSON");

        assert_eq!(parsed.session_id, "sess_test");
        assert_eq!(parsed.creator_id, "ctr_test");
        assert_eq!(parsed.world_id, Some("wld_testworld".to_string()));
        assert_eq!(parsed.task_kind, "brainstorm");
        assert!(!parsed.raw_digest.is_empty());
        assert!(!parsed.created_at.is_empty());
    }

    #[test]
    fn save_capture_to_dir_creates_directory() {
        let capture = SessionCapture::new(
            "sess_test".to_string(),
            "agent".to_string(),
            "ctr_test".to_string(),
            None,
        );
        let digest = SessionDigest::new(60, 5, 2, "Context");

        let tmp = tempfile::tempdir().expect("failed to create temp dir");
        let nested = tmp.path().join("a").join("b").join("c");

        let result = save_capture_to_dir(&nested, "pending_nested", &capture, &digest);

        assert!(
            result.is_ok(),
            "should create nested directories: {result:?}"
        );
        assert!(result.unwrap().exists());
    }
}
