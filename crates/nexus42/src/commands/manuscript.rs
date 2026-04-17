//! Manuscript Command Module
//!
//! Implements manuscript lifecycle management (roadmap §3.1.1).
//! Subcommands: create, edit, phase, status, promote, verify, export, list.

use crate::config::CliConfig;
use crate::errors::{CliError, Result};
use crate::manuscript::manager::{sanitize_title, validate_world_id, ManuscriptManager};
use clap::Subcommand;
use std::io::Write;

#[derive(Debug, Subcommand)]
pub enum ManuscriptCommand {
    /// Create a new manuscript
    Create {
        /// Manuscript title
        title: String,

        /// World ID (must start with wld_)
        #[arg(long)]
        world_id: Option<String>,
    },

    /// Edit manuscript content (opens in $EDITOR or prints content)
    Edit {
        /// Manuscript title
        title: String,
    },

    /// Show current manuscript phase
    Status,

    /// Set manuscript phase
    Phase {
        /// Manuscript title
        title: String,
        /// Target phase: brainstorm, draft, review, finalize, published
        phase: String,
    },

    /// Promote manuscript to the next phase
    Promote {
        /// Manuscript title
        title: String,

        /// Enable strict mode validation (V1.1)
        /// Checks: phase transition validity, manifest status, sync state
        #[arg(long)]
        strict: bool,
    },

    /// Verify manuscript consistency
    Verify {
        /// Manuscript title
        title: String,

        /// Check content integrity using SHA256 (V1.1)
        #[arg(long)]
        check_content: bool,
    },

    /// Export manuscript content
    Export {
        /// Manuscript title
        title: String,
        /// Export format: markdown, plain
        #[arg(long, default_value = "markdown")]
        format: String,
    },

    /// List all manuscripts
    #[command(alias = "ls")]
    List,
}

/// Run manuscript command
pub async fn run(cmd: ManuscriptCommand, _config: &CliConfig) -> Result<()> {
    // Find workspace root
    let workspace_root =
        crate::config::find_workspace_root().ok_or(CliError::WorkspaceNotInitialized)?;

    let manager = ManuscriptManager::new(workspace_root.clone());

    match cmd {
        ManuscriptCommand::Create { title, world_id } => {
            // Validate world_id format if provided
            if let Some(ref wid) = world_id {
                validate_world_id(wid)?;
            }
            let dir = manager.create(&title, world_id.as_deref())?;
            println!("Created manuscript: {}", title);
            println!("  Directory: {}", dir.display());
            println!("  Files: manuscript.md, metadata.json");
            println!("  Phase: brainstorm");
            Ok(())
        }
        ManuscriptCommand::Edit { title } => {
            let content = manager.read_content(&title)?;
            let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());

            // Write to a temp file for editing (title sanitized to prevent path traversal).
            // W-004: Use tempfile::NamedTempFile for automatic cleanup on drop/crash.
            let safe_title = sanitize_title(&title)?;
            let file_name = format!(".nexus42-edit-{}", safe_title);
            let mut temp_file = tempfile::NamedTempFile::with_prefix(file_name)
                .map_err(|e| CliError::Other(format!("Failed to create temp file: {}", e)))?;
            temp_file
                .write_all(content.as_bytes())
                .map_err(|e| CliError::Other(format!("Failed to write temp file: {}", e)))?;

            let tmp_path = temp_file.path().to_path_buf();

            // Open editor
            let status = std::process::Command::new(&editor)
                .arg(&tmp_path)
                .status()
                .map_err(|e| {
                    CliError::Other(format!("Failed to open editor '{}': {}", editor, e))
                })?;

            if status.success() {
                let edited = std::fs::read_to_string(&tmp_path)?;
                manager.write_content(&title, &edited)?;
                println!("Manuscript '{}' updated.", title);
                // NamedTempFile auto-deletes on drop
            } else {
                println!("Editor exited with non-zero status. Changes not saved.");
                // NamedTempFile auto-deletes on drop
            }

            Ok(())
        }
        ManuscriptCommand::Status => {
            let phase = ManuscriptManager::get_from_db(&workspace_root).await?;
            match phase {
                Some(p) => {
                    println!("Manuscript Status:");
                    println!("  Phase: {}", p);
                    println!("  Workspace: {}", workspace_root.display());
                }
                None => {
                    println!("Manuscript Status:");
                    println!("  Phase: not set");
                    println!("  Set with: nexus42 manuscript phase <title> <phase>");
                }
            }
            Ok(())
        }
        ManuscriptCommand::Phase { title, phase } => {
            let pool = open_workspace_db(&workspace_root).await?;
            let target = manager.set_phase(&title, &phase, &pool).await?;
            println!("Manuscript '{}' phase set to: {:?}", title, target);
            Ok(())
        }
        ManuscriptCommand::Promote { title, strict } => {
            let pool = open_workspace_db(&workspace_root).await?;
            let new_phase = manager.promote(&title, strict, &pool).await?;
            println!("Manuscript '{}' promoted to: {:?}", title, new_phase);
            if strict {
                println!("  Strict mode: all validation checks passed");
            }
            Ok(())
        }
        ManuscriptCommand::Verify {
            title,
            check_content,
        } => {
            let pool = open_workspace_db(&workspace_root).await?;
            let checks = manager.verify(&title, check_content, &pool).await?;
            println!("Verifying manuscript '{}'...", title);
            for check in &checks {
                println!("  {}", check);
            }
            let failures = checks
                .iter()
                .filter(|c: &&String| c.starts_with('✗'))
                .count();
            if failures == 0 {
                println!("✓ Verification passed.");
            } else {
                println!("✗ Verification failed: {} issue(s).", failures);
            }
            Ok(())
        }
        ManuscriptCommand::Export { title, format } => {
            let content = manager.export(&title, &format)?;
            println!("{}", content);
            Ok(())
        }
        ManuscriptCommand::List => {
            let manuscripts = manager.list()?;
            if manuscripts.is_empty() {
                println!("No manuscripts found.");
                println!("Create one with: nexus42 manuscript create \"<title>\"");
            } else {
                println!("Manuscripts ({}):", manuscripts.len());
                for m in &manuscripts {
                    println!("  • {}", m);
                }
            }
            Ok(())
        }
    }
}

/// Open the workspace SQLite database
async fn open_workspace_db(workspace_root: &std::path::Path) -> Result<nexus_local_db::SqlitePool> {
    let nexus_dir = crate::config::workspace_nexus_dir(workspace_root);
    let db_path = nexus_dir.join("state.db");

    // Ensure the nexus dir exists (init should have created it, but be safe)
    if !nexus_dir.exists() {
        return Err(CliError::WorkspaceNotInitialized);
    }

    let pool = crate::db::Schema::init(&db_path).await?;
    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_phase_from_empty_db() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test.db");
        let pool = crate::db::Schema::init(&db_path).await.unwrap();
        let result: Option<String> = sqlx::query_scalar(
            "SELECT value FROM workspace_meta WHERE key = 'manuscript_phase'",
        )
        .fetch_optional(&pool)
        .await
        .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_get_phase_after_set() {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test.db");
        let pool = crate::db::Schema::init(&db_path).await.unwrap();

        sqlx::query(
            "INSERT INTO workspace_meta (key, value) VALUES ('manuscript_phase', 'draft')",
        )
        .execute(&pool)
        .await
        .unwrap();

        let result: Option<String> = sqlx::query_scalar(
            "SELECT value FROM workspace_meta WHERE key = 'manuscript_phase'",
        )
        .fetch_optional(&pool)
        .await
        .unwrap();
        assert_eq!(result, Some("draft".to_string()));
    }

    #[test]
    fn test_temp_path_sanitization() {
        // Titles with path traversal should be rejected before reaching temp path
        let result = sanitize_title("../../../etc/passwd");
        assert!(
            result.is_err(),
            "Path traversal in title should be rejected"
        );

        let result = sanitize_title("my/novel");
        assert!(result.is_err(), "Slash in title should be rejected");

        // Normal titles should pass
        let result = sanitize_title("My Great Novel");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "My Great Novel");
    }
}
