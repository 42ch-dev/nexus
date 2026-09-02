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
use std::collections::HashSet;
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
    let mut seen_entry_ids = HashSet::new();

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

        // W-2 / W-3: documented fields whose semantics the locked mapping
        // cannot honor are flagged, never silently dropped.
        diagnostics.extend(documented_field_diagnostics(obj, idx, &name));

        let content = obj.get("content").and_then(Value::as_str).unwrap_or("");
        let keys = activation_keys(obj);
        let constant = obj
            .get("constant")
            .and_then(Value::as_bool)
            .unwrap_or(false);

        // W-4: derive a stable entry id from the lorebook-intrinsic `uid` /
        // `id` fields (see `stable_entry_id`); the positional fallback is
        // reported because it is unstable across lorebook edits.
        let (entry_id, id_diagnostic) = stable_entry_id(obj, idx, &name, &mut seen_entry_ids);
        if let Some(d) = id_diagnostic {
            diagnostics.push(d);
        }

        let mut entry_json = Map::new();
        entry_json.insert("schema_version".to_string(), json!(1));
        entry_json.insert("entry_id".to_string(), json!(entry_id));
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
/// precedence). The `key` field is documented as either a comma-separated
/// string or a list (array) of strings; both forms are accepted (W-3).
fn first_key(obj: &Map<String, Value>) -> Option<String> {
    if let Some(keys) = obj.get("keys").and_then(Value::as_array) {
        if let Some(k) = keys.iter().find_map(Value::as_str) {
            return Some(k.to_string());
        }
    }
    match obj.get("key") {
        Some(Value::String(s)) => s
            .split(',')
            .map(str::trim)
            .find(|k| !k.is_empty())
            .map(str::to_string),
        Some(Value::Array(items)) => items.iter().find_map(Value::as_str).map(str::to_string),
        _ => None,
    }
}

/// The activation keys of an entry: the `keys` array when present, else the
/// `key` field — a comma-separated string (documented plaintext mode) or a
/// list of strings (documented list form, W-3).
fn activation_keys(obj: &Map<String, Value>) -> Vec<String> {
    if let Some(keys) = obj.get("keys").and_then(Value::as_array) {
        return keys
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect();
    }
    match obj.get("key") {
        Some(Value::String(s)) => s
            .split(',')
            .map(str::trim)
            .filter(|k| !k.is_empty())
            .map(str::to_string)
            .collect(),
        Some(Value::Array(items)) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

/// Per-entry diagnostics for documented fields whose semantics the locked
/// mapping cannot honor — the import continues, but the author is told.
///
/// - **W-2** `enabled: false`: nexus has no "disabled" state — `provisional`
///   is still live for activation (the KB query excludes only
///   deleted/merged/deprecated), and those terminal states carry different
///   semantics (superseded/merged/removed). Importing the entry as live
///   `confirmed` content would silently invert the author's intent, so a
///   `Warning` fires; the content is preserved and the author can
///   deprecate/delete it after import to keep it from firing.
/// - **W-3** `key` in an unmappable shape: the field is documented as a
///   comma-separated string or a list (array) of strings; anything else
///   (and non-string array elements) is flagged instead of silently dropping
///   activation keys.
fn documented_field_diagnostics(
    obj: &Map<String, Value>,
    idx: usize,
    name: &str,
) -> Vec<ConversionDiagnostic> {
    let mut diagnostics = Vec::new();
    if obj.get("enabled").and_then(Value::as_bool) == Some(false) {
        diagnostics.push(ConversionDiagnostic {
            severity: DiagnosticSeverity::Warning,
            entry_index: Some(idx),
            entry_name: Some(name.to_string()),
            field: Some("enabled".to_string()),
            message: "enabled=false ignored: nexus has no disabled state; entry imports as live 'confirmed' content — deprecate/delete it after import to keep it from firing".to_string(),
        });
    }
    if let Some(key_val) = obj.get("key") {
        let mappable = match key_val {
            Value::String(_) => true,
            Value::Array(items) => items.iter().all(Value::is_string),
            _ => false,
        };
        if !mappable {
            diagnostics.push(ConversionDiagnostic {
                severity: DiagnosticSeverity::Warning,
                entry_index: Some(idx),
                entry_name: Some(name.to_string()),
                field: Some("key".to_string()),
                message: "key must be a string or an array of strings; unmappable activation keys dropped".to_string(),
            });
        }
    }
    diagnostics
}

/// Derive a stable pack `entry_id` for an ST entry.
///
/// The documented `uid` (ST-internal unique id) and `id` (UUID) fields are
/// stable across lorebook edits; the array position is not — an insert or
/// reorder shifts every positional id, so a re-import under
/// `ConflictPolicy::Overwrite` would overwrite the wrong entries and under
/// `Skip` would silently drop edits (W-4). Prefer `uid`, then `id`; fall
/// back to the positional `kb_st_{idx}` only when neither exists, and report
/// the fallback (and any duplicate stable id within one lorebook) as a
/// `Warning` diagnostic.
fn stable_entry_id(
    obj: &Map<String, Value>,
    idx: usize,
    name: &str,
    seen: &mut HashSet<String>,
) -> (String, Option<ConversionDiagnostic>) {
    let candidate = obj
        .get("uid")
        .and_then(uid_id_suffix)
        .or_else(|| obj.get("id").and_then(Value::as_str).map(id_suffix))
        .filter(|s| !s.is_empty());
    let Some(base) = candidate else {
        let id = format!("kb_st_{idx}");
        seen.insert(id.clone());
        return (
            id.clone(),
            Some(ConversionDiagnostic {
                severity: DiagnosticSeverity::Warning,
                entry_index: Some(idx),
                entry_name: Some(name.to_string()),
                field: None,
                message: format!(
                    "no stable uid/id field; positional entry id '{id}' is unstable across lorebook edits"
                ),
            }),
        );
    };
    let id = format!("kb_st_{base}");
    if seen.insert(id.clone()) {
        return (id, None);
    }
    // Duplicate stable id within one lorebook cannot anchor re-imports — the
    // positional id is unique; report the collision instead of letting the
    // import overwrite/skip the wrong entry.
    let fallback = format!("kb_st_{idx}");
    seen.insert(fallback.clone());
    let diagnostic = ConversionDiagnostic {
        severity: DiagnosticSeverity::Warning,
        entry_index: Some(idx),
        entry_name: Some(name.to_string()),
        field: None,
        message: format!(
            "duplicate stable id '{id}' in lorebook; using positional id '{fallback}'"
        ),
    };
    (fallback, Some(diagnostic))
}

/// Render the documented `uid` field as a stable id suffix: non-negative
/// integers (the documented shape) and strings both work; other shapes are
/// not stable identity and fall through to the next candidate.
fn uid_id_suffix(v: &Value) -> Option<String> {
    if let Some(n) = v.as_u64() {
        return Some(n.to_string());
    }
    if let Some(n) = v.as_i64() {
        return Some(n.to_string());
    }
    v.as_str().map(id_suffix)
}

/// Keep only id-safe characters (`[A-Za-z0-9_-]`) so arbitrary string
/// uids/ids cannot inject path/URL-hostile characters into the entry id.
fn id_suffix(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect()
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

    // ── W-2: `enabled: false` must not import silently as live content ──

    #[test]
    fn disabled_entry_emits_warning_diagnostic() {
        // W-2: `enabled: false` (documented ST strategy field) is not part
        // of the locked mapping — importing it as live `confirmed` content
        // would silently invert the author's intent. A Warning diagnostic
        // must fire.
        let lorebook = json!({
            "entries": [
                { "uid": 0, "key": "dragon", "content": "Dragon lore.", "comment": "Dragon", "enabled": false }
            ]
        });
        let outcome = parse_st_lorebook(&lorebook).expect("must convert");
        assert_eq!(outcome.diagnostics.len(), 1);
        let d = &outcome.diagnostics[0];
        assert_eq!(d.severity, DiagnosticSeverity::Warning);
        assert_eq!(d.field.as_deref(), Some("enabled"));
        assert_eq!(d.entry_index, Some(0));
        assert!(d.message.contains("enabled=false"));
        // The entry still imports (content preserved) as confirmed.
        let entry = &outcome.pack_input["entries"][0];
        assert_eq!(entry["status"], json!("confirmed"));
        let parsed = parse_pack(&outcome.pack_input).expect("pack must parse");
        assert_eq!(parsed.entries.len(), 1);
    }

    #[test]
    fn enabled_true_produces_no_diagnostic() {
        let lorebook = json!({
            "entries": [
                { "uid": 0, "key": "dragon", "content": "Dragon lore.", "comment": "Dragon", "enabled": true }
            ]
        });
        let outcome = parse_st_lorebook(&lorebook).expect("must convert");
        assert!(outcome.diagnostics.is_empty());
    }

    // ── W-3: documented `key` array form must not silently drop ──

    #[test]
    fn key_array_form_maps_to_activation_keys() {
        // W-3: the documented `key` list form (array of strings) must map to
        // `modules.activation.keys` — no silent drop.
        let lorebook = json!({
            "entries": [
                { "uid": 0, "key": ["dragon", "wyrm"], "content": "Dragon lore.", "comment": "Dragon" }
            ]
        });
        let outcome = parse_st_lorebook(&lorebook).expect("must convert");
        assert!(
            outcome.diagnostics.is_empty(),
            "array key form is documented; got {outcome:#?}"
        );
        let entry = &outcome.pack_input["entries"][0];
        assert_eq!(
            entry["modules"]["activation"]["keys"],
            json!(["dragon", "wyrm"])
        );
        let parsed = parse_pack(&outcome.pack_input).expect("pack must parse");
        assert_eq!(parsed.entries.len(), 1);
    }

    #[test]
    fn key_array_form_backfills_canonical_name() {
        // W-3: `first_key` must also accept the array form (canonical-name
        // backfill for empty comments).
        let lorebook = json!({
            "entries": [
                { "uid": 0, "key": ["harbor", "port"], "content": "The harbor gates.", "comment": "" }
            ]
        });
        let outcome = parse_st_lorebook(&lorebook).expect("must convert");
        assert_eq!(
            outcome.pack_input["entries"][0]["canonical_name"],
            json!("harbor")
        );
    }

    #[test]
    fn unmappable_key_emits_warning_diagnostic() {
        // W-3: a `key` that is neither a string nor an array of strings is
        // unmappable — a Warning diagnostic must fire instead of a silent
        // drop.
        let lorebook = json!({
            "entries": [
                { "uid": 0, "key": 42, "content": "Dragon lore.", "comment": "Dragon" }
            ]
        });
        let outcome = parse_st_lorebook(&lorebook).expect("must convert");
        assert_eq!(outcome.diagnostics.len(), 1);
        let d = &outcome.diagnostics[0];
        assert_eq!(d.severity, DiagnosticSeverity::Warning);
        assert_eq!(d.field.as_deref(), Some("key"));
        assert!(d.message.contains("key"));
        // No activation keys are emitted for the unmappable form.
        let entry = &outcome.pack_input["entries"][0];
        assert!(
            entry.get("modules").is_none() || entry["modules"].get("activation").is_none(),
            "unmappable key must not produce an activation module"
        );
    }

    // ── W-4: stable entry ids across lorebook edits ──

    #[test]
    fn entry_ids_stable_across_middle_insertion() {
        // W-4: entry ids derive from the stable `uid` field, not the array
        // position — inserting an entry at the top must not shift the ids of
        // the unchanged entries.
        let original = json!({
            "entries": [
                { "uid": 0, "key": "alpha", "content": "A", "comment": "Alpha" },
                { "uid": 1, "key": "beta", "content": "B", "comment": "Beta" }
            ]
        });
        let edited = json!({
            "entries": [
                { "uid": 9, "key": "new", "content": "N", "comment": "New" },
                { "uid": 0, "key": "alpha", "content": "A updated", "comment": "Alpha" },
                { "uid": 1, "key": "beta", "content": "B", "comment": "Beta" }
            ]
        });
        let original_out = parse_st_lorebook(&original).expect("must convert");
        let edited_out = parse_st_lorebook(&edited).expect("must convert");
        let ids = |out: &ConversionOutcome| -> Vec<String> {
            out.pack_input["entries"]
                .as_array()
                .expect("entries array")
                .iter()
                .map(|e| e["entry_id"].as_str().expect("entry_id string").to_string())
                .collect()
        };
        assert_eq!(ids(&original_out), vec!["kb_st_0", "kb_st_1"]);
        assert_eq!(ids(&edited_out), vec!["kb_st_9", "kb_st_0", "kb_st_1"]);
        // The id set of the unchanged entries is identical across the edit.
        let unchanged: Vec<String> = ids(&edited_out)
            .into_iter()
            .filter(|id| id != "kb_st_9")
            .collect();
        assert_eq!(
            unchanged,
            vec!["kb_st_0".to_string(), "kb_st_1".to_string()]
        );
    }

    #[test]
    fn id_field_used_when_uid_absent() {
        // W-4: the documented `id` (UUID) field is a stable identity when
        // `uid` is absent.
        let lorebook = json!({
            "entries": [
                { "id": "550e8400-e29b-41d4-a716-446655440000", "key": "alpha", "content": "A", "comment": "Alpha" }
            ]
        });
        let outcome = parse_st_lorebook(&lorebook).expect("must convert");
        assert!(outcome.diagnostics.is_empty());
        assert_eq!(
            outcome.pack_input["entries"][0]["entry_id"],
            json!("kb_st_550e8400-e29b-41d4-a716-446655440000")
        );
    }

    #[test]
    fn missing_uid_id_falls_back_to_positional_with_diagnostic() {
        // W-4: without a stable uid/id, the positional id is used and
        // reported — stability cannot be guaranteed.
        let lorebook = json!({
            "entries": [
                { "key": "alpha", "content": "A", "comment": "Alpha" },
                { "key": "beta", "content": "B", "comment": "Beta" }
            ]
        });
        let outcome = parse_st_lorebook(&lorebook).expect("must convert");
        assert_eq!(
            outcome
                .diagnostics
                .iter()
                .filter(|d| d.message.contains("positional"))
                .count(),
            2,
            "one diagnostic per positional fallback"
        );
        assert_eq!(
            outcome.pack_input["entries"][0]["entry_id"],
            json!("kb_st_0")
        );
        assert_eq!(
            outcome.pack_input["entries"][1]["entry_id"],
            json!("kb_st_1")
        );
    }

    #[test]
    fn duplicate_uid_falls_back_to_positional_with_diagnostic() {
        // W-4: duplicate stable ids within one lorebook cannot anchor
        // re-imports — the second entry falls back to a unique positional id
        // and the collision is reported.
        let lorebook = json!({
            "entries": [
                { "uid": 0, "key": "alpha", "content": "A", "comment": "Alpha" },
                { "uid": 0, "key": "beta", "content": "B", "comment": "Beta" }
            ]
        });
        let outcome = parse_st_lorebook(&lorebook).expect("must convert");
        let ids: Vec<&str> = outcome.pack_input["entries"]
            .as_array()
            .expect("entries array")
            .iter()
            .map(|e| e["entry_id"].as_str().expect("entry_id string"))
            .collect();
        assert_eq!(ids, vec!["kb_st_0", "kb_st_1"]);
        assert!(
            outcome
                .diagnostics
                .iter()
                .any(|d| d.message.contains("duplicate")),
            "duplicate uid must be reported"
        );
    }
}
