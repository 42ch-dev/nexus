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
//! Spec: [novel-findings-maturity.md §3 / §4](../../../.mstar/knowledge/specs/novel-findings-maturity.md),
//! [novel-workflow-profile.md §5.5.4](../../../.mstar/knowledge/specs/novel-workflow-profile.md).

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
        "{marker}\n**Accepted {ts}** (finding `{fid}`)\n\n{body}\n",
        marker = format!("<!-- finding_id: {finding_id} -->"),
        ts = timestamp_rfc3339,
        fid = finding_id,
        body = body,
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
            "{content}\n\n{header}\n\n<!-- Appended by `creator works findings accept <finding_id>`. -->\n",
            header = ACCEPTED_SECTION_HEADER,
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
            content.matches(&format!("<!-- finding_id: {fid} -->")).count(),
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
        assert!(!content.contains("fnd_old"), "reset must clear prior entries");
        assert!(!content.contains("POV: first"), "reset must clear user edits");
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
}
