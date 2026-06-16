//! V1.48 P1 — open findings → prompt block builder.
//!
//! Implements the Consumer side of the findings quality loop
//! (`archived/knowledge/novel-findings-maturity.md` §2 Consumer; `novel-writing/workflow-profile.md`
//! §5.5.2 deferred consumer). The builder renders a bounded, prompt-safe
//! Markdown summary of open findings so the `novel-writing` preset can
//! inject it into outline and draft prompt context.
//!
//! ## Why a pure function (no DB handle here)
//!
//! The builder is intentionally decoupled from the DAO: it accepts a
//! pre-fetched `&[Finding]` slice and returns a `String`. The caller
//! (CLI `creator run stage advance --stage produce` or the auto-chain
//! enqueue path) is responsible for fetching findings via
//! [`nexus_local_db::findings::list_open_findings_for_chapter`] and for
//! passing the rendered block into [`crate::stage_gates::WorkFields`].
//! This mirrors the established `world_kb_block` pattern and keeps the
//! DB I/O at the call site that already owns a `SqlitePool`.

use nexus_local_db::findings::Finding;
use std::fmt::Write;

/// Overlay §2.2 — max number of findings listed in the block.
pub const MAX_FINDINGS: usize = 8;

/// Overlay §2.2 — max `body` chars per finding.
pub const MAX_BODY_CHARS: usize = 400;

/// Overlay §2.2 — max total block chars. The builder stops appending
/// once the rendered block would exceed this size.
pub const MAX_TOTAL_BLOCK_CHARS: usize = 3200;

/// Overlay §2.1 — map severity to a numeric rank for ordering
/// (blocker > major > minor > info). Unknown severities sort lowest.
///
/// Exposed so callers that fetch findings outside the chapter-scoped DAO
/// (e.g. the CLI, which lists via the daemon Local API) can re-sort
/// client-side without duplicating the rank ladder.
#[must_use]
pub fn severity_rank(severity: &str) -> i32 {
    match severity {
        "blocker" => 4,
        "major" => 3,
        "minor" => 2,
        _ => 1, // "info" and any unknown severity
    }
}

/// Overlay §2.1 — sort a `&mut [Finding]` in place.
///
/// Severity DESC (blocker > major > minor > info), then `created_at`
/// ASC. This is the same ordering the chapter-scoped DAO query
/// ([`nexus_local_db::findings::list_open_findings_for_chapter`])
/// applies server-side; exposed as a helper for callers that fetch via
/// paths which do not (yet) use that DAO (e.g. CLI Local API round-trip).
pub fn sort_open_findings(findings: &mut [Finding]) {
    findings.sort_by(|a, b| {
        severity_rank(&b.severity)
            .cmp(&severity_rank(&a.severity))
            .then_with(|| a.created_at.cmp(&b.created_at))
    });
}

/// V1.48 P1 (overlay §2.2) — render the `{{ open_findings_block }}`
/// template variable for `novel-writing` outline/draft prompts.
///
/// The block is a Markdown section with one bullet per finding:
///
/// ```markdown
/// ## Open findings (chapter {{ chapter_label }})
///
/// - [{{ severity }}/{{ kind }}] {{ title }}: {{ body_truncated }}
///   Suggested rule: {{ rule_suggestion_truncated }}
/// ```
///
/// ## Limits (overlay §2.2)
///
/// | Limit | Constant | Value |
/// | ----- | -------- | ----- |
/// | Max findings listed | [`MAX_FINDINGS`] | 8 |
/// | Max `body` chars per finding | [`MAX_BODY_CHARS`] | 400 |
/// | Max total block chars | [`MAX_TOTAL_BLOCK_CHARS`] | 3200 |
///
/// Findings beyond the count cap are silently dropped; oversize bodies
/// are truncated with an ellipsis. The `rule_suggestion` line is
/// omitted when the finding has no suggestion (`None` or empty).
///
/// ## Empty input
///
/// When `findings` is empty, the builder returns an empty `String`
/// (no section heading, no sentinel) so the prompt template's
/// `{{#if open_findings_block}}` guard omits the section entirely
/// (overlay §2.2 / AC2: no empty sentinel noise).
///
/// `chapter_label` is interpolated into the section heading per
/// overlay §2.2. Pass an empty string if the caller cannot supply one;
/// the heading will read "Open findings (chapter )".
#[must_use]
pub fn build_open_findings_block(findings: &[Finding], chapter_label: &str) -> String {
    if findings.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    let _ = write!(out, "## Open findings (chapter {chapter_label})\n\n");

    for (idx, f) in findings.iter().enumerate() {
        if idx >= MAX_FINDINGS {
            break;
        }

        let body = truncate_chars(&f.description, MAX_BODY_CHARS);
        let line = match f.rule_suggestion.as_deref() {
            Some(s) if !s.trim().is_empty() => {
                let rule = truncate_chars(s, MAX_BODY_CHARS);
                format!(
                    "- [{sev}/{kind}] {title}: {body}\n  Suggested rule: {rule}\n",
                    sev = f.severity,
                    kind = f.kind,
                    title = f.title,
                )
            }
            _ => format!(
                "- [{sev}/{kind}] {title}: {body}\n",
                sev = f.severity,
                kind = f.kind,
                title = f.title,
            ),
        };

        // Stop appending once the total block would exceed the cap.
        // We never want to half-render a finding line, so the check is
        // "would adding this line overflow?" rather than "trim to cap".
        if out.len() + line.len() > MAX_TOTAL_BLOCK_CHARS {
            break;
        }
        out.push_str(&line);
    }

    // If after applying the total-char cap nothing got appended past the
    // heading, return empty so the template guard omits the whole block
    // (avoids rendering a heading with no body).
    let heading = format!("## Open findings (chapter {chapter_label})\n\n");
    if out.len() == heading.len() {
        return String::new();
    }

    out
}

/// Truncate `s` to at most `max_chars` Unicode scalar values, appending
/// an ellipsis (`…`) when truncation occurred. Returns the (possibly
/// truncated) string.
fn truncate_chars(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let truncated: String = s.chars().take(max_chars).collect();
    format!("{truncated}…")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_finding(severity: &str, kind: &str, title: &str, body: &str) -> Finding {
        Finding {
            finding_id: format!("fnd_test_{}", title.len()),
            work_id: "wrk_test".to_string(),
            chapter: Some(1),
            severity: severity.to_string(),
            status: "open".to_string(),
            title: title.to_string(),
            description: body.to_string(),
            target_executor: "write".to_string(),
            creator_id: "ctr_test".to_string(),
            kind: kind.to_string(),
            rule_suggestion: None,
            created_at: 0,
            updated_at: 0,
        }
    }

    /// AC2 (overlay §2.2): no findings → empty string, no sentinel noise.
    #[test]
    fn findings_block_builder_returns_empty_when_no_findings() {
        let block = build_open_findings_block(&[], "01");
        assert!(block.is_empty(), "empty findings must yield empty block");
    }

    /// Overlay §2.2 count cap: 20 findings → exactly MAX_FINDINGS bullets.
    #[test]
    fn findings_block_builder_respects_token_cap_count() {
        let findings: Vec<Finding> = (0..20)
            .map(|i| make_finding("minor", "craft", &format!("f{i}"), "body"))
            .collect();
        let block = build_open_findings_block(&findings, "01");
        let bullet_count = block.lines().filter(|l| l.starts_with("- [")).count();
        assert_eq!(
            bullet_count, MAX_FINDINGS,
            "expected exactly {MAX_FINDINGS} bullets when 20 findings seeded"
        );
    }

    /// Plan T4 alias — combined token-cap coverage: count cap (20 → ≤8
    /// bullets) AND body truncation (oversize bodies get an ellipsis).
    /// With oversize bodies the total-block cap may also bind (further
    /// reducing the bullet count below MAX_FINDINGS); the assertion
    /// accepts either cap binding as long as both invariants hold.
    #[test]
    fn findings_block_builder_respects_token_cap() {
        let long_body = "z".repeat(MAX_BODY_CHARS * 3);
        let findings: Vec<Finding> = (0..20)
            .map(|i| make_finding("minor", "craft", &format!("f{i}"), &long_body))
            .collect();
        let block = build_open_findings_block(&findings, "01");

        // Count cap (≤ MAX_FINDINGS) — total-char cap may further reduce.
        let bullet_count = block.lines().filter(|l| l.starts_with("- [")).count();
        assert!(
            bullet_count <= MAX_FINDINGS,
            "expected at most {MAX_FINDINGS} bullets when 20 findings seeded; got {bullet_count}"
        );

        // Body truncation — every emitted bullet should carry an ellipsis.
        let bullets: Vec<&str> = block.lines().filter(|l| l.starts_with("- [")).collect();
        for b in &bullets {
            assert!(
                b.contains('…'),
                "oversize body should be truncated with ellipsis; got: {b}"
            );
        }

        // Total-char cap invariant.
        assert!(
            block.len() <= MAX_TOTAL_BLOCK_CHARS,
            "block must not exceed MAX_TOTAL_BLOCK_CHARS ({MAX_TOTAL_BLOCK_CHARS}); got {}",
            block.len()
        );
    }

    /// Overlay §2.2 body cap: a body over `MAX_BODY_CHARS` is truncated
    /// with ellipsis.
    #[test]
    fn findings_block_builder_truncates_oversize_body() {
        let long_body = "x".repeat(MAX_BODY_CHARS * 4);
        let findings = vec![make_finding("major", "craft", "t1", &long_body)];
        let block = build_open_findings_block(&findings, "01");
        assert!(block.contains('…'), "expected ellipsis on truncated body");
        // The body line itself should be no longer than MAX_BODY_CHARS + ellipsis.
        let body_line = block
            .lines()
            .find(|l| l.starts_with("- ["))
            .expect("bullet line should exist");
        let body_part = body_line.split(": ").nth(1).unwrap_or_default();
        // body_part may include `\n  Suggested rule: ...` if suggestion present,
        // but we set rule_suggestion=None above, so it's just the truncated body.
        assert!(
            body_part.chars().count() <= MAX_BODY_CHARS + 1, // +1 for ellipsis
            "body should be truncated to {} chars + ellipsis; got {} chars",
            MAX_BODY_CHARS,
            body_part.chars().count()
        );
    }

    /// Overlay §2.2 total cap: a block that would exceed
    /// `MAX_TOTAL_BLOCK_CHARS` stops appending before the overflow.
    #[test]
    fn findings_block_builder_respects_total_block_cap() {
        // 8 findings × 600-char body each → exceeds MAX_TOTAL_BLOCK_CHARS.
        let long_body = "y".repeat(600);
        let findings: Vec<Finding> = (0..MAX_FINDINGS)
            .map(|i| make_finding("major", "craft", &format!("t{i}"), &long_body))
            .collect();
        let block = build_open_findings_block(&findings, "01");
        assert!(
            block.len() <= MAX_TOTAL_BLOCK_CHARS,
            "block must not exceed MAX_TOTAL_BLOCK_CHARS ({MAX_TOTAL_BLOCK_CHARS}); got {}",
            block.len()
        );
    }

    /// Overlay §2.2 — `rule_suggestion` line present when set, omitted when None.
    #[test]
    fn findings_block_builder_emits_rule_suggestion_when_present() {
        let mut f = make_finding("minor", "craft", "with-rule", "body text");
        f.rule_suggestion = Some("Prefer active voice.".to_string());
        let block = build_open_findings_block(&[f], "01");
        assert!(
            block.contains("Suggested rule: Prefer active voice."),
            "expected rule suggestion line in block:\n{block}"
        );
    }

    /// Section heading interpolates `chapter_label` per overlay §2.2.
    #[test]
    fn findings_block_builder_heading_carries_chapter_label() {
        let findings = vec![make_finding("info", "craft", "h1", "body")];
        let block = build_open_findings_block(&findings, "07");
        assert!(
            block.contains("## Open findings (chapter 07)"),
            "heading should carry the chapter label; got:\n{block}"
        );
    }
}
