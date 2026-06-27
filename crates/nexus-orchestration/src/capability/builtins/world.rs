//! World-state query + world-delta propose/apply capabilities (V1.60 P0 — DF-46).
//!
//! Three orchestration-scope capabilities that operate on a world's KB and
//! metadata, gated by creator ownership of the target world:
//!
//! - `nexus.world.state.query` — joined KB + timeline snapshot for reasoning.
//! - `nexus.world.delta.propose` — produce a structured delta package (no writes).
//! - `nexus.world.delta.apply` — apply a delta package under a transaction with
//!   a lost-update guard.
//!
//! # Design
//!
//! Mirrors the `nexus.reference.refresh` (V1.58 P1) orchestration handler
//! pattern: `Option<Arc<SqlitePool>>`, admission gate inline, structured
//! `CapabilityError`. Spec: `world-delta-propose-apply.md` (Draft, V1.60 P0).
//!
//! `world.delta.apply` is **runtime-side** (resolves `acp-capability-set.md`
//! §8 Open Item line 223): the agent proposes, the runtime applies under a
//! transaction with ownership re-check.

use crate::capability::{Capability, CapabilityError};
use async_trait::async_trait;
use nexus_kb::KbStore;
use nexus_narrative::NarrativeGateway;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

// ─── Shared admission gate ─────────────────────────────────────────────────

/// Verify that `creator_id` owns `world_id`.
///
/// V1.67 P2 (R-V160P0-QC2-W001): delegates to the shared
/// [`nexus_local_db::narrative_write::is_world_owned`] gate so the ownership
/// check is no longer duplicated between the orchestration capabilities and the
/// daemon host-tool layer. `world_id` must exist AND `owner_creator_id` must
/// match the caller.
///
/// # Errors
///
/// Returns `Forbidden` when the world is missing or owned by another creator,
/// `Internal` on database errors.
pub async fn ensure_world_owned(
    pool: &sqlx::SqlitePool,
    creator_id: &str,
    world_id: &str,
) -> Result<(), CapabilityError> {
    match nexus_local_db::narrative_write::is_world_owned(pool, creator_id, world_id).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(CapabilityError::Forbidden(
            "world not found or not owned by creator".into(),
        )),
        Err(e) => Err(CapabilityError::Internal(format!(
            "world ownership check: {e}"
        ))),
    }
}

/// Resolve the workspace slug for a world (diagnostics / audit context).
///
/// Returns `"default"` when the stored `workspace_id` is the local sentinel
/// `"wrk_local"` or empty — matching the local-first single-workspace posture.
async fn workspace_slug_for(pool: &sqlx::SqlitePool, world_id: &str) -> String {
    // SAFETY: single-column SELECT against known narrative_worlds schema.
    let ws: Option<String> =
        sqlx::query_scalar("SELECT workspace_id FROM narrative_worlds WHERE world_id = ?")
            .bind(world_id)
            .fetch_optional(pool)
            .await
            .ok()
            .flatten();
    match ws.as_deref() {
        None | Some("" | "wrk_local") => "default".to_string(),
        Some(slug) => slug.to_string(),
    }
}

// ─── Input shapes ──────────────────────────────────────────────────────────

/// Input for `nexus.world.state.query`.
#[derive(Debug, Deserialize)]
struct WorldStateQueryInput {
    world_id: String,
    /// Caller creator id (admission gate).
    creator_id: String,
    /// Which slice to return. Defaults to `"all"`.
    #[serde(default)]
    slice: Option<String>,
    /// Optional branch filter for the timeline slice.
    #[serde(default)]
    branch_id: Option<String>,
    /// Optional cap on the number of timeline events returned.
    #[serde(default)]
    limit: Option<usize>,
}

/// Input for `nexus.world.delta.propose`.
#[derive(Debug, Deserialize)]
struct WorldDeltaProposeInput {
    world_id: String,
    creator_id: String,
    /// Input changeset: each entry is an intended change without `old_value`
    /// (the handler populates `old_value` from current state).
    changeset: Vec<InputChange>,
}

#[derive(Debug, Deserialize)]
struct InputChange {
    entity: String,
    #[serde(default)]
    entity_id: Option<String>,
    field: String,
    new_value: Value,
    rationale: String,
}

/// Input for `nexus.world.delta.apply` (the delta package).
#[derive(Debug, Deserialize)]
struct WorldDeltaApplyInput {
    policy_context: PolicyContext,
    proposed_changes: Vec<ProposedChange>,
    #[serde(default = "default_true")]
    atomic: bool,
}

// Justification: `*_id` suffix is the canonical schema column naming for
// entity identifiers (world_id, creator_id, source_work_id) — diverging would
// break the delta-package contract documented in world-delta-propose-apply.md.
#[allow(clippy::struct_field_names)]
#[derive(Debug, Deserialize)]
struct PolicyContext {
    world_id: String,
    creator_id: String,
    #[serde(default)]
    source_work_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ProposedChange {
    entity: String,
    #[serde(default)]
    entity_id: Option<String>,
    field: String,
    #[serde(default)]
    old_value: Option<Value>,
    new_value: Value,
    rationale: String,
}

const fn default_true() -> bool {
    true
}

// ─── Capability: nexus.world.state.query ───────────────────────────────────

/// Query KB/timeline slices for a world (read-only snapshot).
#[derive(Debug, Clone)]
pub struct WorldStateQuery {
    pool: Option<Arc<sqlx::SqlitePool>>,
}

impl WorldStateQuery {
    #[must_use]
    pub const fn new() -> Self {
        Self { pool: None }
    }

    #[must_use]
    pub fn with_pool(pool: sqlx::SqlitePool) -> Self {
        Self {
            pool: Some(Arc::new(pool)),
        }
    }
}

impl Default for WorldStateQuery {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Capability for WorldStateQuery {
    fn name(&self) -> &'static str {
        "nexus.world.state.query"
    }

    fn input_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"world_id":{"type":"string"},"creator_id":{"type":"string"},"slice":{"type":"string","enum":["kb","timeline","all"]},"branch_id":{"type":"string"},"limit":{"type":"integer","minimum":0}},"required":["world_id","creator_id"],"additionalProperties":false}"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"world_id":{"type":"string"},"world":{"type":"object"},"kb_blocks":{"type":"array"},"timeline":{"type":"array"},"generated_at":{"type":"string","format":"date-time"}},"required":["world_id","generated_at"],"additionalProperties":false}"#
    }

    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let parsed: WorldStateQueryInput = serde_json::from_value(input)
            .map_err(|e| CapabilityError::InputInvalid(format!("world.state.query input: {e}")))?;

        let pool = self
            .pool
            .as_ref()
            .ok_or(CapabilityError::WorkerUnavailable)?;

        tracing::info!(
            world_id = %parsed.world_id,
            slice = ?parsed.slice,
            "world.state.query admitted"
        );

        // Admission gate: creator must own the world.
        ensure_world_owned(pool, &parsed.creator_id, &parsed.world_id).await?;

        let slice = parsed.slice.as_deref().unwrap_or("all");
        let want_kb = matches!(slice, "kb" | "all");
        let want_timeline = matches!(slice, "timeline" | "all");

        let gw = nexus_local_db::narrative_gateway::SqliteNarrativeGateway::new((**pool).clone());

        // World metadata (always returned).
        let world = gw
            .get_world_state(&parsed.world_id)
            .await
            .map_err(|e| CapabilityError::Internal(format!("world state read: {e}")))?;
        let world_json =
            serde_json::to_value(&world).map_err(|e| CapabilityError::Internal(e.to_string()))?;

        // KB slice.
        let kb_blocks = if want_kb {
            let store = nexus_local_db::kb_store::SqliteKbStore::new((**pool).clone());
            store
                .list_by_world(&parsed.world_id)
                .await
                .map_err(|e| CapabilityError::Internal(format!("kb list: {e}")))?
                .into_iter()
                .filter_map(|kb| serde_json::to_value(&kb).ok())
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        // Timeline slice.
        let timeline = if want_timeline {
            gw.get_timeline(&parsed.world_id, parsed.branch_id.as_deref(), parsed.limit)
                .await
                .map_err(|e| CapabilityError::Internal(format!("timeline read: {e}")))?
                .into_iter()
                .filter_map(|evt| serde_json::to_value(&evt).ok())
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        Ok(json!({
            "world_id": parsed.world_id,
            "world": world_json,
            "kb_blocks": kb_blocks,
            "timeline": timeline,
            "generated_at": chrono::Utc::now().to_rfc3339(),
        }))
    }
}

// ─── Capability: nexus.world.delta.propose ─────────────────────────────────

/// Produce a structured delta package from an input changeset (no writes).
#[derive(Debug, Clone)]
pub struct WorldDeltaPropose {
    pool: Option<Arc<sqlx::SqlitePool>>,
}

impl WorldDeltaPropose {
    #[must_use]
    pub const fn new() -> Self {
        Self { pool: None }
    }

    #[must_use]
    pub fn with_pool(pool: sqlx::SqlitePool) -> Self {
        Self {
            pool: Some(Arc::new(pool)),
        }
    }
}

impl Default for WorldDeltaPropose {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Capability for WorldDeltaPropose {
    fn name(&self) -> &'static str {
        "nexus.world.delta.propose"
    }

    fn input_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"world_id":{"type":"string"},"creator_id":{"type":"string"},"changeset":{"type":"array","items":{"type":"object","properties":{"entity":{"type":"string"},"entity_id":{"type":"string"},"field":{"type":"string"},"new_value":{},"rationale":{"type":"string"}},"required":["entity","field","new_value","rationale"]}}},"required":["world_id","creator_id","changeset"],"additionalProperties":false}"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"schema_version":{"type":"integer"},"policy_context":{"type":"object"},"proposed_changes":{"type":"array"},"atomic":{"type":"boolean"}},"required":["schema_version","policy_context","proposed_changes","atomic"],"additionalProperties":false}"#
    }

    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let parsed: WorldDeltaProposeInput = serde_json::from_value(input).map_err(|e| {
            CapabilityError::InputInvalid(format!("world.delta.propose input: {e}"))
        })?;

        let pool = self
            .pool
            .as_ref()
            .ok_or(CapabilityError::WorkerUnavailable)?;

        tracing::info!(
            world_id = %parsed.world_id,
            changes = parsed.changeset.len(),
            "world.delta.propose admitted"
        );

        // Admission gate.
        ensure_world_owned(pool, &parsed.creator_id, &parsed.world_id).await?;

        let slug = workspace_slug_for(pool, &parsed.world_id).await;
        let store = nexus_local_db::kb_store::SqliteKbStore::new((**pool).clone());
        let mut proposed: Vec<Value> = Vec::with_capacity(parsed.changeset.len());
        for ch in parsed.changeset {
            // V1.60 supports kb_key_block (create/update) + world_metadata title.
            let old_value = match (ch.entity.as_str(), ch.entity_id.as_deref()) {
                ("kb_key_block", Some(kid)) => {
                    let existing = store.get_key_block(kid).await.ok();
                    existing.and_then(|kb| serde_json::to_value(field_of(&kb, &ch.field)).ok())
                }
                ("kb_key_block", None) => {
                    // Create path — no prior value.
                    Some(Value::Null)
                }
                ("world_metadata", _) if ch.field == "title" => {
                    // Read current title from the world row.
                    sqlx::query_scalar::<_, String>(
                        "SELECT title FROM narrative_worlds WHERE world_id = ?",
                    )
                    .bind(&parsed.world_id)
                    .fetch_optional(&**pool)
                    .await
                    .ok()
                    .flatten()
                    .map(Value::String)
                }
                (other_entity, _) => {
                    return Err(CapabilityError::InputInvalid(format!(
                        "unsupported entity '{other_entity}' (V1.60: kb_key_block, world_metadata)"
                    )));
                }
            };

            proposed.push(json!({
                "entity": ch.entity,
                "entity_id": ch.entity_id,
                "field": ch.field,
                "old_value": old_value,
                "new_value": ch.new_value,
                "rationale": ch.rationale,
            }));
        }

        Ok(json!({
            "schema_version": 1,
            "policy_context": {
                "world_id": parsed.world_id,
                "creator_id": parsed.creator_id,
                "source_work_id": "wrk_local",
                "workspace_slug": slug,
            },
            "proposed_changes": proposed,
            "atomic": true,
        }))
    }
}

/// Extract a serializable field value from a `KeyBlock` for the lost-update guard.
fn field_of(kb: &nexus_kb::key_block::KeyBlock, field: &str) -> Value {
    match field {
        "canonical_name" => json!(kb.canonical_name),
        "status" => json!(kb.status),
        "body_json" => kb
            .body
            .as_ref()
            .and_then(|b| serde_json::to_value(b).ok())
            .unwrap_or(Value::Null),
        _ => Value::Null,
    }
}

// ─── Capability: nexus.world.delta.apply ───────────────────────────────────

/// Apply a delta package under a transaction with a lost-update guard.
#[derive(Debug, Clone)]
pub struct WorldDeltaApply {
    pool: Option<Arc<sqlx::SqlitePool>>,
}

impl WorldDeltaApply {
    #[must_use]
    pub const fn new() -> Self {
        Self { pool: None }
    }

    #[must_use]
    pub fn with_pool(pool: sqlx::SqlitePool) -> Self {
        Self {
            pool: Some(Arc::new(pool)),
        }
    }
}

impl Default for WorldDeltaApply {
    fn default() -> Self {
        Self::new()
    }
}

/// Maximum number of `key_block_id` values to bind in a single pre-fetch
/// IN-list. `SQLite` supports far more, but a fixed cap keeps the generated
/// SQL and bind-count bounded regardless of caller input size.
const KB_PREFETCH_CHUNK_SIZE: usize = 500;

#[async_trait]
impl Capability for WorldDeltaApply {
    fn name(&self) -> &'static str {
        "nexus.world.delta.apply"
    }

    fn input_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"policy_context":{"type":"object","properties":{"world_id":{"type":"string"},"creator_id":{"type":"string"},"source_work_id":{"type":"string"}},"required":["world_id","creator_id"]},"proposed_changes":{"type":"array","items":{"type":"object","properties":{"entity":{"type":"string"},"entity_id":{"type":"string"},"field":{"type":"string"},"old_value":{},"new_value":{},"rationale":{"type":"string"}},"required":["entity","field","new_value","rationale"]}},"atomic":{"type":"boolean"}},"required":["policy_context","proposed_changes"],"additionalProperties":false}"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"applied":{"type":"array"},"atomic_applied":{"type":"boolean"}},"required":["applied","atomic_applied"],"additionalProperties":false}"#
    }

    #[allow(clippy::too_many_lines)]
    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let parsed: WorldDeltaApplyInput = serde_json::from_value(input)
            .map_err(|e| CapabilityError::InputInvalid(format!("world.delta.apply input: {e}")))?;

        let pool = self
            .pool
            .as_ref()
            .ok_or(CapabilityError::WorkerUnavailable)?;

        let world_id = parsed.policy_context.world_id.clone();
        let creator_id = parsed.policy_context.creator_id.clone();

        tracing::info!(
            world_id = %world_id,
            changes = parsed.proposed_changes.len(),
            atomic = parsed.atomic,
            "world.delta.apply admitted"
        );

        // Atomic apply: the whole package runs in one transaction. `atomic:false`
        // is accepted but V1.60 still applies transactionally (partial commit is
        // post-1.0); the output reports `atomic_applied: true`.
        let mut tx = pool
            .begin()
            .await
            .map_err(|e| CapabilityError::Internal(format!("begin tx: {e}")))?;

        // TOCTOU guard: re-verify ownership inside the transaction.
        // SAFETY: ownership check against known narrative_worlds schema.
        let owned: Option<String> = sqlx::query_scalar(
            "SELECT world_id FROM narrative_worlds WHERE world_id = ? AND owner_creator_id = ?",
        )
        .bind(&world_id)
        .bind(&creator_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| CapabilityError::Internal(format!("world ownership (tx): {e}")))?;
        if owned.is_none() {
            return Err(CapabilityError::Forbidden(
                "world not found or not owned by creator".into(),
            ));
        }

        let mut results: Vec<Value> = Vec::with_capacity(parsed.proposed_changes.len());
        let mut all_applied = true;

        // V1.67 P2 (R-V160P0-QC3-W001 / W-002): pre-fetch current body_json
        // for all kb_key_block update targets in chunked, deduplicated
        // IN-lists, replacing the per-change SELECT + UPDATE N+1 pattern
        // while bounding the SQL size regardless of caller input.
        let update_kids: Vec<&str> = parsed
            .proposed_changes
            .iter()
            .filter_map(|ch| {
                if ch.entity == "kb_key_block" && ch.entity_id.is_some() {
                    ch.entity_id.as_deref()
                } else {
                    None
                }
            })
            .collect();

        let mut live_body_map: HashMap<String, Option<String>> = HashMap::new();
        if !update_kids.is_empty() {
            // Dedupe ids so duplicate entity_id entries do not inflate bind
            // counts or generate redundant IN-list chunks.
            let mut seen = HashSet::with_capacity(update_kids.len());
            let unique_update_kids: Vec<&str> = update_kids
                .into_iter()
                .filter(|kid| seen.insert(*kid))
                .collect();

            for chunk in unique_update_kids.chunks(KB_PREFETCH_CHUNK_SIZE) {
                // SAFETY: column/table names are string literals; key_block_id
                // values are bound as parameters. Dynamic IN-list length is the
                // only non-static aspect.
                let placeholders = chunk.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
                let sql = format!(
                    "SELECT key_block_id, body_json FROM kb_key_blocks WHERE key_block_id IN ({placeholders})"
                );
                let mut q = sqlx::query_as::<_, (String, Option<String>)>(&sql);
                for kid in chunk {
                    q = q.bind(*kid);
                }
                let rows = q
                    .fetch_all(&mut *tx)
                    .await
                    .map_err(|e| CapabilityError::Internal(format!("kb batch read (tx): {e}")))?;
                for (kid, body) in rows {
                    live_body_map.insert(kid, body);
                }
            }
        }

        for ch in parsed.proposed_changes {
            // Capture the rationale for the audit trail before `ch` is consumed.
            let rationale = ch.rationale.clone();
            match (ch.entity.as_str(), ch.entity_id.as_deref()) {
                ("kb_key_block", Some(kid)) => {
                    // Update path with lost-update guard.
                    // body_json is a nullable column → scalar type is Option<String>.
                    // V1.67 P2 (R-V160P0-QC3-W001): value was pre-fetched in
                    // bulk above instead of issuing one SELECT per change.
                    let live_body = live_body_map.get(kid).cloned().unwrap_or(None);

                    let live = live_body
                        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
                        .unwrap_or(Value::Null);

                    if let Some(expected) = &ch.old_value {
                        if &live != expected {
                            all_applied = false;
                            results.push(json!({
                                "entity": ch.entity,
                                "entity_id": kid,
                                "field": ch.field,
                                "status": "conflict",
                                "live_value": live,
                                "rationale": rationale,
                            }));
                            continue;
                        }
                    }

                    // Apply the field update. V1.60 supports body_json + status.
                    // SAFETY: UPDATE against known kb_key_blocks schema.
                    match ch.field.as_str() {
                        "body_json" => {
                            let body_str = serde_json::to_string(&ch.new_value)
                                .map_err(|e| CapabilityError::Internal(e.to_string()))?;
                            sqlx::query(
                                "UPDATE kb_key_blocks SET body_json = ?, updated_at = ? \
                                 WHERE key_block_id = ?",
                            )
                            .bind(&body_str)
                            .bind(chrono::Utc::now().to_rfc3339())
                            .bind(kid)
                            .execute(&mut *tx)
                            .await
                            .map_err(|e| CapabilityError::Internal(format!("kb update: {e}")))?;
                        }
                        "status" => {
                            let new_status = ch.new_value.as_str().ok_or_else(|| {
                                CapabilityError::InputInvalid(
                                    "status new_value must be a string".into(),
                                )
                            })?;
                            sqlx::query(
                                "UPDATE kb_key_blocks SET status = ?, updated_at = ? \
                                 WHERE key_block_id = ?",
                            )
                            .bind(new_status)
                            .bind(chrono::Utc::now().to_rfc3339())
                            .bind(kid)
                            .execute(&mut *tx)
                            .await
                            .map_err(|e| CapabilityError::Internal(format!("kb status: {e}")))?;
                        }
                        other => {
                            return Err(CapabilityError::InputInvalid(format!(
                                "unsupported kb_key_block field '{other}' (V1.60: body_json, status)"
                            )));
                        }
                    }

                    results.push(json!({
                        "entity": ch.entity,
                        "entity_id": kid,
                        "field": ch.field,
                        "status": "applied",
                        "rationale": rationale,
                    }));
                }
                ("kb_key_block", None) => {
                    // Create path: insert a new provisional key block via the
                    // proper DAO method (`insert_key_block_in_tx`), which shares
                    // this handler's transaction so the create rolls back
                    // atomically with sibling changes.
                    //
                    // R-V160P0-QC1-W001: the prior hand-written INSERT
                    // referenced a non-existent `metadata_json` column and
                    // defaulted `block_type` to the invalid literal "concept".
                    // Routing through the DAO issues the correct INSERT, runs
                    // canonical_name + body validation, and reuses the canonical
                    // `KeyBlock::new` defaults (status = provisional).
                    let canonical = ch
                        .new_value
                        .get("canonical_name")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            CapabilityError::InputInvalid(
                                "create kb_key_block requires new_value.canonical_name".into(),
                            )
                        })?;
                    // Parse `block_type` into the enum (snake_case wire form).
                    // Defaults to `BlockType::Character` (the canonical default)
                    // when absent or unrecognised; the prior "concept" literal
                    // was never a valid variant.
                    let block_type = ch
                        .new_value
                        .get("block_type")
                        .and_then(|v| {
                            serde_json::from_value::<nexus_contracts::BlockType>(v.clone()).ok()
                        })
                        .unwrap_or_default();
                    let mut kb =
                        nexus_kb::key_block::KeyBlock::new(&world_id, block_type, canonical);
                    if let Some(body) = ch.new_value.get("body_json").and_then(|v| {
                        serde_json::from_value::<nexus_kb::key_block::KeyBlockBody>(v.clone()).ok()
                    }) {
                        kb.body = Some(body);
                    }

                    let kb_store = nexus_local_db::kb_store::SqliteKbStore::new((**pool).clone());
                    let insert_result = kb_store
                        .insert_key_block_in_tx(&mut tx, kb)
                        .await
                        .map_err(|e| match e {
                            nexus_kb::store::KbStoreError::Validation(_)
                            | nexus_kb::store::KbStoreError::ValidationLegacy(_) => {
                                CapabilityError::InputInvalid(format!("kb insert: {e}"))
                            }
                            other => CapabilityError::Internal(format!("kb insert: {other}")),
                        })?;

                    results.push(json!({
                        "entity": ch.entity,
                        "entity_id": insert_result.key_block_id,
                        "field": ch.field,
                        "status": "applied",
                        "rationale": rationale,
                    }));
                }
                ("world_metadata", _) if ch.field == "title" => {
                    // Lost-update guard: compare old_value (if supplied) to the
                    // live title before applying. SAFETY: SELECT against known
                    // narrative_worlds schema.
                    let live_title: Option<String> =
                        sqlx::query_scalar("SELECT title FROM narrative_worlds WHERE world_id = ?")
                            .bind(&world_id)
                            .fetch_optional(&mut *tx)
                            .await
                            .map_err(|e| {
                                CapabilityError::Internal(format!("title read (tx): {e}"))
                            })?;
                    if let Some(expected) = &ch.old_value {
                        let live = live_title.map_or(Value::Null, Value::String);
                        if &live != expected {
                            all_applied = false;
                            results.push(json!({
                                "entity": ch.entity,
                                "entity_id": world_id,
                                "field": ch.field,
                                "status": "conflict",
                                "live_value": live,
                                "rationale": rationale,
                            }));
                            continue;
                        }
                    }

                    let new_title = ch.new_value.as_str().ok_or_else(|| {
                        CapabilityError::InputInvalid("title new_value must be a string".into())
                    })?;
                    // SAFETY: UPDATE against known narrative_worlds schema.
                    sqlx::query(
                        "UPDATE narrative_worlds SET title = ?, updated_at = ? WHERE world_id = ?",
                    )
                    .bind(new_title)
                    .bind(chrono::Utc::now().to_rfc3339())
                    .bind(&world_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| CapabilityError::Internal(format!("world title update: {e}")))?;

                    results.push(json!({
                        "entity": ch.entity,
                        "entity_id": world_id,
                        "field": ch.field,
                        "status": "applied",
                        "rationale": rationale,
                    }));
                }
                (other_entity, _) => {
                    return Err(CapabilityError::InputInvalid(format!(
                        "unsupported entity '{other_entity}' (V1.60: kb_key_block, world_metadata)"
                    )));
                }
            }
        }

        // Commit the atomic batch.
        tx.commit()
            .await
            .map_err(|e| CapabilityError::TransientExternal(format!("commit: {e}")))?;

        tracing::info!(
            world_id = %world_id,
            applied = results.len(),
            all_applied,
            "world.delta.apply committed"
        );

        Ok(json!({
            "applied": results,
            "atomic_applied": all_applied,
            "source_work_id": parsed.policy_context.source_work_id,
        }))
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use nexus_local_db::{open_pool, run_migrations};

    async fn fresh_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool).await.unwrap();
        (pool, dir)
    }

    async fn seed_creator(pool: &sqlx::SqlitePool, creator_id: &str) {
        // SAFETY: test-only seed.
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
        // SAFETY: test-only seed using narrative_write helper.
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
        // Rename to a fixed id for deterministic tests.
        sqlx::query("UPDATE narrative_worlds SET world_id = ? WHERE owner_creator_id = ?")
            .bind(world_id)
            .bind(owner)
            .execute(pool)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn world_state_query_success() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool, "ctr_a").await;
        seed_world(&pool, "ctr_a", "wld_a").await;
        // Append a timeline event so the timeline slice is non-empty.
        let world = sqlx::query_scalar::<_, String>(
            "SELECT root_fork_branch_id FROM narrative_worlds WHERE world_id = 'wld_a'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        nexus_local_db::narrative_write::append_event(
            &pool,
            "wld_a",
            &world,
            "story_advance",
            Some("E1"),
            None,
        )
        .await
        .unwrap();

        let cap = WorldStateQuery::with_pool(pool);
        let out = cap
            .run(json!({"world_id": "wld_a", "creator_id": "ctr_a"}))
            .await
            .unwrap();
        assert_eq!(out["world_id"], "wld_a");
        assert_eq!(out["timeline"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn world_state_query_rejects_cross_creator() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool, "ctr_a").await;
        seed_creator(&pool, "ctr_b").await;
        seed_world(&pool, "ctr_a", "wld_a").await;

        let cap = WorldStateQuery::with_pool(pool);
        let err = cap
            .run(json!({"world_id": "wld_a", "creator_id": "ctr_b"}))
            .await
            .unwrap_err();
        assert!(matches!(err, CapabilityError::Forbidden(_)));
    }

    #[tokio::test]
    async fn world_state_query_rejects_invalid_input() {
        let (pool, _dir) = fresh_pool().await;
        let cap = WorldStateQuery::with_pool(pool);
        let err = cap.run(json!(42)).await.unwrap_err();
        assert!(matches!(err, CapabilityError::InputInvalid(_)));
    }

    // ── nexus.world.delta.propose ────────────────────────────────────────────

    #[tokio::test]
    async fn world_delta_propose_success_populates_old_value() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool, "ctr_a").await;
        seed_world(&pool, "ctr_a", "wld_a").await;

        let cap = WorldDeltaPropose::with_pool(pool);
        let out = cap
            .run(json!({
                "world_id": "wld_a",
                "creator_id": "ctr_a",
                "changeset": [{
                    "entity": "world_metadata",
                    "field": "title",
                    "new_value": "New Title",
                    "rationale": "rename for clarity"
                }]
            }))
            .await
            .unwrap();
        assert_eq!(out["schema_version"], 1);
        assert_eq!(out["policy_context"]["world_id"], "wld_a");
        // old_value should be populated from current state (the seeded title).
        assert_eq!(out["proposed_changes"][0]["old_value"], "Test World");
        assert_eq!(out["proposed_changes"][0]["new_value"], "New Title");
    }

    #[tokio::test]
    async fn world_delta_propose_rejects_cross_creator() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool, "ctr_a").await;
        seed_creator(&pool, "ctr_b").await;
        seed_world(&pool, "ctr_a", "wld_a").await;

        let cap = WorldDeltaPropose::with_pool(pool);
        let err = cap
            .run(json!({
                "world_id": "wld_a",
                "creator_id": "ctr_b",
                "changeset": [{
                    "entity": "world_metadata",
                    "field": "title",
                    "new_value": "X",
                    "rationale": "x"
                }]
            }))
            .await
            .unwrap_err();
        assert!(matches!(err, CapabilityError::Forbidden(_)));
    }

    #[tokio::test]
    async fn world_delta_propose_rejects_invalid_input() {
        let (pool, _dir) = fresh_pool().await;
        let cap = WorldDeltaPropose::with_pool(pool);
        let err = cap.run(json!("not-an-object")).await.unwrap_err();
        assert!(matches!(err, CapabilityError::InputInvalid(_)));
    }

    // ── nexus.world.delta.apply ──────────────────────────────────────────────

    #[tokio::test]
    async fn world_delta_apply_title_update_success() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool, "ctr_a").await;
        seed_world(&pool, "ctr_a", "wld_a").await;

        let cap = WorldDeltaApply::with_pool(pool.clone());
        let out = cap
            .run(json!({
                "policy_context": {
                    "world_id": "wld_a",
                    "creator_id": "ctr_a",
                    "source_work_id": "wrk_local"
                },
                "proposed_changes": [{
                    "entity": "world_metadata",
                    "field": "title",
                    "old_value": "Test World",
                    "new_value": "Renamed World",
                    "rationale": "agent rename"
                }],
                "atomic": true
            }))
            .await
            .unwrap();
        assert_eq!(out["applied"][0]["status"], "applied");
        assert_eq!(out["atomic_applied"], true);

        // Verify the title was actually updated.
        let title: String =
            sqlx::query_scalar("SELECT title FROM narrative_worlds WHERE world_id = 'wld_a'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(title, "Renamed World");
    }

    #[tokio::test]
    async fn world_delta_apply_rejects_cross_creator() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool, "ctr_a").await;
        seed_creator(&pool, "ctr_b").await;
        seed_world(&pool, "ctr_a", "wld_a").await;

        let cap = WorldDeltaApply::with_pool(pool);
        let err = cap
            .run(json!({
                "policy_context": {"world_id": "wld_a", "creator_id": "ctr_b"},
                "proposed_changes": [{
                    "entity": "world_metadata",
                    "field": "title",
                    "new_value": "X",
                    "rationale": "x"
                }]
            }))
            .await
            .unwrap_err();
        assert!(matches!(err, CapabilityError::Forbidden(_)));
    }

    #[tokio::test]
    async fn world_delta_apply_lost_update_guard_reports_conflict() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool, "ctr_a").await;
        seed_world(&pool, "ctr_a", "wld_a").await;

        let cap = WorldDeltaApply::with_pool(pool.clone());
        // Supply a stale old_value that does not match the live title.
        let out = cap
            .run(json!({
                "policy_context": {"world_id": "wld_a", "creator_id": "ctr_a"},
                "proposed_changes": [{
                    "entity": "world_metadata",
                    "field": "title",
                    "old_value": "STALE_DOES_NOT_MATCH",
                    "new_value": "Renamed",
                    "rationale": "stale apply attempt"
                }]
            }))
            .await
            .unwrap();
        assert_eq!(out["applied"][0]["status"], "conflict");
        assert_eq!(out["atomic_applied"], false);
        // The live title must NOT have changed (transaction rolled back).
        let title: String =
            sqlx::query_scalar("SELECT title FROM narrative_worlds WHERE world_id = 'wld_a'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(title, "Test World");
    }

    #[tokio::test]
    async fn world_delta_apply_kb_key_block_create_persists_row() {
        // Regression for R-V160P0-QC1-W001: the kb_key_block create branch
        // previously hand-wrote an INSERT referencing a non-existent
        // `metadata_json` column (and an invalid "concept" block_type default).
        // None of the 9 in-file tests exercised the create path, so CI stayed
        // green while runtime would fail with a SQL error. This test drives the
        // create branch end to end and asserts the row lands in kb_key_blocks.
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool, "ctr_a").await;
        seed_world(&pool, "ctr_a", "wld_a").await;

        let cap = WorldDeltaApply::with_pool(pool.clone());
        let out = cap
            .run(json!({
                "policy_context": {
                    "world_id": "wld_a",
                    "creator_id": "ctr_a",
                    "source_work_id": "wrk_local"
                },
                "proposed_changes": [{
                    "entity": "kb_key_block",
                    "field": "canonical_name",
                    "new_value": {
                        "canonical_name": "Hero of Aethel",
                        "block_type": "character",
                        "body_json": {
                            "summary": "The protagonist",
                            "tags": ["protagonist"]
                        }
                    },
                    "rationale": "agent creates a new character key block"
                }],
                "atomic": true
            }))
            .await
            .expect("create kb_key_block must succeed (W-001 regression)");

        // The change reports applied with a freshly minted kb_ id.
        assert_eq!(out["applied"][0]["status"], "applied");
        assert_eq!(out["atomic_applied"], true);
        let new_id = out["applied"][0]["entity_id"]
            .as_str()
            .expect("entity_id should be a kb_ string");
        assert!(
            new_id.starts_with("kb_"),
            "expected a kb_ prefixed id, got {new_id}"
        );

        // The row actually persisted with the canonical schema columns. This
        // read-back would have failed entirely under the old hand-written SQL
        // (the INSERT never succeeded).
        let (row_canonical, row_type, row_status, row_body): (
            String,
            String,
            String,
            Option<String>,
        ) = sqlx::query_as(
            "SELECT canonical_name, block_type, status, body_json \
             FROM kb_key_blocks WHERE key_block_id = ?",
        )
        .bind(new_id)
        .fetch_one(&pool)
        .await
        .expect("created key block row must be readable");
        assert_eq!(row_canonical, "Hero of Aethel");
        assert_eq!(row_type, "character");
        assert_eq!(row_status, "provisional");
        let body = row_body.expect("body_json should be persisted");
        assert!(
            body.contains("The protagonist"),
            "body_json should carry the summary, got {body}"
        );
        assert!(body.contains("protagonist"));
    }

    // V1.67 P2 (R-V160P0-QC3-W001): regression test for the N+1 SELECT fix.
    // Applies 4 kb_key_block updates in one delta package and asserts both
    // successful applies and conflict detection still work, while using the
    // bulk pre-fetch path instead of one SELECT per change.
    #[tokio::test]
    async fn world_delta_apply_batch_kb_updates_prefetch() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool, "ctr_a").await;
        seed_world(&pool, "ctr_a", "wld_a").await;

        // Seed two key blocks via the create path.
        let cap = WorldDeltaApply::with_pool(pool.clone());
        let out = cap
            .run(json!({
                "policy_context": {
                    "world_id": "wld_a",
                    "creator_id": "ctr_a",
                    "source_work_id": "wrk_local"
                },
                "proposed_changes": [
                    {
                        "entity": "kb_key_block",
                        "field": "canonical_name",
                        "new_value": {
                            "canonical_name": "Hero",
                            "block_type": "character",
                            "body_json": {"attributes": {"hp": 10}}
                        },
                        "rationale": "create hero"
                    },
                    {
                        "entity": "kb_key_block",
                        "field": "canonical_name",
                        "new_value": {
                            "canonical_name": "Villain",
                            "block_type": "character",
                            "body_json": {"attributes": {"hp": 20}}
                        },
                        "rationale": "create villain"
                    }
                ],
                "atomic": true
            }))
            .await
            .unwrap();

        let hero_id = out["applied"][0]["entity_id"].as_str().unwrap();
        let villain_id = out["applied"][1]["entity_id"].as_str().unwrap();

        // Apply two body_json updates and one stale conflict in one package.
        let out2 = cap
            .run(json!({
                "policy_context": {
                    "world_id": "wld_a",
                    "creator_id": "ctr_a",
                    "source_work_id": "wrk_local"
                },
                "proposed_changes": [
                    {
                        "entity": "kb_key_block",
                        "entity_id": hero_id,
                        "field": "body_json",
                        "old_value": {"attributes": {"hp": 10}},
                        "new_value": {"attributes": {"hp": 15}},
                        "rationale": "level up hero"
                    },
                    {
                        "entity": "kb_key_block",
                        "entity_id": villain_id,
                        "field": "body_json",
                        "old_value": {"attributes": {"hp": 999}},
                        "new_value": {"attributes": {"hp": 25}},
                        "rationale": "wrong old value"
                    }
                ],
                "atomic": true
            }))
            .await
            .unwrap();

        assert_eq!(out2["applied"][0]["status"], "applied");
        assert_eq!(out2["applied"][1]["status"], "conflict");
        assert_eq!(out2["atomic_applied"], false);

        // The successful update must have persisted; the conflict must not.
        let hero_body: Option<String> =
            sqlx::query_scalar("SELECT body_json FROM kb_key_blocks WHERE key_block_id = ?")
                .bind(hero_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(hero_body.unwrap().contains("15"));

        let villain_body: Option<String> =
            sqlx::query_scalar("SELECT body_json FROM kb_key_blocks WHERE key_block_id = ?")
                .bind(villain_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert!(villain_body.unwrap().contains("20"));
    }

    // V1.67 P2 fix-wave 1 (W-002): regression test for chunked/deduped
    // kb_key_block pre-fetch. Applies body_json updates for more ids than
    // one IN-list chunk can hold, verifying the handler still returns correct
    // results and does not construct an oversized SQL statement.
    #[tokio::test]
    async fn world_delta_apply_batch_kb_updates_prefetch_chunks() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool, "ctr_a").await;
        seed_world(&pool, "ctr_a", "wld_a").await;

        let chunk_size = KB_PREFETCH_CHUNK_SIZE;
        let total = chunk_size + 5;
        let mut ids = Vec::with_capacity(total);
        for i in 0..total {
            let kid = format!("kb_chunk_{i:05}");
            ids.push(kid.clone());
            // SAFETY: test-only direct insert into kb_key_blocks.
            sqlx::query(
                "INSERT INTO kb_key_blocks \
                 (key_block_id, world_id, block_type, canonical_name, status, body_json) \
                 VALUES (?, ?, 'character', ?, 'provisional', ?)",
            )
            .bind(&kid)
            .bind("wld_a")
            .bind(format!("block_{i}"))
            .bind("{\"hp\":1}")
            .execute(&pool)
            .await
            .unwrap();
        }

        let changes: Vec<Value> = ids
            .iter()
            .map(|kid| {
                json!({
                    "entity": "kb_key_block",
                    "entity_id": kid,
                    "field": "body_json",
                    "old_value": {"hp": 1},
                    "new_value": {"hp": 2},
                    "rationale": "bulk level up"
                })
            })
            .collect();

        let cap = WorldDeltaApply::with_pool(pool.clone());
        let out = cap
            .run(json!({
                "policy_context": {
                    "world_id": "wld_a",
                    "creator_id": "ctr_a",
                    "source_work_id": "wrk_local"
                },
                "proposed_changes": changes,
                "atomic": true
            }))
            .await
            .unwrap();

        assert_eq!(out["applied"].as_array().unwrap().len(), total);
        for result in out["applied"].as_array().unwrap() {
            assert_eq!(result["status"], "applied");
        }

        let applied_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM kb_key_blocks WHERE body_json LIKE '%\"hp\":2%'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(applied_count as usize, total);
    }
}
