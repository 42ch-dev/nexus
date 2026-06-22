//! Workspace session management (DF-31 full, V1.56 P0).
//!
//! DB-backed session store for `workspace.open` / `workspace.commit` with
//! file-level optimistic concurrency control (content hash) and changes[] manifest validation.
//! `SQLite`, survive daemon restarts, and expire per TTL.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use nexus_local_db as db;
use sqlx::SqlitePool;
use tracing;

// ── Public types ────────────────────────────────────────────────────────────

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

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

/// Typed error for session operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SessionError {
    /// The requested session ID does not exist in the session store.
    NotFound(SessionId),
    /// The session has already been consumed (committed) and is now stale.
    AlreadyCommitted(SessionId),
    /// The session has exceeded its time-to-live.
    Expired(SessionId),
    /// The changes[] manifest does not match the session snapshot (OCC conflict).
    HashConflict {
        session_id: SessionId,
        path: String,
        expected_hash: String,
        actual_hash: String,
    },
    /// A database error occurred during session operations.
    Database(String),
    /// An I/O error occurred during file operations.
    Io(String),
    /// V1.58 P0 T2 (R-V156P0-M002): the resolved path escapes the canonical
    /// workspace root (symlink traversal or prefix-boundary violation).
    PathEscape {
        path: String,
        workspace_root: String,
    },
}

impl fmt::Display for SessionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(id) => write!(f, "session {id} not found"),
            Self::AlreadyCommitted(id) => {
                write!(f, "session {id} has already been committed (stale session)")
            }
            Self::Expired(id) => write!(f, "session {id} has expired"),
            Self::HashConflict {
                session_id,
                path,
                expected_hash,
                actual_hash,
            } => {
                write!(
                    f,
                    "content hash conflict for {path} in session {session_id}: \
                     expected {expected_hash}, got {actual_hash}"
                )
            }
            Self::Database(msg) => write!(f, "session database error: {msg}"),
            Self::Io(msg) => write!(f, "session I/O error: {msg}"),
            Self::PathEscape { path, workspace_root } => {
                write!(
                    f,
                    "path '{path}' escapes canonical workspace root '{workspace_root}'"
                )
            }
        }
    }
}

/// Operation type for a change in the commit manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeOp {
    Create,
    Modify,
    Delete,
}

/// A single change entry in the `changes[]` manifest.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangeEntry {
    /// Relative path within the workspace for this change.
    pub path: String,
    /// SHA-256 hex digest of the file content *before* the change.
    pub content_hash: String,
    /// Operation type.
    pub op: ChangeOp,
}

/// Snapshot of files at session open time, keyed by relative path → SHA-256 hex.
#[derive(Debug, Clone, Default)]
pub struct FileSnapshots {
    pub hashes: std::collections::HashMap<String, String>,
}

// ── Content hashing ─────────────────────────────────────────────────────────

/// Canonicalize a workspace root path (V1.58 P0 T2 — R-V156P0-M002).
///
/// Wraps `std::fs::canonicalize` to resolve symlinks and relative components
/// so the prefix-boundary check in [`enforce_path_boundary`] is robust.
///
/// # Errors
///
/// Returns [`SessionError::Io`] if canonicalization fails (missing path,
/// permission denied, etc.).
pub fn canonicalize_workspace_root(root: &Path) -> Result<PathBuf, SessionError> {
    std::fs::canonicalize(root).map_err(|e| SessionError::Io(e.to_string()))
}

/// Enforce that `target` lies within the canonical `workspace_root` prefix
/// (V1.58 P0 T2 — R-V156P0-M002).
///
/// Both paths must already be canonical. Returns `Ok(())` if `target` starts
/// with `workspace_root`, otherwise [`SessionError::PathEscape`]. This closes
/// the symlink-traversal gap that string-level `..` checks cannot detect.
pub fn enforce_path_boundary(
    target: &Path,
    workspace_root: &Path,
) -> Result<(), SessionError> {
    if target.starts_with(workspace_root) {
        Ok(())
    } else {
        Err(SessionError::PathEscape {
            path: target.to_string_lossy().to_string(),
            workspace_root: workspace_root.to_string_lossy().to_string(),
        })
    }
}

/// Compute SHA-256 content hashes for all regular files under `root`.
///
/// Walks the directory tree and returns a map of relative path → hex digest.
/// Symlinks, directories, and non-regular files are skipped. Symlinks are
/// explicitly rejected via `symlink_metadata` so a symlink chain cannot
/// escape the workspace root (V1.58 P0 T2 — R-V156P0-M002).
///
/// Uses `tokio::fs` so the function is safe to call from async context
/// (V1.58 P0 T4 — R-V156P0-M004).
///
/// # Errors
///
/// Returns [`SessionError::Io`] if file I/O operations fail (e.g., permission denied).
pub async fn compute_content_hashes(root: &Path) -> Result<FileSnapshots, SessionError> {
    let mut hashes = std::collections::HashMap::new();
    if !root.exists() {
        return Ok(FileSnapshots { hashes });
    }

    compute_content_hashes_inner(root, root, &mut hashes).await?;
    Ok(FileSnapshots { hashes })
}

/// Recursive worker for [`compute_content_hashes`].
async fn compute_content_hashes_inner(
    dir: &Path,
    root: &Path,
    hashes: &mut std::collections::HashMap<String, String>,
) -> Result<(), SessionError> {
    use sha2::{Digest, Sha256};
    use tokio::io::AsyncReadExt;

    let mut entries = tokio::fs::read_dir(dir)
        .await
        .map_err(|e| SessionError::Io(e.to_string()))?;

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| SessionError::Io(e.to_string()))?
    {
        let path = entry.path();
        let relative = match path.strip_prefix(root) {
            Ok(r) => r.to_string_lossy().to_string(),
            Err(_) => continue,
        };

        // V1.58 P0 T2: reject symlinks explicitly — do not follow them.
        // `symlink_metadata` does not traverse symlinks, so a symlink shows
        // up as a symlink rather than its target.
        let meta = tokio::fs::symlink_metadata(&path)
            .await
            .map_err(|e| SessionError::Io(e.to_string()))?;
        if meta.file_type().is_symlink() {
            // Skip symlinks — they could escape the workspace root.
            continue;
        }

        if meta.is_dir() {
            // Recurse into subdirectories.
            Box::pin(compute_content_hashes_inner(&path, root, hashes)).await?;
        } else if meta.is_file() {
            let mut file = tokio::fs::File::open(&path)
                .await
                .map_err(|e| SessionError::Io(e.to_string()))?;
            let mut sha = Sha256::new();
            let mut buffer = [0u8; 8192];
            loop {
                let n = file
                    .read(&mut buffer)
                    .await
                    .map_err(|e| SessionError::Io(e.to_string()))?;
                if n == 0 {
                    break;
                }
                sha.update(&buffer[..n]);
            }
            let digest = sha.finalize();
            hashes.insert(relative, format!("{digest:x}"));
        }
    }

    Ok(())
}

// ── Workspace session manager (DB-backed) ───────────────────────────────────

/// DB-backed workspace session manager.
///
/// Replaces the V1.55 in-memory `WorkspaceSessionManager`.
/// Sessions are persisted in `SQLite`, survive daemon restart, and expire per TTL.
///
/// # Conflict model (DF-31 full OCC)
///
/// - `workspace.open`: Scans files in the workspace scope, computes SHA-256 content
///   hashes for each file, stores them as part of the session snapshot in the DB.
/// - `workspace.commit`: Validates the `changes[]` manifest against the session
///   snapshot. Each change entry must reference a file whose current content hash
///   matches the hash stored in the session. On mismatch, rejects with
///   [`SessionError::HashConflict`].
/// - The session is atomically consumed (marked `consumed = 1`) only if all
///   change entries validate. This guarantees single-consumer semantics.
pub struct WorkspaceSessionManager {
    pool: Arc<SqlitePool>,
}

impl WorkspaceSessionManager {
    /// Default session TTL (5 minutes).
    pub const DEFAULT_TTL_SECS: i64 = 300;

    /// Create a new session manager backed by the given database pool.
    #[must_use]
    pub const fn new(pool: Arc<SqlitePool>) -> Self {
        Self { pool }
    }

    /// Open a new workspace session.
    ///
    /// Scans files under the target directory and computes SHA-256 content hashes.
    /// Creates a session row in the database with the snapshot.
    ///
    /// # Errors
    ///
    /// Returns `SessionError` if file I/O or database operations fail.
    pub async fn open_session(
        &self,
        workspace_root: &str,
        relative_path: &str,
        existed: bool,
    ) -> Result<SessionId, SessionError> {
        // Cleanup expired sessions first
        let _ = db::cleanup_expired_sessions(&self.pool).await;

        // V1.58 P0 T2 (R-V156P0-M002): canonicalize the workspace root so the
        // prefix-boundary check is robust against symlinks and relative
        // components. Compute the canonical target and enforce it stays
        // within the workspace root before hashing anything.
        let canonical_root = canonicalize_workspace_root(Path::new(workspace_root))?;
        let target_path = canonical_root.join(relative_path);
        let canonical_target = if target_path.exists() {
            std::fs::canonicalize(&target_path).map_err(|e| SessionError::Io(e.to_string()))?
        } else {
            // Path may not exist yet (Create op) — enforce boundary on the
            // logical join against the canonical root.
            target_path.clone()
        };
        enforce_path_boundary(&canonical_target, &canonical_root)?;
        let workspace_root_canonical = canonical_root.to_string_lossy().to_string();

        // Compute content hashes if the path exists (async — V1.58 P0 T4).
        let file_hashes = if existed && canonical_target.is_dir() {
            compute_content_hashes(&canonical_target).await?
        } else {
            FileSnapshots::default()
        };

        let file_hashes_json =
            serde_json::to_string(&file_hashes.hashes).unwrap_or_else(|_| "{}".to_string());

        let session_id = SessionId::new();

        db::create_session(
            &self.pool,
            &db::CreateSessionParams {
                session_id: session_id.to_string(),
                workspace_root: workspace_root_canonical,
                relative_path: relative_path.to_string(),
                existed,
                file_hashes_json,
                ttl_secs: Self::DEFAULT_TTL_SECS,
            },
        )
        .await
        .map_err(|e| SessionError::Database(e.to_string()))?;

        tracing::info!(
            session_id = %session_id,
            workspace_root = %workspace_root,
            relative_path = %relative_path,
            "Workspace session opened (DB-backed)"
        );

        Ok(session_id)
    }

    /// Validate that a session exists and is usable.
    ///
    /// # Errors
    ///
    /// Returns `SessionError` if the session is not found, consumed, or expired.
    pub async fn validate_session(
        &self,
        session_id: &SessionId,
    ) -> Result<db::WorkspaceSessionRow, SessionError> {
        let row = db::get_session(&self.pool, &session_id.to_string())
            .await
            .map_err(|e| SessionError::Database(e.to_string()))?
            .ok_or_else(|| SessionError::NotFound(session_id.clone()))?;

        if row.consumed {
            return Err(SessionError::AlreadyCommitted(session_id.clone()));
        }

        // Use `SQLite` datetime comparison to check expiry — avoids timezone
        // format mismatch between chrono::Utc and `SQLite` datetime strings.
        // SAFETY: compile-time checked — simple COUNT query with parameters.
        let sid = session_id.to_string();
        let active = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM workspace_sessions \
             WHERE session_id = ? AND consumed = 0 AND expires_at > strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
            sid
        )
        .fetch_one(self.pool.as_ref())
        .await
        .map_err(|e| SessionError::Database(e.to_string()))?;

        if active == 0 {
            return Err(SessionError::Expired(session_id.clone()));
        }

        Ok(row)
    }

    /// Validate a `changes[]` manifest against the session snapshot.
    ///
    /// For each change entry with `op: Modify`, verifies that the file's current
    /// content hash matches the hash stored in the session snapshot. For `Create`,
    /// the file must not exist in the snapshot. For `Delete`, the file must exist
    /// in the snapshot.
    ///
    /// # Errors
    ///
    /// Returns `SessionError::HashConflict` if any validation fails.
    pub async fn validate_changes_manifest(
        &self,
        session_id: &SessionId,
        changes: &[ChangeEntry],
        workspace_root: &str,
    ) -> Result<(), SessionError> {
        let row = self.validate_session(session_id).await?;

        // Parse stored file hashes
        let stored_hashes: std::collections::HashMap<String, String> =
            serde_json::from_str(&row.file_hashes_json).unwrap_or_default();

        for change in changes {
            let file_path = Path::new(workspace_root).join(&change.path);

            // V1.58 P0 T2 (R-V156P0-M002): enforce the file stays within the
            // workspace root after resolving any symlinks.
            if file_path.exists() {
                let canonical_file =
                    std::fs::canonicalize(&file_path).map_err(|e| SessionError::Io(e.to_string()))?;
                let canonical_root = canonicalize_workspace_root(Path::new(workspace_root))?;
                enforce_path_boundary(&canonical_file, &canonical_root)?;
            }

            match change.op {
                ChangeOp::Modify => {
                    // File must exist in the snapshot
                    let stored_hash = stored_hashes.get(&change.path).ok_or_else(|| {
                        SessionError::HashConflict {
                            session_id: session_id.clone(),
                            path: change.path.clone(),
                            expected_hash: "present-in-snapshot".to_string(),
                            actual_hash: "not-in-snapshot".to_string(),
                        }
                    })?;

                    // Verify the current file hash matches the stored hash
                    if !file_path.exists() {
                        return Err(SessionError::HashConflict {
                            session_id: session_id.clone(),
                            path: change.path.clone(),
                            expected_hash: stored_hash.clone(),
                            actual_hash: "file-not-found".to_string(),
                        });
                    }

                    let current_hash = compute_single_file_hash(&file_path).await?;
                    if current_hash != *stored_hash {
                        return Err(SessionError::HashConflict {
                            session_id: session_id.clone(),
                            path: change.path.clone(),
                            expected_hash: stored_hash.clone(),
                            actual_hash: current_hash,
                        });
                    }

                    // Verify the provided content_hash matches
                    if change.content_hash != *stored_hash {
                        return Err(SessionError::HashConflict {
                            session_id: session_id.clone(),
                            path: change.path.clone(),
                            expected_hash: stored_hash.clone(),
                            actual_hash: change.content_hash.clone(),
                        });
                    }
                }
                ChangeOp::Create => {
                    // File must NOT exist in the snapshot
                    if stored_hashes.contains_key(&change.path) {
                        return Err(SessionError::HashConflict {
                            session_id: session_id.clone(),
                            path: change.path.clone(),
                            expected_hash: "not-in-snapshot".to_string(),
                            actual_hash: "already-tracked".to_string(),
                        });
                    }
                }
                ChangeOp::Delete => {
                    // File must exist in the snapshot
                    if !stored_hashes.contains_key(&change.path) {
                        return Err(SessionError::HashConflict {
                            session_id: session_id.clone(),
                            path: change.path.clone(),
                            expected_hash: "present-in-snapshot".to_string(),
                            actual_hash: "not-in-snapshot".to_string(),
                        });
                    }
                }
            }
        }

        Ok(())
    }

    /// Consume a session — mark it as committed.
    ///
    /// Atomically validates and consumes the session. Two concurrent calls
    /// with the same session ID cannot both succeed — `SQLite`'s UPDATE WHERE
    /// guarantees single-consumer semantics.
    ///
    /// # Errors
    ///
    /// Returns `SessionError` if the session is not found, consumed, or expired.
    pub async fn consume_session(
        &self,
        session_id: &SessionId,
    ) -> Result<db::WorkspaceSessionRow, SessionError> {
        let result = db::consume_session(&self.pool, &session_id.to_string())
            .await
            .map_err(|e| SessionError::Database(e.to_string()))?;

        match result {
            db::ConsumeResult::Consumed(row) => {
                tracing::info!(session_id = %session_id, "Session consumed (committed)");
                Ok(row)
            }
            db::ConsumeResult::NotFound => Err(SessionError::NotFound(session_id.clone())),
            db::ConsumeResult::AlreadyConsumed => {
                Err(SessionError::AlreadyCommitted(session_id.clone()))
            }
            db::ConsumeResult::Expired => Err(SessionError::Expired(session_id.clone())),
        }
    }

    /// Get the underlying database pool.
    #[must_use]
    pub fn pool(&self) -> Arc<SqlitePool> {
        Arc::clone(&self.pool)
    }
}

/// Compute SHA-256 hash for a single file.
///
/// Uses `tokio::fs` so the function is safe to call from async context
/// (V1.58 P0 T4 — R-V156P0-M004).
async fn compute_single_file_hash(path: &Path) -> Result<String, SessionError> {
    use sha2::{Digest, Sha256};
    use tokio::io::AsyncReadExt;

    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|e| SessionError::Io(e.to_string()))?;
    let mut sha = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let n = file
            .read(&mut buffer)
            .await
            .map_err(|e| SessionError::Io(e.to_string()))?;
        if n == 0 {
            break;
        }
        sha.update(&buffer[..n]);
    }
    Ok(format!("{:x}", sha.finalize()))
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_id_uniqueness() {
        let id1 = SessionId::new();
        let id2 = SessionId::new();
        assert_ne!(id1.0, id2.0);
        assert!(id1.0.starts_with("ws_"));
    }

    #[tokio::test]
    async fn compute_content_hashes_empty_dir() {
        let dir = tempfile::tempdir().expect("tempdir");
        let result = compute_content_hashes(dir.path()).await.expect("compute");
        assert!(result.hashes.is_empty());
    }

    #[tokio::test]
    async fn compute_content_hashes_single_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file_path = dir.path().join("test.txt");
        std::fs::write(&file_path, b"hello world").expect("write");

        let result = compute_content_hashes(dir.path()).await.expect("compute");
        assert_eq!(result.hashes.len(), 1);
        assert!(result.hashes.contains_key("test.txt"));

        // Verify SHA-256: "hello world" → b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9
        let expected = "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9";
        assert_eq!(result.hashes.get("test.txt").unwrap(), expected);
    }

    #[tokio::test]
    async fn compute_content_hashes_nested_dir() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("sub")).expect("mkdir");
        std::fs::write(dir.path().join("a.txt"), b"aaa").expect("write");
        std::fs::write(dir.path().join("sub").join("b.txt"), b"bbb").expect("write");

        let result = compute_content_hashes(dir.path()).await.expect("compute");
        assert_eq!(result.hashes.len(), 2);
        assert!(result.hashes.contains_key("a.txt"));
        assert!(result.hashes.contains_key("sub/b.txt"));
    }

    #[tokio::test]
    async fn compute_content_hashes_different_content_different_hash() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("a.txt"), b"content A").expect("write");
        let result_a = compute_content_hashes(dir.path()).await.expect("compute");

        // Rewrite with different content
        std::fs::write(dir.path().join("a.txt"), b"content B").expect("write");
        let result_b = compute_content_hashes(dir.path()).await.expect("compute");

        assert_ne!(
            result_a.hashes.get("a.txt").unwrap(),
            result_b.hashes.get("a.txt").unwrap(),
            "different content must produce different hashes"
        );
    }

    // ── V1.58 P0 T2 (R-V156P0-M002): symlink rejection + boundary ─────────

    #[tokio::test]
    #[cfg(unix)]
    async fn compute_content_hashes_skips_symlinks() {
        use std::os::unix::fs::symlink;
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("real.txt"), b"real").expect("write");
        // Create a symlink inside the workspace that points to an outside file.
        let outside = tempfile::tempdir().expect("tempdir");
        std::fs::write(outside.path().join("secret.txt"), b"secret").expect("write");
        symlink(outside.path().join("secret.txt"), dir.path().join("escape.txt"))
            .expect("symlink");

        let result = compute_content_hashes(dir.path()).await.expect("compute");
        // real.txt is hashed; escape.txt (symlink) is skipped.
        assert!(result.hashes.contains_key("real.txt"));
        assert!(
            !result.hashes.contains_key("escape.txt"),
            "symlink must be skipped, got: {:?}",
            result.hashes
        );
    }

    #[test]
    fn enforce_path_boundary_accepts_nested() {
        let root = Path::new("/tmp/ws");
        let target = Path::new("/tmp/ws/sub/file.txt");
        assert!(enforce_path_boundary(target, root).is_ok());
    }

    #[test]
    fn enforce_path_boundary_rejects_escape() {
        let root = Path::new("/tmp/ws");
        let target = Path::new("/etc/passwd");
        assert!(matches!(
            enforce_path_boundary(target, root),
            Err(SessionError::PathEscape { .. })
        ));
    }

    #[test]
    fn path_escape_display_is_descriptive() {
        let err = SessionError::PathEscape {
            path: "/etc/passwd".to_string(),
            workspace_root: "/tmp/ws".to_string(),
        };
        let s = err.to_string();
        assert!(s.contains("/etc/passwd"));
        assert!(s.contains("/tmp/ws"));
        assert!(s.contains("escapes"));
    }

    #[test]
    fn session_error_display() {
        let id = SessionId("ws_test".to_string());
        let err = SessionError::NotFound(id.clone());
        assert!(err.to_string().contains("not found"));

        let err = SessionError::AlreadyCommitted(id.clone());
        assert!(err.to_string().contains("already been committed"));

        let err = SessionError::Expired(id.clone());
        assert!(err.to_string().contains("expired"));

        let err = SessionError::HashConflict {
            session_id: id,
            path: "test.txt".to_string(),
            expected_hash: "abc".to_string(),
            actual_hash: "def".to_string(),
        };
        assert!(err.to_string().contains("content hash conflict"));
        assert!(err.to_string().contains("test.txt"));
    }

    #[test]
    fn change_entry_deserialization() {
        let json = r#"{"path":"test.txt","contentHash":"abc123","op":"modify"}"#;
        let entry: ChangeEntry = serde_json::from_str(json).expect("deserialize");
        assert_eq!(entry.path, "test.txt");
        assert_eq!(entry.content_hash, "abc123");
        assert!(matches!(entry.op, ChangeOp::Modify));
    }
}
