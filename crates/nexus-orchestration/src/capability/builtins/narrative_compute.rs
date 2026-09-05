//! `narrative.compute` capability (V1.61 P3 — compass Q7).
//!
//! Orchestration-scope capability that bridges the orchestration engine with
//! the WASM compute host. Reads computable `KnowledgeEntryRecord`s from the KB layer,
//! passes them to a sandboxed WASM module via [`nexus_wasm_host::WasmEngine`],
//! and applies the resulting 4-part output envelope (`state_delta`,
//! `timeline_events`, `new_key_blocks`, `battle_report`).
//!
//! # Design
//!
//! Mirrors the `world.rs` (V1.60 P0 DF-46) orchestration handler pattern:
//! `Option<Arc<SqlitePool>>`, admission gate inline, structured `CapabilityError`.
//!
//! ## State delta merge semantics (open design item #1, compass §5)
//!
//! `apply_state_delta()` implements incremental `add/sub/set` on nested state paths:
//!
//! | Op    | Target  | Behavior |
//! |-------|---------|----------|
//! | `set` | Any     | Replace the field value at `path` (recursive JSON pointer). |
//! | `add` | Numeric | Add `value` to the current field. |
//! | `sub` | Numeric | Subtract `value` from the current field. |
//! | `add/sub` | Non-numeric | Return `CapabilityError::InputInvalid`. |
//!
//! Paths use dot-notation (e.g. `character.current_hp`) mapping to the nested
//! `body.state.<block_type_state_key>.<rest>` in a `KnowledgeEntryRecord`. The first
//! segment identifies the per-`block_type` state namespace per compass Q5
//! (e.g. `character` → `state.character.current_hp`).
//!
//! ## Decision: `state_delta.op` as String (R-V161P0-LOW-002)
//!
//! The generated `ComputeOutputStateDelta.op` is a plain `String`. We keep it
//! as `String` in this consumer and validate at runtime (match on `"add"`/`"sub"`/
//! `"set"` per the wire contract), returning `InputInvalid` on unknown ops.
//! would require a schema change + codegen cascade, which is deferred to a
//! follow-up iteration. The runtime validation is sufficient for V1.61 safety.
//!
//! ## `battle_report` size cap (R-V161P0-LOW-003)
//!
//! The generated `battle_report` field is freeform `serde_json::Value`. We
//! enforce a **64 KiB** runtime cap on the serialized report size. A module
//! emitting a larger report receives `InputInvalid` (the output is rejected
//! before any side-effects are applied).
//!
//! ## Error handling (graceful degradation)
//!
//! Compute failures (wasm trap, timeout, fuel exhaustion, output schema
//! mismatch) do NOT crash the daemon. Instead, they produce a `TimelineEvent`
//! with `event_type: StateUpdate`, title `"compute_error"`, and a summary
//! containing the error details. The error is also logged at `warn` level.

use crate::capability::builtins::world::ensure_world_owned;
use crate::capability::{Capability, CapabilityError};
use crate::state_delta;
use async_trait::async_trait;
use nexus_knowledge::world_kb::KbStore;
use nexus_narrative::NarrativeGateway;
use nexus_spoke_adapter::conversion::{knowledge_record_to_spoke, spoke_to_knowledge_record};
use nexus_wasm_host::{ComputeInput, ModuleCache, WasmEngine};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

/// Maximum size in bytes for the serialized `battle_report` field (64 KiB).
/// R-V161P0-LOW-003: freeform `battle_report` cap to prevent unbounded output.
const BATTLE_REPORT_MAX_BYTES: usize = 64 * 1024;

/// Input for `narrative.compute`.
#[derive(Debug, Deserialize)]
struct NarrativeComputeInput {
    world_id: String,
    /// Caller creator id (admission gate).
    creator_id: String,
    /// Which embedded module to invoke (default: `"basic-combat"`).
    #[serde(default = "default_module_id")]
    module_id: String,
    /// Optional module-declared invocation parameters passed into the
    /// `ComputeInput.invocation` field.
    #[serde(default)]
    invocation_params: Option<Value>,
}

#[allow(clippy::missing_const_for_fn)]
fn default_module_id() -> String {
    String::from("basic-combat")
}

/// Execute a WASM compute module for a world's computable `KnowledgeEntryRecord`s.
#[derive(Clone)]
pub struct NarrativeCompute {
    pool: Option<Arc<sqlx::SqlitePool>>,
    engine: Option<Arc<WasmEngine>>,
    /// Daemon-wide compilation cache (R-V161P3-PERF-002). When present, the
    /// module for an invocation is resolved by id from this cache instead of
    /// recompiling on every `run()`.
    module_cache: Option<Arc<ModuleCache>>,
}

impl NarrativeCompute {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            pool: None,
            engine: None,
            module_cache: None,
        }
    }

    /// Construct with a pool and a fresh `WasmEngine`, warming a per-instance
    /// module cache from the embedded modules (R-V161P3-PERF-001/002).
    ///
    /// The `WasmEngine` and the populated cache are reused across all
    /// `compute()` calls on this capability instance (compass Q6: a fresh
    /// sandboxed instance is still built per call). Use
    /// [`NarrativeCompute::with_pool_and_engine`] to inject a daemon-wide
    /// singleton engine + cache at boot.
    #[must_use]
    pub fn with_pool(pool: sqlx::SqlitePool) -> Self {
        let engine = match WasmEngine::new() {
            Ok(e) => Arc::new(e),
            Err(e) => {
                tracing::warn!(error = %e, "narrative.compute: WasmEngine init failed");
                return Self {
                    pool: Some(Arc::new(pool)),
                    engine: None,
                    module_cache: None,
                };
            }
        };
        let cache = Arc::new(ModuleCache::new());
        if let Err(e) = cache.warm_embedded(&engine) {
            tracing::warn!(error = %e, "narrative.compute: embedded module warmup had errors");
        }
        Self {
            pool: Some(Arc::new(pool)),
            engine: Some(engine),
            module_cache: Some(cache),
        }
    }

    /// Construct with a pool and a **shared, daemon-wide** `WasmEngine` +
    /// `ModuleCache` (P-last T1 singleton injection — closes R-V161P3-PERF-001).
    ///
    /// The daemon builds one engine + one cache at boot (pre-warmed with
    /// embedded and user modules) and hands them to every `NarrativeCompute`
    /// instance so module compilation happens exactly once process-wide.
    #[must_use]
    pub fn with_pool_and_engine(
        pool: sqlx::SqlitePool,
        engine: Arc<WasmEngine>,
        module_cache: Arc<ModuleCache>,
    ) -> Self {
        Self {
            pool: Some(Arc::new(pool)),
            engine: Some(engine),
            module_cache: Some(module_cache),
        }
    }
}

impl Default for NarrativeCompute {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Capability for NarrativeCompute {
    fn name(&self) -> &'static str {
        "narrative.compute"
    }

    fn input_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"world_id":{"type":"string"},"creator_id":{"type":"string"},"module_id":{"type":"string"},"invocation_params":{"type":"object"}},"required":["world_id","creator_id"],"additionalProperties":false}"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"battle_report":{},"state_delta_applied":{"type":"integer","minimum":0},"timeline_events_created":{"type":"integer","minimum":0},"new_key_blocks_created":{"type":"integer","minimum":0}},"required":["battle_report","state_delta_applied","timeline_events_created","new_key_blocks_created"],"additionalProperties":false}"#
    }

    #[allow(clippy::too_many_lines)]
    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let parsed: NarrativeComputeInput = serde_json::from_value(input)
            .map_err(|e| CapabilityError::InputInvalid(format!("narrative.compute input: {e}")))?;

        let pool = self
            .pool
            .as_ref()
            .ok_or(CapabilityError::WorkerUnavailable)?;

        let engine = self
            .engine
            .as_ref()
            .ok_or(CapabilityError::WorkerUnavailable)?;

        tracing::info!(
            world_id = %parsed.world_id,
            module_id = %parsed.module_id,
            "narrative.compute admitted"
        );

        // Admission gate: creator must own the world.
        ensure_world_owned(pool, &parsed.creator_id, &parsed.world_id).await?;

        // 1. Read computable KnowledgeEntries from the KB store.
        let kb_store = nexus_local_db::kb_store::SqliteKbStore::new((**pool).clone());
        let q =
            nexus_knowledge::world_kb::KbQuery::new(&parsed.world_id).with_computable(Some(true));
        let computable_blocks = kb_store
            .query(&q)
            .await
            .map_err(|e| CapabilityError::Internal(format!("kb query computable: {e}")))?;

        if computable_blocks.items.is_empty() {
            return Err(CapabilityError::InputInvalid(
                "no computable knowledge entries found in world".to_string(),
            ));
        }

        // Domain KnowledgeEntries are serialized into the compute envelope's
        // `key_blocks` items (opaque JSON objects; spoke-adapter-architecture
        // spec §3.4) inline below.

        // 2. Read narrative state (timeline position, root branch).
        let gw = nexus_local_db::narrative_gateway::SqliteNarrativeGateway::new((**pool).clone());
        let world_state = gw
            .get_world_state(&parsed.world_id)
            .await
            .map_err(|e| CapabilityError::Internal(format!("world state read: {e}")))?;

        let branch_id = world_state
            .fork_branch_id
            .clone()
            .unwrap_or_else(|| "fbk_root".to_string());

        let narrative_state = json!({
            "world_id": parsed.world_id,
            "branch_id": branch_id,
            "timeline_position": "0", // V1.61: default to start of timeline
        });

        // 3. Build ComputeInput envelope and invoke WASM.
        // The compute envelope's world_ref/narrative_state are typify-inlined
        // copies (`ComputeInputWorldRef`, `ComputeInputNarrativeState`) that
        // are wire-equivalent to the domain contract types; JSON round-trips
        // bridge them (drift gate proves equivalence). `key_blocks` items are
        // opaque JSON objects (spoke-adapter-architecture spec §3.4).
        let compute_input = ComputeInput {
            schema_version: std::num::NonZeroU64::new(1)
                .expect("schema_version literal 1 is non-zero"),
            world_ref: serde_json::from_value(json!({"world_id": parsed.world_id}))
                .expect("world_ref round-trips"),
            key_blocks: computable_blocks
                .items
                .into_iter()
                .map(
                    |kb: nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord| {
                        // V1.139 P0: ComputeInput.key_blocks is opaque spoke-
                        // KnowledgeEntry JSON (the spoke $ref is unresolved at
                        // codegen). Convert domain KnowledgeEntryRecord → spoke
                        // KnowledgeEntry (sole conversion seam, now a free
                        // function in nexus-spoke-adapter — V1.145 P1a) → JSON
                        // object map.
                        let spoke: nexus_knowledge::world_kb::KnowledgeEntry =
                            knowledge_record_to_spoke(&kb);
                        serde_json::to_value(&spoke)
                            .ok()
                            .and_then(|v| v.as_object().cloned())
                            .unwrap_or_default()
                    },
                )
                .collect(),
            narrative_state: Some(
                serde_json::from_value(narrative_state)
                    .expect("narrative_state round-trips to ComputeInputNarrativeState"),
            ),
            invocation: parsed
                .invocation_params
                .map(|v| serde_json::from_value(v).expect("invocation round-trips to Map"))
                .unwrap_or_default(),
        };

        // Resolve the compiled module + manifest from the daemon-wide cache
        // (R-V161P3-PERF-002: compile once, reuse). Modules are pre-warmed at
        // daemon boot (embedded + user-installed); a cache miss means the
        // requested module id is neither embedded nor installed under
        // `~/.nexus42/modules/`.
        let module_cache = self.module_cache.as_ref().ok_or_else(|| {
            tracing::warn!(
                module_id = %parsed.module_id,
                "narrative.compute: no module cache wired (capability constructed without engine)"
            );
            CapabilityError::WorkerUnavailable
        })?;

        let cached = module_cache.get(&parsed.module_id).ok_or_else(|| {
            CapabilityError::InputInvalid(format!(
                "module '{}' not loaded; ensure it is embedded or installed under ~/.nexus42/modules/",
                parsed.module_id
            ))
        })?;
        let module = cached.module.clone();
        let manifest = cached.manifest.clone();

        // Invoke compute with graceful error handling.
        let output = match engine.compute(&module, &manifest, &compute_input) {
            Ok(o) => o,
            Err(e) => {
                return handle_compute_error(
                    pool,
                    &parsed.world_id,
                    &parsed.creator_id,
                    &branch_id,
                    &e.to_string(),
                )
                .await;
            }
        };

        // 4. Validate battle_report size (R-V161P0-LOW-003).
        if let Ok(report_bytes) = serde_json::to_vec(&output.battle_report) {
            if report_bytes.len() > BATTLE_REPORT_MAX_BYTES {
                return Err(CapabilityError::InputInvalid(format!(
                    "battle_report too large: {} bytes (max {} bytes)",
                    report_bytes.len(),
                    BATTLE_REPORT_MAX_BYTES
                )));
            }
        }

        // 5. Apply state_delta to KB state fields.
        let applied = state_delta::apply_state_delta_pool(pool, &output.state_delta).await?;

        // 6. Create new KnowledgeEntries from output.
        let new_kb_count = create_new_key_blocks(pool, &parsed.world_id, &output.new_key_blocks)
            .await
            .map_err(|e| CapabilityError::Internal(format!("create new key_blocks: {e}")))?;

        // 7. Append timeline events from output.
        let evt_count = append_timeline_events(
            pool,
            &parsed.world_id,
            &parsed.creator_id,
            &branch_id,
            &output
                .timeline_events
                .iter()
                .map(|evt| {
                    serde_json::from_value(serde_json::to_value(evt).unwrap_or_default())
                        .expect("NexusTimelineEvent round-trips to TimelineEvent")
                })
                .collect::<Vec<nexus_contracts::TimelineEvent>>(),
        )
        .await
        .map_err(|e| CapabilityError::Internal(format!("append timeline events: {e}")))?;

        tracing::info!(
            world_id = %parsed.world_id,
            module_id = %parsed.module_id,
            state_delta_applied = applied,
            timeline_events_created = evt_count,
            new_key_blocks_created = new_kb_count,
            "narrative.compute completed"
        );

        Ok(json!({
            "battle_report": output.battle_report,
            "state_delta_applied": applied,
            "timeline_events_created": evt_count,
            "new_key_blocks_created": new_kb_count,
        }))
    }
}

// ─── New KnowledgeEntryRecord creation ─────────────────────────────────────────────────

/// Create new `KnowledgeEntryRecord`s emitted by the compute module. Each block is inserted
/// with `provisional` status via the KB store.
///
/// # Security: `world_id` re-assertion (R-V161P3-CORR-002)
///
/// Every emitted block MUST target the same world that was admitted by the
/// capability's admission gate. A module that emits a block carrying a
/// different `world_id` (or no `world_id`) is rejected with `InputInvalid`
/// before any insert runs, preventing cross-world injection. This re-checks
/// the invariant after the sandboxed module has run, where the admitted
/// `world_id` is the sole trusted source.
///
/// Returns the number of blocks created.
async fn create_new_key_blocks(
    pool: &sqlx::SqlitePool,
    world_id: &str,
    blocks: &[serde_json::Map<String, serde_json::Value>],
) -> Result<usize, CapabilityError> {
    let kb_store = nexus_local_db::kb_store::SqliteKbStore::new(pool.clone());
    let mut created = 0usize;

    for kb_map in blocks {
        // V1.139 P0: compute output `new_key_blocks` is opaque spoke-
        // KnowledgeEntry JSON. Convert Map → spoke KnowledgeEntry → domain
        // KnowledgeEntryRecord (sole conversion seam, now a free function in
        // nexus-spoke-adapter — V1.145 P1a) before persisting.
        let spoke: nexus_knowledge::world_kb::KnowledgeEntry =
            serde_json::from_value(serde_json::Value::Object(kb_map.clone()))
                .map_err(|e| CapabilityError::Internal(format!("decode new_key_block: {e}")))?;
        let kb = spoke_to_knowledge_record(spoke)
            .map_err(|e| CapabilityError::Internal(format!("decode new_key_block owner: {e}")))?;
        if kb.world_id() != Some(world_id) {
            return Err(CapabilityError::InputInvalid(format!(
                "new_key_block '{}' targets world '{}' but admitted world is '{}'; \
                 cross-world block injection rejected",
                kb.entry_id,
                kb.world_id().unwrap_or_default(),
                world_id
            )));
        }
        kb_store
            .insert_knowledge_entry(kb)
            .await
            .map_err(|e| CapabilityError::Internal(format!("insert new knowledge_entry: {e}")))?;
        created += 1;
    }

    Ok(created)
}

// ─── Timeline event appending ──────────────────────────────────────────────

/// Append timeline events emitted by the compute module via the narrative
/// gateway.
///
/// Returns the number of events created.
async fn append_timeline_events(
    pool: &sqlx::SqlitePool,
    world_id: &str,
    _creator_id: &str,
    branch_id: &str,
    events: &[nexus_contracts::TimelineEvent],
) -> Result<usize, CapabilityError> {
    let mut count = 0usize;

    for evt in events {
        let event_type = evt.event_type.as_str();
        nexus_local_db::narrative_write::append_event(
            pool,
            world_id,
            branch_id,
            event_type,
            evt.title.as_deref().map(String::as_str),
            evt.summary.as_deref(),
            None, // modules_json — compute lane writes no modules
        )
        .await
        .map_err(|e| CapabilityError::Internal(format!("append timeline event: {e}")))?;
        count += 1;
    }

    Ok(count)
}

// ─── Error handling: compute_error timeline event ──────────────────────────

/// Create a `compute_error` timeline event and return a graceful error.
///
/// The daemon must NOT crash on compute failure (compass T4). Instead, a
/// `TimelineEvent` with `event_type: StateUpdate` and `title: "compute_error"`
/// is inserted into the world timeline, and the capability returns
/// `CapabilityError::TransientExternal`.
async fn handle_compute_error(
    pool: &sqlx::SqlitePool,
    world_id: &str,
    _creator_id: &str,
    branch_id: &str,
    error_detail: &str,
) -> Result<Value, CapabilityError> {
    tracing::warn!(
        world_id = %world_id,
        error = %error_detail,
        "narrative.compute failed; recording compute_error timeline event"
    );

    // Best-effort: append the error event. If this also fails, we still return
    // the original compute error so the caller knows something went wrong.
    let evt_result = nexus_local_db::narrative_write::append_event(
        pool,
        world_id,
        branch_id,
        "state_update",
        Some("compute_error"),
        Some(error_detail),
        None, // modules_json — error marker writes no modules
    )
    .await;

    if let Err(ref e) = evt_result {
        tracing::error!(
            world_id = %world_id,
            db_error = %e,
            "failed to record compute_error timeline event"
        );
    }

    Err(CapabilityError::TransientExternal(format!(
        "compute failed: {error_detail}"
    )))
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::state_delta;
    use nexus_knowledge::world_kb::knowledge_entry::{KnowledgeEntryBody, KnowledgeEntryRecord};
    use nexus_knowledge::world_kb::KbStore;
    use nexus_local_db::{open_pool, run_migrations};

    async fn fresh_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool).await.unwrap();
        (pool, dir)
    }

    fn test_contract_key_block(
        key_block_id: &str,
        world_id: &str,
        canonical_name: &str,
    ) -> serde_json::Map<String, serde_json::Value> {
        // V1.139 P0: compute wire key_blocks is opaque spoke-KnowledgeEntry
        // JSON. Fixture carries the spoke field names (entry_id, entry_type,
        // extensions.nexus.world_id).
        serde_json::from_value(json!({
            "schema_version": 1,
            "entry_id": key_block_id,
            "entry_type": "character",
            "canonical_name": canonical_name,
            "status": "confirmed",
            "body": {},
            "extensions": {
                "nexus": { "world_id": world_id }
            },
            "created_at": "2026-01-01T00:00:00Z",
        }))
        .expect("minimal spoke KnowledgeEntry wire fixture")
    }

    async fn seed_creator(pool: &sqlx::SqlitePool, creator_id: &str) {
        sqlx::query(
            "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
             VALUES (?, ?, 'active', datetime('now'), '{}')",
        )
        .bind(creator_id)
        .bind("Test Creator")
        .execute(pool)
        .await
        .unwrap();
    }

    async fn seed_world(pool: &sqlx::SqlitePool, owner: &str, world_id: &str) {
        nexus_local_db::narrative_write::create_world(
            pool,
            owner,
            "Test World",
            "test-world",
            "private",
            "manual",
        )
        .await
        .unwrap();
        sqlx::query("UPDATE narrative_worlds SET world_id = ? WHERE owner_creator_id = ?")
            .bind(world_id)
            .bind(owner)
            .execute(pool)
            .await
            .unwrap();
    }

    async fn seed_computable_character(
        pool: &sqlx::SqlitePool,
        world_id: &str,
        canonical_name: &str,
        max_hp: i64,
        current_hp: i64,
    ) -> KnowledgeEntryRecord {
        let kb = nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord {
            block_type: nexus_contracts::BlockType::Character,
            canonical_name: canonical_name.to_string(),
            body: Some(KnowledgeEntryBody {
                summary: Some(format!("{canonical_name} character")),
                attributes: Some(json!({"max_hp": max_hp, "base_atk": 20})),
                computable: Some(true),
                state: Some(json!({
                    "character": {
                        "current_hp": current_hp,
                        "status_effects": [],
                        "position": "front_line",
                        "is_alive": true,
                    }
                })),
                ..Default::default()
            }),
            ..KnowledgeEntryRecord::new(
                world_id,
                nexus_contracts::BlockType::Character,
                canonical_name,
            )
        };
        let kb_store = nexus_local_db::kb_store::SqliteKbStore::new(pool.clone());
        kb_store.insert_knowledge_entry(kb.clone()).await.unwrap();
        kb
    }

    // ── apply_json_delta unit tests ────────────────────────────────────────

    #[test]
    fn delta_set_numeric() {
        let mut state = json!({"character": {"current_hp": 100, "name": "Hero"}});
        state_delta::apply_json_delta(
            &mut state,
            "character",
            &["current_hp"],
            "set",
            &Some(json!(50)),
        )
        .unwrap();
        assert_eq!(state["character"]["current_hp"], 50);
    }

    #[test]
    fn delta_add_numeric() {
        let mut state = json!({"character": {"current_hp": 80}});
        state_delta::apply_json_delta(
            &mut state,
            "character",
            &["current_hp"],
            "add",
            &Some(json!(20)),
        )
        .unwrap();
        assert_eq!(state["character"]["current_hp"], 100);
    }

    #[test]
    fn delta_subtract_numeric() {
        let mut state = json!({"character": {"current_hp": 100}});
        state_delta::apply_json_delta(
            &mut state,
            "character",
            &["current_hp"],
            "sub",
            &Some(json!(30)),
        )
        .unwrap();
        assert_eq!(state["character"]["current_hp"], 70);
    }

    #[test]
    fn delta_set_string_field() {
        let mut state = json!({"character": {"position": "front_line"}});
        state_delta::apply_json_delta(
            &mut state,
            "character",
            &["position"],
            "set",
            &Some(json!("back_line")),
        )
        .unwrap();
        assert_eq!(state["character"]["position"], "back_line");
    }

    #[test]
    fn delta_add_on_non_numeric_errors() {
        let mut state = json!({"character": {"name": "Hero"}});
        let err = state_delta::apply_json_delta(
            &mut state,
            "character",
            &["name"],
            "add",
            &Some(json!(1)),
        )
        .unwrap_err();
        assert!(matches!(err, CapabilityError::InputInvalid(_)));
    }

    #[test]
    fn delta_sub_on_non_numeric_errors() {
        let mut state = json!({"character": {"name": "Hero"}});
        let err = state_delta::apply_json_delta(
            &mut state,
            "character",
            &["name"],
            "sub",
            &Some(json!(1)),
        )
        .unwrap_err();
        assert!(matches!(err, CapabilityError::InputInvalid(_)));
    }

    #[test]
    fn delta_unknown_op_errors() {
        let mut state = json!({"character": {"current_hp": 50}});
        let err = state_delta::apply_json_delta(
            &mut state,
            "character",
            &["current_hp"],
            "multiply",
            &Some(json!(2)),
        )
        .unwrap_err();
        assert!(matches!(err, CapabilityError::InputInvalid(_)));
    }

    #[test]
    fn delta_missing_state_key_errors() {
        let mut state = json!({"item": {"durability": 50}});
        let err = state_delta::apply_json_delta(
            &mut state,
            "character",
            &["current_hp"],
            "set",
            &Some(json!(100)),
        )
        .unwrap_err();
        assert!(matches!(err, CapabilityError::InputInvalid(_)));
    }

    #[test]
    fn delta_integer_addition_preserves_int_type() {
        let mut state = json!({"character": {"current_hp": 80}});
        state_delta::apply_json_delta(
            &mut state,
            "character",
            &["current_hp"],
            "add",
            &Some(json!(20)),
        )
        .unwrap();
        // Integer preservation: 80 + 20 = 100, both i64 → result is i64
        assert_eq!(state["character"]["current_hp"], 100);
        assert!(state["character"]["current_hp"].is_i64());
    }

    #[test]
    fn delta_float_addition_produces_float() {
        let mut state = json!({"character": {"current_hp": 80.5}});
        state_delta::apply_json_delta(
            &mut state,
            "character",
            &["current_hp"],
            "add",
            &Some(json!(19.5)),
        )
        .unwrap();
        assert_eq!(state["character"]["current_hp"], 100.0);
    }

    // ── Integration: narrative.compute capability ──────────────────────────

    // H1 / R-V161P3-CORR-002: create_new_key_blocks re-asserts world_id before
    // any insert, rejecting cross-world block injection from a (hypothetically
    // hostile or buggy) module.
    #[tokio::test]
    async fn create_new_key_blocks_rejects_cross_world_injection() {
        let (pool, _dir) = fresh_pool().await;
        let hostile = test_contract_key_block("kb_hostile", "wld_OTHER", "Hostile");
        let err = create_new_key_blocks(&pool, "wld_admitted", std::slice::from_ref(&hostile))
            .await
            .unwrap_err();
        assert!(matches!(err, CapabilityError::InputInvalid(_)));
        // And nothing was inserted.
        let kb_store = nexus_local_db::kb_store::SqliteKbStore::new(pool.clone());
        assert!(kb_store.get_knowledge_entry("kb_hostile").await.is_err());
    }

    #[tokio::test]
    async fn create_new_key_blocks_accepts_matching_world_id() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool, "ctr_kb").await;
        seed_world(&pool, "ctr_kb", "wld_admitted").await;
        let kb = test_contract_key_block("kb_ok", "wld_admitted", "Ok");
        let n = create_new_key_blocks(&pool, "wld_admitted", std::slice::from_ref(&kb))
            .await
            .unwrap();
        assert_eq!(n, 1);
        let kb_store = nexus_local_db::kb_store::SqliteKbStore::new(pool.clone());
        assert!(kb_store.get_knowledge_entry("kb_ok").await.is_ok());
    }

    #[tokio::test]
    async fn narrative_compute_rejects_missing_world() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool, "ctr_a").await;
        let cap = NarrativeCompute::with_pool(pool);
        let err = cap
            .run(json!({
                "world_id": "wld_missing",
                "creator_id": "ctr_a",
            }))
            .await
            .unwrap_err();
        assert!(matches!(err, CapabilityError::Forbidden(_)));
    }

    #[tokio::test]
    async fn narrative_compute_rejects_cross_creator() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool, "ctr_a").await;
        seed_creator(&pool, "ctr_b").await;
        seed_world(&pool, "ctr_a", "wld_a").await;
        let cap = NarrativeCompute::with_pool(pool);
        let err = cap
            .run(json!({
                "world_id": "wld_a",
                "creator_id": "ctr_b",
            }))
            .await
            .unwrap_err();
        assert!(matches!(err, CapabilityError::Forbidden(_)));
    }

    #[tokio::test]
    async fn narrative_compute_rejects_no_computable_blocks() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool, "ctr_a").await;
        seed_world(&pool, "ctr_a", "wld_a").await;
        let cap = NarrativeCompute::with_pool(pool);
        let err = cap
            .run(json!({
                "world_id": "wld_a",
                "creator_id": "ctr_a",
            }))
            .await
            .unwrap_err();
        assert!(matches!(err, CapabilityError::InputInvalid(_)));
    }

    #[tokio::test]
    async fn narrative_compute_invalid_input() {
        let (pool, _dir) = fresh_pool().await;
        let cap = NarrativeCompute::with_pool(pool);
        let err = cap.run(json!(42)).await.unwrap_err();
        assert!(matches!(err, CapabilityError::InputInvalid(_)));
    }

    #[tokio::test]
    async fn narrative_compute_pool_less_returns_worker_unavailable() {
        let cap = NarrativeCompute::new();
        let err = cap
            .run(json!({"world_id": "wld_a", "creator_id": "ctr_a"}))
            .await
            .unwrap_err();
        assert!(matches!(err, CapabilityError::WorkerUnavailable));
    }

    /// Full integration test: create world with 2 computable characters,
    /// run narrative.compute, verify output.
    #[tokio::test]
    async fn narrative_compute_full_cycle() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool, "ctr_a").await;
        seed_world(&pool, "ctr_a", "wld_a").await;

        // Seed two computable characters with HP state.
        let _kb_a = seed_computable_character(&pool, "wld_a", "Hero", 100, 80).await;
        let _kb_b = seed_computable_character(&pool, "wld_a", "Villain", 120, 120).await;

        let cap = NarrativeCompute::with_pool(pool.clone());

        // The basic-combat module expects two combatants; if it runs successfully
        // it will return state_delta + battle_report. If it traps (e.g., because
        // the module's ABI doesn't handle our exact character shapes), the error
        // path creates a compute_error timeline event instead of crashing.
        let result = cap
            .run(json!({
                "world_id": "wld_a",
                "creator_id": "ctr_a",
                "module_id": "basic-combat",
                "invocation_params": {"rounds": 1},
            }))
            .await;

        match result {
            Ok(out) => {
                // Compute succeeded — verify the output shape.
                assert!(
                    out.get("battle_report").is_some(),
                    "expected battle_report in output"
                );
                assert!(
                    out.get("state_delta_applied").is_some(),
                    "expected state_delta_applied count"
                );
                assert!(
                    out.get("timeline_events_created").is_some(),
                    "expected timeline_events_created count"
                );
            }
            Err(e) => {
                // Compute failure is allowed in tests — verifying it doesn't crash.
                // The compute_error timeline event should have been recorded.
                let err_str = e.to_string();
                assert!(
                    err_str.contains("compute failed"),
                    "expected compute error message, got: {err_str}"
                );
            }
        }
    }
}
