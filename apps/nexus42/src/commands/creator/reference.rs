//! Reference source management subcommands.
//!
//! CLI surface for the retired V1.26 reference store (`SQLite` registry +
//! body.md on disk). The surviving read leaves go through the `nexus-core`
//! reference authority (`list_references`, `get_reference`), which owns the
//! registry reads and the schema-owned response envelopes, so `reference show`
//! renders the same fields over either transport. There is no loopback HTTP
//! involved.
//!
//! There is deliberately no `reference register` leaf: writing a
//! `reference_sources` row and its `body.md` is retired with the store. There is
//! also no `reference refresh` leaf — the V1.58 P3 entrance only dispatched the
//! retired `nexus.reference.refresh` host capability, and that capability and
//! its execution schedule are gone with the write path.
//!
//! The direct-core open admits the selection at the seam
//! ([`crate::core::open_direct_core`]) first: the writer pool migrates — and
//! therefore creates — the selected workspace, so a selection that names no
//! materialized workspace is refused instead of having one created for it.

use crate::config::CliConfig;
use crate::core::{finish_direct, map_core_error, open_direct_core};
use crate::errors::{CliError, Result};
use clap::Subcommand;
use nexus_core::{CoreError, CoreService, Principal};
use std::fmt::Write as _;

/// Reference source subcommands.
#[derive(Debug, Subcommand)]
pub enum ReferenceCommand {
    /// List registered references (metadata only, no body)
    List,

    /// Show a single reference including body path/content hint
    Show {
        /// Reference source ID (e.g. `ref_abc123`)
        reference_id: String,
    },
}

/// Run a reference command.
///
/// # Errors
///
/// Returns `CliError` if the active creator is not set, the database is
/// unavailable, or the underlying core operation fails.
pub async fn run(cmd: ReferenceCommand, config: &CliConfig) -> Result<()> {
    let core = open_direct_core(config).await?;
    let outcome = async {
        let principal = core.active_principal().await.map_err(map_core_error)?;
        match cmd {
            ReferenceCommand::List => run_list(&core, &principal).await,
            ReferenceCommand::Show { reference_id } => {
                run_show(&core, &principal, &reference_id).await
            }
        }
    }
    .await;
    if let Some(text) = finish_direct(&core, outcome).await? {
        println!("{text}");
    }
    Ok(())
}

/// `reference list` — show metadata for all references.
async fn run_list(core: &CoreService, principal: &Principal) -> Result<Option<String>> {
    let response = core
        .list_references(principal)
        .await
        .map_err(map_core_error)?;

    if response.references.is_empty() {
        return Ok(Some("No registered references.".to_string()));
    }

    let mut out = String::new();
    let _ = writeln!(
        out,
        "{:<40} {:<10} {:<12} {:<40} CREATED_AT",
        "ID", "TYPE", "MUTABILITY", "TITLE"
    );
    for row in &response.references {
        let _ = writeln!(
            out,
            "{:<40} {:<10} {:<12} {:<40} {}",
            row.reference_source_id,
            row.source_type,
            row.source_mutability,
            truncate(&row.title, 40),
            row.created_at
        );
    }

    Ok(Some(finish_render(out)))
}

/// `reference show` — display a single reference with details.
async fn run_show(
    core: &CoreService,
    principal: &Principal,
    reference_id: &str,
) -> Result<Option<String>> {
    let response = core
        .get_reference(principal, reference_id.to_string())
        .await
        // The leaf keeps its own not-found wording: the core resource string is
        // the registry key, and this surface named the reference verbatim.
        .map_err(|err| match err {
            CoreError::NotFound { .. } => {
                CliError::Other(format!("Reference {reference_id} not found."))
            }
            other => map_core_error(other),
        })?;

    let reference = &response.reference;
    let mut out = String::new();
    let _ = writeln!(out, "Reference: {}", reference.reference_source_id);
    let _ = writeln!(out, "  Title:        {}", reference.title);
    let _ = writeln!(out, "  Type:         {}", reference.source_type);
    let _ = writeln!(out, "  Mutability:   {}", reference.source_mutability);
    let _ = writeln!(out, "  URI:          {}", reference.uri);
    let _ = writeln!(out, "  Workspace:    {}", response.workspace_id);
    let _ = writeln!(out, "  Scan Status:  {}", reference.scan_status);
    let _ = writeln!(out, "  Created:      {}", reference.created_at);
    if let Some(updated) = &response.updated_at {
        let _ = writeln!(out, "  Updated:      {updated}");
    }
    if let Some(tags) = &response.tags {
        let _ = writeln!(out, "  Tags:         {tags}");
    }
    if let Some(hash) = &response.content_hash {
        let _ = writeln!(out, "  Content Hash: {hash}");
    }
    if let Some(cp) = &reference.content_path {
        let _ = writeln!(out, "  Body Path:    {cp}");
    }

    Ok(Some(finish_render(out)))
}

/// Finish a render buffer.
///
/// Every buffer above is written with `writeln!`, so it ends with the line
/// delimiter the last line appended. Only that one delimiter byte is removed
/// (the caller's `println!` restores it) — field content is never trimmed, so a
/// stored value with trailing whitespace keeps every byte.
fn finish_render(mut out: String) -> String {
    if out.ends_with('\n') {
        let _ = out.pop();
    }
    out
}

/// Truncate a string to `max_len` chars with ellipsis if needed.
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let mut end = max_len.saturating_sub(1);
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}…", &s[..end])
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn finish_render_drops_only_the_appended_delimiter() {
        assert_eq!(
            finish_render("  Body Path:    body.md  \n".to_string()),
            "  Body Path:    body.md  "
        );
        assert_eq!(finish_render("line".to_string()), "line");
    }

    #[test]
    fn truncate_short_string_unchanged() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn truncate_long_string() {
        let result = truncate("abcdefghij", 5);
        assert_eq!(result, "abcd…");
    }
}
