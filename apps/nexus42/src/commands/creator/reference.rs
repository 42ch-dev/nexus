//! Reference source management subcommands.
//!
//! CLI surface for the V1.26 reference store (`SQLite` registry + body.md on
//! disk). Every leaf goes through the `nexus-core` reference authority
//! ([`CoreService::register_reference`], `list_references`, `get_reference`),
//! which owns the registry writes/reads and the schema-owned response
//! envelopes, so `reference show` renders the same fields over either
//! transport. There is no loopback HTTP involved.
//!
//! There is deliberately no `reference refresh` leaf: the retired V1.58 P3
//! entrance only dispatched `nexus.reference.refresh` through the daemon
//! host-call endpoint and had no complete direct CLI operation. The library
//! refresh helpers (the reference capability and the execution refresh
//! schedule) remain the real owners.
//!
//! The direct-core open admits the selection at the seam
//! ([`crate::core::open_direct_core`]) first: the writer pool migrates — and
//! therefore creates — the selected workspace, so a selection that names no
//! materialized workspace is refused instead of having one created for it.

use crate::config::CliConfig;
use crate::core::{finish_direct, map_core_error, open_direct_core};
use crate::errors::{CliError, Result};
use clap::Subcommand;
use nexus_core::{CoreError, CoreService, Principal, RegisterReferenceParams};
use std::fmt::Write as _;
use std::path::PathBuf;

/// Reference source subcommands.
#[derive(Debug, Subcommand)]
pub enum ReferenceCommand {
    /// Register a new reference source
    Register {
        /// Path or URI of the source material
        #[arg(long)]
        source: String,

        /// Source type: `file`, `url`, `pdf`, or `note`
        #[arg(long, default_value = "note")]
        source_type: String,

        /// Human-readable title
        #[arg(long)]
        title: String,

        /// Tags (comma-separated)
        #[arg(long)]
        tags: Option<String>,

        /// Mutability policy: `static` (default) or `refreshable`
        #[arg(long, default_value = "static")]
        mutability: String,

        /// Body text file path (reads from the file; use `-` for stdin)
        #[arg(long)]
        file: Option<PathBuf>,

        /// Inline body text (mutually exclusive with `--file`)
        #[arg(long)]
        body: Option<String>,
    },

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
            ReferenceCommand::Register {
                source,
                source_type,
                title,
                tags,
                mutability,
                file,
                body,
            } => {
                run_register(
                    &core,
                    &principal,
                    &RegisterInput {
                        source,
                        source_type,
                        title,
                        tags,
                        mutability,
                        file,
                        body,
                    },
                )
                .await
            }
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

/// Collected input for the register command.
struct RegisterInput {
    source: String,
    source_type: String,
    title: String,
    tags: Option<String>,
    mutability: String,
    file: Option<PathBuf>,
    body: Option<String>,
}

/// `reference register` — create the registry row + body.md through the core.
async fn run_register(
    core: &CoreService,
    principal: &Principal,
    input: &RegisterInput,
) -> Result<Option<String>> {
    // Resolve body text
    let body_text = resolve_body_text(input.file.as_ref(), input.body.as_ref())?;

    // Resolve mutability
    let source_mutability = match input.mutability.as_str() {
        "static" => nexus_local_db::SourceMutability::Static,
        "refreshable" => nexus_local_db::SourceMutability::Refreshable,
        other => {
            return Err(CliError::Other(format!(
                "Invalid mutability {other:?}. Use 'static' or 'refreshable'."
            )));
        }
    };

    // Validate source_type
    validate_source_type(&input.source_type)?;

    let reference = core
        .register_reference(
            principal,
            RegisterReferenceParams {
                source_type: input.source_type.clone(),
                source_mutability,
                uri: input.source.clone(),
                title: input.title.clone(),
                tags: input.tags.clone(),
                body: body_text,
            },
        )
        .await
        .map_err(map_core_error)?;

    let mut out = String::new();
    let _ = writeln!(
        out,
        "✓ Reference registered: {}",
        reference.reference_source_id
    );
    let _ = writeln!(out, "  Title:  {}", reference.title);
    let _ = writeln!(out, "  Type:   {}", reference.source_type);
    let _ = writeln!(out, "  URI:    {}", reference.uri);
    if let Some(cp) = &reference.content_path {
        let _ = writeln!(out, "  Body:   {cp}");
    }

    Ok(Some(out.trim_end().to_string()))
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

    Ok(Some(out.trim_end().to_string()))
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

    Ok(Some(out.trim_end().to_string()))
}

/// Resolve body text from `--file` or `--body` flags.
fn resolve_body_text(file: Option<&PathBuf>, body: Option<&String>) -> Result<String> {
    match (file, body) {
        (Some(_), Some(_)) => Err(CliError::Other(
            "Cannot specify both --file and --body. Choose one.".into(),
        )),
        (Some(path), None) => {
            if path.to_string_lossy() == "-" {
                // Read from stdin
                use std::io::Read;
                let mut buf = String::new();
                std::io::stdin()
                    .read_to_string(&mut buf)
                    .map_err(|e| CliError::Other(format!("Failed to read stdin: {e}")))?;
                Ok(buf)
            } else if path.exists() {
                std::fs::read_to_string(path).map_err(|e| {
                    CliError::Other(format!("Failed to read body file {}: {e}", path.display()))
                })
            } else {
                Err(CliError::Other(format!(
                    "Body file not found: {}",
                    path.display()
                )))
            }
        }
        (None, Some(text)) => Ok(text.clone()),
        (None, None) => Err(CliError::Other(
            "Body text is required. Use --file <path> or --body <text>.".into(),
        )),
    }
}

/// Validate that the `source_type` is a known contract enum value.
fn validate_source_type(source_type: &str) -> Result<()> {
    match source_type {
        "file" | "url" | "pdf" | "note" => Ok(()),
        other => Err(CliError::Other(format!(
            "Invalid source type {other:?}. Must be one of: file, url, pdf, note."
        ))),
    }
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
    fn validate_source_type_accepts_known_types() {
        assert!(validate_source_type("file").is_ok());
        assert!(validate_source_type("url").is_ok());
        assert!(validate_source_type("pdf").is_ok());
        assert!(validate_source_type("note").is_ok());
    }

    #[test]
    fn validate_source_type_rejects_unknown() {
        assert!(validate_source_type("unknown").is_err());
        assert!(validate_source_type("image").is_err());
    }

    #[test]
    fn resolve_body_text_rejects_both_file_and_body() {
        let result = resolve_body_text(
            Some(&PathBuf::from("/tmp/test.txt")),
            Some(&"inline text".into()),
        );
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("both"));
    }

    #[test]
    fn resolve_body_text_rejects_neither_file_nor_body() {
        let result = resolve_body_text(None, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("required"));
    }

    #[test]
    fn resolve_body_text_accepts_inline_body() {
        let result = resolve_body_text(None, Some(&"hello".into()));
        assert_eq!(result.unwrap(), "hello");
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
