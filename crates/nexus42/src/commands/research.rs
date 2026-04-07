//! Research Command Module
//!
//! V1.0 minimal research workflow (roadmap §3.1.1).
//! Subcommands: scan, list, extract.
//! Scope: local-only; no platform sync for research data.

use crate::config::CliConfig;
use crate::errors::{CliError, Result};
use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum ResearchCommand {
    /// Scan References/ directory for reference sources
    Scan {
        /// Directory to scan (default: References/)
        #[arg(long, default_value = "References")]
        path: String,
    },

    /// List discovered reference sources
    List {
        /// Filter by status: scanned, pending, error
        #[arg(long)]
        status: Option<String>,
    },

    /// Extract structured data from references
    Extract {
        /// Specific source ID to extract (default: all scanned sources)
        source_id: Option<String>,
    },
}

/// Run research command
pub async fn run(cmd: ResearchCommand, config: &CliConfig) -> Result<()> {
    match cmd {
        ResearchCommand::Scan { path } => scan_references(&path, config),
        ResearchCommand::List { status } => list_references(status.as_deref(), config),
        ResearchCommand::Extract { source_id } => extract_references(source_id.as_deref(), config),
    }
}

/// Scan a directory for reference sources
fn scan_references(path: &str, config: &CliConfig) -> Result<()> {
    let scan_path = std::path::Path::new(path);

    if !scan_path.exists() {
        println!("Directory '{}' not found.", path);
        println!("Create it with: mkdir -p {}", path);
        return Ok(());
    }

    println!("Scanning '{}' for reference sources...", path);

    // V1.0: scan for common reference file types
    let mut found = Vec::new();
    let extensions = ["pdf", "md", "txt", "url", "html"];

    if let Ok(entries) = std::fs::read_dir(scan_path) {
        for entry in entries.flatten() {
            let file_name = entry.file_name().to_string_lossy().to_string();
            let ext = file_name.rsplit('.').next().unwrap_or("").to_lowercase();

            if extensions.contains(&ext.as_str()) {
                found.push(file_name);
            }
        }
    }

    if found.is_empty() {
        println!("  No reference files found.");
        println!("  Supported formats: PDF, Markdown, Text, URL");
    } else {
        println!("  Found {} reference source(s):", found.len());
        for name in &found {
            println!("    • {}", name);
        }

        // Cache to local SQLite
        cache_scan_results(&found)?;
    }

    println!();
    println!("⚠ V1.0: scan records metadata only. PDF/URL content extraction in V1.1+.");

    // Suppress unused warning
    let _ = config;
    Ok(())
}

/// List discovered reference sources from SQLite
fn list_references(status_filter: Option<&str>, _config: &CliConfig) -> Result<()> {
    println!("Reference Sources:");

    // Try to read from local SQLite
    let db_path = crate::config::state_db_path()?;
    if !db_path.exists() {
        println!("  No database found. Run: nexus42 research scan");
        return Ok(());
    }

    let conn = rusqlite::Connection::open(&db_path)?;
    crate::db::Schema::init(&conn)?;

    let sql = match status_filter {
        Some(filter) => {
            println!("  Filter: status={}", filter);
            "SELECT reference_source_id, source_type, uri, title, scan_status, created_at FROM reference_sources WHERE scan_status = ?1 ORDER BY created_at DESC"
        }
        None => "SELECT reference_source_id, source_type, uri, title, scan_status, created_at FROM reference_sources ORDER BY created_at DESC",
    };

    let mut stmt = conn.prepare(sql)?;
    let rows: Vec<(String, String, String, String, String, String)> = match status_filter {
        Some(filter) => stmt
            .query_map(rusqlite::params![filter], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            })?
            .flatten()
            .collect(),
        None => stmt
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            })?
            .flatten()
            .collect(),
    };

    if rows.is_empty() {
        println!("  No sources found in local cache.");
        println!("  Run: nexus42 research scan");
    } else {
        println!("  {} source(s) in local cache:", rows.len());
        println!();
        let header = format!(
            "  {:<20} {:<10} {:<30} {:<10} {}",
            "ID", "TYPE", "URI", "STATUS", "TITLE"
        );
        println!("{}", header);
        println!("  {}", "-".repeat(80));

        for (id, source_type, uri, title, status, _created_at) in &rows {
            println!(
                "  {:<20} {:<10} {:<30} {:<10} {}",
                id, source_type, uri, status, title
            );
        }
    }

    Ok(())
}

/// Extract structured data from references by ID
fn extract_references(source_id: Option<&str>, _config: &CliConfig) -> Result<()> {
    let db_path = crate::config::state_db_path()?;
    if !db_path.exists() {
        println!("No database found. Run: nexus42 research scan");
        return Ok(());
    }

    let conn = rusqlite::Connection::open(&db_path)?;
    crate::db::Schema::init(&conn)?;

    if let Some(id) = source_id {
        // Extract by specific ID
        let result = conn.query_row(
            "SELECT reference_source_id, source_type, uri, title, scan_status, created_at, tags, content_hash FROM reference_sources WHERE reference_source_id = ?1",
            rusqlite::params![id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, Option<String>>(6)?,
                    row.get::<_, Option<String>>(7)?,
                ))
            },
        );

        match result {
            Ok((ref_id, source_type, uri, title, status, created_at, tags, content_hash)) => {
                println!("Reference Source: {}", ref_id);
                println!("  Title:       {}", title);
                println!("  Type:        {}", source_type);
                println!("  URI:         {}", uri);
                println!("  Status:      {}", status);
                println!("  Created:     {}", created_at);
                if let Some(t) = tags {
                    println!("  Tags:        {}", t);
                }
                if let Some(h) = content_hash {
                    println!("  Content Hash: {}", h);
                }
                println!();
                println!("⚠ V1.0: metadata only. Content extraction in V1.1+.");
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                return Err(CliError::Config(format!(
                    "Reference source '{}' not found in local cache.",
                    id
                )));
            }
            Err(e) => return Err(e.into()),
        }
    } else {
        // Extract all — show count and summary
        let count: u64 = conn
            .query_row(
                "SELECT COUNT(*) FROM reference_sources WHERE scan_status = 'scanned'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0);

        println!("Extracting data from {} scanned source(s)...", count);
        println!();
        println!("⚠ V1.0: extract records metadata only. Content extraction in V1.1+.");

        if count == 0 {
            println!("  No scanned sources found. Run: nexus42 research scan");
        }
    }

    Ok(())
}

/// Cache scan results to local SQLite
fn cache_scan_results(files: &[String]) -> Result<()> {
    use crate::config::state_db_path;

    let db_path = state_db_path()?;
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let conn = rusqlite::Connection::open(&db_path)?;
    crate::db::Schema::init(&conn)?;

    let now = chrono::Utc::now().to_rfc3339();

    for file in files {
        let hex = uuid::Uuid::new_v4().to_string().replace('-', "");
        let id = format!("ref_{}", &hex[..12]);
        let ext = file.rsplit('.').next().unwrap_or("unknown");
        let source_type = match ext {
            "pdf" => "pdf",
            "md" => "markdown",
            "txt" => "text",
            "url" => "url",
            "html" => "html",
            _ => "unknown",
        };

        conn.execute(
            "INSERT OR IGNORE INTO reference_sources (reference_source_id, workspace_id, source_type, uri, title, scan_status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![id, "local", source_type, format!("References/{}", file), file, "scanned", now],
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    // Module-level tests for research command
    // Functional tests covered by integration tests
}
