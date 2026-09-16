//! Creator identity family (list/create/status/patch/use/logout) owned by
//! `CoreHomeService` (v1.190 P2-T1).
//!
//! Extracted from the daemon `creators` HTTP handlers: profile-membership
//! SSOT scanning (`<nexus_home>/creators/<id>/`), the creator identity cache
//! and credential auth store, the workspace `creators` SQL enrichment, and
//! the active-creator selection write. The daemon handlers keep only
//! auth/DTO/status translation, so HTTP status and response envelopes are
//! unchanged (responses now come from the generated creator DTOs, the single
//! schema-derived shape).
//!
//! Home-level operations hold no workspace pool; the workspace `creators`
//! table is touched through a transient pool on the resolved active
//! workspace state DB only where the daemon used its attached pool
//! (list backfill, create insert, patch upsert).

use std::collections::HashMap;
use std::path::Path;

use nexus_contracts::generated::daemon_api::creators::{
    active_creator_response::ActiveCreatorResponse,
    list_creators_query::ListCreatorsQuery,
    list_creators_response::{ListCreatorsResponse, NexusCreatorInfo, NexusPaginationInfo},
    logout_response::LogoutResponse,
    set_active_creator_request::SetActiveCreatorRequest,
    set_active_creator_response::SetActiveCreatorResponse,
};
use nexus_contracts::CreatorDetail;
use nexus_home_layout::active_context::{read_active_creator_id, try_resolve_state_db_path};
use nexus_home_layout::validate_creator_id_safe;

use crate::error::{db_err, CoreError, CoreResult};
use crate::home::CoreHomeService;
use sqlx::Row;

/// Maximum creator display name length accepted by this family — parity with
/// the daemon handler rule (char count, so CJK / emoji count once).
pub const MAX_DISPLAY_NAME_CHARS: usize = 256;

/// Wire-internal code carriers this family re-sends verbatim at the adapter:
/// the daemon handler built `Internal { code, message }` envelopes for cache,
/// config and profile-home failures, so the core carries
/// `"{CODE}: {message}"` categories and the adapter matches each prefix
/// exactly (the `database_error:` convention from P2-T0).
pub const CREATOR_INTERNAL_CODES: &[&str] = &[
    "CONFIG_READ_ERROR",
    "CONFIG_PARSE_ERROR",
    "CONFIG_SERIALIZE_ERROR",
    "CONFIG_WRITE_ERROR",
    "CONFIG_ERROR",
    "CACHE_READ_ERROR",
    "CACHE_PARSE_ERROR",
    "CACHE_DIR_ERROR",
    "CACHE_SERIALIZE_ERROR",
    "CACHE_WRITE_ERROR",
    "CACHE_FORMAT_ERROR",
    "PROFILE_HOME_ERROR",
    "CREATOR_ID_GENERATION_ERROR",
];

fn internal(code: &str, message: impl std::fmt::Display) -> CoreError {
    CoreError::Internal {
        category: format!("{code}: {message}"),
    }
}

impl CoreHomeService {
    /// List the registered creator Profiles (membership SSOT: directories
    /// under `<nexus_home>/creators/<id>/`), enriched with display metadata
    /// from the active workspace `creators` SQL rows and the identity cache.
    /// Orphan SQL/cache rows stay invisible until a matching Profile home
    /// exists on disk.
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the config snapshot cannot
    /// be read, the mapped internal carriers when the identity cache or the
    /// workspace enrichment fails, and [`CoreError::Uninitialized`] when the
    /// SQL enrichment path cannot resolve its store.
    pub async fn list_creators(
        &self,
        query: ListCreatorsQuery,
    ) -> CoreResult<ListCreatorsResponse> {
        let limit = query.limit.unwrap_or(50).clamp(1, 250) as usize;

        // Membership SSOT: on-disk Profile homes only.
        let ssot_ids = list_profile_ids_ssot(&self.nexus_home);

        // Secondary enrichment map (may contain dirty/orphan rows — ignored
        // unless the id is in the SSOT set).
        let identity_cache = load_identity_cache(&self.nexus_home);
        let mut sql_by_id: HashMap<String, NexusCreatorInfo> = HashMap::new();
        if let Some(db_path) = self.active_workspace_db_path() {
            let pool = nexus_local_db::init_pool(&db_path)
                .await
                .map_err(crate::error::local_db_err)?;
            let rows = sql_creator_rows(&pool).await;
            match rows {
                Ok(rows) => {
                    for row in rows {
                        sql_by_id.insert(row.creator_id.clone(), row);
                    }
                    // Complete the active-pool cache from SSOT (never the reverse).
                    for creator_id in &ssot_ids {
                        if sql_by_id.contains_key(creator_id) {
                            continue;
                        }
                        let enriched = enrich_profile(creator_id, &sql_by_id, &identity_cache);
                        if let Err(err) =
                            upsert_creator_display_name(&pool, creator_id, &enriched.display_name)
                                .await
                        {
                            pool.close().await;
                            return Err(err);
                        }
                        sql_by_id.insert(creator_id.clone(), enriched);
                    }
                }
                Err(err) => {
                    pool.close().await;
                    return Err(err);
                }
            }
            pool.close().await;
        }

        let mut items: Vec<NexusCreatorInfo> = ssot_ids
            .iter()
            .map(|id| enrich_profile(id, &sql_by_id, &identity_cache))
            .collect();
        items.sort_by(|a, b| {
            b.cached_at
                .as_deref()
                .cmp(&a.cached_at.as_deref())
                .then_with(|| a.creator_id.cmp(&b.creator_id))
        });

        // Apply cursor-based pagination (cursor = creator_id).
        if let Some(ref cursor) = query.cursor {
            let pos = items.iter().position(|i| &i.creator_id == cursor);
            if let Some(idx) = pos {
                items = items.split_off(idx + 1);
            }
        }

        let next_cursor = if items.len() > limit {
            items.truncate(limit);
            items.last().map(|i| i.creator_id.clone())
        } else {
            None
        };

        let pagination: NexusPaginationInfo = NexusPaginationInfo::builder()
            .limit(i64::try_from(limit).unwrap_or(i64::MAX))
            .has_more(next_cursor.is_some())
            .next_cursor(next_cursor)
            .try_into()
            .expect("core pagination info is wire-valid");
        Ok(ListCreatorsResponse::builder()
            .items(items)
            .pagination(pagination)
            .try_into()
            .expect("core creator list is wire-valid"))
    }

    /// Create a new local creator profile: validate the display name, mint a
    /// `ctr_local…` id, materialize the membership-SSOT Profile home, insert
    /// the workspace `creators` row and return the detail-shaped result.
    ///
    /// # Errors
    /// Returns [`CoreError::InvalidInput`] for an empty/over-long display
    /// name, [`CoreError::Uninitialized`] when no active creator/workspace
    /// resolves for the SQL insert, and the mapped internal carriers when the
    /// Profile home or the insert fails.
    pub async fn create_creator(&self, display_name: String) -> CoreResult<CreatorDetail> {
        let display_name = validated_display_name(&display_name)?;

        // Generate a creator id matching the `^ctr_[a-zA-Z0-9]+$` pattern used
        // by `nexus-creator::local_identity::generate_local_id`; the pattern
        // itself is the public contract.
        let random: String = uuid::Uuid::new_v4()
            .to_string()
            .replace('-', "")
            .chars()
            .take(12)
            .collect();
        let creator_id = format!("ctr_local{random}");
        // Defensive: the generated id always matches the safe-id check, but
        // run it anyway so future id-shape changes cannot bypass the
        // path-traversal guard.
        validate_creator_id_safe(&creator_id).map_err(|reason| CoreError::Internal {
            category: format!("CREATOR_ID_GENERATION_ERROR: {reason}"),
        })?;

        // Membership SSOT write — Profile is real only when the home dir exists.
        ensure_profile_home_ssot(&self.nexus_home, &creator_id)?;

        let db_path = self
            .active_workspace_db_path()
            .ok_or(CoreError::Uninitialized)?;
        let pool = nexus_local_db::init_pool(&db_path)
            .await
            .map_err(crate::error::local_db_err)?;
        let now = chrono::Utc::now().to_rfc3339();
        let insert = sqlx::query(
            "INSERT INTO creators (creator_id, display_name, status, cached_at, data) \
             VALUES (?, ?, 'active', ?, '{}')",
        )
        .bind(&creator_id)
        .bind(&display_name)
        .bind(&now)
        .execute(&pool)
        .await;
        if let Err(e) = insert {
            pool.close().await;
            return Err(db_err(&e));
        }
        pool.close().await;

        Ok(CreatorDetail {
            creator_id,
            handle: None,
            display_name: Some(display_name),
            has_api_key: false,
            has_cached_token: false,
            is_active: false,
        })
    }

    /// Project the status/detail of one creator: identity-cache metadata,
    /// credential presence from the auth store, and active-selection state.
    ///
    /// # Errors
    /// Returns [`CoreError::InvalidInput`] for unsafe or verb-shaped ids.
    pub fn creator_detail(&self, creator_id: &str) -> CoreResult<CreatorDetail> {
        reject_colon_verb_segment(creator_id)?;
        validate_creator_id_safe(creator_id).map_err(|reason| CoreError::InvalidInput {
            field: "creator_id".to_string(),
            reason,
        })?;
        let cache = load_identity_cache(&self.nexus_home);
        let entry = get_identity_entry(&cache, creator_id);
        let auth_store = load_auth_store(&self.nexus_home);
        let active_id = read_active_creator_id(&self.nexus_home);
        Ok(creator_detail_from_parts(
            creator_id,
            entry.as_ref(),
            &auth_store,
            active_id.as_deref(),
        ))
    }

    /// Patch a creator's display name (identity cache + workspace `creators`
    /// row), materializing the membership SSOT when missing.
    ///
    /// # Errors
    /// Returns [`CoreError::InvalidInput`] for unsafe ids or an empty/
    /// over-long display name, [`CoreError::Uninitialized`] when the SQL
    /// upsert cannot resolve its store, and the mapped internal carriers
    /// otherwise.
    pub async fn patch_creator(
        &self,
        creator_id: &str,
        display_name: Option<String>,
    ) -> CoreResult<CreatorDetail> {
        reject_colon_verb_segment(creator_id)?;
        validate_creator_id_safe(creator_id).map_err(|reason| CoreError::InvalidInput {
            field: "creator_id".to_string(),
            reason,
        })?;

        // Author-facing PATCH materializes membership SSOT when missing
        // (enrichment targets only exist for Profiles that have a home).
        ensure_profile_home_ssot(&self.nexus_home, creator_id)?;

        if let Some(ref display_name) = display_name {
            if display_name.is_empty() {
                return Err(CoreError::InvalidInput {
                    field: "display_name".to_string(),
                    reason: "display_name cannot be empty".to_string(),
                });
            }
            if display_name.chars().count() > MAX_DISPLAY_NAME_CHARS {
                return Err(CoreError::InvalidInput {
                    field: "display_name".to_string(),
                    reason: "display_name must be 256 characters or fewer".to_string(),
                });
            }
        }

        let cache_path = self.nexus_home.join("creator_identity_cache.json");
        // Only initialize a fresh cache when the file does not exist. If the
        // file exists but cannot be parsed, report an error instead of
        // silently wiping all cached identities (QC2-F-002).
        let mut cache = if cache_path.exists() {
            load_identity_cache_strict(&cache_path)?
        } else {
            serde_json::json!({"creators": {}})
        };

        if !cache.is_object() {
            return Err(internal(
                "CACHE_FORMAT_ERROR",
                "Identity cache root is not an object",
            ));
        }
        if cache.get("creators").is_none_or(|v| !v.is_object()) {
            return Err(internal(
                "CACHE_FORMAT_ERROR",
                "Identity cache creators field is not an object",
            ));
        }

        if let Some(ref display_name) = display_name {
            let db_path = self
                .active_workspace_db_path()
                .ok_or(CoreError::Uninitialized)?;
            let pool = nexus_local_db::init_pool(&db_path)
                .await
                .map_err(crate::error::local_db_err)?;
            let upsert = upsert_creator_display_name(&pool, creator_id, display_name).await;
            if let Err(err) = upsert {
                pool.close().await;
                return Err(err);
            }
            pool.close().await;
            // Write the display name to the cache as well so the rename is
            // reflected by every consumer (QC1-F-001).
            if let Some(entry) = cache
                .get_mut("creators")
                .and_then(|v| v.as_object_mut())
                .and_then(|creators| creators.get_mut(creator_id))
            {
                if let Some(entry_obj) = entry.as_object_mut() {
                    entry_obj.insert(
                        "display_name".to_string(),
                        serde_json::Value::String(display_name.clone()),
                    );
                }
            } else {
                // Ensure a minimal entry exists before inserting the name.
                if let Some(creators) = cache.get_mut("creators").and_then(|v| v.as_object_mut()) {
                    let entry = creators
                        .entry(creator_id.to_string())
                        .or_insert_with(|| serde_json::json!({}));
                    if let Some(entry_obj) = entry.as_object_mut() {
                        entry_obj.insert(
                            "display_name".to_string(),
                            serde_json::Value::String(display_name.clone()),
                        );
                    }
                }
            }
        }

        save_identity_cache(&cache_path, &cache)?;

        // Re-read the updated cache so the returned detail reflects the write.
        let cache = load_identity_cache(&self.nexus_home);
        let entry = get_identity_entry(&cache, creator_id);
        let auth_store = load_auth_store(&self.nexus_home);
        let active_id = read_active_creator_id(&self.nexus_home);
        Ok(creator_detail_from_parts(
            creator_id,
            entry.as_ref(),
            &auth_store,
            active_id.as_deref(),
        ))
    }

    /// Use (activate) a creator: verify it is a known member (auth store or
    /// identity cache), write the active-creator selection and reset its
    /// active workspace slug to `default` so the config stays
    /// self-describing.
    ///
    /// # Errors
    /// Returns [`CoreError::InvalidInput`] for unsafe ids,
    /// [`CoreError::NotFound`] when the creator is unknown, and the mapped
    /// internal carriers when the config write fails.
    pub fn use_creator(
        &self,
        request: SetActiveCreatorRequest,
    ) -> CoreResult<SetActiveCreatorResponse> {
        validate_creator_id_safe(&request.creator_id).map_err(|reason| {
            CoreError::InvalidInput {
                field: "creator_id".to_string(),
                reason,
            }
        })?;

        // Verify the creator has credentials stored.
        let auth_store = load_auth_store(&self.nexus_home);
        let cache = load_identity_cache(&self.nexus_home);
        let in_auth = auth_store
            .get("creators")
            .and_then(|c| c.as_object())
            .is_some_and(|obj| obj.contains_key(&request.creator_id));
        let in_cache = get_identity_entry(&cache, &request.creator_id).is_some();
        if !in_auth && !in_cache {
            return Err(CoreError::NotFound {
                resource: format!("Creator {} not found. Register first.", request.creator_id),
            });
        }

        set_active_creator_id(&self.nexus_home, &request.creator_id)?;

        // Reset workspace slug for this creator to the default. Profile switch
        // previously *removed* the entry and relied on read-path fallback;
        // write `"default"` explicitly so config stays self-describing and
        // older read paths that lack the fallback do not surface AuthRequired.
        let mut config = read_cli_config(&self.nexus_home)?;
        if let Some(table) = config.as_table_mut() {
            if table.get("active_workspace_slug_by_creator").is_none() {
                table.insert(
                    "active_workspace_slug_by_creator".to_string(),
                    toml::Value::Table(toml::map::Map::new()),
                );
            }
            if let Some(slug_table) = table.get_mut("active_workspace_slug_by_creator") {
                if let Some(slugs) = slug_table.as_table_mut() {
                    slugs.insert(
                        request.creator_id.clone(),
                        toml::Value::String("default".to_string()),
                    );
                }
            }
            write_cli_config(&self.nexus_home, &config)?;
        }

        Ok(SetActiveCreatorResponse {
            creator_id: request.creator_id,
        })
    }

    /// Project the active creator (id + cached handle/display name).
    ///
    /// # Errors
    /// Returns [`CoreError::NotFound`] when no active creator is selected.
    pub fn active_creator(&self) -> CoreResult<ActiveCreatorResponse> {
        let creator_id =
            read_active_creator_id(&self.nexus_home).ok_or_else(|| CoreError::NotFound {
                resource: "No active creator selected".to_string(),
            })?;
        let cache = load_identity_cache(&self.nexus_home);
        let entry = get_identity_entry(&cache, &creator_id);
        Ok(ActiveCreatorResponse {
            creator_id,
            handle: entry.as_ref().and_then(|e| e.handle.clone()),
            display_name: entry.and_then(|e| e.display_name),
        })
    }

    /// Log a creator out: clear its credentials from the auth store and clear
    /// the active selection when it pointed at this creator.
    ///
    /// # Errors
    /// Returns [`CoreError::InvalidInput`] for unsafe ids and the mapped
    /// internal carriers when the auth store or config write fails.
    pub fn logout_creator(&self, creator_id: &str) -> CoreResult<LogoutResponse> {
        if creator_id.is_empty() || creator_id.contains(':') {
            return Err(CoreError::InvalidInput {
                field: "creator_id".to_string(),
                reason: "creator_id must not be empty or contain ':'".to_string(),
            });
        }
        validate_creator_id_safe(creator_id).map_err(|reason| CoreError::InvalidInput {
            field: "creator_id".to_string(),
            reason,
        })?;

        let cleared = clear_creator_credentials(&self.nexus_home, creator_id)?;

        // If this was the active creator, clear the active selection.
        if let Some(active) = read_active_creator_id(&self.nexus_home) {
            if active == creator_id {
                let mut config = read_cli_config(&self.nexus_home)?;
                if let Some(table) = config.as_table_mut() {
                    table.remove("active_creator_id");
                    write_cli_config(&self.nexus_home, &config)?;
                }
            }
        }

        Ok(LogoutResponse {
            creator_id: creator_id.to_string(),
            cleared,
        })
    }

    /// Resolve the active workspace state DB for the SQL-backed creator
    /// family paths (`None` before any workspace is selected — the daemon
    /// Tier-0 shape).
    fn active_workspace_db_path(&self) -> Option<std::path::PathBuf> {
        read_active_creator_id(&self.nexus_home)?;
        try_resolve_state_db_path(&self.user_home, &self.nexus_home)
            .filter(|db_path| db_path.exists())
    }
}

/// Validate a display name for create/patch: non-empty after trim and at most
/// 256 characters (by char count, so CJK / emoji count once).
fn validated_display_name(raw: &str) -> CoreResult<String> {
    let display_name = raw.trim().to_string();
    if display_name.is_empty() {
        return Err(CoreError::InvalidInput {
            field: "display_name".to_string(),
            reason: "display_name cannot be empty".to_string(),
        });
    }
    if display_name.chars().count() > MAX_DISPLAY_NAME_CHARS {
        return Err(CoreError::InvalidInput {
            field: "display_name".to_string(),
            reason: "display_name must be 256 characters or fewer".to_string(),
        });
    }
    Ok(display_name)
}

fn creator_detail_from_parts(
    creator_id: &str,
    entry: Option<&IdentityEntry>,
    auth_store: &serde_json::Value,
    active_id: Option<&str>,
) -> CreatorDetail {
    CreatorDetail {
        creator_id: creator_id.to_string(),
        handle: entry.and_then(|e| e.handle.clone()),
        display_name: entry.and_then(|e| e.display_name.clone()),
        has_api_key: has_creator_api_key(auth_store, creator_id),
        has_cached_token: has_cached_token(auth_store, creator_id),
        is_active: active_id == Some(creator_id),
    }
}

/// Reject path segments that look like Google-AIP custom verbs (`id:verb`):
/// those URLs share the `:creator_id` capture with logout; GET/PATCH must not
/// treat `ctr_x:logout` as a valid creator id (ghost 200).
pub fn reject_colon_verb_segment(creator_id: &str) -> CoreResult<()> {
    if creator_id.contains(':') {
        return Err(CoreError::InvalidInput {
            field: "creator_id".to_string(),
            reason: "creator_id must not contain ':'".to_string(),
        });
    }
    Ok(())
}

/// Profile membership SSOT: directories under `<nexus_home>/creators/<id>/`.
/// The active workspace `creators` SQL table and
/// `creator_identity_cache.json` are **enrichment only** — they never expand
/// membership.
fn list_profile_ids_ssot(nexus_home: &Path) -> Vec<String> {
    let creators_dir = nexus_home.join("creators");
    let Ok(entries) = std::fs::read_dir(&creators_dir) else {
        return Vec::new();
    };
    let mut ids = Vec::new();
    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let Some(id) = name.to_str() else {
            continue;
        };
        if validate_creator_id_safe(id).is_err() {
            continue;
        }
        ids.push(id.to_string());
    }
    ids.sort();
    ids
}

/// Ensure a Profile home exists on disk (membership SSOT write).
fn ensure_profile_home_ssot(nexus_home: &Path, creator_id: &str) -> CoreResult<()> {
    validate_creator_id_safe(creator_id).map_err(|reason| CoreError::InvalidInput {
        field: "creator_id".to_string(),
        reason,
    })?;
    let profile_home = nexus_home.join("creators").join(creator_id);
    std::fs::create_dir_all(profile_home.join("workspaces").join("default"))
        .map_err(|e| internal("PROFILE_HOME_ERROR", e))?;
    Ok(())
}

/// Enrich a SSOT Profile id with display metadata from secondary sources.
/// Priority: active-pool SQL row → identity cache → creator id fallback.
/// Never invents membership.
fn enrich_profile(
    creator_id: &str,
    sql_by_id: &HashMap<String, NexusCreatorInfo>,
    identity_cache: &serde_json::Value,
) -> NexusCreatorInfo {
    if let Some(row) = sql_by_id.get(creator_id) {
        return row.clone();
    }
    let display_name = get_identity_entry(identity_cache, creator_id)
        .and_then(|entry| entry.display_name)
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(|| creator_id.to_string());
    NexusCreatorInfo::builder()
        .creator_id(creator_id)
        .display_name(display_name)
        .status("active")
        .try_into()
        .expect("core-minted creator info is wire-valid")
}

#[derive(Clone)]
pub struct IdentityEntry {
    pub(crate) handle: Option<String>,
    pub(crate) display_name: Option<String>,
}

/// Read the CLI config from `nexus_home`.
pub fn read_cli_config(nexus_home: &Path) -> CoreResult<toml::Value> {
    let config_path = nexus_home.join("config.toml");
    if !config_path.exists() {
        return Ok(toml::Value::Table(toml::map::Map::new()));
    }
    let content =
        std::fs::read_to_string(&config_path).map_err(|e| internal("CONFIG_READ_ERROR", e))?;
    if content.trim().is_empty() {
        return Ok(toml::Value::Table(toml::map::Map::new()));
    }
    toml::from_str(&content).map_err(|e| internal("CONFIG_PARSE_ERROR", e))
}

/// Write the CLI config to `nexus_home`.
pub fn write_cli_config(nexus_home: &Path, config: &toml::Value) -> CoreResult<()> {
    let config_path = nexus_home.join("config.toml");
    let toml_str =
        toml::to_string_pretty(config).map_err(|e| internal("CONFIG_SERIALIZE_ERROR", e))?;
    std::fs::write(&config_path, toml_str).map_err(|e| internal("CONFIG_WRITE_ERROR", e))
}

/// Set the active `creator_id` in the CLI config.
pub fn set_active_creator_id(nexus_home: &Path, creator_id: &str) -> CoreResult<()> {
    validate_creator_id_safe(creator_id).map_err(|reason| CoreError::InvalidInput {
        field: "creator_id".to_string(),
        reason,
    })?;
    let mut config = read_cli_config(nexus_home)?;
    let table = config
        .as_table_mut()
        .ok_or_else(|| internal("CONFIG_ERROR", "config root is not a table"))?;
    table.insert(
        "active_creator_id".to_string(),
        toml::Value::String(creator_id.to_string()),
    );
    write_cli_config(nexus_home, &config)
}

/// Load the creator identity cache (`Value::Null` for missing/unparseable —
/// treated as a missing cache).
pub fn load_identity_cache(nexus_home: &Path) -> serde_json::Value {
    let cache_path = nexus_home.join("creator_identity_cache.json");
    if !cache_path.exists() {
        return serde_json::Value::Null;
    }
    let Ok(content) = std::fs::read_to_string(&cache_path) else {
        return serde_json::Value::Null;
    };
    serde_json::from_str(&content).unwrap_or(serde_json::Value::Null)
}

/// Load the identity cache from disk, reporting parse/read errors instead of
/// treating them as a missing cache.
pub fn load_identity_cache_strict(cache_path: &Path) -> CoreResult<serde_json::Value> {
    let content =
        std::fs::read_to_string(cache_path).map_err(|e| internal("CACHE_READ_ERROR", e))?;
    serde_json::from_str(&content).map_err(|e| internal("CACHE_PARSE_ERROR", e))
}

/// Crash-safe file write: write to a sibling temp file on the same
/// filesystem, then rename it into place (`rename` is atomic on POSIX).
fn atomic_write(path: &Path, contents: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, path)
}

/// Write the identity cache to disk with an atomic temp-file + rename.
pub fn save_identity_cache(cache_path: &Path, cache: &serde_json::Value) -> CoreResult<()> {
    if let Some(parent) = cache_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| internal("CACHE_DIR_ERROR", e))?;
    }
    let json =
        serde_json::to_string_pretty(cache).map_err(|e| internal("CACHE_SERIALIZE_ERROR", e))?;
    atomic_write(cache_path, &json).map_err(|e| internal("CACHE_WRITE_ERROR", e))
}

/// Update the SQL `creators` row for `creator_id`, inserting a minimal active
/// row if one does not exist.
async fn upsert_creator_display_name(
    pool: &sqlx::SqlitePool,
    creator_id: &str,
    display_name: &str,
) -> CoreResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let rows =
        sqlx::query("UPDATE creators SET display_name = ?, cached_at = ? WHERE creator_id = ?")
            .bind(display_name)
            .bind(&now)
            .bind(creator_id)
            .execute(pool)
            .await
            .map_err(|e| db_err(&e))?;
    if rows.rows_affected() == 0 {
        sqlx::query(
            "INSERT INTO creators (creator_id, display_name, status, cached_at, data) \
             VALUES (?, ?, 'active', ?, '{}')",
        )
        .bind(creator_id)
        .bind(display_name)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|e| db_err(&e))?;
    }
    Ok(())
}

/// Active-workspace `creators` rows ordered by recency (retained list order
/// input). Failures surface as the retained `DATABASE_ERROR` carrier.
async fn sql_creator_rows(pool: &sqlx::SqlitePool) -> CoreResult<Vec<NexusCreatorInfo>> {
    let rows = sqlx::query(
        "SELECT creator_id, display_name, status, cached_at FROM creators \
         ORDER BY cached_at DESC",
    )
    .fetch_all(pool)
    .await
    .map_err(|e| db_err(&e))?;
    let mut items = Vec::with_capacity(rows.len());
    for row in rows {
        let creator_id: String = row.try_get("creator_id").map_err(|e| db_err(&e))?;
        let display_name: String = row.try_get("display_name").map_err(|e| db_err(&e))?;
        let status: String = row.try_get("status").map_err(|e| db_err(&e))?;
        let cached_at: Option<String> = row.try_get("cached_at").map_err(|e| db_err(&e))?;
        items.push(
            NexusCreatorInfo::builder()
                .creator_id(creator_id)
                .display_name(display_name)
                .status(status)
                .cached_at(cached_at)
                .try_into()
                .expect("storage-backed creator info is wire-valid"),
        );
    }
    Ok(items)
}

/// Get the identity cache entry for a creator.
pub fn get_identity_entry(
    cache: &serde_json::Value,
    creator_id: &str,
) -> Option<IdentityEntry> {
    let creators = cache.get("creators")?.as_object()?;
    let entry = creators.get(creator_id)?;
    Some(IdentityEntry {
        handle: entry
            .get("handle")
            .and_then(|v| v.as_str())
            .map(String::from),
        display_name: entry
            .get("display_name")
            .and_then(|v| v.as_str())
            .map(String::from),
    })
}

/// Load the auth store to check credentials (`Value::Null` for
/// missing/unparseable).
pub fn load_auth_store(nexus_home: &Path) -> serde_json::Value {
    let auth_path = nexus_home.join("auth.json");
    if !auth_path.exists() {
        return serde_json::Value::Null;
    }
    let Ok(content) = std::fs::read_to_string(&auth_path) else {
        return serde_json::Value::Null;
    };
    serde_json::from_str(&content).unwrap_or(serde_json::Value::Null)
}

/// Check if a creator has an API key stored.
fn has_creator_api_key(auth_store: &serde_json::Value, creator_id: &str) -> bool {
    auth_store
        .get("creators")
        .and_then(|c| c.get(creator_id))
        .and_then(|e| e.get("creator_api_key"))
        .and_then(|v| v.as_str())
        .is_some_and(|s| !s.is_empty())
}

/// Check if a creator has a cached access token.
fn has_cached_token(auth_store: &serde_json::Value, creator_id: &str) -> bool {
    auth_store
        .get("creators")
        .and_then(|c| c.get(creator_id))
        .and_then(|e| e.get("access_token"))
        .and_then(|v| v.as_str())
        .is_some_and(|s| !s.is_empty())
}

/// Remove a creator's credentials from the auth store.
fn clear_creator_credentials(nexus_home: &Path, creator_id: &str) -> CoreResult<bool> {
    let auth_path = nexus_home.join("auth.json");
    if !auth_path.exists() {
        return Ok(false);
    }
    let content =
        std::fs::read_to_string(&auth_path).map_err(|e| internal("AUTH_READ_ERROR", e))?;
    let mut store: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| internal("AUTH_PARSE_ERROR", e))?;

    let removed = store
        .get_mut("creators")
        .and_then(|c| c.as_object_mut())
        .is_some_and(|creators| creators.remove(creator_id).is_some());

    if removed {
        let json = serde_json::to_string_pretty(&store)
            .map_err(|e| internal("AUTH_SERIALIZE_ERROR", e))?;
        std::fs::write(&auth_path, json).map_err(|e| internal("AUTH_WRITE_ERROR", e))?;
    }

    Ok(removed)
}
