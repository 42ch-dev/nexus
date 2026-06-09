//! Extract finalize helpers — validate + upsert KeyBlock + SourceAnchor.
//!
//! V1.40 P3 (T3): shared domain logic for `kb.extract_work` capability.
//! Validates P1 rules (canonical_name, body with novel_category), then
//! delegates insert to the `KbStore` implementation.
//!
//! The caller is responsible for job lifecycle (mark running/done/failed).

use crate::key_block::{KeyBlock, KeyBlockBody};
use crate::query::KbInsertResult;
use crate::source_anchor::SourceAnchor;
use crate::store::KbStore;
use crate::store::KbStoreError;
use crate::validation::{validate_body, validate_canonical_name, ValidationMode};
use nexus_contracts::BlockType;

/// Input for `finalize_extract`.
#[derive(Debug, Clone)]
pub struct ExtractFinalizeInput {
    /// Target world ID.
    pub world_id: String,
    /// Block type from LLM extraction.
    pub block_type: BlockType,
    /// Canonical name from LLM extraction.
    pub canonical_name: String,
    /// Body content from LLM extraction.
    pub body: KeyBlockBody,
    /// Source anchor for the chapter artifact.
    pub source_anchor: SourceAnchor,
    /// Validation mode: `Novel` for V1.40 novel works, `Generic` otherwise.
    pub validation_mode: ValidationMode,
}

/// Validate extraction input and insert a `KeyBlock` via the provided store.
///
/// Steps:
/// 1. Validate `canonical_name` format (P1 grammar rules).
/// 2. Validate `body` per `ValidationMode` (Novel: requires `novel_category`).
/// 3. Construct `KeyBlock` with provisional status.
/// 4. Insert via `KbStore::insert_key_block`.
///
/// # Errors
///
/// Returns [`KbStoreError::Validation`] when P1 rules fail.
/// Returns [`KbStoreError::Duplicate`] on uniqueness conflict.
/// Returns other [`KbStoreError`] variants on store failures.
pub async fn finalize_extract<S: KbStore>(
    store: &S,
    input: ExtractFinalizeInput,
) -> Result<KbInsertResult, KbStoreError> {
    // Step 1: Validate canonical name.
    validate_canonical_name(&input.canonical_name)
        .map_err(|e| KbStoreError::ValidationLegacy(e.to_string()))?;

    // Step 2: Validate body per mode.
    validate_body(input.block_type, Some(&input.body), input.validation_mode)
        .map_err(|e| KbStoreError::ValidationLegacy(e.to_string()))?;

    // Step 3: Build KeyBlock.
    let mut kb = KeyBlock::new(&input.world_id, input.block_type, &input.canonical_name);
    kb.body = Some(input.body);
    kb.source_anchor = Some(input.source_anchor);

    // Step 4: Insert.
    store.insert_key_block(kb).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::InMemoryKbStore;

    fn novel_body() -> KeyBlockBody {
        KeyBlockBody {
            summary: Some("A brave warrior".to_string()),
            attributes: Some(serde_json::json!({
                "novel_category": "character",
                "traits": ["brave"]
            })),
            tags: Some(vec!["novel".to_string()]),
        }
    }

    fn chapter_anchor() -> SourceAnchor {
        SourceAnchor::from_excerpt("Chapter 3 body excerpt")
    }

    #[tokio::test]
    async fn test_finalize_extract_novel() {
        let store = InMemoryKbStore::with_validation_mode(ValidationMode::Novel);
        let input = ExtractFinalizeInput {
            world_id: "wld_1".to_string(),
            block_type: BlockType::Character,
            canonical_name: "char_lin_xia".to_string(),
            body: novel_body(),
            source_anchor: chapter_anchor(),
            validation_mode: ValidationMode::Novel,
        };

        let result = finalize_extract(&store, input).await.unwrap();
        assert!(result.key_block_id.starts_with("kb_"));
        assert_eq!(result.world_id, "wld_1");
    }

    #[tokio::test]
    async fn test_finalize_extract_rejects_empty_canonical_name() {
        let store = InMemoryKbStore::with_validation_mode(ValidationMode::Novel);
        let input = ExtractFinalizeInput {
            world_id: "wld_1".to_string(),
            block_type: BlockType::Character,
            canonical_name: String::new(),
            body: novel_body(),
            source_anchor: chapter_anchor(),
            validation_mode: ValidationMode::Novel,
        };

        let err = finalize_extract(&store, input).await.unwrap_err();
        assert!(
            matches!(&err, KbStoreError::ValidationLegacy(msg) if msg.contains("canonical_name")),
            "expected canonical_name validation error, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn test_finalize_extract_rejects_missing_novel_category() {
        let store = InMemoryKbStore::with_validation_mode(ValidationMode::Novel);
        let body = KeyBlockBody {
            summary: Some("test".to_string()),
            attributes: Some(serde_json::json!({})),
            tags: None,
        };
        let input = ExtractFinalizeInput {
            world_id: "wld_1".to_string(),
            block_type: BlockType::Character,
            canonical_name: "char_test".to_string(),
            body,
            source_anchor: chapter_anchor(),
            validation_mode: ValidationMode::Novel,
        };

        let err = finalize_extract(&store, input).await.unwrap_err();
        assert!(
            matches!(&err, KbStoreError::ValidationLegacy(msg) if msg.contains("novel_category")),
            "expected novel_category validation error, got: {err:?}"
        );
    }

    #[tokio::test]
    async fn test_finalize_extract_generic_mode_no_novel_category_required() {
        let store = InMemoryKbStore::new();
        let body = KeyBlockBody {
            summary: Some("generic entity".to_string()),
            attributes: None,
            tags: None,
        };
        let input = ExtractFinalizeInput {
            world_id: "wld_1".to_string(),
            block_type: BlockType::InfoPoint,
            canonical_name: "info_cosmology".to_string(),
            body,
            source_anchor: chapter_anchor(),
            validation_mode: ValidationMode::Generic,
        };

        let result = finalize_extract(&store, input).await.unwrap();
        assert!(result.key_block_id.starts_with("kb_"));
    }

    #[tokio::test]
    async fn test_finalize_extract_duplicate_rejected() {
        let store = InMemoryKbStore::with_validation_mode(ValidationMode::Novel);
        let input = ExtractFinalizeInput {
            world_id: "wld_1".to_string(),
            block_type: BlockType::Character,
            canonical_name: "char_unique".to_string(),
            body: novel_body(),
            source_anchor: chapter_anchor(),
            validation_mode: ValidationMode::Novel,
        };

        let _ = finalize_extract(&store, input.clone()).await.unwrap();
        let err = finalize_extract(&store, input).await.unwrap_err();
        assert!(
            matches!(err, KbStoreError::Duplicate { .. }),
            "expected duplicate error, got: {err:?}"
        );
    }
}
