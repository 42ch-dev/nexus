//! Key-block accessor helpers (AR-3), generalized from
//! `modules/basic-combat/src/lib.rs` L274–336.

use serde_json::Value;

/// Resolve a key block's identity: spoke `entry_id` (canonical since V1.139)
/// with legacy domain `key_block_id` fallback.
pub fn entry_id_of(kb: &Value) -> Option<&str> {
    kb.get("entry_id")
        .and_then(Value::as_str)
        .or_else(|| kb.get("key_block_id").and_then(Value::as_str))
}

/// Whether a key block is of the given kind: spoke `entry_type` (canonical
/// since V1.139) with legacy domain `block_type` fallback. Replaces the
/// basic-combat `is_character` helper (`is_kind(kb, "character")`).
pub fn is_kind(kb: &Value, kind: &str) -> bool {
    kb.get("entry_type")
        .and_then(Value::as_str)
        .or_else(|| kb.get("block_type").and_then(Value::as_str))
        == Some(kind)
}

/// Read an integer attribute from a key block. Supports both the legacy
/// domain flat-object form (`body.attributes.base_atk`) and the canonical
/// spoke ERC721-array form (`body.attributes[].trait_type`/`value`).
///
/// Spoke attribute values round-trip as JSON floats (`20.0` — the spoke
/// `BodyAttributeValue` number variant is an f64, and the
/// `nexus-spoke-adapter` key-block conversion emits the flat-object form
/// from those float values), so BOTH branches accept integer and float
/// values.
pub fn read_attr_int(kb: &Value, trait_name: &str) -> Option<i64> {
    if let Some(v) = read_int(kb, &["body", "attributes", trait_name]) {
        return Some(v);
    }
    // Flat-object form carrying spoke round-tripped float values (`20.0`):
    // `as_i64` misses f64-backed numbers, so fall back to the float read.
    if let Some(v) = read_int_f64(kb, &["body", "attributes", trait_name]) {
        return Some(v);
    }
    let attrs = kb.get("body")?.get("attributes")?.as_array()?;
    attrs.iter().find_map(|item| {
        if item.get("trait_type").and_then(Value::as_str) == Some(trait_name) {
            item.get("value")
                .and_then(Value::as_i64)
                .or_else(|| item.get("value").and_then(Value::as_f64).map(|f| f as i64))
        } else {
            None
        }
    })
}

/// Read a nested integer along a JSON path; returns `None` on any miss.
pub fn read_int(value: &Value, path: &[&str]) -> Option<i64> {
    let mut cur = value;
    for seg in path {
        cur = cur.get(*seg)?;
    }
    cur.as_i64()
}

/// Read a nested number along a JSON path as an integer, accepting
/// f64-backed JSON numbers (`20.0`) that `as_i64` misses.
pub fn read_int_f64(value: &Value, path: &[&str]) -> Option<i64> {
    let mut cur = value;
    for seg in path {
        cur = cur.get(*seg)?;
    }
    cur.as_f64().map(|f| f as i64)
}

/// Emit a wire-valid `evt_*` timeline id (alphanumeric suffix only).
///
/// Generalized from basic-combat's `wire_timeline_event_id` (lib.rs
/// L116–126): `timeline_event_id("combat", &[attacker_id, defender_id])`
/// produces `evt_combat{attacker}{defender}` with all non-alphanumeric
/// characters stripped from the prefix and parts.
pub fn timeline_event_id(prefix: &str, parts: &[&str]) -> String {
    let mut out = String::from("evt_");
    out.extend(prefix.chars().filter(|c| c.is_ascii_alphanumeric()));
    for part in parts {
        out.extend(part.chars().filter(|c| c.is_ascii_alphanumeric()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn entry_id_of_prefers_entry_id_over_legacy_key_block_id() {
        let kb = json!({
            "entry_id": "kb_new",
            "key_block_id": "kb_legacy",
            "block_type": "character",
        });
        assert_eq!(entry_id_of(&kb), Some("kb_new"));

        let legacy = json!({ "key_block_id": "kb_legacy", "block_type": "character" });
        assert_eq!(entry_id_of(&legacy), Some("kb_legacy"));

        let none = json!({ "block_type": "character" });
        assert_eq!(entry_id_of(&none), None);
    }

    #[test]
    fn is_kind_matches_entry_type_and_block_type() {
        let spoke = json!({ "entry_type": "character" });
        assert!(is_kind(&spoke, "character"));
        assert!(!is_kind(&spoke, "monster"));

        let legacy = json!({ "block_type": "character" });
        assert!(is_kind(&legacy, "character"));
    }

    /// The `nexus-spoke-adapter` key-block conversion emits spoke attribute
    /// values as f64-backed JSON numbers in the FLAT form
    /// (`body.attributes.base_atk` = `20.0` — the spoke `BodyAttributeValue`
    /// number variant is an f64). `read_attr_int` must accept them on the
    /// flat path exactly like the array path already did.
    #[test]
    fn flat_object_attributes_accept_f64_values() {
        let kb = json!({
            "entry_type": "character",
            "body": {
                "attributes": { "max_hp": 100.0, "base_atk": 20.0, "base_def": 5.0 },
            },
        });
        assert_eq!(read_attr_int(&kb, "base_atk"), Some(20));
        assert_eq!(read_attr_int(&kb, "max_hp"), Some(100));
        assert_eq!(read_attr_int(&kb, "base_def"), Some(5));
    }

    /// Integer-valued flat attributes keep working (legacy domain form).
    #[test]
    fn flat_object_attributes_accept_integer_values() {
        let kb = json!({
            "entry_type": "character",
            "body": {
                "attributes": { "max_hp": 100, "base_atk": 20, "base_def": 5 },
            },
        });
        assert_eq!(read_attr_int(&kb, "base_atk"), Some(20));
    }

    /// The canonical ERC721-array form still accepts both int and float
    /// values (unchanged behavior — regression pin).
    #[test]
    fn array_attributes_accept_int_and_float_values() {
        let kb = json!({
            "entry_type": "character",
            "body": {
                "attributes": [
                    { "trait_type": "base_atk", "value": 20 },
                    { "trait_type": "base_def", "value": 5.0 },
                ],
            },
        });
        assert_eq!(read_attr_int(&kb, "base_atk"), Some(20));
        assert_eq!(read_attr_int(&kb, "base_def"), Some(5));
    }

    #[test]
    fn read_int_follows_nested_paths() {
        let value = json!({ "body": { "state": { "character": { "current_hp": 30 } } } });
        assert_eq!(
            read_int(&value, &["body", "state", "character", "current_hp"]),
            Some(30)
        );
        assert_eq!(read_int(&value, &["body", "missing"]), None);
    }

    #[test]
    fn read_int_f64_accepts_float_backed_numbers() {
        let value = json!({ "body": { "attributes": { "base_atk": 20.0 } } });
        assert_eq!(
            read_int_f64(&value, &["body", "attributes", "base_atk"]),
            Some(20)
        );
        assert_eq!(read_int_f64(&value, &["body", "nope"]), None);
    }

    /// `timeline_event_id` must produce the same wire-valid id as
    /// basic-combat's `wire_timeline_event_id` for the same inputs.
    #[test]
    fn timeline_event_id_matches_basic_combat_semantics() {
        assert_eq!(
            timeline_event_id("combat", &["kb-atk", "kb-def"]),
            "evt_combatkbatkkbdef"
        );
        assert_eq!(timeline_event_id("combat", &["a1", "b2"]), "evt_combata1b2");
    }
}
