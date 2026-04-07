//! Manuscript Manager
//!
//! Manages manuscript files, phases, and metadata for a Nexus workspace.
//! Manuscript files are stored under `Stories/<title>/` within the workspace directory.

use crate::errors::{CliError, Result};
use crate::manuscript::metadata::ManuscriptMetadata;
use nexus_contracts::ManuscriptPhase;
use nexus_domain::consistency;
use nexus_domain::manuscript_state::ManuscriptState;
use rusqlite::Connection;
use std::path::{Path, PathBuf};

/// Validate that a world ID has the `wld_` prefix
pub fn validate_world_id(world_id: &str) -> Result<()> {
    if !world_id.starts_with("wld_") {
        return Err(CliError::Config(format!(
            "Invalid world_id format '{}': must start with 'wld_'",
            world_id
        )));
    }
    Ok(())
}

/// Default manuscript template content
const MANUSCRIPT_TEMPLATE: &str = r#"# {{title}}

*Created by Nexus42*

---

## Synopsis

<!-- Write your synopsis here -->

## Chapters

<!-- Add chapters as you go -->

"#;

/// Manuscript manager for file and database operations
pub struct ManuscriptManager {
    workspace_root: PathBuf,
}

impl ManuscriptManager {
    /// Create a new manuscript manager for the given workspace root
    pub fn new(workspace_root: PathBuf) -> Self {
        Self { workspace_root }
    }

    /// Get the Stories directory path
    pub fn stories_dir(&self) -> PathBuf {
        self.workspace_root.join("Stories")
    }

    /// Get the path to a specific manuscript's directory
    pub fn manuscript_dir(&self, title: &str) -> PathBuf {
        self.stories_dir().join(title)
    }

    /// Get the path to a manuscript's main file
    pub fn manuscript_file(&self, title: &str) -> PathBuf {
        self.manuscript_dir(title).join("manuscript.md")
    }

    /// Get the path to a manuscript's metadata file
    pub fn metadata_file(&self, title: &str) -> PathBuf {
        self.manuscript_dir(title).join("metadata.json")
    }

    /// Create a new manuscript with the given title
    pub fn create(&self, title: &str, world_id: Option<&str>) -> Result<PathBuf> {
        validate_world_id(world_id.unwrap_or("wld_default"))?;

        let dir = self.manuscript_dir(title);

        if dir.exists() {
            return Err(CliError::Config(format!(
                "Manuscript '{}' already exists at {}",
                title,
                dir.display()
            )));
        }

        // Create directory structure
        std::fs::create_dir_all(&dir)?;

        // Create manuscript.md from template
        let content = MANUSCRIPT_TEMPLATE.replace("{{title}}", title);
        let manuscript_path = self.manuscript_file(title);
        std::fs::write(&manuscript_path, content)?;

        // Create metadata
        let metadata = ManuscriptMetadata::new(title, world_id);
        let metadata_path = self.metadata_file(title);
        let metadata_json = serde_json::to_string_pretty(&metadata)?;
        std::fs::write(&metadata_path, metadata_json)?;

        Ok(dir)
    }

    /// Read the content of a manuscript
    pub fn read_content(&self, title: &str) -> Result<String> {
        let path = self.manuscript_file(title);
        if !path.exists() {
            return Err(CliError::Config(format!(
                "Manuscript '{}' not found. Create it with: nexus42 manuscript create \"{}\"",
                title, title
            )));
        }
        let content = std::fs::read_to_string(&path)?;
        Ok(content)
    }

    /// Read metadata for a manuscript
    pub fn read_metadata(&self, title: &str) -> Result<ManuscriptMetadata> {
        let path = self.metadata_file(title);
        if !path.exists() {
            return Err(CliError::Config(format!(
                "Manuscript '{}' metadata not found.",
                title
            )));
        }
        let content = std::fs::read_to_string(&path)?;
        let metadata: ManuscriptMetadata = serde_json::from_str(&content)?;
        Ok(metadata)
    }

    /// Write content to a manuscript
    pub fn write_content(&self, title: &str, content: &str) -> Result<()> {
        let path = self.manuscript_file(title);
        if !path.exists() {
            return Err(CliError::Config(format!(
                "Manuscript '{}' not found.",
                title
            )));
        }
        // Validate UTF-8 safety
        let _ = std::str::from_utf8(content.as_bytes())
            .map_err(|e| CliError::Config(format!("Content contains invalid UTF-8: {}", e)))?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    /// Export manuscript content in the specified format
    pub fn export(&self, title: &str, format: &str) -> Result<String> {
        let content = self.read_content(title)?;

        match format {
            "markdown" | "md" => Ok(content),
            "plain" | "txt" => {
                // Strip markdown formatting: remove # headers, bold, italic, links
                let plain = content
                    .lines()
                    .map(|line| {
                        let mut l = line.to_string();
                        // Remove header markers
                        while l.starts_with('#') {
                            l = l[1..].trim_start().to_string();
                        }
                        // Remove bold/italic markers
                        l = l
                            .replace("**", "")
                            .replace("*", "")
                            .replace("__", "")
                            .replace("_", "");
                        // Remove link syntax
                        while let Some(start) = l.find('[') {
                            if let Some(end) = l[start..].find("](") {
                                let link_end = l[start + end + 2..].find(')').unwrap_or(0);
                                let link_text = &l[start + 1..start + end];
                                let after_link = &l[start + end + 2 + link_end + 1..];
                                l = format!("{}{}{}", &l[..start], link_text, after_link);
                            } else {
                                break;
                            }
                        }
                        l
                    })
                    .collect::<Vec<_>>()
                    .join("\n");
                Ok(plain)
            }
            _ => Err(CliError::Config(format!(
                "Unknown export format '{}'. Supported: markdown, plain",
                format
            ))),
        }
    }

    /// List all manuscripts
    pub fn list(&self) -> Result<Vec<String>> {
        let stories_dir = self.stories_dir();
        if !stories_dir.exists() {
            return Ok(Vec::new());
        }

        let mut manuscripts = Vec::new();
        let entries = std::fs::read_dir(&stories_dir)?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && path.join("manuscript.md").exists() {
                if let Some(name) = path.file_name() {
                    manuscripts.push(name.to_string_lossy().to_string());
                }
            }
        }

        manuscripts.sort();
        Ok(manuscripts)
    }

    /// Set the manuscript phase in SQLite
    pub fn set_phase(
        &self,
        title: &str,
        phase: &str,
        conn: &Connection,
    ) -> Result<ManuscriptPhase> {
        let target_phase = parse_phase(phase)?;

        // Validate phase transition using domain consistency rules
        consistency::validate_manuscript_phase(phase)?;

        // Check current phase in DB
        let current_phase: Option<String> = conn
            .query_row(
                "SELECT value FROM workspace_meta WHERE key = ?1",
                ["manuscript_phase"],
                |row| row.get(0),
            )
            .ok();

        if let Some(ref current) = current_phase {
            let current_parsed = parse_phase(current)?;
            // Use domain ManuscriptState to validate transition
            let mut state = ManuscriptState::new("local", "wld_default", "local");
            // Set current phase
            set_phase_on_state(&mut state, &current_parsed)?;
            // Check if transition is valid
            if !state.can_transition_to(&target_phase) {
                return Err(CliError::Config(format!(
                    "Invalid phase transition: {} → {}. Valid next phase from {} is {}.",
                    current,
                    phase,
                    current,
                    next_phase_name(&current_parsed)
                )));
            }
        }

        // Store in workspace_meta
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT OR REPLACE INTO workspace_meta (key, value, updated_at) VALUES (?1, ?2, ?3)",
            rusqlite::params!["manuscript_phase", phase, now],
        )?;

        // Also update metadata file
        if let Ok(mut metadata) = self.read_metadata(title) {
            metadata.phase = phase.to_string();
            metadata.updated_at = now.clone();
            if let Ok(json) = serde_json::to_string_pretty(&metadata) {
                let _ = std::fs::write(self.metadata_file(title), json);
            }
        }

        Ok(target_phase)
    }

    /// Get the current manuscript phase from SQLite
    pub fn get_phase(conn: &Connection) -> Result<Option<String>> {
        let phase: Option<String> = conn
            .query_row(
                "SELECT value FROM workspace_meta WHERE key = 'manuscript_phase'",
                [],
                |row| row.get(0),
            )
            .ok();
        Ok(phase)
    }

    /// Get the current manuscript phase from the workspace database (convenience method)
    pub fn get_from_db(workspace_root: &Path) -> Result<Option<String>> {
        let nexus_dir = crate::config::workspace_nexus_dir(workspace_root);
        let db_path = nexus_dir.join("state.db");
        if !db_path.exists() {
            return Ok(None);
        }
        let conn = Connection::open(&db_path)?;
        crate::db::Schema::init(&conn)?;
        Self::get_phase(&conn)
    }

    /// Promote manuscript to the next phase
    pub fn promote(&self, title: &str, conn: &Connection) -> Result<ManuscriptPhase> {
        let current = Self::get_phase(conn)?;

        let current_phase = match current {
            Some(ref p) => parse_phase(p)?,
            None => {
                return Err(CliError::Config(
                    "No manuscript phase set. Use `nexus42 manuscript phase <title> <phase>` first."
                        .to_string(),
                ));
            }
        };

        // Use domain ManuscriptState to perform the promotion with validation
        let mut state = ManuscriptState::new("local", "wld_default", "local");
        set_phase_on_state(&mut state, &current_phase)?;

        state
            .promote()
            .map_err(|e| CliError::Config(format!("{}", e)))?;

        let new_phase = state.current_phase().clone();
        let phase_str = phase_to_str(&new_phase);

        // Persist to SQLite
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT OR REPLACE INTO workspace_meta (key, value, updated_at) VALUES (?1, ?2, ?3)",
            rusqlite::params!["manuscript_phase", phase_str, now],
        )?;

        // Update metadata file
        if let Ok(mut metadata) = self.read_metadata(title) {
            metadata.phase = phase_str.to_string();
            metadata.updated_at = now.clone();
            if let Ok(json) = serde_json::to_string_pretty(&metadata) {
                let _ = std::fs::write(self.metadata_file(title), json);
            }
        }

        Ok(new_phase)
    }

    /// Verify manuscript consistency using domain rules
    pub fn verify(&self, title: &str, conn: &Connection) -> Result<Vec<String>> {
        let mut checks = Vec::new();

        // 1. File integrity
        let manuscript_path = self.manuscript_file(title);
        if manuscript_path.exists() {
            let content = self.read_content(title)?;
            // UTF-8 safety check (CTX-R5)
            if std::str::from_utf8(content.as_bytes()).is_ok() {
                checks.push(format!("✓ UTF-8 integrity: OK ({} bytes)", content.len()));
            } else {
                checks.push("✗ UTF-8 integrity: INVALID".to_string());
            }
        } else {
            checks.push(format!(
                "✗ Manuscript file not found: {}",
                manuscript_path.display()
            ));
        }

        // 2. Metadata integrity
        match self.read_metadata(title) {
            Ok(metadata) => {
                checks.push(format!("✓ Metadata: OK (title: '{}')", metadata.title));

                // Validate world_id format (CTX-R4)
                if let Some(ref wid) = metadata.world_id {
                    match validate_world_id(wid) {
                        Ok(()) => checks.push(format!("✓ World ID format: OK ({})", wid)),
                        Err(e) => checks.push(format!("✗ World ID format: {}", e)),
                    }
                }
            }
            Err(e) => {
                checks.push(format!("✗ Metadata: {}", e));
            }
        }

        // 3. Phase consistency from SQLite
        if let Ok(Some(phase)) = Self::get_phase(conn) {
            if consistency::validate_manuscript_phase(&phase).is_ok() {
                checks.push(format!("✓ Phase consistency: OK ({})", phase));
            } else {
                checks.push(format!("✗ Phase consistency: INVALID phase '{}'", phase));
            }
        } else {
            checks.push("⚠ Phase: not set in database".to_string());
        }

        // 4. Excerpt length validation
        if let Ok(content) = self.read_content(title) {
            match consistency::validate_excerpt_length(&content) {
                Ok(()) => checks.push("✓ Excerpt length: OK".to_string()),
                Err(e) => checks.push(format!("✗ Excerpt length: {}", e)),
            }
        }

        Ok(checks)
    }
}

/// Parse a phase string into ManuscriptPhase
fn parse_phase(phase: &str) -> Result<ManuscriptPhase> {
    match phase.to_lowercase().as_str() {
        "brainstorm" => Ok(ManuscriptPhase::Brainstorm),
        "draft" => Ok(ManuscriptPhase::Draft),
        "review" => Ok(ManuscriptPhase::Review),
        "finalize" => Ok(ManuscriptPhase::Finalize),
        "published" => Ok(ManuscriptPhase::Published),
        _ => Err(CliError::Config(format!(
            "Unknown phase '{}'. Valid: brainstorm, draft, review, finalize, published",
            phase
        ))),
    }
}

/// Convert ManuscriptPhase to string
fn phase_to_str(phase: &ManuscriptPhase) -> &'static str {
    match phase {
        ManuscriptPhase::Brainstorm => "brainstorm",
        ManuscriptPhase::Draft => "draft",
        ManuscriptPhase::Review => "review",
        ManuscriptPhase::Finalize => "finalize",
        ManuscriptPhase::Published => "published",
    }
}

/// Get the next expected phase name
fn next_phase_name(phase: &ManuscriptPhase) -> &'static str {
    match phase {
        ManuscriptPhase::Brainstorm => "draft",
        ManuscriptPhase::Draft => "review",
        ManuscriptPhase::Review => "finalize",
        ManuscriptPhase::Finalize => "published",
        ManuscriptPhase::Published => "none (already published)",
    }
}

/// Set a phase on a ManuscriptState by cycling through promotions
fn set_phase_on_state(state: &mut ManuscriptState, target: &ManuscriptPhase) -> Result<()> {
    loop {
        if state.current_phase() == target {
            return Ok(());
        }
        state
            .promote()
            .map_err(|e| CliError::Config(format!("{}", e)))?;
    }
}

/// Validate UTF-8 safety for content handling (CTX-R5)
#[allow(dead_code)]
pub fn validate_utf8_content(content: &str) -> Result<()> {
    // Ensure content is valid UTF-8 (it should be since it's a &str, but
    // this validates that round-trip through bytes is clean)
    let bytes = content.as_bytes();
    let roundtrip = std::str::from_utf8(bytes)
        .map_err(|e| CliError::Config(format!("Content contains invalid UTF-8: {}", e)))?;
    if roundtrip != content {
        return Err(CliError::Config(
            "Content UTF-8 round-trip mismatch".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_manager() -> (TempDir, ManuscriptManager) {
        let tmp = TempDir::new().unwrap();
        let manager = ManuscriptManager::new(tmp.path().to_path_buf());
        (tmp, manager)
    }

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::Schema::init(&conn).unwrap();
        conn
    }

    // ── World ID validation (CTX-R4) ──

    #[test]
    fn test_validate_world_id_valid() {
        assert!(validate_world_id("wld_abc123").is_ok());
        assert!(validate_world_id("wld_").is_ok());
        assert!(validate_world_id("wld_test-world-42").is_ok());
    }

    #[test]
    fn test_validate_world_id_invalid() {
        assert!(validate_world_id("abc123").is_err());
        assert!(validate_world_id("world_test").is_err());
        assert!(validate_world_id("").is_err());
        assert!(validate_world_id("WLD_test").is_err());
    }

    // ── Manuscript creation ──

    #[test]
    fn test_create_manuscript() {
        let (_tmp, manager) = setup_manager();

        let result = manager.create("My Novel", None);
        assert!(result.is_ok());

        let dir = result.unwrap();
        assert!(dir.exists());
        assert!(dir.join("manuscript.md").exists());
        assert!(dir.join("metadata.json").exists());
    }

    #[test]
    fn test_create_manuscript_with_world_id() {
        let (_tmp, manager) = setup_manager();

        let result = manager.create("Test Story", Some("wld_abc123"));
        assert!(result.is_ok());

        let metadata = manager.read_metadata("Test Story").unwrap();
        assert_eq!(metadata.world_id, Some("wld_abc123".to_string()));
    }

    #[test]
    fn test_create_duplicate_manuscript_fails() {
        let (_tmp, manager) = setup_manager();

        manager.create("Duplicate", None).unwrap();
        let result = manager.create("Duplicate", None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[test]
    fn test_create_manuscript_invalid_world_id() {
        let (_tmp, manager) = setup_manager();

        let result = manager.create("Bad World", Some("invalid_id"));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("wld_"));
    }

    #[test]
    fn test_manuscript_template_content() {
        let (_tmp, manager) = setup_manager();

        manager.create("Template Test", None).unwrap();
        let content = manager.read_content("Template Test").unwrap();

        assert!(content.contains("# Template Test"));
        assert!(content.contains("Synopsis"));
        assert!(content.contains("Chapters"));
    }

    // ── Read/Write content ──

    #[test]
    fn test_read_nonexistent_manuscript() {
        let (_tmp, manager) = setup_manager();

        let result = manager.read_content("Nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_write_and_read_content() {
        let (_tmp, manager) = setup_manager();

        manager.create("ReadWrite", None).unwrap();
        manager
            .write_content("ReadWrite", "New content here")
            .unwrap();

        let content = manager.read_content("ReadWrite").unwrap();
        assert_eq!(content, "New content here");
    }

    #[test]
    fn test_write_nonexistent_manuscript_fails() {
        let (_tmp, manager) = setup_manager();

        let result = manager.write_content("Ghost", "content");
        assert!(result.is_err());
    }

    // ── Export ──

    #[test]
    fn test_export_markdown() {
        let (_tmp, manager) = setup_manager();

        manager.create("Export Test", None).unwrap();
        let result = manager.export("Export Test", "markdown");
        assert!(result.is_ok());
        let content = result.unwrap();
        assert!(content.contains("# Export Test"));
    }

    #[test]
    fn test_export_plain_text() {
        let (_tmp, manager) = setup_manager();

        manager.create("Plain Test", None).unwrap();
        // Write content with markdown formatting
        manager
            .write_content(
                "Plain Test",
                "# Header\n\n**bold** and *italic*\n\n[link](http://example.com)",
            )
            .unwrap();

        let result = manager.export("Plain Test", "plain");
        assert!(result.is_ok());
        let content = result.unwrap();
        assert!(!content.contains('#'));
        assert!(!content.contains("**"));
        assert!(content.contains("bold"));
    }

    #[test]
    fn test_export_unknown_format_fails() {
        let (_tmp, manager) = setup_manager();

        manager.create("Format Test", None).unwrap();
        let result = manager.export("Format Test", "pdf");
        assert!(result.is_err());
    }

    // ── List manuscripts ──

    #[test]
    fn test_list_empty() {
        let (_tmp, manager) = setup_manager();

        let result = manager.list();
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_list_multiple_manuscripts() {
        let (_tmp, manager) = setup_manager();

        manager.create("Alpha", None).unwrap();
        manager.create("Beta", None).unwrap();
        manager.create("Gamma", None).unwrap();

        let result = manager.list().unwrap();
        assert_eq!(result, vec!["Alpha", "Beta", "Gamma"]);
    }

    // ── Phase management ──

    #[test]
    fn test_set_phase_brainstorm() {
        let (_tmp, manager) = setup_manager();
        let conn = setup_db();

        let result = manager.set_phase("Test", "brainstorm", &conn);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ManuscriptPhase::Brainstorm);
    }

    #[test]
    fn test_phase_progression() {
        let (_tmp, manager) = setup_manager();
        let conn = setup_db();

        manager.set_phase("Test", "brainstorm", &conn).unwrap();
        manager.set_phase("Test", "draft", &conn).unwrap();
        manager.set_phase("Test", "review", &conn).unwrap();

        let phase = ManuscriptManager::get_phase(&conn).unwrap();
        assert_eq!(phase, Some("review".to_string()));
    }

    #[test]
    fn test_invalid_phase_transition() {
        let (_tmp, manager) = setup_manager();
        let conn = setup_db();

        manager.set_phase("Test", "brainstorm", &conn).unwrap();
        let result = manager.set_phase("Test", "published", &conn);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid phase transition"));
    }

    #[test]
    fn test_promote() {
        let (_tmp, manager) = setup_manager();
        let conn = setup_db();

        manager.set_phase("Test", "brainstorm", &conn).unwrap();
        let result = manager.promote("Test", &conn);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ManuscriptPhase::Draft);

        let phase = ManuscriptManager::get_phase(&conn).unwrap();
        assert_eq!(phase, Some("draft".to_string()));
    }

    #[test]
    fn test_promote_from_published_fails() {
        let (_tmp, manager) = setup_manager();
        let conn = setup_db();

        // Cycle through to published
        manager.set_phase("Test", "brainstorm", &conn).unwrap();
        manager.promote("Test", &conn).unwrap(); // draft
        manager.promote("Test", &conn).unwrap(); // review
        manager.promote("Test", &conn).unwrap(); // finalize
        manager.promote("Test", &conn).unwrap(); // published

        let result = manager.promote("Test", &conn);
        assert!(result.is_err());
    }

    #[test]
    fn test_promote_without_phase_set_fails() {
        let (_tmp, manager) = setup_manager();
        let conn = setup_db();

        let result = manager.promote("Test", &conn);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No manuscript phase set"));
    }

    // ── Verify ──

    #[test]
    fn test_verify_valid_manuscript() {
        let (_tmp, manager) = setup_manager();
        let conn = setup_db();

        manager.create("Valid", Some("wld_test")).unwrap();
        manager.set_phase("Valid", "draft", &conn).unwrap();

        let checks = manager.verify("Valid", &conn).unwrap();
        assert!(!checks.is_empty());
        // All checks should pass
        for check in &checks {
            assert!(
                check.starts_with('✓') || check.starts_with('⚠'),
                "Check should pass: {}",
                check
            );
        }
    }

    #[test]
    fn test_verify_nonexistent_manuscript() {
        let (_tmp, manager) = setup_manager();
        let conn = setup_db();

        let checks = manager.verify("Ghost", &conn).unwrap();
        // Should have a failure for missing file
        assert!(checks.iter().any(|c| c.starts_with('✗')));
    }

    // ── UTF-8 safety (CTX-R5) ──

    #[test]
    fn test_utf8_content_handling() {
        let (_tmp, manager) = setup_manager();

        manager.create("UTF8 Test", None).unwrap();

        // Various UTF-8 content
        let content = "Hello 世界 🌍 Nous avons été à Paris — café résumé";
        manager.write_content("UTF8 Test", content).unwrap();

        let read_back = manager.read_content("UTF8 Test").unwrap();
        assert_eq!(read_back, content);
    }

    #[test]
    fn test_validate_utf8_content_valid() {
        assert!(validate_utf8_content("Hello 世界").is_ok());
        assert!(validate_utf8_content("").is_ok());
        assert!(validate_utf8_content("café résumé — naïve").is_ok());
    }

    #[test]
    fn test_export_preserves_utf8() {
        let (_tmp, manager) = setup_manager();

        manager.create("UTF8 Export", None).unwrap();
        manager
            .write_content("UTF8 Export", "# 題目\n\n内容：世界の物語")
            .unwrap();

        let markdown = manager.export("UTF8 Export", "markdown").unwrap();
        assert!(markdown.contains("題目"));
        assert!(markdown.contains("世界の物語"));

        let plain = manager.export("UTF8 Export", "plain").unwrap();
        assert!(plain.contains("題目"));
    }

    // ── Helper functions ──

    #[test]
    fn test_parse_phase() {
        assert_eq!(
            parse_phase("brainstorm").unwrap(),
            ManuscriptPhase::Brainstorm
        );
        assert_eq!(parse_phase("draft").unwrap(), ManuscriptPhase::Draft);
        assert_eq!(parse_phase("review").unwrap(), ManuscriptPhase::Review);
        assert_eq!(parse_phase("finalize").unwrap(), ManuscriptPhase::Finalize);
        assert_eq!(
            parse_phase("published").unwrap(),
            ManuscriptPhase::Published
        );
        assert!(parse_phase("invalid").is_err());
    }

    #[test]
    fn test_phase_to_str() {
        assert_eq!(phase_to_str(&ManuscriptPhase::Brainstorm), "brainstorm");
        assert_eq!(phase_to_str(&ManuscriptPhase::Draft), "draft");
        assert_eq!(phase_to_str(&ManuscriptPhase::Published), "published");
    }

    #[test]
    fn test_next_phase_name() {
        assert_eq!(next_phase_name(&ManuscriptPhase::Brainstorm), "draft");
        assert_eq!(
            next_phase_name(&ManuscriptPhase::Published),
            "none (already published)"
        );
    }

    #[test]
    fn test_manuscript_dir_paths() {
        let tmp = TempDir::new().unwrap();
        let manager = ManuscriptManager::new(tmp.path().to_path_buf());

        assert_eq!(manager.stories_dir(), tmp.path().join("Stories"));
        assert_eq!(
            manager.manuscript_dir("My Novel"),
            tmp.path().join("Stories").join("My Novel")
        );
        assert_eq!(
            manager.manuscript_file("My Novel"),
            tmp.path()
                .join("Stories")
                .join("My Novel")
                .join("manuscript.md")
        );
        assert_eq!(
            manager.metadata_file("My Novel"),
            tmp.path()
                .join("Stories")
                .join("My Novel")
                .join("metadata.json")
        );
    }
}
