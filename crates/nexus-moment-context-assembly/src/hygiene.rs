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
//! pattern is skipped with a note; an oversized input or output is
//! truncated with a note. Unlike the Q6 activation engine's scan cap —
//! which truncates only the `is_match` haystack and never alters emitted
//! content — these caps truncate the emitted summary itself, so every
//! truncation is observable via a per-entry trace note (the emitted text
//! never changes silently). Caps mirror the Q6 activation engine
//! (`adapter/activation.rs`): 256 pattern chars / 64 KiB input / 64 KiB
//! output — guardrails, not features.
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
/// `body.attributes.hygiene`. Pattern and replacement borrow the JSON
/// carrier — they are never cloned into an owned buffer before the
/// output budget runs (R2-6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HygieneTransform<'a> {
    pattern: &'a str,
    replacement: &'a str,
    description: Option<&'a str>,
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
        if body
            .attributes
            .as_ref()
            .and_then(|a| a.get("hygiene"))
            .is_none()
        {
            out.push(entry);
            continue;
        }
        let row = {
            // Take the summary first so the attributes JSON can be borrowed
            // for the rest of the pass — `parse_carrier` must not clone
            // replacement templates out of the carrier (R2-6).
            let summary = body.summary.take();
            let Some(carrier) = body.attributes.as_ref().and_then(|a| a.get("hygiene")) else {
                body.summary = summary;
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
            if let Some(summary) = summary {
                let mut text = summary;
                // Input cap (Q6 mirror): the scan text is truncated before
                // the first pass. Unlike the activation engine's match-only
                // scan cap, this cap alters the emitted text, so the
                // truncation is recorded as a per-entry trace note
                // (mirroring the output-cap note below) — the emitted text
                // never changes silently.
                if text.chars().count() > MAX_HYGIENE_INPUT_CHARS {
                    row.notes.push(format!(
                        "hygiene input over {MAX_HYGIENE_INPUT_CHARS} chars truncated"
                    ));
                    text = truncate_chars(&text, MAX_HYGIENE_INPUT_CHARS).to_string();
                }
                for t in &transforms {
                    let pass = apply_transform(&text, t);
                    row.applied += pass.applied;
                    row.skipped += pass.skipped;
                    if let Some(note) = pass.note {
                        row.notes.push(note);
                    }
                    text = pass.text;
                }
                body.summary = Some(text);
            } else {
                // Carrier present but no summary text — every transform is a
                // no-match (nothing to scan).
                row.skipped += transforms.len();
            }
            row
        };
        trace.push(row);
        out.push(entry);
    }
    (out, trace)
}

/// Result of one authored transform pass over the current emitted text.
struct TransformPass {
    text: String,
    applied: usize,
    skipped: usize,
    note: Option<String>,
}

fn output_truncated_note() -> String {
    format!("hygiene output over {MAX_HYGIENE_OUTPUT_CHARS} chars truncated")
}

fn work_bound_note() -> String {
    format!("hygiene replacement work over {MAX_HYGIENE_OUTPUT_CHARS} pieces truncated")
}

/// Apply one transform with a running output-char budget. Unmatched spans
/// and replacements share [`append_capped`] so `out_chars` never exceeds
/// [`MAX_HYGIENE_OUTPUT_CHARS`] (overflow-checking panic / release wrap).
///
/// # Panics
///
/// Never panics on well-formed input. The only `expect` is on regex
/// capture group 0, which the `regex` crate guarantees to exist for every
/// match of a compiled pattern.
fn apply_transform(text: &str, t: &HygieneTransform<'_>) -> TransformPass {
    if t.pattern.chars().count() > MAX_HYGIENE_PATTERN_CHARS {
        return TransformPass {
            text: text.to_string(),
            applied: 0,
            skipped: 1,
            note: Some(format!(
                "hygiene pattern over {MAX_HYGIENE_PATTERN_CHARS} chars skipped"
            )),
        };
    }
    let Ok(re) = Regex::new(t.pattern) else {
        return TransformPass {
            text: text.to_string(),
            applied: 0,
            skipped: 1,
            note: Some("invalid regex".to_string()),
        };
    };
    // F-2 / R2-1 / R2-5 / R2-7: expand the replacement template one piece
    // at a time with a char budget and a shared piece-visit budget. Absent
    // capture refs expand to empty and must still consume the visit budget
    // or a `$9`.repeat(N) template over many matches is unbounded CPU.
    let mut applied = 0;
    let mut out = String::new();
    let mut out_chars = 0usize;
    let mut last_end = 0usize;
    let mut truncated = false;
    let mut work_limited = false;
    let mut piece_budget = MAX_HYGIENE_OUTPUT_CHARS;
    for caps in re.captures_iter(text) {
        let m = caps.get(0).expect("capture 0 always present");
        if !append_capped(&mut out, &mut out_chars, &text[last_end..m.start()]) {
            truncated = true;
            break;
        }
        match expand_replacement(
            t.replacement,
            &caps,
            &mut out,
            &mut out_chars,
            &mut piece_budget,
        ) {
            ExpansionOutcome::Applied => {
                applied += 1;
                last_end = m.end();
            }
            ExpansionOutcome::Partial => {
                applied += 1;
                truncated = true;
                break;
            }
            ExpansionOutcome::None => {
                truncated = true;
                break;
            }
            ExpansionOutcome::WorkLimit => {
                work_limited = true;
                break;
            }
        }
    }
    if applied == 0 && !truncated && !work_limited {
        return TransformPass {
            text: text.to_string(),
            applied: 0,
            skipped: 1,
            note: None,
        };
    }
    if work_limited && applied == 0 && out.is_empty() {
        return TransformPass {
            text: text.to_string(),
            applied: 0,
            skipped: 1,
            note: Some(work_bound_note()),
        };
    }
    if !truncated && !append_capped(&mut out, &mut out_chars, &text[last_end..]) {
        truncated = true;
    }
    let skipped = usize::from(applied == 0);
    let note = if work_limited {
        Some(work_bound_note())
    } else {
        truncated.then(output_truncated_note)
    };
    TransformPass {
        text: out,
        applied,
        skipped,
        note,
    }
}

/// Parse the `body.attributes.hygiene` carrier — a JSON array of
/// `{"pattern": string, "replacement": string, "description"?: string}`
/// objects. Malformed elements are skipped and reported in `notes`; the
/// returned count is the number of malformed elements (degraded transforms).
fn parse_carrier(carrier: &serde_json::Value) -> (Vec<HygieneTransform<'_>>, Vec<String>, usize) {
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
        let description = obj.get("description").and_then(serde_json::Value::as_str);
        transforms.push(HygieneTransform {
            pattern,
            replacement,
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

/// One piece of a parsed replacement template: literal text or a capture
/// group reference. Group references that do not resolve to a live capture
/// expand to the empty string (mirrors `regex`'s expand).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReplacementPiece<'a> {
    Literal(&'a str),
    /// `$N` — capture group by index.
    Index(usize),
    /// `$name` / `${name}` — capture group by name.
    Name(&'a str),
}

/// Streaming cursor over a replacement template. Yields one
/// [`ReplacementPiece`] at a time so a long `$0$0…` template never
/// materializes a piece vector before the output budget is applied.
///
/// Syntax mirrors the `regex` crate's `expand`: `$$` is a literal `$`,
/// `$name` / `${name}` reference capture groups by name (`$N` / `${N}` by
/// index; an all-digit unbraced run is an index), unbraced names are the
/// longest `[0-9A-Za-z_]` run, braced names may contain any byte except
/// `}`, and a `$` that starts no valid reference is literal.
struct ReplacementCursor<'a> {
    rest: &'a str,
}

impl<'a> ReplacementCursor<'a> {
    const fn new(replacement: &'a str) -> Self {
        Self { rest: replacement }
    }
}

impl<'a> Iterator for ReplacementCursor<'a> {
    type Item = ReplacementPiece<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.rest.is_empty() {
            return None;
        }
        match self.rest.find('$') {
            None => {
                let lit = self.rest;
                self.rest = "";
                Some(ReplacementPiece::Literal(lit))
            }
            Some(0) => {
                let after = &self.rest[1..];
                if after.as_bytes().first() == Some(&b'$') {
                    self.rest = &after[1..];
                    return Some(ReplacementPiece::Literal("$"));
                }
                if after.as_bytes().first() == Some(&b'{') {
                    if let Some(end) = after.find('}') {
                        self.rest = &after[end + 1..];
                        return Some(replacement_piece(&after[1..end]));
                    }
                    self.rest = after;
                    return Some(ReplacementPiece::Literal("$"));
                }
                let run = after
                    .bytes()
                    .take_while(|&b| b.is_ascii_alphanumeric() || b == b'_')
                    .count();
                if run == 0 {
                    self.rest = after;
                    return Some(ReplacementPiece::Literal("$"));
                }
                self.rest = &after[run..];
                Some(replacement_piece(&after[..run]))
            }
            Some(i) => {
                let lit = &self.rest[..i];
                self.rest = &self.rest[i..];
                Some(ReplacementPiece::Literal(lit))
            }
        }
    }
}

/// The piece for a template reference name: an all-digit name is a group
/// index.
fn replacement_piece(name: &str) -> ReplacementPiece<'_> {
    name.parse::<usize>()
        .map_or(ReplacementPiece::Name(name), ReplacementPiece::Index)
}

/// How a match's budget-checked replacement expansion landed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExpansionOutcome {
    /// The whole expansion fit — the match is applied and consumes it.
    Applied,
    /// The output budget ran out mid-expansion; a non-empty prefix of the
    /// expansion was appended (the match is still counted applied).
    Partial,
    /// No output-char budget remained for this expansion — nothing was appended.
    None,
    /// The shared piece-visit budget for the transform was exhausted
    /// (absent-group refs still count; they must not scan unbounded).
    WorkLimit,
}

/// Append the budget-checked replacement expansion of `caps` from the
/// authored template, streaming one [`ReplacementPiece`] at a time (R2-5).
/// Parsing stops when the output budget is exhausted, so a template with
/// many backreferences never allocates a piece vector (or scans the unused
/// tail) before the cap runs.
fn expand_replacement(
    template: &str,
    caps: &regex::Captures<'_>,
    out: &mut String,
    out_chars: &mut usize,
    piece_budget: &mut usize,
) -> ExpansionOutcome {
    let start = *out_chars;
    for piece in ReplacementCursor::new(template) {
        if *piece_budget == 0 {
            return if *out_chars > start {
                ExpansionOutcome::Partial
            } else {
                ExpansionOutcome::WorkLimit
            };
        }
        *piece_budget -= 1;
        let seg = match piece {
            ReplacementPiece::Literal(s) => s,
            ReplacementPiece::Index(idx) => caps.get(idx).map_or("", |gm| gm.as_str()),
            ReplacementPiece::Name(name) => caps.name(name).map_or("", |gm| gm.as_str()),
        };
        if seg.is_empty() {
            continue;
        }
        if !append_capped(out, out_chars, seg) {
            return if *out_chars > start {
                ExpansionOutcome::Partial
            } else {
                ExpansionOutcome::None
            };
        }
    }
    ExpansionOutcome::Applied
}

/// Append `seg` while the output char budget holds. Returns `false` when
/// the budget was exhausted (a prefix of `seg` is kept). `out_chars` never
/// exceeds [`MAX_HYGIENE_OUTPUT_CHARS`].
fn append_capped(out: &mut String, out_chars: &mut usize, seg: &str) -> bool {
    if *out_chars >= MAX_HYGIENE_OUTPUT_CHARS {
        return false;
    }
    let remaining = MAX_HYGIENE_OUTPUT_CHARS - *out_chars;
    let seg_chars = seg.chars().count();
    if seg_chars > remaining {
        out.push_str(truncate_chars(seg, remaining));
        *out_chars = MAX_HYGIENE_OUTPUT_CHARS;
        return false;
    }
    out.push_str(seg);
    *out_chars += seg_chars;
    true
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
    fn many_matches_large_replacement_bounded_by_output_cap() {
        // F-2: the old `replace_all` materialized the complete uncapped
        // expansion before the output-cap check — a pattern matching many
        // positions with a large replacement could exhaust memory. The
        // expansion must be built incrementally with a char budget: the
        // pass stops at the cap, records the truncation note, and keeps
        // what was built (bounded, no OOM).
        let input = "a".repeat(MAX_HYGIENE_INPUT_CHARS);
        let big_replacement = "b".repeat(MAX_HYGIENE_OUTPUT_CHARS);
        let (out, trace) = apply_hygiene(vec![entry_with_hygiene(
            "kb_1",
            &input,
            &serde_json::json!([{ "pattern": "a", "replacement": big_replacement }]),
        )]);
        let summary = out[0].body.as_ref().unwrap().summary.as_deref().unwrap();
        // The first match's expansion already fills the whole budget; the
        // pass stops there instead of expanding every match.
        assert_eq!(summary.chars().count(), MAX_HYGIENE_OUTPUT_CHARS);
        assert_eq!(trace[0].applied, 1);
        assert_eq!(trace[0].skipped, 0);
        assert_eq!(trace[0].notes.len(), 1);
        assert!(trace[0].notes[0].contains("truncated"));
    }

    #[test]
    fn many_small_matches_stop_at_output_cap() {
        // F-2: with a small replacement over a long input, the incremental
        // builder applies matches until the running total reaches the cap,
        // then stops — the emitted text is capped and the truncation is
        // recorded (never silently changed).
        let input = "a".repeat(MAX_HYGIENE_INPUT_CHARS);
        let (out, trace) = apply_hygiene(vec![entry_with_hygiene(
            "kb_1",
            &input,
            &serde_json::json!([{ "pattern": "a", "replacement": "bb" }]),
        )]);
        let summary = out[0].body.as_ref().unwrap().summary.as_deref().unwrap();
        assert_eq!(summary.chars().count(), MAX_HYGIENE_OUTPUT_CHARS);
        // Each "a" → "bb" adds one char; the cap is hit after half the
        // matches, and the remaining matches are not expanded.
        assert_eq!(trace[0].applied, MAX_HYGIENE_OUTPUT_CHARS / 2);
        assert_eq!(trace[0].skipped, 0);
        assert_eq!(trace[0].notes.len(), 1);
        assert!(trace[0].notes[0].contains("truncated"));
    }

    #[test]
    fn oversized_input_truncated_with_note() {
        // W-1: an input over the cap is truncated before the first pass AND
        // a per-entry trace note records the degradation — the emitted text
        // must never change silently (mirrors the output-cap note).
        let mut big_input = "x".repeat(MAX_HYGIENE_INPUT_CHARS);
        big_input.push_str(" dragon"); // tail beyond the cap
        let (out, trace) = apply_hygiene(vec![entry_with_hygiene(
            "kb_1",
            &big_input,
            &serde_json::json!([{ "pattern": "dragon", "replacement": "wyrm" }]),
        )]);
        let summary = out[0].body.as_ref().unwrap().summary.as_deref().unwrap();
        // The tail is beyond the cap, so the transform no-matches on the
        // truncated text and the summary is capped.
        assert_eq!(summary.chars().count(), MAX_HYGIENE_INPUT_CHARS);
        assert_eq!(trace[0].applied, 0);
        assert_eq!(trace[0].skipped, 1); // no-match on the capped text
        assert_eq!(trace[0].notes.len(), 1);
        assert!(trace[0].notes[0].contains("input over"));
        assert!(trace[0].notes[0].contains("truncated"));
    }

    #[test]
    fn oversized_input_transform_matching_prefix_still_applies() {
        // W-1: transforms still run on the capped prefix; the input-cap note
        // is recorded alongside the applied replacements.
        let mut big_input = "dragon".to_string();
        big_input.push_str(&"x".repeat(MAX_HYGIENE_INPUT_CHARS));
        let (out, trace) = apply_hygiene(vec![entry_with_hygiene(
            "kb_1",
            &big_input,
            &serde_json::json!([{ "pattern": "dragon", "replacement": "wyrm" }]),
        )]);
        let summary = out[0].body.as_ref().unwrap().summary.as_deref().unwrap();
        assert!(summary.starts_with("wyrm"));
        assert_eq!(trace[0].applied, 1);
        assert_eq!(trace[0].skipped, 0);
        assert_eq!(trace[0].notes.len(), 1);
        assert!(trace[0].notes[0].contains("input over"));
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
        let carrier = serde_json::json!([
            { "pattern": "a", "replacement": "b", "description": "fix a" },
            { "pattern": "c", "replacement": "d" },
        ]);
        let (transforms, notes, malformed) = parse_carrier(&carrier);
        assert!(notes.is_empty());
        assert_eq!(malformed, 0);
        assert_eq!(transforms.len(), 2);
        assert_eq!(transforms[0].description, Some("fix a"));
        assert_eq!(transforms[1].description, None);
    }

    // ── R2-1: budget-checked segment expansion (no per-match
    //    materialization) ──

    #[test]
    fn single_match_pathological_backref_expansion_bounded() {
        // R2-1: the round-1 pass still called `caps.expand`, which
        // materializes the match's COMPLETE expansion before the budget
        // check — a replacement with backrefs into a large capture
        // multiplies the capture by the ref count, unbounded. The
        // expansion must be applied segment-by-segment with a
        // remaining-budget check before each append: a single capture
        // spanning most of a >64KiC input with a doubled `$0` replacement
        // completes bounded at the output cap and emits the truncation
        // note (never an OOM, never an over-cap emit).
        let input = "a".repeat(MAX_HYGIENE_INPUT_CHARS + 1024);
        let (out, trace) = apply_hygiene(vec![entry_with_hygiene(
            "kb_1",
            &input,
            &serde_json::json!([{ "pattern": "^(a+)$", "replacement": "$0$0" }]),
        )]);
        let summary = out[0].body.as_ref().unwrap().summary.as_deref().unwrap();
        // The doubled `$0` expansion would be ~2 × 64KiC; the pass stops
        // at the cap with the full first copy (the second copy would
        // breach the budget).
        assert_eq!(summary.chars().count(), MAX_HYGIENE_OUTPUT_CHARS);
        assert_eq!(trace[0].applied, 1);
        assert!(
            trace[0].notes.iter().any(|n| n.contains("truncated")),
            "truncation must be noted; got {:#?}",
            trace[0].notes
        );
    }

    #[test]
    fn many_backrefs_into_large_capture_stay_bounded() {
        // R2-1 / R2-5: a template authored with many `$0` backrefs into a
        // capture spanning most of the input must not materialize a piece
        // vector (or the full expansion) before the budget check. Streaming
        // parse stops after the first `$0` fills the cap.
        let input = "a".repeat(MAX_HYGIENE_INPUT_CHARS);
        let template = "$0".repeat(100_000);
        let (out, trace) = apply_hygiene(vec![entry_with_hygiene(
            "kb_1",
            &input,
            &serde_json::json!([{ "pattern": "^(a+)$", "replacement": template }]),
        )]);
        let summary = out[0].body.as_ref().unwrap().summary.as_deref().unwrap();
        assert_eq!(summary.chars().count(), MAX_HYGIENE_OUTPUT_CHARS);
        assert_eq!(trace[0].applied, 1);
        assert!(
            trace[0].notes.iter().any(|n| n.contains("truncated")),
            "truncation must be noted; got {:#?}",
            trace[0].notes
        );
    }

    #[test]
    fn multi_match_backrefs_stop_at_output_cap() {
        // R2-1: multi-match passes with backrefs still stop at the output
        // cap and record the truncation (the many-small-matches contract,
        // now with `$0` substitution). Each "a" → "aa" adds one char, so
        // the cap is hit after half the matches.
        let input = "a".repeat(MAX_HYGIENE_INPUT_CHARS);
        let (out, trace) = apply_hygiene(vec![entry_with_hygiene(
            "kb_1",
            &input,
            &serde_json::json!([{ "pattern": "a", "replacement": "$0$0" }]),
        )]);
        let summary = out[0].body.as_ref().unwrap().summary.as_deref().unwrap();
        assert_eq!(summary.chars().count(), MAX_HYGIENE_OUTPUT_CHARS);
        assert_eq!(trace[0].applied, MAX_HYGIENE_OUTPUT_CHARS / 2);
        assert_eq!(trace[0].skipped, 0);
        assert_eq!(trace[0].notes.len(), 1);
        assert!(trace[0].notes[0].contains("truncated"));
    }

    #[test]
    fn backrefs_substitute_like_regex_expand() {
        // R2-1: normal `$N` / `${name}` / `$$` substitution is unchanged —
        // the segment expansion must produce the same output the regex
        // crate's `expand` produced.
        let (out, trace) = apply_hygiene(vec![entry_with_hygiene(
            "kb_1",
            "the hero slays the dragon",
            &serde_json::json!([{
                "pattern": r"^(\w+) (\w+) slays the (\w+)$",
                "replacement": r"$3 ${1} $2 $$"
            }]),
        )]);
        let summary = out[0].body.as_ref().unwrap().summary.as_deref().unwrap();
        assert_eq!(summary, "dragon the hero $");
        assert_eq!(trace[0].applied, 1);
        assert_eq!(trace[0].skipped, 0);
        assert!(trace[0].notes.is_empty());
    }

    #[test]
    fn absent_group_refs_expand_to_empty() {
        // Mirrors `regex`'s expand: `$N` / `${name}` referencing absent
        // groups expand to the empty string, and a lone `$` is literal.
        let (out, trace) = apply_hygiene(vec![entry_with_hygiene(
            "kb_1",
            "x",
            &serde_json::json!([{ "pattern": "x", "replacement": "a$5-${nope}b tail$" }]),
        )]);
        let summary = out[0].body.as_ref().unwrap().summary.as_deref().unwrap();
        assert_eq!(summary, "a-b tail$");
        assert_eq!(trace[0].applied, 1);
        assert!(trace[0].notes.is_empty());
    }

    #[test]
    fn absent_group_refs_over_many_matches_honor_piece_budget() {
        // R2-7: absent `$N` refs expand to empty and previously did not
        // consume the output-char budget, so `$9`.repeat(N) over a long
        // input scanned the whole template per match (unbounded CPU). The
        // shared piece-visit budget stops the pass; the original text is
        // kept when nothing was applied.
        let input = "a".repeat(MAX_HYGIENE_INPUT_CHARS);
        let template = "$9".repeat(100_000);
        let (out, trace) = apply_hygiene(vec![entry_with_hygiene(
            "kb_1",
            &input,
            &serde_json::json!([{ "pattern": "a", "replacement": template }]),
        )]);
        let summary = out[0].body.as_ref().unwrap().summary.as_deref().unwrap();
        assert_eq!(summary, input);
        assert_eq!(trace[0].applied, 0);
        assert_eq!(trace[0].skipped, 1);
        assert!(
            trace[0].notes.iter().any(|n| n.contains("work over")),
            "work bound must be noted; got {:#?}",
            trace[0].notes
        );
    }

    #[test]
    fn unmatched_span_after_near_cap_replacement_does_not_overflow() {
        // A first match can nearly fill the output budget; the unmatched
        // span before a later match must still honor the cap. Growing
        // `out_chars` past the cap and subtracting later panics in
        // overflow-checking builds and wraps (over-cap emit) in release.
        let nearly_full = "b".repeat(MAX_HYGIENE_OUTPUT_CHARS - 8);
        let unmatched = "c".repeat(64);
        let input = format!("X{unmatched}X");
        let (out, trace) = apply_hygiene(vec![entry_with_hygiene(
            "kb_1",
            &input,
            &serde_json::json!([{ "pattern": "X", "replacement": nearly_full }]),
        )]);
        let summary = out[0].body.as_ref().unwrap().summary.as_deref().unwrap();
        assert_eq!(summary.chars().count(), MAX_HYGIENE_OUTPUT_CHARS);
        assert_eq!(trace[0].applied, 1);
        assert!(
            trace[0].notes.iter().any(|n| n.contains("truncated")),
            "truncation must be noted; got {:#?}",
            trace[0].notes
        );
    }

    #[test]
    fn unmatched_tail_after_expanding_replacement_is_capped() {
        // Replacing a short prefix with a longer string leaves an unmatched
        // tail that would otherwise push the emit one char over the cap.
        let tail = "a".repeat(MAX_HYGIENE_INPUT_CHARS - 1);
        let input = format!("X{tail}");
        let (out, trace) = apply_hygiene(vec![entry_with_hygiene(
            "kb_1",
            &input,
            &serde_json::json!([{ "pattern": "X", "replacement": "YY" }]),
        )]);
        let summary = out[0].body.as_ref().unwrap().summary.as_deref().unwrap();
        assert_eq!(summary.chars().count(), MAX_HYGIENE_OUTPUT_CHARS);
        assert_eq!(trace[0].applied, 1);
        assert_eq!(trace[0].notes.len(), 1);
        assert!(trace[0].notes[0].contains("truncated"));
    }
}
