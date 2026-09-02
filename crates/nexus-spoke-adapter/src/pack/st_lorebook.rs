//! Clean-room `SillyTavern` lorebook → Knowledge Pack converter (DF-80).
//!
//! Converts a documented-format `SillyTavern` lorebook JSON file into the pack
//! JSON shape consumed by [`super::parse_pack`], riding the existing DF-77
//! pack transport and import orchestration. The conversion is CLI-local —
//! `nexus-daemon-runtime::pack_import` is not modified.
//!
//! ## Clean-room constraint
//!
//! Format knowledge comes from the public `SillyTavern` "World Info"
//! documentation (docs.sillytavern.app) only — never from ST source code.
//! The converter is tolerant-with-diagnostics: unknown/undocumented entry
//! fields are reported as [`ConversionDiagnostic`]s and the import continues;
//! only file-level malformation (not JSON / not an object / `entries` not an
//! array) aborts with a structured [`StLorebookError`] before any write.
//!
//! ## Mapping (locked)
//!
//! | ST lorebook entry field | Pack entry field |
//! |-------------------------|------------------|
//! | `comment` (title/memo)  | `canonical_name` |
//! | `content`               | `body.summary`   |
//! | `key` / `keys`          | `modules.activation.keys` |
//! | `constant`              | `modules.activation.constant` |

use serde_json::json;
use serde_json::{Map, Value};
use thiserror::Error;

/// Default pack title stamped into `modules.pack.title` when the lorebook
/// carries no top-level `name`.
const DEFAULT_PACK_TITLE: &str = "Imported Lorebook";

/// Default version string stamped into `modules.pack.version`.
const DEFAULT_PACK_VERSION: &str = "0.1.0";

/// Fallback author string stamped into `modules.pack.creator` (matches the
/// CLI's `FALLBACK_CREATOR`; the pack metadata is transport envelope only —
/// the import path uses the active creator id, not this string).
const DEFAULT_PACK_CREATOR: &str = "nexus42";

/// Documented `SillyTavern` lorebook entry fields — the serialized form of the
/// fields documented in the public "World Info" docs (docs.sillytavern.app):
/// key(s), optional filter, content, insertion order/position, title/memo,
/// strategy (constant/enabled), probability, inclusion groups, automation id,
/// scan depth, case sensitivity, whole-word matching, recursion controls,
/// timed effects, min activations / max depth / max recursion steps, plus the
/// serialized bookkeeping fields (`uid`, `id`, `display_index`, `role`,
/// `selective*`, `use_regex`, `addMemo`, `order`, `probabilityRaw`,
/// `original_content`, `extensions`). Fields outside this set are reported as
/// conversion diagnostics — never silently dropped.
const DOCUMENTED_ENTRY_FIELDS: &[&str] = &[
    // Identity / bookkeeping
    "uid",
    "id",
    "display_index",
    "role",
    "extensions",
    // Keys / activation
    "key",
    "keys",
    "keysecondary",
    "selective",
    "selectiveLogic",
    "use_regex",
    "match_whole_words",
    "case_sensitive",
    "scan_depth",
    "constant",
    "enabled",
    "probability",
    "useProbability",
    "probabilityRaw",
    // Content / naming
    "content",
    "original_content",
    "comment",
    "addMemo",
    // Ordering / placement
    "insertion_order",
    "order",
    "position",
    "depth",
    // Recursion / timed effects
    "exclude_recursion",
    "prevent_recursion",
    "delay_until_recursion",
    "min_activations",
    "max_activations",
    "sticky",
    "cooldown",
    "delay",
    // Inclusion groups
    "group",
    "group_override",
    "group_weight",
    "group_priority",
    "use_group_scoring",
    // Automation
    "automation_id",
    // Organization
    "folder",
    // Alternate-scenario matching
    "exclude_alternate_scenarios",
    "exclude_alternate_scenarios_override",
    "exclude_alternate_scenarios_priority",
];

/// Severity of a conversion diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    /// The converter dropped or degraded data; the import still proceeds.
    Warning,
    /// Informational note about a conversion decision.
    Info,
}

/// One per-entry conversion note. Unknown/undocumented fields produce
/// `Warning` diagnostics; the import continues.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversionDiagnostic {
    pub severity: DiagnosticSeverity,
    pub entry_index: Option<usize>,
    pub entry_name: Option<String>,
    pub field: Option<String>,
    pub message: String,
}

/// The result of a successful lorebook conversion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversionOutcome {
    /// Pack JSON fed to [`super::parse_pack`].
    pub pack_input: Value,
    /// Per-entry conversion notes (unknown fields etc.).
    pub diagnostics: Vec<ConversionDiagnostic>,
}

/// File-level malformation of an ST lorebook file. Any of these aborts the
/// import before any write (no partial import).
#[derive(Debug, Error)]
pub enum StLorebookError {
    /// The file is not valid JSON.
    #[error("not valid JSON: {0}")]
    NotJson(#[from] serde_json::Error),
    /// The root value is not a JSON object.
    #[error("lorebook root must be a JSON object")]
    NotObject,
    /// `entries` is missing or not an array.
    #[error("lorebook 'entries' must be an array")]
    EntriesNotArray,
}

/// Convert a documented-format `SillyTavern` lorebook JSON value into a pack
/// JSON value accepted by [`super::parse_pack`].
///
/// # Errors
///
/// Returns [`StLorebookError`] for file-level malformation only: the root is
/// not a JSON object, or `entries` is missing / not an array. Per-entry
/// issues (unknown/undocumented fields, non-object entries) are collected as
/// [`ConversionDiagnostic`]s and the conversion continues.
pub fn parse_st_lorebook(json: &Value) -> Result<ConversionOutcome, StLorebookError> {
    let root = json.as_object().ok_or(StLorebookError::NotObject)?;
    let entries = root
        .get("entries")
        .and_then(Value::as_array)
        .ok_or(StLorebookError::EntriesNotArray)?;

    let mut diagnostics = Vec::new();
    let mut pack_entries = Vec::new();

    for (idx, entry) in entries.iter().enumerate() {
        let Some(obj) = entry.as_object() else {
            diagnostics.push(ConversionDiagnostic {
                severity: DiagnosticSeverity::Warning,
                entry_index: Some(idx),
                entry_name: None,
                field: None,
                message: "entry is not a JSON object; skipped".to_string(),
            });
            continue;
        };

        let name = entry_name(obj, idx);

        // Unknown/undocumented fields → diagnostics; the import continues.
        for key in obj.keys() {
            if !DOCUMENTED_ENTRY_FIELDS.contains(&key.as_str()) {
                diagnostics.push(ConversionDiagnostic {
                    severity: DiagnosticSeverity::Warning,
                    entry_index: Some(idx),
                    entry_name: Some(name.clone()),
                    field: Some(key.clone()),
                    message: format!("unknown/undocumented field '{key}' ignored"),
                });
            }
        }

        let content = obj.get("content").and_then(Value::as_str).unwrap_or("");
        let keys = activation_keys(obj);
        let constant = obj
            .get("constant")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        let mut entry_json = Map::new();
        entry_json.insert("schema_version".to_string(), json!(1));
        entry_json.insert("entry_id".to_string(), json!(format!("kb_st_{idx}")));
        entry_json.insert("entry_type".to_string(), json!("info_point"));
        entry_json.insert("canonical_name".to_string(), json!(name));
        entry_json.insert("status".to_string(), json!("confirmed"));
        entry_json.insert("body".to_string(), json!({ "summary": content }));
        entry_json.insert("extensions".to_string(), json!({}));
        if !keys.is_empty() || constant {
            entry_json.insert(
                "modules".to_string(),
                json!({ "activation": { "keys": keys, "constant": constant } }),
            );
        }
        pack_entries.push(Value::Object(entry_json));
    }

    let title = root
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_PACK_TITLE);

    let pack_input = json!({
        "modules": {
            "pack": {
                "title": title,
                "version": DEFAULT_PACK_VERSION,
                "creator": DEFAULT_PACK_CREATOR,
            }
        },
        "entries": pack_entries,
        "relations": [],
    });

    Ok(ConversionOutcome {
        pack_input,
        diagnostics,
    })
}

/// Resolve the pack `canonical_name` for an ST entry: the `comment`
/// (title/memo) when non-empty, else the first activation key (the
/// documented "fill empty memos" backfill), else a positional fallback.
fn entry_name(obj: &Map<String, Value>, idx: usize) -> String {
    obj.get("comment")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| first_key(obj))
        .unwrap_or_else(|| format!("entry-{idx}"))
}

/// The first activation key of an entry, if any (`keys` array wins over the
/// comma-separated `key` string — the activation engine's documented
/// precedence).
fn first_key(obj: &Map<String, Value>) -> Option<String> {
    if let Some(keys) = obj.get("keys").and_then(Value::as_array) {
        if let Some(k) = keys.iter().find_map(Value::as_str) {
            return Some(k.to_string());
        }
    }
    obj.get("key").and_then(Value::as_str).and_then(|s| {
        s.split(',')
            .map(str::trim)
            .find(|k| !k.is_empty())
            .map(str::to_string)
    })
}

/// The activation keys of an entry: the `keys` array when present, else the
/// comma-separated `key` string split on commas (documented plaintext mode).
fn activation_keys(obj: &Map<String, Value>) -> Vec<String> {
    if let Some(keys) = obj.get("keys").and_then(Value::as_array) {
        return keys
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
    }
    obj.get("key")
        .and_then(Value::as_str)
        .map(|s| {
            s.split(',')
                .map(str::trim)
                .filter(|k| !k.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::parse_pack;
    use serde_json::json;

    fn fixture(name: &str) -> Value {
        let text = std::fs::read_to_string(format!("tests/fixtures/st_lorebook/{name}.json"))
            .unwrap_or_else(|e| panic!("fixture {name}.json must exist: {e}"));
        serde_json::from_str(&text)
            .unwrap_or_else(|e| panic!("fixture {name}.json must be valid JSON: {e}"))
    }

    #[test]
    fn minimal_lorebook_converts_to_pack_parse_pack_accepts() {
        let outcome =
            parse_st_lorebook(&fixture("minimal")).expect("minimal lorebook must convert");
        assert!(
            outcome.diagnostics.is_empty(),
            "minimal fixture has no unknown fields; got {outcome:#?}"
        );
        let entry = &outcome.pack_input["entries"][0];
        assert_eq!(entry["canonical_name"], json!("Dragon lore"));
        assert_eq!(
            entry["body"]["summary"],
            json!("Dragons are ancient, intelligent reptiles capable of speech and magic.")
        );
        assert_eq!(entry["modules"]["activation"]["keys"], json!(["dragon"]));
        assert_eq!(entry["modules"]["activation"]["constant"], json!(false));
        let parsed = parse_pack(&outcome.pack_input).expect("converted pack must parse");
        assert_eq!(parsed.entries.len(), 1);
        assert_eq!(parsed.relations.len(), 0);
    }

    #[test]
    fn multi_entry_maps_keys_constant_and_metadata() {
        let outcome =
            parse_st_lorebook(&fixture("multi_entry")).expect("multi-entry lorebook must convert");
        assert!(
            outcome.diagnostics.is_empty(),
            "multi-entry fixture uses only documented fields; got {outcome:#?}"
        );
        let pack = &outcome.pack_input;
        assert_eq!(
            pack["modules"]["pack"]["title"],
            json!("Farlandia Worldbook")
        );
        let entries = pack["entries"].as_array().expect("entries array");
        assert_eq!(entries.len(), 2);
        // Entry 0: comma-separated `key` string → keys array; not constant.
        assert_eq!(entries[0]["canonical_name"], json!("Mossford"));
        assert_eq!(
            entries[0]["modules"]["activation"]["keys"],
            json!(["Mossford", "town", "moss"])
        );
        assert_eq!(
            entries[0]["modules"]["activation"]["constant"],
            json!(false)
        );
        // Entry 1: `keys` array + constant true.
        assert_eq!(entries[1]["canonical_name"], json!("Slime"));
        assert_eq!(
            entries[1]["modules"]["activation"]["keys"],
            json!(["slime", "slimes"])
        );
        assert_eq!(entries[1]["modules"]["activation"]["constant"], json!(true));
        let parsed = parse_pack(pack).expect("converted pack must parse");
        assert_eq!(parsed.entries.len(), 2);
    }

    #[test]
    fn unknown_fields_produce_diagnostics_and_import_continues() {
        let outcome = parse_st_lorebook(&fixture("unknown_fields"))
            .expect("must convert despite unknown fields");
        let parsed = parse_pack(&outcome.pack_input).expect("converted pack must parse");
        assert_eq!(
            parsed.entries.len(),
            2,
            "unknown fields must not drop entries"
        );
        assert_eq!(outcome.diagnostics.len(), 3);
        let mut fields: Vec<&str> = outcome
            .diagnostics
            .iter()
            .filter_map(|d| d.field.as_deref())
            .collect();
        fields.sort_unstable();
        assert_eq!(
            fields,
            vec!["custom_metadata", "favorite_color", "unknown_knob"]
        );
        for d in &outcome.diagnostics {
            assert_eq!(d.severity, DiagnosticSeverity::Warning);
            assert!(d.entry_index.is_some());
            assert!(d.entry_name.is_some());
        }
    }

    #[test]
    fn not_object_root_is_structured_error() {
        let err = parse_st_lorebook(&json!([1, 2, 3])).unwrap_err();
        assert!(matches!(err, StLorebookError::NotObject));
    }

    #[test]
    fn entries_not_array_is_structured_error() {
        let err = parse_st_lorebook(&json!({ "entries": { "0": {} } })).unwrap_err();
        assert!(matches!(err, StLorebookError::EntriesNotArray));
    }

    #[test]
    fn missing_entries_is_structured_error() {
        let err = parse_st_lorebook(&json!({ "name": "no entries" })).unwrap_err();
        assert!(matches!(err, StLorebookError::EntriesNotArray));
    }

    #[test]
    fn non_object_entry_is_diagnostic_and_skipped() {
        let lorebook = json!({
            "entries": [
                { "uid": 0, "key": "ok", "content": "fine", "comment": "OK" },
                "not-an-object",
                { "uid": 2, "key": "also-ok", "content": "also fine", "comment": "Also OK" }
            ]
        });
        let outcome = parse_st_lorebook(&lorebook).expect("must convert");
        let parsed = parse_pack(&outcome.pack_input).expect("pack must parse");
        assert_eq!(parsed.entries.len(), 2, "non-object entry is skipped");
        assert_eq!(outcome.diagnostics.len(), 1);
        let d = &outcome.diagnostics[0];
        assert_eq!(d.entry_index, Some(1));
        assert!(d.message.contains("not a JSON object"));
    }

    #[test]
    fn empty_comment_falls_back_to_first_key() {
        let lorebook = json!({
            "entries": [
                { "uid": 0, "key": "harbor,port", "content": "The harbor gates.", "comment": "" }
            ]
        });
        let outcome = parse_st_lorebook(&lorebook).expect("must convert");
        assert_eq!(
            outcome.pack_input["entries"][0]["canonical_name"],
            json!("harbor")
        );
    }
}
