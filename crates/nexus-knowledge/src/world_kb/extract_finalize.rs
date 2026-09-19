//! Extract candidate prepare/persist seam — validate + allocate once, then store.
//!
//! V1.40 P3 (T3): shared domain logic for `kb.extract_work` capability.
//! Validates P1 rules (`canonical_name`, body with `novel_category`), then
//! delegates insert to the `KbStore` implementation.
//!
//! # Prepare/persist split (v1.191 P1 T12, durable §8)
//!
//! [`prepare_extract`] runs once per candidate, before anything is persisted:
//! it validates the model output, allocates the candidate's `entry_id` exactly
//! once, and attaches the source anchor plus the **trusted** governance pair
//! resolved from the extraction job policy. [`persist_prepared_extract`] then
//! stores that exact record — it mints no second id and rebuilds no defaults,
//! so the id and governance observed before persistence are the ones observed
//! after it.
//!
//! The holder axis is never model-chosen: the model output reaches only
//! [`ExtractPrepareInput`]'s body/name/block-type fields, and the governance
//! pair enters through [`ExtractPrepareInput::governance`], which the service
//! resolves from the job policy (`.mstar/specs/holder-governance.md` §§3, 8).
//!
//! The caller is responsible for job lifecycle (mark running/done/failed).

use crate::world_kb::knowledge_entry::{
    validate_native_governance, KnowledgeEntryBody, KnowledgeEntryRecord, KnowledgeGovernance,
};
use crate::world_kb::query::KbInsertResult;
use crate::world_kb::source_anchor::SourceAnchor;
use crate::world_kb::store::KbStore;
use crate::world_kb::store::KbStoreError;
use crate::world_kb::validation::{validate_body, validate_canonical_name, ValidationMode};
use nexus_contracts::BlockType;

/// Input for [`prepare_extract`].
///
/// Everything except [`governance`](Self::governance) is model output;
/// `governance` is the trusted job policy and is the only authority for the
/// holder/disclosure pair.
#[derive(Debug, Clone)]
pub struct ExtractPrepareInput {
    /// Target world ID.
    pub world_id: String,
    /// Block type from LLM extraction.
    pub block_type: BlockType,
    /// Canonical name from LLM extraction.
    pub canonical_name: String,
    /// Body content from LLM extraction.
    pub body: KnowledgeEntryBody,
    /// Source anchor for the chapter artifact.
    pub source_anchor: SourceAnchor,
    /// Validation mode: `Novel` for V1.40 novel works, `Generic` otherwise.
    pub validation_mode: ValidationMode,
    /// Governance resolved by the service from the extraction job policy
    /// before the model ran. Model output never reaches this pair.
    pub governance: KnowledgeGovernance,
}

/// A validated extraction candidate awaiting persistence.
///
/// [`record`](Self::record) is final: its `entry_id` was allocated once by
/// [`prepare_extract`], its governance pair came from the trusted policy, and
/// its source anchor is attached. Callers convert it for the wire (the adapter
/// seam) and persist it through [`persist_prepared_extract`] in their own job
/// transaction.
#[derive(Debug, Clone)]
pub struct PreparedExtractCandidate {
    /// The exact aggregate to persist.
    pub record: KnowledgeEntryRecord,
}

/// Validate extraction input and prepare the candidate to persist.
///
/// Steps:
/// 1. Validate `canonical_name` format (P1 grammar rules).
/// 2. Validate `body` per `ValidationMode` (Novel: requires `novel_category`).
/// 3. Validate the trusted governance pair at the seam where it is attached
///    (empty columns and `owner-private` without a holder are refused).
/// 4. Allocate the candidate `entry_id` **once** and attach the source anchor
///    plus the governance pair.
///
/// # Errors
///
/// Returns [`KbStoreError::ValidationLegacy`] when P1 rules, the body mode
/// requirement, or the governance pair fail validation. No id is allocated on
/// a rejected call.
pub fn prepare_extract(
    input: ExtractPrepareInput,
) -> Result<PreparedExtractCandidate, KbStoreError> {
    // Step 1: Validate canonical name.
    validate_canonical_name(&input.canonical_name)
        .map_err(|e| KbStoreError::ValidationLegacy(e.to_string()))?;

    // Step 2: Validate body per mode.
    validate_body(input.block_type, Some(&input.body), input.validation_mode)
        .map_err(|e| KbStoreError::ValidationLegacy(e.to_string()))?;

    // Step 3: Validate the trusted policy before it can reach a wire
    // conversion or a store (durable §3).
    let KnowledgeGovernance {
        holder_entry_id,
        disclosure,
    } = input.governance;
    validate_native_governance(holder_entry_id.as_deref(), disclosure.as_deref())
        .map_err(|e| KbStoreError::ValidationLegacy(e.to_string()))?;

    // Step 4: Allocate the id once and attach the source anchor + governance.
    let mut record =
        KnowledgeEntryRecord::new(&input.world_id, input.block_type, &input.canonical_name);
    record.body = Some(input.body);
    record.source_anchor = Some(input.source_anchor);
    record.holder_entry_id = holder_entry_id;
    record.disclosure = disclosure;

    Ok(PreparedExtractCandidate { record })
}

/// Persist exactly the prepared record.
///
/// No id is allocated and no default is rebuilt here: the prepared candidate's
/// `entry_id`, governance pair and source anchor are written as they are, so
/// the caller's observed ids survive persistence. Store-side validation and
/// uniqueness behave as for any other insert.
///
/// # Errors
///
/// Returns [`KbStoreError::Duplicate`] on uniqueness conflict. Returns other
/// [`KbStoreError`] variants on store failures.
pub async fn persist_prepared_extract<S: KbStore + Sync>(
    store: &S,
    prepared: PreparedExtractCandidate,
) -> Result<KbInsertResult, KbStoreError> {
    store.insert_knowledge_entry(prepared.record).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world_kb::knowledge_entry::{KnowledgeOwnerRef, DISCLOSURE_OWNER_PRIVATE};
    use crate::world_kb::store::InMemoryKbStore;

    fn novel_body() -> KnowledgeEntryBody {
        KnowledgeEntryBody {
            summary: Some("A brave warrior".to_string()),
            attributes: Some(serde_json::json!({
                "novel_category": "character",
                "traits": ["brave"]
            })),
            tags: Some(vec!["novel".to_string()]),
            ..Default::default()
        }
    }

    // WAIVER: pre-1.0 local-first; see V1.41 P-last residual R-V140P3-S4
    // — SourceAnchor::from_excerpt is overloaded (excerpt vs chapter locator);
    // acceptable while only one variant exists; refactor when multi-source anchors needed.
    fn chapter_anchor() -> SourceAnchor {
        SourceAnchor::from_excerpt("Chapter 3 body excerpt")
    }

    fn prepare_input() -> ExtractPrepareInput {
        ExtractPrepareInput {
            world_id: "wld_1".to_string(),
            block_type: BlockType::Character,
            canonical_name: "char_lin_xia".to_string(),
            body: novel_body(),
            source_anchor: chapter_anchor(),
            validation_mode: ValidationMode::Novel,
            governance: KnowledgeGovernance::shared(),
        }
    }

    #[test]
    fn test_prepare_extract_novel_allocates_id_and_attaches_source() {
        let prepared = prepare_extract(prepare_input()).unwrap();
        assert!(prepared.record.entry_id.starts_with("kb_"));
        assert_eq!(prepared.record.owner, KnowledgeOwnerRef::world("wld_1"));
        assert_eq!(prepared.record.status, "provisional");
        assert_eq!(prepared.record.canonical_name, "char_lin_xia");
        assert!(prepared.record.source_anchor.is_some());
        // Shared policy → both governance columns stay absent.
        assert!(prepared.record.holder_entry_id.is_none());
        assert!(prepared.record.disclosure.is_none());
    }

    #[test]
    fn test_prepare_extract_allocates_a_fresh_id_per_call() {
        let first = prepare_extract(prepare_input()).unwrap();
        let second = prepare_extract(prepare_input()).unwrap();
        assert_ne!(
            first.record.entry_id, second.record.entry_id,
            "each prepare allocates its own candidate id"
        );
    }

    #[test]
    fn test_prepare_extract_rejects_empty_canonical_name() {
        let input = ExtractPrepareInput {
            canonical_name: String::new(),
            ..prepare_input()
        };

        let err = prepare_extract(input).unwrap_err();
        assert!(
            matches!(&err, KbStoreError::ValidationLegacy(msg) if msg.contains("canonical_name")),
            "expected canonical_name validation error, got: {err:?}"
        );
    }

    #[test]
    fn test_prepare_extract_rejects_missing_novel_category() {
        let body = KnowledgeEntryBody {
            summary: Some("test".to_string()),
            attributes: Some(serde_json::json!({})),
            tags: None,
            ..Default::default()
        };
        let input = ExtractPrepareInput {
            body,
            ..prepare_input()
        };

        let err = prepare_extract(input).unwrap_err();
        assert!(
            matches!(&err, KbStoreError::ValidationLegacy(msg) if msg.contains("novel_category")),
            "expected novel_category validation error, got: {err:?}"
        );
    }

    #[test]
    fn test_prepare_extract_generic_mode_no_novel_category_required() {
        let body = KnowledgeEntryBody {
            summary: Some("generic entity".to_string()),
            attributes: None,
            tags: None,
            ..Default::default()
        };
        let input = ExtractPrepareInput {
            world_id: "wld_1".to_string(),
            block_type: BlockType::InfoPoint,
            canonical_name: "info_cosmology".to_string(),
            body,
            source_anchor: chapter_anchor(),
            validation_mode: ValidationMode::Generic,
            governance: KnowledgeGovernance::shared(),
        };

        let prepared = prepare_extract(input).unwrap();
        assert!(prepared.record.entry_id.starts_with("kb_"));
    }

    #[test]
    fn test_prepare_extract_takes_holders_from_the_policy_not_the_model_output() {
        // Model output rides body/name/block type only. Even a body whose
        // attributes carry governance-shaped keys cannot choose the holder.
        let mut input = prepare_input();
        input.body = KnowledgeEntryBody {
            summary: Some("A brave warrior".to_string()),
            attributes: Some(serde_json::json!({
                "novel_category": "character",
                "holder_entry_id": "hld_model_choice",
                "disclosure": DISCLOSURE_OWNER_PRIVATE,
            })),
            tags: None,
            ..Default::default()
        };
        input.governance = KnowledgeGovernance {
            holder_entry_id: Some("hld_job_policy".to_string()),
            disclosure: Some(DISCLOSURE_OWNER_PRIVATE.to_string()),
        };

        let prepared = prepare_extract(input).unwrap();
        assert_eq!(
            prepared.record.holder_entry_id.as_deref(),
            Some("hld_job_policy")
        );
        assert_eq!(
            prepared.record.disclosure.as_deref(),
            Some(DISCLOSURE_OWNER_PRIVATE)
        );
    }

    #[test]
    fn test_prepare_extract_rejects_invalid_governance_policy() {
        let empty_holder = ExtractPrepareInput {
            governance: KnowledgeGovernance {
                holder_entry_id: Some(String::new()),
                disclosure: Some(DISCLOSURE_OWNER_PRIVATE.to_string()),
            },
            ..prepare_input()
        };
        let err = prepare_extract(empty_holder).unwrap_err();
        assert!(
            matches!(&err, KbStoreError::ValidationLegacy(msg) if msg.contains("holder_entry_id")),
            "expected empty holder rejection, got: {err:?}"
        );

        let private_without_holder = ExtractPrepareInput {
            governance: KnowledgeGovernance {
                holder_entry_id: None,
                disclosure: Some(DISCLOSURE_OWNER_PRIVATE.to_string()),
            },
            ..prepare_input()
        };
        let err = prepare_extract(private_without_holder).unwrap_err();
        assert!(
            matches!(&err, KbStoreError::ValidationLegacy(msg) if msg.contains("nonempty holder_entry_id")),
            "expected owner-private-without-holder rejection, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn test_persist_prepared_extract_stores_the_exact_record_without_minting() {
        let store = InMemoryKbStore::with_validation_mode(ValidationMode::Novel);
        let mut prepared = prepare_extract(prepare_input()).unwrap();
        // Provable no-mint: persist stores the record it is handed, including
        // an id it did not allocate and governance it did not rebuild.
        prepared.record.entry_id = "kb_t12_fixed".to_string();
        prepared.record.holder_entry_id = Some("hld_t12".to_string());
        prepared.record.disclosure = Some(DISCLOSURE_OWNER_PRIVATE.to_string());

        let result = persist_prepared_extract(&store, prepared).await.unwrap();
        assert_eq!(result.entry_id, "kb_t12_fixed");
        assert_eq!(result.owner, KnowledgeOwnerRef::world("wld_1"));

        let stored = store.get_knowledge_entry("kb_t12_fixed").await.unwrap();
        assert_eq!(stored.entry_id, "kb_t12_fixed");
        assert_eq!(stored.holder_entry_id.as_deref(), Some("hld_t12"));
        assert_eq!(stored.disclosure.as_deref(), Some(DISCLOSURE_OWNER_PRIVATE));
        assert!(stored.source_anchor.is_some());
    }

    #[tokio::test]
    async fn test_persist_prepared_extract_duplicate_rejected() {
        let store = InMemoryKbStore::with_validation_mode(ValidationMode::Novel);

        let first = prepare_extract(prepare_input()).unwrap();
        let first_id = first.record.entry_id.clone();
        let result = persist_prepared_extract(&store, first).await.unwrap();
        assert_eq!(result.entry_id, first_id);

        let second = prepare_extract(prepare_input()).unwrap();
        let err = persist_prepared_extract(&store, second).await.unwrap_err();
        assert!(
            matches!(err, KbStoreError::Duplicate { .. }),
            "expected duplicate error, got: {err:?}"
        );
    }
}
