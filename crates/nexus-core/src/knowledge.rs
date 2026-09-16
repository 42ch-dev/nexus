//! Local knowledge surfaces over the guarded core workspace: the
//! creator-scoped work file index (`creator kb`, V1.20 Batch 5 T39 — scope
//! clarified KCA-003 C2; V1.27 H3 scope honesty) and the User-scoped global
//! knowledge entries (`creator knowledge`, entity-scope-model §5.3–5.4).
//! Extracted from the legacy daemon `kb.rs` handlers plus the CLI-local
//! `SqliteKnowledgeStore` composition (P1-T3). This is NOT the World narrative
//! KB graph (`nexus-kb` / P0 `world_kb`).
//! The file-index surface is deliberately `async` despite syncing only against
//! `std::fs`: it keeps a uniform `CoreService` method surface for the P6 CLI
//! composition and the daemon adapters.
#![allow(clippy::unused_async_trait_impl, clippy::unused_async)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::LazyLock;
use std::sync::Mutex;

use nexus_contracts::daemon_api::kb::{
    AddKbEntryRequest, AddKbEntryResponse, DeleteKbEntryResponse, GetKbEntryResponse,
    KbEntrySummary, ListKbEntriesQuery, ListKbEntriesResponse,
};
use nexus_contracts::PaginationInfo;
use nexus_home_layout::validate_entry_id_safe;
use nexus_knowledge::knowledge::{
    KnowledgeQuery, KnowledgeResult, KnowledgeTag, UserKnowledgeEntry,
};
use nexus_knowledge::store::KnowledgeStore;
use nexus_local_db::SqliteKnowledgeStore;

use crate::content::wire_cast;
use crate::{CoreError, CoreResult, CoreService, Principal};

/// Default workspace slug.
const DEFAULT_WORKSPACE_SLUG: &str = "default";

/// Default page limit.
const DEFAULT_LIMIT: usize = 50;
/// Maximum page limit.
const MAX_LIMIT: usize = 250;

/// Default User ID for local knowledge usage (until platform `usr_*` mapping).
const DEFAULT_USER_ID: &str = "user_default";

/// Knowledge-family fault carrying the legacy classification until the
/// adapter boundary. `InvalidInput` keeps the legacy 400 mapping; legacy
/// internal codes (`DIR_CREATE_ERROR`, `FILE_WRITE_ERROR`, …) ride verbatim
/// as `<CODE>: <message>`.
#[derive(Debug, thiserror::Error)]
enum KnowledgeFault {
    #[error("invalid input: {reason}")]
    InvalidInput { field: String, reason: String },
    #[error("{0}")]
    NotFound(String),
    /// Foreign-creator entry access; carries the `kb_owner:` prefix so the
    /// adapter can re-emit the legacy `Forbidden { resource, reason }` pair.
    #[error("forbidden: {0}")]
    ForeignEntry(String),
    #[error("{code}: {message}")]
    Internal { code: String, message: String },
    #[error(transparent)]
    Core(#[from] CoreError),
}

impl From<KnowledgeFault> for CoreError {
    fn from(error: KnowledgeFault) -> Self {
        match error {
            KnowledgeFault::InvalidInput { field, reason } => Self::InvalidInput { field, reason },
            KnowledgeFault::NotFound(resource) => Self::NotFound { resource },
            KnowledgeFault::ForeignEntry(resource) => Self::Forbidden { resource },
            KnowledgeFault::Internal { code, message } => Self::Internal {
                category: format!("{code}: {message}"),
            },
            KnowledgeFault::Core(error) => error,
        }
    }
}

fn invalid_input(field: &str, reason: impl Into<String>) -> KnowledgeFault {
    KnowledgeFault::InvalidInput {
        field: field.to_string(),
        reason: reason.into(),
    }
}

/// Validate KB scope — only `work` is supported by this surface (H3
/// work-scope honesty).
fn validate_scope(scope: Option<&str>) -> Result<(), KnowledgeFault> {
    match scope {
        None | Some("work") => Ok(()),
        Some(other) => Err(invalid_input(
            "scope",
            format!(
                "scope '{other}' is not supported. These endpoints serve the \
                 work-scope file index only (entity-scope-model §5.3). \
                 World KB and User knowledge are served by separate routes."
            ),
        )),
    }
}

/// `workspace_slug` is a logical single-segment workspace identifier (not an
/// arbitrary filesystem selection): it is joined raw into the KB dir layout,
/// so a separator/`..`/absolute form would escape the workspace root.
fn validate_workspace_slug(slug: Option<&str>) -> Result<(), KnowledgeFault> {
    let valid = slug.is_none_or(|slug| {
        !slug.is_empty()
            && !slug.contains('/')
            && !slug.contains('\\')
            && slug != "."
            && slug != ".."
    });
    if valid {
        Ok(())
    } else {
        Err(invalid_input(
            "workspace_slug",
            "must be a single path segment",
        ))
    }
}

// ─── Work-scope KB Index types ─────────────────────────────────────────────
//
// These types represent the local work file index, NOT the World KB graph
// (nexus-kb) or User knowledge (nexus-knowledge).

/// KB index on disk: `{"entries": [{"entry_id": "...", "title": "...", "created_at": "..."}]}`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
struct KbIndex {
    #[serde(default)]
    entries: Vec<KbIndexEntry>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct KbIndexEntry {
    entry_id: String,
    title: String,
    created_at: String,
}

/// Resolve KB directory paths under the nexus root.
/// `<nexus-root>/creators/<creator>/workspaces/<slug>/kb/` and its `entries/`.
fn resolve_kb_paths(
    nexus_root: &std::path::Path,
    creator_id: &str,
    workspace_slug: Option<&str>,
) -> (PathBuf, PathBuf) {
    let slug = workspace_slug.unwrap_or(DEFAULT_WORKSPACE_SLUG);
    let ws_root = nexus_root
        .join("creators")
        .join(creator_id)
        .join("workspaces")
        .join(slug);
    let kb_dir = ws_root.join("kb");
    (kb_dir.clone(), kb_dir.join("entries"))
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
fn write_kb_index(index_path: &std::path::Path, index: &KbIndex) -> Result<(), KnowledgeFault> {
    if let Some(parent) = index_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| KnowledgeFault::Internal {
            code: "DIR_CREATE_ERROR".into(),
            message: e.to_string(),
        })?;
    }
    let json = serde_json::to_string_pretty(index).map_err(|e| KnowledgeFault::Internal {
        code: "JSON_ERROR".into(),
        message: e.to_string(),
    })?;
    let tmp_path = index_path.with_extension("json.tmp");
    std::fs::write(&tmp_path, &json).map_err(|e| KnowledgeFault::Internal {
        code: "FILE_WRITE_ERROR".into(),
        message: e.to_string(),
    })?;
    std::fs::rename(&tmp_path, index_path).map_err(|e| KnowledgeFault::Internal {
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

// ─── Work-scope KB Entry Index (QC3 W-005) ─────────────────────────────────
//
// In-memory index over work-scope entry files for O(1) lookup by entry_id.
// Only covers the local work file index — no World KB or User knowledge.

/// Key for the KB entry index: [`String`] → (`creator_id`, `workspace_slug`).
type EntryLocationMap = HashMap<String, (String, String)>;

/// Process-wide KB entry index. Built lazily on first get/delete, invalidated
/// on add/delete. Converts O(n) filesystem scans to O(1) hash lookups.
static KB_ENTRY_INDEX: LazyLock<Mutex<Option<EntryLocationMap>>> =
    LazyLock::new(|| Mutex::new(None));

/// Rebuild the KB entry index by scanning all workspace index.json files.
fn rebuild_kb_entry_index(nexus_root: &std::path::Path) -> EntryLocationMap {
    let creators_root = nexus_root.join("creators");
    let mut index = HashMap::new();

    let Ok(creator_entries) = std::fs::read_dir(&creators_root) else {
        return index;
    };

    for creator_entry in creator_entries.flatten() {
        if !creator_entry.path().is_dir() {
            continue;
        }
        let Ok(creator_id) = creator_entry.file_name().into_string() else {
            continue;
        };

        let ws_root = creators_root.join(&creator_id).join("workspaces");
        let Ok(ws_entries) = std::fs::read_dir(&ws_root) else {
            continue;
        };

        for ws_entry in ws_entries.flatten() {
            if !ws_entry.path().is_dir() {
                continue;
            }
            let Ok(workspace_slug) = ws_entry.file_name().into_string() else {
                continue;
            };

            let index_path = ws_entry.path().join("kb").join("index.json");
            let kb_index = read_kb_index(&index_path);
            for entry in &kb_index.entries {
                index.insert(
                    entry.entry_id.clone(),
                    (creator_id.clone(), workspace_slug.clone()),
                );
            }
        }
    }

    index
}

/// Look up `entry_id` in the KB entry index. Rebuilds index on first access.
/// Returns `(creator_id, workspace_slug)` or `None`.
fn lookup_entry_location(entry_id: &str, nexus_root: &std::path::Path) -> Option<(String, String)> {
    {
        let index = KB_ENTRY_INDEX
            .lock()
            .expect("KB entry index lock should not be poisoned");
        if let Some(ref map) = *index {
            return map.get(entry_id).cloned();
        }
    }
    // Index not built yet — rebuild it.
    let new_index = rebuild_kb_entry_index(nexus_root);
    let result = new_index.get(entry_id).cloned();
    *KB_ENTRY_INDEX
        .lock()
        .expect("KB entry index lock should not be poisoned") = Some(new_index);
    result
}

/// Invalidate the KB entry index (call after add/delete).
fn invalidate_kb_entry_index() {
    *KB_ENTRY_INDEX
        .lock()
        .expect("KB entry index lock should not be poisoned") = None;
}

/// Add an entry to the KB entry index (if already built).
fn add_to_kb_entry_index(entry_id: &str, creator_id: &str, workspace_slug: &str) {
    let mut index = KB_ENTRY_INDEX
        .lock()
        .expect("KB entry index lock should not be poisoned");
    if let Some(ref mut map) = *index {
        map.insert(
            entry_id.to_string(),
            (creator_id.to_string(), workspace_slug.to_string()),
        );
    }
    // If None, index will be rebuilt lazily on next access.
}

/// Remove an entry from the KB entry index (if already built).
fn remove_from_kb_entry_index(entry_id: &str) {
    let mut index = KB_ENTRY_INDEX
        .lock()
        .expect("KB entry index lock should not be poisoned");
    if let Some(ref mut map) = *index {
        map.remove(entry_id);
    }
}

/// Scan one creator's workspace roots for `entry_file` under `kb/entries`.
/// Returns `(workspace_root, entry_path)` of the first workspace holding it.
fn scan_creator_workspaces(
    creator_root: &std::path::Path,
    entry_file: &str,
) -> Option<(PathBuf, PathBuf)> {
    let ws_entries = std::fs::read_dir(creator_root.join("workspaces")).ok()?;
    for ws_entry in ws_entries.flatten() {
        let ws_root = ws_entry.path();
        let candidate = ws_root.join("kb").join("entries").join(entry_file);
        if candidate.exists() {
            return Some((ws_root, candidate));
        }
    }
    None
}

/// Whether any other creator's workspaces hold `entry_file`. Keeps the
/// slow-path fallback fail-closed: an entry that exists only under a foreign
/// creator must classify Forbidden (`kb_owner:`), never a silent NotFound.
fn foreign_creator_holds_entry(
    creators_root: &std::path::Path,
    owner_id: &str,
    entry_file: &str,
) -> bool {
    let Ok(creators) = std::fs::read_dir(creators_root) else {
        return false;
    };
    for creator_entry in creators.flatten() {
        if !creator_entry.path().is_dir() {
            continue;
        }
        let Ok(creator_id) = creator_entry.file_name().into_string() else {
            continue;
        };
        if creator_id == owner_id {
            continue;
        }
        if scan_creator_workspaces(&creator_entry.path(), entry_file).is_some() {
            return true;
        }
    }
    false
}

/// Map a knowledge-store failure onto the family fault.
fn knowledge_err(error: nexus_knowledge::errors::KnowledgeError) -> KnowledgeFault {
    match error {
        nexus_knowledge::errors::KnowledgeError::ValidationError(message) => {
            invalid_input("knowledge", message)
        }
        other @ nexus_knowledge::errors::KnowledgeError::InvalidUri { .. } => {
            KnowledgeFault::Internal {
                code: "DATABASE_ERROR".into(),
                message: other.to_string(),
            }
        }
    }
}

impl CoreService {
    // ── Work-scope file index (`creator kb` / `/v1/daemon/kb/entries`) ──

    /// List/search work-scope entries for a creator/workspace.
    ///
    /// Only `scope=work` is supported; no World KB or User knowledge access.
    /// The request `creator_id` must match the principal's creator (the
    /// ownership correction: this index is creator-scoped, never World).
    ///
    /// # Errors
    /// Returns [`CoreError::InvalidInput`] for a non-`work` scope, missing or
    /// unsafe `creator_id`, non-segment `workspace_slug`, and
    /// [`CoreError::Forbidden`] on a foreign `creator_id`.
    pub async fn list_kb_entries(
        &self,
        principal: &Principal,
        query: ListKbEntriesQuery,
    ) -> CoreResult<ListKbEntriesResponse> {
        self.verify_principal(principal)?;

        // H3: Reject non-work scope with 400
        validate_scope(query.scope.as_deref())?;

        let creator_id = query
            .creator_id
            .as_deref()
            .ok_or_else(|| invalid_input("creator_id", "creator_id is required"))?;

        nexus_home_layout::validate_creator_id_safe(creator_id)
            .map_err(|reason| invalid_input("creator_id", reason))?;
        validate_workspace_slug(query.workspace_slug.as_deref())?;
        if creator_id != principal.creator_id() {
            return Err(KnowledgeFault::ForeignEntry(format!("kb_owner:{creator_id}")).into());
        }

        let (kb_dir, _entries_dir) = resolve_kb_paths(
            &self.inner.nexus_home,
            creator_id,
            query.workspace_slug.as_deref(),
        );
        let index_path = kb_dir.join("index.json");

        let index = read_kb_index(&index_path);
        let limit = query
            .limit
            .map_or(DEFAULT_LIMIT, |value| {
                usize::try_from(value).unwrap_or(DEFAULT_LIMIT)
            })
            .min(MAX_LIMIT);

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
        let pagination: PaginationInfo = PaginationInfo {
            limit: i64::try_from(limit).unwrap_or(i64::MAX),
            has_more: next_cursor.is_some(),
            next_cursor,
        };
        self.verify_principal(principal)?;
        Ok(ListKbEntriesResponse {
            items: wire_cast(items)?,
            pagination: wire_cast(pagination)?,
        })
    }

    /// Add a work-scope entry to the local file index.
    ///
    /// Content comes from inline `content` or an existing `file_path`. The
    /// write sequence is crash-consistent (QC3 W-006): temp index rename
    /// commits the metadata, then the temp content rename commits the entry.
    ///
    /// # Trust boundary (QC2-F-004, operator-local by contract)
    ///
    /// When `content` is absent, `file_path` is read verbatim via
    /// `std::fs::read_to_string` with no confinement to the workspace root:
    /// any path the daemon process can read may be ingested. This is a
    /// deliberate retained behavior — the surface is operator-local (the
    /// caller already holds host filesystem access), and constraining reads
    /// here would change retained behavior. Revisit only as an explicit
    /// containment decision (e.g. confining to the creative root), not as a
    /// drive-by fix.
    ///
    /// # Errors
    /// As [`CoreService::list_kb_entries`]; additionally
    /// [`CoreError::NotFound`] for a missing `file_path`, and
    /// [`CoreError::Forbidden`] under read-only core access.
    pub async fn add_kb_entry(
        &self,
        principal: &Principal,
        request: AddKbEntryRequest,
    ) -> CoreResult<AddKbEntryResponse> {
        self.verify_principal(principal)?;
        self.require_work_write()?;

        // H3: Reject non-work scope with 400
        validate_scope(request.scope.as_deref())?;

        nexus_home_layout::validate_creator_id_safe(&request.creator_id)
            .map_err(|reason| invalid_input("creator_id", reason))?;
        validate_workspace_slug(request.workspace_slug.as_deref())?;
        if request.creator_id != principal.creator_id() {
            return Err(
                KnowledgeFault::ForeignEntry(format!("kb_owner:{}", request.creator_id)).into(),
            );
        }

        tracing::info!(creator_id = %request.creator_id, "Adding KB entry");

        // Get content from either inline content or file path
        let content = if let Some(ref content) = request.content {
            content.clone()
        } else if let Some(ref file_path) = request.file_path {
            let path = std::path::Path::new(file_path);
            if !path.exists() {
                return Err(KnowledgeFault::NotFound(format!(
                    "Source file not found: {}",
                    path.display()
                ))
                .into());
            }
            std::fs::read_to_string(path).map_err(|e| KnowledgeFault::Internal {
                code: "FILE_READ_ERROR".into(),
                message: e.to_string(),
            })?
        } else {
            return Err(invalid_input(
                "content",
                "either 'content' or 'file_path' must be provided",
            )
            .into());
        };

        let (kb_dir, entries_dir) = resolve_kb_paths(
            &self.inner.nexus_home,
            &request.creator_id,
            request.workspace_slug.as_deref(),
        );
        let index_path = kb_dir.join("index.json");

        std::fs::create_dir_all(&entries_dir).map_err(|e| KnowledgeFault::Internal {
            code: "DIR_CREATE_ERROR".into(),
            message: e.to_string(),
        })?;

        let base_id = generate_entry_id();
        let mut index = read_kb_index(&index_path);
        let entry_id = deduplicate_entry_id(&base_id, &index);
        let entry_title = request.title.unwrap_or_else(|| entry_id.clone());

        // Step 1: Write updated index to temp file
        let created_at = chrono::Utc::now().to_rfc3339();
        index.entries.push(KbIndexEntry {
            entry_id: entry_id.clone(),
            title: entry_title.clone(),
            created_at,
        });

        let tmp_index_path = index_path.with_extension("json.tmp");
        {
            if let Some(parent) = tmp_index_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| KnowledgeFault::Internal {
                    code: "DIR_CREATE_ERROR".into(),
                    message: e.to_string(),
                })?;
            }
            let json =
                serde_json::to_string_pretty(&index).map_err(|e| KnowledgeFault::Internal {
                    code: "JSON_ERROR".into(),
                    message: e.to_string(),
                })?;
            std::fs::write(&tmp_index_path, json).map_err(|e| KnowledgeFault::Internal {
                code: "FILE_WRITE_ERROR".into(),
                message: e.to_string(),
            })?;
        }

        // Step 2: Write entry content to temp file first (QC3 W-006: crash-consistency).
        // Write to .tmp, then atomic rename after index commit.
        let dest = entries_dir.join(format!("{entry_id}.md"));
        let tmp_content_path = entries_dir.join(format!("{entry_id}.md.tmp"));
        std::fs::write(&tmp_content_path, &content).map_err(|e| KnowledgeFault::Internal {
            code: "FILE_WRITE_ERROR".into(),
            message: e.to_string(),
        })?;

        // Step 3: Atomically rename temp index to final — this commits the metadata.
        std::fs::rename(&tmp_index_path, &index_path).map_err(|e| KnowledgeFault::Internal {
            code: "FILE_RENAME_ERROR".into(),
            message: e.to_string(),
        })?;

        // Step 4: Atomically rename temp content to final — entry is now fully committed.
        std::fs::rename(&tmp_content_path, &dest).map_err(|e| KnowledgeFault::Internal {
            code: "FILE_RENAME_ERROR".into(),
            message: e.to_string(),
        })?;

        // Update KB entry index (W-005).
        let workspace_slug = request
            .workspace_slug
            .as_deref()
            .unwrap_or(DEFAULT_WORKSPACE_SLUG);
        add_to_kb_entry_index(&entry_id, &request.creator_id, workspace_slug);

        self.verify_principal(principal)?;
        Ok(AddKbEntryResponse {
            entry_id,
            title: entry_title,
        })
    }

    /// Get a single work-scope entry, via the O(1) entry index (QC3 W-005)
    /// with a filesystem-scan fallback for a stale index. Ownership is
    /// classified identically in both paths: a foreign entry is always
    /// Forbidden, never hidden behind a cold/stale index as NotFound.
    ///
    /// # Errors
    /// Returns [`CoreError::InvalidInput`] for an unsafe `entry_id`,
    /// [`CoreError::NotFound`] when the entry does not exist, and
    /// [`CoreError::Forbidden`] when it belongs to a different creator.
    pub async fn get_kb_entry(
        &self,
        principal: &Principal,
        entry_id: String,
    ) -> CoreResult<GetKbEntryResponse> {
        self.verify_principal(principal)?;
        validate_entry_id_safe(&entry_id).map_err(|reason| invalid_input("entry_id", reason))?;

        let nexus_root = self.inner.nexus_home.clone();

        // Try index lookup first (O(1)), fall back to filesystem scan.
        if let Some((creator_id, workspace_slug)) = lookup_entry_location(&entry_id, &nexus_root) {
            if creator_id != principal.creator_id() {
                return Err(
                    KnowledgeFault::ForeignEntry(format!("kb_owner:KB entry {entry_id}")).into(),
                );
            }
            // Fast path: read entry from known location.
            let (_, entries_dir) =
                resolve_kb_paths(&nexus_root, &creator_id, Some(&workspace_slug));
            let candidate = entries_dir.join(format!("{entry_id}.md"));
            if candidate.exists() {
                let content =
                    std::fs::read_to_string(&candidate).map_err(|e| KnowledgeFault::Internal {
                        code: "FILE_READ_ERROR".into(),
                        message: e.to_string(),
                    })?;

                let (kb_dir, _) = resolve_kb_paths(&nexus_root, &creator_id, Some(&workspace_slug));
                let index_path = kb_dir.join("index.json");
                let index = read_kb_index(&index_path);
                let index_entry = index.entries.iter().find(|e| e.entry_id == entry_id);

                let (title, created_at) = index_entry.map_or_else(
                    || (entry_id.clone(), String::new()),
                    |ie| (ie.title.clone(), ie.created_at.clone()),
                );

                return Ok(GetKbEntryResponse {
                    entry_id,
                    title,
                    created_at,
                    content,
                });
            }
            // Entry was in index but file missing — stale index, fall through.
            invalidate_kb_entry_index();
        }

        // Slow path: filesystem scan (used when index is stale or on first access).
        let creators_root = nexus_root.join("creators");
        if !creators_root.is_dir() {
            return Err(KnowledgeFault::NotFound(format!("KB entry {entry_id} not found")).into());
        }

        let entry_file = format!("{entry_id}.md");

        // Ownership is classified deterministically, independent of index
        // temperature: the creator's own copy wins; an entry that exists only
        // under a foreign creator gets the same `kb_owner:` Forbidden as the
        // warm index path; only a truly absent entry is NotFound.
        let own_root = creators_root.join(principal.creator_id());
        if let Some((ws_root, candidate)) = scan_creator_workspaces(&own_root, &entry_file) {
            let content =
                std::fs::read_to_string(&candidate).map_err(|e| KnowledgeFault::Internal {
                    code: "FILE_READ_ERROR".into(),
                    message: e.to_string(),
                })?;

            // Read index for metadata
            let index_path = ws_root.join("kb").join("index.json");
            let index = read_kb_index(&index_path);
            let index_entry = index.entries.iter().find(|e| e.entry_id == entry_id);

            let (title, created_at) = index_entry.map_or_else(
                || (entry_id.clone(), String::new()),
                |ie| (ie.title.clone(), ie.created_at.clone()),
            );

            return Ok(GetKbEntryResponse {
                entry_id,
                title,
                created_at,
                content,
            });
        }
        if foreign_creator_holds_entry(&creators_root, principal.creator_id(), &entry_file) {
            return Err(
                KnowledgeFault::ForeignEntry(format!("kb_owner:KB entry {entry_id}")).into(),
            );
        }

        Err(KnowledgeFault::NotFound(format!("KB entry {entry_id} not found")).into())
    }

    /// Delete a single work-scope entry, via the O(1) entry index (QC3 W-005)
    /// with a filesystem-scan fallback for a stale index; ownership is
    /// classified identically in both paths (as [`CoreService::get_kb_entry`]).
    ///
    /// # Errors
    /// As [`CoreService::get_kb_entry`]; additionally
    /// [`CoreError::Forbidden`] under read-only core access.
    pub async fn delete_kb_entry(
        &self,
        principal: &Principal,
        entry_id: String,
    ) -> CoreResult<DeleteKbEntryResponse> {
        self.verify_principal(principal)?;
        self.require_work_write()?;
        validate_entry_id_safe(&entry_id).map_err(|reason| invalid_input("entry_id", reason))?;

        let nexus_root = self.inner.nexus_home.clone();

        // Try index lookup first (O(1)).
        if let Some((creator_id, workspace_slug)) = lookup_entry_location(&entry_id, &nexus_root) {
            if creator_id != principal.creator_id() {
                return Err(
                    KnowledgeFault::ForeignEntry(format!("kb_owner:KB entry {entry_id}")).into(),
                );
            }
            let (_, entries_dir) =
                resolve_kb_paths(&nexus_root, &creator_id, Some(&workspace_slug));
            let candidate = entries_dir.join(format!("{entry_id}.md"));
            if candidate.exists() {
                std::fs::remove_file(&candidate).map_err(|e| KnowledgeFault::Internal {
                    code: "FILE_DELETE_ERROR".into(),
                    message: e.to_string(),
                })?;

                let (kb_dir, _) = resolve_kb_paths(&nexus_root, &creator_id, Some(&workspace_slug));
                let index_path = kb_dir.join("index.json");
                let mut index = read_kb_index(&index_path);
                index.entries.retain(|e| e.entry_id != entry_id);
                if index.entries.is_empty() {
                    let _ = std::fs::remove_file(&index_path);
                } else {
                    write_kb_index(&index_path, &index)?;
                }

                remove_from_kb_entry_index(&entry_id);

                self.verify_principal(principal)?;
                return Ok(DeleteKbEntryResponse {
                    entry_id,
                    deleted: true,
                });
            }
            // Stale index — invalidate and fall through.
            invalidate_kb_entry_index();
        }

        // Slow path: filesystem scan.
        let creators_root = nexus_root.join("creators");
        if !creators_root.is_dir() {
            return Err(KnowledgeFault::NotFound(format!("KB entry {entry_id} not found")).into());
        }

        let entry_file = format!("{entry_id}.md");

        // Ownership is classified deterministically, independent of index
        // temperature: the creator's own copy is deleted; an entry that exists
        // only under a foreign creator gets the same `kb_owner:` Forbidden as
        // the warm index path; only a truly absent entry is NotFound.
        let own_root = creators_root.join(principal.creator_id());
        if let Some((ws_root, candidate)) = scan_creator_workspaces(&own_root, &entry_file) {
            // Remove entry file
            std::fs::remove_file(&candidate).map_err(|e| KnowledgeFault::Internal {
                code: "FILE_DELETE_ERROR".into(),
                message: e.to_string(),
            })?;

            // Update index
            let index_path = ws_root.join("kb").join("index.json");
            let mut index = read_kb_index(&index_path);
            index.entries.retain(|e| e.entry_id != entry_id);
            if index.entries.is_empty() {
                // Clean up empty index
                let _ = std::fs::remove_file(&index_path);
            } else {
                write_kb_index(&index_path, &index)?;
            }

            remove_from_kb_entry_index(&entry_id);

            self.verify_principal(principal)?;
            return Ok(DeleteKbEntryResponse {
                entry_id,
                deleted: true,
            });
        }
        if foreign_creator_holds_entry(&creators_root, principal.creator_id(), &entry_file) {
            return Err(
                KnowledgeFault::ForeignEntry(format!("kb_owner:KB entry {entry_id}")).into(),
            );
        }

        Err(KnowledgeFault::NotFound(format!("KB entry {entry_id} not found")).into())
    }

    // ── User-scoped global knowledge (`creator knowledge`) ──────────────

    /// Store a User-scoped global knowledge entry (entity-scope-model §5.4).
    /// The active creator's default user identity (`user_default` until the
    /// platform `usr_*` mapping exists) owns the entry.
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal fails
    /// verification, [`CoreError::Forbidden`] under read-only core access,
    /// and [`CoreError::InvalidInput`] for store-side validation rejections
    /// (empty user/content).
    pub async fn add_user_knowledge(
        &self,
        principal: &Principal,
        content: String,
        tags: Option<Vec<String>>,
    ) -> CoreResult<UserKnowledgeEntry> {
        self.verify_principal(principal)?;
        self.require_work_write()?;
        let tag_list = tags
            .unwrap_or_default()
            .into_iter()
            .map(|s| KnowledgeTag::new(&s))
            .collect();
        let entry = UserKnowledgeEntry::new(DEFAULT_USER_ID, tag_list, &content);
        let stored = SqliteKnowledgeStore::new(self.inner.pool.clone())
            .store(entry)
            .await
            .map_err(knowledge_err)?;
        self.verify_principal(principal)?;
        Ok(stored)
    }

    /// List User-scoped knowledge entries with optional tag filter and
    /// offset pagination.
    ///
    /// # Errors
    /// As [`CoreService::add_user_knowledge`] minus write validation.
    pub async fn list_user_knowledge(
        &self,
        principal: &Principal,
        tags: Option<Vec<String>>,
        limit: u32,
        offset: u32,
    ) -> CoreResult<KnowledgeResult> {
        self.verify_principal(principal)?;
        let mut query = KnowledgeQuery::for_user(DEFAULT_USER_ID)
            .with_limit(limit)
            .with_offset(offset);
        if let Some(tag_strs) = tags {
            let tag_list: Vec<KnowledgeTag> = tag_strs
                .into_iter()
                .map(|s| KnowledgeTag::new(&s))
                .collect();
            query = query.with_tags(tag_list);
        }
        let result = SqliteKnowledgeStore::new(self.inner.pool.clone())
            .list(&query)
            .await
            .map_err(knowledge_err)?;
        self.verify_principal(principal)?;
        Ok(result)
    }

    /// Text-search User-scoped knowledge entries with optional tag filter.
    ///
    /// # Errors
    /// As [`CoreService::list_user_knowledge`].
    pub async fn search_user_knowledge(
        &self,
        principal: &Principal,
        query_text: &str,
        tags: Option<Vec<String>>,
        limit: u32,
        offset: u32,
    ) -> CoreResult<KnowledgeResult> {
        self.verify_principal(principal)?;
        let tag_refs: Option<Vec<KnowledgeTag>> =
            tags.map(|ts| ts.into_iter().map(|s| KnowledgeTag::new(&s)).collect());
        let result = SqliteKnowledgeStore::new(self.inner.pool.clone())
            .search(
                DEFAULT_USER_ID,
                query_text,
                tag_refs.as_deref(),
                limit,
                offset,
            )
            .await
            .map_err(knowledge_err)?;
        self.verify_principal(principal)?;
        Ok(result)
    }
}

#[cfg(test)]
mod kb_ownership_tests {
    use super::*;

    async fn kb_service(home: &std::path::Path) -> (CoreService, Principal) {
        std::fs::create_dir_all(home.join(".nexus42")).unwrap();
        std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
            home, "author", "default",
        ))
        .unwrap();
        std::fs::write(
            home.join(".nexus42/config.toml"),
            "active_creator_id = \"author\"\n[active_workspace_slug_by_creator]\n\"author\" = \"default\"\n",
        )
        .unwrap();
        {
            let db = nexus_home_layout::workspace_state_db_path(home, "author", "default");
            let seed = nexus_local_db::writer_protocol::init_guarded_pool(&db, "author")
                .await
                .unwrap();
            seed.clone_pool().close().await;
        }
        let core = CoreService::open(crate::CoreOpenOptions {
            user_home: home.to_path_buf(),
            access: crate::CoreAccess::DirectWriter,
        })
        .await
        .unwrap();
        let principal = core.active_principal().await.unwrap();
        (core, principal)
    }

    /// Foreign-creator fixture: `<root>/creators/creator_b/workspaces/default/
    /// kb/entries/<entry_id>.md`, optionally registered in the workspace
    /// `index.json` so an index rebuild resolves it (warm).
    fn foreign_entry(root: &std::path::Path, entry_id: &str, registered: bool) {
        let kb_dir = root
            .join("creators")
            .join("creator_b")
            .join("workspaces")
            .join("default")
            .join("kb");
        std::fs::create_dir_all(kb_dir.join("entries")).unwrap();
        std::fs::write(
            kb_dir.join("entries").join(format!("{entry_id}.md")),
            "foreign body",
        )
        .unwrap();
        if registered {
            let index = KbIndex {
                entries: vec![KbIndexEntry {
                    entry_id: entry_id.to_string(),
                    title: "Foreign".to_string(),
                    created_at: String::new(),
                }],
            };
            std::fs::write(
                kb_dir.join("index.json"),
                serde_json::to_string(&index).unwrap(),
            )
            .unwrap();
        }
    }

    /// GET/DELETE classify foreign vs missing entries identically whether the
    /// process-wide entry index resolves the entry (warm) or the filesystem
    /// scan answers instead (cold/stale): a foreign entry is always Forbidden
    /// with the `kb_owner:` carrier, only a truly absent entry is NotFound.
    #[tokio::test]
    async fn kb_get_delete_ownership_independent_of_index_temperature() {
        let temp = tempfile::tempdir().unwrap();
        let (core, principal) = kb_service(temp.path()).await;
        let root = core.inner.nexus_home.clone();

        // Warm: the index (rebuilt here from index.json) knows the entry.
        invalidate_kb_entry_index();
        foreign_entry(&root, "kb_warmf", true);
        assert!(matches!(
            core.get_kb_entry(&principal, "kb_warmf".into()).await,
            Err(CoreError::Forbidden { ref resource })
                if resource == "kb_owner:KB entry kb_warmf"
        ));

        // Cold: the entry exists on disk but the freshly rebuilt index misses
        // it — the scan fallback must classify the same as the warm path.
        invalidate_kb_entry_index();
        foreign_entry(&root, "kb_coldf", false);
        assert!(matches!(
            core.get_kb_entry(&principal, "kb_coldf".into()).await,
            Err(CoreError::Forbidden { ref resource })
                if resource == "kb_owner:KB entry kb_coldf"
        ));

        // Missing: warm index miss and cold scan miss are both NotFound.
        assert!(matches!(
            core.get_kb_entry(&principal, "kb_warmmiss".into()).await,
            Err(CoreError::NotFound { ref resource }) if resource.contains("kb_warmmiss")
        ));
        invalidate_kb_entry_index();
        assert!(matches!(
            core.get_kb_entry(&principal, "kb_coldmiss".into()).await,
            Err(CoreError::NotFound { ref resource }) if resource.contains("kb_coldmiss")
        ));

        // DELETE: the same four quadrants.
        invalidate_kb_entry_index();
        foreign_entry(&root, "kb_warmd", true);
        assert!(matches!(
            core.delete_kb_entry(&principal, "kb_warmd".into()).await,
            Err(CoreError::Forbidden { ref resource })
                if resource == "kb_owner:KB entry kb_warmd"
        ));

        invalidate_kb_entry_index();
        foreign_entry(&root, "kb_coldd", false);
        assert!(matches!(
            core.delete_kb_entry(&principal, "kb_coldd".into()).await,
            Err(CoreError::Forbidden { ref resource })
                if resource == "kb_owner:KB entry kb_coldd"
        ));

        assert!(matches!(
            core.delete_kb_entry(&principal, "kb_warmmiss".into()).await,
            Err(CoreError::NotFound { ref resource }) if resource.contains("kb_warmmiss")
        ));
        invalidate_kb_entry_index();
        assert!(matches!(
            core.delete_kb_entry(&principal, "kb_coldmiss".into()).await,
            Err(CoreError::NotFound { ref resource }) if resource.contains("kb_coldmiss")
        ));

        core.close().await.unwrap();
    }
}
