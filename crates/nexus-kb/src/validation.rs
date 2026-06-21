//! Validation helpers for wire `BlockType` and novel-profile `body` semantics.
//!
//! Per entity-scope-model.md §5.1.1 (V1.40 grill-me locked):
//! - `BlockType` wire enum (`nexus_contracts::BlockType`) is the single SSOT.
//! - Novel "seven categories" are carried in `body.attributes.novel_category`.
//! - When `ValidationMode::Novel`, certain `BlockType`s require `novel_category`.

use crate::errors::{KbError, ValidationError, ValidationKind};
use crate::key_block::KeyBlockBody;
use nexus_contracts::BlockType;
use std::fmt;

/// Valid `novel_category` values (body layer, NOT a second `block_type`).
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
///
/// **DRIFT RISK (R-V140P1-S1):** This list is duplicated in the kb-extract prompt
/// (`embedded-presets/kb-extract/prompts/extract.md`). When updating, update both
/// locations to prevent validation-vs-prompt drift.
pub const NOVEL_CATEGORIES: &[&str] = &[
    "foundation",
    "background",
    "character",
    "location",
    "society",
    "rules",
    "economy",
];

/// Default mapping from `novel_category` to `BlockType`.
///
/// Used for advisory warnings when the provided `block_type` differs
/// from the default for a given `novel_category`.
pub fn default_block_type_for_category(category: &str) -> Option<BlockType> {
    match category {
        "foundation" => Some(BlockType::InfoPoint),
        "background" => Some(BlockType::Event),
        "character" => Some(BlockType::Character),
        "location" => Some(BlockType::Scene),
        "society" => Some(BlockType::Organization),
        "rules" => Some(BlockType::Conflict),
        "economy" => Some(BlockType::Item),
        _ => None,
    }
}

/// Maximum allowed length for `canonical_name`.
pub const CANONICAL_NAME_MAX_LEN: usize = 256;

/// Characters forbidden in `canonical_name`.
const FORBIDDEN_CHARS: &[char] = &[
    '/', '\\', '`', '$', ';', '&', '|', '>', '<', '!', '*', '?', '"', '\'', '(', ')', '{', '}',
    '[', ']', '#',
];

/// Valid `game_bible_category` values (V1.54 P1).
///
/// Per entity-scope-model.md §5.1.1 game-bible taxonomy:
///
/// | `game_bible_category` | Default wire `block_type` |
/// |-----------------------|---------------------------|
/// | `species`             | `species`                 |
/// | `faction`             | `faction`                 |
/// | `magic_system`        | `magic_system`            |
/// | `technology`          | `technology`              |
/// | `deity`               | `deity`                   |
/// | `level`               | `level`                   |
/// | `economy_tier`        | `economy_tier`            |
pub const GAME_BIBLE_CATEGORIES: &[&str] = &[
    "species",
    "faction",
    "magic_system",
    "technology",
    "deity",
    "level",
    "economy_tier",
];

/// Valid `script_category` values (V1.55 P3).
///
/// Per entity-scope-model.md §5.1.1 script taxonomy:
///
/// | `script_category` | Default wire `block_type` |
/// |-------------------|---------------------------|
/// | `dialogue`        | `dialogue`                |
/// | `beat`            | `beat`                    |
/// | `act`             | `act`                     |
pub const SCRIPT_CATEGORIES: &[&str] = &["dialogue", "beat", "act"];

/// Validation mode controlling how strictly `body` is checked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationMode {
    /// Standard validation — only structural checks on `body`.
    Generic,
    /// Novel profile validation — requires `novel_category` in `body.attributes`
    /// and validates it against the mapping table.
    Novel,
    /// Game-bible profile validation (V1.54 P1) — requires `game_bible_category`
    /// in `body.attributes` and validates it against the mapping table.
    /// Rejects `novel_category` when active.
    GameBible,
    /// Script profile validation (V1.55 P3) — requires `script_category`
    /// in `body.attributes` and validates it against the mapping table.
    /// Rejects `novel_category` and `game_bible_category` when active.
    Script,
}

impl fmt::Display for ValidationMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Generic => write!(f, "generic"),
            Self::Novel => write!(f, "novel"),
            Self::GameBible => write!(f, "game_bible"),
            Self::Script => write!(f, "script"),
        }
    }
}

/// Check whether a string is a valid `game_bible_category` (V1.54 P1).
#[must_use]
pub fn is_valid_game_bible_category(category: &str) -> bool {
    GAME_BIBLE_CATEGORIES.contains(&category)
}

/// Default mapping from `game_bible_category` to `BlockType` (V1.54 P1).
pub fn default_block_type_for_game_bible_category(category: &str) -> Option<BlockType> {
    match category {
        "species" => Some(BlockType::Species),
        "faction" => Some(BlockType::Faction),
        "magic_system" => Some(BlockType::MagicSystem),
        "technology" => Some(BlockType::Technology),
        "deity" => Some(BlockType::Deity),
        "level" => Some(BlockType::Level),
        "economy_tier" => Some(BlockType::EconomyTier),
        _ => None,
    }
}

/// Check whether a string is a valid `script_category` (V1.55 P3).
#[must_use]
pub fn is_valid_script_category(category: &str) -> bool {
    SCRIPT_CATEGORIES.contains(&category)
}

/// Default mapping from `script_category` to `BlockType` (V1.55 P3).
#[must_use]
pub fn default_block_type_for_script_category(category: &str) -> Option<BlockType> {
    match category {
        "dialogue" => Some(BlockType::Dialogue),
        "beat" => Some(BlockType::Beat),
        "act" => Some(BlockType::Act),
        _ => None,
    }
}

/// Check whether a string is a valid `novel_category`.
#[must_use]
pub fn is_valid_novel_category(category: &str) -> bool {
    NOVEL_CATEGORIES.contains(&category)
}

/// Validate `canonical_name` format and safety.
///
/// WAIVER: pre-1.0 local-first; see V1.41 P-last residual R-V140P1-S5
/// — String allocations in validation error paths (format! per error) are
/// acceptable for non-hot-path validation; optimization deferred.
///
/// Rejects:
/// - Empty strings
/// - Control characters (codepoints 0x00–0x1F, 0x7F)
/// - Path separators (`/`, `\`)
/// - Shell metacharacters (`` ` `$`; & | > < ! * ? " ' ( ) { } [ ] # ``)
/// - Excessive length (> 256 chars)
///
/// # Errors
///
/// Returns [`KbError::Validation`] with [`ValidationKind::InvalidCanonicalName`]
/// when the name violates any rule.
pub fn validate_canonical_name(name: &str) -> Result<(), KbError> {
    if name.is_empty() {
        return Err(KbError::Validation(ValidationError {
            kind: ValidationKind::InvalidCanonicalName,
            field: Some("canonical_name".to_string()),
            message: "canonical_name must not be empty".to_string(),
        }));
    }

    if name.len() > CANONICAL_NAME_MAX_LEN {
        return Err(KbError::Validation(ValidationError {
            kind: ValidationKind::InvalidCanonicalName,
            field: Some("canonical_name".to_string()),
            message: format!(
                "canonical_name exceeds max length ({} > {})",
                name.len(),
                CANONICAL_NAME_MAX_LEN
            ),
        }));
    }

    // Check for control characters
    if let Some((pos, _)) = name.char_indices().find(|(_, c)| c.is_control()) {
        return Err(KbError::Validation(ValidationError {
            kind: ValidationKind::InvalidCanonicalName,
            field: Some("canonical_name".to_string()),
            message: format!("canonical_name contains control character at position {pos}"),
        }));
    }

    // Check for forbidden characters
    if let Some(fc) = name.chars().find(|c| FORBIDDEN_CHARS.contains(c)) {
        return Err(KbError::Validation(ValidationError {
            kind: ValidationKind::InvalidCanonicalName,
            field: Some("canonical_name".to_string()),
            message: format!(
                "canonical_name contains forbidden character '{}'",
                fc.escape_default()
            ),
        }));
    }

    Ok(())
}

/// Validate a `KeyBlockBody` for the given `BlockType` and `ValidationMode`.
///
/// # Errors
///
/// Returns [`KbError::Validation`] when:
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
                return Err(KbError::Validation(ValidationError {
                    kind: ValidationKind::NonObjectAttributes,
                    field: Some("body.attributes".to_string()),
                    message: "body.attributes must be a JSON object".to_string(),
                }));
            }
        }
    }

    match mode {
        ValidationMode::Generic => Ok(()),
        ValidationMode::Novel => validate_novel_body(block_type, body),
        ValidationMode::GameBible => validate_game_bible_body(block_type, body),
        ValidationMode::Script => validate_script_body(block_type, body),
    }
}

/// Validate novel-profile `body` semantics (V1.40 P1).
fn validate_novel_body(block_type: BlockType, body: Option<&KeyBlockBody>) -> Result<(), KbError> {
    let b = body.ok_or_else(|| {
        KbError::Validation(ValidationError {
            kind: ValidationKind::MissingBody,
            field: Some("body".to_string()),
            message: "body is required for novel-profile KeyBlocks".to_string(),
        })
    })?;

    let attrs = b.attributes.as_ref().ok_or_else(|| {
        KbError::Validation(ValidationError {
            kind: ValidationKind::MissingAttributes,
            field: Some("body.attributes".to_string()),
            message: "body.attributes is required for novel-profile KeyBlocks".to_string(),
        })
    })?;

    let category_value = attrs.get("novel_category").ok_or_else(|| {
        KbError::Validation(ValidationError {
            kind: ValidationKind::MissingNovelCategory,
            field: Some("body.attributes.novel_category".to_string()),
            message: "body.attributes.novel_category is required for novel-profile KeyBlocks"
                .to_string(),
        })
    })?;

    let category = category_value.as_str().ok_or_else(|| {
        KbError::Validation(ValidationError {
            kind: ValidationKind::NonStringNovelCategory,
            field: Some("body.attributes.novel_category".to_string()),
            message: "body.attributes.novel_category must be a string".to_string(),
        })
    })?;

    if !is_valid_novel_category(category) {
        return Err(KbError::Validation(ValidationError {
            kind: ValidationKind::InvalidNovelCategory,
            field: Some("body.attributes.novel_category".to_string()),
            message: format!(
                "invalid novel_category '{}': must be one of {:?}",
                category, NOVEL_CATEGORIES
            ),
        }));
    }

    // Advisory: warn if the novel_category doesn't map to the default block_type.
    if let Some(default_bt) = default_block_type_for_category(category) {
        if block_type != default_bt {
            tracing::warn!(
                novel_category = category,
                provided_block_type = ?block_type,
                default_block_type = ?default_bt,
                "novel_category '{}' does not map to default block_type {:?} \
                 (provided {:?}); this is advisory, not an error",
                category,
                default_bt,
                block_type
            );
        }
    }

    Ok(())
}

/// Validate game-bible profile `body` semantics (V1.54 P1).
///
/// Requires `game_bible_category` in `body.attributes` and validates it
/// against the seven valid values. Rejects `novel_category` if present.
fn validate_game_bible_body(
    block_type: BlockType,
    body: Option<&KeyBlockBody>,
) -> Result<(), KbError> {
    let b = body.ok_or_else(|| {
        KbError::Validation(ValidationError {
            kind: ValidationKind::MissingBody,
            field: Some("body".to_string()),
            message: "body is required for game-bible-profile KeyBlocks".to_string(),
        })
    })?;

    let attrs = b.attributes.as_ref().ok_or_else(|| {
        KbError::Validation(ValidationError {
            kind: ValidationKind::MissingAttributes,
            field: Some("body.attributes".to_string()),
            message: "body.attributes is required for game-bible-profile KeyBlocks".to_string(),
        })
    })?;

    // Reject novel_category in game-bible mode — profile categories must not leak
    if attrs.get("novel_category").is_some() {
        return Err(KbError::Validation(ValidationError {
            kind: ValidationKind::InvalidNovelCategory,
            field: Some("body.attributes.novel_category".to_string()),
            message: "body.attributes.novel_category is not valid for game-bible-profile KeyBlocks"
                .to_string(),
        }));
    }

    let category_value = attrs.get("game_bible_category").ok_or_else(|| {
        KbError::Validation(ValidationError {
            kind: ValidationKind::MissingGameBibleCategory,
            field: Some("body.attributes.game_bible_category".to_string()),
            message:
                "body.attributes.game_bible_category is required for game-bible-profile KeyBlocks"
                    .to_string(),
        })
    })?;

    let category = category_value.as_str().ok_or_else(|| {
        KbError::Validation(ValidationError {
            kind: ValidationKind::NonStringGameBibleCategory,
            field: Some("body.attributes.game_bible_category".to_string()),
            message: "body.attributes.game_bible_category must be a string".to_string(),
        })
    })?;

    if !is_valid_game_bible_category(category) {
        return Err(KbError::Validation(ValidationError {
            kind: ValidationKind::InvalidGameBibleCategory,
            field: Some("body.attributes.game_bible_category".to_string()),
            message: format!(
                "invalid game_bible_category '{}': must be one of {:?}",
                category, GAME_BIBLE_CATEGORIES
            ),
        }));
    }

    // Advisory: warn if the game_bible_category doesn't map to the default block_type.
    if let Some(default_bt) = default_block_type_for_game_bible_category(category) {
        if block_type != default_bt {
            tracing::warn!(
                game_bible_category = category,
                provided_block_type = ?block_type,
                default_block_type = ?default_bt,
                "game_bible_category '{}' does not map to default block_type {:?} \
                 (provided {:?}); this is advisory, not an error",
                category,
                default_bt,
                block_type
            );
        }
    }

    Ok(())
}

/// Validate script profile `body` semantics (V1.55 P3).
///
/// Requires `script_category` in `body.attributes` and validates it
/// against the three valid values. Rejects `novel_category` and
/// `game_bible_category` if present.
fn validate_script_body(block_type: BlockType, body: Option<&KeyBlockBody>) -> Result<(), KbError> {
    let b = body.ok_or_else(|| {
        KbError::Validation(ValidationError {
            kind: ValidationKind::MissingBody,
            field: Some("body".to_string()),
            message: "body is required for script-profile KeyBlocks".to_string(),
        })
    })?;

    let attrs = b.attributes.as_ref().ok_or_else(|| {
        KbError::Validation(ValidationError {
            kind: ValidationKind::MissingAttributes,
            field: Some("body.attributes".to_string()),
            message: "body.attributes is required for script-profile KeyBlocks".to_string(),
        })
    })?;

    // Reject novel_category and game_bible_category in script mode — profile categories must not leak
    if attrs.get("novel_category").is_some() {
        return Err(KbError::Validation(ValidationError {
            kind: ValidationKind::InvalidNovelCategory,
            field: Some("body.attributes.novel_category".to_string()),
            message: "body.attributes.novel_category is not valid for script-profile KeyBlocks"
                .to_string(),
        }));
    }
    if attrs.get("game_bible_category").is_some() {
        return Err(KbError::Validation(ValidationError {
            kind: ValidationKind::InvalidGameBibleCategory,
            field: Some("body.attributes.game_bible_category".to_string()),
            message:
                "body.attributes.game_bible_category is not valid for script-profile KeyBlocks"
                    .to_string(),
        }));
    }

    let category_value = attrs.get("script_category").ok_or_else(|| {
        KbError::Validation(ValidationError {
            kind: ValidationKind::MissingScriptCategory,
            field: Some("body.attributes.script_category".to_string()),
            message: "body.attributes.script_category is required for script-profile KeyBlocks"
                .to_string(),
        })
    })?;

    let category = category_value.as_str().ok_or_else(|| {
        KbError::Validation(ValidationError {
            kind: ValidationKind::NonStringScriptCategory,
            field: Some("body.attributes.script_category".to_string()),
            message: "body.attributes.script_category must be a string".to_string(),
        })
    })?;

    if !is_valid_script_category(category) {
        return Err(KbError::Validation(ValidationError {
            kind: ValidationKind::InvalidScriptCategory,
            field: Some("body.attributes.script_category".to_string()),
            message: format!(
                "invalid script_category '{}': must be one of {:?}",
                category, SCRIPT_CATEGORIES
            ),
        }));
    }

    // Advisory: warn if the script_category doesn't map to the default block_type.
    if let Some(default_bt) = default_block_type_for_script_category(category) {
        if block_type != default_bt {
            tracing::warn!(
                script_category = category,
                provided_block_type = ?block_type,
                default_block_type = ?default_bt,
                "script_category '{}' does not map to default block_type {:?} \
                 (provided {:?}); this is advisory, not an error",
                category,
                default_bt,
                block_type
            );
        }
    }

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
            BlockType::InfoPoint,    // foundation
            BlockType::Event,        // background
            BlockType::Character,    // character
            BlockType::Scene,        // location
            BlockType::Organization, // society
            BlockType::Conflict,     // rules
            BlockType::Item,         // economy
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

    // ── Structured error verification ─────────────────────────────

    #[test]
    fn novel_missing_body_returns_structured_kind() {
        let err = validate_body(BlockType::Character, None, ValidationMode::Novel).unwrap_err();
        match err {
            KbError::Validation(ve) => {
                assert_eq!(ve.kind, ValidationKind::MissingBody);
                assert_eq!(ve.field.as_deref(), Some("body"));
            }
            other => panic!("expected structured Validation, got: {other}"),
        }
    }

    #[test]
    fn novel_missing_category_returns_structured_kind() {
        let body = make_body_without_category();
        let err =
            validate_body(BlockType::Character, Some(&body), ValidationMode::Novel).unwrap_err();
        match err {
            KbError::Validation(ve) => {
                assert_eq!(ve.kind, ValidationKind::MissingNovelCategory);
                assert_eq!(ve.field.as_deref(), Some("body.attributes.novel_category"));
            }
            other => panic!("expected structured Validation, got: {other}"),
        }
    }

    #[test]
    fn novel_invalid_category_returns_structured_kind() {
        let body = make_body(Some("invalid_category"));
        let err =
            validate_body(BlockType::Character, Some(&body), ValidationMode::Novel).unwrap_err();
        match err {
            KbError::Validation(ve) => {
                assert_eq!(ve.kind, ValidationKind::InvalidNovelCategory);
                assert!(ve.message.contains("invalid_category"));
            }
            other => panic!("expected structured Validation, got: {other}"),
        }
    }

    #[test]
    fn non_object_attributes_returns_structured_kind() {
        let body = KeyBlockBody {
            summary: None,
            attributes: Some(serde_json::json!("not an object")),
            tags: None,
        };
        let err =
            validate_body(BlockType::Character, Some(&body), ValidationMode::Generic).unwrap_err();
        match err {
            KbError::Validation(ve) => {
                assert_eq!(ve.kind, ValidationKind::NonObjectAttributes);
            }
            other => panic!("expected structured Validation, got: {other}"),
        }
    }

    // ── Game-Bible mode: happy paths ──────────────────────────────

    fn make_game_bible_body(category: Option<&str>) -> KeyBlockBody {
        KeyBlockBody {
            summary: Some("test".to_string()),
            attributes: category.map(|cat| {
                serde_json::json!({
                    "game_bible_category": cat,
                    "traits": ["expansionist"]
                })
            }),
            tags: Some(vec!["game_bible".to_string()]),
        }
    }

    #[test]
    fn game_bible_mode_accepts_all_seven_categories() {
        let block_types = [
            BlockType::Species,
            BlockType::Faction,
            BlockType::MagicSystem,
            BlockType::Technology,
            BlockType::Deity,
            BlockType::Level,
            BlockType::EconomyTier,
        ];

        for (i, bt) in block_types.iter().enumerate() {
            let body = make_game_bible_body(Some(GAME_BIBLE_CATEGORIES[i]));
            assert!(
                validate_body(*bt, Some(&body), ValidationMode::GameBible).is_ok(),
                "game_bible_category '{}' with block_type {:?} should pass",
                GAME_BIBLE_CATEGORIES[i],
                bt
            );
        }
    }

    #[test]
    fn game_bible_mode_rejects_novel_category() {
        let body = make_body(Some("character")); // novel_category in game-bible mode
        let result = validate_body(BlockType::Species, Some(&body), ValidationMode::GameBible);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("novel_category is not valid for game-bible-profile"),
            "expected rejection of novel_category in game-bible mode, got: {msg}"
        );
    }

    #[test]
    fn game_bible_mode_rejects_missing_game_bible_category() {
        let body = KeyBlockBody {
            summary: Some("test".to_string()),
            attributes: Some(serde_json::json!({"traits": ["ancient"]})),
            tags: None,
        };
        let result = validate_body(BlockType::Species, Some(&body), ValidationMode::GameBible);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("game_bible_category is required"),
            "expected missing game_bible_category error, got: {msg}"
        );
    }

    #[test]
    fn game_bible_mode_rejects_invalid_game_bible_category() {
        let body = make_game_bible_body(Some("invalid_category"));
        let result = validate_body(BlockType::Species, Some(&body), ValidationMode::GameBible);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("invalid game_bible_category"));
    }

    #[test]
    fn game_bible_mode_rejects_non_string_game_bible_category() {
        let body = KeyBlockBody {
            summary: Some("test".to_string()),
            attributes: Some(serde_json::json!({"game_bible_category": 42})),
            tags: None,
        };
        let result = validate_body(BlockType::Species, Some(&body), ValidationMode::GameBible);
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(msg.contains("must be a string"));
    }

    // ── Game-Bible: structured error verification ──────────────────

    #[test]
    fn game_bible_missing_body_returns_structured_kind() {
        let err = validate_body(BlockType::Species, None, ValidationMode::GameBible).unwrap_err();
        match err {
            KbError::Validation(ve) => {
                assert_eq!(ve.kind, ValidationKind::MissingBody);
            }
            other => panic!("expected structured Validation, got: {other}"),
        }
    }

    #[test]
    fn game_bible_missing_category_returns_structured_kind() {
        let body = KeyBlockBody {
            summary: Some("test".to_string()),
            attributes: Some(serde_json::json!({"traits": ["ancient"]})),
            tags: None,
        };
        let err =
            validate_body(BlockType::Species, Some(&body), ValidationMode::GameBible).unwrap_err();
        match err {
            KbError::Validation(ve) => {
                assert_eq!(ve.kind, ValidationKind::MissingGameBibleCategory);
                assert_eq!(
                    ve.field.as_deref(),
                    Some("body.attributes.game_bible_category")
                );
            }
            other => panic!("expected structured Validation, got: {other}"),
        }
    }

    #[test]
    fn game_bible_invalid_category_returns_structured_kind() {
        let body = make_game_bible_body(Some("bad_category"));
        let err =
            validate_body(BlockType::Species, Some(&body), ValidationMode::GameBible).unwrap_err();
        match err {
            KbError::Validation(ve) => {
                assert_eq!(ve.kind, ValidationKind::InvalidGameBibleCategory);
                assert!(ve.message.contains("bad_category"));
            }
            other => panic!("expected structured Validation, got: {other}"),
        }
    }

    // ── BlockType deserialize new variants ─────────────────────────

    #[test]
    fn blocktype_deserialize_new_game_bible_variants() {
        let variants: Vec<(&str, BlockType)> = vec![
            ("species", BlockType::Species),
            ("faction", BlockType::Faction),
            ("magic_system", BlockType::MagicSystem),
            ("technology", BlockType::Technology),
            ("deity", BlockType::Deity),
            ("level", BlockType::Level),
            ("economy_tier", BlockType::EconomyTier),
        ];

        for (wire_name, expected) in variants {
            let v = serde_json::Value::String(wire_name.to_string());
            let bt: BlockType = serde_json::from_value(v)
                .unwrap_or_else(|e| panic!("failed to deserialize '{wire_name}': {e}"));
            assert_eq!(
                bt, expected,
                "wire '{wire_name}' should deserialize to {:?}",
                expected
            );
        }
    }

    // ── canonical_name validation ─────────────────────────────────

    #[test]
    fn canonical_name_accepts_valid() {
        assert!(validate_canonical_name("char_lin_xia").is_ok());
        assert!(validate_canonical_name("loc_neon.city").is_ok());
        assert!(validate_canonical_name("a").is_ok());
        assert!(validate_canonical_name("foundation_cosmology_2024").is_ok());
    }

    #[test]
    fn canonical_name_rejects_empty() {
        let err = validate_canonical_name("").unwrap_err();
        match err {
            KbError::Validation(ve) => {
                assert_eq!(ve.kind, ValidationKind::InvalidCanonicalName);
                assert!(ve.message.contains("must not be empty"));
            }
            other => panic!("expected structured Validation, got: {other}"),
        }
    }

    #[test]
    fn canonical_name_rejects_path_separators() {
        assert!(validate_canonical_name("foo/bar").is_err());
        assert!(validate_canonical_name("foo\\bar").is_err());
    }

    #[test]
    fn canonical_name_rejects_shell_metacharacters() {
        assert!(validate_canonical_name("foo`bar").is_err());
        assert!(validate_canonical_name("foo$bar").is_err());
        assert!(validate_canonical_name("foo;bar").is_err());
        assert!(validate_canonical_name("foo&bar").is_err());
        assert!(validate_canonical_name("foo|bar").is_err());
        assert!(validate_canonical_name("foo>bar").is_err());
        assert!(validate_canonical_name("foo<bar").is_err());
    }

    #[test]
    fn canonical_name_rejects_control_chars() {
        assert!(validate_canonical_name("foo\x00bar").is_err());
        assert!(validate_canonical_name("foo\x1Fbar").is_err());
        assert!(validate_canonical_name("foo\x7Fbar").is_err());
    }

    #[test]
    fn canonical_name_rejects_excessive_length() {
        let long_name = "a".repeat(257);
        assert!(validate_canonical_name(&long_name).is_err());
        // Exactly 256 is ok
        let max_name = "a".repeat(256);
        assert!(validate_canonical_name(&max_name).is_ok());
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
        assert_eq!(ValidationMode::GameBible.to_string(), "game_bible");
    }

    #[test]
    fn validation_kind_display() {
        assert_eq!(
            ValidationKind::MissingNovelCategory.to_string(),
            "missing_novel_category"
        );
        assert_eq!(
            ValidationKind::InvalidCanonicalName.to_string(),
            "invalid_canonical_name"
        );
        assert_eq!(
            ValidationKind::MissingGameBibleCategory.to_string(),
            "missing_game_bible_category"
        );
        assert_eq!(
            ValidationKind::InvalidGameBibleCategory.to_string(),
            "invalid_game_bible_category"
        );
    }

    #[test]
    fn default_block_type_mapping() {
        assert_eq!(
            default_block_type_for_category("foundation"),
            Some(BlockType::InfoPoint)
        );
        assert_eq!(
            default_block_type_for_category("character"),
            Some(BlockType::Character)
        );
        assert_eq!(default_block_type_for_category("unknown"), None);
    }

    // ── Game-Bible utility ─────────────────────────────────────────

    #[test]
    fn is_valid_game_bible_category_true_for_all_seven() {
        for cat in GAME_BIBLE_CATEGORIES {
            assert!(
                is_valid_game_bible_category(cat),
                "expected '{}' valid",
                cat
            );
        }
    }

    #[test]
    fn is_valid_game_bible_category_false_for_unknown() {
        assert!(!is_valid_game_bible_category("unknown"));
        assert!(!is_valid_game_bible_category("Species")); // case-sensitive
        assert!(!is_valid_game_bible_category("character")); // novel category, not game-bible
    }

    #[test]
    fn default_block_type_for_game_bible_category_mapping() {
        assert_eq!(
            default_block_type_for_game_bible_category("species"),
            Some(BlockType::Species)
        );
        assert_eq!(
            default_block_type_for_game_bible_category("faction"),
            Some(BlockType::Faction)
        );
        assert_eq!(
            default_block_type_for_game_bible_category("magic_system"),
            Some(BlockType::MagicSystem)
        );
        assert_eq!(
            default_block_type_for_game_bible_category("technology"),
            Some(BlockType::Technology)
        );
        assert_eq!(
            default_block_type_for_game_bible_category("deity"),
            Some(BlockType::Deity)
        );
        assert_eq!(
            default_block_type_for_game_bible_category("level"),
            Some(BlockType::Level)
        );
        assert_eq!(
            default_block_type_for_game_bible_category("economy_tier"),
            Some(BlockType::EconomyTier)
        );
        assert_eq!(default_block_type_for_game_bible_category("unknown"), None);
    }
}
