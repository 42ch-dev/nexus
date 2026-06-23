//! Daemon-wide compilation cache for compute modules (R-V161P3-PERF-002).
//!
//! Compiling a `.wasm` into a wasmtime [`Module`] is the expensive step; once
//! compiled, a [`WasmModule`] is cheap to clone (internally `Arc`-shared) and
//! may be reused across many `compute()` calls — each call still gets a fresh
//! isolated instance. This module provides [`ModuleCache`], a thread-safe
//! `id → CachedModule` map built once at daemon startup and read on every
//! `narrative.compute` invocation.
//!
//! # Warmup
//!
//! [`ModuleCache::warm_embedded`] compiles every module shipped under
//! `embedded-modules/` (compass Q2 Embedded layer). [`ModuleCache::warm_dir`]
//! scans a user modules directory (`~/.nexus42/modules/`, compass Q2 User
//! layer) for `<id>/<id>.wasm` + `<id>/manifest.json` pairs. Both are best
//! effort: a single malformed module is aggregated into the returned error
//! rather than aborting the whole warmup, so the daemon still boots with the
//! valid modules available.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, RwLock};

use crate::embedded::{embedded_module_bytes, embedded_module_ids, embedded_module_manifest};
use crate::engine::{WasmEngine, WasmModule};
use crate::error::{ComputeError, Result};
use crate::manifest::ModuleManifest;

/// A compiled module paired with its parsed manifest — the cached entry shape.
///
/// Cloning is cheap: [`WasmModule`] is `Arc`-shared and [`ModuleManifest`] is a
/// small plain struct.
#[derive(Clone, Debug)]
pub struct CachedModule {
    /// The compiled wasmtime module, reusable across `compute()` calls.
    pub module: WasmModule,
    /// The module's parsed `manifest.json`.
    pub manifest: ModuleManifest,
}

/// Thread-safe `id → CachedModule` compilation cache.
///
/// Built once at daemon startup (P-last T1/T2/T3) and shared by every
/// `narrative.compute` invocation. Resolution at runtime is a single
/// [`ModuleCache::get`] lookup; modules are compiled once during warmup, not on
/// the hot path (closes R-V161P3-PERF-002).
pub struct ModuleCache {
    entries: RwLock<HashMap<String, Arc<CachedModule>>>,
}

impl Default for ModuleCache {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ModuleCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ids: Vec<String> = self
            .entries
            .read()
            .map(|g| g.keys().cloned().collect())
            .unwrap_or_default();
        f.debug_struct("ModuleCache")
            .field("len", &ids.len())
            .field("ids", &ids)
            .finish()
    }
}

impl ModuleCache {
    /// Create an empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
        }
    }

    /// Number of cached modules.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.read().map_or(0, |g| g.len())
    }

    /// Whether the cache holds no modules.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Look up a cached module by id. Returns a cheap clone of the entry.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<Arc<CachedModule>> {
        let guard = self.entries.read().ok()?;
        guard.get(id).map(Arc::clone)
    }

    /// Whether `id` is present in the cache.
    #[must_use]
    pub fn contains(&self, id: &str) -> bool {
        self.get(id).is_some()
    }

    /// Insert a pre-compiled entry. Overwrites any prior entry for the same id.
    pub fn insert(&self, id: impl Into<String>, entry: Arc<CachedModule>) {
        if let Ok(mut guard) = self.entries.write() {
            guard.insert(id.into(), entry);
        }
    }

    /// Compile `bytes` + `manifest_json` against `engine` and insert under `id`.
    ///
    /// Convenience for loaders that have already located the artifacts. Returns
    /// the freshly-cached entry (a cheap clone).
    ///
    /// # Errors
    ///
    /// Returns [`ComputeError::InvalidModule`] if `bytes` is not valid wasm or
    /// `manifest_json` cannot be deserialized into a [`ModuleManifest`].
    pub fn compile_and_insert(
        &self,
        engine: &WasmEngine,
        id: &str,
        bytes: &[u8],
        manifest_json: &str,
    ) -> Result<Arc<CachedModule>> {
        let module = engine.load_module(bytes)?;
        let manifest: ModuleManifest = serde_json::from_str(manifest_json)
            .map_err(|e| ComputeError::InvalidModule(format!("manifest parse for '{id}': {e}")))?;
        let entry = Arc::new(CachedModule { module, manifest });
        self.insert(id, Arc::clone(&entry));
        Ok(entry)
    }

    /// Pre-warm the cache with every embedded module (compass Q2 Embedded).
    ///
    /// Returns the number of modules successfully warmed. Per-module failures
    /// are aggregated into the returned error so a single malformed embedded
    /// module reports every problem at once without aborting the rest.
    ///
    /// # Errors
    ///
    /// Returns [`ComputeError::CacheWarmup`] if any embedded module could not
    /// be compiled or parsed (the successfully-warmed count is still reflected
    /// in the error message; valid modules remain in the cache).
    pub fn warm_embedded(&self, engine: &WasmEngine) -> Result<usize> {
        let mut warmed = 0usize;
        let mut errors: Vec<String> = Vec::new();
        for id in embedded_module_ids() {
            let Some(bytes) = embedded_module_bytes(id) else {
                errors.push(format!("embedded module '{id}': missing .wasm"));
                continue;
            };
            let Some(manifest) = embedded_module_manifest(id) else {
                errors.push(format!("embedded module '{id}': missing manifest.json"));
                continue;
            };
            match self.compile_and_insert(engine, id, bytes, manifest) {
                Ok(_) => warmed += 1,
                Err(e) => errors.push(format!("embedded module '{id}': {e}")),
            }
        }
        if errors.is_empty() {
            Ok(warmed)
        } else {
            Err(ComputeError::CacheWarmup(format!(
                "errors warming embedded modules (warmed {warmed}): {}",
                errors.join("; ")
            )))
        }
    }

    /// Scan a user modules directory and warm every `<id>/<id>.wasm` +
    /// `<id>/manifest.json` pair (compass Q2 User layer).
    ///
    /// `dir` is typically `~/.nexus42/modules/` (see `nexus_home_layout`).
    /// A missing directory is treated as empty (not an error) — user modules
    /// are optional. Returns the number of modules warmed; per-module failures
    /// are aggregated so one bad module does not block the others.
    ///
    /// # Errors
    ///
    /// Returns [`ComputeError::CacheWarmup`] if `dir` cannot be read (other
    /// than because it is missing) or if any individual module fails to compile
    /// or parse. A missing `dir` returns `Ok(0)`.
    pub fn warm_dir(&self, engine: &WasmEngine, dir: &Path) -> Result<usize> {
        let read = match std::fs::read_dir(dir) {
            Ok(r) => r,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(e) => {
                return Err(ComputeError::CacheWarmup(format!(
                    "cannot read user modules dir {}: {e}",
                    dir.display()
                )));
            }
        };
        let mut warmed = 0usize;
        let mut errors: Vec<String> = Vec::new();
        for entry in read.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let Some(id) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            let wasm_path = path.join(format!("{id}.wasm"));
            let manifest_path = path.join("manifest.json");
            if !wasm_path.exists() || !manifest_path.exists() {
                continue;
            }
            let bytes = match std::fs::read(&wasm_path) {
                Ok(b) => b,
                Err(e) => {
                    errors.push(format!(
                        "user module '{id}': read {}: {e}",
                        wasm_path.display()
                    ));
                    continue;
                }
            };
            let manifest_json = match std::fs::read_to_string(&manifest_path) {
                Ok(s) => s,
                Err(e) => {
                    errors.push(format!(
                        "user module '{id}': read {}: {e}",
                        manifest_path.display()
                    ));
                    continue;
                }
            };
            match self.compile_and_insert(engine, id, &bytes, &manifest_json) {
                Ok(_) => warmed += 1,
                Err(e) => errors.push(format!("user module '{id}': {e}")),
            }
        }
        if errors.is_empty() {
            Ok(warmed)
        } else {
            Err(ComputeError::CacheWarmup(format!(
                "errors warming user modules in {} (warmed {warmed}): {}",
                dir.display(),
                errors.join("; ")
            )))
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn cache_starts_empty() {
        let cache = ModuleCache::new();
        assert!(cache.is_empty());
        assert_eq!(cache.len(), 0);
        assert!(!cache.contains("basic-combat"));
    }

    #[test]
    fn warm_embedded_loads_basic_combat() {
        let engine = WasmEngine::new().unwrap();
        let cache = ModuleCache::new();
        let warmed = cache.warm_embedded(&engine).expect("embedded warmup");
        assert!(warmed >= 1, "at least basic-combat must warm");
        assert!(cache.contains("basic-combat"));
        let entry = cache.get("basic-combat").unwrap();
        assert_eq!(entry.manifest.module_id, "basic-combat");
    }

    #[test]
    fn warm_dir_missing_dir_is_ok_zero() {
        let engine = WasmEngine::new().unwrap();
        let cache = ModuleCache::new();
        let warmed = cache
            .warm_dir(
                &engine,
                std::path::Path::new("/nonexistent/nexus-modules-12345"),
            )
            .expect("missing dir is not an error");
        assert_eq!(warmed, 0);
    }

    #[test]
    fn warm_dir_loads_user_module_pair() {
        let engine = WasmEngine::new().unwrap();
        let cache = ModuleCache::new();
        let dir = tempfile::tempdir().unwrap();
        let module_dir = dir.path().join("basic-combat");
        std::fs::create_dir_all(&module_dir).unwrap();
        // Reuse the embedded bytes/manifest so the user module is valid wasm.
        let bytes = embedded_module_bytes("basic-combat").unwrap();
        let manifest = embedded_module_manifest("basic-combat").unwrap();
        std::fs::write(module_dir.join("basic-combat.wasm"), bytes).unwrap();
        std::fs::write(module_dir.join("manifest.json"), manifest).unwrap();

        let warmed = cache.warm_dir(&engine, dir.path()).expect("user warmup");
        assert_eq!(warmed, 1);
        assert!(cache.contains("basic-combat"));
    }

    #[test]
    fn warm_dir_skips_incomplete_pairs() {
        let engine = WasmEngine::new().unwrap();
        let cache = ModuleCache::new();
        let dir = tempfile::tempdir().unwrap();
        // A subdir with only a .wasm (no manifest) must be skipped, not error.
        let partial = dir.path().join("partial");
        std::fs::create_dir_all(&partial).unwrap();
        std::fs::write(partial.join("partial.wasm"), b"not wasm anyway").unwrap();

        let warmed = cache
            .warm_dir(&engine, dir.path())
            .expect("incomplete pair skipped");
        assert_eq!(warmed, 0);
        assert!(cache.is_empty());
    }

    #[test]
    fn get_returns_none_for_unknown() {
        let cache = ModuleCache::new();
        assert!(cache.get("nope").is_none());
    }
}
