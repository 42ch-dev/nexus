//! V1.48 P2 — Layer 2 `AGENTS.md` runtime helpers.
//!
//! Pure functions for the three Layer-2 operations introduced by the V1.48
//! P2 rules-runtime plan (`2026-06-16-v1.48-rules-runtime`):
//!
//! - [`append_rule_suggestion`] — idempotent append of an accepted
//!   `rule_suggestion` to `Works/<work_ref>/AGENTS.md` (T3 core).
//! - [`render_default_agents_md`] — render the embedded scaffold template
//!   for a Work (used by T2 novel-project-init and T4 `rules reset`).
//! - [`reset_agents_md`] — overwrite `Works/<work_ref>/AGENTS.md` with the
//!   default scaffold (T4 core).
//!
//! The Layer 2 *read* path lives in
//! [`crate::stage_gates::read_rules_layers`](../stage_gates/fn.read_rules_layers.html).
//!
//! Spec: [archived/knowledge/novel-findings-maturity.md §3 / §4](../../../.mstar/archived/knowledge/novel-findings-maturity.md),
//! [novel-writing/workflow-profile.md §5.5.4](../../../.mstar/knowledge/specs/novel-writing/workflow-profile.md).

use std::io::Write;
use std::path::Path;

/// Embedded default scaffold for `Works/<work_ref>/AGENTS.md`.
///
/// Compiled into the binary via `include_str!`. The `{{work_ref}}`
/// placeholder is rendered by [`render_default_agents_md`].
pub const DEFAULT_AGENTS_MD_SCAFFOLD: &str =
    include_str!("../embedded-rules/work-agents-scaffold.md");

/// Marker line used to locate the `## Accepted rule suggestions` section
/// when appending. Kept in sync with [`DEFAULT_AGENTS_MD_SCAFFOLD`].
const ACCEPTED_SECTION_HEADER: &str = "## Accepted rule suggestions";

/// Outcome of [`append_rule_suggestion`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppendOutcome {
    /// A new entry was appended to the file.
    Appended,
    /// The `finding_id` was already present in the file; the file was not
    /// modified (idempotent).
    AlreadyPresent,
}

/// Render the default `AGENTS.md` scaffold for a Work.
///
/// Replaces the `{{work_ref}}` placeholder in the embedded template.
#[must_use]
pub fn render_default_agents_md(work_ref: &str) -> String {
    DEFAULT_AGENTS_MD_SCAFFOLD.replace("{{work_ref}}", work_ref)
}

/// Render a unified diff previewing what [`reset_agents_md`] would change.
///
/// Returns an empty [`String`] when `current` already equals the default
/// scaffold for `work_ref` (i.e. reset would be a no-op). The diff uses the
/// standard unified-diff format:
///
/// - `--- AGENTS.md (current)` / `+++ AGENTS.md (default scaffold)` headers.
/// - `@@ -a,b +c,d @@` hunk headers with 3 context lines (`diff -u` default).
/// - ` ` context, `-` removed (discarded by reset), `+` added (after reset).
///
/// Used by the `--dry-run` flag of `creator works rules reset`. Pure and
/// hermetically testable; performs no filesystem I/O.
#[must_use]
pub fn diff_agents_md_vs_scaffold(current: &str, work_ref: &str) -> String {
    let target = render_default_agents_md(work_ref);
    unified_diff(current, &target)
}

/// Context lines shown around each change (matches `diff -u` default).
const DIFF_CONTEXT: usize = 3;
/// Edit-op tags for [`build_edit_script`].
const DIFF_EQUAL: u8 = 0;
const DIFF_DELETE: u8 = 1;
const DIFF_INSERT: u8 = 2;

/// Standard line-level unified diff between `from` (`---`) and `to` (`+++`).
///
/// Returns an empty [`String`] when the inputs are line-wise identical.
/// Dependency-free: a single CLI preview path does not justify pulling in a
/// diff library.
fn unified_diff(from: &str, to: &str) -> String {
    let from_lines: Vec<&str> = from.lines().collect();
    let to_lines: Vec<&str> = to.lines().collect();
    if from_lines == to_lines {
        return String::new();
    }
    let script = build_edit_script(&from_lines, &to_lines);
    emit_hunks(&script)
}

/// Build the LCS edit script between two line slices as `(tag, line)` tuples.
///
/// Tags: [`DIFF_EQUAL`] (context), [`DIFF_DELETE`] (in `from_lines`),
/// [`DIFF_INSERT`] (in `to_lines`).
fn build_edit_script<'a>(from_lines: &'a [&'a str], to_lines: &'a [&'a str]) -> Vec<(u8, &'a str)> {
    let (from_len, to_len) = (from_lines.len(), to_lines.len());
    // LCS length table: `dp[i][j]` = LCS length of `from_lines[i..]` / `to_lines[j..]`.
    let mut dp = vec![vec![0u32; to_len + 1]; from_len + 1];
    for i in (0..from_len).rev() {
        for j in (0..to_len).rev() {
            dp[i][j] = if from_lines[i] == to_lines[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }

    let mut script: Vec<(u8, &str)> = Vec::with_capacity(from_len + to_len);
    let (mut i, mut j) = (0, 0);
    while i < from_len && j < to_len {
        if from_lines[i] == to_lines[j] {
            script.push((DIFF_EQUAL, from_lines[i]));
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            script.push((DIFF_DELETE, from_lines[i]));
            i += 1;
        } else {
            script.push((DIFF_INSERT, to_lines[j]));
            j += 1;
        }
    }
    while i < from_len {
        script.push((DIFF_DELETE, from_lines[i]));
        i += 1;
    }
    while j < to_len {
        script.push((DIFF_INSERT, to_lines[j]));
        j += 1;
    }
    script
}

/// Group the edit `script` into unified-diff hunks and emit it with
/// `---` / `+++` headers.
fn emit_hunks(script: &[(u8, &str)]) -> String {
    let len = script.len();
    // Prefix counts of from/to lines consumed up to each index → hunk headers.
    let mut a_prefix = vec![0usize; len + 1];
    let mut b_prefix = vec![0usize; len + 1];
    for (idx, &(tag, _)) in script.iter().enumerate() {
        a_prefix[idx + 1] = a_prefix[idx] + usize::from(tag != DIFF_INSERT);
        b_prefix[idx + 1] = b_prefix[idx] + usize::from(tag != DIFF_DELETE);
    }

    // Dilate each change by `DIFF_CONTEXT` and merge overlapping runs → hunks.
    let mut in_hunk = vec![false; len];
    for (idx, &(tag, _)) in script.iter().enumerate() {
        if tag != DIFF_EQUAL {
            let lo = idx.saturating_sub(DIFF_CONTEXT);
            let hi = (idx + DIFF_CONTEXT).min(len.saturating_sub(1));
            in_hunk[lo..=hi].fill(true);
        }
    }

    let mut out = String::new();
    out.push_str("--- AGENTS.md (current)\n+++ AGENTS.md (default scaffold)\n");

    let mut idx = 0;
    while idx < len {
        if !in_hunk[idx] {
            idx += 1;
            continue;
        }
        let hunk_start = idx;
        while idx < len && in_hunk[idx] {
            idx += 1;
        }
        let hunk_end_excl = idx;

        let a_count = a_prefix[hunk_end_excl] - a_prefix[hunk_start];
        let b_count = b_prefix[hunk_end_excl] - b_prefix[hunk_start];
        // 1-based start; when a side contributes 0 lines, anchor at the line
        // before the hunk per diff convention.
        let a_start = if a_count == 0 {
            a_prefix[hunk_start]
        } else {
            a_prefix[hunk_start] + 1
        };
        let b_start = if b_count == 0 {
            b_prefix[hunk_start]
        } else {
            b_prefix[hunk_start] + 1
        };
        let header = format!(
            "@@ -{} +{} @@\n",
            hunk_range(a_start, a_count),
            hunk_range(b_start, b_count)
        );
        out.push_str(&header);
        for &(tag, line) in &script[hunk_start..hunk_end_excl] {
            let prefix = match tag {
                DIFF_EQUAL => ' ',
                DIFF_DELETE => '-',
                _ => '+',
            };
            out.push(prefix);
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// Format a hunk range as `start` (when `count == 1`) or `start,count`,
/// matching the `diff -u` / `git diff` convention.
fn hunk_range(start: usize, count: usize) -> String {
    if count == 1 {
        format!("{start}")
    } else {
        format!("{start},{count}")
    }
}

/// Append an accepted `rule_suggestion` to `Works/<work_ref>/AGENTS.md`.
///
/// The entry is written under the `## Accepted rule suggestions` section.
/// The append is **idempotent on `finding_id`**: if the file already
/// contains a line with the marker `<!-- finding_id: <finding_id> -->`,
/// the file is left unchanged and [`AppendOutcome::AlreadyPresent`] is
/// returned.
///
/// If the file does not exist, it is created from
/// [`render_default_agents_md`] with `work_ref` before appending.
///
/// # Arguments
///
/// * `agents_md_path` — absolute path to `Works/<work_ref>/AGENTS.md`.
/// * `work_ref` — Work ref used to seed the scaffold when the file is
///   missing. May be empty when the file is known to exist; passing the
///   correct value is strongly recommended so a missing file is seeded
///   with the right header.
/// * `finding_id` — stable finding identifier (ULID). Used as the
///   idempotency key.
/// * `rule_text` — the accepted `rule_suggestion` body. Must be non-empty
///   after trim (callers validate before calling).
/// * `timestamp_rfc3339` — ISO 8601 timestamp for the audit header.
///
/// # Errors
///
/// Returns `std::io::Error` on filesystem read/write failure.
///
/// # Idempotency
///
/// Repeated calls with the same `finding_id` do not duplicate the entry.
/// The marker comment `<!-- finding_id: <id> -->` is the dedup key.
pub fn append_rule_suggestion(
    agents_md_path: &Path,
    work_ref: &str,
    finding_id: &str,
    rule_text: &str,
    timestamp_rfc3339: &str,
) -> std::io::Result<AppendOutcome> {
    // Ensure the file exists; seed from scaffold if missing.
    if !agents_md_path.exists() {
        if let Some(parent) = agents_md_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let seeded = render_default_agents_md(work_ref);
        std::fs::write(agents_md_path, seeded)?;
    }

    let existing = std::fs::read_to_string(agents_md_path)?;
    let marker = format!("<!-- finding_id: {finding_id} -->");
    if existing.contains(&marker) {
        return Ok(AppendOutcome::AlreadyPresent);
    }

    let entry = format_accepted_entry(finding_id, rule_text, timestamp_rfc3339);
    let updated = ensure_accepted_section(&existing);
    let updated = format!("{updated}\n{entry}\n");

    // Atomic write via temp + rename (mirrors rules_history.rs pattern).
    let tmp_path = agents_md_path.with_extension("md.tmp");
    {
        let mut f = std::fs::File::create(&tmp_path)?;
        f.write_all(updated.as_bytes())?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp_path, agents_md_path)?;
    Ok(AppendOutcome::Appended)
}

/// Reset `Works/<work_ref>/AGENTS.md` to the default scaffold.
///
/// Overwrites the file with [`render_default_agents_md`]. The Work's
/// chapter artifacts and other files are NOT touched (spec §4). If the
/// file is missing, it is created.
///
/// # Errors
///
/// Returns `std::io::Error` on filesystem write failure.
pub fn reset_agents_md(agents_md_path: &Path, work_ref: &str) -> std::io::Result<()> {
    if let Some(parent) = agents_md_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let scaffold = render_default_agents_md(work_ref);
    // Atomic write via temp + rename.
    let tmp_path = agents_md_path.with_extension("md.tmp");
    {
        let mut f = std::fs::File::create(&tmp_path)?;
        f.write_all(scaffold.as_bytes())?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp_path, agents_md_path)?;
    Ok(())
}

/// Build the markdown entry for one accepted rule suggestion.
fn format_accepted_entry(finding_id: &str, rule_text: &str, timestamp_rfc3339: &str) -> String {
    // Trim and normalize newlines so the entry renders as a tight block.
    let body = rule_text.trim();
    format!(
        "<!-- finding_id: {finding_id} -->\n**Accepted {timestamp_rfc3339}** (finding `{finding_id}`)\n\n{body}\n",
    )
}

/// Ensure the `## Accepted rule suggestions` section exists in `content`.
///
/// Returns the content with the section header appended if it was missing.
fn ensure_accepted_section(content: &str) -> String {
    if content.contains(ACCEPTED_SECTION_HEADER) {
        content.to_string()
    } else {
        format!(
            "{content}\n\n{ACCEPTED_SECTION_HEADER}\n\n<!-- Appended by `creator works findings accept <finding_id>`. -->\n",
        )
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_agents_md(work_ref: &str) -> (tempfile::TempDir, std::path::PathBuf) {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let path = tmp.path().join("AGENTS.md");
        // Seed with default scaffold so the section exists.
        std::fs::write(&path, render_default_agents_md(work_ref)).expect("seed");
        (tmp, path)
    }

    #[test]
    fn rules_layers_render_default_replaces_work_ref() {
        let rendered = render_default_agents_md("neon-river");
        assert!(rendered.contains("# AGENTS.md — neon-river"));
        assert!(rendered.contains("## Accepted rule suggestions"));
        // Template placeholder must be fully replaced.
        assert!(!rendered.contains("{{work_ref}}"));
    }

    #[test]
    fn rules_layers_append_creates_entry_under_section() {
        let (_tmp, path) = tmp_agents_md("neon-river");
        let out = append_rule_suggestion(
            &path,
            "neon-river",
            "fnd_01HMV",
            "Prefer first-person POV in dialogue scenes.",
            "2026-06-16T12:00:00Z",
        )
        .expect("append");

        assert_eq!(out, AppendOutcome::Appended);
        let content = std::fs::read_to_string(&path).expect("read");
        assert!(content.contains("<!-- finding_id: fnd_01HMV -->"));
        assert!(content.contains("**Accepted 2026-06-16T12:00:00Z**"));
        assert!(content.contains("Prefer first-person POV in dialogue scenes."));
    }

    #[test]
    fn rules_layers_append_is_idempotent_on_finding_id() {
        let (_tmp, path) = tmp_agents_md("neon-river");
        let fid = "fnd_dup";
        append_rule_suggestion(
            &path,
            "neon-river",
            fid,
            "Avoid passive voice.",
            "2026-06-16T12:00:00Z",
        )
        .expect("first append");

        let out2 = append_rule_suggestion(
            &path,
            "neon-river",
            fid,
            "Avoid passive voice.",
            "2026-06-16T12:00:01Z",
        )
        .expect("second append");

        assert_eq!(out2, AppendOutcome::AlreadyPresent);
        let content = std::fs::read_to_string(&path).expect("read");
        // The marker appears exactly once.
        assert_eq!(
            content
                .matches(&format!("<!-- finding_id: {fid} -->"))
                .count(),
            1,
            "idempotency: finding marker should appear exactly once"
        );
    }

    #[test]
    fn rules_layers_append_seeds_missing_file_from_scaffold() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let path = tmp.path().join("nested").join("AGENTS.md");
        let out = append_rule_suggestion(
            &path,
            "ghost-novel",
            "fnd_seed",
            "Keep chapters under 4000 words.",
            "2026-06-16T12:00:00Z",
        )
        .expect("append");

        assert_eq!(out, AppendOutcome::Appended);
        let content = std::fs::read_to_string(&path).expect("read");
        // Seeded header carries the work_ref.
        assert!(content.contains("# AGENTS.md — ghost-novel"));
        assert!(content.contains("## Accepted rule suggestions"));
        assert!(content.contains("<!-- finding_id: fnd_seed -->"));
    }

    #[test]
    fn rules_layers_append_adds_section_when_missing() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let path = tmp.path().join("AGENTS.md");
        // Pre-existing file without the accepted section (e.g. legacy file).
        std::fs::write(&path, "# AGENTS.md — legacy\n\n- POV: first\n").expect("write");

        let out = append_rule_suggestion(
            &path,
            "legacy",
            "fnd_legacy",
            "Ban adverbs.",
            "2026-06-16T12:00:00Z",
        )
        .expect("append");

        assert_eq!(out, AppendOutcome::Appended);
        let content = std::fs::read_to_string(&path).expect("read");
        assert!(content.contains("## Accepted rule suggestions"));
        assert!(content.contains("<!-- finding_id: fnd_legacy -->"));
        // Original content preserved.
        assert!(content.contains("POV: first"));
    }

    #[test]
    fn rules_layers_reset_restores_default_scaffold() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let path = tmp.path().join("AGENTS.md");
        // File with user edits + accepted entries.
        std::fs::write(
            &path,
            "# AGENTS.md — neon-river\n\n- POV: first\n\n## Accepted rule suggestions\n\n<!-- finding_id: fnd_old -->\nold\n",
        )
        .expect("write");

        reset_agents_md(&path, "neon-river").expect("reset");

        let content = std::fs::read_to_string(&path).expect("read");
        assert!(content.contains("# AGENTS.md — neon-river"));
        assert!(content.contains("## Style Preferences"));
        assert!(content.contains("## Accepted rule suggestions"));
        assert!(
            !content.contains("fnd_old"),
            "reset must clear prior entries"
        );
        assert!(
            !content.contains("POV: first"),
            "reset must clear user edits"
        );
    }

    #[test]
    fn rules_layers_reset_creates_missing_file() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let path = tmp.path().join("Works").join("neon").join("AGENTS.md");
        reset_agents_md(&path, "neon").expect("reset");
        assert!(path.is_file());
        let content = std::fs::read_to_string(&path).expect("read");
        assert!(content.contains("# AGENTS.md — neon"));
    }

    // ── V1.48 P2-fix1: diff_agents_md_vs_scaffold ──────────────────────

    #[test]
    fn rules_layers_diff_empty_when_current_equals_scaffold() {
        let scaffold = render_default_agents_md("neon-river");
        let diff = diff_agents_md_vs_scaffold(&scaffold, "neon-river");
        assert!(
            diff.is_empty(),
            "no diff expected when current == scaffold; got:\n{diff}"
        );
    }

    #[test]
    fn rules_layers_diff_marks_accepted_entries_as_removed() {
        // Current file = scaffold + one accepted entry (what `findings accept` produced).
        let mut current = render_default_agents_md("neon-river");
        current.push_str("\n<!-- finding_id: fnd_01HMV -->\n");
        current.push_str("**Accepted 2026-06-16T12:00:00Z** (finding `fnd_01HMV`)\n");
        current.push_str("\nPrefer first-person POV in dialogue scenes.\n");

        let diff = diff_agents_md_vs_scaffold(&current, "neon-river");

        // The accepted entry lines are in `current` but NOT the scaffold, so
        // reset would discard them → they must appear as `-` (removed) lines.
        assert!(
            diff.contains("-<!-- finding_id: fnd_01HMV -->"),
            "diff:\n{diff}"
        );
        assert!(
            diff.contains("-Prefer first-person POV in dialogue scenes."),
            "diff:\n{diff}"
        );
        // The scaffold lines are unchanged context, not added.
        assert!(
            !diff.contains("+Prefer first-person POV"),
            "added (+) lines should not include current-only content"
        );
    }

    #[test]
    fn rules_layers_diff_has_unified_format_headers() {
        // A file missing some scaffold lines → those lines appear as `+`.
        let current = "# AGENTS.md — neon-river\n\n- POV: first\n";
        let diff = diff_agents_md_vs_scaffold(current, "neon-river");

        assert!(diff.starts_with("--- AGENTS.md (current)\n+++ AGENTS.md (default scaffold)\n"));
        assert!(
            diff.contains("@@ -"),
            "expected at least one @@ hunk header:\n{diff}"
        );
        // Scaffold-only content (e.g. the accepted-section header) is added.
        assert!(
            diff.contains("+## Accepted rule suggestions") || diff.contains("+## Style"),
            "expected an added scaffold section header; diff:\n{diff}"
        );
    }
}
