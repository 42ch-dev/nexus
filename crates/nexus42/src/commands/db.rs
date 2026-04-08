//! Database Command — Database status and management
//!
//! Provides commands for inspecting database state and running migrations.

use crate::config::CliConfig;
use crate::db;
use crate::errors::Result;
use clap::Subcommand;
use nexus_local_db::{read_versions, validate};
use rusqlite::Connection;

#[derive(Debug, Subcommand)]
pub enum DbCommand {
    /// Show database status (versions, health, tables)
    Status,
}

/// Run db command
pub async fn run(cmd: DbCommand, _config: &CliConfig) -> Result<()> {
    match cmd {
        DbCommand::Status => status().await,
    }
}

/// Show database status
///
/// Prints:
/// - Schema versions (db_schema_version and schema_version)
/// - Health check result
/// - Existing tables
async fn status() -> Result<()> {
    // Get database path from config
    let db_path = crate::config::state_db_path()?;

    // Open connection
    let conn = Connection::open(&db_path)?;

    // Initialize schema if needed
    db::Schema::init(&conn)?;

    println!("Database Status");
    println!("===============");
    println!();

    // Show database path
    println!("Path: {}", db_path.display());
    println!();

    // Read and display versions
    match read_versions(&conn) {
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
            println!("  {}", e);
            println!();
        }
    }

    // Run health check
    match validate(&conn) {
        Ok(()) => {
            println!("Health Check: OK");
            println!();
        }
        Err(e) => {
            println!("Health Check: FAILED");
            println!("  {}", e);
            println!();
        }
    }

    // List existing tables
    println!("Tables:");
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")?
        .query_map([], |row| row.get::<_, String>(0))?
        .flatten()
        .collect();

    if tables.is_empty() {
        println!("  (none)");
    } else {
        for table in tables {
            println!("  - {}", table);
        }
    }
    println!();

    // Show pragmas
    println!("Pragmas:");
    let journal_mode: String = conn.query_row("PRAGMA journal_mode", [], |row| row.get(0))?;
    println!("  journal_mode: {}", journal_mode);

    let foreign_keys: i32 = conn.query_row("PRAGMA foreign_keys", [], |row| row.get(0))?;
    println!(
        "  foreign_keys: {}",
        if foreign_keys == 1 { "ON" } else { "OFF" }
    );
    println!();

    Ok(())
}
