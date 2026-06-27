//! Reference source management subcommands.
//!
//! CLI surface for the V1.26 reference store (`SQLite` registry + body.md on disk).
//! Uses `nexus_local_db::reference_source` as the repository layer.
//!
//! V1.58 P3 adds `reference refresh` — dispatches `nexus.reference.refresh`
//! through the daemon's host-call endpoint for one or all non-offline sources.

use crate::api::daemon_client::DaemonClient;
use crate::config::CliConfig;
use crate::errors::{CliError, Result};
use clap::Subcommand;
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

    /// Refresh one or all reference source bodies (V1.58 P3 — DF-44).
    ///
    /// Dispatches `nexus.reference.refresh` through the daemon's host-call
    /// endpoint for each matching non-offline reference source.  Use `all`
    /// to refresh every eligible source; otherwise pass a specific
    /// `reference_source_id`.
    ///
    /// --dry-run prints what would be refreshed without mutating.
    Refresh {
        /// Reference source ID or "all" to refresh every non-offline source
        reference_ref: String,

        /// Print what would be refreshed without mutating
        #[arg(long)]
        dry_run: bool,
    },
}

/// Run a reference command.
///
/// # Errors
///
/// Returns `CliError` if the active creator is not set, the database is unavailable,
/// or the underlying repository operation fails.
pub async fn run(cmd: ReferenceCommand, config: &CliConfig) -> Result<()> {
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
                config,
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
        ReferenceCommand::List => run_list(config).await,
        ReferenceCommand::Show { reference_id } => run_show(config, &reference_id).await,
        ReferenceCommand::Refresh {
            reference_ref,
            dry_run,
        } => run_refresh(config, &reference_ref, dry_run).await,
    }
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

/// Resolve state.db path and open a pool with migrations.
async fn open_workspace_pool(config: &CliConfig) -> Result<sqlx::SqlitePool> {
    let db_path = crate::config::resolve_state_db_path(config)?;
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let pool = crate::db::Schema::init(&db_path).await?;
    Ok(pool)
}

/// Resolve active creator and workspace context for reference operations.
fn resolve_creator_context(config: &CliConfig) -> Result<(String, String, PathBuf)> {
    let creator_id = config
        .active_creator_id
        .as_deref()
        .ok_or(CliError::CreatorNotSelected)?
        .to_string();
    let slug = config.workspace_slug_for_creator(&creator_id).to_string();
    let home = dirs::home_dir()
        .ok_or_else(|| CliError::Other("Cannot determine home directory".into()))?;
    Ok((creator_id, slug, home))
}

/// `reference register` — create SQL row + body.md.
async fn run_register(config: &CliConfig, input: &RegisterInput) -> Result<()> {
    let (creator_id, _slug, home) = resolve_creator_context(config)?;

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

    let pool = open_workspace_pool(config).await?;

    // Default workspace_id — uses the operational workspace slug convention
    let workspace_id = format!("wrk_{}", config.workspace_slug_for_creator(&creator_id));

    let params = nexus_local_db::RegisterParams {
        home: &home,
        creator_id: &creator_id,
        workspace_id: &workspace_id,
        source_type: &input.source_type,
        source_mutability,
        uri: &input.source,
        title: &input.title,
        tags: input.tags.as_deref(),
        body: &body_text,
    };

    let row = nexus_local_db::register_reference(&pool, params).await?;

    println!("✓ Reference registered: {}", row.reference_source_id);
    println!("  Title:  {}", row.title);
    println!("  Type:   {}", row.source_type);
    println!("  URI:    {}", row.uri);
    if let Some(cp) = &row.content_path {
        println!("  Body:   {cp}");
    }

    Ok(())
}

/// `reference list` — show metadata for all references.
async fn run_list(config: &CliConfig) -> Result<()> {
    let _creator_context = resolve_creator_context(config)?;
    let pool = open_workspace_pool(config).await?;

    let rows = nexus_local_db::list_references(&pool, None, None, None).await?;

    if rows.is_empty() {
        println!("No registered references.");
        return Ok(());
    }

    println!(
        "{:<40} {:<10} {:<12} {:<40} CREATED_AT",
        "ID", "TYPE", "MUTABILITY", "TITLE"
    );
    for row in &rows {
        println!(
            "{:<40} {:<10} {:<12} {:<40} {}",
            row.reference_source_id,
            row.source_type,
            row.source_mutability,
            truncate(&row.title, 40),
            row.created_at
        );
    }

    Ok(())
}

/// `reference show` — display a single reference with details.
async fn run_show(config: &CliConfig, reference_id: &str) -> Result<()> {
    let _creator_context = resolve_creator_context(config)?;
    let pool = open_workspace_pool(config).await?;

    let row = nexus_local_db::get_reference_by_id(&pool, reference_id)
        .await?
        .ok_or_else(|| CliError::Other(format!("Reference {reference_id} not found.")))?;

    println!("Reference: {}", row.reference_source_id);
    println!("  Title:        {}", row.title);
    println!("  Type:         {}", row.source_type);
    println!("  Mutability:   {}", row.source_mutability);
    println!("  URI:          {}", row.uri);
    println!("  Workspace:    {}", row.workspace_id);
    println!("  Scan Status:  {}", row.scan_status);
    println!("  Created:      {}", row.created_at);
    if let Some(updated) = &row.updated_at {
        println!("  Updated:      {updated}");
    }
    if let Some(tags) = &row.tags {
        println!("  Tags:         {tags}");
    }
    if let Some(hash) = &row.content_hash {
        println!("  Content Hash: {hash}");
    }
    if let Some(cp) = &row.content_path {
        println!("  Body Path:    {cp}");
    }

    Ok(())
}

/// `reference refresh` — dispatch `nexus.reference.refresh` for one or all non-offline sources.
///
/// V1.58 P3: closes DF-44 by wiring the user-facing CLI surface to the daemon's
/// host-call endpoint.  `--dry-run` lists what would be refreshed without mutating.
async fn run_refresh(config: &CliConfig, reference_ref: &str, dry_run: bool) -> Result<()> {
    let (creator_id, _slug, _home) = resolve_creator_context(config)?;
    let pool = open_workspace_pool(config).await?;

    // Determine which reference sources to refresh — scoped by creator (H-002).
    let sources: Vec<nexus_local_db::ReferenceSourceRow> = if reference_ref == "all" {
        // Refresh every non-offline source owned by the active creator.
        let all =
            nexus_local_db::list_references(&pool, Some(1000), None, Some(&creator_id)).await?;
        all.into_iter()
            .filter(|s| s.refresh_policy != "offline")
            .collect()
    } else {
        // Single reference by ID, scoped to creator.
        let source =
            nexus_local_db::find_reference_by_id_for_creator(&pool, reference_ref, &creator_id)
                .await?
                .ok_or_else(|| {
                    CliError::Other(format!(
                        "Reference source '{reference_ref}' not found or not owned by creator '{creator_id}'."
                    ))
                })?;
        if source.refresh_policy == "offline" {
            return Err(CliError::Other(format!(
                "Reference source '{reference_ref}' has refresh policy 'offline' — cannot refresh."
            )));
        }
        vec![source]
    };

    if sources.is_empty() {
        println!("No reference sources to refresh.");
        return Ok(());
    }

    if dry_run {
        println!(
            "[DRY RUN] Would refresh {} reference source(s):",
            sources.len()
        );
        for s in &sources {
            println!(
                "  {}  title=\"{}\"  policy={}  uri={}",
                s.reference_source_id, s.title, s.refresh_policy, s.uri
            );
        }
        return Ok(());
    }

    // Connect to the daemon.
    let client = DaemonClient::from_config(config);
    if !client.health_check().await? {
        return Err(CliError::daemon_not_reachable_with_remediation());
    }

    let mut refreshed = 0u64;
    let mut errors = 0u64;

    for source in &sources {
        let params = serde_json::json!({
            "tool_name": "nexus.reference.refresh",
            "parameters": {
                "reference_source_id": source.reference_source_id,
            },
        });

        match client
            .post::<serde_json::Value, _>("/v1/local/agent-host/internal/tool-executions", &params)
            .await
        {
            Ok(result) => {
                let status = result
                    .get("status")
                    .and_then(|s| s.as_str())
                    .unwrap_or("unknown");
                let content_changed = result
                    .get("content_changed")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);
                let marker = if content_changed { "✓" } else { "○" };
                println!("  {marker} {}  status={status}", source.reference_source_id);
                refreshed += 1;
            }
            Err(e) => {
                eprintln!("  ✗ {}  error={e}", source.reference_source_id);
                errors += 1;
            }
        }
    }

    println!(
        "Refreshed {refreshed} source(s){}.",
        if errors > 0 {
            format!("; {errors} error(s)")
        } else {
            String::new()
        }
    );

    Ok(())
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
