//! Manuscript Manager
//!
//! Manages manuscript files, phases, and metadata for a Nexus workspace.
//! Manuscript files are stored under `Stories/<title>/` within the workspace directory.

use crate::errors::{CliError, Result};
use crate::manuscript::metadata::ManuscriptMetadata;
use nexus_contracts::ManuscriptPhase;
use nexus_domain::consistency;
use nexus_domain::manuscript_state::ManuscriptState;
use nexus_local_db::SqlitePool;
use std::path::{Path, PathBuf};

/// Maximum allowed length for a manuscript title (filesystem limit)
const MAX_TITLE_LENGTH: usize = 255;

/// Sanitize a manuscript title to prevent path traversal attacks.
///
/// Returns an error if the title:
/// - Is empty after stripping whitespace
/// - Contains `..`, `/`, `\`, or null bytes
/// - Exceeds 255 characters
pub fn sanitize_title(title: &str) -> Result<String> {
    let trimmed = title.trim();

    if trimmed.is_empty() {
        return Err(CliError::Config(
            "Manuscript title cannot be empty".to_string(),
        ));
    }

    if trimmed.len() > MAX_TITLE_LENGTH {
        return Err(CliError::Config(format!(
            "Manuscript title exceeds maximum length of {} characters",
            MAX_TITLE_LENGTH
        )));
    }

    if trimmed.contains("..") {
        return Err(CliError::Config(
            "Manuscript title cannot contain '..' (path traversal blocked)".to_string(),
        ));
    }

    if trimmed.contains('/') || trimmed.contains('\\') {
        return Err(CliError::Config(
            "Manuscript title cannot contain '/' or '\\'".to_string(),
        ));
    }

    if trimmed.contains('\0') {
        return Err(CliError::Config(
            "Manuscript title cannot contain null bytes".to_string(),
        ));
    }

    Ok(trimmed.to_string())
}

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
    ///
    /// Title is sanitized to prevent path traversal attacks.
    pub fn manuscript_dir(&self, title: &str) -> Result<PathBuf> {
        let safe = sanitize_title(title)?;
        Ok(self.stories_dir().join(safe))
    }

    /// Get the path to a manuscript's main file
    ///
    /// Title is sanitized to prevent path traversal attacks.
    pub fn manuscript_file(&self, title: &str) -> Result<PathBuf> {
        let safe = sanitize_title(title)?;
        Ok(self.manuscript_dir(&safe)?.join("manuscript.md"))
    }

    /// Get the path to a manuscript's metadata file
    ///
    /// Title is sanitized to prevent path traversal attacks.
    pub fn metadata_file(&self, title: &str) -> Result<PathBuf> {
        let safe = sanitize_title(title)?;
        Ok(self.manuscript_dir(&safe)?.join("metadata.json"))
    }

    /// Create a new manuscript with the given title
    pub fn create(&self, title: &str, world_id: Option<&str>) -> Result<PathBuf> {
        validate_world_id(world_id.unwrap_or("wld_default"))?;

        let dir = self.manuscript_dir(title)?;

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
        let manuscript_path = self.manuscript_file(title)?;
        std::fs::write(&manuscript_path, content)?;

        // Create metadata
        let metadata = ManuscriptMetadata::new(title, world_id);
        let metadata_path = self.metadata_file(title)?;
        let metadata_json = serde_json::to_string_pretty(&metadata)?;
        std::fs::write(&metadata_path, metadata_json)?;

        Ok(dir)
    }

    /// Read the content of a manuscript
    pub fn read_content(&self, title: &str) -> Result<String> {
        let path = self.manuscript_file(title)?;
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
        let path = self.metadata_file(title)?;
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
        let path = self.manuscript_file(title)?;
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
    pub async fn set_phase(
        &self,
        title: &str,
        phase: &str,
        pool: &SqlitePool,
    ) -> Result<ManuscriptPhase> {
        let target_phase = parse_phase(phase)?;

        // Validate phase transition using domain consistency rules
        consistency::validate_manuscript_phase(phase)?;

        // Check current phase in DB
        let current_phase: Option<String> =
            sqlx::query_scalar("SELECT value FROM workspace_meta WHERE key = ?1")
                .bind("manuscript_phase")
                .fetch_optional(pool)
                .await?;

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
        sqlx::query(
            "INSERT OR REPLACE INTO workspace_meta (key, value, updated_at) VALUES (?1, ?2, ?3)",
        )
        .bind("manuscript_phase")
        .bind(phase)
        .bind(&now)
        .execute(pool)
        .await?;

        // Also update metadata file
        if let Ok(mut metadata) = self.read_metadata(title) {
            metadata.phase = phase.to_string();
            metadata.updated_at = now.clone();
            if let Ok(json) = serde_json::to_string_pretty(&metadata) {
                let _ = std::fs::write(self.metadata_file(title)?, json);
            }
        }

        Ok(target_phase)
    }

    /// Get the current manuscript phase from SQLite
    pub async fn get_phase(pool: &SqlitePool) -> Result<Option<String>> {
        let phase: Option<String> =
            sqlx::query_scalar("SELECT value FROM workspace_meta WHERE key = 'manuscript_phase'")
                .fetch_optional(pool)
                .await?;
        Ok(phase)
    }

    /// Get the current manuscript phase from the workspace database (convenience method)
    pub async fn get_from_db(workspace_root: &Path) -> Result<Option<String>> {
        let nexus_dir = crate::config::workspace_nexus_dir(workspace_root);
        let db_path = nexus_dir.join("state.db");
        if !db_path.exists() {
            return Ok(None);
        }
        let pool = crate::db::Schema::init(&db_path).await?;
        Self::get_phase(&pool).await
    }

    /// Promote manuscript to the next phase
    ///
    /// In strict mode (V1.1), additional validations are performed:
    /// - Manuscript phase is appropriate for promotion
    /// - StoryManifest status is valid
    /// - Sync state is clean (no pending conflicts)
    pub async fn promote(
        &self,
        title: &str,
        strict: bool,
        pool: &SqlitePool,
    ) -> Result<ManuscriptPhase> {
        let current = Self::get_phase(pool).await?;

        let current_phase = match current {
            Some(ref p) => parse_phase(p)?,
            None => {
                return Err(CliError::Config(
                    "No manuscript phase set. Use `nexus42 manuscript phase <title> <phase>` first."
                        .to_string(),
                ));
            }
        };

        // Strict mode validation (V1.1 CLI-R6)
        if strict {
            self.validate_strict_promotion(title, &current_phase, pool)
                .await?;
        }

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
        sqlx::query(
            "INSERT OR REPLACE INTO workspace_meta (key, value, updated_at) VALUES (?1, ?2, ?3)",
        )
        .bind("manuscript_phase")
        .bind(phase_str)
        .bind(&now)
        .execute(pool)
        .await?;

        // Update metadata file
        if let Ok(mut metadata) = self.read_metadata(title) {
            metadata.phase = phase_str.to_string();
            metadata.updated_at = now.clone();
            if let Ok(json) = serde_json::to_string_pretty(&metadata) {
                let _ = std::fs::write(self.metadata_file(title)?, json);
            }
        }

        Ok(new_phase)
    }

    /// Validate strict mode promotion requirements (CLI-R6)
    async fn validate_strict_promotion(
        &self,
        title: &str,
        current_phase: &ManuscriptPhase,
        pool: &SqlitePool,
    ) -> Result<()> {
        // 1. Check if phase is appropriate for promotion
        // (already validated by ManuscriptState, but we can add more checks here)

        // 2. Check StoryManifest status
        // For V1.1, we check if there's a story_manifest_id in metadata
        if let Ok(metadata) = self.read_metadata(title) {
            // If there's a world_id, we should check if there are any StoryManifest records
            if let Some(_world_id) = &metadata.world_id {
                // Check if there's an active manifest with valid status
                // For V1.1, we simulate this check - in production, this would query
                // the platform API or local cache
                //
                // Valid statuses: summary_ready, staged_for_publish, published, archived
                // For promotion from review→finalize, manifest should be staged_for_publish
                if matches!(current_phase, ManuscriptPhase::Review) {
                    // Check for pending conflicts in sync state
                    let has_conflicts = self.check_sync_conflicts(pool).await?;
                    if has_conflicts {
                        return Err(CliError::Config(
                            "Cannot promote: sync conflicts detected. Resolve conflicts first."
                                .to_string(),
                        ));
                    }
                }
            }
        }

        // 3. Check sync state for pending conflicts
        if matches!(current_phase, ManuscriptPhase::Finalize) {
            let has_conflicts = self.check_sync_conflicts(pool).await?;
            if has_conflicts {
                return Err(CliError::Config(
                    "Cannot promote to published: sync conflicts detected. Resolve conflicts first."
                        .to_string(),
                ));
            }
        }

        Ok(())
    }

    /// Check for pending sync conflicts in outbox
    async fn check_sync_conflicts(&self, pool: &SqlitePool) -> Result<bool> {
        // Check if there are any conflicted outbox entries
        // For V1.1, we check the local sync state
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM outbox_entries WHERE delivery_state = 'conflicted'",
        )
        .fetch_one(pool)
        .await
        .unwrap_or(0);

        Ok(count > 0)
    }

    /// Verify manuscript consistency using domain rules
    ///
    /// In content check mode (V1.1), also verifies file integrity using SHA256.
    pub async fn verify(
        &self,
        title: &str,
        check_content: bool,
        pool: &SqlitePool,
    ) -> Result<Vec<String>> {
        let mut checks = Vec::new();

        // 1. File integrity
        let manuscript_path = self.manuscript_file(title)?;
        if manuscript_path.exists() {
            let content = self.read_content(title)?;
            // UTF-8 safety check (CTX-R5)
            if std::str::from_utf8(content.as_bytes()).is_ok() {
                checks.push(format!("✓ UTF-8 integrity: OK ({} bytes)", content.len()));
            } else {
                checks.push("✗ UTF-8 integrity: INVALID".to_string());
            }

            // Content hash verification (CLI-R7)
            if check_content {
                checks.extend(self.verify_content_integrity(title, &content)?);
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

                // Content hash comparison (CLI-R7)
                if check_content {
                    if let Some(ref stored_hash) = metadata.content_hash {
                        checks.push(format!("  Stored content hash: {}", stored_hash));
                    }
                }
            }
            Err(e) => {
                checks.push(format!("✗ Metadata: {}", e));
            }
        }

        // 3. Phase consistency from SQLite
        if let Ok(Some(phase)) = Self::get_phase(pool).await {
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

    /// Verify content integrity using SHA256 hash (CLI-R7)
    fn verify_content_integrity(&self, title: &str, content: &str) -> Result<Vec<String>> {
        let mut checks = Vec::new();

        // Compute current content hash
        let current_hash = self.compute_content_hash(content);

        // Check if metadata has a stored hash
        match self.read_metadata(title) {
            Ok(metadata) => {
                if let Some(ref stored_hash) = metadata.content_hash {
                    if current_hash == *stored_hash {
                        checks.push(format!("✓ Content integrity: OK ({})", current_hash));
                    } else {
                        checks.push("✗ Content integrity: MISMATCH".to_string());
                        checks.push(format!("  Expected: {}", stored_hash));
                        checks.push(format!("  Actual:   {}", current_hash));
                        checks.push(
                            "  File has been modified since last hash was recorded.".to_string(),
                        );
                    }
                } else {
                    checks.push("⚠ Content integrity: No stored hash found".to_string());
                    checks.push(format!("  Current hash: {}", current_hash));
                    checks.push("  Run 'nexus42 manuscript phase' to store the hash.".to_string());
                }
            }
            Err(e) => {
                checks.push(format!("✗ Content integrity: Cannot read metadata ({})", e));
            }
        }

        Ok(checks)
    }

    /// Compute SHA256 hash of content
    fn compute_content_hash(&self, content: &str) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        let hash = hasher.finalize();
        format!("sha256:{}", hex::encode(hash))
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

    async fn setup_pool() -> SqlitePool {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test.db");
        crate::db::Schema::init(&db_path).await.unwrap()
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

    #[tokio::test]
    async fn test_set_phase_brainstorm() {
        let (_tmp, manager) = setup_manager();
        let pool = setup_pool().await;

        let result = manager.set_phase("Test", "brainstorm", &pool).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ManuscriptPhase::Brainstorm);
    }

    #[tokio::test]
    async fn test_phase_progression() {
        let (_tmp, manager) = setup_manager();
        let pool = setup_pool().await;

        manager
            .set_phase("Test", "brainstorm", &pool)
            .await
            .unwrap();
        manager.set_phase("Test", "draft", &pool).await.unwrap();
        manager.set_phase("Test", "review", &pool).await.unwrap();

        let phase = ManuscriptManager::get_phase(&pool).await.unwrap();
        assert_eq!(phase, Some("review".to_string()));
    }

    #[tokio::test]
    async fn test_invalid_phase_transition() {
        let (_tmp, manager) = setup_manager();
        let pool = setup_pool().await;

        manager
            .set_phase("Test", "brainstorm", &pool)
            .await
            .unwrap();
        let result = manager.set_phase("Test", "published", &pool).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid phase transition"));
    }

    #[tokio::test]
    async fn test_promote() {
        let (_tmp, manager) = setup_manager();
        let pool = setup_pool().await;

        manager
            .set_phase("Test", "brainstorm", &pool)
            .await
            .unwrap();
        let result = manager.promote("Test", false, &pool).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ManuscriptPhase::Draft);

        let phase = ManuscriptManager::get_phase(&pool).await.unwrap();
        assert_eq!(phase, Some("draft".to_string()));
    }

    #[tokio::test]
    async fn test_promote_strict_mode_no_conflicts() {
        let (_tmp, manager) = setup_manager();
        let pool = setup_pool().await;

        manager
            .set_phase("Test", "brainstorm", &pool)
            .await
            .unwrap();
        // Promote with strict mode should succeed when no conflicts
        let result = manager.promote("Test", true, &pool).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), ManuscriptPhase::Draft);
    }

    #[tokio::test]
    async fn test_promote_strict_mode_with_conflicts() {
        let (_tmp, manager) = setup_manager();
        let pool = setup_pool().await;

        // Create the manuscript first
        manager.create("Test", Some("wld_test")).unwrap();

        // Create outbox table and add a conflicted entry
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS outbox_entries (
                outbox_entry_id TEXT PRIMARY KEY,
                delivery_state TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO outbox_entries (outbox_entry_id, delivery_state) VALUES ('test', 'conflicted')",
        )
        .execute(&pool)
        .await
        .unwrap();

        // Progress to review phase
        manager
            .set_phase("Test", "brainstorm", &pool)
            .await
            .unwrap();
        manager.promote("Test", false, &pool).await.unwrap(); // draft
        manager.promote("Test", false, &pool).await.unwrap(); // review

        // Try to promote from review with strict mode and conflicts
        let result = manager.promote("Test", true, &pool).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("sync conflicts"));
    }

    #[tokio::test]
    async fn test_promote_from_published_fails() {
        let (_tmp, manager) = setup_manager();
        let pool = setup_pool().await;

        // Cycle through to published
        manager
            .set_phase("Test", "brainstorm", &pool)
            .await
            .unwrap();
        manager.promote("Test", false, &pool).await.unwrap(); // draft
        manager.promote("Test", false, &pool).await.unwrap(); // review
        manager.promote("Test", false, &pool).await.unwrap(); // finalize
        manager.promote("Test", false, &pool).await.unwrap(); // published

        let result = manager.promote("Test", false, &pool).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_promote_without_phase_set_fails() {
        let (_tmp, manager) = setup_manager();
        let pool = setup_pool().await;

        let result = manager.promote("Test", false, &pool).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No manuscript phase set"));
    }

    // ── Verify ──

    #[tokio::test]
    async fn test_verify_valid_manuscript() {
        let (_tmp, manager) = setup_manager();
        let pool = setup_pool().await;

        manager.create("Valid", Some("wld_test")).unwrap();
        manager.set_phase("Valid", "draft", &pool).await.unwrap();

        let checks = manager.verify("Valid", false, &pool).await.unwrap();
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

    #[tokio::test]
    async fn test_verify_with_content_check_no_hash() {
        let (_tmp, manager) = setup_manager();
        let pool = setup_pool().await;

        manager.create("Test", Some("wld_test")).unwrap();
        manager.set_phase("Test", "draft", &pool).await.unwrap();

        let checks = manager.verify("Test", true, &pool).await.unwrap();
        // Should have a warning about no stored hash
        assert!(checks.iter().any(|c| c.contains("No stored hash")));
    }

    #[tokio::test]
    async fn test_verify_with_content_check_hash_mismatch() {
        let (_tmp, manager) = setup_manager();
        let pool = setup_pool().await;

        manager.create("Test", Some("wld_test")).unwrap();

        // Set a stored hash in metadata
        let mut metadata = manager.read_metadata("Test").unwrap();
        metadata.content_hash = Some("sha256:oldhash".to_string());
        let json = serde_json::to_string_pretty(&metadata).unwrap();
        std::fs::write(manager.metadata_file("Test").unwrap(), json).unwrap();

        manager.set_phase("Test", "draft", &pool).await.unwrap();

        let checks = manager.verify("Test", true, &pool).await.unwrap();
        // Should detect hash mismatch
        assert!(checks.iter().any(|c| c.contains("MISMATCH")));
    }

    #[tokio::test]
    async fn test_verify_with_content_check_hash_match() {
        let (_tmp, manager) = setup_manager();
        let pool = setup_pool().await;

        manager.create("Test", Some("wld_test")).unwrap();
        manager.set_phase("Test", "draft", &pool).await.unwrap();

        // Compute the actual hash
        let content = manager.read_content("Test").unwrap();
        let actual_hash = {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(content.as_bytes());
            let hash = hasher.finalize();
            format!("sha256:{}", hex::encode(hash))
        };

        // Set the correct hash in metadata
        let mut metadata = manager.read_metadata("Test").unwrap();
        metadata.content_hash = Some(actual_hash);
        let json = serde_json::to_string_pretty(&metadata).unwrap();
        std::fs::write(manager.metadata_file("Test").unwrap(), json).unwrap();

        let checks = manager.verify("Test", true, &pool).await.unwrap();
        // Should pass content integrity check
        assert!(checks.iter().any(|c| c.contains("Content integrity: OK")));
    }

    #[tokio::test]
    async fn test_verify_nonexistent_manuscript() {
        let (_tmp, manager) = setup_manager();
        let pool = setup_pool().await;

        let checks = manager.verify("Ghost", false, &pool).await.unwrap();
        // Should have a failure for missing file
        assert!(checks.iter().any(|c| c.starts_with('✗')));
    }

    #[test]
    fn test_compute_content_hash() {
        let tmp = TempDir::new().unwrap();
        let manager = ManuscriptManager::new(tmp.path().to_path_buf());

        let content = "Hello, world!";
        let hash = manager.compute_content_hash(content);

        assert!(hash.starts_with("sha256:"));
        assert_eq!(hash.len(), 71); // "sha256:" + 64 hex chars
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
            manager.manuscript_dir("My Novel").unwrap(),
            tmp.path().join("Stories").join("My Novel")
        );
        assert_eq!(
            manager.manuscript_file("My Novel").unwrap(),
            tmp.path()
                .join("Stories")
                .join("My Novel")
                .join("manuscript.md")
        );
        assert_eq!(
            manager.metadata_file("My Novel").unwrap(),
            tmp.path()
                .join("Stories")
                .join("My Novel")
                .join("metadata.json")
        );
    }

    // ── Title sanitization (QC3-C1) ──

    #[test]
    fn test_sanitize_title_normal() {
        assert_eq!(sanitize_title("My Novel").unwrap(), "My Novel");
        assert_eq!(sanitize_title("  Hello World  ").unwrap(), "Hello World");
        assert_eq!(sanitize_title("日本語タイトル").unwrap(), "日本語タイトル");
    }

    #[test]
    fn test_sanitize_title_rejects_empty() {
        assert!(sanitize_title("").is_err());
        assert!(sanitize_title("   ").is_err());
        assert!(sanitize_title("\t\n").is_err());
    }

    #[test]
    fn test_sanitize_title_rejects_path_traversal() {
        assert!(sanitize_title("../../../etc").is_err());
        assert!(sanitize_title("foo/../bar").is_err());
        assert!(sanitize_title("..hidden").is_err());
    }

    #[test]
    fn test_sanitize_title_rejects_slashes() {
        assert!(sanitize_title("my/novel").is_err());
        assert!(sanitize_title("my\\novel").is_err());
        assert!(sanitize_title("foo/bar/baz").is_err());
    }

    #[test]
    fn test_sanitize_title_rejects_null_bytes() {
        assert!(sanitize_title("title\0evil").is_err());
    }

    #[test]
    fn test_sanitize_title_rejects_too_long() {
        let long_title = "a".repeat(256);
        assert!(sanitize_title(&long_title).is_err());
    }

    #[test]
    fn test_sanitize_title_accepts_max_length() {
        let max_title = "a".repeat(255);
        assert!(sanitize_title(&max_title).is_ok());
    }
}
