//! Delta Bundle Builder
//!
//! Constructs `Bundle` envelopes with metadata fields for CLI <-> Platform sync.
//! Uses generated `Bundle` type from `nexus-contracts`.
//!
//! # Bundle Metadata Fields (V1.0 — SYNC-R1)
//!
//! - `submitting_creator_id`: identifies which Creator submitted this bundle
//! - `manuscript_phase`: current manuscript lifecycle phase
//! - `output_manuscript`: whether execution requires manuscript output
//!
//! # Story Manifest Delta Type
//!
//! V1.0 supports `story_manifest` delta type in the deltas array,
//! required for context-assembly summary payloads.

use nexus_contracts::generated::{Bundle, Delta, SourceAnchor};
use nexus_contracts::{BundleType, ManuscriptPhase};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::command::SyncCommandVariant;
use crate::errors::{SyncError, SyncResult};

/// Delta operation within a bundle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LocalDelta {
    /// Target aggregate type for this delta.
    pub delta_type: DeltaType,
    /// Operation to apply.
    pub operation: DeltaOperation,
    /// Sub-type (e.g., 'character' when delta_type='key_block').
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_entity_type: Option<String>,
    /// Target entity ID (null for create).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_entity_id: Option<String>,
    /// Delta payload.
    pub payload: Value,
    /// Optional source anchor for provenance.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_anchor: Option<Value>,
    /// Local timestamp of this delta.
    pub local_timestamp: String,
}

/// Target aggregate type for a delta.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeltaType {
    World,
    KeyBlock,
    TimelineEvent,
    ForkBranch,
    MemoryItem,
    StoryManifest,
}

impl DeltaType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::World => "world",
            Self::KeyBlock => "key_block",
            Self::TimelineEvent => "timeline_event",
            Self::ForkBranch => "fork_branch",
            Self::MemoryItem => "memory_item",
            Self::StoryManifest => "story_manifest",
        }
    }
}

/// Operation to apply for a delta.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeltaOperation {
    Create,
    Update,
    Upsert,
    Delete,
    Append,
}

impl DeltaOperation {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Create => "create",
            Self::Update => "update",
            Self::Upsert => "upsert",
            Self::Delete => "delete",
            Self::Append => "append",
        }
    }
}

/// Bundle builder with fluent API.
///
/// Usage:
/// ```ignore
/// let bundle = BundleBuilder::new("wrk_001", "wld_001", "ctr_001")
///     .submitting_creator_id("ctr_001")
///     .manuscript_phase(ManuscriptPhase::Draft)
///     .output_manuscript(true)
///     .command(&sync_command)
///     .add_delta(delta)
///     .base_world_revision(5)
///     .last_confirmed_delta_sequence(10)
///     .build()?;
/// ```
pub struct BundleBuilder {
    workspace_id: String,
    world_id: String,
    creator_id: String,
    submitting_creator_id: Option<String>,
    manuscript_phase: Option<ManuscriptPhase>,
    output_manuscript: Option<bool>,
    bundle_type: BundleType,
    command_id: Option<String>,
    idempotency_key: Option<String>,
    deltas: Vec<LocalDelta>,
    base_world_revision: Option<u64>,
    base_timeline_head_id: Option<String>,
    base_canon_revision: Option<u64>,
    last_confirmed_delta_sequence: Option<u64>,
}

impl BundleBuilder {
    /// Create a new bundle builder for a world/workspace context.
    pub fn new(workspace_id: &str, world_id: &str, creator_id: &str) -> Self {
        Self {
            workspace_id: workspace_id.to_string(),
            world_id: world_id.to_string(),
            creator_id: creator_id.to_string(),
            submitting_creator_id: None,
            manuscript_phase: None,
            output_manuscript: None,
            bundle_type: BundleType::WorldSync,
            command_id: None,
            idempotency_key: None,
            deltas: Vec::new(),
            base_world_revision: None,
            base_timeline_head_id: None,
            base_canon_revision: None,
            last_confirmed_delta_sequence: None,
        }
    }

    /// Set the submitting creator ID (required for V1.0).
    pub fn submitting_creator_id(mut self, id: &str) -> Self {
        self.submitting_creator_id = Some(id.to_string());
        self
    }

    /// Set the manuscript phase (optional but recommended).
    pub fn manuscript_phase(mut self, phase: ManuscriptPhase) -> Self {
        self.manuscript_phase = Some(phase);
        self
    }

    /// Set whether this execution requires manuscript output.
    pub fn output_manuscript(mut self, output: bool) -> Self {
        self.output_manuscript = Some(output);
        self
    }

    /// Set the bundle type.
    pub fn bundle_type(mut self, bundle_type: BundleType) -> Self {
        self.bundle_type = bundle_type;
        self
    }

    /// Associate a command with this bundle.
    pub fn command(mut self, _command: &SyncCommandVariant) -> Self {
        self.command_id = Some(format!("cmd_{}", Uuid::new_v4().simple()));
        self
    }

    /// Set command ID explicitly.
    pub fn command_id(mut self, id: &str) -> Self {
        self.command_id = Some(id.to_string());
        self
    }

    /// Set the idempotency key.
    pub fn idempotency_key(mut self, key: &str) -> Self {
        self.idempotency_key = Some(key.to_string());
        self
    }

    /// Add a delta to the bundle.
    pub fn add_delta(mut self, delta: LocalDelta) -> Self {
        self.deltas.push(delta);
        self
    }

    /// Add multiple deltas to the bundle.
    pub fn add_deltas(mut self, deltas: Vec<LocalDelta>) -> Self {
        self.deltas.extend(deltas);
        self
    }

    /// Set the base world revision for optimistic concurrency.
    pub fn base_world_revision(mut self, revision: u64) -> Self {
        self.base_world_revision = Some(revision);
        self
    }

    /// Set the base timeline head ID.
    pub fn base_timeline_head_id(mut self, id: &str) -> Self {
        self.base_timeline_head_id = Some(id.to_string());
        self
    }

    /// Set the base canon revision.
    pub fn base_canon_revision(mut self, revision: u64) -> Self {
        self.base_canon_revision = Some(revision);
        self
    }

    /// Set the last confirmed delta sequence.
    pub fn last_confirmed_delta_sequence(mut self, seq: u64) -> Self {
        self.last_confirmed_delta_sequence = Some(seq);
        self
    }

    /// Validate the bundle and build the final `Bundle` envelope.
    pub fn build(self) -> SyncResult<Bundle> {
        // Validate required fields
        if self.deltas.is_empty() {
            return Err(SyncError::BundleEmptyDeltas);
        }

        let submitting_creator_id =
            self.submitting_creator_id
                .ok_or_else(|| SyncError::BundleMissingField {
                    field: "submitting_creator_id".to_string(),
                })?;

        let command_id = self
            .command_id
            .unwrap_or_else(|| format!("cmd_{}", Uuid::new_v4().simple()));

        let idempotency_key = self
            .idempotency_key
            .unwrap_or_else(|| format!("idk_{}", Uuid::new_v4().simple()));

        let bundle_id = format!("bdl_{}", Uuid::new_v4().simple());
        let now = chrono::Utc::now().to_rfc3339();

        // Build base_versions object
        let mut base_versions = json!({});
        if let Some(rev) = self.base_world_revision {
            base_versions["world_revision"] = json!(rev);
        }
        if let Some(id) = self.base_timeline_head_id {
            base_versions["timeline_head_id"] = json!(id);
        }
        if let Some(rev) = self.base_canon_revision {
            base_versions["canon_revision"] = json!(rev);
        }

        // Convert deltas to Delta (generated contract type)
        let delta_values: Vec<Delta> = self
            .deltas
            .into_iter()
            .map(|d| {
                let source_anchor = d
                    .source_anchor
                    .and_then(|v| serde_json::from_value::<SourceAnchor>(v).ok());
                Delta {
                    delta_type: d.delta_type.as_str().to_string(),
                    operation: d.operation.as_str().to_string(),
                    target_entity_type: d.target_entity_type.map(|s| s.to_string()),
                    target_entity_id: d.target_entity_id.map(|s| s.to_string()),
                    payload: d.payload,
                    source_anchor,
                    local_timestamp: d.local_timestamp.to_string(),
                }
            })
            .collect();

        // Compute canonical hash placeholder (V1.0: empty string; real hash TBD)
        let canonical_hash = String::new();

        let bundle = Bundle {
            schema_version: 1,
            bundle_id,
            command_id,
            workspace_id: self.workspace_id,
            world_id: self.world_id,
            creator_id: self.creator_id,
            submitting_creator_id,
            bundle_type: self.bundle_type,
            manuscript_phase: self.manuscript_phase,
            output_manuscript: self.output_manuscript,
            idempotency_key,
            canonical_hash,
            base_versions,
            last_confirmed_delta_sequence: self.last_confirmed_delta_sequence,
            deltas: delta_values,
            bundle_apply_status: None,
            delta_results: None,
            created_at: now,
        };

        tracing::debug!(
            bundle_id = %bundle.bundle_id,
            world_id = %bundle.world_id,
            delta_count = bundle.deltas.len(),
            "Bundle built successfully"
        );

        Ok(bundle)
    }
}

/// Create a story_manifest delta.
pub fn story_manifest_delta(
    summary_text: &str,
    story_manifest_id: &str,
    manifest_type: &str,
) -> LocalDelta {
    LocalDelta {
        delta_type: DeltaType::StoryManifest,
        operation: DeltaOperation::Upsert,
        target_entity_type: Some("story_manifest".to_string()),
        target_entity_id: Some(story_manifest_id.to_string()),
        payload: json!({
            "summary_text": summary_text,
            "manifest_type": manifest_type,
        }),
        source_anchor: None,
        local_timestamp: chrono::Utc::now().to_rfc3339(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_contracts::generated::LATEST_SCHEMA_VERSION;

    fn make_test_delta() -> LocalDelta {
        LocalDelta {
            delta_type: DeltaType::KeyBlock,
            operation: DeltaOperation::Create,
            target_entity_type: Some("character".to_string()),
            target_entity_id: None,
            payload: json!({
                "display_name": "Test Character",
                "block_type": "character",
            }),
            source_anchor: None,
            local_timestamp: chrono::Utc::now().to_rfc3339(),
        }
    }

    #[test]
    fn bundle_build_minimal() {
        let delta = make_test_delta();
        let bundle = BundleBuilder::new("wrk_001", "wld_001", "ctr_001")
            .submitting_creator_id("ctr_001")
            .add_delta(delta)
            .build()
            .expect("should build");

        assert_eq!(bundle.schema_version, LATEST_SCHEMA_VERSION);
        assert_eq!(bundle.workspace_id, "wrk_001");
        assert_eq!(bundle.world_id, "wld_001");
        assert_eq!(bundle.creator_id, "ctr_001");
        assert_eq!(bundle.submitting_creator_id, "ctr_001");
        assert_eq!(bundle.deltas.len(), 1);
        assert!(bundle.bundle_id.starts_with("bdl_"));
        assert!(bundle.command_id.starts_with("cmd_"));
    }

    #[test]
    fn bundle_build_with_metadata() {
        let delta = make_test_delta();
        let bundle = BundleBuilder::new("wrk_001", "wld_001", "ctr_001")
            .submitting_creator_id("ctr_001")
            .manuscript_phase(ManuscriptPhase::Draft)
            .output_manuscript(true)
            .add_delta(delta)
            .base_world_revision(5)
            .last_confirmed_delta_sequence(10)
            .build()
            .expect("should build");

        assert_eq!(bundle.manuscript_phase, Some(ManuscriptPhase::Draft));
        assert_eq!(bundle.output_manuscript, Some(true));
        assert_eq!(bundle.last_confirmed_delta_sequence, Some(10));
        assert_eq!(bundle.base_versions["world_revision"], json!(5));
    }

    #[test]
    fn bundle_build_with_story_manifest_delta() {
        let sm_delta =
            story_manifest_delta("A hero rises from the ashes.", "sm_001", "chapter_summary");
        let bundle = BundleBuilder::new("wrk_001", "wld_001", "ctr_001")
            .submitting_creator_id("ctr_001")
            .add_delta(sm_delta)
            .build()
            .expect("should build");

        assert_eq!(bundle.deltas.len(), 1);
        let delta = &bundle.deltas[0];
        assert_eq!(delta.delta_type, "story_manifest");
        assert_eq!(delta.operation, "upsert");
    }

    #[test]
    fn bundle_empty_deltas_rejected() {
        let result = BundleBuilder::new("wrk_001", "wld_001", "ctr_001")
            .submitting_creator_id("ctr_001")
            .build();

        assert!(matches!(result, Err(SyncError::BundleEmptyDeltas)));
    }

    #[test]
    fn bundle_missing_submitting_creator_rejected() {
        let delta = make_test_delta();
        let result = BundleBuilder::new("wrk_001", "wld_001", "ctr_001")
            .add_delta(delta)
            .build();

        assert!(matches!(result, Err(SyncError::BundleMissingField { .. })));
    }

    #[test]
    fn bundle_serialization_roundtrip() {
        let delta = make_test_delta();
        let bundle = BundleBuilder::new("wrk_001", "wld_001", "ctr_001")
            .submitting_creator_id("ctr_001")
            .manuscript_phase(ManuscriptPhase::Review)
            .bundle_type(BundleType::MemorySync)
            .add_delta(delta)
            .base_world_revision(3)
            .build()
            .expect("should build");

        let json_str = serde_json::to_string(&bundle).expect("serialize");
        let recovered: Bundle = serde_json::from_str(&json_str).expect("deserialize");
        assert_eq!(bundle, recovered);
    }

    #[test]
    fn delta_type_as_str() {
        assert_eq!(DeltaType::World.as_str(), "world");
        assert_eq!(DeltaType::StoryManifest.as_str(), "story_manifest");
    }

    #[test]
    fn delta_operation_as_str() {
        assert_eq!(DeltaOperation::Create.as_str(), "create");
        assert_eq!(DeltaOperation::Upsert.as_str(), "upsert");
    }

    #[test]
    fn bundle_type_enum_serialization() {
        assert_eq!(
            serde_json::to_string(&BundleType::WorldSync).unwrap(),
            "\"world_sync\""
        );
        assert_eq!(
            serde_json::to_string(&BundleType::MemorySync).unwrap(),
            "\"memory_sync\""
        );
    }
}
