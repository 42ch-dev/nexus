//! CLI-local Creator identity cache (V1.16).
//!
//! Stores display/alias metadata keyed by canonical `creator_id` in
//! `~/.nexus42/creator-identities.json`. Pre-1.0 disposable — no
//! generated contract changes.

use crate::config::nexus_home;
use crate::errors::Result;
use std::collections::HashMap;
use std::path::PathBuf;

/// Single creator identity entry in the CLI-local cache.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreatorIdentityEntry {
    /// Canonical creator ID (e.g. `ctr_abc123`).
    pub creator_id: String,
    /// Handle if provided during registration (e.g. `alpha`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handle: Option<String>,
    /// Display name from platform response or registration input.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}

/// CLI-local cache of creator identities, persisted as JSON.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct CreatorIdentityCache {
    /// Keyed by canonical `creator_id`.
    #[serde(default)]
    pub creators: HashMap<String, CreatorIdentityEntry>,
}

/// File name for the identity cache, next to `auth.json` and `config.toml`.
const FILENAME: &str = "creator-identities.json";

/// Return the path to `~/.nexus42/creator-identities.json`.
///
/// # Errors
///
/// Returns an error if the Nexus home directory cannot be resolved.
pub fn cache_path() -> Result<PathBuf> {
    Ok(nexus_home()?.join(FILENAME))
}

/// Load the creator identity cache from disk.
///
/// Returns an empty cache if the file is missing or cannot be parsed.
#[must_use]
pub fn load_creator_identity_cache() -> CreatorIdentityCache {
    let Ok(path) = cache_path() else {
        return CreatorIdentityCache::default();
    };

    if !path.exists() {
        return CreatorIdentityCache::default();
    }

    let Ok(content) = std::fs::read_to_string(&path) else {
        return CreatorIdentityCache::default();
    };

    if content.trim().is_empty() {
        return CreatorIdentityCache::default();
    }

    let mut cache: CreatorIdentityCache = serde_json::from_str(&content).unwrap_or_default();

    // C-001: Validate all creator_id values loaded from disk.
    // A corrupt cache may contain entries with unsafe IDs (path traversal, etc.)
    // that bypass the write-time validate_creator_id_safe() check.
    let invalid_ids: Vec<String> = cache
        .creators
        .keys()
        .filter(|id| crate::paths::validate_creator_id_safe(id).is_err())
        .cloned()
        .collect();

    for id in &invalid_ids {
        tracing::warn!("Removing cache entry with unsafe creator_id: {id:?} — rejected for safety");
        cache.creators.remove(id);
    }

    cache
}

/// Save the creator identity cache to disk atomically.
///
/// # Errors
///
/// Returns an error if the file cannot be written.
pub fn save_creator_identity_cache(cache: &CreatorIdentityCache) -> Result<()> {
    let path = cache_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(cache)?;
    let tmp_path = path.with_extension("json.tmp");
    std::fs::write(&tmp_path, &json)?;
    std::fs::rename(&tmp_path, &path)?;
    Ok(())
}

/// Look up a creator identity by exact `creator_id`.
#[must_use]
pub fn get_creator_identity<'a>(
    cache: &'a CreatorIdentityCache,
    creator_id: &str,
) -> Option<&'a CreatorIdentityEntry> {
    cache.creators.get(creator_id)
}

/// Insert or update a creator identity entry in the cache and persist.
///
/// # Errors
///
/// Returns an error if the cache cannot be saved to disk.
// TODO(V1.17): The load-modify-save pattern here is not atomic — concurrent
// CLI invocations may race and lose updates. Consider file locking or migrating
// the identity cache to SQLite (nexus-local-db) for proper concurrency safety.
pub fn set_creator_identity(entry: CreatorIdentityEntry) -> Result<()> {
    let mut cache = load_creator_identity_cache();
    cache.creators.insert(entry.creator_id.clone(), entry);
    save_creator_identity_cache(&cache)
}

/// Resolve a user-provided `creator_ref` to a canonical `creator_id`.
///
/// Resolution order:
/// 1. Exact match on `creator_id` in the cache → return that ID.
/// 2. Exact match on `handle` in the cache → return the matched `creator_id`.
/// 3. No match but input is path-safe → return input as-is (backward compat).
/// 4. Otherwise → error.
///
/// # Errors
///
/// Returns an error if the input contains unsafe characters and is not found
/// in the identity cache.
pub fn resolve_creator_ref(input: &str) -> Result<String> {
    let cache = load_creator_identity_cache();

    // 1. Exact creator_id match
    if cache.creators.contains_key(input) {
        return Ok(input.to_string());
    }

    // 2. Handle match
    for entry in cache.creators.values() {
        if entry.handle.as_deref() == Some(input) {
            return Ok(entry.creator_id.clone());
        }
    }

    // 3. Path-safe fallback (backward compat)
    if crate::paths::validate_creator_id_safe(input).is_ok() {
        return Ok(input.to_string());
    }

    // 4. Unsafe input — error
    Err(crate::errors::CliError::Other(format!(
        "Unknown creator reference {input:?}. \
         Use `nexus42 creator list` to see available creators, \
         or provide a valid creator ID."
    )))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_entry(
        creator_id: &str,
        handle: Option<&str>,
        display_name: Option<&str>,
    ) -> CreatorIdentityEntry {
        CreatorIdentityEntry {
            creator_id: creator_id.to_string(),
            handle: handle.map(std::string::ToString::to_string),
            display_name: display_name.map(std::string::ToString::to_string),
        }
    }

    #[test]
    fn cache_roundtrip_save_load() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let nexus_dir = tmp.path().join(".nexus42");
        std::fs::create_dir_all(&nexus_dir).expect("create nexus dir");

        let original_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", tmp.path());

        let mut cache = CreatorIdentityCache::default();
        cache.creators.insert(
            "ctr_alpha".to_string(),
            sample_entry("ctr_alpha", Some("alpha"), Some("Alpha Creator")),
        );
        cache
            .creators
            .insert("ctr_beta".to_string(), sample_entry("ctr_beta", None, None));
        save_creator_identity_cache(&cache).expect("save");

        let loaded = load_creator_identity_cache();
        assert_eq!(loaded.creators.len(), 2);
        let alpha = loaded.creators.get("ctr_alpha").expect("alpha");
        assert_eq!(alpha.handle.as_deref(), Some("alpha"));
        assert_eq!(alpha.display_name.as_deref(), Some("Alpha Creator"));

        let beta = loaded.creators.get("ctr_beta").expect("beta");
        assert!(beta.handle.is_none());

        if let Some(home) = original_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }
    }

    #[test]
    fn load_missing_file_returns_empty() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let original_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", tmp.path());

        let cache = load_creator_identity_cache();
        assert!(cache.creators.is_empty());

        if let Some(home) = original_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }
    }

    #[test]
    fn load_corrupt_file_returns_empty() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let nexus_dir = tmp.path().join(".nexus42");
        std::fs::create_dir_all(&nexus_dir).expect("create nexus dir");
        std::fs::write(nexus_dir.join(FILENAME), "not valid json{{{").expect("write corrupt");

        let original_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", tmp.path());

        let cache = load_creator_identity_cache();
        assert!(cache.creators.is_empty());

        if let Some(home) = original_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }
    }

    #[test]
    fn resolve_exact_creator_id() {
        // Build cache in-memory, test the matching logic directly.
        let mut cache = CreatorIdentityCache::default();
        cache.creators.insert(
            "ctr_alpha".to_string(),
            sample_entry("ctr_alpha", Some("alpha"), None),
        );

        // Step 1: exact creator_id match
        assert!(cache.creators.contains_key("ctr_alpha"));
        let matched = cache.creators.get("ctr_alpha").expect("entry");
        assert_eq!(matched.creator_id, "ctr_alpha");
    }

    #[test]
    fn resolve_by_handle_in_memory() {
        let mut cache = CreatorIdentityCache::default();
        cache.creators.insert(
            "ctr_alpha".to_string(),
            sample_entry("ctr_alpha", Some("alpha"), None),
        );

        // Step 2: handle match — loop through entries
        let found = cache
            .creators
            .values()
            .find(|e| e.handle.as_deref() == Some("alpha"));
        assert!(found.is_some());
        assert_eq!(found.expect("entry").creator_id, "ctr_alpha");
    }

    #[test]
    fn resolve_by_handle_via_disk() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let nexus_dir = tmp.path().join(".nexus42");
        std::fs::create_dir_all(&nexus_dir).expect("create nexus dir");

        let original_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", tmp.path());

        let mut cache = CreatorIdentityCache::default();
        cache.creators.insert(
            "ctr_alpha".to_string(),
            sample_entry("ctr_alpha", Some("alpha_handle_unique"), None),
        );
        save_creator_identity_cache(&cache).expect("save");

        // "alpha_handle_unique" should resolve to "ctr_alpha"
        let result = resolve_creator_ref("alpha_handle_unique").expect("resolve handle");
        assert_eq!(result, "ctr_alpha");

        if let Some(home) = original_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }
    }

    #[test]
    fn resolve_path_safe_unknown_passthrough() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let nexus_dir = tmp.path().join(".nexus42");
        std::fs::create_dir_all(&nexus_dir).expect("create nexus dir");

        let original_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", tmp.path());

        // Empty cache — unknown but path-safe input should pass through
        let cache = CreatorIdentityCache::default();
        save_creator_identity_cache(&cache).expect("save");

        let result = resolve_creator_ref("ctr_brand_new").expect("resolve");
        assert_eq!(result, "ctr_brand_new");

        if let Some(home) = original_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }
    }

    #[test]
    fn resolve_unsafe_input_errors() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let nexus_dir = tmp.path().join(".nexus42");
        std::fs::create_dir_all(&nexus_dir).expect("create nexus dir");

        let original_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", tmp.path());

        let cache = CreatorIdentityCache::default();
        save_creator_identity_cache(&cache).expect("save");

        let result = resolve_creator_ref("../etc");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Unknown creator reference"));

        if let Some(home) = original_home {
            std::env::set_var("HOME", home);
        } else {
            std::env::remove_var("HOME");
        }
    }
}
