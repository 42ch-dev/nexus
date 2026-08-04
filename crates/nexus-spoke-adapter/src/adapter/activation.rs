//! Lore activation engine — scans `WorldKB` entries against assembled moment
//! text and classifies them by `modules.activation` fire-conditions.
//!
//! V1.149 P0 T1 (DF-74): this is the **default-on** engine. The flag-gated
//! V1.146 spike is promoted to a consumer of the spoke `modules.activation`
//! dialect — the handbook field table is `spoke/.mstar/specs/domain-profile-lore-activation.md`
//! §"`modules.activation` — portable subset" (iteration spec `fl-l-w4-activation.md`
//! §2). Nexus parses the portable subset, never invents nexus-private portable
//! fields, and keeps all matching/scan logic product-local — no
//! `spoke-operations` matchers (spoke owns the dialect wire only).
//!
//! MCA calls `apply_activation` between `WorldKB` fetch and User Knowledge
//! assembly; the engine operates on the already-fetched entries — MCA stays
//! generic (spec Architecture Lock Decision 5).
//!
//! # Logic truth table (handbook §2.1 — replaces the V1.146 primary-only spike)
//!
//! When `secondary_keys` is **absent or empty**, only the primary `keys`
//! participate and `logic` is ignored: the entry fires when **any** primary
//! key matches. When `secondary_keys` is present, `logic` combines both sets:
//!
//! | `logic`   | Entry fires when…                                                        |
//! |-----------|--------------------------------------------------------------------------|
//! | `and_any` | any primary **and** any secondary key match (handbook default)           |
//! | `and_all` | **all** primary **and** all secondary keys match                         |
//! | `not_any` | any primary matches **and** no secondary key matches                     |
//! | `not_all` | any primary matches **and** it is false that every secondary matches     |
//! | unknown   | treated as `and_any`; recorded in the trace                              |
//!
//! # Match modes
//!
//! - `literal` (default) — case-insensitive substring.
//! - `regex` — `regress::Regex` (workspace pin); key ≤ 256 chars (longer keys
//!   are skipped with a trace note), scan text capped at 64 KiB, compile
//!   failure → non-match + `"invalid regex"` trace note.
//! - `whole_word` — case-insensitive Unicode-aware word-boundary match.
//!
//! # Neutral entries (the byte-equivalence ship guarantee)
//!
//! Entries with no `modules` map, with `modules` but no `activation`, or with
//! an `activation` module whose `keys` are empty and `constant` is false are
//! **always** in `matched` — identical to V1.146 flag-off behavior. `constant:
//! true` entries are always-on seed candidates (emitted first by the caller,
//! V1.149 P0 T3 ordering).

use nexus_knowledge::world_kb::knowledge_entry::WorldKbEntry;
use serde::{Deserialize, Serialize};

/// Maximum regex key length (architect lock Q6) — chars.
const MAX_REGEX_KEY_CHARS: usize = 256;
/// Scan-text cap for the `regex` match path (architect lock Q6) — chars,
/// consistent with the chars/4 token heuristic used across MCA.
const MAX_REGEX_SCAN_CHARS: usize = 64 * 1024;

/// Result of an activation pass over a set of `WorldKB` entries.
#[derive(Debug, Clone)]
pub struct ActivationResult {
    /// Entries that passed activation (or are `constant` seeds, or have no
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

/// The parsed `modules.activation` data from a single entry (handbook shape).
///
/// Deserialization goes through [`ActivationConfigRaw`], which applies the
/// V1.146 spike aliases (`key` → `keys`, `constant_seeds` self-id → `constant`)
/// and ignores unknown fields (consumer-only dialect — no `deny_unknown_fields`).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "ActivationConfigRaw")]
struct ActivationConfig {
    /// Primary activation triggers (handbook `keys`).
    keys: Vec<String>,
    /// Secondary / selective triggers evaluated with `logic` (handbook).
    secondary_keys: Vec<String>,
    /// How primary + secondary combine; only meaningful when
    /// `secondary_keys` is non-empty (handbook §2.1).
    logic: String,
    /// Always-on **seed** candidate when `true` (handbook).
    constant: bool,
    /// Insertion / scan order hint — lower first (handbook; sorted by caller).
    order: f64,
    /// Tie-break / budget preference — higher wins (handbook; sorted by caller).
    priority: f64,
    /// Preferred placement (`before_defs`/`after_defs`/`depth`/`outlet`).
    /// Parsed + preserved; not actioned until DF-75.
    position_hint: Option<String>,
    /// Named injection outlet id paired with `position_hint: "outlet"`.
    /// Parsed + preserved; not actioned until DF-75.
    outlet: Option<String>,
    /// How key strings match scanned context (handbook `match`; default `literal`).
    #[serde(rename = "match")]
    match_mode: String,
    /// V1.146 spike alias carrier: `constant_seeds` self-id membership derives
    /// `constant` at evaluation time (spec §2.2). Never exported to the wire.
    #[serde(skip_serializing)]
    constant_seeds: Vec<String>,
}

/// Serde intermediate for `modules.activation` — handbook fields plus the
/// one-minor V1.146 spike aliases (`key`, `constant_seeds`). Unknown fields
/// are ignored (no `deny_unknown_fields`), so portable packs round-trip.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
struct ActivationConfigRaw {
    #[serde(default)]
    keys: Option<Vec<String>>,
    /// Spike alias for `keys` — used only when `keys` is absent (spec §2.2).
    #[serde(default)]
    key: Option<Vec<String>>,
    #[serde(default)]
    secondary_keys: Vec<String>,
    #[serde(default = "default_logic")]
    logic: String,
    #[serde(default)]
    constant: bool,
    /// Spike alias carrier — per-entry self-id membership → `constant: true`.
    #[serde(default)]
    constant_seeds: Vec<String>,
    #[serde(default)]
    order: f64,
    #[serde(default)]
    priority: f64,
    #[serde(default)]
    position_hint: Option<String>,
    #[serde(default)]
    outlet: Option<String>,
    #[serde(default = "default_match_mode", rename = "match")]
    match_mode: String,
}

impl From<ActivationConfigRaw> for ActivationConfig {
    fn from(raw: ActivationConfigRaw) -> Self {
        // Spike alias: prefer `keys` when both are present, else fall back to
        // `key` (spec §2.2 "Prefer `keys` when both present").
        let keys = raw.keys.or(raw.key).unwrap_or_default();
        Self {
            keys,
            secondary_keys: raw.secondary_keys,
            logic: raw.logic,
            constant: raw.constant,
            order: raw.order,
            priority: raw.priority,
            position_hint: raw.position_hint,
            outlet: raw.outlet,
            match_mode: raw.match_mode,
            constant_seeds: raw.constant_seeds,
        }
    }
}

fn default_logic() -> String {
    "and_any".to_string()
}

fn default_match_mode() -> String {
    "literal".to_string()
}

/// Why a single key could not be evaluated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MatchKeyError {
    /// Regex key exceeds the length cap (architect lock Q6) — skipped.
    OverlongKey,
    /// Pattern failed to compile — treated as a non-match.
    InvalidRegex,
}

/// Evaluate one activation key against the scan text under `mode`.
///
/// `scan_raw` is the unconverted scan text (regex path); `scan_lower` is the
/// lowercased form used by `literal` / `whole_word` (spec §3 match casing —
/// lowercasing applies only to those modes; regex case-folding is the
/// pattern's business). Unknown modes fall back to `literal` (handbook default).
fn match_key(
    mode: &str,
    key: &str,
    scan_raw: &str,
    scan_lower: &str,
) -> Result<bool, MatchKeyError> {
    match mode.to_ascii_lowercase().as_str() {
        "regex" => {
            if key.chars().count() > MAX_REGEX_KEY_CHARS {
                return Err(MatchKeyError::OverlongKey);
            }
            let re = regress::Regex::new(key).map_err(|_| MatchKeyError::InvalidRegex)?;
            // Truncate with a stable prefix (architect lock Q6).
            let capped = truncate_chars(scan_raw, MAX_REGEX_SCAN_CHARS);
            Ok(re.find(capped).is_some())
        }
        "whole_word" => Ok(whole_word_match(key, scan_lower)),
        _ => Ok(literal_match(key, scan_lower)),
    }
}

fn literal_match(key: &str, scan_lower: &str) -> bool {
    scan_lower.contains(&key.to_lowercase())
}

/// Case-insensitive word-boundary match (Unicode-aware: boundary = not
/// alphanumeric and not `_`).
fn whole_word_match(key: &str, scan_lower: &str) -> bool {
    let key_lower = key.to_lowercase();
    if key_lower.is_empty() {
        return true;
    }
    let mut offset = 0;
    while let Some(rel) = scan_lower[offset..].find(&key_lower) {
        let start = offset + rel;
        let end = start + key_lower.len();
        let before_ok = start == 0 || !is_word_char(scan_lower[..start].chars().next_back());
        let after_ok = end == scan_lower.len() || !is_word_char(scan_lower[end..].chars().next());
        if before_ok && after_ok {
            return true;
        }
        // Advance past this occurrence by one full char (not one byte):
        // `start + 1` sliced mid-char for multi-byte keys and panicked
        // ("byte index is not a char boundary") — e.g. CJK keys. `start` is
        // always a char boundary and `key_lower` is non-empty, so this is
        // always `Some`; the `map_or` default is defensive only.
        offset = start + scan_lower[start..].chars().next().map_or(1, char::len_utf8);
    }
    false
}

fn is_word_char(c: Option<char>) -> bool {
    c.is_some_and(|c| c.is_alphanumeric() || c == '_')
}

/// First `max_chars` characters of `text` (stable prefix).
fn truncate_chars(text: &str, max_chars: usize) -> &str {
    text.char_indices()
        .nth(max_chars)
        .map_or(text, |(idx, _)| &text[..idx])
}

/// Evaluate a key list: returns `(matched keys, skip/error notes)`.
fn eval_keys<'a>(
    keys: &'a [String],
    mode: &str,
    scan_raw: &str,
    scan_lower: &str,
) -> (Vec<&'a str>, Vec<String>) {
    let mut hits = Vec::new();
    let mut notes = Vec::new();
    for key in keys {
        match match_key(mode, key, scan_raw, scan_lower) {
            Ok(true) => hits.push(key.as_str()),
            Ok(false) => {}
            Err(MatchKeyError::OverlongKey) => {
                notes.push(format!(
                    "regex key over {MAX_REGEX_KEY_CHARS} chars skipped"
                ));
            }
            Err(MatchKeyError::InvalidRegex) => notes.push("invalid regex".to_string()),
        }
    }
    (hits, notes)
}

/// Keys in `keys` that are not present in `hits`.
fn missing_keys<'a>(keys: &'a [String], hits: &[&'a str]) -> Vec<&'a str> {
    keys.iter()
        .filter(|k| !hits.contains(&k.as_str()))
        .map(String::as_str)
        .collect()
}

fn miss_reason(arm: &str, mode: &str, scanned: usize, notes: &[String]) -> String {
    if notes.is_empty() {
        format!("{arm} ({mode}): no key matched ({scanned} keys scanned)")
    } else {
        format!(
            "{arm} ({mode}): no key matched ({scanned} keys scanned; {})",
            notes.join("; ")
        )
    }
}

/// Apply lore activation to a set of `WorldKB` entries.
///
/// For each entry:
/// 1. No `modules.activation` → "neutral", included in `matched` (byte-
///    equivalence guarantee — identical to V1.146 flag-off).
/// 2. `constant: true` (or V1.146 `constant_seeds` self-id, or caller-supplied
///    seed id) → always-on seed, included in `matched`.
/// 3. `keys` empty (and not constant) → neutral, included in `matched`.
/// 4. Otherwise evaluate the handbook truth table over `keys` +
///    `secondary_keys` under `match` mode, with per-entry self-match text
///    (`canonical_name` + `body.summary`) appended to the scan (spec §3 #4).
///
/// # Arguments
///
/// * `entries` — Slice of `WorldKB` entries to classify.
/// * `scan_text` — The assembled scan text (Stage-0 + outline beats as wired
///   by MCA; per-entry self-match is appended internally).
/// * `constant_seed_ids` — Caller-supplied entry IDs that are always included
///   (tests/config; merged with per-entry `constant`/`constant_seeds`).
#[must_use]
pub fn apply_activation(
    entries: &[WorldKbEntry],
    scan_text: &str,
    constant_seed_ids: &[String],
) -> ActivationResult {
    let base_lower = scan_text.to_lowercase();
    let mut matched = Vec::new();
    let mut unmatched = Vec::new();
    let mut trace = Vec::new();

    for entry in entries {
        let entry_id = entry.entry_id.clone();
        let canonical_name = entry.canonical_name.clone();

        // Per-entry self-match text (V1.146 behavior, spec §3 source 4):
        // canonical_name + body.summary appended to the external scan.
        let mut entry_raw = canonical_name.clone();
        if let Some(ref body) = entry.body {
            if let Some(ref summary) = body.summary {
                entry_raw.push(' ');
                entry_raw.push_str(summary);
            }
        }
        let scan_raw = format!("{scan_text}\n{entry_raw}");
        let scan_lower = format!("{base_lower}\n{}", entry_raw.to_lowercase());

        let activation = entry
            .modules
            .as_ref()
            .and_then(|m| m.get("activation"))
            .and_then(|v| serde_json::from_value::<ActivationConfig>(v.clone()).ok());

        let (accepted, reason) = activation.as_ref().map_or_else(
            || {
                // No activation module → neutral, always included.
                (true, "no activation module".to_string())
            },
            |cfg| evaluate_entry(cfg, &entry_id, &scan_raw, &scan_lower, constant_seed_ids),
        );

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

/// Evaluate one entry against the handbook logic truth table (spec §2.1).
///
/// Returns `(accepted, reason)`. The exhaustive table keeps this over the
/// pedantic line budget; splitting would obscure the truth-table shape.
#[allow(clippy::too_many_lines)] // exhaustive handbook logic truth table
fn evaluate_entry(
    cfg: &ActivationConfig,
    entry_id: &str,
    scan_raw: &str,
    scan_lower: &str,
    caller_seed_ids: &[String],
) -> (bool, String) {
    // Always-on seeds: handbook `constant: true`, V1.146 spike
    // `constant_seeds` self-id, or caller-supplied seed ids.
    if cfg.constant
        || cfg.constant_seeds.iter().any(|s| s == entry_id)
        || caller_seed_ids.iter().any(|s| s == entry_id)
    {
        let seed_source = if cfg.constant {
            "constant"
        } else if cfg.constant_seeds.iter().any(|s| s == entry_id) {
            "self-declared"
        } else {
            "caller-supplied"
        };
        return (true, format!("constant seed ({seed_source})"));
    }

    // Empty primary keys + no constant → neutral (spec §1 neutral-only:
    // "activation with empty keys and no constant").
    if cfg.keys.is_empty() {
        return (true, "no activation keys (neutral)".to_string());
    }

    let mode = &cfg.match_mode;
    let primary = eval_keys(&cfg.keys, mode, scan_raw, scan_lower);
    let primary_hit = !primary.0.is_empty();

    // Handbook §2.1: secondary absent/empty → primary-only, logic ignored.
    if cfg.secondary_keys.is_empty() {
        return if primary_hit {
            (
                true,
                format!(
                    "primary-any ({mode}): matched key [{}]",
                    primary.0.join(", ")
                ),
            )
        } else {
            (
                false,
                miss_reason("primary-any", mode, cfg.keys.len(), &primary.1),
            )
        };
    }

    let secondary = eval_keys(&cfg.secondary_keys, mode, scan_raw, scan_lower);
    let secondary_hit = !secondary.0.is_empty();

    match cfg.logic.as_str() {
        "and_any" => {
            if primary_hit && secondary_hit {
                (
                    true,
                    format!(
                        "and_any ({mode}): primary [{}] + secondary [{}] matched",
                        primary.0.join(", "),
                        secondary.0.join(", ")
                    ),
                )
            } else {
                (
                    false,
                    format!(
                        "and_any ({mode}): no full match — primary matched: {}, secondary matched: {}",
                        primary.0.len(),
                        secondary.0.len()
                    ),
                )
            }
        }
        "and_all" => {
            let primary_all = primary.0.len() == cfg.keys.len();
            let secondary_all = secondary.0.len() == cfg.secondary_keys.len();
            if primary_all && secondary_all {
                (
                    true,
                    format!(
                        "and_all ({mode}): all {} primary + {} secondary keys matched",
                        cfg.keys.len(),
                        cfg.secondary_keys.len()
                    ),
                )
            } else {
                let missing: Vec<&str> = missing_keys(&cfg.keys, &primary.0)
                    .into_iter()
                    .chain(missing_keys(&cfg.secondary_keys, &secondary.0))
                    .collect();
                (
                    false,
                    format!(
                        "and_all ({mode}): missing keys [{missing}]",
                        missing = missing.join(", ")
                    ),
                )
            }
        }
        "not_any" => {
            if primary_hit && !secondary_hit {
                (
                    true,
                    format!("not_any ({mode}): primary matched, no secondary matched"),
                )
            } else if !primary_hit {
                (
                    false,
                    format!(
                        "not_any ({mode}): no key matched ({} primary keys scanned)",
                        cfg.keys.len()
                    ),
                )
            } else {
                (
                    false,
                    format!(
                        "not_any ({mode}): secondary exclusion matched [{secondary}]",
                        secondary = secondary.0.join(", ")
                    ),
                )
            }
        }
        "not_all" => {
            let secondary_all = secondary.0.len() == cfg.secondary_keys.len();
            if primary_hit && !secondary_all {
                (
                    true,
                    format!("not_all ({mode}): primary matched, not all secondary matched"),
                )
            } else if !primary_hit {
                (
                    false,
                    format!(
                        "not_all ({mode}): no key matched ({} primary keys scanned)",
                        cfg.keys.len()
                    ),
                )
            } else {
                (
                    false,
                    format!("not_all ({mode}): all secondary matched — excluded"),
                )
            }
        }
        other => {
            // Unknown logic → handbook default `and_any`; recorded in trace (spec §2.1).
            if primary_hit && secondary_hit {
                (
                    true,
                    format!(
                        "unknown logic '{other}' — treated as and_any ({mode}): primary [{}] + secondary [{}] matched",
                        primary.0.join(", "),
                        secondary.0.join(", ")
                    ),
                )
            } else {
                (
                    false,
                    format!(
                        "unknown logic '{other}' — treated as and_any ({mode}): no key matched ({} primary + {} secondary keys scanned)",
                        cfg.keys.len(),
                        cfg.secondary_keys.len()
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

    /// Helper: build activation JSON with handbook `keys` (+ optional secondary).
    fn activation_json(keys: &[&str], secondary_keys: &[&str], logic: &str) -> serde_json::Value {
        json!({
            "activation": {
                "keys": keys,
                "secondary_keys": secondary_keys,
                "logic": logic,
            }
        })
    }

    /// Shortcut for the common no-secondary case.
    fn activation_primary(keys: &[&str], logic: &str) -> serde_json::Value {
        activation_json(keys, &[], logic)
    }

    fn run(entries: &[WorldKbEntry], scan: &str) -> ActivationResult {
        apply_activation(entries, scan, &[])
    }

    // ── primary-only (secondary absent) — logic IGNORED ──────────────

    #[test]
    fn test_secondary_absent_and_any_fires_on_any_primary() {
        let entries = vec![entry_with_activation(
            "kb_1",
            "Old King",
            "The elderly ruler of the northern kingdom",
            Some(activation_primary(&["king", "dragon"], "and_any")),
        )];
        let result = run(
            &entries,
            "The story takes place in a northern kingdom ruled by an old king.",
        );
        assert_eq!(result.matched.len(), 1);
        assert!(result.trace[0].reason.contains("matched key [king]"));
    }

    #[test]
    fn test_secondary_absent_ignores_logic_and_all_any_hit_fires() {
        // CRITICAL truth-table check: with no secondary_keys, `logic: and_all`
        // must NOT require all primary keys — any primary hit fires.
        let entries = vec![entry_with_activation(
            "kb_2",
            "Royal Guard",
            "Protector of the throne",
            Some(activation_primary(&["king", "dragon"], "and_all")),
        )];
        let result = run(&entries, "The king sat on the throne.");
        assert_eq!(
            result.matched.len(),
            1,
            "and_all with no secondary = primary-any"
        );
        assert!(result.trace[0].reason.contains("primary-any (literal)"));
    }

    #[test]
    fn test_secondary_absent_ignores_logic_not_any_any_hit_fires() {
        // `not_any` with no secondary is NOT an exclusion list anymore —
        // primary-any fires (handbook: logic ignored).
        let entries = vec![entry_with_activation(
            "kb_3",
            "Orc Warlord",
            "A brutal commander",
            Some(activation_primary(&["orc", "warlord"], "not_any")),
        )];
        let result = run(&entries, "The orc army marched forward.");
        assert_eq!(
            result.matched.len(),
            1,
            "not_any with no secondary = primary-any"
        );
        assert!(result.trace[0].reason.contains("primary-any (literal)"));
    }

    #[test]
    fn test_secondary_absent_no_primary_hit_misses() {
        let entries = vec![entry_with_activation(
            "kb_4",
            "Dragon",
            "A fearsome dragon",
            Some(activation_primary(&["elf", "wizard"], "and_any")),
        )];
        let result = run(&entries, "The story takes place in a northern kingdom.");
        assert_eq!(result.matched.len(), 0);
        assert_eq!(result.unmatched.len(), 1);
        assert!(result.trace[0].reason.contains("no key matched"));
        assert!(result.trace[0].reason.contains("primary-any"));
    }

    // ── and_any + secondary ─────────────────────────────────────────

    #[test]
    fn test_and_any_with_secondary_requires_both() {
        let entries = vec![entry_with_activation(
            "kb_5",
            "Harbor",
            "The dawn dock district",
            Some(activation_json(&["harbor"], &["chapter 1"], "and_any")),
        )];
        let result = run(&entries, "The harbor gates open.");
        assert_eq!(
            result.matched.len(),
            0,
            "secondary 'chapter 1' missing → no fire"
        );
        assert!(result.trace[0].reason.contains("and_any"));
        assert!(result.trace[0].reason.contains("no full match"));

        let result2 = run(&entries, "The harbor gates open in chapter 1.");
        assert_eq!(
            result2.matched.len(),
            1,
            "primary + secondary both present → fire"
        );
        assert!(result2.trace[0]
            .reason
            .contains("primary [harbor] + secondary [chapter 1]"));
    }

    // ── and_all + secondary ──────────────────────────────────────────

    #[test]
    fn test_and_all_with_secondary_requires_all_primary_and_all_secondary() {
        // Secondary keys chosen to avoid the entry's own self-match text
        // ("Royal Guard" / "Protector of the throne").
        let entries = vec![entry_with_activation(
            "kb_6",
            "Royal Guard",
            "Protector of the throne",
            Some(activation_json(
                &["king", "throne"],
                &["horn", "bell"],
                "and_all",
            )),
        )];
        let result = run(&entries, "The king sat on the throne.");
        assert_eq!(result.matched.len(), 0, "secondary missing → no fire");
        assert!(result.trace[0].reason.contains("missing keys [horn, bell]"));

        let result2 = run(
            &entries,
            "The king sat on the throne while a horn and a bell rang.",
        );
        assert_eq!(
            result2.matched.len(),
            1,
            "all primary + all secondary → fire"
        );
        assert!(result2.trace[0]
            .reason
            .contains("all 2 primary + 2 secondary keys matched"));
    }

    #[test]
    fn test_and_all_with_secondary_missing_primary_fails() {
        let entries = vec![entry_with_activation(
            "kb_7",
            "Royal Guard",
            "Protector of the throne",
            Some(activation_json(&["king", "dragon"], &["guard"], "and_all")),
        )];
        let result = run(&entries, "The king and the guard stood watch.");
        assert_eq!(
            result.matched.len(),
            0,
            "primary 'dragon' missing → no fire"
        );
        assert!(result.trace[0].reason.contains("missing keys [dragon]"));
    }

    // ── not_any + secondary ──────────────────────────────────────────

    #[test]
    fn test_not_any_with_secondary_excludes_when_secondary_hits() {
        let entries = vec![entry_with_activation(
            "kb_8",
            "Orc Warlord",
            "A brutal commander",
            Some(activation_json(&["orc"], &["ally"], "not_any")),
        )];
        let result = run(&entries, "The orc army marched forward.");
        assert_eq!(result.matched.len(), 1, "primary hit, no secondary → fire");

        let result2 = run(&entries, "The orc and his ally marched forward.");
        assert_eq!(
            result2.matched.len(),
            0,
            "secondary 'ally' matches → excluded"
        );
        assert!(result2.trace[0]
            .reason
            .contains("secondary exclusion matched [ally]"));
    }

    #[test]
    fn test_not_any_with_secondary_no_primary_hit_fails() {
        let entries = vec![entry_with_activation(
            "kb_9",
            "Peaceful Elf",
            "A gentle forest dweller",
            Some(activation_json(&["orc"], &["ally"], "not_any")),
        )];
        let result = run(&entries, "The elves sang in the forest.");
        assert_eq!(result.matched.len(), 0, "primary missing → no fire");
        assert!(result.trace[0].reason.contains("no key matched"));
    }

    // ── not_all + secondary ──────────────────────────────────────────

    #[test]
    fn test_not_all_with_secondary_fires_when_not_every_secondary_matches() {
        let entries = vec![entry_with_activation(
            "kb_10",
            "Dark Wizard",
            "A powerful spellcaster",
            Some(activation_json(
                &["dark"],
                &["wizard", "necromancer"],
                "not_all",
            )),
        )];
        let result = run(&entries, "The dark wizard cast a spell.");
        assert_eq!(
            result.matched.len(),
            1,
            "'wizard' hits, 'necromancer' misses → fire"
        );

        let result2 = run(&entries, "The dark wizard necromancer cast a spell.");
        assert_eq!(
            result2.matched.len(),
            0,
            "every secondary matches → excluded"
        );
        assert!(result2.trace[0].reason.contains("all secondary matched"));
    }

    // ── match modes ──────────────────────────────────────────────────

    #[test]
    fn test_literal_is_case_insensitive_substring() {
        let entries = vec![entry_with_activation(
            "kb_11",
            "King Arthur",
            "Ruler of Camelot",
            Some(json!({"activation": {"keys": ["KING"]}})),
        )];
        let result = run(&entries, "The king ruled wisely.");
        assert_eq!(result.matched.len(), 1);
        assert!(result.trace[0].accepted);
    }

    #[test]
    fn test_whole_word_requires_boundary() {
        // Entry self-text must NOT contain the key, so matches come from the
        // scan text alone (self-match would defeat the boundary check).
        let entries = vec![entry_with_activation(
            "kb_12",
            "The Monarch",
            "Ruler of the realm",
            Some(json!({"activation": {"keys": ["king"], "match": "whole_word"}})),
        )];
        // "king" inside "kings" must NOT match; standalone "king" must.
        let result = run(&entries, "Two kings ruled.");
        assert_eq!(
            result.matched.len(),
            0,
            "'king' inside 'kings' is not a whole word"
        );

        let result2 = run(&entries, "A king ruled.");
        assert_eq!(result2.matched.len(), 1);
    }

    #[test]
    fn test_whole_word_cjk_char_boundary_safe_advance() {
        // Regression (QC F1): the advance after a failed boundary check used
        // `start + 1` bytes, slicing mid-char for multi-byte keys — this
        // panicked with "byte index 1 is not a char boundary" for
        // `whole_word_match("王", "王宫")`. "王" inside the CJK word "王宫" is
        // not a whole word → must not match, and must not panic.
        assert!(!whole_word_match("王", "王宫"));

        // A CJK key at a true boundary still matches; the scan must continue
        // past the failed boundary and find the later standalone occurrence.
        assert!(whole_word_match("王", "王宫。王"));

        // A CJK key inside a longer CJK word must not match mid-word.
        assert!(!whole_word_match("宫", "王宫"));
        assert!(!whole_word_match("王宫", "大秦王宫"));
    }

    #[test]
    fn test_regex_matches_pattern() {
        let entries = vec![entry_with_activation(
            "kb_13",
            "Draconic Beasts",
            "Fire breathers",
            Some(json!({"activation": {"keys": ["drag[ou]n"], "match": "regex"}})),
        )];
        let result = run(&entries, "A dragon and a dragoon met.");
        assert_eq!(result.matched.len(), 1);
        assert!(result.trace[0].accepted);
    }

    #[test]
    fn test_regex_invalid_pattern_is_non_match_with_trace() {
        let entries = vec![entry_with_activation(
            "kb_14",
            "Broken Pattern",
            "Unparseable regex",
            Some(json!({"activation": {"keys": ["(unclosed"], "match": "regex"}})),
        )];
        let result = run(&entries, "Any text.");
        assert_eq!(result.matched.len(), 0);
        assert!(result.trace[0].reason.contains("invalid regex"));
    }

    #[test]
    fn test_regex_overlong_key_skipped_with_trace() {
        let long_key = "x".repeat(MAX_REGEX_KEY_CHARS + 1);
        let entries = vec![entry_with_activation(
            "kb_15",
            "Overlong Key",
            "Key too long to compile",
            Some(json!({"activation": {"keys": [long_key], "match": "regex"}})),
        )];
        let result = run(&entries, "Any text.");
        assert_eq!(result.matched.len(), 0);
        assert!(result.trace[0]
            .reason
            .contains("key over 256 chars skipped"));
    }

    #[test]
    fn test_regex_scan_text_truncated_to_64k() {
        // A key beyond the 64 KiB cap must still match when present near the
        // head (stable prefix) — and must not panic on huge scan text.
        let scan = format!("harbor {}", "y".repeat(70 * 1024));
        let entries = vec![entry_with_activation(
            "kb_16",
            "Harbor",
            "Dawn dock",
            Some(json!({"activation": {"keys": ["harbor"], "match": "regex"}})),
        )];
        let result = run(&entries, &scan);
        assert_eq!(result.matched.len(), 1, "key at stable prefix head matches");
    }

    #[test]
    fn test_unknown_match_mode_falls_back_to_literal() {
        let entries = vec![entry_with_activation(
            "kb_17",
            "Fallback",
            "Unknown mode",
            Some(json!({"activation": {"keys": ["KING"], "match": "fuzzy"}})),
        )];
        let result = run(&entries, "The king ruled.");
        assert_eq!(
            result.matched.len(),
            1,
            "unknown mode → literal (CI substring)"
        );
    }

    // ── constant seeds ───────────────────────────────────────────────

    #[test]
    fn test_constant_true_is_always_matched() {
        let entries = vec![entry_with_activation(
            "kb_c1",
            "World Rule",
            "Always-on seed",
            Some(json!({"activation": {"keys": [], "constant": true, "priority": 100}})),
        )];
        let result = run(&entries, "Nothing matches.");
        assert_eq!(result.matched.len(), 1);
        assert!(result.trace[0].reason.contains("constant seed (constant)"));
    }

    #[test]
    fn test_caller_supplied_seed_always_matches() {
        let entries = vec![entry_with_activation(
            "kb_cs_1",
            "Seed Entry",
            "This should always be included",
            Some(activation_primary(&["nonexistent"], "and_all")),
        )];
        let result = apply_activation(
            &entries,
            "This story has nothing matching.",
            &["kb_cs_1".to_string()],
        );
        assert_eq!(result.matched.len(), 1);
        assert!(result.trace[0]
            .reason
            .contains("constant seed (caller-supplied)"));
    }

    #[test]
    fn test_self_declared_constant_seeds_spike_alias() {
        // V1.146 spike alias: constant_seeds self-id → constant.
        let mut activation = activation_primary(&["nonexistent"], "and_all");
        activation["activation"]["constant_seeds"] = json!(["kb_self_1"]);
        let entries = vec![entry_with_activation(
            "kb_self_1",
            "Self Seed",
            "Always included",
            Some(activation),
        )];
        let result = run(&entries, "Nothing matches.");
        assert_eq!(result.matched.len(), 1);
        assert!(result.trace[0]
            .reason
            .contains("constant seed (self-declared)"));
    }

    // ── spike aliases: `key` ─────────────────────────────────────────

    #[test]
    fn test_spike_key_alias_when_keys_absent() {
        let entries = vec![entry_with_activation(
            "kb_k1",
            "Aliased",
            "Spike key field",
            Some(json!({"activation": {"key": ["king"], "logic": "and_any"}})),
        )];
        let result = run(&entries, "The king ruled.");
        assert_eq!(result.matched.len(), 1, "spike `key` accepted as `keys`");
    }

    #[test]
    fn test_spike_keys_preferred_when_both_present() {
        // Both `keys` and `key`: `keys` wins (spec §2.2).
        let entries = vec![entry_with_activation(
            "kb_k2",
            "Preferred",
            "Keys wins over key",
            Some(json!({"activation": {"keys": ["wizard"], "key": ["king"], "logic": "and_any"}})),
        )];
        let result = run(&entries, "The king ruled.");
        assert_eq!(
            result.matched.len(),
            0,
            "`keys` wins — 'king' from spike `key` must not fire"
        );
        let result2 = run(&entries, "A wizard appeared.");
        assert_eq!(result2.matched.len(), 1);
    }

    // ── neutral entries (byte-equivalence guarantee) ─────────────────

    #[test]
    fn test_neutral_entry_no_modules() {
        let entries = vec![entry_with_activation(
            "kb_n1",
            "Neutral",
            "No modules",
            None,
        )];
        let result = run(&entries, "Any text.");
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
        let result = run(&entries, "Any text.");
        assert_eq!(result.matched.len(), 1);
        assert!(result.trace[0].reason.contains("no activation module"));
    }

    #[test]
    fn test_empty_activation_keys_neutral() {
        let entries = vec![entry_with_activation(
            "kb_ek",
            "Empty Keys",
            "No keys defined",
            Some(activation_primary(&[], "and_any")),
        )];
        let result = run(&entries, "Any text.");
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
        let result = run(&entries, "Text.");
        // modules is Some(null) → get("activation") on null returns None → neutral.
        assert_eq!(result.matched.len(), 1);
        assert!(result.trace[0].reason.contains("no activation module"));
    }

    #[test]
    fn test_unknown_logic_treated_as_and_any_with_secondary() {
        // Unknown logic + secondary present → and_any fallback (spec §2.1).
        // Secondary key "chapter" must not appear in the entry self-text.
        let entries = vec![entry_with_activation(
            "kb_u1",
            "Matched",
            "Contains king",
            Some(activation_json(&["king"], &["chapter"], "fuzzy")),
        )];
        let result = run(&entries, "The king ruled in chapter 5.");
        assert_eq!(result.matched.len(), 1);
        assert!(result.trace[0].reason.contains("treated as and_any"));

        let result2 = run(&entries, "The king ruled alone.");
        assert_eq!(result2.matched.len(), 0);
        assert!(result2.trace[0].reason.contains("treated as and_any"));
    }

    // ── mixed classification ─────────────────────────────────────────

    #[test]
    fn test_mixed_matched_unmatched_and_neutral() {
        let entries = vec![
            entry_with_activation(
                "kb_m1",
                "Match",
                "Contains king",
                Some(activation_primary(&["king"], "and_any")),
            ),
            entry_with_activation(
                "kb_m2",
                "NoMatch",
                "Nothing",
                Some(activation_primary(&["dragon"], "and_any")),
            ),
            entry_with_activation("kb_m3", "Neutral", "No activation", None),
        ];
        let result = run(&entries, "The king ruled.");
        assert_eq!(result.matched.len(), 2); // kb_m1 (matched) + kb_m3 (neutral)
        assert_eq!(result.unmatched.len(), 1); // kb_m2
    }

    // ── unknown-field round-trip + module preservation ───────────────

    #[test]
    fn test_activation_unknown_fields_ignored() {
        // Consumer-only dialect: nexus-private unknown fields inside
        // `activation` are ignored, not rejected.
        let entries = vec![entry_with_activation(
            "kb_uf",
            "Unknown Fields",
            "Portable subset with extras",
            Some(json!({
                "activation": {
                    "keys": ["king"],
                    "nexus_private_flag": "must-not-break-parse",
                    "outlet": "lore-main",
                }
            })),
        )];
        let result = run(&entries, "The king ruled.");
        assert_eq!(result.matched.len(), 1);
    }

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
                "keys": ["hero", "protagonist"],
                "secondary_keys": ["chapter 1"],
                "logic": "and_any",
                "constant": true,
                "order": 10,
                "priority": 5,
                "position_hint": "before_defs",
                "match": "whole_word"
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
        let activation = &modules["activation"];
        assert_eq!(activation["keys"], json!(["hero", "protagonist"]));
        assert_eq!(activation["secondary_keys"], json!(["chapter 1"]));
        assert_eq!(activation["logic"], "and_any");
        assert_eq!(activation["constant"], true);
        assert_eq!(activation["order"], 10);
        assert_eq!(activation["priority"], 5);
        assert_eq!(activation["position_hint"], "before_defs");
        assert_eq!(activation["match"], "whole_word");
        // Unknown module namespace preserved.
        assert_eq!(modules["other_module"]["version"], 1);
    }
}
