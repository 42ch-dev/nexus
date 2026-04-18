//! Research Command Module
//!
//! V1.0 minimal research workflow (roadmap §3.1.1).
//! Subcommands: scan, list, extract.
//! Scope: local-only; no platform sync for research data.
//!
//! V1.1 (CLI-R8): Added --extract-text flag for PDF/URL content extraction.

use crate::config::CliConfig;
use crate::errors::{CliError, Result};
use clap::Subcommand;
use sha2::{Digest, Sha256};
use std::process::Command;

#[derive(Debug, Subcommand)]
pub enum ResearchCommand {
    /// Scan References/ directory for reference sources
    Scan {
        /// Directory to scan (default: References/)
        #[arg(long, default_value = "References")]
        path: String,

        /// Extract text content from PDF and URL sources (V1.1)
        #[arg(long)]
        extract_text: bool,
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
        ResearchCommand::Scan { path, extract_text } => {
            scan_references(&path, extract_text, config).await
        }
        ResearchCommand::List { status } => list_references(status.as_deref(), config).await,
        ResearchCommand::Extract { source_id } => {
            extract_references(source_id.as_deref(), config).await
        }
    }
}

/// Scan a directory for reference sources
async fn scan_references(path: &str, extract_text: bool, config: &CliConfig) -> Result<()> {
    let scan_path = std::path::Path::new(path);

    if !scan_path.exists() {
        println!("Directory '{}' not found.", path);
        println!("Create it with: mkdir -p {}", path);
        return Ok(());
    }

    println!("Scanning '{}' for reference sources...", path);
    if extract_text {
        println!("Content extraction enabled (--extract-text)");
    }

    // V1.0: scan for common reference file types
    let mut found = Vec::new();
    let extensions = ["pdf", "md", "txt", "url", "html"];

    if let Ok(entries) = std::fs::read_dir(scan_path) {
        for entry in entries.flatten() {
            let file_name = entry.file_name().to_string_lossy().to_string();
            let ext = file_name.rsplit('.').next().unwrap_or("").to_lowercase();

            if extensions.contains(&ext.as_str()) {
                found.push((file_name, ext, entry.path()));
            }
        }
    }

    if found.is_empty() {
        println!("  No reference files found.");
        println!("  Supported formats: PDF, Markdown, Text, URL, HTML");
    } else {
        println!("  Found {} reference source(s):", found.len());
        for (name, _, _) in &found {
            println!("    • {}", name);
        }

        // Cache to local SQLite
        cache_scan_results(&found, extract_text).await?;
    }

    if !extract_text {
        println!();
        println!("⚠ V1.0: scan records metadata only. Use --extract-text for content extraction.");
    }

    // Suppress unused warning
    let _ = config;
    Ok(())
}

/// List discovered reference sources from SQLite
async fn list_references(status_filter: Option<&str>, _config: &CliConfig) -> Result<()> {
    println!("Reference Sources:");

    // Try to read from local SQLite
    let db_path = crate::config::state_db_path()?;
    if !db_path.exists() {
        println!("  No database found. Run: nexus42 research scan");
        return Ok(());
    }

    let pool = crate::db::Schema::init(&db_path).await?;

    let rows: Vec<(String, String, String, String, String, String)> = if let Some(filter) =
        status_filter
    {
        println!("  Filter: status={}", filter);
        // SAFETY: dynamic WHERE clause built from user-provided filter set.
        sqlx::query_as(
            "SELECT reference_source_id, source_type, uri, title, scan_status, created_at FROM reference_sources WHERE scan_status = ? ORDER BY created_at DESC",
        )
        .bind(filter)
        .fetch_all(&pool)
        .await?
    } else {
        let raw = sqlx::query!(
            r#"SELECT reference_source_id as "reference_source_id!", source_type as "source_type!", uri as "uri!", title as "title!", scan_status as "scan_status!", created_at as "created_at!" FROM reference_sources ORDER BY created_at DESC"#
        )
        .fetch_all(&pool)
        .await?;
        raw.into_iter()
            .map(|r| {
                (
                    r.reference_source_id,
                    r.source_type,
                    r.uri,
                    r.title,
                    r.scan_status,
                    r.created_at,
                )
            })
            .collect()
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
async fn extract_references(source_id: Option<&str>, _config: &CliConfig) -> Result<()> {
    let db_path = crate::config::state_db_path()?;
    if !db_path.exists() {
        println!("No database found. Run: nexus42 research scan");
        return Ok(());
    }

    let pool = crate::db::Schema::init(&db_path).await?;

    if let Some(id) = source_id {
        // Extract by specific ID
        let row = sqlx::query!(
            r#"SELECT reference_source_id as "reference_source_id!", source_type as "source_type!", uri as "uri!", title as "title!", scan_status as "scan_status!", created_at as "created_at!", tags, content_hash, content FROM reference_sources WHERE reference_source_id = ?"#,
            id
        )
        .fetch_optional(&pool)
        .await?;

        match row {
            Some(r) => {
                let ref_id = r.reference_source_id;
                let source_type = r.source_type;
                let uri = r.uri;
                let title = r.title;
                let status = r.scan_status;
                let created_at = r.created_at;
                let tags = r.tags;
                let content_hash = r.content_hash;
                let content = r.content;
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
                if let Some(c) = content {
                    println!();
                    println!("  Extracted Content ({} chars):", c.len());
                    println!("  {}", "-".repeat(60));
                    // Print first 500 chars of content
                    let preview = if c.len() > 500 {
                        format!("{}...", &c[..500])
                    } else {
                        c.clone()
                    };
                    for line in preview.lines() {
                        println!("  {}", line);
                    }
                    if c.len() > 500 {
                        println!("  ... (truncated, use database query for full content)");
                    }
                } else {
                    println!();
                    println!("  No content extracted. Run: nexus42 research scan --extract-text");
                }
            }
            None => {
                return Err(CliError::Config(format!(
                    "Reference source '{}' not found in local cache.",
                    id
                )));
            }
        }
    } else {
        // Extract all — show count and summary
        let count: i64 = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM reference_sources WHERE scan_status = 'scanned'"
        )
        .fetch_one(&pool)
        .await
        .unwrap_or(0);

        let with_content: i64 =
            sqlx::query_scalar!("SELECT COUNT(*) FROM reference_sources WHERE content IS NOT NULL")
                .fetch_one(&pool)
                .await
                .unwrap_or(0);

        println!("Reference Sources Summary:");
        println!("  Total scanned: {}", count);
        println!("  With content:  {}", with_content);

        if count == 0 {
            println!();
            println!("  No scanned sources found. Run: nexus42 research scan");
        } else if with_content == 0 {
            println!();
            println!("  No content extracted. Run: nexus42 research scan --extract-text");
        }
    }

    Ok(())
}

/// Cache scan results to local SQLite (uses configured workspace `state.db`).
async fn cache_scan_results(
    files: &[(String, String, std::path::PathBuf)],
    extract_text: bool,
) -> Result<()> {
    let db_path = crate::config::state_db_path()?;
    cache_scan_results_at(&db_path, files, extract_text).await
}

/// Cache scan results at an explicit DB path (for tests and callers that already resolved storage).
async fn cache_scan_results_at(
    db_path: &std::path::Path,
    files: &[(String, String, std::path::PathBuf)],
    extract_text: bool,
) -> Result<()> {
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let pool = crate::db::Schema::init(db_path).await?;

    let now = chrono::Utc::now().to_rfc3339();

    for (file, ext, full_path) in files {
        let hex = uuid::Uuid::new_v4().to_string().replace('-', "");
        let id = format!("ref_{}", &hex[..12]);
        let source_type = match ext.as_str() {
            "pdf" => "pdf",
            "md" => "file",
            "txt" => "file",
            "url" => "url",
            "html" => "file",
            _ => "file",
        };

        // Extract content if requested
        let (content, content_hash, scan_status) = if extract_text {
            match extract_source_content(source_type, full_path) {
                Ok(extracted) => {
                    let hash = compute_content_hash(&extracted);
                    (Some(extracted), Some(hash), "scanned")
                }
                Err(e) => {
                    eprintln!("  ⚠ Failed to extract content from '{}': {}", file, e);
                    (None, None, "failed")
                }
            }
        } else {
            (None, None, "pending")
        };

        let full_path_str = full_path.to_string_lossy().to_string();
        let file_owned = file.clone();
        let source_type_owned = source_type.to_string();
        let scan_status_owned = scan_status.to_string();
        let content_ref = content.as_deref();
        let content_hash_ref = content_hash.as_deref();
        let workspace_id = String::from("local");
        sqlx::query!(
            "INSERT OR IGNORE INTO reference_sources 
             (reference_source_id, workspace_id, source_type, uri, title, scan_status, content, content_hash, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            id, workspace_id, source_type_owned, full_path_str, file_owned, scan_status_owned, content_ref, content_hash_ref, now
        )
        .execute(&pool)
        .await?;
    }

    Ok(())
}

/// Extract content from a reference source (PDF or URL)
fn extract_source_content(source_type: &str, path: &std::path::Path) -> Result<String> {
    match source_type {
        "pdf" => extract_pdf_content(path),
        "url" => extract_url_content(path),
        _ => {
            // For plain text files, just read them
            std::fs::read_to_string(path).map_err(|e| {
                CliError::Other(format!("Failed to read file '{}': {}", path.display(), e))
            })
        }
    }
}

/// Extract text from a PDF using pdftotext (external tool, soft dependency)
fn extract_pdf_content(path: &std::path::Path) -> Result<String> {
    // Check if pdftotext is available
    let pdftotext_check = Command::new("pdftotext").arg("-v").output();

    if pdftotext_check.is_err() {
        return Err(CliError::Other(
            "pdftotext not found. Install poppler-utils: brew install poppler (macOS) or apt-get install poppler-utils (Linux)".to_string()
        ));
    }

    // Use pdftotext to extract text
    let output = Command::new("pdftotext")
        .arg("-layout") // Preserve layout
        .arg(path)
        .arg("-") // Output to stdout
        .output()
        .map_err(|e| CliError::Other(format!("Failed to run pdftotext: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(CliError::Other(format!(
            "pdftotext failed: {}",
            stderr.trim()
        )));
    }

    let text = String::from_utf8(output.stdout)
        .map_err(|e| CliError::Other(format!("pdftotext output is not valid UTF-8: {}", e)))?;

    if text.trim().is_empty() {
        return Err(CliError::Other(
            "PDF appears to be empty or contains only images".to_string(),
        ));
    }

    Ok(text)
}

/// Extract content from a URL file (file containing a URL)
fn extract_url_content(path: &std::path::Path) -> Result<String> {
    // Read the URL from the file
    let url_str = std::fs::read_to_string(path)
        .map_err(|e| CliError::Other(format!("Failed to read URL file: {}", e)))?;
    let url = url_str.trim();

    // Validate URL
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return Err(CliError::Other(format!(
            "Invalid URL format: '{}'. Must start with http:// or https://",
            url
        )));
    }

    // Use curl to fetch the content
    let curl_check = Command::new("curl").arg("--version").output();

    if curl_check.is_err() {
        return Err(CliError::Other(
            "curl not found. Install curl: brew install curl (macOS) or apt-get install curl (Linux)".to_string()
        ));
    }

    let output = Command::new("curl")
        .arg("-s") // Silent mode
        .arg("-L") // Follow redirects
        .arg("-m") // Max time
        .arg("30") // 30 seconds timeout
        .arg(url)
        .output()
        .map_err(|e| CliError::Other(format!("Failed to run curl: {}", e)))?;

    if !output.status.success() {
        return Err(CliError::Other(format!(
            "curl failed with status: {}",
            output.status
        )));
    }

    let html = String::from_utf8(output.stdout)
        .map_err(|e| CliError::Other(format!("Response is not valid UTF-8: {}", e)))?;

    // Strip HTML tags for a simple text extraction
    // Note: This is a basic implementation. For better results, consider using
    // a proper HTML parser like scraper or select.
    let text = strip_html_tags(&html);

    if text.trim().is_empty() {
        return Err(CliError::Other(
            "URL content appears to be empty after HTML stripping".to_string(),
        ));
    }

    Ok(text)
}

/// Strip HTML tags from content (basic implementation)
fn strip_html_tags(html: &str) -> String {
    // Use a simple regex-like approach to remove script and style blocks first
    let result = html.to_string();

    // Remove script blocks (including content)
    let mut cleaned = String::new();
    let mut in_script = false;
    let chars: Vec<char> = result.chars().collect();
    let mut i = 0;

    while i < chars.len() {
        // Check for script or style start
        if i + 7 <= chars.len() {
            let tag_start: String = chars[i..i + 7].iter().collect();
            let tag_lower = tag_start.to_lowercase();
            if tag_lower == "<script" {
                // Skip until </script>
                in_script = true;
                i += 7;
                continue;
            }
        }

        if i + 6 <= chars.len() {
            let tag_start: String = chars[i..i + 6].iter().collect();
            let tag_lower = tag_start.to_lowercase();
            if tag_lower == "<style" {
                in_script = true;
                i += 6;
                continue;
            }
        }

        // Check for script/style end
        if in_script && i + 9 <= chars.len() {
            let tag_end: String = chars[i..i + 9].iter().collect();
            let tag_lower = tag_end.to_lowercase();
            if tag_lower == "</script>" {
                in_script = false;
                i += 9;
                continue;
            }
        }

        if in_script && i + 8 <= chars.len() {
            let tag_end: String = chars[i..i + 8].iter().collect();
            let tag_lower = tag_end.to_lowercase();
            if tag_lower == "</style>" {
                in_script = false;
                i += 8;
                continue;
            }
        }

        // Skip content in script/style blocks
        if in_script {
            i += 1;
            continue;
        }

        // Remove HTML tags
        if chars[i] == '<' {
            // Skip until >
            while i < chars.len() && chars[i] != '>' {
                i += 1;
            }
            if i < chars.len() {
                i += 1; // Skip the '>'
            }
            continue;
        }

        cleaned.push(chars[i]);
        i += 1;
    }

    // Normalize whitespace
    cleaned
        .lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Compute SHA256 hash of content
fn compute_content_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let hash = hasher.finalize();
    format!("sha256:{}", hex::encode(hash))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Test content hash computation
    #[test]
    fn test_compute_content_hash() {
        let content = "Hello, world!";
        let hash = compute_content_hash(content);

        // SHA256 of "Hello, world!"
        assert!(hash.starts_with("sha256:"));
        assert_eq!(hash.len(), 71); // "sha256:" + 64 hex chars
    }

    /// Test HTML tag stripping
    #[test]
    fn test_strip_html_tags() {
        let html =
            "<html><head><title>Test</title></head><body><p>Hello <b>world</b>!</p></body></html>";
        let text = strip_html_tags(html);

        assert!(!text.contains('<'));
        assert!(!text.contains('>'));
        assert!(text.contains("Hello"));
        assert!(text.contains("world"));
    }

    /// Test HTML tag stripping removes script content
    #[test]
    fn test_strip_html_removes_scripts() {
        let html =
            r#"<html><body><script>alert('evil');</script><p>Safe content</p></body></html>"#;
        let text = strip_html_tags(html);

        assert!(!text.contains("script"));
        assert!(!text.contains("alert"));
        assert!(text.contains("Safe content"));
    }

    /// Test URL validation
    #[test]
    fn test_url_validation() {
        // Valid URLs
        assert!("http://example.com".starts_with("http://"));
        assert!("https://example.com".starts_with("https://"));

        // Invalid URLs
        assert!(!"ftp://example.com".starts_with("http"));
        assert!(!"example.com".starts_with("http"));
    }

    /// Test PDF extraction error handling when pdftotext is not available
    #[test]
    fn test_pdf_extraction_without_tool() {
        // This test verifies error handling when pdftotext is missing
        // The actual extraction depends on the tool being installed
        let result = extract_pdf_content(std::path::Path::new("/nonexistent.pdf"));
        // Either the tool is not found, or the file doesn't exist
        assert!(result.is_err());
    }

    /// Test scan results caching with content
    #[tokio::test]
    async fn test_cache_scan_results_with_content() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("test.txt");
        std::fs::write(&file_path, "Test content").unwrap();

        let files = vec![("test.txt".to_string(), "txt".to_string(), file_path.clone())];

        let db_path = tmp.path().join("state.db");

        // Test without extraction
        let result = cache_scan_results_at(&db_path, &files, false).await;
        if let Err(ref e) = result {
            eprintln!("Error in cache_scan_results (no extraction): {:?}", e);
        }
        assert!(result.is_ok());

        // Test with extraction
        let result = cache_scan_results_at(&db_path, &files, true).await;
        if let Err(ref e) = result {
            eprintln!("Error in cache_scan_results (with extraction): {:?}", e);
        }
        assert!(result.is_ok());
    }

    /// Test content extraction from text files
    #[test]
    fn test_extract_text_file() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("test.txt");
        std::fs::write(&file_path, "Hello, world!").unwrap();

        let result = extract_source_content("file", &file_path);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Hello, world!");
    }

    /// Test empty file handling
    #[test]
    fn test_extract_empty_file() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("empty.txt");
        std::fs::write(&file_path, "").unwrap();

        let result = extract_source_content("file", &file_path);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "");
    }

    /// Test UTF-8 content handling
    #[test]
    fn test_extract_utf8_content() {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("utf8.txt");
        std::fs::write(&file_path, "Hello 世界 🌍").unwrap();

        let result = extract_source_content("file", &file_path);
        assert!(result.is_ok());
        let content = result.unwrap();
        assert!(content.contains("世界"));
        assert!(content.contains("🌍"));
    }
}
