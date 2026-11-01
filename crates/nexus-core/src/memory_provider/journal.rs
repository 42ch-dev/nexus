//! Durable intent journal and staged-file publish for the provider's
//! cross-store writes (spoke-memory adoption plan §5.2, D-10, C-113).
//!
//! SQLite cannot make a markdown write atomic with a row write, so the
//! provider never claims file+SQL atomicity. Instead:
//!
//! 1. **Intent first.** Every mutating content operation that produces a file
//!    effect commits a [`FileEffect`] list — target, exact bytes, content
//!    digest — into `memory_journal` in the SAME guarded transaction as its
//!    rows and receipt. That commit is the point of no return.
//! 2. **Staged publish.** The bytes land in a hidden staged file
//!    (`.<name>.<receipt_id>.staged`, dot-prefixed so no reader lists it),
//!    are verified against the intent digest, and are atomically renamed into
//!    place. Only then is the entry marked `applied`.
//! 3. **Bounded recovery.** A reopen settles every `pending` entry of its
//!    space ([`RECOVERY_BOUND`] entries per pass, no retries): a staged file
//!    matching the intent digest is published, a missing/mismatched one is
//!    regenerated from the journaled bytes, and a target that already holds
//!    different content, an unsafe path or an I/O failure quarantines the
//!    intent. Objects of a subject with an unsettled entry stay out of
//!    recall/list/get until it is resolved; a quarantined entry answers the
//!    terminal `INTERNAL_ERROR` on that subject instead of serving
//!    half-committed content.
//!
//! File effects are the provider's own markdown projection of its
//! provider-owned content (`records/<record_id>.md`,
//! `candidates/<candidate_id>.md`, `profiles/<digest>.md`) under the
//! workspace's `memory-store/` directory. They never touch the legacy bearer
//! layout (`SOUL.md`, `memory/long-term/*.md`), which D11/Global Constraints
//! keep byte-identical, and they are the artifact N4 transfer tooling stages.
//!
//! The digest is the provider's standard `jcs_sha256_hex` over the rendered
//! markdown as a JSON string — the same helper the candidate `content_digest`
//! uses — so journal, publish and recovery all compare one computation.

use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::storage::{CandidateRow, ProfileRow, RecordRow, StoreError, StoreResult};

/// The provider's file root inside the workspace directory, next to `state.db`.
pub(super) const FILES_DIR: &str = "memory-store";

/// How many unsettled intents one recovery pass settles. Recovery runs at open
/// and must never page an unbounded journal; the remainder is settled by the
/// next pass.
pub(super) const RECOVERY_BOUND: i64 = 64;

pub(super) const STATE_PENDING: &str = "pending";
pub(super) const STATE_APPLIED: &str = "applied";
pub(super) const STATE_QUARANTINED: &str = "quarantined";

/// One planned file write: the target relative to the file root, the exact
/// bytes, and their digest. The same shape is stored in the journal and
/// replayed by recovery, so an intent and its effect cannot drift.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct FileEffect {
    pub path: String,
    pub digest: String,
    pub content: String,
}

/// What recovery did with one target.
pub(super) enum Settled {
    Published,
    Quarantined(String),
}

fn digest_of(content: &str) -> StoreResult<String> {
    match spoke_operations::jcs_sha256_hex(&Value::String(content.to_string())) {
        spoke_operations::SpokeResult::Ok(digest) => Ok(digest),
        spoke_operations::SpokeResult::Reject(reject) => Err(StoreError::Corrupt(reject.message)),
    }
}

fn effect(path: String, content: String) -> StoreResult<FileEffect> {
    let digest = digest_of(&content)?;
    Ok(FileEffect {
        path,
        digest,
        content,
    })
}

/// A frontmatter value must stay one line; ids come off the wire.
fn line(value: &str) -> String {
    value.replace(['\n', '\r'], " ")
}

fn body_text(content_json: &str) -> String {
    match serde_json::from_str::<Value>(content_json) {
        Ok(Value::Object(map)) => map
            .get("text")
            .and_then(Value::as_str)
            .map_or_else(|| content_json.to_string(), str::to_string),
        _ => content_json.to_string(),
    }
}

fn document_text(document_json: &str) -> String {
    match serde_json::from_str::<Value>(document_json) {
        Ok(Value::Object(map)) => map
            .get("body")
            .and_then(Value::as_str)
            .map_or_else(|| document_json.to_string(), str::to_string),
        _ => document_json.to_string(),
    }
}

/// A frontmatter line for an optional value, or nothing when it is absent.
fn optional(label: &str, value: Option<&str>) -> String {
    value.map_or_else(String::new, |value| format!("{label}: {}\n", line(value)))
}

/// §6.6 record projection: the stored row as markdown.
pub(super) fn record_effect(row: &RecordRow) -> StoreResult<FileEffect> {
    let out = format!(
        "---\n\
         record_id: {}\n\
         space_id: {}\n\
         subject_id: {}\n\
         kind: {}\n\
         visibility: {}\n\
         {}\
         revision: {}\n\
         created_at: {}\n\
         updated_at: {}\n\
         {}\
         {}\
         ---\n\n\
         {}",
        line(&row.record_id),
        line(&row.space_id),
        line(&row.subject_id),
        line(&row.kind),
        line(&row.scope_visibility),
        optional("context_id", row.scope_context_id.as_deref()),
        row.revision,
        line(&row.created_at),
        line(&row.updated_at),
        optional("occurred_at", row.occurred_at.as_deref()),
        if row.tombstone_json.is_some() {
            "tombstoned: true\n"
        } else {
            ""
        },
        body_text(&row.content_json),
    );
    effect(format!("records/{}.md", row.record_id), out)
}

/// §6.7 consumed-candidate shell projection: the content-free audit shell.
pub(super) fn candidate_effect(row: &CandidateRow) -> StoreResult<FileEffect> {
    let out = format!(
        "---\n\
         candidate_id: {}\n\
         space_id: {}\n\
         subject_id: {}\n\
         kind: {}\n\
         status: {}\n\
         revision: {}\n\
         created_at: {}\n\
         {}\
         ---\n\n\
         {}",
        line(&row.candidate_id),
        line(&row.space_id),
        line(&row.subject_id),
        line(&row.kind),
        line(&row.status),
        row.revision,
        line(&row.created_at),
        optional("content_digest", row.content_digest.as_deref()),
        row.consumed_by_json.as_deref().unwrap_or(""),
    );
    effect(format!("candidates/{}.md", row.candidate_id), out)
}

/// §6.8 profile projection. The file name is the digest of the subject/kind
/// pair, so open-vocabulary ids never become path components.
pub(super) fn profile_effect(row: &ProfileRow) -> StoreResult<FileEffect> {
    let name = digest_of(&format!("{}\u{1f}{}", row.subject_id, row.profile_kind))?;
    let out = format!(
        "---\n\
         space_id: {}\n\
         subject_id: {}\n\
         profile_kind: {}\n\
         revision: {}\n\
         updated_by: {}\n\
         created_at: {}\n\
         updated_at: {}\n\
         ---\n\n\
         {}",
        line(&row.space_id),
        line(&row.subject_id),
        line(&row.profile_kind),
        row.revision,
        line(&row.updated_by),
        line(&row.created_at),
        line(&row.updated_at),
        document_text(&row.document_json),
    );
    effect(format!("profiles/{name}.md"), out)
}

/// Resolve an intent target below the file root, refusing anything that would
/// leave it (absolute paths, `..`, prefixes).
fn resolve(root: &Path, effect: &FileEffect) -> Option<PathBuf> {
    let mut path = root.to_path_buf();
    let mut components = Path::new(&effect.path).components();
    match components.next() {
        Some(Component::Normal(part)) => path.push(part),
        _ => return None,
    }
    for component in components {
        match component {
            Component::Normal(part) => path.push(part),
            _ => return None,
        }
    }
    Some(path)
}

/// The hidden staged sibling of a target: dot-prefixed and receipt-scoped, so
/// no provider reader lists it and two operations never share a staged file.
fn staged(target: &Path, receipt_id: &str) -> PathBuf {
    let name = target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("target");
    target.with_file_name(format!(".{name}.{receipt_id}.staged"))
}

async fn write_staged(
    root: &Path,
    receipt_id: &str,
    effect: &FileEffect,
) -> std::io::Result<PathBuf> {
    let Some(target) = resolve(root, effect) else {
        return Err(std::io::Error::other("unsafe journal target"));
    };
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let staged_path = staged(&target, receipt_id);
    tokio::fs::write(&staged_path, effect.content.as_bytes()).await?;
    Ok(staged_path)
}

fn matches_digest(bytes: &[u8], digest: &str) -> bool {
    std::str::from_utf8(bytes)
        .ok()
        .and_then(|content| digest_of(content).ok())
        .is_some_and(|actual| actual == digest)
}

/// Write the staged file for one intent (step 2's first half).
pub(super) async fn stage(
    root: &Path,
    receipt_id: &str,
    effect: &FileEffect,
) -> std::io::Result<()> {
    write_staged(root, receipt_id, effect).await.map(|_| ())
}

/// Verify the staged bytes against the intent digest and atomically rename
/// them into place (step 2's second half).
pub(super) async fn publish(
    root: &Path,
    receipt_id: &str,
    effect: &FileEffect,
) -> std::io::Result<()> {
    let Some(target) = resolve(root, effect) else {
        return Err(std::io::Error::other("unsafe journal target"));
    };
    let staged_path = staged(&target, receipt_id);
    let bytes = tokio::fs::read(&staged_path).await?;
    if !matches_digest(&bytes, &effect.digest) {
        return Err(std::io::Error::other(
            "staged file does not match the intent digest",
        ));
    }
    tokio::fs::rename(&staged_path, &target).await
}

/// Settle one target from the crash-consistent state on disk: publish an
/// already-consistent target, publish a matching staged file, or regenerate
/// the staged file from the journaled bytes. Anything that would overwrite
/// content the intent did not produce is quarantined instead.
pub(super) async fn settle(root: &Path, receipt_id: &str, effect: &FileEffect) -> Settled {
    let Some(target) = resolve(root, effect) else {
        return Settled::Quarantined("unsafe journal target".to_string());
    };
    let staged_path = staged(&target, receipt_id);
    match tokio::fs::read(&target).await {
        Ok(bytes) => {
            if !matches_digest(&bytes, &effect.digest) {
                return Settled::Quarantined(format!(
                    "target {} already holds different content",
                    effect.path
                ));
            }
            // Crash after publish, before the applied mark: the content is
            // already consistent, so the stale staged copy (if any) is dropped.
            let _ = tokio::fs::remove_file(&staged_path).await;
            return Settled::Published;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Settled::Quarantined(format!("target read failed: {error}"));
        }
    }
    if let Ok(bytes) = tokio::fs::read(&staged_path).await {
        if matches_digest(&bytes, &effect.digest) {
            return match tokio::fs::rename(&staged_path, &target).await {
                Ok(()) => Settled::Published,
                Err(error) => Settled::Quarantined(format!("publish rename failed: {error}")),
            };
        }
    }
    // Missing or mismatched staged content: regenerate it from the journaled
    // bytes, verify what was written, and publish that.
    let staged_path = match write_staged(root, receipt_id, effect).await {
        Ok(path) => path,
        Err(error) => return Settled::Quarantined(format!("regeneration failed: {error}")),
    };
    let written = match tokio::fs::read(&staged_path).await {
        Ok(bytes) => bytes,
        Err(error) => return Settled::Quarantined(format!("staged read failed: {error}")),
    };
    if !matches_digest(&written, &effect.digest) {
        return Settled::Quarantined("journaled content does not match its digest".to_string());
    }
    match tokio::fs::rename(&staged_path, &target).await {
        Ok(()) => Settled::Published,
        Err(error) => Settled::Quarantined(format!("publish rename failed: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> FileEffect {
        effect("records/rec_x.md".to_string(), "body\n".to_string()).unwrap()
    }

    #[test]
    fn targets_cannot_escape_the_file_root() {
        let root = Path::new("/root");
        assert_eq!(
            resolve(root, &sample()),
            Some(PathBuf::from("/root/records/rec_x.md"))
        );
        for path in ["/abs.md", "../out.md", "records/../../out.md", ""] {
            let hostile = FileEffect {
                path: path.to_string(),
                digest: String::new(),
                content: String::new(),
            };
            assert_eq!(resolve(root, &hostile), None, "{path:?} escapes the root");
        }
    }

    #[test]
    fn staged_names_are_hidden_and_receipt_scoped() {
        let target = Path::new("/root/records/rec_x.md");
        assert_eq!(
            staged(target, "rcp_a"),
            PathBuf::from("/root/records/.rec_x.md.rcp_a.staged")
        );
    }
}
