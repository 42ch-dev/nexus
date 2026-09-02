//! DF-79 — author-defined regex hygiene transforms on the lore emission
//! path (read-path only).
//!
//! A lore author attaches find/replace transforms to an entry via the
//! `body.attributes.hygiene` carrier — a JSON array of
//! `{"pattern": string, "replacement": string, "description"?: string}`
//! objects on the nexus [`WorldKbBody`]. The transforms are applied to the
//! emitted `body.summary` text at slot emission (between the generation-
//! stage gate and slot routing in `moment.rs`), on the owned assembly-local
//! entry copies — the stored World-KB body is never mutated.
//!
//! # Degrade semantics (Q6 mirror)
//!
//! A bad transform never fails the assembly: an invalid regex emits the
//! entry untransformed with an `"invalid regex"` trace note; an oversized
//! pattern is skipped with a note; an oversized output is truncated with a
//! note. Caps mirror the Q6 activation engine (`adapter/activation.rs`):
//! 256 pattern chars / 64 KiB input / 64 KiB output — guardrails, not
//! features.
//!
//! [`WorldKbBody`]: nexus_knowledge::world_kb::knowledge_entry::WorldKbBody

use nexus_knowledge::world_kb::knowledge_entry::WorldKbEntry;
use regex::Regex;

/// Maximum hygiene pattern length (architect lock, Q6-mirror) — chars.
pub const MAX_HYGIENE_PATTERN_CHARS: usize = 256;
/// Input text cap for a hygiene pass (Q6-mirror) — chars.
pub const MAX_HYGIENE_INPUT_CHARS: usize = 64 * 1024;
/// Output text cap after a hygiene pass (Q6-mirror) — chars.
pub const MAX_HYGIENE_OUTPUT_CHARS: usize = 64 * 1024;

/// One author-defined find/replace transform, parsed from
/// `body.attributes.hygiene`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HygieneTransform {
    /// Regex pattern matched against the emitted summary.
    pub pattern: String,
    /// Replacement text (`$1` / `${name}` backrefs supported).
    pub replacement: String,
    /// Optional author note — parsed but never actioned.
    pub description: Option<String>,
}

/// Per-entry hygiene trace row (inspector `hygiene` section).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HygieneTraceEntry {
    /// The carrier-bearing entry's stable id.
    pub entry_id: String,
    /// Performed replacements (total across all applied transforms).
    pub applied: usize,
    /// No-match and degraded (invalid/capped) transforms.
    pub skipped: usize,
    /// `"invalid regex"` / truncation / malformed-carrier diagnostics
    /// (Q6 mirror — no-match is silent).
    pub notes: Vec<String>,
}

/// Apply each entry's `body.attributes.hygiene` transforms to the emitted
/// `body.summary` text on the owned assembly-local copies.
///
/// Read-path only: the returned entries are the caller's owned copies; the
/// stored World-KB rows are never touched. Entries without a `hygiene`
/// carrier pass through unchanged and produce no trace rows.
#[must_use]
pub fn apply_hygiene(entries: Vec<WorldKbEntry>) -> (Vec<WorldKbEntry>, Vec<HygieneTraceEntry>) {
    let mut trace = Vec::new();
    let mut out = Vec::with_capacity(entries.len());
    for mut entry in entries {
        let Some(body) = entry.body.as_mut() else {
            out.push(entry);
            continue;
        };
        let Some(attributes) = body.attributes.as_ref() else {
            out.push(entry);
            continue;
        };
        let Some(carrier) = attributes.get("hygiene") else {
            out.push(entry);
            continue;
        };
        let (transforms, parse_notes, malformed) = parse_carrier(carrier);
        let mut row = HygieneTraceEntry {
            entry_id: entry.entry_id.clone(),
            applied: 0,
            skipped: malformed,
            notes: parse_notes,
        };
        if let Some(summary) = body.summary.take() {
            let mut text = summary;
            // Input cap (Q6 mirror): the scan text is truncated before the
            // first pass — silent, like the activation engine's scan cap.
            if text.chars().count() > MAX_HYGIENE_INPUT_CHARS {
                text = truncate_chars(&text, MAX_HYGIENE_INPUT_CHARS).to_string();
            }
            for t in &transforms {
                if t.pattern.chars().count() > MAX_HYGIENE_PATTERN_CHARS {
                    row.skipped += 1;
                    row.notes.push(format!(
                        "hygiene pattern over {MAX_HYGIENE_PATTERN_CHARS} chars skipped"
                    ));
                    continue;
                }
                let Ok(re) = Regex::new(&t.pattern) else {
                    row.skipped += 1;
                    row.notes.push("invalid regex".to_string());
                    continue;
                };
                let mut applied = 0;
                let replaced = re.replace_all(&text, |caps: &regex::Captures<'_>| {
                    applied += 1;
                    let mut dst = String::new();
                    caps.expand(&t.replacement, &mut dst);
                    dst
                });
                if applied == 0 {
                    // No-match — silent skip (Q6 mirror).
                    row.skipped += 1;
                    continue;
                }
                row.applied += applied;
                let replaced = replaced.into_owned();
                if replaced.chars().count() > MAX_HYGIENE_OUTPUT_CHARS {
                    row.notes.push(format!(
                        "hygiene output over {MAX_HYGIENE_OUTPUT_CHARS} chars truncated"
                    ));
                    text = truncate_chars(&replaced, MAX_HYGIENE_OUTPUT_CHARS).to_string();
                } else {
                    text = replaced;
                }
            }
            body.summary = Some(text);
        } else {
            // Carrier present but no summary text — every transform is a
            // no-match (nothing to scan).
            row.skipped += transforms.len();
        }
        trace.push(row);
        out.push(entry);
    }
    (out, trace)
}

/// Parse the `body.attributes.hygiene` carrier — a JSON array of
/// `{"pattern": string, "replacement": string, "description"?: string}`
/// objects. Malformed elements are skipped and reported in `notes`; the
/// returned count is the number of malformed elements (degraded transforms).
fn parse_carrier(carrier: &serde_json::Value) -> (Vec<HygieneTransform>, Vec<String>, usize) {
    let Some(items) = carrier.as_array() else {
        return (
            Vec::new(),
            vec!["hygiene carrier must be an array of transforms".to_string()],
            0,
        );
    };
    let mut transforms = Vec::new();
    let mut notes = Vec::new();
    let mut malformed = 0;
    for item in items {
        let Some(obj) = item.as_object() else {
            malformed += 1;
            notes.push(
                "hygiene transform must be an object with string pattern and replacement"
                    .to_string(),
            );
            continue;
        };
        let Some(pattern) = obj.get("pattern").and_then(serde_json::Value::as_str) else {
            malformed += 1;
            notes.push(
                "hygiene transform must be an object with string pattern and replacement"
                    .to_string(),
            );
            continue;
        };
        let Some(replacement) = obj.get("replacement").and_then(serde_json::Value::as_str) else {
            malformed += 1;
            notes.push(
                "hygiene transform must be an object with string pattern and replacement"
                    .to_string(),
            );
            continue;
        };
        let description = obj
            .get("description")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        transforms.push(HygieneTransform {
            pattern: pattern.to_string(),
            replacement: replacement.to_string(),
            description,
        });
    }
    (transforms, notes, malformed)
}

/// First `max_chars` characters of `text` (stable prefix).
fn truncate_chars(text: &str, max_chars: usize) -> &str {
    text.char_indices()
        .nth(max_chars)
        .map_or(text, |(idx, _)| &text[..idx])
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_contracts::BlockType;
    use nexus_knowledge::world_kb::knowledge_entry::WorldKbBody;

    fn entry_with_hygiene(
        entry_id: &str,
        summary: &str,
        carrier: &serde_json::Value,
    ) -> WorldKbEntry {
        let mut entry = WorldKbEntry::new("wld_1", BlockType::Character, "Hero");
        entry.entry_id = entry_id.to_string();
        entry.body = Some(WorldKbBody {
            summary: Some(summary.to_string()),
            attributes: Some(serde_json::json!({ "hygiene": carrier })),
            ..WorldKbBody::default()
        });
        entry
    }

    fn entry_without_carrier(entry_id: &str, summary: &str) -> WorldKbEntry {
        let mut entry = WorldKbEntry::new("wld_1", BlockType::Character, "Hero");
        entry.entry_id = entry_id.to_string();
        entry.body = Some(WorldKbBody {
            summary: Some(summary.to_string()),
            ..WorldKbBody::default()
        });
        entry
    }

    #[test]
    fn transform_applies_to_summary() {
        let (out, trace) = apply_hygiene(vec![entry_with_hygiene(
            "kb_1",
            "The hero fights the dragon",
            &serde_json::json!([{ "pattern": "dragon", "replacement": "wyrm" }]),
        )]);
        let summary = out[0].body.as_ref().unwrap().summary.as_deref().unwrap();
        assert_eq!(summary, "The hero fights the wyrm");
        assert_eq!(trace.len(), 1);
        assert_eq!(trace[0].entry_id, "kb_1");
        assert_eq!(trace[0].applied, 1);
        assert_eq!(trace[0].skipped, 0);
        assert!(trace[0].notes.is_empty());
    }

    #[test]
    fn multiple_transforms_apply_in_declared_order() {
        let (out, trace) = apply_hygiene(vec![entry_with_hygiene(
            "kb_1",
            "foo",
            &serde_json::json!([
                { "pattern": "foo", "replacement": "bar" },
                { "pattern": "bar", "replacement": "baz" },
            ]),
        )]);
        let summary = out[0].body.as_ref().unwrap().summary.as_deref().unwrap();
        // Declared order: foo → bar → baz. Reversed order would leave "foo".
        assert_eq!(summary, "baz");
        assert_eq!(trace[0].applied, 2);
        assert_eq!(trace[0].skipped, 0);
    }

    #[test]
    fn invalid_regex_degrades_untransformed() {
        let (out, trace) = apply_hygiene(vec![entry_with_hygiene(
            "kb_1",
            "The hero fights the dragon",
            &serde_json::json!([{ "pattern": "[", "replacement": "x" }]),
        )]);
        let summary = out[0].body.as_ref().unwrap().summary.as_deref().unwrap();
        assert_eq!(summary, "The hero fights the dragon");
        assert_eq!(trace[0].applied, 0);
        assert_eq!(trace[0].skipped, 1);
        assert_eq!(trace[0].notes, vec!["invalid regex".to_string()]);
    }

    #[test]
    fn oversized_pattern_skipped_with_note() {
        let long_pattern = "x".repeat(MAX_HYGIENE_PATTERN_CHARS + 1);
        let (out, trace) = apply_hygiene(vec![entry_with_hygiene(
            "kb_1",
            "The hero fights the dragon",
            &serde_json::json!([{ "pattern": long_pattern, "replacement": "y" }]),
        )]);
        let summary = out[0].body.as_ref().unwrap().summary.as_deref().unwrap();
        assert_eq!(summary, "The hero fights the dragon");
        assert_eq!(trace[0].applied, 0);
        assert_eq!(trace[0].skipped, 1);
        assert_eq!(trace[0].notes.len(), 1);
        assert!(trace[0].notes[0].contains("chars skipped"));
    }

    #[test]
    fn oversized_output_truncated_with_note() {
        let big_replacement = "y".repeat(MAX_HYGIENE_OUTPUT_CHARS + 1);
        let (out, trace) = apply_hygiene(vec![entry_with_hygiene(
            "kb_1",
            "x",
            &serde_json::json!([{ "pattern": "x", "replacement": big_replacement }]),
        )]);
        let summary = out[0].body.as_ref().unwrap().summary.as_deref().unwrap();
        assert_eq!(summary.chars().count(), MAX_HYGIENE_OUTPUT_CHARS);
        assert_eq!(trace[0].applied, 1);
        assert_eq!(trace[0].skipped, 0);
        assert_eq!(trace[0].notes.len(), 1);
        assert!(trace[0].notes[0].contains("truncated"));
    }

    #[test]
    fn neutral_entries_produce_no_trace_rows_and_byte_identical() {
        let input = vec![
            entry_without_carrier("kb_1", "The hero fights the dragon"),
            entry_without_carrier("kb_2", "The castle stands tall"),
        ];
        let (out, trace) = apply_hygiene(input.clone());
        assert!(trace.is_empty());
        assert_eq!(
            out, input,
            "no-carrier entries must pass through byte-identical"
        );
    }

    #[test]
    fn no_match_counts_skipped_silently() {
        let (out, trace) = apply_hygiene(vec![entry_with_hygiene(
            "kb_1",
            "The hero fights the dragon",
            &serde_json::json!([{ "pattern": "zzz", "replacement": "y" }]),
        )]);
        let summary = out[0].body.as_ref().unwrap().summary.as_deref().unwrap();
        assert_eq!(summary, "The hero fights the dragon");
        assert_eq!(trace[0].applied, 0);
        assert_eq!(trace[0].skipped, 1);
        assert!(trace[0].notes.is_empty(), "no-match is silent (Q6 mirror)");
    }

    #[test]
    fn malformed_carrier_and_transforms_degrades() {
        // Non-array carrier → note, no transforms, summary unchanged.
        let (out, trace) = apply_hygiene(vec![entry_with_hygiene(
            "kb_1",
            "The hero fights the dragon",
            &serde_json::json!("not-an-array"),
        )]);
        let summary = out[0].body.as_ref().unwrap().summary.as_deref().unwrap();
        assert_eq!(summary, "The hero fights the dragon");
        assert_eq!(trace[0].applied, 0);
        assert_eq!(trace[0].notes.len(), 1);
        assert!(trace[0].notes[0].contains("array"));

        // Malformed element (missing replacement) → skipped + note.
        let (out, trace) = apply_hygiene(vec![entry_with_hygiene(
            "kb_2",
            "The hero fights the dragon",
            &serde_json::json!([{ "pattern": "dragon" }]),
        )]);
        let summary = out[0].body.as_ref().unwrap().summary.as_deref().unwrap();
        assert_eq!(summary, "The hero fights the dragon");
        assert_eq!(trace[0].skipped, 1);
        assert_eq!(trace[0].notes.len(), 1);
    }

    #[test]
    fn carrier_without_summary_counts_all_skipped() {
        let mut entry = WorldKbEntry::new("wld_1", BlockType::Character, "Hero");
        entry.entry_id = "kb_1".to_string();
        entry.body = Some(WorldKbBody {
            summary: None,
            attributes: Some(serde_json::json!({ "hygiene": [
                { "pattern": "dragon", "replacement": "wyrm" },
                { "pattern": "hero", "replacement": "champion" },
            ] })),
            ..WorldKbBody::default()
        });
        let (out, trace) = apply_hygiene(vec![entry]);
        assert!(out[0].body.as_ref().unwrap().summary.is_none());
        assert_eq!(trace[0].applied, 0);
        assert_eq!(trace[0].skipped, 2);
    }

    #[test]
    fn carrier_parses_description() {
        let (transforms, notes, malformed) = parse_carrier(&serde_json::json!([
            { "pattern": "a", "replacement": "b", "description": "fix a" },
            { "pattern": "c", "replacement": "d" },
        ]));
        assert!(notes.is_empty());
        assert_eq!(malformed, 0);
        assert_eq!(transforms.len(), 2);
        assert_eq!(transforms[0].description.as_deref(), Some("fix a"));
        assert_eq!(transforms[1].description, None);
    }
}
