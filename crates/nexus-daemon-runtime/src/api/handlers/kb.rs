//! Knowledge Base (KB) handlers (V1.20 Batch 5, T39).
//!
//! CRUD endpoints for local KB entries:
//! - `GET /v1/local/kb/entries` — list/search KB entries
//! - `POST /v1/local/kb/entries` — add KB entry
//! - `GET /v1/local/kb/entries/{id}` — get single KB entry
//! - `DELETE /v1/local/kb/entries/{id}` — delete KB entry

#![allow(clippy::missing_errors_doc)]

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;
use axum::extract::{Path, Query, State};
use axum::Json;
use nexus_home_layout::validate_entry_id_safe;
use serde::{Deserialize, Serialize};
use tracing::info;

// ─── Request / Response types ──────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ListKbEntriesQuery {
    pub creator_id: Option<String>,
    pub workspace_slug: Option<String>,
    pub q: Option<String>,
    pub limit: Option<usize>,
    pub cursor: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct KbEntrySummary {
    pub entry_id: String,
    pub title: String,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct ListKbEntriesResponse {
    pub items: Vec<KbEntrySummary>,
    pub pagination: PaginationInfo,
}

#[derive(Debug, Serialize)]
pub struct PaginationInfo {
    pub limit: usize,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct AddKbEntryRequest {
    pub creator_id: String,
    pub workspace_slug: Option<String>,
    pub title: Option<String>,
    /// File content as UTF-8 string.
    pub content: Option<String>,
    /// Path to a local file to read content from.
    pub file_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AddKbEntryResponse {
    pub entry_id: String,
    pub title: String,
}

#[derive(Debug, Serialize)]
pub struct GetKbEntryResponse {
    pub entry_id: String,
    pub title: String,
    pub created_at: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct DeleteKbEntryResponse {
    pub entry_id: String,
    pub deleted: bool,
}

// ─── KB Index types ────────────────────────────────────────────────────────

/// KB index on disk: `{"entries": [{"entry_id": "...", "title": "...", "created_at": "..."}]}`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct KbIndex {
    #[serde(default)]
    entries: Vec<KbIndexEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct KbIndexEntry {
    entry_id: String,
    title: String,
    created_at: String,
}

// ─── Helpers ───────────────────────────────────────────────────────────────

/// Default workspace slug.
const DEFAULT_WORKSPACE_SLUG: &str = "default";

/// Default page limit.
const DEFAULT_LIMIT: usize = 50;
/// Maximum page limit.
const MAX_LIMIT: usize = 250;

/// Resolve KB directory paths.
fn resolve_kb_paths(
    home: &std::path::Path,
    creator_id: &str,
    workspace_slug: Option<&str>,
) -> (std::path::PathBuf, std::path::PathBuf) {
    let slug = workspace_slug.unwrap_or(DEFAULT_WORKSPACE_SLUG);
    let kb_dir = nexus_home_layout::creator_kb_dir(home, creator_id, slug);
    let entries_dir = nexus_home_layout::creator_kb_entries_dir(home, creator_id, slug);
    (kb_dir, entries_dir)
}

/// Read the KB index from disk. Returns default (empty) if file is missing.
fn read_kb_index(index_path: &std::path::Path) -> KbIndex {
    if !index_path.exists() {
        return KbIndex::default();
    }
    let Ok(content) = std::fs::read_to_string(index_path) else {
        return KbIndex::default();
    };
    if content.trim().is_empty() {
        return KbIndex::default();
    }
    serde_json::from_str(&content).unwrap_or_default()
}

/// Write the KB index to disk atomically.
fn write_kb_index(index_path: &std::path::Path, index: &KbIndex) -> Result<(), NexusApiError> {
    if let Some(parent) = index_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| NexusApiError::Internal {
            code: "DIR_CREATE_ERROR".into(),
            message: e.to_string(),
        })?;
    }
    let json = serde_json::to_string_pretty(index).map_err(|e| NexusApiError::Internal {
        code: "JSON_ERROR".into(),
        message: e.to_string(),
    })?;
    let tmp_path = index_path.with_extension("json.tmp");
    std::fs::write(&tmp_path, &json).map_err(|e| NexusApiError::Internal {
        code: "FILE_WRITE_ERROR".into(),
        message: e.to_string(),
    })?;
    std::fs::rename(&tmp_path, index_path).map_err(|e| NexusApiError::Internal {
        code: "FILE_RENAME_ERROR".into(),
        message: e.to_string(),
    })?;
    Ok(())
}

/// Generate a KB entry ID.
#[allow(clippy::cast_possible_truncation)]
fn generate_entry_id() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let millis = now.as_millis() as u32;
    let diversifier = ((millis << 16) ^ (now.subsec_nanos() >> 4)) as u16;
    format!("kb_{:08x}{:04x}", millis % 0xFFFF_FFFF, diversifier)
}

/// Deduplicate entry ID within index.
fn deduplicate_entry_id(base_id: &str, index: &KbIndex) -> String {
    if !index.entries.iter().any(|e| e.entry_id == base_id) {
        return base_id.to_string();
    }
    for counter in 1..100 {
        let candidate = format!("{base_id}_{counter}");
        if !index.entries.iter().any(|e| e.entry_id == candidate) {
            return candidate;
        }
    }
    format!("{base_id}_overflow")
}

// ─── Handlers ──────────────────────────────────────────────────────────────

/// `GET /v1/local/kb/entries` — list/search KB entries (T39)
pub async fn list_entries(
    State(_state): State<WorkspaceState>,
    Query(query): Query<ListKbEntriesQuery>,
) -> Result<Json<ListKbEntriesResponse>, NexusApiError> {
    let creator_id = query
        .creator_id
        .as_deref()
        .ok_or_else(|| NexusApiError::InvalidInput {
            field: "creator_id".to_string(),
            reason: "creator_id is required".to_string(),
        })?;

    nexus_home_layout::validate_creator_id_safe(creator_id).map_err(|reason| {
        NexusApiError::InvalidInput {
            field: "creator_id".to_string(),
            reason,
        }
    })?;

    let home = dirs::home_dir().ok_or_else(|| NexusApiError::Internal {
        code: "HOME_DIR_ERROR".into(),
        message: "Cannot determine home directory".to_string(),
    })?;

    let (kb_dir, _entries_dir) =
        resolve_kb_paths(&home, creator_id, query.workspace_slug.as_deref());
    let index_path = kb_dir.join("index.json");

    let index = read_kb_index(&index_path);
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).min(MAX_LIMIT);

    let mut items: Vec<KbEntrySummary> = index
        .entries
        .iter()
        .filter(|e| {
            query
                .q
                .as_ref()
                .is_none_or(|q| e.title.to_lowercase().contains(&q.to_lowercase()))
        })
        .map(|e| KbEntrySummary {
            entry_id: e.entry_id.clone(),
            title: e.title.clone(),
            created_at: e.created_at.clone(),
        })
        .collect();

    // Apply cursor-based pagination (cursor = last entry_id seen)
    if let Some(ref cursor) = query.cursor {
        let pos = items.iter().position(|i| i.entry_id == *cursor);
        if let Some(idx) = pos {
            items = items.split_off(idx + 1);
        }
    }

    let next_cursor = if items.len() > limit {
        items.truncate(limit);
        items.last().map(|i| i.entry_id.clone())
    } else {
        None
    };

    Ok(Json(ListKbEntriesResponse {
        items,
        pagination: PaginationInfo { limit, next_cursor },
    }))
}

/// `POST /v1/local/kb/entries` — add KB entry (T39)
pub async fn add_entry(
    State(_state): State<WorkspaceState>,
    Json(req): Json<AddKbEntryRequest>,
) -> Result<Json<AddKbEntryResponse>, NexusApiError> {
    info!(creator_id = %req.creator_id, "Adding KB entry");

    nexus_home_layout::validate_creator_id_safe(&req.creator_id).map_err(|reason| {
        NexusApiError::InvalidInput {
            field: "creator_id".to_string(),
            reason,
        }
    })?;

    // Get content from either inline content or file path
    let content = if let Some(ref content) = req.content {
        content.clone()
    } else if let Some(ref file_path) = req.file_path {
        let path = std::path::Path::new(file_path);
        if !path.exists() {
            return Err(NexusApiError::NotFound(format!(
                "Source file not found: {}",
                path.display()
            )));
        }
        std::fs::read_to_string(path).map_err(|e| NexusApiError::Internal {
            code: "FILE_READ_ERROR".into(),
            message: e.to_string(),
        })?
    } else {
        return Err(NexusApiError::InvalidInput {
            field: "content".to_string(),
            reason: "either 'content' or 'file_path' must be provided".to_string(),
        });
    };

    let home = dirs::home_dir().ok_or_else(|| NexusApiError::Internal {
        code: "HOME_DIR_ERROR".into(),
        message: "Cannot determine home directory".to_string(),
    })?;

    let (kb_dir, entries_dir) =
        resolve_kb_paths(&home, &req.creator_id, req.workspace_slug.as_deref());
    let index_path = kb_dir.join("index.json");

    std::fs::create_dir_all(&entries_dir).map_err(|e| NexusApiError::Internal {
        code: "DIR_CREATE_ERROR".into(),
        message: e.to_string(),
    })?;

    let base_id = generate_entry_id();
    let mut index = read_kb_index(&index_path);
    let entry_id = deduplicate_entry_id(&base_id, &index);
    let entry_title = req.title.unwrap_or_else(|| entry_id.clone());

    // Step 1: Write updated index to temp file (W2 — index update first)
    let created_at = chrono::Utc::now().to_rfc3339();
    index.entries.push(KbIndexEntry {
        entry_id: entry_id.clone(),
        title: entry_title.clone(),
        created_at,
    });

    let tmp_index_path = index_path.with_extension("json.tmp");
    {
        if let Some(parent) = tmp_index_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| NexusApiError::Internal {
                code: "DIR_CREATE_ERROR".into(),
                message: e.to_string(),
            })?;
        }
        let json = serde_json::to_string_pretty(&index).map_err(|e| NexusApiError::Internal {
            code: "JSON_ERROR".into(),
            message: e.to_string(),
        })?;
        std::fs::write(&tmp_index_path, json).map_err(|e| NexusApiError::Internal {
            code: "FILE_WRITE_ERROR".into(),
            message: e.to_string(),
        })?;
    }

    // Step 2: Write entry content file
    let dest = entries_dir.join(format!("{entry_id}.md"));
    std::fs::write(&dest, &content).map_err(|e| NexusApiError::Internal {
        code: "FILE_WRITE_ERROR".into(),
        message: e.to_string(),
    })?;

    // Step 3: Atomically rename temp index to final
    std::fs::rename(&tmp_index_path, &index_path).map_err(|e| NexusApiError::Internal {
        code: "FILE_RENAME_ERROR".into(),
        message: e.to_string(),
    })?;

    Ok(Json(AddKbEntryResponse {
        entry_id,
        title: entry_title,
    }))
}

/// `GET /v1/local/kb/entries/{id}` — get single KB entry (T39)
pub async fn get_entry(
    State(_state): State<WorkspaceState>,
    Path(entry_id): Path<String>,
) -> Result<Json<GetKbEntryResponse>, NexusApiError> {
    info!(entry_id = %entry_id, "Getting KB entry");

    // Require query params via headers — but we need creator_id + workspace_slug.
    // For this endpoint, we'll look them up from active selection.
    // The path-only approach requires the client to specify in query params,
    // but since this is a path-only handler, we search all KB dirs.
    validate_entry_id_safe(&entry_id).map_err(|reason| NexusApiError::InvalidInput {
        field: "entry_id".to_string(),
        reason,
    })?;

    let home = dirs::home_dir().ok_or_else(|| NexusApiError::Internal {
        code: "HOME_DIR_ERROR".into(),
        message: "Cannot determine home directory".to_string(),
    })?;

    // Search across all creator/workspace combos
    let creators_root = home.join(".nexus42").join("creators");
    if !creators_root.is_dir() {
        return Err(NexusApiError::NotFound(format!(
            "KB entry {entry_id} not found"
        )));
    }

    let entry_file = format!("{entry_id}.md");

    for creator_entry in std::fs::read_dir(&creators_root)
        .map_err(|e| NexusApiError::Internal {
            code: "DIR_READ_ERROR".into(),
            message: e.to_string(),
        })?
        .flatten()
    {
        if !creator_entry.path().is_dir() {
            continue;
        }
        let ws_root = creator_entry.path().join("workspaces");
        let Ok(ws_entries) = std::fs::read_dir(&ws_root) else {
            continue;
        };
        for ws_entry in ws_entries.flatten() {
            let kb_entries = ws_entry.path().join("kb").join("entries");
            let candidate = kb_entries.join(&entry_file);
            if candidate.exists() {
                let content =
                    std::fs::read_to_string(&candidate).map_err(|e| NexusApiError::Internal {
                        code: "FILE_READ_ERROR".into(),
                        message: e.to_string(),
                    })?;

                // Read index for metadata
                let index_path = ws_entry.path().join("kb").join("index.json");
                let index = read_kb_index(&index_path);
                let index_entry = index.entries.iter().find(|e| e.entry_id == entry_id);

                let (title, created_at) = index_entry.map_or_else(
                    || (entry_id.clone(), String::new()),
                    |ie| (ie.title.clone(), ie.created_at.clone()),
                );

                return Ok(Json(GetKbEntryResponse {
                    entry_id,
                    title,
                    created_at,
                    content,
                }));
            }
        }
    }

    Err(NexusApiError::NotFound(format!(
        "KB entry {entry_id} not found"
    )))
}

/// `DELETE /v1/local/kb/entries/{id}` — delete KB entry (T39)
pub async fn delete_entry(
    State(_state): State<WorkspaceState>,
    Path(entry_id): Path<String>,
) -> Result<Json<DeleteKbEntryResponse>, NexusApiError> {
    info!(entry_id = %entry_id, "Deleting KB entry");

    validate_entry_id_safe(&entry_id).map_err(|reason| NexusApiError::InvalidInput {
        field: "entry_id".to_string(),
        reason,
    })?;

    let home = dirs::home_dir().ok_or_else(|| NexusApiError::Internal {
        code: "HOME_DIR_ERROR".into(),
        message: "Cannot determine home directory".to_string(),
    })?;

    let creators_root = home.join(".nexus42").join("creators");
    if !creators_root.is_dir() {
        return Err(NexusApiError::NotFound(format!(
            "KB entry {entry_id} not found"
        )));
    }

    let entry_file = format!("{entry_id}.md");

    for creator_entry in std::fs::read_dir(&creators_root)
        .map_err(|e| NexusApiError::Internal {
            code: "DIR_READ_ERROR".into(),
            message: e.to_string(),
        })?
        .flatten()
    {
        if !creator_entry.path().is_dir() {
            continue;
        }
        let ws_root = creator_entry.path().join("workspaces");
        let Ok(ws_entries) = std::fs::read_dir(&ws_root) else {
            continue;
        };
        for ws_entry in ws_entries.flatten() {
            let kb_entries = ws_entry.path().join("kb").join("entries");
            let candidate = kb_entries.join(&entry_file);
            if candidate.exists() {
                // Remove entry file
                std::fs::remove_file(&candidate).map_err(|e| NexusApiError::Internal {
                    code: "FILE_DELETE_ERROR".into(),
                    message: e.to_string(),
                })?;

                // Update index
                let index_path = ws_entry.path().join("kb").join("index.json");
                let mut index = read_kb_index(&index_path);
                index.entries.retain(|e| e.entry_id != entry_id);
                if index.entries.is_empty() {
                    // Clean up empty index
                    let _ = std::fs::remove_file(&index_path);
                } else {
                    write_kb_index(&index_path, &index)?;
                }

                return Ok(Json(DeleteKbEntryResponse {
                    entry_id,
                    deleted: true,
                }));
            }
        }
    }

    Err(NexusApiError::NotFound(format!(
        "KB entry {entry_id} not found"
    )))
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_entry_id_rejects_traversal() {
        assert!(validate_entry_id_safe("../etc/passwd").is_err());
    }

    #[test]
    fn validate_entry_id_accepts_valid() {
        assert!(validate_entry_id_safe("kb_abc12345").is_ok());
    }

    #[test]
    fn generate_entry_id_format() {
        let id = generate_entry_id();
        assert!(id.starts_with("kb_"));
        assert!(id.len() > 10);
    }

    #[test]
    fn deduplicate_no_collision() {
        let index = KbIndex::default();
        let id = deduplicate_entry_id("kb_test", &index);
        assert_eq!(id, "kb_test");
    }

    #[test]
    fn deduplicate_with_collision() {
        let index = KbIndex {
            entries: vec![KbIndexEntry {
                entry_id: "kb_test".to_string(),
                title: "Test".to_string(),
                created_at: String::new(),
            }],
        };
        let id = deduplicate_entry_id("kb_test", &index);
        assert_eq!(id, "kb_test_1");
    }
}
