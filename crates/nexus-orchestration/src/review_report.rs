//! `review-report.md` parser (V1.48 P0 T1).
//!
//! Implements the producer-side parsing contract from
//! `.mstar/knowledge/specs/novel-findings-maturity.md` §1 and the vocabulary
//! table in `.mstar/knowledge/specs/novel-quality-loop.md` §8 / §2.1.
//!
//! ## Hermeticity
//!
//! `parse_review_report(content: &str)` is a **pure function**: no filesystem,
//! no DB, no clock. Callers (the supervisor's `from-review` synthesis path in
//! [`crate::auto_chain`]) are responsible for reading the report file from
//! `Works/<work_ref>/Logs/review/review-report.md` and for the documented
//! fallback when the file is missing or unparsable.
//!
//! ## Mapping table (spec §1.2)
//!
//! | Report field        | Finding field        | Fallback   |
//! |---------------------|----------------------|------------|
//! | section / kind tag  | `kind`               | `craft`    |
//! | `severity`          | `severity`           | `info`     |
//! | issue body          | `body`               | excerpt    |
//! | suggestion block    | `rule_suggestion`    | `None`     |
//! | routing hint        | `target_executor`    | per-table  |

use std::collections::HashMap;

/// Finding `kind` vocabulary (spec §2.1 / §8; matches
/// `nexus_local_db::findings::FindingKind::ALL_STRS`).
///
/// Source-of-truth is the DB layer's closed set; this array mirrors it for
/// parser-side validation. A `spec §1.2` fallback applies when the report's
/// kind token is not in this set.
pub const KNOWN_FINDING_KINDS: &[&str] = &[
    "continuity",
    "craft",
    "plot_hole",
    "world_inconsistency",
    "pacing",
];

/// Finding `severity` vocabulary (spec §2.1).
pub const KNOWN_FINDING_SEVERITIES: &[&str] = &["info", "minor", "major", "blocker"];

/// Finding `target_executor` vocabulary (spec §2.2).
pub const KNOWN_TARGET_EXECUTORS: &[&str] = &["write", "brainstorm", "none", "master"];

/// Map report-side severity tokens → finding-side severity enum.
///
/// Reports written by the `novel-chapter-review` preset use
/// `critical / major / minor` (review-framework vocabulary). The findings
/// table stores `blocker / major / minor / info` (spec §2.1). Unknown tokens
/// fall back to `info` per spec §1.2.
///
/// Spec: `novel-findings-maturity.md` §1.2 + `novel-quality-loop.md` §2.1.
#[must_use]
pub fn map_severity(report_token: &str) -> &'static str {
    match report_token.trim().to_ascii_lowercase().as_str() {
        "critical" | "blocker" => "blocker",
        "major" => "major",
        "minor" => "minor",
        // `info`, unknown, or empty → safe default.
        _ => "info",
    }
}

/// Map a report-side `kind` token to the finding-side closed set.
///
/// Returns `None` for tokens that are not in [`KNOWN_FINDING_KINDS`]; the
/// caller applies the `craft` fallback per spec §1.2.
#[must_use]
pub fn map_kind(report_token: &str) -> Option<&'static str> {
    let normalized = report_token.trim().to_ascii_lowercase();
    KNOWN_FINDING_KINDS
        .iter()
        .copied()
        .find(|&known| known == normalized.as_str())
}

/// Map a report-side `target_executor` hint to the finding-side vocabulary.
///
/// Spec §1.2 default rule: `write` when the kind is `craft` or `continuity`;
/// else `brainstorm`. When the report names a known executor explicitly, that
/// wins.
#[must_use]
pub fn map_target_executor(report_token: &str, fallback_kind: &str) -> &'static str {
    let normalized = report_token.trim().to_ascii_lowercase();
    if let Some(matched) = KNOWN_TARGET_EXECUTORS
        .iter()
        .copied()
        .find(|&known| known == normalized.as_str())
    {
        return matched;
    }
    // Spec §1.2 default.
    match fallback_kind {
        "craft" | "continuity" => "write",
        _ => "brainstorm",
    }
}

/// One parsed finding row from a review report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedFinding {
    /// Closed-set kind (`continuity` / `craft` / `plot_hole` /
    /// `world_inconsistency` / `pacing`). Never empty.
    pub kind: String,
    /// Closed-set severity (`info` / `minor` / `major` / `blocker`).
    pub severity: String,
    /// Finding body — the issue description from the report. Never empty for
    /// a parsed row; callers may truncate before persisting.
    pub body: String,
    /// Optional Layer-2 rule suggestion prose; `None` when the report does
    /// not include one.
    pub rule_suggestion: Option<String>,
    /// Closed-set routing hint (`write` / `brainstorm` / `none` / `master`).
    pub target_executor: String,
}

/// Outcome of parsing a review report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedReviewReport {
    /// ≥0 parsed findings. Empty when the report contains no `## Issues`
    /// section or no parseable bullets (caller treats this as the
    /// "malformed → fallback" branch per spec §1.3).
    pub findings: Vec<ParsedFinding>,
    /// Best-effort overall assessment text from the report (used as a
    /// fallback `body` excerpt when no individual issues parse).
    pub overall_assessment: Option<String>,
}

/// Parse a `review-report.md` body into structured findings.
///
/// Looks for an `## Issues` (or `### Issues`) section and parses each bullet
/// as one finding row. Recognizes inline `kind:` / `severity:` / `executor:`
/// tags (case-insensitive) on the bullet line; the remainder of the bullet is
/// the finding `body`. An optional `rule_suggestion:` inline tag populates
/// the corresponding field.
///
/// Also reads a `## Overall Assessment` section (if present) so callers can
/// synthesize a fallback finding when the Issues section is empty or absent.
///
/// # Errors
///
/// Returns `Err(ParseError)` only on structural impossibility (empty input).
/// Malformed individual bullets are skipped — the parse is best-effort per
/// spec §1.3 (a partially-parsed report MUST still yield the rows that did
/// parse, plus the caller's documented fallback for the rest).
pub fn parse_review_report(content: &str) -> Result<ParsedReviewReport, ParseError> {
    if content.trim().is_empty() {
        return Err(ParseError::Empty);
    }

    let sections = split_sections(content);
    let overall_assessment = sections
        .iter()
        .find(|(heading, _)| heading_eq(heading, "overall assessment"))
        .map(|(_, body)| body.trim().to_string())
        .filter(|s| !s.is_empty());

    let findings_section = sections
        .iter()
        .find(|(heading, _)| heading_eq(heading, "issues"))
        .map(|(_, body)| body.as_str())
        .unwrap_or("");

    let findings = parse_issues_bullets(findings_section);

    Ok(ParsedReviewReport {
        findings,
        overall_assessment,
    })
}

/// Parse failures for [`parse_review_report`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    /// Input was empty or whitespace-only.
    #[error("review report content is empty")]
    Empty,
}

/// Split a markdown body into `(heading, body)` pairs by `##` (and `###`)
/// ATX headings. Lines before the first heading are attached to an empty
/// heading (discarded by callers that look up specific sections).
fn split_sections(content: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut current_heading = String::new();
    let mut current_body = String::new();
    let mut saw_any_heading = false;

    for line in content.lines() {
        let trimmed = line.trim_start();
        // Match `### ` before `## ` so an h3 heading doesn't get its first
        // `#` swallowed by the h2 prefix.
        if let Some(rest) = trimmed
            .strip_prefix("### ")
            .or_else(|| trimmed.strip_prefix("## "))
        {
            if saw_any_heading || !current_body.trim().is_empty() {
                out.push((std::mem::take(&mut current_heading), std::mem::take(&mut current_body)));
            } else {
                // First ever content was a heading — no preamble to push.
                current_heading.clear();
                current_body.clear();
            }
            current_heading.push_str(rest.trim());
            saw_any_heading = true;
        } else {
            current_body.push_str(line);
            current_body.push('\n');
        }
    }
    if saw_any_heading || !current_body.trim().is_empty() {
        out.push((current_heading, current_body));
    }
    out
}

fn heading_eq(actual: &str, expected_lower: &str) -> bool {
    actual.trim().to_ascii_lowercase() == expected_lower
}

/// Parse bullets inside the `## Issues` section body into findings.
fn parse_issues_bullets(section_body: &str) -> Vec<ParsedFinding> {
    section_body
        .lines()
        .filter_map(parse_issue_line)
        .collect()
}

/// Parse one `- ...` (or `* ...`) bullet into a finding.
fn parse_issue_line(line: &str) -> Option<ParsedFinding> {
    let trimmed = line.trim();
    let bullet = trimmed
        .strip_prefix("- ")
        .or_else(|| trimmed.strip_prefix("* "))?;
    let bullet = bullet.trim();
    if bullet.is_empty() {
        return None;
    }

    let (tags, body_tokens) = scan_tokens(bullet);
    let body = body_tokens.join(" ");
    let body = body.trim();
    if body.is_empty() {
        return None;
    }

    let kind_raw = tags.get("kind").map(String::as_str).unwrap_or("");
    let kind = map_kind(kind_raw).unwrap_or("craft").to_string();
    let severity = map_severity(tags.get("severity").map(String::as_str).unwrap_or(""));
    let target_executor = map_target_executor(
        tags.get("executor")
            .or_else(|| tags.get("target_executor"))
            .map(String::as_str)
            .unwrap_or(""),
        &kind,
    )
    .to_string();
    let rule_suggestion = tags
        .get("rule_suggestion")
        .or_else(|| tags.get("rule"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    Some(ParsedFinding {
        kind,
        severity: severity.to_string(),
        body: body.to_string(),
        rule_suggestion,
        target_executor,
    })
}

/// Recognized inline tag keys (case-insensitive). The value is the next
/// whitespace-delimited token after the `key:` marker.
const TAG_KEYS: &[&str] = &[
    "kind",
    "severity",
    "executor",
    "target_executor",
    "rule_suggestion",
    "rule",
];

/// Scan a bullet string token-by-token, separating inline `key: value` tags
/// from prose tokens.
///
/// Tags are recognized when a token ends with `:` (after trimming trailing
/// punctuation we already split on whitespace, so the colon is part of the
/// token) **and** the stripped token matches one of [`TAG_KEYS`]
/// (case-insensitive).
///
/// - `kind` / `severity` / `executor` / `target_executor` consume exactly
///   the next whitespace-delimited token as the value (trimming trailing
///   `,` / `.`).
/// - `rule_suggestion` / `rule` capture the **rest of the line** as the
///   value, stopping if another recognized tag key appears later on the same
///   line. This is because the suggestion is multi-word prose, not a single
///   vocabulary token.
///
/// Returns `(tags, prose_tokens)`. Token order is preserved in `prose_tokens`.
fn scan_tokens(bullet: &str) -> (HashMap<String, String>, Vec<String>) {
    let mut tags: HashMap<String, String> = HashMap::new();
    let mut prose: Vec<String> = Vec::new();

    let raw_tokens: Vec<&str> = bullet.split_whitespace().collect();
    let mut i = 0;
    while i < raw_tokens.len() {
        let tok = raw_tokens[i];
        // Recognize `key:` (optionally followed by other punctuation that
        // split_whitespace has already isolated).
        if let Some(key) = tok
            .strip_suffix(':')
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|k| TAG_KEYS.contains(&k.as_str()))
        {
            if matches!(key.as_str(), "rule_suggestion" | "rule") {
                // Capture the rest of the line as prose, stopping if another
                // recognized tag appears later.
                let mut value_parts: Vec<&str> = Vec::new();
                let mut j = i + 1;
                while j < raw_tokens.len() {
                    let next_tok = raw_tokens[j];
                    let is_next_tag = next_tok
                        .strip_suffix(':')
                        .map(|s| s.trim().to_ascii_lowercase())
                        .is_some_and(|k| TAG_KEYS.contains(&k.as_str()));
                    if is_next_tag {
                        break;
                    }
                    value_parts.push(next_tok);
                    j += 1;
                }
                let value = value_parts.join(" ");
                if !value.is_empty() {
                    tags.insert(key, value);
                }
                i = j;
                continue;
            }
            // Single-token value tag (kind/severity/executor/...).
            if let Some(value_tok) = raw_tokens.get(i + 1) {
                let value = value_tok.trim_end_matches([',', '.']).to_string();
                if !value.is_empty() {
                    tags.insert(key, value);
                    i += 2;
                    continue;
                }
            }
            // Lone `key:` with no following value — drop the marker token.
            i += 1;
            continue;
        }
        // Prose token. Trim trailing comma-only artifacts so the body reads
        // naturally (`, ` separators don't belong in prose).
        prose.push(tok.trim_end_matches(',').to_string());
        i += 1;
    }

    (tags, prose)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Vocabulary mapping ────────────────────────────────────────────────

    #[test]
    fn map_severity_translates_review_vocab_to_finding_vocab() {
        assert_eq!(map_severity("critical"), "blocker");
        assert_eq!(map_severity("Critical"), "blocker");
        assert_eq!(map_severity("  blocker  "), "blocker");
        assert_eq!(map_severity("major"), "major");
        assert_eq!(map_severity("MAJOR"), "major");
        assert_eq!(map_severity("minor"), "minor");
        assert_eq!(map_severity("info"), "info");
        // Unknown tokens fall back to the safe default.
        assert_eq!(map_severity("severe"), "info");
        assert_eq!(map_severity(""), "info");
    }

    #[test]
    fn map_kind_accepts_closed_set_and_rejects_unknown() {
        for &known in KNOWN_FINDING_KINDS {
            assert_eq!(map_kind(known), Some(known));
            assert_eq!(map_kind(&known.to_uppercase()), Some(known));
        }
        assert_eq!(map_kind("nonsense"), None);
        assert_eq!(map_kind(""), None);
    }

    #[test]
    fn map_target_executor_uses_explicit_hint_then_kind_default() {
        // Explicit known executor wins.
        assert_eq!(map_target_executor("write", "craft"), "write");
        assert_eq!(map_target_executor("brainstorm", "craft"), "brainstorm");
        assert_eq!(map_target_executor("master", "plot_hole"), "master");
        // Unknown token + craft kind → write (spec §1.2).
        assert_eq!(map_target_executor("", "craft"), "write");
        assert_eq!(map_target_executor("nonsense", "craft"), "write");
        // Unknown token + continuity → write.
        assert_eq!(map_target_executor("", "continuity"), "write");
        // Unknown token + other kind → brainstorm.
        assert_eq!(map_target_executor("", "plot_hole"), "brainstorm");
        assert_eq!(map_target_executor("", "world_inconsistency"), "brainstorm");
    }

    // ── Full-parse contract ───────────────────────────────────────────────

    #[test]
    fn parse_empty_content_is_an_error() {
        assert_eq!(parse_review_report("").unwrap_err(), ParseError::Empty);
        assert_eq!(parse_review_report("   \n\t ").unwrap_err(), ParseError::Empty);
    }

    #[test]
    fn parse_well_formed_report_yields_one_finding_per_issue_bullet() {
        let report = "\
# Review Report

## Overall Assessment
Solid chapter overall; a couple of craft issues to address.

## Issues
- POV drifts mid-chapter. kind: craft, severity: major, executor: write
- World timeline contradicts ch2. kind: world_inconsistency, severity: critical
- Plot thread introduced then dropped. kind: plot_hole, severity: minor

## Recommendations
- Re-pin POV per chapter.
";
        let parsed = parse_review_report(report).expect("well-formed report must parse");
        assert_eq!(
            parsed.overall_assessment.as_deref(),
            Some("Solid chapter overall; a couple of craft issues to address.")
        );
        assert_eq!(parsed.findings.len(), 3, "one finding per bullet");

        let f0 = &parsed.findings[0];
        assert_eq!(f0.kind, "craft");
        assert_eq!(f0.severity, "major");
        assert_eq!(f0.target_executor, "write");
        assert!(f0.body.contains("POV drifts mid-chapter"));
        assert!(f0.rule_suggestion.is_none());

        // critical → blocker mapping.
        let f1 = &parsed.findings[1];
        assert_eq!(f1.kind, "world_inconsistency");
        assert_eq!(f1.severity, "blocker");
        // No executor + world_inconsistency kind → brainstorm default.
        assert_eq!(f1.target_executor, "brainstorm");

        let f2 = &parsed.findings[2];
        assert_eq!(f2.kind, "plot_hole");
        assert_eq!(f2.severity, "minor");
    }

    #[test]
    fn parse_captures_optional_rule_suggestion_tag() {
        let report = "\
## Issues
- Inconsistent dialect. kind: craft, severity: minor, \
rule_suggestion: Pin dialect per region in AGENTS.md
";
        let parsed = parse_review_report(report).expect("must parse");
        assert_eq!(parsed.findings.len(), 1);
        let f = &parsed.findings[0];
        assert_eq!(f.kind, "craft");
        assert_eq!(
            f.rule_suggestion.as_deref(),
            Some("Pin dialect per region in AGENTS.md")
        );
        // Body is the prose minus the tag chunks.
        assert!(f.body.contains("Inconsistent dialect"));
        // Tag value must NOT leak into the body.
        assert!(!f.body.contains("Pin dialect"));
        assert!(!f.body.contains("kind:"));
    }

    #[test]
    fn parse_unknown_kind_falls_back_to_craft_per_spec_1_2() {
        let report = "\
## Issues
- Something odd. kind: novel_invention, severity: minor
";
        let parsed = parse_review_report(report).expect("must parse");
        assert_eq!(parsed.findings.len(), 1);
        let f = &parsed.findings[0];
        assert_eq!(f.kind, "craft", "unknown kind falls back to craft");
        assert_eq!(f.severity, "minor");
    }

    #[test]
    fn parse_missing_severity_falls_back_to_info() {
        let report = "\
## Issues
- Vague tension. kind: craft
";
        let parsed = parse_review_report(report).expect("must parse");
        assert_eq!(parsed.findings.len(), 1);
        assert_eq!(parsed.findings[0].severity, "info");
    }

    #[test]
    fn parse_missing_issues_section_yields_empty_findings_but_keeps_overall() {
        // No ## Issues heading → findings empty, overall_assessment preserved.
        // The caller (auto_chain wiring) falls back to the placeholder shape
        // per spec §1.3 when findings is empty.
        let report = "\
## Overall Assessment
Adequate; no actionable issues.

## Recommendations
- Ship as-is.
";
        let parsed = parse_review_report(report).expect("must parse");
        assert!(parsed.findings.is_empty());
        assert_eq!(parsed.overall_assessment.as_deref(), Some("Adequate; no actionable issues."));
    }

    #[test]
    fn parse_accepts_h3_heading_for_issues() {
        let report = "\
### Issues
- Tiny craft nit. kind: craft, severity: minor
";
        let parsed = parse_review_report(report).expect("must parse");
        assert_eq!(parsed.findings.len(), 1);
    }

    #[test]
    fn parse_skips_malformed_bullets_but_keeps_good_ones() {
        // Best-effort: the second bullet (empty prose after tag-strip) is
        // dropped; the first and third are kept.
        let report = "\
## Issues
- Real issue. kind: craft, severity: minor
- kind: craft, severity: major
- Also real. kind: plot_hole, severity: minor
";
        let parsed = parse_review_report(report).expect("must parse");
        assert_eq!(parsed.findings.len(), 2, "middle bullet (tag-only) is dropped");
        assert_eq!(parsed.findings[0].kind, "craft");
        assert_eq!(parsed.findings[1].kind, "plot_hole");
    }

    #[test]
    fn parse_star_bullets_are_accepted_alongside_dash() {
        let report = "\
## Issues
* Star bullet issue. kind: continuity, severity: minor
- Dash bullet issue. kind: craft, severity: minor
";
        let parsed = parse_review_report(report).expect("must parse");
        assert_eq!(parsed.findings.len(), 2);
        assert_eq!(parsed.findings[0].kind, "continuity");
        assert_eq!(parsed.findings[1].kind, "craft");
    }
}
