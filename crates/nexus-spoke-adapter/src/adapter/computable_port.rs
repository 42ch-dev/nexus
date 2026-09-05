//! Production `ComputablePort` impl — bridges spoke compute requests to nexus's
//! stateless WASM host via a local session store (plan Decision 2).
//!
//! # Mapping (plan Decision 2)
//!
//! | Concept | spoke side | Nexus side | Mapping |
//! |---------|-----------|------------|---------|
//! | Session identity | `session_id` (opaque, product-owned) | `compute_sessions` row | Nexus owns session lifecycle; spoke `session_id` is a pass-through correlation id |
//! | Session state | `ProjectRequest.state` (`ComputableFieldMap`) | `state_json` column | `project()` stages static state; `compute()` merges dynamic updates |
//! | Computable I/O | `ProjectRequest` / `ComputeRequest` | `WasmEngine::compute(module, manifest, input)` | `project()`: validate entry, stage state; `compute()`: load session, build `ComputeInput`, call WASM |
//! | Settle | `ComputeRequest.settle: bool` | Merge `ComputeOutput.state_delta` into entry's `body.state` | On `settle=true`, update entry via `KnowledgeEntryPort::put_knowledge_entry` |
//!
//! # Module identity resolution
//!
//! The spoke wire does not carry a `module_id` field. When `compute()` is called:
//!
//! 1. Check the merged session state for a `module_id` key.
//! 2. Fall back to the entry's `body.computable` (if present) for a `module_id` field.
//! 3. Reject with `InvalidInput` if neither source provides a module identity.
//!
//! This is documented as the honest bridge decision per the plan's Architecture
//! Lock: the module identity is genuinely absent from the spoke wire, so the
//! adapter resolves it from the staged session context. Callers must
//! explicitly declare their module choice via `project()` state or entry
//! `body.computable`.
//!
//! # Async surface (V1.153 P0 T2)
//!
//! The port methods are natively `async fn` (spoke-operations 0.9.1 surface)
//! and await `SQLite` I/O directly; the former `block_on` bridge is gone.
//! The `WasmEngine::new()` call is one-time (cold; subsequent calls reuse the
//! engine via a `OnceCell`), and the WASM compute call is synchronous
//! (the wasmtime host does not require an async runtime).

use super::NexusAdapter;
use crate::{
    ComputablePort, KnowledgeEntryPort, ProjectRequest, ProjectResponse, SpokeReject,
    SpokeRejectCode, SpokeResult,
};
use crate::{ComputeRequest, ComputeResponse};
use async_trait::async_trait;
use nexus_local_db::compute_session::{get_compute_session, insert_compute_session};
use nexus_wasm_host::{
    embedded_module_bytes, embedded_module_ids, embedded_module_manifest, ComputeInput,
    ModuleCache, ModuleManifest, WasmEngine, WasmModule,
};
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;

/// One-time initialised WASM engine, reused across all compute invocations.
/// The constructor is sync and cheap enough to call once per process lifetime.
static WASM_ENGINE: OnceLock<WasmEngine> = OnceLock::new();

fn engine() -> &'static WasmEngine {
    WASM_ENGINE.get_or_init(|| {
        WasmEngine::new().expect("WasmEngine failed to initialize (wasmtime engine unavailable)")
    })
}

/// Resolve the module identity for a compute invocation.
///
/// Priority:
/// 1. `module_id` in the merged session state
/// 2. `module_id` in the entry's `body.computable`
/// 3. Reject — module identity is required; callers must explicitly declare
///    their module choice via `project()` state or entry `body.computable`.
fn resolve_module_id(
    state: &Map<String, Value>,
    entry: &crate::KnowledgeEntry,
) -> SpokeResult<String> {
    if let Some(module_id) = state.get("module_id").and_then(Value::as_str) {
        return SpokeResult::Ok(module_id.to_string());
    }
    // Check the entry body's computable field for module_id.
    // This reads the typed body.computable map from the spoke KnowledgeEntry
    // rather than relying on a derived key_block map (which would be fragile
    // if `spoke_entry_to_key_block` ever strips or transforms
    // `body.computable`).
    //
    // NOTE (P2 QC fix wave FW-1 — dead-code tier): under the current
    // conversion seam this tier can never fire — `knowledge_record_to_spoke` emits
    // only the marker map `{"_computable": true}` from the nexus
    // `Option<bool>`, and `spoke_to_knowledge_record` collapses spoke maps back to
    // `Some(true)`, so `entry.body.computable.get("module_id")` is always
    // `None` today. The tier is retained as the documented resolution
    // precedence (spec §2.2) and as defense if body.computable maps ever
    // survive the seam.
    if let Some(module_id) = entry
        .body
        .computable
        .get("module_id")
        .and_then(Value::as_str)
    {
        return SpokeResult::Ok(module_id.to_string());
    }
    // The `module_identity_missing` details marker (P2 QC fix wave FW-5) is
    // the structured control-flow signal for this reject: hosts classify
    // the missing-module-name denial via
    // [`is_module_identity_missing_reject`] instead of string-sniffing the
    // message.
    reject(
        SpokeRejectCode::InvalidInput,
        "module identity required on session state or entry body.computable",
        json!({ "module_identity_missing": true }),
    )
}

/// Shared module-id path-safety check (P2 QC fix wave FW-4 — single source
/// of truth for the gate AND the execution guard).
///
/// The id must be a single path component — non-empty, no `/` or `\`
/// separators, and not `.` / `..` — so joining it under the module store
/// directory can never escape the store. The Connect gate's host-store
/// check (`apps/nexus42` `module_installed`) and the adapter's user-module
/// loader ([`load_user_module`]) both route through this so they can never
/// drift.
#[must_use]
pub fn is_safe_module_id(module_id: &str) -> bool {
    !module_id.is_empty()
        && !module_id.contains('/')
        && !module_id.contains('\\')
        && module_id != "."
        && module_id != ".."
}

/// Structured marker predicate for the missing-module-identity reject
/// (P2 QC fix wave FW-5 — same pattern as
/// [`crate::is_world_conflict_reject`]).
///
/// `resolve_module_id`'s `InvalidInput` reject carries a
/// `module_identity_missing: true` details marker so hosts classify the
/// denial by marker, never by sniffing the reject message.
#[must_use]
pub fn is_module_identity_missing_reject(reject: &SpokeReject) -> bool {
    reject
        .details
        .as_ref()
        .and_then(|d| d.get("module_identity_missing"))
        .and_then(serde_json::Value::as_bool)
        == Some(true)
}

impl NexusAdapter<'_> {
    /// Load (or get cached) a compiled WASM module by id — host-local store.
    ///
    /// When a user modules dir is configured
    /// ([`NexusAdapter::with_user_modules_dir`] — the Connect host's
    /// `~/.nexus42/modules/`), the module MUST be installed there as
    /// `<dir>/<id>/<id>.wasm` + `<dir>/<id>/manifest.json` (fail-closed: an
    /// absent/incomplete pair is `InvalidInput`; the embedded ship set is
    /// NOT reachable — the Connect surface serves only operator-installed
    /// modules, spec §2.1). Without a configured dir (baseline consumers),
    /// the embedded ship set is used (V1.146 behavior unchanged).
    ///
    /// # Compiled-module cache (P2 QC fix wave FW-2; manifest half:
    /// Greptile P1)
    ///
    /// Both load paths route through the per-adapter
    /// [`ModuleCache`](nexus_wasm_host::ModuleCache): the wasmtime compile
    /// runs once per distinct `(module id, wasm bytes hash, manifest
    /// hash)` instead of once per invocation (the module artifacts are
    /// still re-read per invoke so an operator content change is
    /// observable — a wasm OR manifest-only change then misses and the
    /// entry is recompiled + overwritten, so updated schemas / sandbox
    /// settings take effect without a wasm change). The Connect host keeps
    /// ONE adapter for the process lifetime, so the cache is process-wide
    /// there.
    ///
    /// # Error classification (V1.146 P2 QC fix-wave + P2 user-store
    /// extension)
    ///
    /// - Unknown / invalid `module_id` (not in the store) → `InvalidInput` —
    ///   the id comes from session state / `body.computable`
    ///   (caller-controlled), so "not available" is a client-input error,
    ///   not a host failure.
    /// - Known module whose install/compile/trap fails → `InternalError`
    ///   (host problem after the id itself was resolved correctly).
    fn load_module(&self, module_id: &str) -> SpokeResult<(WasmModule, ModuleManifest)> {
        if let Some(dir) = &self.user_modules_dir {
            return load_user_module(&self.module_cache, dir, module_id);
        }
        load_embedded_module(&self.module_cache, module_id)
    }
}

/// Load a user-installed module from the host-local store (spec §2.1 —
/// bytes are never peer-supplied).
///
/// The module id is operator/peer-named: it must be a single path
/// component (no separators, no `.` / `..` — [`is_safe_module_id`]), so the
/// join below can never escape the store directory (mirrors the embedded
/// allowlist gate's fail-closed spirit — an escaped path would otherwise
/// read arbitrary files).
///
/// The compiled module is served through `cache` (id + wasm-bytes-hash +
/// manifest-hash keying, P2 QC fix wave FW-2 + Greptile P1): repeated
/// invokes of unchanged artifacts reuse the cached compile; a changed
/// module file — wasm OR manifest-only (new schemas / sandbox overrides) —
/// recompiles and overwrites the entry.
fn load_user_module(
    cache: &ModuleCache,
    dir: &Path,
    module_id: &str,
) -> SpokeResult<(WasmModule, ModuleManifest)> {
    if !is_safe_module_id(module_id) {
        return reject(
            SpokeRejectCode::InvalidInput,
            format!("invalid module id: {module_id:?}"),
            json!({ "module_id": module_id }),
        );
    }
    let module_dir = dir.join(module_id);
    let wasm_path = module_dir.join(format!("{module_id}.wasm"));
    let manifest_path = module_dir.join("manifest.json");
    if !wasm_path.is_file() || !manifest_path.is_file() {
        return reject(
            SpokeRejectCode::InvalidInput,
            format!(
                "module '{module_id}' is not installed under {}",
                dir.display()
            ),
            json!({ "module_id": module_id }),
        );
    }
    // Post-existence-check read failures are host faults (the store
    // changed mid-invoke or is unreadable) — InternalError, like the
    // embedded path's "known module whose embed/compile fails"
    // classification.
    //
    // Consistent-snapshot read (Greptile P1 — non-atomic module reload):
    // an operator replacing a module mid-invoke writes `<id>.wasm` and
    // `manifest.json` as two INDEPENDENT files, so reading them separately
    // can observe a mixed pair (wasm v1 + manifest v2) that would then be
    // compiled and cached. The reads are ordered as a three-step fence —
    // manifest (m1, the small version marker) → wasm stat (s1) → wasm
    // bytes (b) → wasm stat (s2) → manifest re-read (m2) — and the pair is
    // rejected as a host fault (never compiled) when m1 != m2 (manifest
    // mutated mid-load) or s1 != s2 (wasm mutated between the stats).
    //
    // Content-based pairing (the root cause fix): when the manifest
    // declares `wasm_sha256`, the loaded bytes are hashed and must match —
    // an old manifest + new wasm ALWAYS mismatches, closing the residual
    // below for operators who set the field (they SHOULD; see the manifest
    // docs). The hash check runs BEFORE `get_or_compile`, so a mixed pair
    // never enters the cache.
    //
    // The stat fence below is the LEGACY fallback for manifests without
    // `wasm_sha256`: size + mtime, not a byte comparison. Note `modified()`
    // mtime can have coarse granularity on some filesystems — a same-size
    // rewrite landing between s1 and s2 within one clock tick may then slip
    // past the fence.
    //
    // Residual (legacy manifests only — undetectable without a content
    // hash): a pair whose writes land OUTSIDE their observation windows —
    // e.g. the wasm write between m1 and s1 plus a manifest write after m2,
    // or a fully atomic pair swap straddling the reads — leaves each file
    // stable at its own observation points, so a mixed pair is
    // indistinguishable from a coherent one. For true atomicity the install
    // tool should write to a temp directory and rename the module directory
    // into place; the loader then observes either the old pair or the new
    // pair, never a mix. A manifest WITHOUT `wasm_sha256` cannot be paired
    // by content, so this residual is the operator's choice to accept.
    let manifest_json = match std::fs::read_to_string(&manifest_path) {
        Ok(s) => s,
        Err(e) => {
            return reject(
                SpokeRejectCode::InternalError,
                format!("failed to read {}: {e}", manifest_path.display()),
                json!({ "module_id": module_id }),
            );
        }
    };
    // s1: open the wasm stat fence BEFORE reading the bytes, so a wasm
    // replacement landing between the manifest read and the wasm read —
    // previously invisible to a bytes-only re-read — is caught by the
    // s1 != s2 comparison.
    let wasm_stat_first = match std::fs::metadata(&wasm_path) {
        Ok(m) => m,
        Err(e) => {
            return reject(
                SpokeRejectCode::InternalError,
                format!("failed to stat {}: {e}", wasm_path.display()),
                json!({ "module_id": module_id }),
            );
        }
    };
    let bytes = match std::fs::read(&wasm_path) {
        Ok(b) => b,
        Err(e) => {
            return reject(
                SpokeRejectCode::InternalError,
                format!("failed to read {}: {e}", wasm_path.display()),
                json!({ "module_id": module_id }),
            );
        }
    };
    match module_pair_changed_mid_load(&wasm_path, &manifest_path, &wasm_stat_first, &manifest_json)
    {
        Ok(true) => {
            return reject(
                SpokeRejectCode::InternalError,
                format!("module '{module_id}' changed while it was being read; retry the invoke"),
                json!({ "module_id": module_id }),
            );
        }
        Ok(false) => {}
        Err(e) => {
            return reject(
                SpokeRejectCode::InternalError,
                format!("failed to re-read module store for '{module_id}': {e}"),
                json!({ "module_id": module_id }),
            );
        }
    }
    // Content-based pairing (Greptile P1 root cause): when the manifest
    // declares `wasm_sha256`, the loaded bytes must hash to it. This is the
    // ONLY detector that sees a mixed pair whose writes landed outside the
    // stat fence's observation windows — an old manifest + new wasm always
    // mismatches. The check runs BEFORE `get_or_compile`, so a mixed pair
    // never enters the cache; the stat fence above remains the legacy
    // fallback for manifests without the field.
    let manifest: ModuleManifest = match serde_json::from_str(&manifest_json) {
        Ok(m) => m,
        Err(e) => {
            return reject(
                SpokeRejectCode::InternalError,
                format!("failed to parse manifest for module '{module_id}': {e}"),
                json!({ "module_id": module_id }),
            );
        }
    };
    if let Err(msg) = manifest.verify_wasm_sha256(&bytes) {
        return reject(
            SpokeRejectCode::InternalError,
            format!("module '{module_id}': {msg}"),
            json!({ "module_id": module_id }),
        );
    }
    let cached = match cache.get_or_compile(engine(), module_id, &bytes, &manifest_json) {
        Ok(entry) => entry,
        Err(e) => {
            return reject(
                SpokeRejectCode::InternalError,
                format!("failed to compile WASM module {module_id}: {e}"),
                json!({ "module_id": module_id }),
            );
        }
    };
    SpokeResult::Ok((cached.module.clone(), cached.manifest.clone()))
}

/// Complete the coherence fence behind [`load_user_module`]'s
/// consistent-snapshot read (Greptile P1 — non-atomic module reload): the
/// caller has already read the manifest (m1), statted the wasm (s1) and
/// read the wasm bytes; this closes the fence — re-stat the wasm (s2) and
/// re-read the manifest (m2) — and reports whether EITHER changed
/// mid-load. `Ok(true)` means the store mutated mid-load — the caller
/// must NOT compile the mixed pair into the cache; `Ok(false)` means the
/// pair is a consistent snapshot. A read failure here is a host fault
/// like any other post-check read failure.
///
/// This is the LEGACY fallback tier: manifests that declare `wasm_sha256`
/// are additionally verified by content hash (see
/// [`ModuleManifest::verify_wasm_sha256`]), which catches mixed pairs the
/// fence cannot see.
fn module_pair_changed_mid_load(
    wasm_path: &Path,
    manifest_path: &Path,
    wasm_stat_first: &std::fs::Metadata,
    manifest_first: &str,
) -> Result<bool, std::io::Error> {
    let wasm_stat_now = std::fs::metadata(wasm_path)?;
    let manifest_now = std::fs::read_to_string(manifest_path)?;
    // Stat fence (wasm): a size or mtime change between the two stats
    // means the wasm was replaced while it was being read. `modified()`
    // may quantize on coarse-granularity filesystems — see
    // [`load_user_module`] for the documented residual. An unreadable
    // mtime counts as a change (fail-closed).
    let mtime_changed = match (wasm_stat_first.modified(), wasm_stat_now.modified()) {
        (Ok(first), Ok(now)) => first != now,
        _ => true,
    };
    let wasm_changed = wasm_stat_first.len() != wasm_stat_now.len() || mtime_changed;
    Ok(wasm_changed || manifest_now != manifest_first)
}

impl NexusAdapter<'_> {
    /// Resolve the module identity for a compute invocation using the locked
    /// precedence (spec §2.2): session state `module_id` first, then the
    /// entry's `body.computable.module_id`. The Connect compute gate uses
    /// this to scope the peer BEFORE any WASM execution; `ComputablePort::compute`
    /// applies the same precedence internally (single source of truth).
    ///
    /// # Errors
    /// `InvalidInput` when the session row is missing, the entry is missing,
    /// or neither tier carries a module id.
    pub async fn resolve_compute_module_id(
        &self,
        session_id: &str,
        entry_id: &str,
    ) -> SpokeResult<String> {
        let session = match get_compute_session(&self.pool, session_id).await {
            Ok(Some(s)) => s,
            Ok(None) => {
                return reject(
                    SpokeRejectCode::InvalidInput,
                    format!("compute session not found: {session_id}"),
                    json!({ "session_id": session_id }),
                );
            }
            Err(e) => {
                return reject(
                    SpokeRejectCode::InternalError,
                    format!("storage error on compute session read: {e}"),
                    json!({ "session_id": session_id }),
                );
            }
        };
        let entry = match self.get_knowledge_entry(entry_id).await {
            SpokeResult::Ok(e) => e,
            SpokeResult::Reject(r) => return SpokeResult::Reject(r),
        };
        let state: Map<String, Value> = session
            .state_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();
        resolve_module_id(&state, &entry)
    }
}

/// Load a compiled module from the embedded ship set (baseline consumers
/// without a configured user store — V1.146 behavior). The compiled module
/// is served through `cache` (id + wasm-bytes-hash + manifest-hash keying,
/// P2 QC fix wave FW-2 + Greptile P1): embedded bytes are immutable, so
/// after the first invocation the compile is a pure cache hit.
fn load_embedded_module(
    cache: &ModuleCache,
    module_id: &str,
) -> SpokeResult<(WasmModule, ModuleManifest)> {
    // F-002+F-005: validate module_id is a known embedded module before
    // doing any path-formatting or I/O. The embedded_module_ids() list is
    // the authoritative allowlist; an id not present there is InvalidInput.
    let known = embedded_module_ids();
    if !known.contains(&module_id) {
        return reject(
            SpokeRejectCode::InvalidInput,
            format!("unknown embedded WASM module: {module_id}"),
            json!({ "module_id": module_id }),
        );
    }
    // After the allowlist gate, module_id is known — the following lookups
    // should succeed. If they fail despite allowlist membership (e.g. stale
    // include_dir! after build.rs change), that is a host error.
    let Some(wasm_bytes) = embedded_module_bytes(module_id) else {
        return reject(
            SpokeRejectCode::InternalError,
            format!("embedded WASM module bytes missing for known id: {module_id}"),
            json!({ "module_id": module_id }),
        );
    };
    let Some(manifest_json) = embedded_module_manifest(module_id) else {
        return reject(
            SpokeRejectCode::InternalError,
            format!("embedded module manifest missing for known id: {module_id}"),
            json!({ "module_id": module_id }),
        );
    };
    let cached = match cache.get_or_compile(engine(), module_id, wasm_bytes, manifest_json) {
        Ok(entry) => entry,
        Err(e) => {
            return reject(
                SpokeRejectCode::InternalError,
                format!("failed to compile WASM module {module_id}: {e}"),
                json!({ "module_id": module_id }),
            );
        }
    };
    SpokeResult::Ok((cached.module.clone(), cached.manifest.clone()))
}

impl NexusAdapter<'_> {}

#[async_trait]
impl ComputablePort for NexusAdapter<'_> {
    async fn project(&self, request: ProjectRequest) -> SpokeResult<ProjectResponse> {
        let pool = self.pool.clone();
        let session_id = request.session_id.clone();
        let entry_id = request.entry_id.clone();
        let state = request.state;

        // Validate entry exists (rejects with InvalidInput if entry is missing).
        let _entry = match self.get_knowledge_entry(&entry_id).await {
            SpokeResult::Ok(e) => e,
            SpokeResult::Reject(r) => {
                if r.code == SpokeRejectCode::KnowledgeEntryNotFound {
                    return reject(
                        SpokeRejectCode::InvalidInput,
                        format!("target KnowledgeEntry not found for compute project: {entry_id}"),
                        json!({ "entry_id": entry_id }),
                    );
                }
                return SpokeResult::Reject(r);
            }
        };

        let state_json = serde_json::to_string(&state).unwrap_or_else(|_| "{}".to_string());

        match insert_compute_session(&pool, &session_id, &entry_id, &state_json).await {
            Ok(_session) => SpokeResult::Ok(ProjectResponse::Variant0 {
                computable: state,
                entry_id,
                session_id,
                extensions: HashMap::default(),
            }),
            Err(e) => {
                // sqlx::Error::Database with UNIQUE constraint → session
                // already exists. Map to a friendly InvalidInput.
                let msg = e.to_string();
                if msg.contains("UNIQUE") {
                    reject(
                        SpokeRejectCode::InvalidInput,
                        format!("session already exists: {session_id}"),
                        json!({ "session_id": session_id }),
                    )
                } else {
                    reject(
                        SpokeRejectCode::InternalError,
                        format!("storage error on compute session insert: {e}"),
                        json!({ "session_id": session_id }),
                    )
                }
            }
        }
    }

    #[allow(clippy::too_many_lines)]
    async fn compute(&self, request: ComputeRequest) -> SpokeResult<ComputeResponse> {
        let pool = self.pool.clone();
        let session_id = request.session_id.clone();
        let entry_id = request.entry_id.clone();
        let dynamic_computable = request.computable;
        let settle = request.settle.unwrap_or(false);

        // ── 1. Load session row ──────────────────────────────────────────
        let session = get_compute_session(&pool, &session_id).await;
        let session = match session {
            Ok(Some(s)) => s,
            Ok(None) => {
                return reject(
                    SpokeRejectCode::InvalidInput,
                    format!("compute session not found: {session_id}"),
                    json!({ "session_id": session_id }),
                );
            }
            Err(e) => {
                return reject(
                    SpokeRejectCode::InternalError,
                    format!("storage error on compute session read: {e}"),
                    json!({ "session_id": session_id }),
                );
            }
        };

        // Verify entry_id consistency.
        if session.entry_id != entry_id {
            return reject(
                SpokeRejectCode::InvalidInput,
                format!(
                    "entry_id mismatch: session stores '{}' but compute request targets '{}'",
                    session.entry_id, entry_id
                ),
                json!({
                    "session_id": session_id,
                    "session_entry_id": session.entry_id,
                    "request_entry_id": entry_id,
                }),
            );
        }

        // ── 2. Load the entry for compute envelope ────────────────────────
        // A missing target entry is a client-input error (the request names
        // `entry_id` on the wire) — mapped to InvalidInput like `project()`
        // (P2 QC fix wave FW-3); the Connect gate already denies missing
        // entries before execution, so this arm covers the check-then-act
        // race where the entry is deleted between the gate and this call.
        let entry = match self.get_knowledge_entry(&entry_id).await {
            SpokeResult::Ok(e) => e,
            SpokeResult::Reject(r) => {
                if r.code == SpokeRejectCode::KnowledgeEntryNotFound {
                    return reject(
                        SpokeRejectCode::InvalidInput,
                        format!("target KnowledgeEntry not found for compute: {entry_id}"),
                        json!({ "entry_id": entry_id }),
                    );
                }
                return SpokeResult::Reject(r);
            }
        };

        // ── 3. Merge staged state + dynamic computable (in-memory only) ──
        // Do NOT persist session state until WASM compute succeeds — otherwise
        // a failed compute leaves the session advanced while the entry is
        // unchanged (Greptile P1: failed computes advance session state).
        let mut merged_state: Map<String, Value> = session
            .state_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();
        for (k, v) in &dynamic_computable {
            merged_state.insert(k.clone(), v.clone());
        }

        // ── 4. Build ComputeInput envelope ────────────────────────────────
        // Collect key_blocks: primary entry + any additional entries
        // referenced in the invocation (e.g., attacker_id, defender_id for
        // basic-combat). Each entry is converted from spoke format to the
        // flat-attributes format the WASM module expects.

        // Extract world_id from extensions.nexus (the spoke-provided field)
        // early — needed for the cross-entry world-scope check (F-003).
        let primary_world_id = entry
            .extensions
            .get(&crate::KnowledgeEntryExtensionsKey::try_from("nexus").expect("valid nexus key"))
            .and_then(|ns| ns.get("world_id"))
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        let nexus_key = crate::KnowledgeEntryExtensionsKey::try_from("nexus").expect("valid");

        let mut key_blocks: Vec<Map<String, Value>> = Vec::new();
        let primary_kb = spoke_entry_to_key_block(&entry);
        key_blocks.push(primary_kb);

        // Load additional entries referenced by known invocation keys that
        // carry entry IDs (attacker_id, defender_id for basic-combat).
        // F-003: only load entries whose world_id matches the primary entry's
        // world; cross-world references are rejected as InvalidInput.
        for key in &["attacker_id", "defender_id"] {
            if let Some(ref_id) = merged_state.get(*key).and_then(Value::as_str) {
                if ref_id != entry_id
                    && !key_blocks
                        .iter()
                        .any(|kb| kb.get("entry_id").and_then(Value::as_str) == Some(ref_id))
                {
                    match self.get_knowledge_entry(ref_id).await {
                        SpokeResult::Ok(ref_entry) => {
                            let ref_world_id = ref_entry
                                .extensions
                                .get(&nexus_key)
                                .and_then(|ns| ns.get("world_id"))
                                .and_then(Value::as_str)
                                .unwrap_or("unknown");
                            if ref_world_id != primary_world_id {
                                return reject(
                                    SpokeRejectCode::InvalidInput,
                                    format!(
                                        "cross-world reference: entry {ref_id} belongs to world \
                                         {ref_world_id}, not primary world {primary_world_id}"
                                    ),
                                    json!({
                                        "entry_id": ref_id,
                                        "ref_world_id": ref_world_id,
                                        "primary_world_id": primary_world_id,
                                    }),
                                );
                            }
                            key_blocks.push(spoke_entry_to_key_block(&ref_entry));
                        }
                        SpokeResult::Reject(r) => return SpokeResult::Reject(r),
                    }
                }
            }
        }

        let world_id = primary_world_id.clone();

        // Build invocation from the merged computable state (carries
        // module-specific params like attacker_id, defender_id).
        let invocation = if merged_state.is_empty() {
            Map::new()
        } else {
            merged_state.clone()
        };

        let compute_input = ComputeInput {
            key_blocks,
            // narrative_state is not yet wired: the adapter does not feed
            // narrative context from the gateway into the compute envelope.
            // This is acceptable while all embedded modules (basic-combat)
            // do not use `narrative_query` — the first module that needs
            // narrative state should trigger wiring this field.
            narrative_state: None,
            invocation,
            schema_version: std::num::NonZeroU64::new(1).expect("1 is non-zero"),
            world_ref: {
                // ComputeInputWorldRef is a generated type. Build from JSON.
                let raw = json!({"world_id": world_id});
                serde_json::from_value(raw).unwrap_or_else(|_| {
                    serde_json::from_value(json!({"world_id": "unknown"}))
                        .expect("fallback world_ref is valid")
                })
            },
        };

        // ── 6. Resolve module and run compute ─────────────────────────────
        let module_id = match resolve_module_id(&merged_state, &entry) {
            SpokeResult::Ok(id) => id,
            SpokeResult::Reject(r) => return SpokeResult::Reject(r),
        };
        let (wasm_module, manifest) = match self.load_module(&module_id) {
            SpokeResult::Ok(m) => m,
            SpokeResult::Reject(r) => return SpokeResult::Reject(r),
        };

        let output = match engine().compute(&wasm_module, &manifest, &compute_input) {
            Ok(o) => o,
            Err(e) => {
                return reject(
                    SpokeRejectCode::InternalError,
                    format!("WASM compute failed for module {module_id}: {e}"),
                    json!({
                        "module_id": module_id,
                        "session_id": session_id,
                    }),
                );
            }
        };

        // ── 7. Apply state_delta to merged_state (in-memory view) ─────────
        for delta in &output.state_delta {
            let delta_value: Value = serde_json::to_value(delta).unwrap_or_default();
            if let Some(path) = delta_value.get("path").and_then(Value::as_str) {
                let op = delta_value
                    .get("op")
                    .and_then(Value::as_str)
                    .unwrap_or("set");
                if let Some(value) = delta_value.get("value") {
                    match op {
                        "+" => {
                            add_json_path(&mut merged_state, path, value);
                        }
                        "-" => {
                            sub_json_path(&mut merged_state, path, value);
                        }
                        _ => {
                            set_json_path(&mut merged_state, path, value.clone());
                        }
                    }
                }
            }
        }

        // ── 8. Settle: prepare in-memory updates, then commit atomically ──
        // F-001: state_delta carries `target_key_block_id` naming the
        // KnowledgeEntry each delta should apply to (schema L29-31). Default
        // to `request.entry_id` when omitted. The adapter must group deltas
        // by target and apply each group to its respective entry — NOT
        // blindly apply everything to the primary entry.
        //
        // Greptile P1 (partial settle): all target CAS writes + session
        // state update share one SQLite transaction via
        // `commit_compute_settlement`. A later CAS/session failure rolls
        // back earlier entry writes so retries never double-apply deltas.
        let mut post_settle_state = Map::new();
        let mut pending_entry_updates: Vec<(crate::KnowledgeEntry, u64)> = Vec::new();

        if settle {
            // Group deltas by target_key_block_id.
            let mut deltas_by_target: HashMap<String, Vec<_>> = HashMap::new();
            for delta in &output.state_delta {
                let delta_value: Value = serde_json::to_value(delta).unwrap_or_default();
                // target_key_block_id → request.entry_id is the default per
                // schema contract. Skip deltas that have no path/op (defensive).
                if delta_value.get("path").and_then(Value::as_str).is_none() {
                    continue;
                }
                let target = delta_value
                    .get("target_key_block_id")
                    .and_then(Value::as_str)
                    .unwrap_or(&entry_id);
                deltas_by_target
                    .entry(target.to_string())
                    .or_default()
                    .push(delta);
            }

            for (target_id, deltas) in &deltas_by_target {
                // Load the target entry.
                let target_entry = match self.get_knowledge_entry(target_id).await {
                    SpokeResult::Ok(e) => e,
                    SpokeResult::Reject(r) => return SpokeResult::Reject(r),
                };

                // F-003: world-scope check — a delta must only settle to an
                // entry in the same world as the primary compute entry.
                let target_world_id = target_entry
                    .extensions
                    .get(&nexus_key)
                    .and_then(|ns| ns.get("world_id"))
                    .and_then(Value::as_str)
                    .unwrap_or("unknown");
                if target_world_id != primary_world_id {
                    return reject(
                        SpokeRejectCode::InvalidInput,
                        format!(
                            "settle target {target_id} belongs to world {target_world_id}, \
                             not primary world {primary_world_id}"
                        ),
                        json!({
                            "target_id": target_id,
                            "target_world_id": target_world_id,
                            "primary_world_id": primary_world_id,
                        }),
                    );
                }

                let Some(expected_rev) = target_entry.revision else {
                    return reject(
                        SpokeRejectCode::InvalidInput,
                        format!("settle target {target_id} has no revision"),
                        json!({ "target_id": target_id }),
                    );
                };

                // Serialize the target entry to a JSON map for mutation.
                let mut target_body = match serde_json::to_value(&target_entry).unwrap_or_default()
                {
                    Value::Object(m) => m,
                    _ => Map::new(),
                };

                let existing_state: Map<String, Value> = target_body
                    .get("body")
                    .and_then(|b| b.get("state"))
                    .and_then(|s| s.as_object())
                    .cloned()
                    .unwrap_or_default();

                let mut final_state = existing_state;
                for delta in deltas {
                    let delta_value: Value = serde_json::to_value(delta).unwrap_or_default();
                    if let Some(path) = delta_value.get("path").and_then(Value::as_str) {
                        let op = delta_value
                            .get("op")
                            .and_then(Value::as_str)
                            .unwrap_or("set");
                        if let Some(value) = delta_value.get("value") {
                            match op {
                                "+" => {
                                    add_json_path(&mut final_state, path, value);
                                }
                                "-" => {
                                    sub_json_path(&mut final_state, path, value);
                                }
                                _ => {
                                    set_json_path(&mut final_state, path, value.clone());
                                }
                            }
                        }
                    }
                }

                // Track the primary entry's post-settle state for the response.
                if *target_id == entry_id {
                    post_settle_state.clone_from(&final_state);
                }

                // Write the updated state back into the entry body.
                if let Some(body) = target_body.get_mut("body") {
                    if let Some(body_obj) = body.as_object_mut() {
                        body_obj.insert("state".to_string(), Value::Object(final_state));
                    }
                }

                // F-006: reconstruct must NOT silently fall back to the
                // unchanged entry — a round-trip failure here means the
                // state is inconsistent and the settle must be rejected.
                let updated_target: crate::KnowledgeEntry =
                    match serde_json::from_value(Value::Object(target_body)) {
                        Ok(e) => e,
                        Err(e) => {
                            return reject(
                                SpokeRejectCode::InternalError,
                                format!(
                                    "failed to reconstruct entry {target_id} after settle: {e}"
                                ),
                                json!({ "target_id": target_id }),
                            );
                        }
                    };

                pending_entry_updates.push((updated_target, expected_rev));
            }
        }

        // ── 9. Atomic commit: entry settles + session state ───────────────
        // Session state is only advanced when compute succeeded and every
        // settle CAS (if any) is ready. One TX → no partial entry writes and
        // no session advance without successful settles.
        let updated_state_json =
            serde_json::to_string(&merged_state).unwrap_or_else(|_| "{}".to_string());
        match super::knowledge_entry_port::commit_compute_settlement(
            self,
            pending_entry_updates,
            Some((session_id.clone(), updated_state_json)),
        )
        .await
        {
            SpokeResult::Ok(()) => {}
            SpokeResult::Reject(r) => return SpokeResult::Reject(r),
        }

        // ── 10. Return ComputeResponse ────────────────────────────────────
        SpokeResult::Ok(ComputeResponse::Variant0 {
            computable: merged_state,
            entry_id,
            session_id,
            extensions: HashMap::default(),
            state: if settle {
                post_settle_state
            } else {
                Map::new()
            },
        })
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────

/// Convert a spoke `KnowledgeEntry` to a JSON `key_block` map suitable for the
/// WASM module's `ComputeInput.key_blocks` snapshot.
///
/// The spoke `KnowledgeEntry.body.attributes` is a `Vec<AttributesItem>`
/// (typed array), but the basic-combat module expects `body.attributes` as a
/// flat object `{max_hp: 100, base_atk: 20, ...}`. This helper converts the
/// spoke attributes vector into the flat-object format.
fn spoke_entry_to_key_block(entry: &crate::KnowledgeEntry) -> Map<String, Value> {
    // Serialize the spoke entry to a JSON map first.
    let Value::Object(mut kb) = serde_json::to_value(entry).unwrap_or_default() else {
        return Map::new();
    };

    // The basic-combat module uses `key_block_id` (not `entry_id`) to
    // identify key_blocks. Add it as a duplicate of entry_id so the
    // module's select_combatants lookup works.
    if let Some(entry_id) = kb.get("entry_id").and_then(Value::as_str) {
        kb.insert(
            "key_block_id".to_string(),
            Value::String(entry_id.to_string()),
        );
    }

    // Convert body.attributes from spoke Vec<AttributesItem> to flat object.
    if let Some(body) = kb.get_mut("body").and_then(Value::as_object_mut) {
        if let Some(attrs_val) = body.remove("attributes") {
            let flat: Map<String, Value> = match attrs_val {
                Value::Array(items) => items
                    .iter()
                    .filter_map(|item| {
                        let trait_type = item.get("trait_type").and_then(Value::as_str)?;
                        let value = item
                            .get("value")
                            .or_else(|| item.get("value"))
                            .cloned()
                            .unwrap_or(Value::Null);
                        Some((trait_type.to_string(), value))
                    })
                    .collect(),
                Value::Object(m) => m, // already flat — pass through
                _ => Map::new(),
            };
            body.insert("attributes".to_string(), Value::Object(flat));
        }
        // Ensure body.state is present (spoken entry may have empty state map).
        if !body.contains_key("state") {
            body.insert("state".to_string(), Value::Object(Map::new()));
        }
    }

    kb
}

/// Set a value at a dotted JSON path (e.g. `"character.current_hp"`).
fn set_json_path(root: &mut Map<String, Value>, path: &str, value: Value) {
    let segments: Vec<&str> = path.split('.').collect();
    if segments.is_empty() {
        return;
    }
    if segments.len() == 1 {
        root.insert(segments[0].to_string(), value);
        return;
    }
    let mut current: &mut Value = root
        .entry(segments[0].to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    for seg in &segments[1..segments.len() - 1] {
        if let Value::Object(ref mut obj) = current {
            current = obj
                .entry(seg.to_string())
                .or_insert_with(|| Value::Object(Map::new()));
        } else {
            // Can't traverse into non-object; overwrite.
            *current = Value::Object(Map::new());
            if let Value::Object(ref mut obj) = current {
                current = obj
                    .entry(seg.to_string())
                    .or_insert_with(|| Value::Object(Map::new()));
            }
        }
    }
    if let Value::Object(ref mut obj) = current {
        obj.insert(segments.last().unwrap().to_string(), value);
    }
}

/// Add a numeric value at a dotted JSON path.
fn add_json_path(root: &mut Map<String, Value>, path: &str, delta: &Value) {
    let segments: Vec<&str> = path.split('.').collect();
    if segments.is_empty() {
        return;
    }
    let mut current: &mut Value = root
        .entry(segments[0].to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    for seg in &segments[1..segments.len() - 1] {
        if let Value::Object(ref mut obj) = current {
            current = obj
                .entry(seg.to_string())
                .or_insert_with(|| Value::Object(Map::new()));
        } else {
            return; // Can't traverse
        }
    }
    let last_key = segments.last().unwrap();
    if let Value::Object(ref mut obj) = current {
        let current_val = obj.get(*last_key).and_then(Value::as_f64).unwrap_or(0.0);
        let delta_f = delta.as_f64().unwrap_or(0.0);
        obj.insert(
            last_key.to_string(),
            Value::Number(
                serde_json::Number::from_f64(current_val + delta_f)
                    .unwrap_or_else(|| serde_json::Number::from_f64(0.0).unwrap()),
            ),
        );
    }
}

/// Subtract a numeric value at a dotted JSON path.
fn sub_json_path(root: &mut Map<String, Value>, path: &str, delta: &Value) {
    let delta_val = delta.as_f64().unwrap_or(0.0);
    let neg_delta = Value::Number(
        serde_json::Number::from_f64(-delta_val)
            .unwrap_or_else(|| serde_json::Number::from_f64(0.0).unwrap()),
    );
    add_json_path(root, path, &neg_delta);
}

/// Construct a `SpokeResult::Reject`.
fn reject<T>(code: SpokeRejectCode, message: impl Into<String>, details: Value) -> SpokeResult<T> {
    let details_map = match details {
        Value::Object(map) => Some(map),
        other => {
            let mut map = Map::new();
            map.insert("detail".to_string(), other);
            Some(map)
        }
    };
    SpokeResult::Reject(SpokeReject {
        code,
        message: message.into(),
        details: details_map,
    })
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ComputablePort;
    use crate::KnowledgeEntryPort;
    use nexus_contracts::BlockType;
    use nexus_knowledge::world_kb::{KnowledgeEntryBody, KnowledgeEntryRecord};
    use nexus_local_db::{open_pool, run_migrations};

    async fn fresh_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool).await.unwrap();
        (pool, dir)
    }

    async fn seed_world(pool: &sqlx::SqlitePool) {
        sqlx::query(
            "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
             VALUES ('ctr_test', 'Test Creator', 'active', datetime('now'), '{}')",
        )
        .execute(pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO narrative_worlds \
             (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, time_policy, metadata_json) \
             VALUES ('wld_cmp', 'wrk_test', 'ctr_test', 'Compute World', 'compute-world', 'active', 'private', 'manual', '{}')",
        )
        .execute(pool)
        .await
        .unwrap();
    }

    /// Build a spoke `KnowledgeEntry` with computable state for a character.
    /// Build a JSON `key_block` in the format the basic-combat WASM module
    /// expects (flat `body.attributes` + `body.state.character`).
    /// Build a spoke `KnowledgeEntry` with computable state (creates via
    /// `put_knowledge_entry`). Uses the V1.145 P1a conversion seam.
    fn spoke_character_entry(
        entry_id: &str,
        canonical_name: &str,
        max_hp: i64,
        base_atk: i64,
        base_def: i64,
        current_hp: i64,
    ) -> crate::KnowledgeEntry {
        use crate::conversion::knowledge_record_to_spoke;
        let mut world = KnowledgeEntryRecord::new("wld_cmp", BlockType::Character, canonical_name);
        world.entry_id = entry_id.to_string();
        world.revision = Some(1);
        world.body = Some(KnowledgeEntryBody {
            summary: Some(format!("{canonical_name} the warrior")),
            attributes: Some({
                let mut attrs = Map::new();
                attrs.insert("max_hp".to_string(), Value::Number(max_hp.into()));
                attrs.insert("base_atk".to_string(), Value::Number(base_atk.into()));
                attrs.insert("base_def".to_string(), Value::Number(base_def.into()));
                serde_json::to_value(attrs).unwrap()
            }),
            state: Some({
                let mut state = Map::new();
                let mut char_state = Map::new();
                char_state.insert("current_hp".to_string(), Value::Number(current_hp.into()));
                char_state.insert("max_hp".to_string(), Value::Number(max_hp.into()));
                state.insert("character".to_string(), Value::Object(char_state));
                Value::Object(state)
            }),
            ..Default::default()
        });
        knowledge_record_to_spoke(&world)
    }

    fn unwrap_ok<T>(result: SpokeResult<T>, label: &str) -> T {
        match result {
            SpokeResult::Ok(v) => v,
            SpokeResult::Reject(r) => panic!("{label}: expected ok, got reject {r:?}"),
        }
    }

    // ── project tests ───────────────────────────────────────────────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn project_happy_path_stages_session() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let adapter = NexusAdapter::new(pool.clone());

        // Create a character entry first.
        let entry = spoke_character_entry("kb_hero", "Hero", 100, 20, 10, 100);
        let _created = unwrap_ok(
            adapter.put_knowledge_entry(entry, None).await,
            "create character",
        );

        // Stage a project session.
        let mut state = Map::new();
        state.insert(
            "module_id".to_string(),
            Value::String("basic-combat".to_string()),
        );
        state.insert(
            "attacker_id".to_string(),
            Value::String("kb_hero".to_string()),
        );
        state.insert("defender_id".to_string(), Value::String(String::new()));

        let project_req = ProjectRequest {
            session_id: "ses_test_001".to_string(),
            entry_id: "kb_hero".to_string(),
            state: state.clone(),
            extensions: HashMap::default(),
        };

        match adapter.project(project_req).await {
            SpokeResult::Ok(ProjectResponse::Variant0 {
                computable,
                entry_id,
                session_id,
                ..
            }) => {
                assert_eq!(entry_id, "kb_hero");
                assert_eq!(session_id, "ses_test_001");
                assert_eq!(
                    computable.get("module_id").and_then(Value::as_str),
                    Some("basic-combat")
                );
            }
            other => panic!("expected Variant0, got {other:?}"),
        }

        // Verify the session row exists in storage.
        let row = get_compute_session(&pool, "ses_test_001")
            .await
            .unwrap()
            .expect("session should exist");
        assert_eq!(row.entry_id, "kb_hero");
        assert!(row.state_json.is_some());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn project_missing_entry_rejects_invalid_input() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let adapter = NexusAdapter::new(pool);
        let project_req = ProjectRequest {
            session_id: "ses_missing".to_string(),
            entry_id: "kb_nonexistent".to_string(),
            state: Map::new(),
            extensions: HashMap::default(),
        };

        match adapter.project(project_req).await {
            SpokeResult::Reject(r) => {
                assert_eq!(r.code, SpokeRejectCode::InvalidInput);
                assert!(r.message.contains("not found"));
            }
            SpokeResult::Ok(_) => panic!("expected InvalidInput reject"),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn project_duplicate_session_rejects_invalid_input() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let adapter = NexusAdapter::new(pool.clone());
        let entry = spoke_character_entry("kb_dup_ses", "DupSes", 100, 20, 10, 100);
        unwrap_ok(adapter.put_knowledge_entry(entry, None).await, "create");

        let project_req = ProjectRequest {
            session_id: "ses_dup".to_string(),
            entry_id: "kb_dup_ses".to_string(),
            state: Map::new(),
            extensions: HashMap::default(),
        };
        unwrap_ok(adapter.project(project_req.clone()).await, "first project");

        match adapter.project(project_req).await {
            SpokeResult::Reject(r) => {
                assert_eq!(r.code, SpokeRejectCode::InvalidInput);
                assert!(r.message.contains("already exists"));
            }
            SpokeResult::Ok(_) => panic!("expected InvalidInput reject for duplicate"),
        }
    }

    // ── compute tests ───────────────────────────────────────────────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn compute_session_not_found_rejects_invalid_input() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let adapter = NexusAdapter::new(pool);
        let compute_req = ComputeRequest {
            session_id: "ses_nonexistent".to_string(),
            entry_id: "kb_whatever".to_string(),
            computable: Map::new(),
            settle: None,
            extensions: HashMap::default(),
        };

        match adapter.compute(compute_req).await {
            SpokeResult::Reject(r) => {
                assert_eq!(r.code, SpokeRejectCode::InvalidInput);
                assert!(r.message.contains("not found"));
            }
            SpokeResult::Ok(_) => panic!("expected InvalidInput for missing session"),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn compute_entry_id_mismatch_rejects_invalid_input() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let adapter = NexusAdapter::new(pool.clone());
        let entry = spoke_character_entry("kb_mismatch", "Mismatch", 100, 20, 10, 100);
        unwrap_ok(adapter.put_knowledge_entry(entry, None).await, "create");

        // Stage a session against kb_mismatch.
        let project_req = ProjectRequest {
            session_id: "ses_mismatch".to_string(),
            entry_id: "kb_mismatch".to_string(),
            state: Map::new(),
            extensions: HashMap::default(),
        };
        unwrap_ok(adapter.project(project_req).await, "project");

        // Compute against a different entry_id.
        let compute_req = ComputeRequest {
            session_id: "ses_mismatch".to_string(),
            entry_id: "kb_different".to_string(),
            computable: Map::new(),
            settle: None,
            extensions: HashMap::default(),
        };

        match adapter.compute(compute_req).await {
            SpokeResult::Reject(r) => {
                assert_eq!(r.code, SpokeRejectCode::InvalidInput);
                assert!(r.message.contains("mismatch"));
            }
            SpokeResult::Ok(_) => panic!("expected InvalidInput for mismatch"),
        }
    }

    /// Integration test: full project → compute round-trip with the
    /// embedded basic-combat module. Creates two character entries,
    /// stages a combat session, runs compute, and verifies the response.
    /// Both character `key_blocks` are bundled into the `ComputeInput` so
    /// the module can look up attacker and defender via `kb_read`.
    ///
    /// Gated behind `nexus_spoke_adapter_no_wasm_target` (set by build.rs
    /// when wasm32-unknown-unknown is absent) — the test exercises the
    /// embedded WASM module and cannot run without the target.
    #[cfg(not(nexus_spoke_adapter_no_wasm_target))]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn project_then_compute_roundtrip_with_basic_combat() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let adapter = NexusAdapter::new(pool.clone());

        // Create two character entries in spoke format.
        let hero = spoke_character_entry("kb_hero_c", "HeroC", 100, 25, 15, 100);
        let monster = spoke_character_entry("kb_monster_c", "MonsterC", 60, 15, 8, 60);
        unwrap_ok(adapter.put_knowledge_entry(hero, None).await, "create hero");
        unwrap_ok(
            adapter.put_knowledge_entry(monster, None).await,
            "create monster",
        );

        // ── project: stage the combat session ──
        let mut state = Map::new();
        state.insert(
            "module_id".to_string(),
            Value::String("basic-combat".to_string()),
        );
        state.insert(
            "attacker_id".to_string(),
            Value::String("kb_hero_c".to_string()),
        );
        state.insert(
            "defender_id".to_string(),
            Value::String("kb_monster_c".to_string()),
        );

        let project_resp = unwrap_ok(
            adapter
                .project(ProjectRequest {
                    session_id: "ses_combat_001".to_string(),
                    entry_id: "kb_hero_c".to_string(),
                    state,
                    extensions: HashMap::default(),
                })
                .await,
            "project combat",
        );

        assert!(matches!(project_resp, ProjectResponse::Variant0 { .. }));

        // ── compute: run the combat ──
        // The compute() method loads both attacker and defender entries
        // from storage and converts them to key_blocks for the WASM module.
        let mut computable = Map::new();
        computable.insert(
            "attacker_id".to_string(),
            Value::String("kb_hero_c".to_string()),
        );
        computable.insert(
            "defender_id".to_string(),
            Value::String("kb_monster_c".to_string()),
        );

        let compute_resp = unwrap_ok(
            adapter
                .compute(ComputeRequest {
                    session_id: "ses_combat_001".to_string(),
                    entry_id: "kb_hero_c".to_string(),
                    computable,
                    settle: Some(false),
                    extensions: HashMap::default(),
                })
                .await,
            "compute combat",
        );

        let ComputeResponse::Variant0 {
            computable: result_computable,
            entry_id,
            session_id,
            ..
        } = compute_resp
        else {
            panic!("expected Variant0 from compute, got {compute_resp:?}");
        };

        assert_eq!(entry_id, "kb_hero_c");
        assert_eq!(session_id, "ses_combat_001");
        // The module should produce some computable output.
        assert!(
            !result_computable.is_empty(),
            "compute should return computable state"
        );
    }

    /// Integration test: project → compute with settle=true persists
    /// the `state_delta` into the `KnowledgeEntry`.
    ///
    /// Gated behind `nexus_spoke_adapter_no_wasm_target` (set by build.rs
    /// when wasm32-unknown-unknown is absent).
    #[cfg(not(nexus_spoke_adapter_no_wasm_target))]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn compute_with_settle_persists_state_delta() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let adapter = NexusAdapter::new(pool.clone());

        let hero = spoke_character_entry("kb_settle_h", "SettleHero", 100, 25, 15, 100);
        let monster = spoke_character_entry("kb_settle_m", "SettleMonster", 60, 15, 8, 60);
        unwrap_ok(adapter.put_knowledge_entry(hero, None).await, "create hero");
        unwrap_ok(
            adapter.put_knowledge_entry(monster, None).await,
            "create monster",
        );

        // project
        let mut state = Map::new();
        state.insert(
            "module_id".to_string(),
            Value::String("basic-combat".to_string()),
        );
        state.insert(
            "attacker_id".to_string(),
            Value::String("kb_settle_h".to_string()),
        );
        state.insert(
            "defender_id".to_string(),
            Value::String("kb_settle_m".to_string()),
        );
        unwrap_ok(
            adapter
                .project(ProjectRequest {
                    session_id: "ses_settle".to_string(),
                    entry_id: "kb_settle_h".to_string(),
                    state,
                    extensions: HashMap::default(),
                })
                .await,
            "project",
        );

        // compute with settle=true
        let mut computable = Map::new();
        computable.insert(
            "attacker_id".to_string(),
            Value::String("kb_settle_h".to_string()),
        );
        computable.insert(
            "defender_id".to_string(),
            Value::String("kb_settle_m".to_string()),
        );

        let compute_resp = unwrap_ok(
            adapter
                .compute(ComputeRequest {
                    session_id: "ses_settle".to_string(),
                    entry_id: "kb_settle_h".to_string(),
                    computable,
                    settle: Some(true),
                    extensions: HashMap::default(),
                })
                .await,
            "compute settle",
        );

        let ComputeResponse::Variant0 { .. } = compute_resp else {
            panic!("expected Variant0 from compute");
        };

        // F-001: with correct target routing, deltas apply to the defender
        // (kb_settle_m), not the primary entry (kb_settle_h). The primary
        // entry's post_state may be empty — that is correct when no deltas
        // target it.

        // Re-read both entries — the defender (target) must have updated state.
        let entry_h = unwrap_ok(
            adapter.get_knowledge_entry("kb_settle_h").await,
            "re-read hero",
        );
        let entry_m = unwrap_ok(
            adapter.get_knowledge_entry("kb_settle_m").await,
            "re-read monster",
        );
        let hero_state = &entry_h.body.state;
        let monster_state = &entry_m.body.state;
        // At least one entry must have post-settle state.
        assert!(
            !hero_state.is_empty() || !monster_state.is_empty(),
            "settle should have persisted state into at least one entry"
        );

        // ── Finding 1 assertion: body.state must NOT contain invocation
        //    metadata (module_id, attacker_id, defender_id). The settle path
        //    merges only state_delta, not the full merged session state.
        for forbidden_key in &["module_id", "attacker_id", "defender_id"] {
            for (label, state) in &[("hero", hero_state), ("monster", monster_state)] {
                assert!(
                    !state.contains_key(*forbidden_key),
                    "{label} body.state must not contain invocation metadata key '{forbidden_key}': {label}_state={state:?}"
                );
            }
        }
    }

    /// When neither session state nor entry `body.computable` provide a
    /// `module_id`, `compute()` must reject with `InvalidInput` — no silent
    /// default.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn compute_missing_module_id_rejects_invalid_input() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let adapter = NexusAdapter::new(pool.clone());
        let entry = spoke_character_entry("kb_no_mod", "NoModule", 100, 20, 10, 100);
        unwrap_ok(adapter.put_knowledge_entry(entry, None).await, "create");

        // Project WITHOUT module_id in state and WITHOUT body.computable on
        // the entry — both resolution tiers are empty.
        unwrap_ok(
            adapter
                .project(ProjectRequest {
                    session_id: "ses_no_mod".to_string(),
                    entry_id: "kb_no_mod".to_string(),
                    state: Map::new(),
                    extensions: HashMap::default(),
                })
                .await,
            "project no-module",
        );

        match adapter
            .compute(ComputeRequest {
                session_id: "ses_no_mod".to_string(),
                entry_id: "kb_no_mod".to_string(),
                computable: Map::new(),
                settle: None,
                extensions: HashMap::default(),
            })
            .await
        {
            SpokeResult::Reject(r) => {
                assert_eq!(r.code, SpokeRejectCode::InvalidInput);
                assert!(r.message.contains("module identity required"));
            }
            SpokeResult::Ok(_) => panic!("expected InvalidInput for missing module_id"),
        }
    }

    // ── WASM engine / module loading edge cases ──────────────────────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn compute_unknown_module_rejects_invalid_input() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let adapter = NexusAdapter::new(pool.clone());
        let entry = spoke_character_entry("kb_unknown_mod", "UnknownMod", 100, 20, 10, 100);
        unwrap_ok(adapter.put_knowledge_entry(entry, None).await, "create");

        // Stage with a non-existent module_id.
        let mut state = Map::new();
        state.insert(
            "module_id".to_string(),
            Value::String("nonexistent-module".to_string()),
        );
        unwrap_ok(
            adapter
                .project(ProjectRequest {
                    session_id: "ses_bad_mod".to_string(),
                    entry_id: "kb_unknown_mod".to_string(),
                    state,
                    extensions: HashMap::default(),
                })
                .await,
            "project",
        );

        match adapter
            .compute(ComputeRequest {
                session_id: "ses_bad_mod".to_string(),
                entry_id: "kb_unknown_mod".to_string(),
                computable: Map::new(),
                settle: None,
                extensions: HashMap::default(),
            })
            .await
        {
            SpokeResult::Reject(r) => {
                // F-002: unknown module_id is a client-input error → InvalidInput.
                assert_eq!(r.code, SpokeRejectCode::InvalidInput);
                assert!(r.message.contains("unknown embedded WASM module"));
            }
            SpokeResult::Ok(_) => panic!("expected InvalidInput for unknown module"),
        }
    }

    /// P2 QC fix wave FW-2: the user-module load path must reuse the
    /// per-adapter compiled-module cache — repeated loads of UNCHANGED
    /// bytes return the SAME cached entry (Arc identity across two loads is
    /// the timing-independent no-recompile observable), and a CHANGED
    /// module file (same id, different bytes hash) is a cache miss that
    /// recompiles and OVERWRITES the entry (the cache's eviction
    /// semantics) without ever growing the cache.
    #[cfg(not(nexus_spoke_adapter_no_wasm_target))]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn user_module_load_reuses_compiled_module_cache() {
        let (pool, _db_dir) = fresh_pool().await;

        // Install the embedded basic-combat bytes as a user module under a
        // hermetic module store.
        let module_root = tempfile::tempdir().unwrap();
        let module_dir = module_root.path().join("basic-combat");
        std::fs::create_dir_all(&module_dir).unwrap();
        let wasm_path = module_dir.join("basic-combat.wasm");
        let manifest_path = module_dir.join("manifest.json");
        let bytes = nexus_wasm_host::embedded_module_bytes("basic-combat")
            .expect("embedded basic-combat bytes");
        let manifest = nexus_wasm_host::embedded_module_manifest("basic-combat")
            .expect("embedded basic-combat manifest");
        std::fs::write(&wasm_path, bytes).expect("write module wasm");
        std::fs::write(&manifest_path, manifest).expect("write module manifest");

        let adapter =
            NexusAdapter::new(pool).with_user_modules_dir(module_root.path().to_path_buf());
        let cache = adapter.module_cache();
        assert_eq!(cache.len(), 0, "fresh adapter starts with an empty cache");

        let (_first_module, _) =
            unwrap_ok(adapter.load_module("basic-combat"), "first load compiles");
        assert_eq!(cache.len(), 1, "first load compiles and caches");
        assert!(cache.contains("basic-combat"));
        // Capture the cached entry NOW — before the second load — so the
        // no-recompile assertion compares the first compile against the
        // post-second-load cache state (fetching both after both loads
        // would trivially return the same latest entry).
        let first_entry = cache
            .get("basic-combat")
            .expect("cached entry after first load");

        let (_second_module, _) = unwrap_ok(
            adapter.load_module("basic-combat"),
            "second load hits the cache",
        );
        assert_eq!(cache.len(), 1, "second load must not grow the cache");
        let second_entry = cache
            .get("basic-combat")
            .expect("cached entry after second load");
        assert!(
            std::sync::Arc::ptr_eq(&first_entry, &second_entry),
            "repeated loads of unchanged bytes must return the SAME compiled module \
             (cache hit — no recompile)"
        );

        // Operator updates the module pair: different bytes under the same
        // id ⇒ bytes-hash miss ⇒ recompile ⇒ the cached entry is REPLACED
        // (overwrite eviction), never duplicated. The replacement is a
        // minimal valid wasm module (magic + version, zero sections) —
        // different bytes that still compile. The manifest is rewritten in
        // the same update (a coherent pair) WITHOUT `wasm_sha256` — the
        // embedded field only hashes the original bytes — so this pair
        // loads through the legacy stat-fence fallback.
        let changed: &[u8] = b"\0asm\x01\0\0\0";
        std::fs::write(&wasm_path, changed).expect("rewrite module wasm");
        let manifest_no_hash = {
            let mut value: serde_json::Value =
                serde_json::from_str(manifest).expect("embedded manifest parses");
            value
                .as_object_mut()
                .expect("manifest object")
                .remove("wasm_sha256");
            serde_json::to_string(&value).expect("manifest without wasm_sha256 serializes")
        };
        std::fs::write(&manifest_path, &manifest_no_hash).expect("rewrite module manifest");

        let (_third_module, _) = unwrap_ok(
            adapter.load_module("basic-combat"),
            "changed bytes recompile",
        );
        assert_eq!(cache.len(), 1, "recompile overwrites, never duplicates");
        let third_entry = cache.get("basic-combat").expect("recompiled entry");
        assert!(
            !std::sync::Arc::ptr_eq(&first_entry, &third_entry),
            "changed bytes must produce a FRESH compiled module (recompile, not stale serve)"
        );

        // Greptile P1 (manifest half of the cache identity): an operator
        // updating manifest.json WITHOUT touching the wasm (new schemas /
        // sandbox overrides) must miss the cache and recompile with the
        // NEW settings — the old behavior kept serving the stale manifest
        // until the wasm changed or the process restarted.
        let manifest_v2 = {
            let mut value: serde_json::Value =
                serde_json::from_str(&manifest_no_hash).expect("legacy manifest parses");
            value["version"] = serde_json::json!("2.0.0");
            serde_json::to_string(&value).expect("manifest v2 serializes")
        };
        std::fs::write(&manifest_path, manifest_v2).expect("rewrite module manifest");

        let (reloaded_module, reloaded_manifest) = unwrap_ok(
            adapter.load_module("basic-combat"),
            "manifest-only change recompiles",
        );
        assert_eq!(cache.len(), 1, "recompile overwrites, never duplicates");
        let manifest_entry = cache.get("basic-combat").expect("recompiled entry");
        assert!(
            !std::sync::Arc::ptr_eq(&third_entry, &manifest_entry),
            "a manifest-only change must produce a FRESH compiled module (stale settings never served)"
        );
        assert_eq!(
            reloaded_manifest.version, "2.0.0",
            "the fresh entry serves the NEW manifest settings"
        );
        let _ = reloaded_module;
    }

    /// Greptile P1 (root cause — content-based pairing): a user module whose
    /// manifest declares the CORRECT `wasm_sha256` for its `.wasm` bytes
    /// loads normally (and caches).
    #[cfg(not(nexus_spoke_adapter_no_wasm_target))]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn user_module_load_accepts_matching_wasm_sha256() {
        let (pool, _db_dir) = fresh_pool().await;
        let module_root = tempfile::tempdir().unwrap();
        let module_dir = module_root.path().join("basic-combat");
        std::fs::create_dir_all(&module_dir).unwrap();
        let wasm_path = module_dir.join("basic-combat.wasm");
        let manifest_path = module_dir.join("manifest.json");
        let bytes = nexus_wasm_host::embedded_module_bytes("basic-combat")
            .expect("embedded basic-combat bytes");
        // The embedded manifest carries `wasm_sha256` computed by build.rs
        // from the EXACT embedded bytes — a correct pair.
        let manifest = nexus_wasm_host::embedded_module_manifest("basic-combat")
            .expect("embedded basic-combat manifest");
        assert!(
            serde_json::from_str::<serde_json::Value>(manifest).expect("manifest parses")
                ["wasm_sha256"]
                .is_string(),
            "the embedded manifest must declare wasm_sha256 (build.rs injects it)"
        );
        std::fs::write(&wasm_path, bytes).expect("write module wasm");
        std::fs::write(&manifest_path, manifest).expect("write module manifest");

        let adapter =
            NexusAdapter::new(pool).with_user_modules_dir(module_root.path().to_path_buf());
        let cache = adapter.module_cache();
        let (module, served_manifest) = unwrap_ok(
            adapter.load_module("basic-combat"),
            "correct wasm_sha256 pair loads",
        );
        assert_eq!(
            served_manifest.wasm_sha256.as_deref(),
            serde_json::from_str::<serde_json::Value>(manifest).expect("manifest parses")
                ["wasm_sha256"]
                .as_str(),
            "the served manifest carries the declared hash"
        );
        assert_eq!(cache.len(), 1, "the verified pair is cached");
        let _ = module;
    }

    /// Greptile P1 (root cause — content-based pairing): a mixed pair — OLD
    /// manifest + NEW wasm — must be rejected with `InternalError` BEFORE
    /// the cache: the bytes do not hash to the manifest's `wasm_sha256`.
    /// This is the case the stat fence could not see (the straddling-swap
    /// residual) — the content hash closes it for manifests that declare it.
    #[cfg(not(nexus_spoke_adapter_no_wasm_target))]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn user_module_load_rejects_mismatched_wasm_sha256_without_caching() {
        let (pool, _db_dir) = fresh_pool().await;
        let module_root = tempfile::tempdir().unwrap();
        let module_dir = module_root.path().join("basic-combat");
        std::fs::create_dir_all(&module_dir).unwrap();
        let wasm_path = module_dir.join("basic-combat.wasm");
        let manifest_path = module_dir.join("manifest.json");
        let bytes = nexus_wasm_host::embedded_module_bytes("basic-combat")
            .expect("embedded basic-combat bytes");
        // Mixed pair: the manifest is the OLD manifest for the embedded
        // bytes, but the installed wasm is DIFFERENT (a new build swapped in
        // without updating the manifest — the Greptile P1 scenario). The
        // manifest declares the hash of the ORIGINAL bytes; the installed
        // bytes hash to something else.
        let manifest = nexus_wasm_host::embedded_module_manifest("basic-combat")
            .expect("embedded basic-combat manifest");
        let changed: &[u8] = b"\0asm\x01\0\0\0";
        assert_ne!(
            changed, bytes,
            "the swapped-in wasm must differ from the manifest's bytes"
        );
        std::fs::write(&wasm_path, changed).expect("write module wasm");
        std::fs::write(&manifest_path, manifest).expect("write module manifest");

        let adapter =
            NexusAdapter::new(pool).with_user_modules_dir(module_root.path().to_path_buf());
        let cache = adapter.module_cache();
        assert_eq!(cache.len(), 0, "fresh adapter starts with an empty cache");

        match adapter.load_module("basic-combat") {
            SpokeResult::Ok(_) => panic!("a mixed pair must NOT load"),
            SpokeResult::Reject(r) => {
                assert_eq!(
                    r.code,
                    SpokeRejectCode::InternalError,
                    "a manifest/wasm mismatch is a host fault: {r:?}"
                );
                assert!(
                    r.message
                        .contains("wasm does not match manifest wasm_sha256"),
                    "reject must name the pairing failure, got: {}",
                    r.message
                );
            }
        }
        assert_eq!(
            cache.len(),
            0,
            "the mixed pair must never enter the cache (reject happens BEFORE get_or_compile)"
        );
        assert!(!cache.contains("basic-combat"));
    }

    /// Greptile P1 (legacy fallback): a manifest WITHOUT `wasm_sha256` still
    /// loads through the stat fence — the content check is skipped, so the
    /// legacy behavior (and its straddling-swap residual) is unchanged for
    /// manifests that do not declare the field.
    #[cfg(not(nexus_spoke_adapter_no_wasm_target))]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn user_module_load_without_wasm_sha256_falls_back_to_stat_fence() {
        let (pool, _db_dir) = fresh_pool().await;
        let module_root = tempfile::tempdir().unwrap();
        let module_dir = module_root.path().join("basic-combat");
        std::fs::create_dir_all(&module_dir).unwrap();
        let wasm_path = module_dir.join("basic-combat.wasm");
        let manifest_path = module_dir.join("manifest.json");
        let bytes = nexus_wasm_host::embedded_module_bytes("basic-combat")
            .expect("embedded basic-combat bytes");
        let manifest = nexus_wasm_host::embedded_module_manifest("basic-combat")
            .expect("embedded basic-combat manifest");
        let legacy_manifest = {
            let mut value: serde_json::Value =
                serde_json::from_str(manifest).expect("embedded manifest parses");
            value
                .as_object_mut()
                .expect("manifest object")
                .remove("wasm_sha256");
            serde_json::to_string(&value).expect("legacy manifest serializes")
        };
        std::fs::write(&wasm_path, bytes).expect("write module wasm");
        std::fs::write(&manifest_path, &legacy_manifest).expect("write legacy module manifest");

        let adapter =
            NexusAdapter::new(pool).with_user_modules_dir(module_root.path().to_path_buf());
        let (module, served_manifest) = unwrap_ok(
            adapter.load_module("basic-combat"),
            "legacy manifest without wasm_sha256 loads via the stat fence",
        );
        assert!(
            served_manifest.wasm_sha256.is_none(),
            "legacy manifest serves without a declared hash"
        );
        let _ = module;
    }

    /// Greptile P1 (non-atomic module reload — fence tier): the coherence
    /// fence behind [`module_pair_changed_mid_load`] must report ANY
    /// mid-load mutation of the module pair. The caller's sequence is
    /// manifest read (m1) → wasm stat (s1) → wasm read (b) → fence close
    /// (s2 + m2); the tests below replay that sequence with a replacement
    /// injected at each observable point: a stable pair is coherent, a
    /// wasm replaced between the two stats OR a manifest replaced between
    /// m1 and m2 is a mid-load mutation the loader must reject (never
    /// compile the mixed pair into the cache).
    #[test]
    fn module_pair_changed_mid_load_detects_replaced_files() {
        let temp = tempfile::tempdir().expect("tempdir");
        let wasm_path = temp.path().join("m.wasm");
        let manifest_path = temp.path().join("manifest.json");
        std::fs::write(&wasm_path, b"\0asm\x01\0\0\0").expect("write wasm");
        std::fs::write(&manifest_path, r#"{"module_id":"m"}"#).expect("write manifest");

        // (c) Coherent pair ⇒ accepted: nothing changes between m1, s1,
        // the wasm read, s2 and m2.
        let manifest_first = std::fs::read_to_string(&manifest_path).expect("first manifest read");
        let wasm_stat_first = std::fs::metadata(&wasm_path).expect("first wasm stat");
        let wasm_first = std::fs::read(&wasm_path).expect("first wasm read");
        assert!(
            !module_pair_changed_mid_load(
                &wasm_path,
                &manifest_path,
                &wasm_stat_first,
                &manifest_first,
            )
            .expect("stable pair fence"),
            "a stable pair is a consistent snapshot"
        );
        let _ = wasm_first;

        // (a) WASM replaced between the two stats ⇒ rejected: the
        // replacement lands after the caller's s1 stat (on the OLD wasm)
        // and before the fence's s2 stat — the Greptile hole, where a
        // bytes-only re-read observed a stable wasm (both byte reads saw
        // the NEW file) and accepted OLD manifest + NEW wasm. The stat
        // fence fires because s2 observes the NEW file's metadata.
        let manifest_first = std::fs::read_to_string(&manifest_path).expect("manifest read");
        let wasm_stat_first = std::fs::metadata(&wasm_path).expect("wasm stat");
        std::fs::write(&wasm_path, b"\0asm\x01\0\0\x01").expect("rewrite wasm between stats");
        let wasm_first = std::fs::read(&wasm_path).expect("wasm read");
        assert!(
            module_pair_changed_mid_load(
                &wasm_path,
                &manifest_path,
                &wasm_stat_first,
                &manifest_first,
            )
            .expect("replaced wasm fence"),
            "a wasm replaced between the two stats must be detected"
        );
        let _ = wasm_first;

        // (b) Manifest replaced between m1 and m2 ⇒ rejected.
        std::fs::write(&wasm_path, b"\0asm\x01\0\0\0").expect("restore wasm");
        std::fs::write(&manifest_path, r#"{"module_id":"m"}"#).expect("restore manifest");
        let manifest_first = std::fs::read_to_string(&manifest_path).expect("manifest read");
        let wasm_stat_first = std::fs::metadata(&wasm_path).expect("wasm stat");
        std::fs::write(&manifest_path, r#"{"module_id":"m","version":"2"}"#)
            .expect("rewrite manifest");
        assert!(
            module_pair_changed_mid_load(
                &wasm_path,
                &manifest_path,
                &wasm_stat_first,
                &manifest_first,
            )
            .expect("replaced manifest fence"),
            "a manifest replaced between m1 and m2 must be detected"
        );
    }

    /// Greptile P1 (non-atomic module reload — legacy-only residual): a
    /// replacement pair whose writes land OUTSIDE the fence's observation
    /// windows — here the wasm write lands between m1 and s1 (before the
    /// first stat) while the manifest is untouched — leaves each file
    /// stable at its own observation points, so the mixed pair is
    /// indistinguishable from a coherent one without a content hash. This
    /// test uses a manifest WITHOUT `wasm_sha256`, so it documents the
    /// residual that remains ONLY for legacy manifests: operators who set
    /// `wasm_sha256` (SHOULD, per the manifest docs) are protected by
    /// content-based pairing instead — an old manifest + new wasm always
    /// mismatches and is rejected before it can be cached. The operator
    /// install tool should additionally do atomic directory replacement
    /// (write tmp → rename) so the loader observes either the old pair or
    /// the new pair, never a mix.
    #[test]
    fn module_pair_changed_mid_load_accepts_straddling_pair_swap_residual() {
        let temp = tempfile::tempdir().expect("tempdir");
        let wasm_path = temp.path().join("m.wasm");
        let manifest_path = temp.path().join("manifest.json");
        std::fs::write(&wasm_path, b"\0asm\x01\0\0\0").expect("write wasm");
        std::fs::write(&manifest_path, r#"{"module_id":"m"}"#).expect("write manifest");

        // m1 read, then the operator's wasm write lands BEFORE s1 — both
        // wasm stats and the byte read observe the NEW wasm, and both
        // manifest reads observe the OLD manifest: the fence passes.
        let manifest_first = std::fs::read_to_string(&manifest_path).expect("manifest read");
        std::fs::write(&wasm_path, b"\0asm\x01\0\0\x01").expect("operator writes wasm");
        let wasm_stat_first = std::fs::metadata(&wasm_path).expect("wasm stat");
        let wasm_first = std::fs::read(&wasm_path).expect("wasm read");
        assert!(
            !module_pair_changed_mid_load(
                &wasm_path,
                &manifest_path,
                &wasm_stat_first,
                &manifest_first,
            )
            .expect("straddling swap fence"),
            "residual: a mixed pair stable at every observation point passes the fence"
        );
        let _ = wasm_first;
    }

    /// Greptile P1 (non-atomic module reload — integration): a concurrent
    /// writer thrashing the module pair while `load_module` runs must
    /// never produce a panicking loader or a served mixed pair. Every load
    /// either returns a coherent pair (both files read identically twice)
    /// or rejects with `InternalError` (mid-load change detected); once
    /// the writer stops, the loader serves the final coherent pair.
    #[cfg(not(nexus_spoke_adapter_no_wasm_target))]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn user_module_load_under_concurrent_replace_never_serves_mixed_pair() {
        let (pool, _db_dir) = fresh_pool().await;
        let module_root = tempfile::tempdir().unwrap();
        let module_dir = module_root.path().join("basic-combat");
        std::fs::create_dir_all(&module_dir).unwrap();
        let wasm_path = module_dir.join("basic-combat.wasm");
        let manifest_path = module_dir.join("manifest.json");
        let bytes = nexus_wasm_host::embedded_module_bytes("basic-combat")
            .expect("embedded basic-combat bytes");
        let manifest = nexus_wasm_host::embedded_module_manifest("basic-combat")
            .expect("embedded basic-combat manifest");
        // Pair B: different (still valid) wasm + a DIFFERENT manifest — a
        // coherent B is legal to serve; a mixed A/B read must be rejected.
        let changed: &[u8] = b"\0asm\x01\0\0\0";
        let manifest_b = r#"{"module_id":"basic-combat","name":"Churn","version":"2"}"#;
        std::fs::write(&wasm_path, bytes).expect("write module wasm");
        std::fs::write(&manifest_path, manifest).expect("write module manifest");

        let adapter =
            NexusAdapter::new(pool).with_user_modules_dir(module_root.path().to_path_buf());
        let adapter = std::sync::Arc::new(adapter);

        // Churn writer: alternate the pair while the loader runs.
        let wasm_path_t = wasm_path.clone();
        let manifest_path_t = manifest_path.clone();
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop_t = std::sync::Arc::clone(&stop);
        let writer = std::thread::spawn(move || {
            let mut pair_b = false;
            while !stop_t.load(std::sync::atomic::Ordering::Relaxed) {
                if pair_b {
                    std::fs::write(&wasm_path_t, changed).expect("write wasm B");
                    std::fs::write(&manifest_path_t, manifest_b).expect("write manifest B");
                } else {
                    std::fs::write(&wasm_path_t, bytes).expect("write wasm A");
                    std::fs::write(&manifest_path_t, manifest).expect("write manifest A");
                }
                pair_b = !pair_b;
                std::thread::yield_now();
            }
        });

        // Loader: every outcome must be Ok (coherent pair) or
        // InternalError (mid-load mutation) — never a panic, never a
        // misclassified code.
        for _ in 0..200 {
            match adapter.load_module("basic-combat") {
                SpokeResult::Ok(_) => {}
                SpokeResult::Reject(r) => assert_eq!(
                    r.code,
                    SpokeRejectCode::InternalError,
                    "a mid-load mutation must classify as InternalError, got {r:?}"
                ),
            }
        }
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        writer.join().expect("writer thread joins");

        // Final coherent pair (pair A restored) loads cleanly.
        std::fs::write(&wasm_path, bytes).expect("restore wasm");
        std::fs::write(&manifest_path, manifest).expect("restore manifest");
        let (module, served_manifest) =
            unwrap_ok(adapter.load_module("basic-combat"), "final pair loads");
        assert_eq!(
            served_manifest.module_id, "basic-combat",
            "the served manifest is the coherent final pair"
        );
        let _ = module;
    }

    /// P2 QC fix wave FW-3 (adapter tier — TOCTOU defense): compute on a
    /// session whose target entry was deleted must reject with
    /// `InvalidInput` ("target `KnowledgeEntry` not found"), mirroring
    /// `project()` — never an unclassified `KnowledgeEntryNotFound` reject.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn compute_missing_entry_rejects_invalid_input() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let adapter = NexusAdapter::new(pool.clone());
        let entry = spoke_character_entry("kb_vanished", "Vanished", 100, 20, 10, 100);
        unwrap_ok(adapter.put_knowledge_entry(entry, None).await, "create");

        // Stage a session against the entry, then delete the entry before
        // compute runs (the check-then-act interleaving the Connect gate
        // cannot see).
        unwrap_ok(
            adapter
                .project(ProjectRequest {
                    session_id: "ses_vanished".to_string(),
                    entry_id: "kb_vanished".to_string(),
                    state: Map::new(),
                    extensions: HashMap::default(),
                })
                .await,
            "project",
        );
        // Direct store delete — the adapter has no delete port, so remove
        // the row like a concurrent writer would.
        sqlx::query("DELETE FROM kb_key_blocks WHERE key_block_id = ?")
            .bind("kb_vanished")
            .execute(&pool)
            .await
            .expect("delete entry");

        match adapter
            .compute(ComputeRequest {
                session_id: "ses_vanished".to_string(),
                entry_id: "kb_vanished".to_string(),
                computable: Map::new(),
                settle: None,
                extensions: HashMap::default(),
            })
            .await
        {
            SpokeResult::Reject(r) => {
                assert_eq!(
                    r.code,
                    SpokeRejectCode::InvalidInput,
                    "compute on a missing entry must be InvalidInput (client-input family)"
                );
                assert!(r.message.contains("not found for compute"));
            }
            SpokeResult::Ok(_) => panic!("expected InvalidInput for missing compute target entry"),
        }
    }

    /// P2 QC fix wave FW-5: the missing-module-identity reject carries the
    /// `module_identity_missing` details marker and the shared predicate
    /// recognizes it — hosts classify the defined `module_not_found`
    /// denial by marker, never by sniffing the reject message (a message
    /// rewording cannot silently remap the wire code).
    #[test]
    fn module_identity_missing_reject_carries_marker() {
        // No module id anywhere (empty state, entry without body.computable).
        let entry = spoke_character_entry("kb_marker", "Marker", 100, 20, 10, 100);
        match resolve_module_id(&Map::new(), &entry) {
            SpokeResult::Reject(r) => {
                assert_eq!(r.code, SpokeRejectCode::InvalidInput);
                assert!(
                    is_module_identity_missing_reject(&r),
                    "the identity-missing reject must carry the marker"
                );
                assert!(r.message.contains("module identity required"));
            }
            SpokeResult::Ok(_) => panic!("no module identity must reject"),
        }
        // A non-marker client-input reject is NOT classified as
        // identity-missing (the gate must not remap it to module_not_found).
        let other: SpokeResult<()> = reject(
            SpokeRejectCode::InvalidInput,
            "some other client error",
            json!({}),
        );
        let other = match other {
            SpokeResult::Reject(r) => r,
            SpokeResult::Ok(()) => panic!("reject helper must reject"),
        };
        assert!(!is_module_identity_missing_reject(&other));
    }

    /// P2 QC fix wave FW-4: the shared id-safety helper accepts only single
    /// path components — the Connect gate's host-store check and the
    /// adapter's user-module loader both route through it.
    #[test]
    fn is_safe_module_id_accepts_single_components_only() {
        assert!(is_safe_module_id("basic-combat"));
        assert!(is_safe_module_id("a1"));
        assert!(!is_safe_module_id(""));
        assert!(!is_safe_module_id("a/b"));
        assert!(!is_safe_module_id("a\\b"));
        assert!(!is_safe_module_id("."));
        assert!(!is_safe_module_id(".."));
        assert!(!is_safe_module_id("../etc/passwd"));
    }

    /// F-001 regression: settle must route deltas by `target_key_block_id`.
    /// After settle with attacker + defender, the defender's HP must decrease
    /// by the delta value while the attacker's HP remains unchanged.
    #[cfg(not(nexus_spoke_adapter_no_wasm_target))]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    // Long integration test; splitting would obscure the end-to-end scenario
    #[allow(clippy::too_many_lines)]
    async fn settle_routes_deltas_by_target_key_block_id() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let adapter = NexusAdapter::new(pool.clone());

        // Create attacker (hero) and defender (monster) entries.
        let hero = spoke_character_entry("kb_atk_r", "AttackerR", 100, 25, 15, 100);
        let monster = spoke_character_entry("kb_def_r", "DefenderR", 60, 15, 8, 60);
        unwrap_ok(adapter.put_knowledge_entry(hero, None).await, "create hero");
        unwrap_ok(
            adapter.put_knowledge_entry(monster, None).await,
            "create monster",
        );

        // Record pre-settle HP values.
        let pre_hero = unwrap_ok(
            adapter.get_knowledge_entry("kb_atk_r").await,
            "read hero pre",
        );
        let pre_monster = unwrap_ok(
            adapter.get_knowledge_entry("kb_def_r").await,
            "read monster pre",
        );
        let pre_hero_hp = pre_hero
            .body
            .state
            .get("character")
            .and_then(|c| c.get("current_hp"))
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(-1);
        let pre_monster_hp = pre_monster
            .body
            .state
            .get("character")
            .and_then(|c| c.get("current_hp"))
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(-1);
        assert_eq!(pre_hero_hp, 100, "attacker starts at 100 HP");
        assert_eq!(pre_monster_hp, 60, "defender starts at 60 HP");

        // Project.
        let mut state = Map::new();
        state.insert(
            "module_id".to_string(),
            Value::String("basic-combat".to_string()),
        );
        state.insert(
            "attacker_id".to_string(),
            Value::String("kb_atk_r".to_string()),
        );
        state.insert(
            "defender_id".to_string(),
            Value::String("kb_def_r".to_string()),
        );
        unwrap_ok(
            adapter
                .project(ProjectRequest {
                    session_id: "ses_f001".to_string(),
                    entry_id: "kb_atk_r".to_string(),
                    state,
                    extensions: HashMap::default(),
                })
                .await,
            "project",
        );

        // Compute with settle=true.
        let mut computable = Map::new();
        computable.insert(
            "attacker_id".to_string(),
            Value::String("kb_atk_r".to_string()),
        );
        computable.insert(
            "defender_id".to_string(),
            Value::String("kb_def_r".to_string()),
        );
        unwrap_ok(
            adapter
                .compute(ComputeRequest {
                    session_id: "ses_f001".to_string(),
                    entry_id: "kb_atk_r".to_string(),
                    computable,
                    settle: Some(true),
                    extensions: HashMap::default(),
                })
                .await,
            "compute settle",
        );

        // Re-read both entries.
        let post_hero = unwrap_ok(
            adapter.get_knowledge_entry("kb_atk_r").await,
            "read hero post",
        );
        let post_monster = unwrap_ok(
            adapter.get_knowledge_entry("kb_def_r").await,
            "read monster post",
        );
        let post_hero_hp = post_hero
            .body
            .state
            .get("character")
            .and_then(|c| c.get("current_hp"))
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(-1);
        let post_monster_hp = post_monster
            .body
            .state
            .get("character")
            .and_then(|c| c.get("current_hp"))
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(-1);

        // F-001 key assertions:
        // 1. Defender HP decreased (delta target = kb_def_r).
        assert!(
            post_monster_hp < pre_monster_hp,
            "defender HP should decrease: was {pre_monster_hp}, now {post_monster_hp}"
        );
        // 2. Attacker HP unchanged (delta target ≠ kb_atk_r).
        assert_eq!(
            post_hero_hp, pre_hero_hp,
            "attacker HP must remain unchanged: was {pre_hero_hp}, now {post_hero_hp}"
        );
    }

    /// F-003: cross-world references must be rejected as `InvalidInput`.
    /// A compute session for world A must not pull `key_block` bodies from
    /// world B.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cross_world_defender_rejects_invalid_input() {
        use crate::conversion::knowledge_record_to_spoke;
        use nexus_contracts::BlockType;
        use nexus_knowledge::world_kb::{KnowledgeEntryBody, KnowledgeEntryRecord};
        let (pool, _dir) = fresh_pool().await;
        // Seed two distinct worlds.
        seed_world(&pool).await;
        sqlx::query(
            "INSERT INTO narrative_worlds \
             (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, time_policy, metadata_json) \
             VALUES ('wld_other', 'wrk_test', 'ctr_test', 'Other World', 'other-world', 'active', 'private', 'manual', '{}')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let adapter = NexusAdapter::new(pool.clone());

        // Create a character in world A (wld_cmp, seeded by seed_world).
        let hero = spoke_character_entry("kb_hero_f003", "HeroF003", 100, 20, 10, 100);
        unwrap_ok(
            adapter.put_knowledge_entry(hero, None).await,
            "create hero in wld_cmp",
        );

        // Create a character in world B (wld_other).
        // spoke_character_entry hardcodes wld_cmp; we create a KnowledgeEntryRecord
        // for wld_other explicitly and convert.
        let mut world_b = KnowledgeEntryRecord::new("wld_other", BlockType::Character, "OtherF003");
        world_b.entry_id = "kb_other_f003".to_string();
        world_b.revision = Some(1);
        world_b.body = Some(KnowledgeEntryBody {
            summary: Some("Other world character".into()),
            attributes: Some({
                let mut attrs = Map::new();
                attrs.insert("max_hp".to_string(), Value::Number(50.into()));
                attrs.insert("base_atk".to_string(), Value::Number(10.into()));
                attrs.insert("base_def".to_string(), Value::Number(5.into()));
                serde_json::to_value(attrs).unwrap()
            }),
            state: Some({
                let mut state = Map::new();
                let mut char_state = Map::new();
                char_state.insert("current_hp".to_string(), Value::Number(50.into()));
                char_state.insert("max_hp".to_string(), Value::Number(50.into()));
                state.insert("character".to_string(), Value::Object(char_state));
                Value::Object(state)
            }),
            ..Default::default()
        });
        let other_spoke = knowledge_record_to_spoke(&world_b);
        unwrap_ok(
            adapter.put_knowledge_entry(other_spoke, None).await,
            "create other_world char",
        );

        // Project a session in wld_cmp with attacker_id=kb_hero_f003,
        // defender_id=kb_other_f003 (cross-world).
        let mut state = Map::new();
        state.insert(
            "module_id".to_string(),
            Value::String("basic-combat".to_string()),
        );
        state.insert(
            "attacker_id".to_string(),
            Value::String("kb_hero_f003".to_string()),
        );
        state.insert(
            "defender_id".to_string(),
            Value::String("kb_other_f003".to_string()),
        );
        unwrap_ok(
            adapter
                .project(ProjectRequest {
                    session_id: "ses_f003".to_string(),
                    entry_id: "kb_hero_f003".to_string(),
                    state,
                    extensions: HashMap::default(),
                })
                .await,
            "project",
        );

        // Compute — should reject because defender is in a different world.
        let mut computable = Map::new();
        computable.insert(
            "attacker_id".to_string(),
            Value::String("kb_hero_f003".to_string()),
        );
        computable.insert(
            "defender_id".to_string(),
            Value::String("kb_other_f003".to_string()),
        );
        match adapter
            .compute(ComputeRequest {
                session_id: "ses_f003".to_string(),
                entry_id: "kb_hero_f003".to_string(),
                computable,
                settle: None,
                extensions: HashMap::default(),
            })
            .await
        {
            SpokeResult::Reject(r) => {
                assert_eq!(r.code, SpokeRejectCode::InvalidInput);
                assert!(r.message.contains("cross-world"));
            }
            SpokeResult::Ok(_) => panic!("expected InvalidInput for cross-world reference"),
        }
    }

    /// F-005: `module_id` with path-traversal characters must be rejected
    /// as `InvalidInput` (format allowlist via `embedded_module_ids()`).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn compute_module_id_with_path_traversal_rejects_invalid_input() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let adapter = NexusAdapter::new(pool.clone());
        let entry = spoke_character_entry("kb_pathmod", "PathMod", 100, 20, 10, 100);
        unwrap_ok(adapter.put_knowledge_entry(entry, None).await, "create");

        // Stage with a module_id that looks like path traversal.
        let mut state = Map::new();
        state.insert(
            "module_id".to_string(),
            Value::String("../etc/passwd".to_string()),
        );
        unwrap_ok(
            adapter
                .project(ProjectRequest {
                    session_id: "ses_pathmod".to_string(),
                    entry_id: "kb_pathmod".to_string(),
                    state,
                    extensions: HashMap::default(),
                })
                .await,
            "project",
        );

        match adapter
            .compute(ComputeRequest {
                session_id: "ses_pathmod".to_string(),
                entry_id: "kb_pathmod".to_string(),
                computable: Map::new(),
                settle: None,
                extensions: HashMap::default(),
            })
            .await
        {
            SpokeResult::Reject(r) => {
                assert_eq!(
                    r.code,
                    SpokeRejectCode::InvalidInput,
                    "path-traversal module_id must be InvalidInput"
                );
                assert!(r.message.contains("unknown embedded WASM module"));
            }
            SpokeResult::Ok(_) => panic!("expected InvalidInput for path-traversal module_id"),
        }
    }

    /// Verify the blanket `ComputablePorts` impl compiles.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn nexus_adapter_satisfies_computable_ports_blanket_impl() {
        fn accepts_computable_ports(_: &dyn crate::ComputablePorts) {}
        fn accepts_computable_port(_: &dyn crate::ComputablePort) {}

        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;
        let adapter = NexusAdapter::new(pool);

        accepts_computable_port(&adapter);
        accepts_computable_ports(&adapter);
    }
}
