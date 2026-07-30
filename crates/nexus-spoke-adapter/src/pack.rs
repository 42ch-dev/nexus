//! Narrative Knowledge Pack build/parse helpers.
//!
//! This module implements the pack dialect defined in the spoke handbook
//! [`domain-profile-narrative-knowledge-pack.md`][pack-handbook] — a portable
//! lore bundle that ships ordered KnowledgeEntries, Relations, and optional
//! SourceAnchors between narrative hosts, with pack-level metadata under
//! `modules.pack`.
//!
//! ## Pack shape
//!
//! ```text
//! {
//!   "modules": { "pack": { "title", "version", "creator", "description?" } },
//!   "entries": [ /* KnowledgeEntry[] */ ],
//!   "relations": [ /* Relation[] */ ],
//!   "source_anchors": [ /* optional SourceAnchor[] */ ]
//! }
//! ```
//!
//! ## Triad ADR (spoke-extension-modules.md)
//!
//! - `modules.pack` is the **only** home for pack-level metadata (cross-product
//!   functional dialect → `modules.*`, NOT `extensions.*`).
//! - Unknown `modules.*` keys and unknown `extensions.*` namespaces on atoms
//!   **MUST round-trip verbatim** — see [`ParsedPack::extra_modules`] and the
//!   round-trip test.
//!
//! ## Validation (locked minimum)
//!
//! - `modules.pack` must be present with required `title` / `version` / `creator`
//!   (description is optional).
//! - `entries` array must be present (may be empty).
//! - `relations` array must be present (may be empty).
//! - `source_anchors` is optional.
//!
//! ## Call-boundary invariant (spec §7)
//!
//! Public functions accept and return spoke standard types only
//! (`KnowledgeEntry`, `Relation`, `SourceAnchor`). [PackError] is a
//! **library-level** error type (not a spoke port), used by the CLI for exit
//! code mapping.
//!
//! [pack-handbook]: https://github.com/42ch-dev/spoke/blob/main/.mstar/specs/domain-profile-narrative-knowledge-pack.md

use serde_json::{Map, Value};
use spoke_schemas::{KnowledgeEntry, Relation, SourceAnchor};

// ── Error type ────────────────────────────────────────────────────────────────

/// Errors that can occur during pack build/parse operations.
///
/// This is a library-level error type — not a spoke [`SpokeReject`]. The CLI
/// (T2/T3) maps these variants to appropriate exit codes / user messages.
#[derive(Debug)]
pub enum PackError {
    /// `modules.pack` key is absent or the `modules` object is missing.
    MissingModulesPack,
    /// A required field inside `modules.pack` is missing.
    MissingPackField { field: &'static str },
    /// The top-level `entries` array is missing or not an array.
    MissingEntries,
    /// The top-level `relations` array is missing or not an array.
    MissingRelations,
    /// The input JSON failed to parse as a valid `serde_json` value.
    Json(serde_json::Error),
}

impl std::fmt::Display for PackError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingModulesPack => write!(f, "missing required key 'modules.pack'"),
            Self::MissingPackField { field } => {
                write!(f, "missing required pack metadata field: {field}")
            }
            Self::MissingEntries => {
                write!(f, "missing required field 'entries' (must be an array)")
            }
            Self::MissingRelations => {
                write!(f, "missing required field 'relations' (must be an array)")
            }
            Self::Json(e) => write!(f, "JSON error: {e}"),
        }
    }
}

impl std::error::Error for PackError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(e) => Some(e),
            _ => None,
        }
    }
}

impl From<serde_json::Error> for PackError {
    fn from(e: serde_json::Error) -> Self {
        Self::Json(e)
    }
}

// ── Structured types ──────────────────────────────────────────────────────────

/// Pack-level metadata from `modules.pack`.
///
/// Fields map to the handbook-defined field table:
///
/// | Field | Required | Semantics |
/// |-------|----------|-----------|
/// | `title` | yes | Human pack name |
/// | `version` | yes | Pack author version string |
/// | `creator` | yes | Authoring identity |
/// | `description` | no | Short human blurb |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackMetadata {
    pub title: String,
    pub version: String,
    pub creator: String,
    pub description: Option<String>,
}

/// The structured result of [`parse_pack`] — validated, typed, and ready for the
/// CLI to consume.
///
/// ## Round-trip preservation
///
/// Unknown `modules.*` keys (anything outside `"pack"`) are carried in
/// [`extra_modules`](Self::extra_modules) and re-emitted during
/// [`build_pack`] so a parse → build cycle preserves unknown dialect keys.
#[derive(Debug, Clone)]
pub struct ParsedPack {
    /// Parsed `modules.pack` metadata.
    pub pack_metadata: PackMetadata,
    /// Ordered `KnowledgeEntries` from the pack.
    pub entries: Vec<KnowledgeEntry>,
    /// Relations from the pack.
    pub relations: Vec<Relation>,
    /// Optional `SourceAnchors` (absent when the key is missing or null).
    pub source_anchors: Option<Vec<SourceAnchor>>,
    /// Unknown `modules.*` keys preserved for round-trip (always includes at
    /// least "pack" for re-emission, plus any extra discovered keys).
    pub extra_modules: Map<String, Value>,
}

// ── build_pack ────────────────────────────────────────────────────────────────

/// Build a handbook-conformant Narrative Knowledge Pack as a JSON [`Value`].
///
/// # Parameters
///
/// - `entries` — ordered `KnowledgeEntry` list. Each entry carries its own
///   `world_id` via the typed `extensions.nexus.world_id` accessor (see
///   [`crate::extensions`]); there is no top-level `world_id` parameter because
///   the spoke handbook pack shape is intentionally world-agnostic.
/// - `relations` — Relations referencing entries in the pack.
/// - `anchors` — optional `SourceAnchors` (pass `None` or empty slice to omit the key).
/// - `title`, `version`, `creator` — required `modules.pack` metadata.
/// - `description` — optional `modules.pack` description.
/// - `extra_modules` — unknown `modules.*` keys carried from a prior [`parse_pack`]
///   cycle (or `None` when building a pack from scratch).
///
/// # Round-trip guarantee
///
/// When `extra_modules` is supplied (from a prior parse), its keys are merged
/// with the authoritatively-written `"pack"` module, preserving any unknown
/// dialect keys verbatim.
///
/// # Panics
///
/// Panics if a passed [`KnowledgeEntry`], [`Relation`], or [`SourceAnchor`] fails
/// to serialize. This can only happen with a buggy serde derive (spoke's types
/// always serialize cleanly).
// allow: 8 parameters is the natural shape for pack assembly — the CLI passes
// each piece from separate query results. Packing into a struct here would be
// premature abstraction with a single call site (T2).
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn build_pack(
    entries: &[KnowledgeEntry],
    relations: &[Relation],
    anchors: Option<&[SourceAnchor]>,
    title: &str,
    version: &str,
    creator: &str,
    description: Option<&str>,
    extra_modules: Option<&Map<String, Value>>,
) -> Value {
    let mut modules = extra_modules.cloned().unwrap_or_default();

    // Authoritatively write modules.pack (overwrites any prior pack key).
    let mut pack_obj = Map::new();
    pack_obj.insert("title".into(), Value::String(title.to_owned()));
    pack_obj.insert("version".into(), Value::String(version.to_owned()));
    pack_obj.insert("creator".into(), Value::String(creator.to_owned()));
    if let Some(desc) = description {
        pack_obj.insert("description".into(), Value::String(desc.to_owned()));
    }
    modules.insert("pack".into(), Value::Object(pack_obj));

    let mut root = Map::new();
    root.insert("modules".into(), Value::Object(modules));
    root.insert(
        "entries".into(),
        serde_json::to_value(entries).expect("KnowledgeEntry is always serializable"),
    );
    root.insert(
        "relations".into(),
        serde_json::to_value(relations).expect("Relation is always serializable"),
    );
    if let Some(anchors_slice) = anchors {
        if !anchors_slice.is_empty() {
            root.insert(
                "source_anchors".into(),
                serde_json::to_value(anchors_slice).expect("SourceAnchor is always serializable"),
            );
        }
    }

    Value::Object(root)
}

// ── parse_pack ────────────────────────────────────────────────────────────────

/// Parse and validate a pack JSON value into a structured [`ParsedPack`].
///
/// # Validation (locked minimum)
///
/// - `modules.pack` must be present with required fields `title` / `version` /
///   `creator` (description is optional).
/// - `entries` must be an array (may be empty).
/// - `relations` must be an array (may be empty).
/// - `source_anchors` is optional (absent or null → `None`).
///
/// # Round-trip guarantee
///
/// Unknown `modules.*` keys (outside `"pack"`) and unknown `extensions.*`
/// namespaces on atoms are preserved. Unknown `modules` keys are stored in
/// [`ParsedPack::extra_modules`]; unknown extensions are kept on the spoke
/// atoms themselves (their native `extensions` maps already hold them).
///
/// # Errors
///
/// Returns [`PackError`] when:
/// - `modules.pack` is missing or not an object ([`PackError::MissingModulesPack`]).
/// - A required pack metadata field (`title`, `version`, `creator`) is absent
///   ([`PackError::MissingPackField`]).
/// - `entries` or `relations` is missing ([`PackError::MissingEntries`],
///   [`PackError::MissingRelations`]).
/// - Any spoke wire type fails to deserialize from the provided JSON
///   ([`PackError::Json`]).
pub fn parse_pack(json: &Value) -> Result<ParsedPack, PackError> {
    let obj = json.as_object().ok_or(PackError::MissingModulesPack)?;

    // ── modules.pack validation ─────────────────────────────────────────
    let modules = obj
        .get("modules")
        .and_then(Value::as_object)
        .ok_or(PackError::MissingModulesPack)?;

    let pack_val = modules.get("pack").ok_or(PackError::MissingModulesPack)?;

    let pack_obj = pack_val.as_object().ok_or(PackError::MissingModulesPack)?;

    let title = required_string(pack_obj, "title")?;
    let version = required_string(pack_obj, "version")?;
    let creator = required_string(pack_obj, "creator")?;
    let description = pack_obj
        .get("description")
        .and_then(Value::as_str)
        .map(String::from);

    // ── entries (required, but may be empty) ────────────────────────────
    let entries_raw = obj.get("entries").ok_or(PackError::MissingEntries)?;
    let entries: Vec<KnowledgeEntry> = serde_json::from_value(entries_raw.clone())?;

    // ── relations (required, but may be empty) ──────────────────────────
    let relations_raw = obj.get("relations").ok_or(PackError::MissingRelations)?;
    let relations: Vec<Relation> = serde_json::from_value(relations_raw.clone())?;

    // ── source_anchors (optional) ───────────────────────────────────────
    let source_anchors = match obj.get("source_anchors") {
        Some(a) if !a.is_null() => Some(serde_json::from_value(a.clone())?),
        _ => None,
    };

    // ── Preserve unknown modules.* keys for round-trip ──────────────────
    // Clone the full modules map; "pack" is already accounted for, but any
    // extra keys (e.g. future dialects) survive here.
    let extra_modules = modules.clone();

    Ok(ParsedPack {
        pack_metadata: PackMetadata {
            title,
            version,
            creator,
            description,
        },
        entries,
        relations,
        source_anchors,
        extra_modules,
    })
}

/// Extract a required string field from a JSON object, returning
/// [`PackError::MissingPackField`] if absent or not a string.
fn required_string(obj: &Map<String, Value>, key: &'static str) -> Result<String, PackError> {
    obj.get(key)
        .and_then(Value::as_str)
        .map(String::from)
        .ok_or(PackError::MissingPackField { field: key })
}

// ── str → Value convenience (used primarily by tests) ─────────────────────────

/// Parse a JSON string into a [`Value`], for convenience at the call boundary.
///
/// This is a thin wrapper around [`serde_json::from_str`] that delegates to
/// [`parse_pack`] for validation.
///
/// # Errors
///
/// Returns [`PackError::Json`] if the string is not valid JSON, or any
/// validation error from [`parse_pack`].
pub fn parse_pack_str(s: &str) -> Result<ParsedPack, PackError> {
    let value: Value = serde_json::from_str(s)?;
    parse_pack(&value)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── Helpers ────────────────────────────────────────────────────────────

    fn sample_entry(id: &str, name: &str) -> KnowledgeEntry {
        serde_json::from_value(json!({
            "schema_version": 1,
            "entry_id": id,
            "entry_type": "character",
            "canonical_name": name,
            "status": "confirmed",
            "body": { "summary": "A sample entry." },
            "extensions": {}
        }))
        .expect("sample_entry is valid KnowledgeEntry JSON")
    }

    fn sample_relation(id: &str, from_id: &str, to_id: &str) -> Relation {
        serde_json::from_value(json!({
            "schema_version": 1,
            "relation_id": id,
            "relation_type": "related_to",
            "from_id": from_id,
            "to_id": to_id,
            "extensions": {}
        }))
        .expect("sample_relation is valid Relation JSON")
    }

    fn sample_anchor(source_id: &str) -> SourceAnchor {
        serde_json::from_value(json!({
            "schema_version": 1,
            "source_id": source_id,
            "extensions": {}
        }))
        .expect("sample_anchor is valid SourceAnchor JSON")
    }

    // ── Round-trip: build → parse → build → parse ─────────────────────────

    #[test]
    fn round_trip_build_parse_build() {
        let entries = vec![
            sample_entry("kb_1", "Mira"),
            sample_entry("kb_2", "Ashford"),
        ];
        let relations = vec![sample_relation("rel_1", "kb_1", "kb_2")];
        let anchors = vec![sample_anchor("src:ch1")];

        let pack1 = build_pack(
            &entries,
            &relations,
            Some(&anchors),
            "Test World",
            "0.1.0",
            "tester",
            Some("A test pack"),
            None,
        );

        let parsed = parse_pack(&pack1).expect("round-trip parse must succeed");
        assert_eq!(parsed.pack_metadata.title, "Test World");
        assert_eq!(parsed.pack_metadata.version, "0.1.0");
        assert_eq!(parsed.pack_metadata.creator, "tester");
        assert_eq!(
            parsed.pack_metadata.description.as_deref(),
            Some("A test pack")
        );
        assert_eq!(parsed.entries.len(), 2);
        assert_eq!(parsed.relations.len(), 1);
        assert!(parsed.source_anchors.is_some());
        assert_eq!(parsed.source_anchors.as_ref().unwrap().len(), 1);

        // Rebuild from parsed data (full round-trip)
        let pack2 = build_pack(
            &parsed.entries,
            &parsed.relations,
            parsed.source_anchors.as_deref(),
            &parsed.pack_metadata.title,
            &parsed.pack_metadata.version,
            &parsed.pack_metadata.creator,
            parsed.pack_metadata.description.as_deref(),
            Some(&parsed.extra_modules),
        );

        let parsed2 = parse_pack(&pack2).expect("second round-trip parse must succeed");
        assert_eq!(parsed2.pack_metadata, parsed.pack_metadata);
        assert_eq!(parsed2.entries.len(), 2);
        assert_eq!(parsed2.relations.len(), 1);
    }

    // ── Missing modules.pack ──────────────────────────────────────────────

    #[test]
    fn error_missing_modules_pack() {
        let bad = json!({
            "entries": [],
            "relations": []
        });
        let err = parse_pack(&bad).unwrap_err();
        assert!(matches!(err, PackError::MissingModulesPack));
    }

    // ── Missing required pack fields ──────────────────────────────────────

    #[test]
    fn error_missing_title() {
        let bad = json!({
            "modules": { "pack": { "version": "1.0", "creator": "me" } },
            "entries": [],
            "relations": []
        });
        let err = parse_pack(&bad).unwrap_err();
        assert!(matches!(
            err,
            PackError::MissingPackField { field: "title" }
        ));
    }

    #[test]
    fn error_missing_version() {
        let bad = json!({
            "modules": { "pack": { "title": "T", "creator": "me" } },
            "entries": [],
            "relations": []
        });
        let err = parse_pack(&bad).unwrap_err();
        assert!(matches!(
            err,
            PackError::MissingPackField { field: "version" }
        ));
    }

    // ── Type-mismatch validation paths ───────────────────────────────────

    #[test]
    fn error_modules_pack_not_object() {
        let bad = json!({
            "modules": { "pack": "not-an-object" },
            "entries": [],
            "relations": []
        });
        let err = parse_pack(&bad).unwrap_err();
        assert!(matches!(err, PackError::MissingModulesPack));
    }

    #[test]
    fn error_title_wrong_type() {
        let bad = json!({
            "modules": { "pack": { "title": 42, "version": "1.0", "creator": "me" } },
            "entries": [],
            "relations": []
        });
        let err = parse_pack(&bad).unwrap_err();
        assert!(matches!(
            err,
            PackError::MissingPackField { field: "title" }
        ));
    }

    #[test]
    fn error_entries_not_array() {
        let bad = json!({
            "modules": { "pack": { "title": "T", "version": "1.0", "creator": "me" } },
            "entries": { "not": "an-array" },
            "relations": []
        });
        let err = parse_pack(&bad).unwrap_err();
        assert!(matches!(err, PackError::Json(_)));
    }

    // ── Missing entries / relations ───────────────────────────────────────

    #[test]
    fn error_missing_entries() {
        let bad = json!({
            "modules": { "pack": { "title": "T", "version": "1.0", "creator": "me" } },
            "relations": []
        });
        let err = parse_pack(&bad).unwrap_err();
        assert!(matches!(err, PackError::MissingEntries));
    }

    #[test]
    fn error_missing_relations() {
        let bad = json!({
            "modules": { "pack": { "title": "T", "version": "1.0", "creator": "me" } },
            "entries": []
        });
        let err = parse_pack(&bad).unwrap_err();
        assert!(matches!(err, PackError::MissingRelations));
    }

    // ── Empty entries / relations are valid ───────────────────────────────

    #[test]
    fn empty_entries_valid() {
        let pack = json!({
            "modules": { "pack": { "title": "T", "version": "1.0", "creator": "me" } },
            "entries": [],
            "relations": []
        });
        let parsed = parse_pack(&pack).expect("empty entries must be valid");
        assert!(parsed.entries.is_empty());
        assert!(parsed.relations.is_empty());
    }

    // ── Anchors optional ──────────────────────────────────────────────────

    #[test]
    fn anchors_optional() {
        let pack = json!({
            "modules": { "pack": { "title": "T", "version": "1.0", "creator": "me" } },
            "entries": [],
            "relations": []
        });
        let parsed = parse_pack(&pack).expect("pack without anchors must be valid");
        assert!(parsed.source_anchors.is_none());
    }

    // ── Round-trip with unknown modules.* keys ────────────────────────────

    #[test]
    fn round_trip_preserves_unknown_modules() {
        let pack = json!({
            "modules": {
                "pack": { "title": "T", "version": "1.0", "creator": "me" },
                "future_dialect": { "color": "blue", "priority": 42 }
            },
            "entries": [],
            "relations": []
        });

        let parsed = parse_pack(&pack).expect("pack with extra modules must parse");
        assert!(parsed.extra_modules.contains_key("future_dialect"));

        // Rebuild and verify the extra key survives
        let rebuilt = build_pack(
            &[],
            &[],
            None,
            "T",
            "1.0",
            "me",
            None,
            Some(&parsed.extra_modules),
        );

        let modules_obj = rebuilt
            .get("modules")
            .and_then(Value::as_object)
            .expect("rebuilt pack must have modules");
        assert!(modules_obj.contains_key("future_dialect"));

        // Verify the extra value is intact
        let future = &modules_obj["future_dialect"];
        assert_eq!(future["color"], json!("blue"));
        assert_eq!(future["priority"], json!(42));
    }

    // ── Description optional ──────────────────────────────────────────────

    #[test]
    fn description_optional_in_parse() {
        let pack = json!({
            "modules": { "pack": { "title": "T", "version": "1.0", "creator": "me" } },
            "entries": [],
            "relations": []
        });
        let parsed = parse_pack(&pack).expect("pack without description must parse");
        assert!(parsed.pack_metadata.description.is_none());
    }

    // ── build_pack omits source_anchors when None ──────────────────────────

    #[test]
    fn build_pack_omits_anchors_when_none() {
        let pack = build_pack(
            &[],
            &[],
            None::<&[SourceAnchor]>,
            "T",
            "1.0",
            "me",
            None,
            None,
        );
        assert!(pack.get("source_anchors").is_none());
    }

    #[test]
    fn build_pack_omits_anchors_when_empty_slice() {
        let pack = build_pack(&[], &[], Some(&[]), "T", "1.0", "me", None, None);
        assert!(pack.get("source_anchors").is_none());
    }

    // ── parse_pack_str convenience ────────────────────────────────────────

    #[test]
    fn parse_pack_str_round_trip() {
        let json_str = r#"{
            "modules": { "pack": { "title": "Harbor", "version": "0.1.0", "creator": "nexus" } },
            "entries": [],
            "relations": []
        }"#;
        let parsed = parse_pack_str(json_str).expect("parse_pack_str must succeed");
        assert_eq!(parsed.pack_metadata.title, "Harbor");
    }
}
