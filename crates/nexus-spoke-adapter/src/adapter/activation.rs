//! Lore activation engine — scans `WorldKB` entries against a stage-0 moment
//! context and classifies them by `modules.activation` fire-conditions.
//!
//! V1.146 P4 T1: the activation engine lives in the spoke adapter (not MCA)
//! because `modules.activation` is a spoke `narrative-modules` dialect. MCA
//! calls `apply_activation` when the `NEXUS_MCA_LORE_ACTIVATION` flag is ON
//! (T2), and the engine operates on the already-fetched entries — MCA stays
//! generic (plan Architecture Lock Decision 5).
//!
//! # Activation logic
//!
//! | Logic    | Entry passes when…                                   |
//! |----------|------------------------------------------------------|
//! | `and_any`| ANY activation key matches ANY scan field             |
//! | `and_all`| ALL activation keys match (logical AND across keys)   |
//! | `not_any`| Entry EXCLUDED if ANY key matches (exclusion list)    |
//! | `not_all`| Entry EXCLUDED only if ALL keys match                 |
//!
//! # Scan fields
//!
//! (Case-insensitive substring match):
//! - `stage0_context` (the full Stage-0 assembled string)
//! - Entry `canonical_name`
//! - Entry `body.summary` (if present)
//! - Entry `body.content` (if present) — currently no nexus field; scan `body.summary` + `body.state` text
//!
//! `constant_seeds` entries are always included regardless of key match.

use nexus_knowledge::world_kb::knowledge_entry::WorldKbEntry;
use serde::{Deserialize, Serialize};

/// Result of an activation pass over a set of `WorldKB` entries.
#[derive(Debug, Clone)]
pub struct ActivationResult {
    /// Entries that passed activation (or are `constant_seeds`, or have no
    /// activation module — neutral entries are included as matched).
    pub matched: Vec<WorldKbEntry>,
    /// Entries that did not pass activation and are not constant seeds.
    pub unmatched: Vec<WorldKbEntry>,
    /// Per-entry fire/miss trace for diagnostics.
    pub trace: Vec<ActivationTraceEntry>,
}

/// Per-entry activation trace record.
#[derive(Debug, Clone, Serialize)]
pub struct ActivationTraceEntry {
    pub entry_id: String,
    pub canonical_name: String,
    /// Why this entry was matched or not.
    pub reason: String,
    /// Whether the entry ended up in `matched`.
    pub accepted: bool,
}

/// The parsed `modules.activation` data from a single entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
struct ActivationConfig {
    /// Entry IDs that are always included regardless of key match.
    #[serde(default)]
    constant_seeds: Vec<String>,
    /// Activation key strings (case-insensitive substring match targets).
    #[serde(default)]
    key: Vec<String>,
    /// Fire logic: `and_any`, `and_all`, `not_any`, `not_all`.
    /// Defaults to `and_any` when absent (most permissive).
    #[serde(default = "default_logic")]
    logic: String,
}

fn default_logic() -> String {
    "and_any".to_string()
}

/// Apply lore activation to a set of `WorldKB` entries.
///
/// For each entry:
/// 1. If `modules.activation` is absent or `modules` is absent → entry is
///    "neutral" and included in `matched` with reason "no activation module".
/// 2. If `constant_seeds` includes this entry's `entry_id` → always matched.
/// 3. Otherwise, evaluate the `activation.key` set against the scan fields
///    using `activation.logic`.
///
/// # Arguments
///
/// * `entries` — Slice of `WorldKB` entries to classify.
/// * `stage0_context` — The full Stage-0 assembled moment context string
///   (personality + experience + fragments + prompt). Used as a scan target.
/// * `constant_seed_ids` — Additional entry IDs that should always be included
///   (caller-supplied constant seeds, e.g. from a global config). Merged with
///   per-entry `constant_seeds`.
#[must_use]
pub fn apply_activation(
    entries: &[WorldKbEntry],
    stage0_context: &str,
    constant_seed_ids: &[String],
) -> ActivationResult {
    let stage0_lower = stage0_context.to_lowercase();
    let mut matched = Vec::new();
    let mut unmatched = Vec::new();
    let mut trace = Vec::new();

    for entry in entries {
        let entry_id = entry.entry_id.clone();
        let canonical_name = entry.canonical_name.clone();

        // Compute scan text for this entry: canonical_name + body.summary + body content.
        let mut entry_text = canonical_name.to_lowercase();
        if let Some(ref body) = entry.body {
            if let Some(ref summary) = body.summary {
                entry_text.push(' ');
                entry_text.push_str(&summary.to_lowercase());
            }
        }
        // Combine with stage0 text for matching.
        let scan_text = format!("{stage0_lower} {entry_text}");

        // Check if this entry is a constant seed (plan or fallback).
        let is_constant_seed = constant_seed_ids.contains(&entry_id);

        // Parse modules.activation.
        let activation = entry
            .modules
            .as_ref()
            .and_then(|m| m.get("activation"))
            .and_then(|v| serde_json::from_value::<ActivationConfig>(v.clone()).ok());

        let (accepted, reason) = match activation {
            None => {
                // No activation module → neutral, always included.
                (true, "no activation module".to_string())
            }
            Some(ref cfg) if is_constant_seed || cfg.constant_seeds.contains(&entry_id) => {
                let seed_source = if is_constant_seed {
                    "caller-supplied"
                } else {
                    "self-declared"
                };
                (true, format!("constant seed ({seed_source})"))
            }
            Some(ref cfg) => {
                if cfg.key.is_empty() {
                    // Empty keys → no match criteria, treat as neutral.
                    (true, "no activation keys (neutral)".to_string())
                } else {
                    evaluate_activation(cfg, &scan_text)
                }
            }
        };

        trace.push(ActivationTraceEntry {
            entry_id: entry_id.clone(),
            canonical_name: canonical_name.clone(),
            reason: reason.clone(),
            accepted,
        });

        if accepted {
            matched.push(entry.clone());
        } else {
            unmatched.push(entry.clone());
        }
    }

    ActivationResult {
        matched,
        unmatched,
        trace,
    }
}

/// Evaluate `cfg.logic` against the combined scan text.
///
/// Returns `(accepted, reason)`.
///
/// Four logic arms + unknown→`and_any` fallback keep this slightly over the
/// pedantic line budget; splitting would obscure the exhaustive match table.
#[allow(clippy::too_many_lines)] // exhaustive activation.logic match table
fn evaluate_activation(cfg: &ActivationConfig, scan_text: &str) -> (bool, String) {
    let lower_keys: Vec<String> = cfg.key.iter().map(|k| k.to_lowercase()).collect();

    match cfg.logic.as_str() {
        "and_any" => {
            let matches: Vec<&str> = lower_keys
                .iter()
                .filter(|k| scan_text.contains(k.as_str()))
                .map(String::as_str)
                .collect();
            if matches.is_empty() {
                (
                    false,
                    format!("and_any: no key matched ({} keys scanned)", cfg.key.len()),
                )
            } else {
                let matched_keys = matches.join(", ");
                (true, format!("and_any: matched keys [{matched_keys}]"))
            }
        }
        "and_all" => {
            let all_matched = lower_keys.iter().all(|k| scan_text.contains(k.as_str()));
            if all_matched {
                (true, format!("and_all: all {} keys matched", cfg.key.len()))
            } else {
                let missing: Vec<&str> = lower_keys
                    .iter()
                    .filter(|k| !scan_text.contains(k.as_str()))
                    .map(String::as_str)
                    .collect();
                (
                    false,
                    format!(
                        "and_all: {} of {} keys missing [{missing}]",
                        missing.len(),
                        cfg.key.len(),
                        missing = missing.join(", ")
                    ),
                )
            }
        }
        "not_any" => {
            let any_matched = lower_keys.iter().any(|k| scan_text.contains(k.as_str()));
            if any_matched {
                let matched_keys: Vec<&str> = lower_keys
                    .iter()
                    .filter(|k| scan_text.contains(k.as_str()))
                    .map(String::as_str)
                    .collect();
                (
                    false,
                    format!(
                        "not_any: exclusion triggered by [{matched}]",
                        matched = matched_keys.join(", ")
                    ),
                )
            } else {
                (
                    true,
                    format!(
                        "not_any: no exclusion keys matched ({} keys scanned)",
                        cfg.key.len()
                    ),
                )
            }
        }
        "not_all" => {
            let all_matched = lower_keys.iter().all(|k| scan_text.contains(k.as_str()));
            if all_matched {
                (
                    false,
                    format!(
                        "not_all: all {} exclusion keys matched — entry excluded",
                        cfg.key.len()
                    ),
                )
            } else {
                let missing_count = lower_keys
                    .iter()
                    .filter(|k| !scan_text.contains(k.as_str()))
                    .count();
                (
                    true,
                    format!(
                        "not_all: {}/{} keys missing — entry included",
                        missing_count,
                        cfg.key.len()
                    ),
                )
            }
        }
        other => {
            // Unknown logic → treat as and_any (most permissive fallback),
            // actually run matching so the entry is only accepted when a key hits.
            let matches: Vec<&str> = lower_keys
                .iter()
                .filter(|k| scan_text.contains(k.as_str()))
                .map(String::as_str)
                .collect();
            if matches.is_empty() {
                (
                    false,
                    format!(
                        "unknown activation logic '{other}' — treated as and_any: no key matched ({} keys scanned)",
                        cfg.key.len()
                    ),
                )
            } else {
                let matched_keys = matches.join(", ");
                (
                    true,
                    format!(
                        "unknown activation logic '{other}' — treated as and_any: matched keys [{matched_keys}]"
                    ),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_contracts::BlockType;
    use nexus_knowledge::world_kb::knowledge_entry::WorldKbBody;
    use serde_json::json;

    /// Helper: build a WorldKbEntry with modules.activation.
    fn entry_with_activation(
        id: &str,
        name: &str,
        summary: &str,
        modules_val: Option<serde_json::Value>,
    ) -> WorldKbEntry {
        let mut entry = WorldKbEntry::new("wld_test", BlockType::Character, name);
        // Override the random entry_id with a predictable one for tests
        entry.entry_id = id.to_string();
        entry.body = Some(WorldKbBody {
            summary: Some(summary.to_string()),
            ..Default::default()
        });
        entry.modules = modules_val;
        entry
    }

    /// Helper: build activation JSON with keys and logic.
    fn activation_json(keys: &[&str], logic: &str) -> serde_json::Value {
        json!({
            "activation": {
                "key": keys,
                "logic": logic,
            }
        })
    }

    // ── and_any ──────────────────────────────────────────────────────

    #[test]
    fn test_and_any_matches_when_one_key_hits() {
        let entries = vec![entry_with_activation(
            "kb_1",
            "Old King",
            "The elderly ruler of the northern kingdom",
            Some(activation_json(&["king", "dragon"], "and_any")),
        )];
        let stage0 = "The story takes place in a northern kingdom ruled by an old king.";
        let result = apply_activation(&entries, stage0, &[]);
        assert_eq!(result.matched.len(), 1);
        assert_eq!(result.unmatched.len(), 0);
        assert!(result.trace[0].reason.contains("matched keys [king]"));
    }

    #[test]
    fn test_and_any_no_match_when_no_key_hits() {
        let entries = vec![entry_with_activation(
            "kb_2",
            "Dragon",
            "A fearsome dragon",
            Some(activation_json(&["elf", "wizard"], "and_any")),
        )];
        let stage0 = "The story takes place in a northern kingdom.";
        let result = apply_activation(&entries, stage0, &[]);
        assert_eq!(result.matched.len(), 0);
        assert_eq!(result.unmatched.len(), 1);
        assert!(result.trace[0].reason.contains("no key matched"));
    }

    #[test]
    fn test_and_any_case_insensitive_match() {
        let entries = vec![entry_with_activation(
            "kb_3",
            "King Arthur",
            "Ruler of Camelot",
            Some(activation_json(&["KING"], "and_any")),
        )];
        let stage0 = "The king ruled wisely.";
        let result = apply_activation(&entries, stage0, &[]);
        assert_eq!(result.matched.len(), 1);
        assert!(result.trace[0].accepted);
    }

    #[test]
    fn test_and_any_matches_stage0_context() {
        let entries = vec![entry_with_activation(
            "kb_4",
            "Forest Spirit",
            "A mysterious entity",
            Some(activation_json(&["elderly"], "and_any")),
        )];
        // "elderly" appears in stage0 context, not in entry fields.
        let stage0 = "An elderly wizard cast a spell.";
        let result = apply_activation(&entries, stage0, &[]);
        assert_eq!(result.matched.len(), 1);
        assert!(result.trace[0].reason.contains("matched keys"));
    }

    #[test]
    fn test_and_any_matches_entry_body_summary() {
        let entries = vec![entry_with_activation(
            "kb_5",
            "Unknown",
            "The blacksmith forged a legendary sword",
            Some(activation_json(&["blacksmith"], "and_any")),
        )];
        // "blacksmith" only in body.summary, not in stage0 or canonical_name.
        let stage0 = "The town was quiet.";
        let result = apply_activation(&entries, stage0, &[]);
        assert_eq!(result.matched.len(), 1);
        assert!(result.trace[0].accepted);
    }

    #[test]
    fn test_and_any_matches_canonical_name() {
        let entries = vec![entry_with_activation(
            "kb_6",
            "Merlin the Wise",
            "An old wizard",
            Some(activation_json(&["merlin"], "and_any")),
        )];
        let stage0 = "Magic filled the air.";
        let result = apply_activation(&entries, stage0, &[]);
        assert_eq!(result.matched.len(), 1);
        assert!(result.trace[0].accepted);
    }

    // ── and_all ─────────────────────────────────────────────────────

    #[test]
    fn test_and_all_matches_when_all_keys_hit() {
        let entries = vec![entry_with_activation(
            "kb_7",
            "Royal Guard",
            "Protector of the throne",
            Some(activation_json(&["king", "throne", "guard"], "and_all")),
        )];
        let stage0 = "The king sat on the throne while the guard stood watch.";
        let result = apply_activation(&entries, stage0, &[]);
        assert_eq!(result.matched.len(), 1);
        assert!(result.trace[0].reason.contains("all 3 keys matched"));
    }

    #[test]
    fn test_and_all_fails_when_one_key_misses() {
        let entries = vec![entry_with_activation(
            "kb_8",
            "Royal Guard",
            "Protector of the throne",
            Some(activation_json(&["king", "dragon"], "and_all")),
        )];
        let stage0 = "The king sat on the throne.";
        let result = apply_activation(&entries, stage0, &[]);
        assert_eq!(result.matched.len(), 0);
        assert!(result.trace[0]
            .reason
            .contains("1 of 2 keys missing [dragon]"));
    }

    // ── not_any ─────────────────────────────────────────────────────

    #[test]
    fn test_not_any_excludes_when_any_key_hits() {
        let entries = vec![entry_with_activation(
            "kb_9",
            "Orc Warlord",
            "A brutal commander",
            Some(activation_json(&["orc", "warlord"], "not_any")),
        )];
        let stage0 = "The orc army marched forward.";
        let result = apply_activation(&entries, stage0, &[]);
        assert_eq!(result.matched.len(), 0);
        assert_eq!(result.unmatched.len(), 1);
        assert!(result.trace[0].reason.contains("exclusion triggered"));
    }

    #[test]
    fn test_not_any_includes_when_no_key_hits() {
        let entries = vec![entry_with_activation(
            "kb_10",
            "Peaceful Elf",
            "A gentle forest dweller",
            Some(activation_json(&["orc", "warlord"], "not_any")),
        )];
        let stage0 = "The elves sang in the forest.";
        let result = apply_activation(&entries, stage0, &[]);
        assert_eq!(result.matched.len(), 1);
        assert!(result.trace[0].reason.contains("no exclusion keys matched"));
    }

    // ── not_all ─────────────────────────────────────────────────────

    #[test]
    fn test_not_all_includes_when_some_keys_miss() {
        let entries = vec![entry_with_activation(
            "kb_11",
            "Dark Wizard",
            "A powerful spellcaster",
            Some(activation_json(
                &["dark", "wizard", "necromancer"],
                "not_all",
            )),
        )];
        let stage0 = "The dark wizard cast a spell.";
        let result = apply_activation(&entries, stage0, &[]);
        // "dark" and "wizard" match, but "necromancer" does not → entry included.
        assert_eq!(result.matched.len(), 1);
        assert!(result.trace[0].reason.contains("1/3 keys missing"));
    }

    #[test]
    fn test_not_all_excludes_when_all_keys_hit() {
        let entries = vec![entry_with_activation(
            "kb_12",
            "Dark Wizard",
            "A powerful spellcaster",
            Some(activation_json(&["dark", "wizard"], "not_all")),
        )];
        let stage0 = "The dark wizard cast a spell.";
        let result = apply_activation(&entries, stage0, &[]);
        assert_eq!(result.matched.len(), 0);
        assert_eq!(result.unmatched.len(), 1);
        assert!(result.trace[0]
            .reason
            .contains("all 2 exclusion keys matched"));
    }

    // ── constant_seeds ──────────────────────────────────────────────

    #[test]
    fn test_constant_seeds_always_match() {
        let entries = vec![entry_with_activation(
            "kb_cs_1",
            "Seed Entry",
            "This should always be included",
            Some(activation_json(&["nonexistent"], "and_all")),
        )];
        let stage0 = "This story has nothing matching.";
        let result = apply_activation(&entries, stage0, &["kb_cs_1".to_string()]);
        assert_eq!(result.matched.len(), 1);
        assert!(result.trace[0]
            .reason
            .contains("constant seed (caller-supplied)"));
    }

    #[test]
    fn test_self_declared_constant_seeds() {
        let mut activation = activation_json(&["nonexistent"], "and_all");
        activation["activation"]["constant_seeds"] = json!(["kb_self_1"]);
        let entries = vec![entry_with_activation(
            "kb_self_1",
            "Self Seed",
            "Always included",
            Some(activation),
        )];
        let stage0 = "Nothing matches.";
        let result = apply_activation(&entries, stage0, &[]);
        assert_eq!(result.matched.len(), 1);
        assert!(result.trace[0]
            .reason
            .contains("constant seed (self-declared)"));
    }

    // ── edge cases ──────────────────────────────────────────────────

    #[test]
    fn test_neutral_entry_no_modules() {
        let entries = vec![entry_with_activation(
            "kb_n1",
            "Neutral",
            "No modules",
            None,
        )];
        let stage0 = "Any text.";
        let result = apply_activation(&entries, stage0, &[]);
        assert_eq!(result.matched.len(), 1);
        assert!(result.trace[0].reason.contains("no activation module"));
    }

    #[test]
    fn test_neutral_entry_modules_without_activation() {
        let entries = vec![entry_with_activation(
            "kb_n2",
            "Other Module",
            "Has modules but not activation",
            Some(json!({"pack": {"version": 1}})),
        )];
        let stage0 = "Any text.";
        let result = apply_activation(&entries, stage0, &[]);
        assert_eq!(result.matched.len(), 1);
        assert!(result.trace[0].reason.contains("no activation module"));
    }

    #[test]
    fn test_empty_activation_keys_neutral() {
        let entries = vec![entry_with_activation(
            "kb_ek",
            "Empty Keys",
            "No keys defined",
            Some(activation_json(&[], "and_any")),
        )];
        let stage0 = "Any text.";
        let result = apply_activation(&entries, stage0, &[]);
        assert_eq!(result.matched.len(), 1);
        assert!(result.trace[0].reason.contains("no activation keys"));
    }

    #[test]
    fn test_null_modules() {
        let entries = vec![entry_with_activation(
            "kb_null",
            "Null Modules",
            "No modules",
            Some(json!(null)),
        )];
        let stage0 = "Text.";
        let result = apply_activation(&entries, stage0, &[]);
        // modules is Some(null) → get("activation") on null returns None → neutral.
        assert_eq!(result.matched.len(), 1);
        assert!(result.trace[0].reason.contains("no activation module"));
    }

    #[test]
    fn test_no_matches_all_unmatched() {
        let entries = vec![
            entry_with_activation(
                "kb_x1",
                "X1",
                "First",
                Some(activation_json(&["zzz_nonexistent"], "and_any")),
            ),
            entry_with_activation(
                "kb_x2",
                "X2",
                "Second",
                Some(activation_json(&["qqq_also_nonexistent"], "and_all")),
            ),
        ];
        let stage0 = "Nothing relevant here.";
        let result = apply_activation(&entries, stage0, &[]);
        assert_eq!(result.matched.len(), 0);
        assert_eq!(result.unmatched.len(), 2);
    }

    #[test]
    fn test_mixed_matched_and_unmatched() {
        let entries = vec![
            entry_with_activation(
                "kb_m1",
                "Match",
                "Contains king",
                Some(activation_json(&["king"], "and_any")),
            ),
            entry_with_activation(
                "kb_m2",
                "NoMatch",
                "Nothing",
                Some(activation_json(&["dragon"], "and_any")),
            ),
            entry_with_activation("kb_m3", "Neutral", "No activation", None),
        ];
        let stage0 = "The king ruled.";
        let result = apply_activation(&entries, stage0, &[]);
        assert_eq!(result.matched.len(), 2); // kb_m1 (matched) + kb_m3 (neutral)
        assert_eq!(result.unmatched.len(), 1); // kb_m2
    }

    // ── unknown logic ───────────────────────────────────────────────

    #[test]
    fn test_unknown_logic_treated_as_and_any_with_matching() {
        // Unknown logic "fuzzy" must actually run and_any matching.
        let entries = vec![
            entry_with_activation(
                "kb_u1",
                "Matched",
                "Contains king",
                Some(activation_json(&["king"], "fuzzy")),
            ),
            entry_with_activation(
                "kb_u2",
                "Unmatched",
                "Nothing relevant",
                Some(activation_json(&["dragon"], "fuzzy")),
            ),
        ];
        let stage0 = "The king ruled the land.";
        let result = apply_activation(&entries, stage0, &[]);
        assert_eq!(result.matched.len(), 1);
        assert_eq!(result.unmatched.len(), 1);
        assert!(result.trace[0]
            .reason
            .contains("treated as and_any: matched keys"));
        assert!(result.trace[0].accepted);
        assert!(result.trace[1].reason.contains("no key matched"));
        assert!(!result.trace[1].accepted);
    }

    // ── modules durability round-trip test ──────────────────────────
    //
    // Proves that modules.activation survives world↔spoke conversion.

    #[test]
    fn test_modules_activation_survives_spoke_roundtrip() {
        use crate::conversion::{spoke_to_world_kb, world_kb_to_spoke};

        let mut entry = WorldKbEntry::new("wld_test", BlockType::Character, "Hero");
        entry.entry_id = "kb_rt1".to_string();
        entry.body = Some(WorldKbBody {
            summary: Some("The hero of the story".to_string()),
            ..Default::default()
        });
        entry.modules = Some(json!({
            "activation": {
                "key": ["hero", "protagonist"],
                "logic": "and_any",
                "constant_seeds": ["kb_seed_1"]
            },
            "other_module": {"version": 1}
        }));

        // Forward → spoke, reverse → nexus.
        let spoke = world_kb_to_spoke(&entry);
        let roundtripped = spoke_to_world_kb(spoke);

        let modules = roundtripped
            .modules
            .as_ref()
            .expect("modules survive the spoke round-trip");
        assert_eq!(modules["activation"]["key"], json!(["hero", "protagonist"]));
        assert_eq!(modules["activation"]["logic"], "and_any");
        assert_eq!(
            modules["activation"]["constant_seeds"],
            json!(["kb_seed_1"])
        );
        // Unknown module namespace preserved.
        assert_eq!(modules["other_module"]["version"], 1);
    }
}
