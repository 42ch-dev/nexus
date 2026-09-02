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
//! output — guardrails, not features. A carrier with more than
//! [`MAX_HYGIENE_TRANSFORMS`] transforms is also bounded, at the PARSE
//! site: the carrier loop stops after the cap's worth of transforms are
//! collected, elements beyond are never read, each counts toward the
//! entry's trace `skipped` (via the serde-known array length), and one
//! note records the degradation — CPU per assembly never scales with an
//! unrestricted carrier array length. Malformed diagnostics never
//! allocate per element either: they aggregate into a single count note,
//! and the scan halts once more than 8 × [`MAX_HYGIENE_TRANSFORMS`]
//! elements have been visited without filling the valid cap, so a
//! persisted carrier with a huge malformed prefix costs bounded parse
//! work and note storage per assembly (R8-1). Replacement-append work is
//! bounded the same way: the output-cap decision scans at most
//! `remaining + 1` chars of an authored replacement literal, never the
//! whole literal, so one match against a huge replacement costs O(budget)
//! work, not O(authored length) (R8-2). The replacement cursor's `$` scan
//! is bounded the same way: each `next()` searches at most
//! [`REPLACEMENT_SCAN_WINDOW_CHARS`] chars of the template (a
//! char-boundary-safe window), so a huge literal template costs O(budget)
//! scan work per piece, not O(authored length) (R8-3). The two scans on
//! the `$`-at-0 path are bounded the same way (R9b): `reference_piece`'s
//! `}` search and name-run scan never walk the whole remaining template,
//! so a `$` followed by a huge run costs O(budget) work per piece, not
//! O(authored length).
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
/// Maximum transforms parsed and applied per entry (Q6-guardrail family).
///
/// A persisted carrier with an unbounded array would parse, allocate, and
/// run a fresh regex pass per transform over up to 64 KiB of text, so CPU
/// per assembly scales with the array length. The cap bounds the PARSE
/// loop: parsing stops once this many transforms are collected, elements
/// beyond are never read, each counts toward the entry's trace `skipped`
/// (via the serde-known array length), and one degradation note records
/// the cap.
pub const MAX_HYGIENE_TRANSFORMS: usize = 32;

/// Replacement-cursor scan window — chars.
///
/// [`ReplacementCursor::next`] searches at most this many chars of the
/// authored replacement template for the next `$` (R8-3), and
/// [`ReplacementCursor::reference_piece`] bounds its `}` search and
/// name-run scan to the same window (R9b). `MAX + 1` so a `$` exactly at
/// the output-budget boundary is still found; a huge literal template (no
/// `$`) costs O(budget) scan work per piece, never O(authored length).
const REPLACEMENT_SCAN_WINDOW_CHARS: usize = MAX_HYGIENE_OUTPUT_CHARS + 1;

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
            // G-1: `parse_carrier` already bounds parsing at
            // MAX_HYGIENE_TRANSFORMS (elements beyond the cap are never
            // read; the unread tail is counted into `skipped` with one
            // degradation note at the parse site), so the apply loop below
            // never sees more than the cap — no apply-time truncation
            // needed, and the regex work per transform is bounded too.
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
                // The match is consumed (partially replaced): the tail
                // append must resume after it, never re-emit the prefix.
                last_end = m.end();
                truncated = true;
                break;
            }
            ExpansionOutcome::None => {
                truncated = true;
                break;
            }
            ExpansionOutcome::WorkLimit => {
                // The match is consumed (replacement abandoned): the tail
                // append must resume after it, never re-emit the prefix.
                last_end = m.end();
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
/// objects. Malformed elements are skipped and reported as ONE aggregated
/// note with a count (R8-1 — never one note per element); the scan itself
/// is hard-bounded by a visited-element budget. The returned count is the
/// number of elements that did not yield an executable transform:
/// malformed elements before the halt point plus every element beyond it
/// (never read, counted via the serde-known array length — G-1).
fn parse_carrier(carrier: &serde_json::Value) -> (Vec<HygieneTransform<'_>>, Vec<String>, usize) {
    // R8-1: the scan halts once this many elements have been visited
    // without filling the valid cap — a persisted carrier with a huge
    // malformed prefix costs bounded parse work per assembly.
    const SCAN_BUDGET: usize = MAX_HYGIENE_TRANSFORMS * 8;
    let Some(items) = carrier.as_array() else {
        return (
            Vec::new(),
            vec!["hygiene carrier must be an array of transforms".to_string()],
            0,
        );
    };
    // G-1 / R8-1: the cap bounds the PARSE loop. Elements beyond the first
    // MAX_HYGIENE_TRANSFORMS successfully-parsed transforms are never read
    // (no object/field visit, no diagnostic, no allocation), and the scan
    // itself halts after a fixed visited-element budget — a persisted
    // carrier with a huge malformed prefix neither scans the whole array
    // nor allocates one note per malformed element.
    let mut transforms = Vec::with_capacity(MAX_HYGIENE_TRANSFORMS.min(items.len()));
    let mut notes = Vec::new();
    let mut malformed = 0;
    let mut scanned = 0usize;
    let mut scan_budget_hit = false;
    for item in items {
        if transforms.len() == MAX_HYGIENE_TRANSFORMS {
            break;
        }
        if scanned == SCAN_BUDGET {
            scan_budget_hit = true;
            break;
        }
        scanned += 1;
        let Some(obj) = item.as_object() else {
            malformed += 1;
            continue;
        };
        let Some(pattern) = obj.get("pattern").and_then(serde_json::Value::as_str) else {
            malformed += 1;
            continue;
        };
        let Some(replacement) = obj.get("replacement").and_then(serde_json::Value::as_str) else {
            malformed += 1;
            continue;
        };
        let description = obj.get("description").and_then(serde_json::Value::as_str);
        transforms.push(HygieneTransform {
            pattern,
            replacement,
            description,
        });
    }
    // The unread tail beyond the halt point counts toward the entry's
    // `skipped` total — the array length is already known from serde, so
    // the count needs no further iteration. Malformed diagnostics
    // aggregate into ONE note carrying the count of SCANNED malformed
    // elements (the unread tail is the halt note's subject, never a
    // per-element diagnostic).
    let beyond = items.len().saturating_sub(scanned);
    let scanned_malformed = malformed;
    malformed += beyond;
    if scanned_malformed > 0 {
        notes.push(format!(
            "{scanned_malformed} malformed hygiene transforms skipped"
        ));
    }
    if scan_budget_hit {
        notes.push("hygiene carrier scan budget exceeded; remaining elements ignored".to_string());
    } else if beyond > 0 {
        notes.push(format!(
            "hygiene transforms beyond {MAX_HYGIENE_TRANSFORMS} not applied"
        ));
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
///
/// Each `next()` scans at most [`REPLACEMENT_SCAN_WINDOW_CHARS`] chars
/// (R8-3): a huge literal template costs O(budget) work per piece, never
/// O(authored length). The `$`-at-0 path is bounded the same way (R9b):
/// [`Self::reference_piece`]'s `}` search and name-run scan never walk
/// the whole remaining template.
struct ReplacementCursor<'a> {
    rest: &'a str,
}

impl<'a> ReplacementCursor<'a> {
    const fn new(replacement: &'a str) -> Self {
        Self { rest: replacement }
    }

    /// Parse the reference starting at the current `$` (position 0 of
    /// `self.rest`) and advance `rest` past it. Mirrors the `regex` crate's
    /// `expand` syntax; a `$` that starts no valid reference is literal.
    ///
    /// Both scans are bounded to [`REPLACEMENT_SCAN_WINDOW_CHARS`] chars
    /// (R9b): the `}` search and the name-run scan never walk the whole
    /// remaining template, so a `$` followed by a huge run costs O(budget)
    /// work per piece, not O(authored length).
    fn reference_piece(&mut self) -> ReplacementPiece<'a> {
        let after = &self.rest[1..];
        if after.as_bytes().first() == Some(&b'$') {
            self.rest = &after[1..];
            return ReplacementPiece::Literal("$");
        }
        if after.as_bytes().first() == Some(&b'{') {
            // R9b: bound the `}` search to a char-boundary-safe window of
            // REPLACEMENT_SCAN_WINDOW_CHARS chars — a `${name` whose closing
            // `}` sits beyond the window is a literal `$` (the name can
            // never resolve: the pattern is capped at
            // MAX_HYGIENE_PATTERN_CHARS, far below the window), and the tail
            // is emitted as literal pieces, truncated by append_capped —
            // never a whole-rest scan per match. A `}` in-window is found
            // at the same index as the old whole-rest `find`, so the slices
            // are byte-identical.
            let mut end = None;
            for (idx, ch) in after.char_indices().take(REPLACEMENT_SCAN_WINDOW_CHARS) {
                if ch == '}' {
                    end = Some(idx);
                    break;
                }
            }
            if let Some(end) = end {
                self.rest = &after[end + 1..];
                return replacement_piece(&after[1..end]);
            }
            self.rest = after;
            return ReplacementPiece::Literal("$");
        }
        // R9b: bound the name-run scan to the window — the run is ASCII
        // (`[0-9A-Za-z_]`), so `take` on `bytes()` is a char-boundary-safe
        // window. A run that exceeds the window is truncated to it: the
        // truncated name can never resolve (the pattern is capped at
        // MAX_HYGIENE_PATTERN_CHARS, far below the window), so it expands
        // to empty and the tail is emitted as literal pieces, truncated by
        // append_capped — never a whole-rest scan per match.
        let run = after
            .bytes()
            .take(REPLACEMENT_SCAN_WINDOW_CHARS)
            .take_while(|&b| b.is_ascii_alphanumeric() || b == b'_')
            .count();
        if run == 0 {
            self.rest = after;
            return ReplacementPiece::Literal("$");
        }
        self.rest = &after[run..];
        replacement_piece(&after[..run])
    }
}

impl<'a> Iterator for ReplacementCursor<'a> {
    type Item = ReplacementPiece<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.rest.is_empty() {
            return None;
        }
        // R8-3: bound the `$` scan to a char-boundary-safe window of
        // REPLACEMENT_SCAN_WINDOW_CHARS chars — a huge literal template
        // (no `$`) costs O(budget) scan work per piece, not O(authored
        // length). The scan stops at the first `$` or the window edge,
        // whichever comes first, so a backref-heavy template (`$0$0…`)
        // still costs O(1) per piece. A `$` at the current position is
        // parsed by [`Self::reference_piece`]; otherwise the window is a
        // prefix of `rest`, so `&self.rest[..i]` / `&self.rest[i..]` are
        // byte-identical to the old whole-rest slices, and a window with no
        // `$` is one literal piece with `rest` advanced past it
        // (append_capped applies the output bound either way).
        let mut window_end = self.rest.len();
        let mut dollar_at = None;
        for (idx, ch) in self.rest.char_indices().take(REPLACEMENT_SCAN_WINDOW_CHARS) {
            if ch == '$' {
                dollar_at = Some(idx);
                break;
            }
            window_end = idx + ch.len_utf8();
        }
        match dollar_at {
            None => {
                let lit = &self.rest[..window_end];
                self.rest = &self.rest[window_end..];
                Some(ReplacementPiece::Literal(lit))
            }
            Some(0) => Some(self.reference_piece()),
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
    // R8-2: the cap decision scans at most `remaining + 1` chars — never
    // the whole authored segment — so a huge replacement literal costs
    // O(budget) work per append, not O(authored length). `chars()` is
    // lazy: nothing beyond the take-bound is visited or materialized.
    let seg_chars = seg.chars().take(remaining + 1).count();
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
    fn huge_replacement_literal_append_capped_by_output_budget() {
        // R8-2: `append_capped` used to call `seg.chars().count()` — a
        // FULL scan of the authored replacement before the cap decision, so
        // one match against a huge literal cost O(authored length) per
        // assembly. The cap decision must scan only up to `remaining + 1`
        // chars. The observable contract is unchanged: the output is capped
        // at the budget, the kept prefix is the literal's head, and the
        // truncation is recorded.
        let big_replacement = "x".repeat(1_000_000);
        let (out, trace) = apply_hygiene(vec![entry_with_hygiene(
            "kb_1",
            "a",
            &serde_json::json!([{ "pattern": "a", "replacement": big_replacement }]),
        )]);
        let summary = out[0].body.as_ref().unwrap().summary.as_deref().unwrap();
        assert_eq!(summary.chars().count(), MAX_HYGIENE_OUTPUT_CHARS);
        assert_eq!(summary, &"x".repeat(MAX_HYGIENE_OUTPUT_CHARS));
        assert_eq!(trace[0].applied, 1);
        assert_eq!(trace[0].skipped, 0);
        assert_eq!(trace[0].notes.len(), 1);
        assert!(trace[0].notes[0].contains("truncated"));
    }

    // ── R8-3: bounded replacement-cursor scan window ──

    #[test]
    fn replacement_cursor_scan_window_bounded() {
        // R8-3: `ReplacementCursor::next` used to call `self.rest.find('$')`
        // over the ENTIRE remaining template — a huge literal replacement
        // (no `$` at all) cost O(authored length) scan work per match before
        // the output cap ran. The scan is now bounded to a char-boundary-safe
        // window of MAX_HYGIENE_OUTPUT_CHARS + 1 chars: a literal template
        // yields window-sized pieces, never one piece spanning the whole
        // template, and the tail stays reachable in further window-sized
        // pieces (nothing is dropped).
        let template = "x".repeat(1_000_000);
        let mut cursor = ReplacementCursor::new(&template);
        for _ in 0..2 {
            let piece = cursor.next().expect("non-empty template");
            match piece {
                ReplacementPiece::Literal(s) => assert_eq!(
                    s.chars().count(),
                    MAX_HYGIENE_OUTPUT_CHARS + 1,
                    "literal piece must be window-sized, not the whole template"
                ),
                other => panic!("expected literal piece, got {other:?}"),
            }
        }
        let mut total = 0usize;
        for piece in &mut cursor {
            match piece {
                ReplacementPiece::Literal(s) => total += s.chars().count(),
                other => panic!("expected literal piece, got {other:?}"),
            }
        }
        assert_eq!(
            total + 2 * (MAX_HYGIENE_OUTPUT_CHARS + 1),
            template.chars().count(),
            "the windowed scan must cover the whole template"
        );
    }

    #[test]
    fn first_dollar_beyond_scan_window_defers_to_truncation() {
        // R8-3: a template whose first `$` sits beyond the scan window must
        // defer it — the window literal exhausts the output budget first, so
        // the emitted output is the budget-capped prefix of the literal with
        // the truncation note, exactly as before the window bound.
        let mut template = "a".repeat(MAX_HYGIENE_OUTPUT_CHARS + 1);
        template.push_str("${0}");
        template.push_str(&"b".repeat(100));
        let (out, trace) = apply_hygiene(vec![entry_with_hygiene(
            "kb_1",
            "x",
            &serde_json::json!([{ "pattern": "x", "replacement": template }]),
        )]);
        let summary = out[0].body.as_ref().unwrap().summary.as_deref().unwrap();
        assert_eq!(summary.chars().count(), MAX_HYGIENE_OUTPUT_CHARS);
        assert_eq!(summary, &"a".repeat(MAX_HYGIENE_OUTPUT_CHARS));
        assert_eq!(trace[0].applied, 1);
        assert_eq!(trace[0].skipped, 0);
        assert_eq!(trace[0].notes.len(), 1);
        assert!(trace[0].notes[0].contains("truncated"));
    }

    #[test]
    fn in_window_dollar_still_expands() {
        // R8-3: a `$` reference inside the scan window is still found and
        // expanded — the window bound must not hide in-window references.
        let mut template = "a".repeat(MAX_HYGIENE_OUTPUT_CHARS - 10);
        template.push_str("${0}");
        template.push_str(&"b".repeat(100));
        let (out, trace) = apply_hygiene(vec![entry_with_hygiene(
            "kb_1",
            "x",
            &serde_json::json!([{ "pattern": "x", "replacement": template }]),
        )]);
        let summary = out[0].body.as_ref().unwrap().summary.as_deref().unwrap();
        // The in-window `${0}` expands to the match; the tail beyond the
        // remaining budget is truncated.
        let mut expected = "a".repeat(MAX_HYGIENE_OUTPUT_CHARS - 10);
        expected.push('x');
        expected.push_str(&"b".repeat(9));
        assert_eq!(summary, expected);
        assert_eq!(summary.chars().count(), MAX_HYGIENE_OUTPUT_CHARS);
        assert_eq!(trace[0].applied, 1);
        assert_eq!(trace[0].skipped, 0);
        assert_eq!(trace[0].notes.len(), 1);
        assert!(trace[0].notes[0].contains("truncated"));
    }

    #[test]
    fn multibyte_template_crossing_scan_window_no_panic() {
        // R8-3: the scan window is a char-boundary-safe prefix — a multi-byte
        // UTF-8 char straddling the window edge must be kept whole (no panic
        // on the `rest` re-slice) and the emitted output stays the
        // budget-capped prefix with the truncation note.
        let mut template = "a".repeat(MAX_HYGIENE_OUTPUT_CHARS);
        template.push('é'); // 2-byte char: the last char of the window
        template.push_str(&"b".repeat(100));
        let (out, trace) = apply_hygiene(vec![entry_with_hygiene(
            "kb_1",
            "x",
            &serde_json::json!([{ "pattern": "x", "replacement": template }]),
        )]);
        let summary = out[0].body.as_ref().unwrap().summary.as_deref().unwrap();
        assert_eq!(summary.chars().count(), MAX_HYGIENE_OUTPUT_CHARS);
        assert_eq!(summary, &"a".repeat(MAX_HYGIENE_OUTPUT_CHARS));
        assert_eq!(trace[0].applied, 1);
        assert_eq!(trace[0].skipped, 0);
        assert_eq!(trace[0].notes.len(), 1);
        assert!(trace[0].notes[0].contains("truncated"));
    }

    // ── R9b: bounded reference_piece scans (the `$`-at-0 path) ──

    #[test]
    fn reference_piece_braced_scan_window_bounded() {
        // R9b: `reference_piece`'s `}` search used to scan the ENTIRE
        // remaining template for the closing brace — a `$` followed by `{`
        // and a huge non-`}` run cost O(authored length) scan work per
        // match. The search is now bounded to the char-safe window: a `$`
        // whose closing `}` sits beyond the window is a literal `$` (the
        // name can never resolve — the pattern is capped at
        // MAX_HYGIENE_PATTERN_CHARS, far below the window) and the tail is
        // emitted as window-sized literal pieces, never one piece spanning
        // the whole template.
        let mut template = "${".to_string();
        template.push_str(&"a".repeat(1_000_000));
        template.push('}'); // closing brace beyond the scan window
        let mut cursor = ReplacementCursor::new(&template);
        let first = cursor.next().expect("non-empty template");
        assert_eq!(first, ReplacementPiece::Literal("$"));
        for _ in 0..2 {
            let piece = cursor.next().expect("tail must stay reachable");
            match piece {
                ReplacementPiece::Literal(s) => assert_eq!(
                    s.chars().count(),
                    REPLACEMENT_SCAN_WINDOW_CHARS,
                    "literal piece must be window-sized, not the whole template"
                ),
                other => panic!("expected literal piece, got {other:?}"),
            }
        }
        let mut total = 1usize; // the leading literal "$"
        for piece in &mut cursor {
            match piece {
                ReplacementPiece::Literal(s) => total += s.chars().count(),
                other => panic!("expected literal piece, got {other:?}"),
            }
        }
        assert_eq!(
            total + 2 * REPLACEMENT_SCAN_WINDOW_CHARS,
            template.chars().count(),
            "the windowed scan must cover the whole template"
        );
    }

    #[test]
    fn reference_piece_name_run_scan_window_bounded() {
        // R9b: `reference_piece`'s name-run scan used to scan the ENTIRE
        // remaining template for the `[0-9A-Za-z_]` run — a `$` followed by
        // a huge name run cost O(authored length) scan work per match. The
        // scan is now bounded to the char-safe window: the name is truncated
        // to the window (a name that long can never resolve — the pattern
        // is capped at MAX_HYGIENE_PATTERN_CHARS, far below the window) and
        // the tail is emitted as window-sized literal pieces, never one
        // piece spanning the whole template.
        let mut template = "$".to_string();
        template.push_str(&"a".repeat(1_000_000));
        let mut cursor = ReplacementCursor::new(&template);
        let first = cursor.next().expect("non-empty template");
        match first {
            ReplacementPiece::Name(s) => assert_eq!(
                s.chars().count(),
                REPLACEMENT_SCAN_WINDOW_CHARS,
                "name piece must be window-sized, not the whole run"
            ),
            other => panic!("expected name piece, got {other:?}"),
        }
        for _ in 0..2 {
            let piece = cursor.next().expect("tail must stay reachable");
            match piece {
                ReplacementPiece::Literal(s) => assert_eq!(
                    s.chars().count(),
                    REPLACEMENT_SCAN_WINDOW_CHARS,
                    "literal piece must be window-sized, not the whole template"
                ),
                other => panic!("expected literal piece, got {other:?}"),
            }
        }
        let mut total = 0usize;
        for piece in &mut cursor {
            match piece {
                ReplacementPiece::Literal(s) => total += s.chars().count(),
                other => panic!("expected literal piece, got {other:?}"),
            }
        }
        // The leading `$` is consumed by the reference (not a piece), so
        // the covered chars are the name piece + the literal tail.
        assert_eq!(
            total + 3 * REPLACEMENT_SCAN_WINDOW_CHARS + 1,
            template.chars().count(),
            "the windowed scan must cover the whole template"
        );
    }

    #[test]
    fn braced_name_without_closing_brace_bounded() {
        // R9b end-to-end: a `${name` with no `}` in-window is a literal `$`
        // and the `{aaa…` tail is emitted as literal pieces, truncated at
        // the output budget — no panic, bounded pieces, and the emitted
        // output is identical to the pre-bound behavior (the `$` was
        // already literal when no `}` exists anywhere).
        let mut template = "${".to_string();
        template.push_str(&"a".repeat(1_000_000));
        let (out, trace) = apply_hygiene(vec![entry_with_hygiene(
            "kb_1",
            "x",
            &serde_json::json!([{ "pattern": "x", "replacement": template }]),
        )]);
        let summary = out[0].body.as_ref().unwrap().summary.as_deref().unwrap();
        let mut expected = "${".to_string();
        expected.push_str(&"a".repeat(MAX_HYGIENE_OUTPUT_CHARS - 2));
        assert_eq!(summary, expected);
        assert_eq!(summary.chars().count(), MAX_HYGIENE_OUTPUT_CHARS);
        assert_eq!(trace[0].applied, 1);
        assert_eq!(trace[0].skipped, 0);
        assert_eq!(trace[0].notes.len(), 1);
        assert!(trace[0].notes[0].contains("truncated"));
    }

    #[test]
    fn huge_bare_name_run_bounded() {
        // R9b end-to-end: a `$` followed by a huge `[0-9A-Za-z_]` run — the
        // name is truncated to the window (it can never resolve: the
        // pattern is capped at MAX_HYGIENE_PATTERN_CHARS, far below the
        // window) and the tail is emitted as literal pieces, truncated at
        // the output budget — no panic, bounded pieces.
        let mut template = "$".to_string();
        template.push_str(&"a".repeat(1_000_000));
        let (out, trace) = apply_hygiene(vec![entry_with_hygiene(
            "kb_1",
            "x",
            &serde_json::json!([{ "pattern": "x", "replacement": template }]),
        )]);
        let summary = out[0].body.as_ref().unwrap().summary.as_deref().unwrap();
        assert_eq!(summary.chars().count(), MAX_HYGIENE_OUTPUT_CHARS);
        assert_eq!(summary, &"a".repeat(MAX_HYGIENE_OUTPUT_CHARS));
        assert_eq!(trace[0].applied, 1);
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
    fn transforms_beyond_cap_skipped_with_note() {
        // G-1: the transform-count cap now bounds the PARSE loop (round-7
        // fix). A persisted carrier with an unrestricted transform array
        // used to parse/diagnose every element before the apply-time slice
        // — CPU per assembly scaled with the array length even though only
        // MAX_HYGIENE_TRANSFORMS ever executed. Parsing stops at the cap:
        // elements beyond are never read, each counts toward `skipped` (via
        // the serde-known array length), and one note records the
        // degradation — a large array stays cheap per assembly.
        let total = MAX_HYGIENE_TRANSFORMS * 128 + 4;
        let summary = (0..total)
            .map(|i| format!("a{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let carrier: Vec<serde_json::Value> = (0..total)
            .map(|i| {
                serde_json::json!({
                    "pattern": format!(r"\ba{i}\b"),
                    "replacement": format!("b{i}"),
                })
            })
            .collect();
        let (out, trace) = apply_hygiene(vec![entry_with_hygiene(
            "kb_1",
            &summary,
            &serde_json::json!(carrier),
        )]);
        let out_summary = out[0].body.as_ref().unwrap().summary.as_deref().unwrap();
        // The first cap transforms applied; the tail beyond the cap is
        // untouched (its transforms were never parsed or executed).
        let expected = (0..total)
            .map(|i| {
                if i < MAX_HYGIENE_TRANSFORMS {
                    format!("b{i}")
                } else {
                    format!("a{i}")
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(out_summary, expected);
        assert_eq!(trace[0].applied, MAX_HYGIENE_TRANSFORMS);
        assert_eq!(trace[0].skipped, total - MAX_HYGIENE_TRANSFORMS);
        assert_eq!(trace[0].notes.len(), 1);
        assert!(trace[0].notes[0].contains("transforms beyond"));
        assert!(trace[0].notes[0].contains(&MAX_HYGIENE_TRANSFORMS.to_string()));
    }

    #[test]
    fn parse_carrier_stops_collecting_at_cap() {
        // G-1 (round-7): the cap bounds the parse loop itself. Elements
        // beyond the first MAX_HYGIENE_TRANSFORMS successfully-parsed
        // transforms are never read — no per-element diagnostic. The
        // serde-known array length accounts for the unread tail in the
        // returned degraded count, and one collective note records the
        // degradation.
        let carrier: Vec<serde_json::Value> = (0..MAX_HYGIENE_TRANSFORMS)
            .map(|i| {
                serde_json::json!({
                    "pattern": format!("a{i}"),
                    "replacement": format!("b{i}"),
                })
            })
            .chain(
                // Malformed tail (missing replacement): never read.
                (0..8).map(|i| serde_json::json!({ "pattern": format!("m{i}") })),
            )
            .collect();
        let parsed = serde_json::json!(carrier);
        let (transforms, notes, malformed) = parse_carrier(&parsed);
        assert_eq!(transforms.len(), MAX_HYGIENE_TRANSFORMS);
        // The whole tail is degraded without being read: skipped counted via
        // length arithmetic, exactly one collective note, no per-element
        // malformed diagnostics.
        assert_eq!(malformed, 8);
        assert_eq!(notes.len(), 1);
        assert!(notes[0].contains("transforms beyond"));
    }

    #[test]
    fn malformed_elements_beyond_cap_count_once_without_per_element_notes() {
        // G-1 (round-7): a malformed element beyond the cap must not be
        // individually diagnosed — parsing stops at the cap and the unread
        // tail is counted into `skipped` via the array length. The old
        // parse-everything behavior emitted one diagnostic note per
        // malformed element beyond the cap.
        let carrier: Vec<serde_json::Value> = (0..MAX_HYGIENE_TRANSFORMS)
            .map(|i| {
                serde_json::json!({
                    "pattern": format!(r"\ba{i}\b"),
                    "replacement": format!("b{i}"),
                })
            })
            .chain(
                // Malformed tail (missing replacement): counted into
                // `skipped` collectively, never read.
                (0..4).map(|i| serde_json::json!({ "pattern": format!("a{i}") })),
            )
            .collect();
        let summary = (0..MAX_HYGIENE_TRANSFORMS)
            .map(|i| format!("a{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let (out, trace) = apply_hygiene(vec![entry_with_hygiene(
            "kb_1",
            &summary,
            &serde_json::json!(carrier),
        )]);
        let out_summary = out[0].body.as_ref().unwrap().summary.as_deref().unwrap();
        let expected = (0..MAX_HYGIENE_TRANSFORMS)
            .map(|i| format!("b{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(out_summary, expected);
        assert_eq!(trace[0].applied, MAX_HYGIENE_TRANSFORMS);
        // 4 unread tail elements, no per-element parse note: exactly one
        // collective degradation note.
        assert_eq!(trace[0].skipped, 4);
        assert_eq!(
            trace[0].notes.len(),
            1,
            "beyond-cap tail must yield one collective note, got {:#?}",
            trace[0].notes
        );
        assert!(trace[0].notes[0].contains("transforms beyond"));
    }

    #[test]
    fn malformed_before_cap_and_beyond_tail_account_together() {
        // G-1 (round-7) + R8-1: malformed elements before the cap keep
        // their skipped slots (they were never executable) but their
        // diagnostics aggregate into a single count note; the unread tail
        // beyond the cap is counted into `skipped` via length arithmetic.
        // Total skipped stays malformed + beyond — identical accounting to
        // the old apply-time slice, now computed from the parse site.
        let summary = (0..MAX_HYGIENE_TRANSFORMS + 2)
            .map(|i| format!("a{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let mut carrier: Vec<serde_json::Value> = (0..2)
            .map(|i| serde_json::json!({ "pattern": format!("a{i}") })) // malformed: no replacement
            .collect();
        carrier.extend((0..MAX_HYGIENE_TRANSFORMS + 2).map(|i| {
            serde_json::json!({
                "pattern": format!(r"\ba{i}\b"),
                "replacement": format!("b{i}"),
            })
        }));
        let (out, trace) = apply_hygiene(vec![entry_with_hygiene(
            "kb_1",
            &summary,
            &serde_json::json!(carrier),
        )]);
        let out_summary = out[0].body.as_ref().unwrap().summary.as_deref().unwrap();
        // First 32 valid transforms (carrier indices 2..34) applied; the
        // last two valid transforms sit beyond the cap and never ran.
        let expected = (0..MAX_HYGIENE_TRANSFORMS)
            .map(|i| format!("b{i}"))
            .chain((MAX_HYGIENE_TRANSFORMS..MAX_HYGIENE_TRANSFORMS + 2).map(|i| format!("a{i}")))
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(out_summary, expected);
        assert_eq!(trace[0].applied, MAX_HYGIENE_TRANSFORMS);
        // 2 malformed before the cap + 2 valid beyond the cap.
        assert_eq!(trace[0].skipped, 4);
        // 1 aggregated malformed note + 1 collective beyond note.
        assert_eq!(trace[0].notes.len(), 2);
        assert!(trace[0]
            .notes
            .iter()
            .any(|n| n.contains("2 malformed hygiene transforms skipped")));
        assert!(trace[0]
            .notes
            .iter()
            .any(|n| n.contains("transforms beyond")));
    }

    #[test]
    fn huge_malformed_prefix_aggregates_notes_and_hard_stops() {
        // R8-1: a persisted carrier with a large malformed prefix used to
        // push ONE note string per malformed element (unbounded allocation)
        // and scan the whole array (CPU O(len)) when fewer than
        // MAX_HYGIENE_TRANSFORMS transforms ever parse. Diagnostics now
        // aggregate into a single count note and the scan halts after
        // 8 x MAX_HYGIENE_TRANSFORMS visited elements; the unread tail
        // counts into `skipped` via the serde-known array length.
        let total = 100_000;
        let carrier: Vec<serde_json::Value> = (0..total).map(|_| serde_json::json!(17)).collect();
        let (out, trace) = apply_hygiene(vec![entry_with_hygiene(
            "kb_1",
            "The hero fights the dragon",
            &serde_json::json!(carrier),
        )]);
        let summary = out[0].body.as_ref().unwrap().summary.as_deref().unwrap();
        assert_eq!(summary, "The hero fights the dragon");
        assert_eq!(trace[0].applied, 0);
        assert_eq!(trace[0].skipped, total);
        assert_eq!(
            trace[0].notes.len(),
            2,
            "exactly two notes regardless of carrier size, got {:#?}",
            trace[0].notes
        );
        assert!(trace[0].notes.iter().any(|n| {
            n.contains(&format!(
                "{} malformed hygiene transforms skipped",
                MAX_HYGIENE_TRANSFORMS * 8
            ))
        }));
        assert!(trace[0]
            .notes
            .iter()
            .any(|n| n.contains("scan budget exceeded")));
    }

    #[test]
    fn malformed_before_cap_aggregates_into_one_note() {
        // R8-1: malformed elements inside the scan budget produce ONE
        // aggregated note with a count — never one note per element. Their
        // skipped slots are unchanged.
        let carrier: Vec<serde_json::Value> = (0..5)
            .map(|i| serde_json::json!({ "pattern": format!("a{i}") })) // malformed: no replacement
            .collect();
        let parsed = serde_json::json!(carrier);
        let (transforms, notes, malformed) = parse_carrier(&parsed);
        assert!(transforms.is_empty());
        assert_eq!(malformed, 5);
        assert_eq!(notes.len(), 1);
        assert!(notes[0].contains("5 malformed hygiene transforms skipped"));
        assert!(!notes[0].contains("transforms beyond"));
    }

    #[test]
    fn scan_budget_boundary_is_exactly_scan_budget_elements() {
        // R8-1 boundary: the halt fires only when the visited-element
        // budget is EXCEEDED. Exactly 8 x MAX_HYGIENE_TRANSFORMS malformed
        // elements is still in budget (scanned fully, one aggregated note);
        // one more element trips the scan-budget note and the unread tail
        // still counts toward `skipped`.
        let in_budget: Vec<serde_json::Value> = (0..MAX_HYGIENE_TRANSFORMS * 8)
            .map(|_| serde_json::json!(17))
            .collect();
        let parsed = serde_json::json!(in_budget);
        let (transforms, notes, malformed) = parse_carrier(&parsed);
        assert!(transforms.is_empty());
        assert_eq!(malformed, MAX_HYGIENE_TRANSFORMS * 8);
        assert_eq!(notes.len(), 1);
        assert!(notes[0].contains("malformed hygiene transforms skipped"));
        assert!(!notes.iter().any(|n| n.contains("scan budget exceeded")));

        let over: Vec<serde_json::Value> = (0..=(MAX_HYGIENE_TRANSFORMS * 8))
            .map(|_| serde_json::json!(17))
            .collect();
        let parsed = serde_json::json!(over);
        let (_, notes, malformed) = parse_carrier(&parsed);
        assert_eq!(malformed, MAX_HYGIENE_TRANSFORMS * 8 + 1);
        assert_eq!(notes.len(), 2);
        assert!(notes.iter().any(|n| n.contains("scan budget exceeded")));
        assert!(notes
            .iter()
            .any(|n| n.contains("malformed hygiene transforms skipped")));
    }

    #[test]
    fn transforms_at_cap_apply_without_note() {
        // G-1 boundary: exactly MAX_HYGIENE_TRANSFORMS transforms is not a
        // degradation — all apply, no cap note.
        let total = MAX_HYGIENE_TRANSFORMS;
        let summary = (0..total)
            .map(|i| format!("a{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let carrier: Vec<serde_json::Value> = (0..total)
            .map(|i| {
                serde_json::json!({
                    "pattern": format!(r"\ba{i}\b"),
                    "replacement": format!("b{i}"),
                })
            })
            .collect();
        let (out, trace) = apply_hygiene(vec![entry_with_hygiene(
            "kb_1",
            &summary,
            &serde_json::json!(carrier),
        )]);
        let out_summary = out[0].body.as_ref().unwrap().summary.as_deref().unwrap();
        let expected = (0..total)
            .map(|i| format!("b{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        assert_eq!(out_summary, expected);
        assert_eq!(trace[0].applied, total);
        assert_eq!(trace[0].skipped, 0);
        assert!(trace[0].notes.is_empty());
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
    fn work_limit_after_unmatched_prefix_does_not_duplicate_prefix() {
        // G-2 regression (PR #237 re-review): when the shared piece-visit
        // budget is exhausted mid-pass after a non-empty unmatched prefix
        // was appended, the WorkLimit break must still advance `last_end`
        // past the match — otherwise the final tail append re-emits the
        // already-appended prefix (`prefixprefixX`). The match is consumed
        // (replacement abandoned), so the tail resumes after it.
        let template = "$9".repeat(MAX_HYGIENE_OUTPUT_CHARS + 1);
        let (out, trace) = apply_hygiene(vec![entry_with_hygiene(
            "kb_1",
            "prefixMX",
            &serde_json::json!([{ "pattern": "M", "replacement": template }]),
        )]);
        let summary = out[0].body.as_ref().unwrap().summary.as_deref().unwrap();
        assert_eq!(summary, "prefixX", "no duplicated prefix; got {summary:?}");
        assert_eq!(summary.matches("prefix").count(), 1);
        assert_eq!(trace[0].applied, 0);
        assert_eq!(trace[0].skipped, 1);
        assert!(
            trace[0].notes.iter().any(|n| n.contains("work over")),
            "work bound must be noted; got {:#?}",
            trace[0].notes
        );
    }

    #[test]
    fn partial_expansion_after_unmatched_prefix_does_not_duplicate_prefix() {
        // G-2 regression: when the output budget runs out mid-expansion
        // after a non-empty unmatched prefix was appended, the Partial
        // break must not re-emit the prefix. The match is consumed
        // (partially replaced), so the emitted text is the prefix plus the
        // partial expansion — exactly once.
        let big_replacement = "y".repeat(MAX_HYGIENE_OUTPUT_CHARS);
        let (out, trace) = apply_hygiene(vec![entry_with_hygiene(
            "kb_1",
            "prefixMX",
            &serde_json::json!([{ "pattern": "M", "replacement": big_replacement }]),
        )]);
        let summary = out[0].body.as_ref().unwrap().summary.as_deref().unwrap();
        assert_eq!(summary.chars().count(), MAX_HYGIENE_OUTPUT_CHARS);
        assert!(summary.starts_with("prefix"));
        assert_eq!(summary.matches("prefix").count(), 1);
        assert_eq!(trace[0].applied, 1);
        assert_eq!(trace[0].skipped, 0);
        assert!(
            trace[0].notes.iter().any(|n| n.contains("truncated")),
            "truncation must be noted; got {:#?}",
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
