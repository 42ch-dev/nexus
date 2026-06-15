//! Append-only rules history writer (DF-65, Layer 3).
//!
//! Appends audit entries to `Works/<work_ref>/Rules/novel-rules-history.md`
//! using atomic write-temp + rename to prevent corruption on crash.

use std::path::Path;

/// Append a history row to the rules history file.
///
/// Each entry is a Markdown line with:
/// - ISO 8601 timestamp
/// - Reason for the change
/// - Actor/source (e.g. "novel-chapter-review", "user")
///
/// The file is created if it doesn't exist. Writes are atomic
/// (write to temp file, then rename) per V1.33 R-V133P4-05.
///
/// # Errors
///
/// Returns `Err` if the file cannot be written (permissions, disk full, etc.).
pub fn append_rules_history(
    workspace_dir: &Path,
    work_ref: &str,
    reason: &str,
    actor: &str,
) -> std::io::Result<()> {
    let history_path = nexus_home_layout::work_novel_rules_history_path(workspace_dir, work_ref);

    // Ensure parent directory exists
    if let Some(parent) = history_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let timestamp = chrono::Utc::now().to_rfc3339();
    let entry = format!("| {timestamp} | {actor} | {reason} |\n");

    // Atomic write: append via temp file + rename if the file doesn't exist yet.
    // For existing files, we append directly (append mode is safe for small writes).
    if history_path.exists() {
        // Append to existing file
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&history_path)?;
        file.write_all(entry.as_bytes())?;
    } else {
        // First write — create with header + entry (atomic via temp + rename)
        let header = "| Timestamp | Actor | Reason |\n| --- | --- | --- |\n";
        let content = format!("{header}{entry}");

        let temp_path = history_path.with_extension("md.tmp");
        std::fs::write(&temp_path, &content)?;
        std::fs::rename(&temp_path, &history_path)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn append_creates_new_history_file() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let ws = tmp.path();

        append_rules_history(ws, "test-novel", "Initial rules set", "novel-project-init")
            .expect("append");

        let path = nexus_home_layout::work_novel_rules_history_path(ws, "test-novel");
        assert!(path.exists());

        let content = fs::read_to_string(&path).expect("read");
        assert!(content.contains("Initial rules set"));
        assert!(content.contains("novel-project-init"));
        assert!(content.contains("| Timestamp | Actor | Reason |"));
    }

    #[test]
    fn append_adds_rows_to_existing() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let ws = tmp.path();

        append_rules_history(ws, "my-novel", "Changed POV to first person", "user")
            .expect("first append");
        append_rules_history(
            ws,
            "my-novel",
            "Updated chapter length target",
            "novel-chapter-review",
        )
        .expect("second append");

        let path = nexus_home_layout::work_novel_rules_history_path(ws, "my-novel");
        let content = fs::read_to_string(&path).expect("read");

        // Header should appear once
        assert_eq!(
            content.matches("| Timestamp | Actor | Reason |").count(),
            1,
            "header should appear exactly once"
        );

        // Both entries should be present
        assert!(content.contains("Changed POV to first person"));
        assert!(content.contains("Updated chapter length target"));
        assert!(content.contains("user"));
        assert!(content.contains("novel-chapter-review"));
    }
}
