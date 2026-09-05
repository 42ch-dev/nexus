//! Shared `ComputeInput` assembly for WASM compute invocations.
//!
//! Extracted from the duplicate logic in `narrative_compute.rs` L216–291.
//! This builder queries the KB store, filters by the module manifest's
//! `required_key_block_types`, loads referenced entries for `*_id`
//! invocation-param keys (with cross-world reject), converts domain entries
//! to spoke `KnowledgeEntry` JSON, and assembles the full [`ComputeInput`]
//! envelope.
//!
//! Consumed by both the daemon handler (direct Control Room lane, Task 4)
//! and (in a future refactor) the `narrative.compute` capability.

use nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord;
use nexus_knowledge::world_kb::{KbQuery, KbStore};
use nexus_local_db::kb_store::SqliteKbStore;
use nexus_local_db::narrative_gateway::SqliteNarrativeGateway;
use nexus_narrative::NarrativeGateway;
use nexus_spoke_adapter::conversion::knowledge_record_to_spoke;
use nexus_wasm_host::ModuleManifest;
use serde_json::{Map, Value};
use sqlx::SqlitePool;
use std::num::NonZeroU64;
use thiserror::Error;

// Re-export the generated ComputeInput types used in the public API.
use nexus_contracts::generated::daemon_api::compute::compute_input::{
    ComputeInput, ComputeInputNarrativeState, ComputeInputWorldRef,
    ComputeInputWorldRefTimelineHeadEventId, ComputeInputWorldRefWorldId,
};

/// Errors that can occur while assembling a [`ComputeInput`].
#[derive(Debug, Error)]
pub enum ComputeBuildError {
    /// The world has no computable knowledge entries matching the module's
    /// `required_key_block_types`.
    #[error("no computable knowledge entries found in world")]
    NoComputableEntries,

    /// An invocation-parameter `*_id` referenced an entry that belongs to a
    /// different world.
    #[error("cross-world reference: {0}")]
    ReferencedEntryNotInWorld(String),

    /// An invocation-parameter `*_id` referenced an entry that does not exist.
    #[error("referenced entry not found: {0}")]
    ReferencedEntryNotFound(String),

    /// A database error occurred during KB or narrative-state queries.
    #[error("store error: {0}")]
    Store(#[from] sqlx::Error),

    /// A narrative gateway error occurred while reading world state.
    #[error("narrative error: {0}")]
    Narrative(#[from] nexus_narrative::NarrativeError),

    /// A KB store error occurred during entry queries.
    #[error("kb store error: {0}")]
    KbStore(String),

    /// An internal invariant was violated (should not occur with valid inputs).
    #[error("internal error: {0}")]
    Internal(String),
}

impl From<nexus_knowledge::world_kb::KbStoreError> for ComputeBuildError {
    fn from(e: nexus_knowledge::world_kb::KbStoreError) -> Self {
        Self::KbStore(e.to_string())
    }
}

/// Builder that assembles a [`ComputeInput`] envelope for a World-scoped
/// compute invocation.
///
/// # Example
///
/// ```ignore
/// # use nexus_orchestration::compute_input_builder::ComputeInputBuilder;
/// # use serde_json::Map;
/// # let pool: sqlx::SqlitePool = unimplemented!();
/// # let manifest: nexus_wasm_host::ModuleManifest = unimplemented!();
/// let builder = ComputeInputBuilder::new(pool, "wld_abc123", manifest, Map::new());
/// let input = builder.build().await?;
/// # Ok::<(), nexus_orchestration::compute_input_builder::ComputeBuildError>(())
/// ```
pub struct ComputeInputBuilder {
    pool: SqlitePool,
    world_id: String,
    module_manifest: ModuleManifest,
    invocation_params: Map<String, Value>,
    /// Optional branch/head override resolved by the caller (direct lane).
    /// When `None`, [`Self::read_narrative_state`] falls back to the
    /// gateway's world state (world root branch) — preserving the original
    /// behavior for callers that do not resolve a position themselves.
    narrative_position: Option<(String, Option<String>)>,
}

impl ComputeInputBuilder {
    /// Create a new builder.
    #[must_use]
    pub fn new(
        pool: SqlitePool,
        world_id: impl Into<String>,
        module_manifest: ModuleManifest,
        invocation_params: Map<String, Value>,
    ) -> Self {
        Self {
            pool,
            world_id: world_id.into(),
            module_manifest,
            invocation_params,
            narrative_position: None,
        }
    }

    /// Override the narrative position (branch + timeline head) used for the
    /// `ComputeInput.world_ref`.
    ///
    /// The direct Control Room lane resolves the branch (validated under the
    /// owned world, defaulting to the world root) and snapshots it onto the
    /// run row — the module must see exactly the position that is snapshotted
    /// (F-002/F-003).  When not called, the builder reads the world state and
    /// defaults to the root branch, as before.
    #[must_use]
    pub fn with_narrative_position(
        mut self,
        branch_id: String,
        timeline_head_event_id: Option<String>,
    ) -> Self {
        self.narrative_position = Some((branch_id, timeline_head_event_id));
        self
    }

    /// Assemble the [`ComputeInput`] envelope.
    ///
    /// # Errors
    ///
    /// Returns [`ComputeBuildError::NoComputableEntries`] when no computable
    /// entries matching the manifest's block types exist in the world.
    ///
    /// Returns [`ComputeBuildError::ReferencedEntryNotInWorld`] when an
    /// `*_id` invocation parameter points to an entry in a different world.
    /// # Panics
    ///
    /// Panics if `schema_version` literal 1 is not representable as `NonZeroU64`
    /// (this can never happen — 1 is always non-zero).
    pub async fn build(self) -> Result<ComputeInput, ComputeBuildError> {
        let kb_store = SqliteKbStore::new(self.pool.clone());
        let narrative_gw = SqliteNarrativeGateway::new(self.pool.clone());

        let mut key_blocks = self.query_entries(&kb_store).await?;
        self.load_referenced_entries(&kb_store, &mut key_blocks)
            .await?;
        let (branch_id, timeline_head_event_id) = match self.narrative_position {
            Some(ref pos) => pos.clone(),
            None => self.read_narrative_state(&narrative_gw).await?,
        };
        let key_blocks_json = convert_entries_to_spoke_json(key_blocks);
        let world_ref = self.build_world_ref(&branch_id, timeline_head_event_id.as_ref())?;
        let narrative_state = ComputeInputNarrativeState {
            timeline_position: Some("0".to_string()),
            ..Default::default()
        };

        Ok(ComputeInput {
            schema_version: NonZeroU64::new(1).expect("schema_version literal 1 is non-zero"),
            world_ref,
            key_blocks: key_blocks_json,
            narrative_state: Some(narrative_state),
            invocation: self.invocation_params,
        })
    }

    /// Query computable entries and filter by manifest's `required_key_block_types`.
    async fn query_entries(
        &self,
        kb_store: &SqliteKbStore,
    ) -> Result<Vec<KnowledgeEntryRecord>, ComputeBuildError> {
        let q = KbQuery::new(&self.world_id).with_computable(Some(true));
        let computable_blocks = kb_store.query(&q).await?;

        let required_types: Vec<&str> = self
            .module_manifest
            .required_key_block_types
            .iter()
            .map(String::as_str)
            .collect();

        let key_blocks: Vec<KnowledgeEntryRecord> = computable_blocks
            .items
            .into_iter()
            .filter(|kb| {
                required_types.is_empty() || required_types.contains(&kb.block_type.as_str())
            })
            .collect();

        if key_blocks.is_empty() {
            return Err(ComputeBuildError::NoComputableEntries);
        }

        Ok(key_blocks)
    }

    /// Load entries referenced by `*_id` invocation-param keys, with cross-world check.
    async fn load_referenced_entries(
        &self,
        kb_store: &SqliteKbStore,
        key_blocks: &mut Vec<KnowledgeEntryRecord>,
    ) -> Result<(), ComputeBuildError> {
        let mut referenced_entry_ids: Vec<String> = Vec::new();
        for key in self.invocation_params.keys() {
            if key.ends_with("_id") {
                if let Some(Value::String(ref_id)) = self.invocation_params.get(key) {
                    if !key_blocks.iter().any(|kb| kb.entry_id == *ref_id)
                        && !referenced_entry_ids.contains(ref_id)
                    {
                        referenced_entry_ids.push(ref_id.clone());
                    }
                }
            }
        }

        for ref_id in &referenced_entry_ids {
            let ref_entry = kb_store.get_knowledge_entry(ref_id).await.map_err(|e| {
                ComputeBuildError::ReferencedEntryNotFound(format!(
                    "referenced entry {ref_id} not found: {e}"
                ))
            })?;

            if ref_entry.world_id() != Some(self.world_id.as_str()) {
                return Err(ComputeBuildError::ReferencedEntryNotInWorld(format!(
                    "referenced entry {ref_id} belongs to world {}, not {}",
                    ref_entry.world_id().unwrap_or_default(),
                    self.world_id
                )));
            }

            key_blocks.push(ref_entry);
        }

        Ok(())
    }

    /// Read narrative state: branch ID (default root) and timeline head.
    async fn read_narrative_state(
        &self,
        gw: &SqliteNarrativeGateway,
    ) -> Result<(String, Option<String>), ComputeBuildError> {
        let world_state = gw.get_world_state(&self.world_id).await?;
        let branch_id = world_state
            .fork_branch_id
            .unwrap_or_else(|| "fbk_root".to_string());
        let timeline_head_event_id = world_state.current_timeline_head_id;
        Ok((branch_id, timeline_head_event_id))
    }

    /// Build the `ComputeInputWorldRef` from resolved narrative state.
    fn build_world_ref(
        &self,
        branch_id: &str,
        timeline_head_event_id: Option<&String>,
    ) -> Result<ComputeInputWorldRef, ComputeBuildError> {
        let world_id_newtype = ComputeInputWorldRefWorldId::try_from(self.world_id.clone())
            .map_err(|e| {
                ComputeBuildError::Internal(format!("invalid world_id for ComputeInput: {e}"))
            })?;

        let timeline_head_newtype = timeline_head_event_id
            .map(String::as_str)
            .map(ComputeInputWorldRefTimelineHeadEventId::try_from)
            .transpose()
            .map_err(|e| {
                ComputeBuildError::Internal(format!(
                    "invalid timeline_head_event_id for ComputeInput: {e}"
                ))
            })?;

        Ok(ComputeInputWorldRef {
            world_id: Some(world_id_newtype),
            branch_id: Some(branch_id.to_string()),
            timeline_head_event_id: timeline_head_newtype,
        })
    }
}

/// Convert domain `KnowledgeEntryRecord` entries to spoke `KnowledgeEntry` JSON object maps.
fn convert_entries_to_spoke_json(entries: Vec<KnowledgeEntryRecord>) -> Vec<Map<String, Value>> {
    entries
        .into_iter()
        .map(|kb| {
            let spoke = knowledge_record_to_spoke(&kb);
            serde_json::to_value(&spoke)
                .ok()
                .and_then(|v| v.as_object().cloned())
                .unwrap_or_default()
        })
        .collect()
}
