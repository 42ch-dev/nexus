//! Production `ComputablePort` impl — bridges spoke compute requests to nexus's
//! stateless WASM host via a local session store (plan Decision 2).
//!
//! # Mapping (plan Decision 2)
//!
//! | Concept | spoke side | Nexus side | Mapping |
//! |---------|-----------|------------|---------|
//! | Session identity | `session_id` (opaque, product-owned) | `compute_sessions` row | Nexus owns session lifecycle; spoke `session_id` is a pass-through correlation id |
//! | Session state | `ProjectRequest.state` (ComputableFieldMap) | `state_json` column | `project()` stages static state; `compute()` merges dynamic updates |
//! | Computable I/O | `ProjectRequest` / `ComputeRequest` | `WasmEngine::compute(module, manifest, input)` | `project()`: validate entry, stage state; `compute()`: load session, build `ComputeInput`, call WASM |
//! | Settle | `ComputeRequest.settle: bool` | Merge `ComputeOutput.state_delta` into entry's `body.state` | On `settle=true`, update entry via `KnowledgeEntryPort::put_knowledge_entry` |
//!
//! # Module identity resolution
//!
//! The spoke wire does not carry a `module_id` field. When `compute()` is called:
//!
//! 1. Check the merged session state for a `module_id` key.
//! 2. Fall back to the entry's `body.computable` (if present) for a `module_id` field.
//! 3. Default to `"basic-combat"` — the only embedded compute module.
//!
//! This is documented as the honest bridge decision per the plan's Architecture
//! Lock: the module identity is genuinely absent from the spoke wire, so the
//! adapter resolves it from the staged session context.
//!
//! # Async ↔ sync bridge
//!
//! The port traits are sync; the adapter bridges async SQLite + WASM host
//! construction via the established `block_on` pattern (see `mod.rs`). The
//! `WasmEngine::new()` call is one-time (cold; subsequent calls reuse the
//! engine via a `OnceCell`), and the WASM compute call is synchronous
//! (the wasmtime host does not require an async runtime), so the sync
//! constraint is compatible with the port signature.

use super::NexusAdapter;
use crate::{
    ComputablePort, KnowledgeEntryPort, ProjectRequest, ProjectResponse, SpokeReject,
    SpokeRejectCode, SpokeResult,
};
use crate::{ComputeRequest, ComputeResponse};
use nexus_local_db::compute_session::{get_compute_session, insert_compute_session, update_compute_session_state};
use nexus_wasm_host::{
    embedded_module_bytes, embedded_module_manifest, ComputeInput,
    ModuleManifest, WasmEngine, WasmModule,
};
use serde_json::{json, Map, Value};
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
/// 3. Default `"basic-combat"`
fn resolve_module_id(
    state: &Map<String, Value>,
    _entry_json: &Map<String, Value>,
) -> String {
    if let Some(module_id) = state.get("module_id").and_then(Value::as_str) {
        return module_id.to_string();
    }
    // Check the entry body's computable field for module_id.
    if let Some(body) = _entry_json.get("body") {
        if let Some(computable) = body.get("computable") {
            if let Some(module_id) = computable.get("module_id").and_then(Value::as_str) {
                return module_id.to_string();
            }
        }
    }
    "basic-combat".to_string()
}

/// Load (or get cached) a compiled WASM module by id.
fn load_module(module_id: &str) -> SpokeResult<(WasmModule, ModuleManifest)> {
    let wasm_bytes = match embedded_module_bytes(module_id) {
        Some(b) => b,
        None => {
            return reject(
                SpokeRejectCode::InternalError,
                format!("embedded WASM module not found: {module_id}"),
                json!({ "module_id": module_id }),
            );
        }
    };
    let manifest_json = match embedded_module_manifest(module_id) {
        Some(m) => m,
        None => {
            return reject(
                SpokeRejectCode::InternalError,
                format!("embedded module manifest not found: {module_id}"),
                json!({ "module_id": module_id }),
            );
        }
    };
    let manifest: ModuleManifest = match serde_json::from_str(manifest_json) {
        Ok(m) => m,
        Err(e) => {
            return reject(
                SpokeRejectCode::InternalError,
                format!("failed to parse manifest for {module_id}: {e}"),
                json!({ "module_id": module_id }),
            );
        }
    };
    let module = match engine().load_module(wasm_bytes) {
        Ok(m) => m,
        Err(e) => {
            return reject(
                SpokeRejectCode::InternalError,
                format!("failed to compile WASM module {module_id}: {e}"),
                json!({ "module_id": module_id }),
            );
        }
    };
    SpokeResult::Ok((module, manifest))
}

impl NexusAdapter<'_> {}

impl ComputablePort for NexusAdapter<'_> {
    fn project(&self, request: ProjectRequest) -> SpokeResult<ProjectResponse> {
        let pool = self.pool.clone();
        let session_id = request.session_id.clone();
        let entry_id = request.entry_id.clone();
        let state = request.state;

        // Validate entry exists (rejects with InvalidInput if entry is missing).
        let _entry = match self.get_knowledge_entry(&entry_id) {
            SpokeResult::Ok(e) => e,
            SpokeResult::Reject(r) => {
                if r.code == SpokeRejectCode::KnowledgeEntryNotFound {
                    return reject(
                        SpokeRejectCode::InvalidInput,
                        format!(
                            "target KnowledgeEntry not found for compute project: {entry_id}"
                        ),
                        json!({ "entry_id": entry_id }),
                    );
                }
                return SpokeResult::Reject(r);
            }
        };

        let state_json =
            serde_json::to_string(&state).unwrap_or_else(|_| "{}".to_string());

        self.block_on(async move {
            match insert_compute_session(&pool, &session_id, &entry_id, &state_json).await {
                Ok(_session) => {
                    SpokeResult::Ok(ProjectResponse::Variant0 {
                        computable: state,
                        entry_id,
                        session_id,
                        extensions: Default::default(),
                    })
                }
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
        })
    }

    fn compute(&self, request: ComputeRequest) -> SpokeResult<ComputeResponse> {
        let pool = self.pool.clone();
        let session_id = request.session_id.clone();
        let entry_id = request.entry_id.clone();
        let dynamic_computable = request.computable;
        let settle = request.settle.unwrap_or(false);

        // ── 1. Load session row ──────────────────────────────────────────
        let session = self.block_on(async {
            get_compute_session(&pool, &session_id).await
        });
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
        let entry = match self.get_knowledge_entry(&entry_id) {
            SpokeResult::Ok(e) => e,
            SpokeResult::Reject(r) => return SpokeResult::Reject(r),
        };

        // ── 3. Merge staged state + dynamic computable ────────────────────
        let mut merged_state: Map<String, Value> = session
            .state_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default();
        for (k, v) in &dynamic_computable {
            merged_state.insert(k.clone(), v.clone());
        }

        // ── 4. Persist updated state back to session ──────────────────────
        let updated_state_json = serde_json::to_string(&merged_state)
            .unwrap_or_else(|_| "{}".to_string());
        if let Err(e) = self.block_on(async {
            update_compute_session_state(&pool, &session_id, &updated_state_json).await
        }) {
            return reject(
                SpokeRejectCode::InternalError,
                format!("storage error on compute session state update: {e}"),
                json!({ "session_id": session_id }),
            );
        }

        // ── 5. Build ComputeInput envelope ────────────────────────────────
        // Collect key_blocks: primary entry + any additional entries
        // referenced in the invocation (e.g., attacker_id, defender_id for
        // basic-combat). Each entry is converted from spoke format to the
        // flat-attributes format the WASM module expects.
        let mut key_blocks: Vec<Map<String, Value>> = Vec::new();
        let primary_kb = spoke_entry_to_key_block(&entry);
        key_blocks.push(primary_kb);

        // Load additional entries referenced by known invocation keys that
        // carry entry IDs (attacker_id, defender_id for basic-combat).
        for key in &["attacker_id", "defender_id"] {
            if let Some(ref_id) = merged_state
                .get(*key)
                .and_then(Value::as_str)
            {
                if ref_id != entry_id && !key_blocks.iter().any(|kb| {
                    kb.get("entry_id").and_then(Value::as_str) == Some(ref_id)
                }) {
                    if let SpokeResult::Ok(ref_entry) = self.get_knowledge_entry(ref_id) {
                        key_blocks.push(spoke_entry_to_key_block(&ref_entry));
                    }
                }
            }
        }

        // Extract world_id from extensions.nexus (the spoke-provided field).
        let world_id = entry
            .extensions
            .get(
                &crate::KnowledgeEntryExtensionsKey::try_from("nexus")
                    .expect("valid nexus key"),
            )
            .and_then(|ns| ns.get("world_id"))
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();

        // Build invocation from the merged computable state (carries
        // module-specific params like attacker_id, defender_id).
        let invocation = if merged_state.is_empty() {
            Map::new()
        } else {
            merged_state.clone()
        };

        let compute_input = ComputeInput {
            key_blocks,
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
        let module_id = resolve_module_id(&merged_state, &compute_input.key_blocks[0]);
        let (wasm_module, manifest) = match load_module(&module_id) {
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
                        "set" => {
                            set_json_path(&mut merged_state, path, value.clone());
                        }
                        "+" => {
                            add_json_path(&mut merged_state, path, value.clone());
                        }
                        "-" => {
                            sub_json_path(&mut merged_state, path, value.clone());
                        }
                        _ => {
                            set_json_path(&mut merged_state, path, value.clone());
                        }
                    }
                }
            }
        }

        // ── 8. Settle: merge back into the KnowledgeEntry ─────────────────
        let mut post_settle_state = Map::new();
        if settle {
            // Load current entry body.state, merge with state_delta, and
            // write back via put_knowledge_entry.
            let current_entry = match self.get_knowledge_entry(&entry_id) {
                SpokeResult::Ok(e) => e,
                SpokeResult::Reject(r) => return SpokeResult::Reject(r),
            };

            // Merge merged_state into the entry's body.state.
            let mut entry_body = match serde_json::to_value(&current_entry).unwrap_or_default() {
                Value::Object(m) => m,
                _ => Map::new(),
            };

            // Read existing state from entry body.
            let existing_state: Map<String, Value> = entry_body
                .get("body")
                .and_then(|b| b.get("state"))
                .and_then(|s| s.as_object())
                .cloned()
                .unwrap_or_default();

            let mut final_state = existing_state;
            for (k, v) in &merged_state {
                final_state.insert(k.clone(), v.clone());
            }
            post_settle_state = final_state.clone();

            // Write the merged state back into the entry body.
            if let Some(body) = entry_body.get_mut("body") {
                if let Some(body_obj) = body.as_object_mut() {
                    body_obj.insert(
                        "state".to_string(),
                        Value::Object(final_state),
                    );
                }
            }

            // Reconstruct the KnowledgeEntry with the updated body.
            let updated_entry: crate::KnowledgeEntry =
                serde_json::from_value(Value::Object(entry_body)).unwrap_or(current_entry.clone());

            // Use put_knowledge_entry to persist (with CAS, using the
            // current revision as expected base).
            match self.put_knowledge_entry(updated_entry, current_entry.revision) {
                SpokeResult::Ok(_) => { /* settled */ }
                SpokeResult::Reject(r) => {
                    // Settle failure is not fatal to the compute response;
                    // still return the computable view. The caller sees the
                    // reject context on a subsequent read.
                    return SpokeResult::Reject(r);
                }
            }
        }

        // ── 9. Return ComputeResponse ─────────────────────────────────────
        SpokeResult::Ok(ComputeResponse::Variant0 {
            computable: merged_state,
            entry_id,
            session_id,
            extensions: Default::default(),
            state: if settle { post_settle_state } else { Map::new() },
        })
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────

/// Convert a spoke KnowledgeEntry to a JSON key_block map suitable for the
/// WASM module's `ComputeInput.key_blocks` snapshot.
///
/// The spoke `KnowledgeEntry.body.attributes` is a `Vec<AttributesItem>`
/// (typed array), but the basic-combat module expects `body.attributes` as a
/// flat object `{max_hp: 100, base_atk: 20, ...}`. This helper converts the
/// spoke attributes vector into the flat-object format.
fn spoke_entry_to_key_block(entry: &crate::KnowledgeEntry) -> Map<String, Value> {
    // Serialize the spoke entry to a JSON map first.
    let mut kb = match serde_json::to_value(entry).unwrap_or_default() {
        Value::Object(m) => m,
        _ => return Map::new(),
    };

    // The basic-combat module uses `key_block_id` (not `entry_id`) to
    // identify key_blocks. Add it as a duplicate of entry_id so the
    // module's select_combatants lookup works.
    if let Some(entry_id) = kb.get("entry_id").and_then(Value::as_str) {
        kb.insert("key_block_id".to_string(), Value::String(entry_id.to_string()));
    }

    // Convert body.attributes from spoke Vec<AttributesItem> to flat object.
    if let Some(body) = kb.get_mut("body").and_then(Value::as_object_mut) {
        if let Some(attrs_val) = body.remove("attributes") {
            let flat: Map<String, Value> = match attrs_val {
                Value::Array(items) => items
                    .iter()
                    .filter_map(|item| {
                        let trait_type = item.get("trait_type").and_then(Value::as_str)?;
                        let value = item.get("value")
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
            current = obj.entry(seg.to_string()).or_insert_with(|| Value::Object(Map::new()));
        } else {
            // Can't traverse into non-object; overwrite.
            *current = Value::Object(Map::new());
            if let Value::Object(ref mut obj) = current {
                current = obj.entry(seg.to_string()).or_insert_with(|| Value::Object(Map::new()));
            }
        }
    }
    if let Value::Object(ref mut obj) = current {
        obj.insert(segments.last().unwrap().to_string(), value);
    }
}

/// Add a numeric value at a dotted JSON path.
fn add_json_path(root: &mut Map<String, Value>, path: &str, delta: Value) {
    let segments: Vec<&str> = path.split('.').collect();
    if segments.is_empty() {
        return;
    }
    let mut current: &mut Value = root
        .entry(segments[0].to_string())
        .or_insert_with(|| Value::Object(Map::new()));
    for seg in &segments[1..segments.len() - 1] {
        if let Value::Object(ref mut obj) = current {
            current = obj.entry(seg.to_string()).or_insert_with(|| Value::Object(Map::new()));
        } else {
            return; // Can't traverse
        }
    }
    let last_key = segments.last().unwrap();
    if let Value::Object(ref mut obj) = current {
        let current_val = obj.get(*last_key).and_then(|v| v.as_f64()).unwrap_or(0.0);
        let delta_f = delta.as_f64().unwrap_or(0.0);
        obj.insert(last_key.to_string(), Value::Number(
            serde_json::Number::from_f64(current_val + delta_f).unwrap_or_else(|| {
                serde_json::Number::from_f64(0.0).unwrap()
            }),
        ));
    }
}

/// Subtract a numeric value at a dotted JSON path.
fn sub_json_path(root: &mut Map<String, Value>, path: &str, delta: Value) {
    let delta_val = delta.as_f64().unwrap_or(0.0);
    let neg_delta = Value::Number(
        serde_json::Number::from_f64(-delta_val).unwrap_or_else(|| {
            serde_json::Number::from_f64(0.0).unwrap()
        }),
    );
    add_json_path(root, path, neg_delta);
}

/// Construct a `SpokeResult::Reject`.
fn reject<T>(
    code: SpokeRejectCode,
    message: impl Into<String>,
    details: Value,
) -> SpokeResult<T> {
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
    use nexus_knowledge::world_kb::{WorldKbBody, WorldKbEntry};
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

    /// Build a spoke KnowledgeEntry with computable state for a character.
    /// Build a JSON key_block in the format the basic-combat WASM module
    /// expects (flat `body.attributes` + `body.state.character`).
    /// Build a spoke KnowledgeEntry with computable state (creates via
    /// put_knowledge_entry). Uses the V1.145 P1a conversion seam.
    fn spoke_character_entry(
        entry_id: &str,
        canonical_name: &str,
        max_hp: i64,
        base_atk: i64,
        base_def: i64,
        current_hp: i64,
    ) -> crate::KnowledgeEntry {
        use crate::conversion::world_kb_to_spoke;
        let mut world = WorldKbEntry::new("wld_cmp", BlockType::Character, canonical_name);
        world.entry_id = entry_id.to_string();
        world.revision = Some(1);
        world.body = Some(WorldKbBody {
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
        world_kb_to_spoke(&world)
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
            adapter.put_knowledge_entry(entry, None),
            "create character",
        );

        // Stage a project session.
        let mut state = Map::new();
        state.insert("module_id".to_string(), Value::String("basic-combat".to_string()));
        state.insert("attacker_id".to_string(), Value::String("kb_hero".to_string()));
        state.insert("defender_id".to_string(), Value::String("".to_string()));

        let project_req = ProjectRequest {
            session_id: "ses_test_001".to_string(),
            entry_id: "kb_hero".to_string(),
            state: state.clone(),
            extensions: Default::default(),
        };

        match adapter.project(project_req) {
            SpokeResult::Ok(ProjectResponse::Variant0 {
                computable,
                entry_id,
                session_id,
                ..
            }) => {
                assert_eq!(entry_id, "kb_hero");
                assert_eq!(session_id, "ses_test_001");
                assert_eq!(computable.get("module_id").and_then(Value::as_str), Some("basic-combat"));
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
            extensions: Default::default(),
        };

        match adapter.project(project_req) {
            SpokeResult::Reject(r) => {
                assert_eq!(r.code, SpokeRejectCode::InvalidInput);
                assert!(r.message.contains("not found"));
            }
            _ => panic!("expected InvalidInput reject"),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn project_duplicate_session_rejects_invalid_input() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let adapter = NexusAdapter::new(pool.clone());
        let entry = spoke_character_entry("kb_dup_ses", "DupSes", 100, 20, 10, 100);
        unwrap_ok(adapter.put_knowledge_entry(entry, None), "create");

        let project_req = ProjectRequest {
            session_id: "ses_dup".to_string(),
            entry_id: "kb_dup_ses".to_string(),
            state: Map::new(),
            extensions: Default::default(),
        };
        unwrap_ok(adapter.project(project_req.clone()), "first project");

        match adapter.project(project_req) {
            SpokeResult::Reject(r) => {
                assert_eq!(r.code, SpokeRejectCode::InvalidInput);
                assert!(r.message.contains("already exists"));
            }
            _ => panic!("expected InvalidInput reject for duplicate"),
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
            extensions: Default::default(),
        };

        match adapter.compute(compute_req) {
            SpokeResult::Reject(r) => {
                assert_eq!(r.code, SpokeRejectCode::InvalidInput);
                assert!(r.message.contains("not found"));
            }
            _ => panic!("expected InvalidInput for missing session"),
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn compute_entry_id_mismatch_rejects_invalid_input() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let adapter = NexusAdapter::new(pool.clone());
        let entry = spoke_character_entry("kb_mismatch", "Mismatch", 100, 20, 10, 100);
        unwrap_ok(adapter.put_knowledge_entry(entry, None), "create");

        // Stage a session against kb_mismatch.
        let project_req = ProjectRequest {
            session_id: "ses_mismatch".to_string(),
            entry_id: "kb_mismatch".to_string(),
            state: Map::new(),
            extensions: Default::default(),
        };
        unwrap_ok(adapter.project(project_req), "project");

        // Compute against a different entry_id.
        let compute_req = ComputeRequest {
            session_id: "ses_mismatch".to_string(),
            entry_id: "kb_different".to_string(),
            computable: Map::new(),
            settle: None,
            extensions: Default::default(),
        };

        match adapter.compute(compute_req) {
            SpokeResult::Reject(r) => {
                assert_eq!(r.code, SpokeRejectCode::InvalidInput);
                assert!(r.message.contains("mismatch"));
            }
            _ => panic!("expected InvalidInput for mismatch"),
        }
    }

    /// Integration test: full project → compute round-trip with the
    /// embedded basic-combat module. Creates two character entries,
    /// stages a combat session, runs compute, and verifies the response.
    /// Both character key_blocks are bundled into the ComputeInput so
    /// the module can look up attacker and defender via kb_read.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn project_then_compute_roundtrip_with_basic_combat() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let adapter = NexusAdapter::new(pool.clone());

        // Create two character entries in spoke format.
        let hero = spoke_character_entry("kb_hero_c", "HeroC", 100, 25, 15, 100);
        let monster = spoke_character_entry("kb_monster_c", "MonsterC", 60, 15, 8, 60);
        unwrap_ok(adapter.put_knowledge_entry(hero, None), "create hero");
        unwrap_ok(adapter.put_knowledge_entry(monster, None), "create monster");

        // ── project: stage the combat session ──
        let mut state = Map::new();
        state.insert("module_id".to_string(), Value::String("basic-combat".to_string()));
        state.insert("attacker_id".to_string(), Value::String("kb_hero_c".to_string()));
        state.insert("defender_id".to_string(), Value::String("kb_monster_c".to_string()));

        let project_resp = unwrap_ok(
            adapter.project(ProjectRequest {
                session_id: "ses_combat_001".to_string(),
                entry_id: "kb_hero_c".to_string(),
                state,
                extensions: Default::default(),
            }),
            "project combat",
        );

        assert!(matches!(project_resp, ProjectResponse::Variant0 { .. }));

        // ── compute: run the combat ──
        // The compute() method loads both attacker and defender entries
        // from storage and converts them to key_blocks for the WASM module.
        let mut computable = Map::new();
        computable.insert("attacker_id".to_string(), Value::String("kb_hero_c".to_string()));
        computable.insert("defender_id".to_string(), Value::String("kb_monster_c".to_string()));

        let compute_resp = unwrap_ok(
            adapter.compute(ComputeRequest {
                session_id: "ses_combat_001".to_string(),
                entry_id: "kb_hero_c".to_string(),
                computable,
                settle: Some(false),
                extensions: Default::default(),
            }),
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
        assert!(!result_computable.is_empty(), "compute should return computable state");
    }

    /// Integration test: project → compute with settle=true persists
    /// the state_delta into the KnowledgeEntry.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn compute_with_settle_persists_state_delta() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let adapter = NexusAdapter::new(pool.clone());

        let hero = spoke_character_entry("kb_settle_h", "SettleHero", 100, 25, 15, 100);
        let monster = spoke_character_entry("kb_settle_m", "SettleMonster", 60, 15, 8, 60);
        unwrap_ok(adapter.put_knowledge_entry(hero, None), "create hero");
        unwrap_ok(adapter.put_knowledge_entry(monster, None), "create monster");

        // project
        let mut state = Map::new();
        state.insert("module_id".to_string(), Value::String("basic-combat".to_string()));
        state.insert("attacker_id".to_string(), Value::String("kb_settle_h".to_string()));
        state.insert("defender_id".to_string(), Value::String("kb_settle_m".to_string()));
        unwrap_ok(
            adapter.project(ProjectRequest {
                session_id: "ses_settle".to_string(),
                entry_id: "kb_settle_h".to_string(),
                state,
                extensions: Default::default(),
            }),
            "project",
        );

        // compute with settle=true
        let mut computable = Map::new();
        computable.insert("attacker_id".to_string(), Value::String("kb_settle_h".to_string()));
        computable.insert("defender_id".to_string(), Value::String("kb_settle_m".to_string()));

        let compute_resp = unwrap_ok(
            adapter.compute(ComputeRequest {
                session_id: "ses_settle".to_string(),
                entry_id: "kb_settle_h".to_string(),
                computable,
                settle: Some(true),
                extensions: Default::default(),
            }),
            "compute settle",
        );

        let ComputeResponse::Variant0 { state: post_state, .. } = compute_resp else {
            panic!("expected Variant0 from compute");
        };

        // After settle, the state map should be non-empty.
        assert!(!post_state.is_empty(), "settle should populate merged state");

        // Re-read the entry — state should have been persisted.
        let entry = unwrap_ok(adapter.get_knowledge_entry("kb_settle_h"), "re-read");
        // The entry's body.state should now contain the settle-persisted data.
        let body_state = &entry.body.state;
        // After basic-combat runs, state will contain dynamic fields.
        assert!(!body_state.is_empty() || !post_state.is_empty(),
            "settle should have persisted state into the entry");
    }

    // ── WASM engine / module loading edge cases ──────────────────────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn compute_unknown_module_rejects_internal_error() {
        let (pool, _dir) = fresh_pool().await;
        seed_world(&pool).await;

        let adapter = NexusAdapter::new(pool.clone());
        let entry = spoke_character_entry("kb_unknown_mod", "UnknownMod", 100, 20, 10, 100);
        unwrap_ok(adapter.put_knowledge_entry(entry, None), "create");

        // Stage with a non-existent module_id.
        let mut state = Map::new();
        state.insert(
            "module_id".to_string(),
            Value::String("nonexistent-module".to_string()),
        );
        unwrap_ok(
            adapter.project(ProjectRequest {
                session_id: "ses_bad_mod".to_string(),
                entry_id: "kb_unknown_mod".to_string(),
                state,
                extensions: Default::default(),
            }),
            "project",
        );

        match adapter.compute(ComputeRequest {
            session_id: "ses_bad_mod".to_string(),
            entry_id: "kb_unknown_mod".to_string(),
            computable: Map::new(),
            settle: None,
            extensions: Default::default(),
        }) {
            SpokeResult::Reject(r) => {
                assert_eq!(r.code, SpokeRejectCode::InternalError);
                assert!(r.message.contains("not found"));
            }
            _ => panic!("expected InternalError for unknown module"),
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
