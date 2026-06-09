//! Validation helpers for wire `BlockType` and novel-profile `body` semantics.
//!
//! Per entity-scope-model.md §5.1.1 (V1.40 grill-me locked):
//! - `BlockType` wire enum (`nexus_contracts::BlockType`) is the single SSOT.
//! - Novel "seven categories" are carried in `body.attributes.novel_category`.
//! - When `ValidationMode::Novel`, certain `BlockType`s require `novel_category`.

use crate::errors::KbError;
use crate::key_block::KeyBlockBody;
use nexus_contracts::BlockType;
use std::fmt;

/// Valid `novel_category` values (body layer, NOT a second block_type).
///
/// Per entity-scope-model.md §5.1.1 mapping table:
///
/// | `novel_category`   | Default wire `block_type` |
/// |--------------------|---------------------------|
/// | `foundation`       | `info_point`              |
/// | `background`       | `event`                   |
/// | `character`        | `character`               |
/// | `location`         | `scene`                   |
/// | `society`          | `organization`            |
/// | `rules`            | `conflict`                |
/// | `economy`          | `item`                    |
pub const NOVEL_CATEGORIES: &[&str] = &[
    "foundation",
    "background",
    "character",
    "location",
    "society",
    "rules",
    "economy",
];

/// Validation mode controlling how strictly `body` is checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationMode {
    /// Standard validation — only structural checks on `body`.
    Generic,
    /// Novel profile validation — requires `novel_category` in `body.attributes`
    /// and validates it against the mapping table.
    Novel,
}

impl fmt::Display for ValidationMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Generic => write!(f, "generic"),
            Self::Novel => write!(f, "novel"),
        }
    }
}

/// Check whether a string is a valid `novel_category`.
#[must_use]
pub fn is_valid_novel_category(category: &str) -> bool {
    NOVEL_CATEGORIES.contains(&category)
}

/// Validate a `KeyBlockBody` for the given `BlockType` and `ValidationMode`.
///
/// # Errors
///
/// Returns `KbError::ValidationError` when:
/// - In `Novel` mode, `body` is `None`.
/// - In `Novel` mode, `body.attributes` is missing or not an object.
/// - In `Novel` mode, `body.attributes.novel_category` is missing or not a string.
/// - In `Novel` mode, `body.attributes.novel_category` is not one of the seven
///   valid values.
pub fn validate_body(
    block_type: BlockType,
    body: Option<&KeyBlockBody>,
    mode: ValidationMode,
) -> Result<(), KbError> {
    // Structural checks that apply regardless of mode
    if let Some(b) = body {
        if let Some(ref attrs) = b.attributes {
            if !attrs.is_object() {
                return Err(KbError::ValidationError(
                    "body.attributes must be a JSON object".to_string(),
                ));
            }
        }
    }

    if mode != ValidationMode::Novel {
        return Ok(());
    }

    // Novel-mode checks
    let b = body.ok_or_else(|| {
        KbError::ValidationError("body is required for novel-profile KeyBlocks".to_string())
    })?;

    let attrs = b.attributes.as_ref().ok_or_else(|| {
        KbError::ValidationError(
            "body.attributes is required for novel-profile KeyBlocks".to_string(),
        )
    })?;

    let category_value = attrs
        .get("novel_category")
        .ok_or_else(|| {
            KbError::ValidationError(
                "body.attributes.novel_category is required for novel-profile KeyBlocks"
                    .to_string(),
            )
        })?;

    let category = category_value.as_str().ok_or_else(|| {
        KbError::ValidationError(
            "body.attributes.novel_category must be a string".to_string(),
        )
    })?;

    if !is_valid_novel_category(category) {
        return Err(KbError::ValidationError(format!(
            "invalid novel_category '{}': must be one of {:?}",
            category, NOVEL_CATEGORIES
        )));
    }

    // Advisory: warn (via log, not error) if the novel_category doesn't map
    // to the default block_type. This is not an error because multiple
    // categories may map to the same block_type and the user may intentionally
    // override.
    let _ = block_type; // used only for advisory mapping (not enforced)

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_body(novel_category: Option<&str>) -> KeyBlockBody {
        KeyBlockBody {
            summary: Some("test".to_string()),
            attributes: novel_category.map(|cat| {
                serde_json::json!({
                    "novel_category": cat,
                    "traits": ["brave"]
                })
            }),
            tags: Some(vec!["novel".to_string()]),
        }
    }

    fn make_body_without_category() -> KeyBlockBody {
        KeyBlockBody {
            summary: Some("test".to_string()),
            attributes: Some(serde_json::json!({"traits": ["brave"]})),
            tags: None,
        }
    }

    // ── Generic mode ──────────────────────────────────────────────

    #[test]
    fn generic_mode_accepts_none_body() {
        assert!(validate_body(BlockType::Character, None, ValidationMode::Generic).is_ok());
    }

    #[test]
    fn generic_mode_accepts_body_without_attributes() {
        let body = KeyBlockBody {
            summary: Some("test".to_string()),
            attributes: None,
            tags: None,
        };
        assert!(validate_body(BlockType::Character, Some(&body), ValidationMode::Generic).is_ok());
    }

    #[test]
    fn generic_mode_rejects_non_object_attributes() {
        let body = KeyBlockBody {
            summary: None,
            attributes: Some(serde_json::json!("not an object")),
            tags: None,
        };
        assert!(validate_body(BlockType::Character, Some(&body), ValidationMode::Generic).is_err());
    }

    // ── Novel mode: happy paths ──────────────────────────────────

    #[test]
    fn novel_mode_accepts_all_seven_categories() {
        let block_types = [
            BlockType::InfoPoint,   // foundation
            BlockType::Event,       // background
            BlockType::Character,   // character
            BlockType::Scene,       // location
            BlockType::Organization, // society
            BlockType::Conflict,    // rules
            BlockType::Item,        // economy
        ];

        for (i, bt) in block_types.iter().enumerate() {
            let body = make_body(Some(NOVEL_CATEGORIES[i]));
            assert!(
                validate_body(*bt, Some(&body), ValidationMode::Novel).is_ok(),
                "novel_category '{}' with block_type {:?} should pass",
                NOVEL_CATEGORIES[i],
                bt
            );
        }
    }

    // ── Novel mode: error paths ──────────────────────────────────

    #[test]
    fn novel_mode_rejects_none_body() {
        let result = validate_body(BlockType::Character, None, ValidationMode::Novel);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("body is required"));
    }

    #[test]
    fn novel_mode_rejects_missing_attributes() {
        let body = KeyBlockBody {
            summary: Some("test".to_string()),
            attributes: None,
            tags: None,
        };
        let result = validate_body(BlockType::Character, Some(&body), ValidationMode::Novel);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("body.attributes is required"));
    }

    #[test]
    fn novel_mode_rejects_missing_novel_category() {
        let body = make_body_without_category();
        let result = validate_body(BlockType::Character, Some(&body), ValidationMode::Novel);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("novel_category is required"));
    }

    #[test]
    fn novel_mode_rejects_invalid_novel_category() {
        let body = make_body(Some("invalid_category"));
        let result = validate_body(BlockType::Character, Some(&body), ValidationMode::Novel);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("invalid novel_category"));
    }

    #[test]
    fn novel_mode_rejects_non_string_novel_category() {
        let body = KeyBlockBody {
            summary: Some("test".to_string()),
            attributes: Some(serde_json::json!({"novel_category": 42})),
            tags: None,
        };
        let result = validate_body(BlockType::Character, Some(&body), ValidationMode::Novel);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("must be a string"));
    }

    // ── Utility ──────────────────────────────────────────────────

    #[test]
    fn is_valid_novel_category_true_for_all_seven() {
        for cat in NOVEL_CATEGORIES {
            assert!(is_valid_novel_category(cat), "expected '{}' valid", cat);
        }
    }

    #[test]
    fn is_valid_novel_category_false_for_unknown() {
        assert!(!is_valid_novel_category("unknown"));
        assert!(!is_valid_novel_category("Character")); // case-sensitive
    }

    #[test]
    fn validation_mode_display() {
        assert_eq!(ValidationMode::Generic.to_string(), "generic");
        assert_eq!(ValidationMode::Novel.to_string(), "novel");
    }
}
