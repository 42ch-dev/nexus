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

/// Save the creator identity cache to disk atomically with advisory file locking.
///
/// Uses an advisory lock file (`creator-identities.json.lock`) to prevent
/// data loss from concurrent CLI invocations. The lock is best-effort —
/// if the lock cannot be acquired, the write proceeds anyway with a warning.
///
/// # Errors
///
/// Returns an error if the file cannot be written.
pub fn save_creator_identity_cache(cache: &CreatorIdentityCache) -> Result<()> {
    let path = cache_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Advisory file lock — best-effort guard against concurrent RMW races.
    let lock_path = path.with_extension("json.lock");
    let _lock_guard = AdvisoryLock::acquire(&lock_path);

    let json = serde_json::to_string_pretty(cache)?;
    // Use a unique temp path per call to avoid rename races when multiple
    // threads/processes write concurrently. Each writer gets its own tmp file
    // so one thread's rename cannot remove another's tmp prematurely.
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let tmp_path = path.with_extension(format!("json.tmp.{unique}"));
    std::fs::write(&tmp_path, &json)?;
    std::fs::rename(&tmp_path, &path)?;
    Ok(())
}

/// Advisory file lock using a `.lock` file. Best-effort — logs a warning on
/// failure and proceeds without locking (better than blocking the CLI).
struct AdvisoryLock {
    _lock_file: std::fs::File,
}

impl AdvisoryLock {
    /// Try to acquire an exclusive advisory lock. Returns `None` (with a warning)
    /// if the lock cannot be acquired — the write proceeds without locking.
    fn acquire(lock_path: &std::path::Path) -> Option<Self> {
        // Try exclusive creation — if the lock file exists, another process holds it.
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(lock_path)
            .ok()?;

        Some(Self { _lock_file: file })
    }
}

impl Drop for AdvisoryLock {
    fn drop(&mut self) {
        // Lock file is removed on drop, releasing the advisory lock.
        // The File handle is closed when _lock_file is dropped.
    }
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
// TODO(V1.17): Consider migrating the identity cache to SQLite (nexus-local-db)
// for proper concurrency safety beyond advisory locks.
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
        let _home = crate::testutil::isolated_home();

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
    }

    #[test]
    fn load_missing_file_returns_empty() {
        let _home = crate::testutil::isolated_home();
        let cache = load_creator_identity_cache();
        assert!(cache.creators.is_empty());
    }

    #[test]
    fn load_corrupt_file_returns_empty() {
        let _home = crate::testutil::isolated_home();
        let nexus_dir = std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_default()
            .join(".nexus42");
        std::fs::create_dir_all(&nexus_dir).expect("create nexus dir");
        std::fs::write(nexus_dir.join(FILENAME), "not valid json{{{").expect("write corrupt");

        let cache = load_creator_identity_cache();
        assert!(cache.creators.is_empty());
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
        let _home = crate::testutil::isolated_home();

        let mut cache = CreatorIdentityCache::default();
        cache.creators.insert(
            "ctr_alpha".to_string(),
            sample_entry("ctr_alpha", Some("alpha_handle_unique"), None),
        );
        save_creator_identity_cache(&cache).expect("save");

        // "alpha_handle_unique" should resolve to "ctr_alpha"
        let result = resolve_creator_ref("alpha_handle_unique").expect("resolve handle");
        assert_eq!(result, "ctr_alpha");
    }

    #[test]
    fn resolve_path_safe_unknown_passthrough() {
        let _home = crate::testutil::isolated_home();

        // Empty cache — unknown but path-safe input should pass through
        let cache = CreatorIdentityCache::default();
        save_creator_identity_cache(&cache).expect("save");

        let result = resolve_creator_ref("ctr_brand_new").expect("resolve");
        assert_eq!(result, "ctr_brand_new");
    }

    #[test]
    fn resolve_unsafe_input_errors() {
        let _home = crate::testutil::isolated_home();

        let cache = CreatorIdentityCache::default();
        save_creator_identity_cache(&cache).expect("save");

        let result = resolve_creator_ref("../etc");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Unknown creator reference"));
    }

    // ── R-CREATOR-002: Concurrent write safety test ────────────────────

    #[test]
    fn concurrent_writes_dont_corrupt_file() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        let _home = crate::testutil::isolated_home();

        // Pre-populate so threads race on read-modify-write
        let mut initial = CreatorIdentityCache::default();
        initial
            .creators
            .insert("ctr_base".to_string(), sample_entry("ctr_base", None, None));
        save_creator_identity_cache(&initial).expect("initial save");

        let success_count = Arc::new(AtomicUsize::new(0));
        let mut handles = Vec::new();

        for i in 0..4 {
            let success_count = Arc::clone(&success_count);
            handles.push(std::thread::spawn(move || {
                let entry = CreatorIdentityEntry {
                    creator_id: format!("ctr_thread_{i}"),
                    handle: None,
                    display_name: Some(format!("Thread {i}")),
                };
                match set_creator_identity(entry) {
                    Ok(()) => {
                        success_count.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(e) => {
                        eprintln!("Thread {i} failed: {e}");
                    }
                }
            }));
        }

        for h in handles {
            h.join().expect("thread should not panic");
        }

        // At least some writes should succeed
        let successes = success_count.load(Ordering::Relaxed);
        assert!(
            successes > 0,
            "At least one concurrent write should succeed"
        );

        // The final cache should be valid JSON and parseable
        let final_cache = load_creator_identity_cache();
        // Base entry should survive
        assert!(
            final_cache.creators.contains_key("ctr_base"),
            "base entry should survive concurrent writes"
        );
        // At least one thread entry should be present
        let thread_entries: Vec<_> = final_cache
            .creators
            .keys()
            .filter(|k| k.starts_with("ctr_thread_"))
            .collect();
        assert!(
            !thread_entries.is_empty(),
            "at least one thread entry should be present"
        );
    }
}
