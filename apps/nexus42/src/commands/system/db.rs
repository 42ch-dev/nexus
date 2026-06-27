//! Database Command — Database status and management
//!
//! Provides commands for inspecting database state and running migrations.

use crate::config::CliConfig;
use crate::errors::Result;
use clap::Subcommand;
use nexus_local_db::{read_versions, validate, RuntimeRole};

#[derive(Debug, Subcommand)]
pub enum DbCommand {
    /// Show database status (versions, health, tables)
    Status,
}

/// Run db command
///
/// # Errors
///
/// Returns `CliError` if:
/// - Database connection fails
/// - Schema validation fails
pub async fn run(cmd: DbCommand, _config: &CliConfig) -> Result<()> {
    match cmd {
        DbCommand::Status => status().await,
    }
}

/// Show database status
///
/// Prints:
/// - Schema versions (`db_schema_version` and `schema_version`)
/// - Health check result
/// - Existing tables
async fn status() -> Result<()> {
    // Get database path from config
    let db_path = crate::config::state_db_path()?;

    // Initialize schema if needed (opens pool + runs migrations + seeds versions)
    let pool = crate::db::Schema::init(&db_path).await?;

    println!("Database Status");
    println!("===============");
    println!();

    // Show database path
    println!("Path: {}", db_path.display());
    println!();

    // Read and display versions
    match read_versions(&pool).await {
        Ok(versions) => {
            println!("Schema Versions:");
            println!("  db_schema_version: {}", versions.db_schema_version);
            println!("  schema_version:    {}", versions.schema_version);
            println!(
                "  (from nexus-contracts: {})",
                nexus_contracts::generated::LATEST_SCHEMA_VERSION
            );
            println!();
        }
        Err(e) => {
            println!("Schema Versions: ERROR");
            println!("  {e}");
            println!();
        }
    }

    // Run health check
    match validate(&pool, RuntimeRole::Cli).await {
        Ok(()) => {
            println!("Health Check: OK");
            println!();
        }
        Err(e) => {
            println!("Health Check: FAILED");
            println!("  {e}");
            println!();
        }
    }

    // List existing tables
    println!("Tables:");
    let tables_raw: Vec<Option<String>> =
        sqlx::query_scalar!("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
            .fetch_all(&pool)
            .await?;
    let tables: Vec<String> = tables_raw.into_iter().flatten().collect();

    if tables.is_empty() {
        println!("  (none)");
    } else {
        for table in tables {
            println!("  - {table}");
        }
    }
    println!();

    // Show pragmas
    println!("Pragmas:");
    // SAFETY: PRAGMA statement — no table schema to validate against.
    let journal_mode: String = sqlx::query_scalar("PRAGMA journal_mode")
        .fetch_one(&pool)
        .await?;
    println!("  journal_mode: {journal_mode}");

    // SAFETY: PRAGMA statement — no table schema to validate against.
    let foreign_keys: i32 = sqlx::query_scalar("PRAGMA foreign_keys")
        .fetch_one(&pool)
        .await?;
    println!(
        "  foreign_keys: {}",
        if foreign_keys == 1 { "ON" } else { "OFF" }
    );
    println!();

    Ok(())
}
