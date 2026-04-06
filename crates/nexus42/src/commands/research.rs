//! Research Command Module
//!
//! V1.0 minimal research workflow (roadmap §3.1.1).
//! Subcommands: scan, list, extract.
//! Scope: local-only; no platform sync for research data.

use crate::config::CliConfig;
use crate::errors::Result;
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
pub async fn run(cmd: ResearchCommand, _config: &CliConfig) -> Result<()> {
    match cmd {
        ResearchCommand::Scan { path } => scan_references(&path),
        ResearchCommand::List { status } => list_references(status.as_deref()),
        ResearchCommand::Extract { source_id } => extract_references(source_id.as_deref()),
    }
}

/// Scan a directory for reference sources
fn scan_references(path: &str) -> Result<()> {
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

    Ok(())
}

/// List discovered reference sources
fn list_references(status_filter: Option<&str>) -> Result<()> {
    println!("Reference Sources:");

    if let Some(filter) = status_filter {
        println!("  Filter: status={}", filter);
    }

    println!();
    println!("  ⚠ V1.0 skeleton: listing from local SQLite cache.");
    println!("  No sources scanned yet. Run: nexus42 research scan");

    Ok(())
}

/// Extract structured data from references
fn extract_references(source_id: Option<&str>) -> Result<()> {
    if let Some(id) = source_id {
        println!("Extracting data from source: {}", id);
    } else {
        println!("Extracting data from all scanned sources...");
    }

    println!();
    println!("⚠ V1.0: extract records metadata only. Content extraction in V1.1+.");

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
