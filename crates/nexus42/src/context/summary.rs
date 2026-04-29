//! Summary generation from local manuscript files.
//!
//! V1.0 uses basic extraction: title (from front-matter or first heading),
//! chapter list (from heading structure), word count, and opening excerpt.
//! No LLM call required.
//!
//! V1.1 adds safety constraints (file size limit, path traversal validation,
//! UTF-8 truncation safety) as defined in the knowledge SSOT §9.
//!
//! NOTE: The types and functions in this module are not yet wired into the
//! CLI command pipeline. They will be used by the context assembly workflow
//! once the daemon context endpoint is integrated. Suppressing dead-code
//! warnings to keep the implementation ready for that integration.

#![allow(dead_code)]

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tracing::warn;

/// Maximum summary text length (characters).
const MAX_SUMMARY_CHARS: usize = 4096;

/// Maximum opening excerpt length (characters).
const MAX_EXCERPT_CHARS: usize = 500;

/// Default maximum file size (10 MB).
const DEFAULT_MAX_FILE_SIZE: usize = 10 * 1024 * 1024;

/// Default maximum recursion depth.
const DEFAULT_MAX_DEPTH: usize = 10;

/// Default file extensions to include.
const DEFAULT_INCLUDE_EXTENSIONS: &[&str] = &["md", "txt"];

/// Configuration for summary generation.
///
/// V1.1 safety constraints (CTX-R2): configurable file size limit,
/// recursion depth, and file extension whitelist.
#[derive(Debug, Clone)]
pub struct SummaryConfig {
    /// Maximum file size in bytes (default: 10 MB).
    pub max_file_size: usize,
    /// Maximum recursion depth for directory scanning (default: 10).
    pub max_depth: usize,
    /// File extensions to include (default: `["md", "txt"]`).
    pub include_extensions: Vec<String>,
}

impl Default for SummaryConfig {
    fn default() -> Self {
        Self {
            max_file_size: DEFAULT_MAX_FILE_SIZE,
            max_depth: DEFAULT_MAX_DEPTH,
            include_extensions: DEFAULT_INCLUDE_EXTENSIONS
                .iter()
                .map(std::string::ToString::to_string)
                .collect(),
        }
    }
}

impl SummaryConfig {
    /// Load configuration from environment variables.
    ///
    /// Reads `NEXUS_CONTEXT_MAX_FILE_SIZE` to override the default file size limit.
    /// Falls back to defaults for unconfigured fields.
    pub fn from_env() -> Self {
        let mut config = Self::default();

        if let Ok(size_str) = env::var("NEXUS_CONTEXT_MAX_FILE_SIZE") {
            if let Ok(size) = size_str.parse::<usize>() {
                config.max_file_size = size;
            } else {
                warn!(
                    "Invalid NEXUS_CONTEXT_MAX_FILE_SIZE value '{}', using default {}",
                    size_str, DEFAULT_MAX_FILE_SIZE
                );
            }
        }

        config
    }
}

/// Errors that can occur during summary generation.
///
/// V1.1 safety constraints (CTX-R3): path traversal validation errors.
#[derive(Debug, Error)]
pub enum SummaryError {
    /// Path traversal attempt detected.
    #[error(
        "Path traversal detected: '{}' escapes base '{}'",
        attempted_path,
        base_path
    )]
    PathTraversal {
        /// The path that attempted traversal.
        attempted_path: String,
        /// The base directory that was escaped.
        base_path: String,
    },

    /// Invalid path (canonicalization failed).
    #[error("Invalid path: {0}")]
    InvalidPath(String),

    /// I/O error during summary generation.
    #[error("I/O error: {}", source)]
    IoError {
        #[source]
        source: std::io::Error,
    },
}

impl From<std::io::Error> for SummaryError {
    fn from(source: std::io::Error) -> Self {
        Self::IoError { source }
    }
}

/// A manuscript file discovered by scanning.
#[derive(Debug, Clone)]
pub struct ManuscriptFile {
    /// Relative path from the manuscript root.
    pub relative_path: PathBuf,
    /// File content.
    pub content: String,
}

/// Result of summary generation.
#[derive(Debug, Clone)]
pub struct GeneratedSummary {
    /// Title extracted from front-matter or first heading.
    pub title: Option<String>,
    /// List of chapter titles (from heading structure).
    pub chapters: Vec<String>,
    /// Total word count across all manuscript files.
    pub word_count: usize,
    /// Opening excerpt (first N chars of first chapter body).
    pub opening_excerpt: String,
    /// Final summary text suitable for `StoryManifest.summary_text`.
    pub summary_text: String,
}

/// Summary generator for local manuscript files.
///
/// V1.1 safety: includes file size limit, path traversal validation,
/// and UTF-8 truncation safety.
pub struct SummaryGenerator {
    /// Path to the manuscript root (e.g., `Stories/<world_ref>/`).
    manuscript_root: PathBuf,
    /// Maximum summary length in characters.
    max_summary_chars: usize,
    /// Maximum excerpt length in characters.
    max_excerpt_chars: usize,
    /// Configuration for safety constraints.
    config: SummaryConfig,
}

impl SummaryGenerator {
    /// Create a new summary generator for the given manuscript root.
    #[must_use]
    pub fn new(manuscript_root: PathBuf) -> Self {
        Self {
            manuscript_root,
            max_summary_chars: MAX_SUMMARY_CHARS,
            max_excerpt_chars: MAX_EXCERPT_CHARS,
            config: SummaryConfig::default(),
        }
    }

    /// Create a new summary generator with custom configuration.
    #[must_use]
    pub const fn with_config(manuscript_root: PathBuf, config: SummaryConfig) -> Self {
        Self {
            manuscript_root,
            max_summary_chars: MAX_SUMMARY_CHARS,
            max_excerpt_chars: MAX_EXCERPT_CHARS,
            config,
        }
    }

    /// Set maximum file size limit (in bytes).
    ///
    /// This is a convenience builder method that modifies the config.
    /// Deprecated: prefer using `with_config()` instead.
    #[deprecated(note = "Use with_config() instead")]
    #[must_use]
    pub fn with_max_file_size(self, max_file_size: Option<u64>) -> Self {
        Self {
            config: SummaryConfig {
                max_file_size: usize::try_from(
                    max_file_size.unwrap_or(DEFAULT_MAX_FILE_SIZE as u64),
                )
                .unwrap_or(usize::MAX),
                ..self.config
            },
            ..self
        }
    }

    /// Scan the manuscript directory for recognized file types.
    ///
    /// Walks `Stories/<world_ref>/` for files matching configured extensions.
    /// Ignores `References/` tree and non-recognized extensions.
    /// Enforces file size limit and path traversal validation (V1.1 safety).
    ///
    /// # Errors
    ///
    /// Returns a `SummaryError` if directory traversal fails or path validation fails.
    pub fn scan_manuscript_dir(&self) -> Result<Vec<ManuscriptFile>, SummaryError> {
        let mut files = Vec::new();
        if !self.manuscript_root.exists() {
            return Ok(files);
        }
        self.scan_recursive(&self.manuscript_root, 0, &mut files)?;
        Ok(files)
    }

    fn scan_recursive(
        &self,
        dir: &Path,
        depth: usize,
        files: &mut Vec<ManuscriptFile>,
    ) -> Result<(), SummaryError> {
        // V1.1 safety: enforce max depth (CTX-R2)
        if depth > self.config.max_depth {
            warn!(
                "Max recursion depth {} exceeded at '{}'",
                self.config.max_depth,
                dir.display()
            );
            return Ok(());
        }

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                // Skip References/ tree
                if let Some(name) = path.file_name() {
                    if name == "References" {
                        continue;
                    }
                }
                // V1.1 safety: validate path before recursing (CTX-R3)
                let validated_path = validate_path_within_base(&path, &self.manuscript_root)?;
                self.scan_recursive(&validated_path, depth + 1, files)?;
            } else {
                let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");

                // V1.1 safety: check extension whitelist (CTX-R2)
                if !self
                    .config
                    .include_extensions
                    .contains(&extension.to_string())
                {
                    continue;
                }

                // V1.1 safety: validate path before processing (CTX-R3)
                let validated_path = validate_path_within_base(&path, &self.manuscript_root)?;

                // V1.1 safety: check file size limit (CTX-R2)
                let metadata = fs::metadata(&validated_path)?;
                let file_size = usize::try_from(metadata.len()).unwrap_or(usize::MAX);
                if file_size > self.config.max_file_size {
                    let relative = validated_path
                        .strip_prefix(&self.manuscript_root)
                        .unwrap_or(&validated_path);
                    warn!(
                        "Skipping file {} ({} bytes) exceeding limit {} bytes",
                        relative.display(),
                        file_size,
                        self.config.max_file_size
                    );
                    continue;
                }

                let relative = validated_path
                    .strip_prefix(&self.manuscript_root)
                    .unwrap_or(&validated_path)
                    .to_path_buf();
                let content = fs::read_to_string(&validated_path)?;
                files.push(ManuscriptFile {
                    relative_path: relative,
                    content,
                });
            }
        }
        Ok(())
    }

    /// Generate a basic summary from manuscript files.
    ///
    /// # Errors
    ///
    /// Returns a `SummaryError` if scanning fails.
    pub fn generate(&self) -> Result<GeneratedSummary, SummaryError> {
        let files = self.scan_manuscript_dir()?;
        let mut title = None;
        let mut chapters = Vec::new();
        let mut total_words = 0;
        let mut first_body_excerpt: Option<String> = None;

        for file in &files {
            let words = file.content.split_whitespace().count();
            total_words += words;

            // Extract title from first file with a heading
            if title.is_none() {
                title = extract_title(&file.content);
            }

            // Extract chapter headings from all files
            extract_chapters(&file.content, &mut chapters);

            // Extract opening excerpt from first file with body text
            if first_body_excerpt.is_none() {
                first_body_excerpt = extract_opening_excerpt(&file.content, self.max_excerpt_chars);
            }
        }

        // Build summary text
        let mut summary_parts: Vec<String> = Vec::new();
        if let Some(t) = &title {
            summary_parts.push(format!("Title: {t}"));
        }
        if !chapters.is_empty() {
            let chapter_list: Vec<String> = chapters
                .iter()
                .enumerate()
                .map(|(i, ch)| format!("  {}. {}", i + 1, ch))
                .collect();
            summary_parts.push("Chapters:".to_string());
            summary_parts.extend(chapter_list);
        }
        summary_parts.push(format!("Word count: {total_words}"));
        if let Some(excerpt) = &first_body_excerpt {
            summary_parts.push(format!("Opening: {excerpt}"));
        }

        let mut summary_text = summary_parts.join("\n");
        // V1.1 safety: truncate to max length with UTF-8 safety (CTX-R5)
        if summary_text.len() > self.max_summary_chars {
            let truncate_len = self.max_summary_chars.saturating_sub(3);
            // Use UTF-8 safe truncation
            let truncated = truncate_at_char_boundary(&summary_text, truncate_len);
            summary_text = truncated.to_string();
            summary_text.push_str("...");
        }

        Ok(GeneratedSummary {
            title,
            chapters,
            word_count: total_words,
            opening_excerpt: first_body_excerpt.unwrap_or_default(),
            summary_text,
        })
    }
}

/// Validate that a path stays within the base directory.
///
/// V1.1 safety constraint (CTX-R3): prevents path traversal attacks
/// and symlink escape by canonicalizing both paths and checking containment.
///
/// Returns the canonicalized path on success, or `SummaryError::PathTraversal`
/// if the resolved path escapes the base directory.
///
/// # Errors
///
/// Returns `SummaryError::InvalidPath` if path canonicalization fails.
/// Returns `SummaryError::PathTraversal` if the path escapes the base directory.
pub fn validate_path_within_base(path: &Path, base: &Path) -> Result<PathBuf, SummaryError> {
    // Canonicalize both paths to resolve symlinks and normalize
    let canonical_path = path
        .canonicalize()
        .map_err(|e| SummaryError::InvalidPath(format!("{}: {}", path.display(), e)))?;

    let canonical_base = base
        .canonicalize()
        .map_err(|e| SummaryError::InvalidPath(format!("{}: {}", base.display(), e)))?;

    // Check that canonical_path starts with canonical_base
    if !canonical_path.starts_with(&canonical_base) {
        return Err(SummaryError::PathTraversal {
            attempted_path: path.display().to_string(),
            base_path: base.display().to_string(),
        });
    }

    Ok(canonical_path)
}

/// Truncate a string at a valid UTF-8 character boundary.
///
/// V1.1 safety constraint (CTX-R5): ensures truncation does not split
/// multi-byte UTF-8 characters (CJK 3-byte, emoji 4-byte).
///
/// Walks backwards from `max_bytes` to find the last valid character boundary.
/// If the string is shorter than `max_bytes`, returns the entire string.
#[must_use]
pub fn truncate_at_char_boundary(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }

    // Find the nearest valid character boundary by walking backwards
    let mut pos = max_bytes;
    while pos > 0 && !s.is_char_boundary(pos) {
        pos -= 1;
    }

    // If we found a valid boundary, return the truncated slice
    // If pos == 0, the string starts with a multi-byte char that exceeds max_bytes
    // In that case, return empty string to avoid splitting the first char
    &s[..pos]
}

/// Extract title from markdown content (front-matter `title:` or first `#` heading).
fn extract_title(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        // Check YAML front-matter title
        if let Some(rest) = trimmed.strip_prefix("title:") {
            let value = rest.trim();
            // Remove surrounding quotes if present
            let cleaned = value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
                .unwrap_or(value);
            if !cleaned.is_empty() {
                return Some(cleaned.to_string());
            }
        }
        // Check first Markdown heading (but not inside front-matter)
        if trimmed.starts_with("# ") && !trimmed.starts_with("# #") {
            let value = trimmed["# ".len()..].trim().to_string();
            if !value.is_empty() {
                return Some(value);
            }
        }
    }
    None
}

/// Extract chapter titles from Markdown headings (`## ` level).
fn extract_chapters(content: &str, chapters: &mut Vec<String>) {
    let mut in_front_matter = false;
    for line in content.lines() {
        let trimmed = line.trim();
        // Track front-matter boundaries
        if trimmed == "---" {
            in_front_matter = !in_front_matter;
            continue;
        }
        if in_front_matter {
            continue;
        }
        // Extract `## ` headings as chapters
        if let Some(rest) = trimmed.strip_prefix("## ") {
            let chapter_title = rest.trim().to_string();
            if !chapter_title.is_empty() {
                chapters.push(chapter_title);
            }
        }
    }
}

/// Extract opening excerpt from the first body text (after front-matter and headings).
///
/// V1.1 safety: uses UTF-8 safe truncation (CTX-R5).
fn extract_opening_excerpt(content: &str, max_chars: usize) -> Option<String> {
    let mut body_lines: Vec<&str> = Vec::new();
    let mut in_front_matter = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "---" {
            in_front_matter = !in_front_matter;
            continue;
        }
        if in_front_matter {
            continue;
        }
        // Skip heading lines
        if trimmed.starts_with('#') {
            continue;
        }
        // Skip empty lines before we have content
        if trimmed.is_empty() && body_lines.is_empty() {
            continue;
        }
        body_lines.push(trimmed);
    }

    if body_lines.is_empty() {
        return None;
    }

    let body_text = body_lines.join(" ");
    if body_text.len() <= max_chars {
        Some(body_text)
    } else {
        // V1.1 safety: truncate at UTF-8 char boundary (CTX-R5)
        // First try to break at a space near max_chars
        let mut end = max_chars;
        // Find valid char boundary
        while end > 0 && !body_text.is_char_boundary(end) {
            end -= 1;
        }
        // Try to break at a space for cleaner truncation
        if let Some(space_pos) = body_text[..end].rfind(' ') {
            end = space_pos;
        }
        // Ensure end is still a valid char boundary
        while end > 0 && !body_text.is_char_boundary(end) {
            end -= 1;
        }
        Some(format!("{}...", &body_text[..end]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    /// Helper: create a temp manuscript directory with files.
    fn create_test_manuscript(files: &[(&str, &str)]) -> TempDir {
        let tmp = TempDir::new().expect("temp dir");
        for (path, content) in files {
            let full = tmp.path().join(path);
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent).expect("create parent dir");
            }
            let mut f = fs::File::create(&full).expect("create file");
            f.write_all(content.as_bytes()).expect("write file");
        }
        tmp
    }

    // ========================================
    // Task 1 Tests: File Size Limit (CTX-R2)
    // ========================================

    #[test]
    fn summary_config_default_values() {
        let config = SummaryConfig::default();
        assert_eq!(config.max_file_size, DEFAULT_MAX_FILE_SIZE);
        assert_eq!(config.max_depth, DEFAULT_MAX_DEPTH);
        assert_eq!(config.include_extensions, vec!["md", "txt"]);
    }

    #[test]
    fn summary_config_from_env_override() {
        // Set environment variable
        env::set_var("NEXUS_CONTEXT_MAX_FILE_SIZE", "5000");
        let config = SummaryConfig::from_env();
        assert_eq!(config.max_file_size, 5000);
        assert_eq!(config.max_depth, DEFAULT_MAX_DEPTH); // Unchanged
        env::remove_var("NEXUS_CONTEXT_MAX_FILE_SIZE");
    }

    #[test]
    fn summary_config_from_env_invalid_value() {
        env::set_var("NEXUS_CONTEXT_MAX_FILE_SIZE", "invalid");
        let config = SummaryConfig::from_env();
        // Should fall back to default
        assert_eq!(config.max_file_size, DEFAULT_MAX_FILE_SIZE);
        env::remove_var("NEXUS_CONTEXT_MAX_FILE_SIZE");
    }

    #[test]
    fn summary_config_from_env_unset() {
        env::remove_var("NEXUS_CONTEXT_MAX_FILE_SIZE");
        let config = SummaryConfig::from_env();
        assert_eq!(config.max_file_size, DEFAULT_MAX_FILE_SIZE);
    }

    #[test]
    fn scan_skips_large_files_with_config() {
        let small_content = "Small file content";
        let large_content = "a".repeat(10_000); // 10KB file

        let tmp =
            create_test_manuscript(&[("small.md", small_content), ("large.md", &large_content)]);

        let config = SummaryConfig {
            max_file_size: 5000,
            ..SummaryConfig::default()
        };
        let gen = SummaryGenerator::with_config(tmp.path().to_path_buf(), config);

        let files = gen.scan_manuscript_dir().expect("scan should succeed");

        // Should only include small.md (17 bytes), not large.md (10001 bytes)
        assert_eq!(files.len(), 1);
        assert!(files[0].relative_path.ends_with("small.md"));
    }

    #[test]
    fn scan_includes_all_files_when_no_limit() {
        let small_content = "Small file content";
        let large_content = "a".repeat(10_000); // 10KB file

        let tmp =
            create_test_manuscript(&[("small.md", small_content), ("large.md", &large_content)]);

        // Use default config (10MB limit, should include both)
        let gen = SummaryGenerator::new(tmp.path().to_path_buf());

        let files = gen.scan_manuscript_dir().expect("scan should succeed");

        // Should include both files
        assert_eq!(files.len(), 2);
        assert!(files.iter().any(|f| f.relative_path.ends_with("small.md")));
        assert!(files.iter().any(|f| f.relative_path.ends_with("large.md")));
    }

    #[test]
    fn max_file_size_limit_boundary() {
        // Test file exactly at the limit boundary is included
        let content = "a".repeat(1000); // Exactly 1000 bytes

        let tmp = create_test_manuscript(&[
            ("exact.md", &content),
            ("over.md", &"a".repeat(1001)), // 1001 bytes, just over limit
        ]);

        let config = SummaryConfig {
            max_file_size: 1000,
            ..SummaryConfig::default()
        };
        let gen = SummaryGenerator::with_config(tmp.path().to_path_buf(), config);

        let files = gen.scan_manuscript_dir().expect("scan should succeed");

        // Should include exact.md (1000 bytes) but skip over.md (1001 bytes)
        assert_eq!(files.len(), 1);
        assert!(files[0].relative_path.ends_with("exact.md"));
    }

    #[test]
    fn scan_with_custom_extensions() {
        let tmp = create_test_manuscript(&[
            ("story.md", "# Story"),
            ("notes.txt", "Notes"),
            ("data.json", "{}"),
        ]);

        let config = SummaryConfig {
            include_extensions: vec!["md".to_string(), "json".to_string()],
            ..SummaryConfig::default()
        };
        let gen = SummaryGenerator::with_config(tmp.path().to_path_buf(), config);

        let files = gen.scan_manuscript_dir().expect("scan should succeed");

        // Should include .md and .json, but not .txt
        assert_eq!(files.len(), 2);
        assert!(files.iter().any(|f| f.relative_path.ends_with("story.md")));
        assert!(files.iter().any(|f| f.relative_path.ends_with("data.json")));
    }

    #[test]
    fn scan_enforces_max_depth() {
        let tmp = create_test_manuscript(&[(
            "level1/level2/level3/level4/level5/file.md",
            "# Deep file",
        )]);

        let config = SummaryConfig {
            max_depth: 3,
            ..SummaryConfig::default()
        };
        let gen = SummaryGenerator::with_config(tmp.path().to_path_buf(), config);

        let files = gen.scan_manuscript_dir().expect("scan should succeed");

        // Should not reach depth 5 (exceeds max_depth=3)
        assert_eq!(files.len(), 0);
    }

    // ========================================
    // Task 2 Tests: Path Traversal Validation (CTX-R3)
    // ========================================

    #[test]
    fn validate_path_within_base_accepts_valid_path() {
        let tmp = TempDir::new().expect("temp dir");
        let base = tmp.path();
        let valid_path = base.join("subdir/file.md");

        // Create the path
        fs::create_dir_all(valid_path.parent().unwrap()).expect("create dir");
        fs::write(&valid_path, "content").expect("write file");

        let result = validate_path_within_base(&valid_path, base);
        assert!(result.is_ok());
        let canonical = result.expect("should succeed");
        assert!(canonical.starts_with(base.canonicalize().expect("canonicalize base")));
    }

    #[test]
    fn validate_path_within_base_rejects_traversal() {
        // Note: We can't actually test path traversal with ".." for non-existent paths
        // because canonicalization will fail. Instead, we test with a real path that
        // is outside the base.
        let tmp = TempDir::new().expect("temp dir");
        let base = tmp.path();

        // Create a file outside the base
        let outside_tmp = TempDir::new().expect("outside temp dir");
        let outside_file = outside_tmp.path().join("outside.md");
        fs::write(&outside_file, "content").expect("write file");

        // Try to validate a path that's outside the base
        let result = validate_path_within_base(&outside_file, base);
        assert!(result.is_err());
        match result {
            Err(SummaryError::PathTraversal {
                attempted_path,
                base_path: _,
            }) => {
                assert!(attempted_path.contains("outside"));
            }
            Err(e) => panic!("Expected PathTraversal error, got {e:?}"),
            Ok(_) => panic!("Expected error, got success"),
        }
    }

    #[test]
    fn validate_path_within_base_rejects_symlink_escape() {
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;

            let tmp = TempDir::new().expect("temp dir");
            let base = tmp.path();

            // Create a symlink that points outside the base
            let outside_tmp = TempDir::new().expect("outside temp dir");
            let outside_file = outside_tmp.path().join("outside.md");
            fs::write(&outside_file, "outside content").expect("write outside file");

            let symlink_path = base.join("escape.md");
            symlink(&outside_file, &symlink_path).expect("create symlink");

            let result = validate_path_within_base(&symlink_path, base);
            assert!(result.is_err());
            match result {
                Err(SummaryError::PathTraversal { .. }) => {}
                Err(e) => panic!("Expected PathTraversal error, got {e:?}"),
                Ok(_) => panic!("Expected error, got success"),
            }
        }

        #[cfg(not(unix))]
        {
            // Skip on non-Unix platforms
        }
    }

    #[test]
    fn validate_path_within_base_handles_missing_path() {
        let tmp = TempDir::new().expect("temp dir");
        let base = tmp.path();
        let missing_path = base.join("nonexistent/file.md");

        let result = validate_path_within_base(&missing_path, base);
        // Should return InvalidPath error because canonicalization fails
        assert!(result.is_err());
        match result {
            Err(SummaryError::InvalidPath(_)) => {}
            Err(e) => panic!("Expected InvalidPath error, got {e:?}"),
            Ok(_) => panic!("Expected error, got success"),
        }
    }

    #[test]
    fn scan_with_traversal_attempt_fails() {
        let tmp = create_test_manuscript(&[("safe.md", "# Safe")]);
        let base = tmp.path();

        // Try to scan with a path that attempts traversal
        // This test verifies that scan_recursive validates paths
        let gen = SummaryGenerator::new(base.to_path_buf());

        // If we create a symlink pointing outside, scan should reject it
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;

            let outside_tmp = TempDir::new().expect("outside temp dir");
            let outside_file = outside_tmp.path().join("outside.md");
            fs::write(&outside_file, "outside").expect("write outside file");

            let symlink_path = base.join("escape.md");
            symlink(&outside_file, &symlink_path).expect("create symlink");

            let result = gen.scan_manuscript_dir();
            // The symlink should be validated and rejected (PathTraversal error)
            // But since validation happens before reading, the error might be caught
            match result {
                Ok(files) => {
                    // If scan succeeded, only the safe file should be included
                    // The escaped symlink should have been rejected during validation
                    assert!(
                        files.len() <= 1,
                        "Expected at most 1 file, got {}",
                        files.len()
                    );
                    // If there's a file, it should be the safe one
                    if !files.is_empty() {
                        assert!(files[0].relative_path.ends_with("safe.md"));
                    }
                }
                Err(SummaryError::PathTraversal { .. }) => {
                    // This is also acceptable - symlink was detected and rejected
                }
                Err(e) => {
                    // Other errors are acceptable
                    eprintln!("Scan returned error (acceptable): {e}");
                }
            }
        }

        #[cfg(not(unix))]
        {
            // Skip on non-Unix platforms
        }
    }

    // ========================================
    // Task 3 Tests: UTF-8 Truncation Safety (CTX-R5)
    // ========================================

    #[test]
    fn truncate_at_char_boundary_ascii() {
        let s = "Hello world";
        let truncated = truncate_at_char_boundary(s, 5);
        assert_eq!(truncated, "Hello");
        assert!(truncated.is_char_boundary(truncated.len()));
    }

    #[test]
    fn truncate_at_char_boundary_cjk() {
        // Chinese characters are 3 bytes each
        // "你好世界" = 12 bytes (4 chars * 3 bytes)
        let s = "你好世界更多文字";
        // Truncate at 10 bytes - should stop at 9 bytes (3 chars)
        let truncated = truncate_at_char_boundary(s, 10);
        assert_eq!(truncated, "你好世");
        assert!(truncated.is_char_boundary(truncated.len()));
    }

    #[test]
    fn truncate_at_char_boundary_emoji() {
        // Emoji are 4 bytes each
        // "😊🎉🎊" = 12 bytes (3 chars * 4 bytes)
        let s = "😊🎉🎊🔥💡";
        // Truncate at 10 bytes - should stop at 8 bytes (2 chars)
        let truncated = truncate_at_char_boundary(s, 10);
        assert_eq!(truncated, "😊🎉");
        assert!(truncated.is_char_boundary(truncated.len()));
    }

    #[test]
    fn truncate_at_char_boundary_mixed() {
        // Mixed ASCII (1 byte), CJK (3 bytes), emoji (4 bytes)
        // "Hello你好😊world世界🎉"
        // Bytes: H(1) e(2) l(3) l(4) o(5) 你(6-8) 好(9-11) 😊(12-15) ...
        let s = "Hello你好😊world世界🎉";

        // Position 5 is the end of "Hello"
        let truncated1 = truncate_at_char_boundary(s, 5);
        assert_eq!(truncated1, "Hello");

        // Position 7 is inside "你" (bytes 5-7), should stop at 5
        let truncated2 = truncate_at_char_boundary(s, 7);
        assert_eq!(truncated2, "Hello");

        // Position 8 is the end of "你", should include "Hello你"
        let truncated3 = truncate_at_char_boundary(s, 8);
        assert_eq!(truncated3, "Hello你");

        // Position 9 is the start of "好", should include "Hello你"
        let truncated4 = truncate_at_char_boundary(s, 9);
        assert_eq!(truncated4, "Hello你");

        assert!(truncated1.is_char_boundary(truncated1.len()));
        assert!(truncated2.is_char_boundary(truncated2.len()));
        assert!(truncated3.is_char_boundary(truncated3.len()));
        assert!(truncated4.is_char_boundary(truncated4.len()));
    }

    #[test]
    fn truncate_at_char_boundary_empty_string() {
        let s = "";
        let truncated = truncate_at_char_boundary(s, 10);
        assert_eq!(truncated, "");
        assert!(truncated.is_char_boundary(truncated.len()));
    }

    #[test]
    fn truncate_at_char_boundary_exact_boundary() {
        // Create a string where max_bytes lands exactly on a char boundary
        let s = "Hello你好世界"; // 5 + 3*3 = 14 bytes
                                 // "你" is bytes 5-7, "好" is bytes 8-10, "世" is bytes 11-13, "界" is bytes 14-16
        let truncated = truncate_at_char_boundary(s, 8); // Exact boundary at end of "你"
        assert_eq!(truncated, "Hello你");
        assert!(truncated.is_char_boundary(truncated.len()));
    }

    #[test]
    fn truncate_at_char_boundary_beyond_length() {
        let s = "Short";
        let truncated = truncate_at_char_boundary(s, 100);
        assert_eq!(truncated, "Short"); // Returns entire string
        assert!(truncated.is_char_boundary(truncated.len()));
    }

    #[test]
    fn truncate_at_char_boundary_first_char_exceeds_limit() {
        // Edge case: first character exceeds max_bytes
        // Emoji is 4 bytes, max_bytes=3
        let s = "😊world";
        let truncated = truncate_at_char_boundary(s, 3);
        // Should return empty string (can't include first char without splitting)
        assert_eq!(truncated, "");
        assert!(truncated.is_char_boundary(truncated.len()));
    }

    #[test]
    fn truncate_at_char_boundary_near_boundary() {
        // Test truncation near multi-byte char boundary
        let s = "Hello你好😊世界";
        // Position 7 is inside "你" (bytes 5-7), should stop at 5
        let truncated1 = truncate_at_char_boundary(s, 7);
        assert_eq!(truncated1, "Hello");

        // Position 11 is inside "😊" (bytes 8-11), should stop at 8
        let truncated2 = truncate_at_char_boundary(s, 11);
        assert_eq!(truncated2, "Hello你好");

        assert!(truncated1.is_char_boundary(truncated1.len()));
        assert!(truncated2.is_char_boundary(truncated2.len()));
    }

    #[test]
    fn summary_truncation_with_emoji() {
        // Use emoji (4-byte UTF-8 characters) to force truncation at non-char boundary
        let emoji_content = format!(
            "# Title\n\n{}\n",
            "这是一段中文文字 mixed with emoji 😊🎉🎊 and more text here to exceed limit. "
                .repeat(100)
        );
        let tmp = create_test_manuscript(&[("emoji.md", &emoji_content)]);
        let gen = SummaryGenerator::new(tmp.path().to_path_buf());
        let summary = gen.generate().expect("generate should succeed");
        assert!(summary.summary_text.len() <= 4096);
        // Verify the string is valid UTF-8
        assert!(summary
            .summary_text
            .is_char_boundary(summary.summary_text.len()));
    }

    #[test]
    fn summary_truncation_with_cjk() {
        // Use CJK characters (3-byte UTF-8)
        let cjk_content = format!(
            "# 标题\n\n{}\n",
            "中文字符测试内容，这段文字会被截断。每一行都包含足够的内容来超过限制。".repeat(200)
        );
        let tmp = create_test_manuscript(&[("cjk.md", &cjk_content)]);
        let gen = SummaryGenerator::new(tmp.path().to_path_buf());
        let summary = gen.generate().expect("generate should succeed");
        assert!(summary.summary_text.len() <= 4096);
        assert!(summary
            .summary_text
            .is_char_boundary(summary.summary_text.len()));
    }

    #[test]
    fn summary_truncation_with_mixed_multibyte() {
        // Mixed: ASCII (1 byte), CJK (3 bytes), emoji (4 bytes)
        let mixed_content = format!(
            "# Mixed Title\n\n{}\n",
            "English 日本語 한국어 emoji 😊🎉 text with mixed encoding types.".repeat(150)
        );
        let tmp = create_test_manuscript(&[("mixed.md", &mixed_content)]);
        let gen = SummaryGenerator::new(tmp.path().to_path_buf());
        let summary = gen.generate().expect("generate should succeed");
        assert!(summary.summary_text.len() <= 4096);
        assert!(summary
            .summary_text
            .is_char_boundary(summary.summary_text.len()));
    }

    #[test]
    fn opening_excerpt_truncation_with_emoji() {
        let emoji_body = "😊🎉🎊".repeat(200);
        let content = format!("# Title\n\n{emoji_body}");
        let excerpt = extract_opening_excerpt(&content, 100).expect("should have excerpt");
        assert!(excerpt.len() <= 103);
        assert!(excerpt.is_char_boundary(excerpt.len()));
    }

    #[test]
    fn opening_excerpt_truncation_with_cjk() {
        let cjk_body = "中文测试".repeat(100);
        let content = format!("# 标题\n\n{cjk_body}");
        let excerpt = extract_opening_excerpt(&content, 50).expect("should have excerpt");
        assert!(excerpt.len() <= 53);
        assert!(excerpt.is_char_boundary(excerpt.len()));
    }

    // ========================================
    // Existing Tests (updated for new API)
    // ========================================

    #[test]
    fn scan_finds_markdown_files() {
        let tmp = create_test_manuscript(&[
            ("chapter-01.md", "# Chapter One\n\nHello world."),
            ("chapter-02.md", "# Chapter Two\n\nGoodbye world."),
            ("notes.txt", "Some notes."),
            ("image.png", "not text"),
        ]);
        let gen = SummaryGenerator::new(tmp.path().to_path_buf());
        let files = gen.scan_manuscript_dir().expect("scan should succeed");
        assert_eq!(files.len(), 3); // 2 md + 1 txt, not png
        assert!(files
            .iter()
            .any(|f| f.relative_path.ends_with("chapter-01.md")));
        assert!(files
            .iter()
            .any(|f| f.relative_path.ends_with("chapter-02.md")));
        assert!(files.iter().any(|f| f.relative_path.ends_with("notes.txt")));
    }

    #[test]
    fn scan_skips_references_directory() {
        let tmp = create_test_manuscript(&[
            ("chapter-01.md", "# Chapter One"),
            ("References/research.md", "# Research Notes"),
        ]);
        let gen = SummaryGenerator::new(tmp.path().to_path_buf());
        let files = gen.scan_manuscript_dir().expect("scan should succeed");
        assert_eq!(files.len(), 1);
        assert!(files[0].relative_path.ends_with("chapter-01.md"));
    }

    #[test]
    fn scan_empty_directory() {
        let tmp = TempDir::new().expect("temp dir");
        let gen = SummaryGenerator::new(tmp.path().to_path_buf());
        let files = gen.scan_manuscript_dir().expect("scan should succeed");
        assert!(files.is_empty());
    }

    #[test]
    fn scan_nonexistent_directory() {
        let gen = SummaryGenerator::new(PathBuf::from("/nonexistent/path"));
        let files = gen.scan_manuscript_dir().expect("scan should succeed");
        assert!(files.is_empty());
    }

    #[test]
    fn extract_title_from_heading() {
        assert_eq!(
            extract_title("# My Story\n\nOnce upon a time..."),
            Some("My Story".to_string())
        );
    }

    #[test]
    fn extract_title_from_front_matter() {
        let content = "---\ntitle: \"The Great Adventure\"\n---\n\n# Chapter 1\n\nHello.";
        assert_eq!(
            extract_title(content),
            Some("The Great Adventure".to_string())
        );
    }

    #[test]
    fn extract_title_none_for_empty() {
        assert_eq!(extract_title(""), None);
        assert_eq!(extract_title("Just plain text."), None);
    }

    #[test]
    fn extract_chapters_from_headings() {
        let content = "# Title\n\n## The Beginning\n\n## The Middle\n\n## The End\n";
        let mut chapters = Vec::new();
        extract_chapters(content, &mut chapters);
        assert_eq!(chapters, vec!["The Beginning", "The Middle", "The End"]);
    }

    #[test]
    fn extract_chapters_skips_front_matter() {
        let content = "---\n## Not a chapter\n---\n\n## Real Chapter\n\n## Another\n";
        let mut chapters = Vec::new();
        extract_chapters(content, &mut chapters);
        assert_eq!(chapters, vec!["Real Chapter", "Another"]);
    }

    #[test]
    fn extract_opening_excerpt_basic() {
        let content =
            "# Title\n\nThis is the opening paragraph of the story.\n\nIt continues here.";
        let excerpt = extract_opening_excerpt(content, 100).expect("should have excerpt");
        assert!(excerpt.contains("This is the opening paragraph"));
    }

    #[test]
    fn extract_opening_excerpt_truncates() {
        let long_body = "a".repeat(1000);
        let content = format!("# Title\n\n{long_body}");
        let excerpt = extract_opening_excerpt(&content, 50).expect("should have excerpt");
        assert!(excerpt.len() <= 53); // 50 + "..."
    }

    #[test]
    fn generate_full_summary() {
        let tmp = create_test_manuscript(&[
            (
                "chapter-01.md",
                "---\ntitle: \"My Novel\"\n---\n\n## The Beginning\n\nIn a land far away, there lived a hero.\n\n## The Journey\n\nThe hero set out on a quest.",
            ),
            (
                "chapter-02.md",
                "## The Return\n\nAfter many adventures, the hero came home.\n\nThe end.",
            ),
        ]);
        let gen = SummaryGenerator::new(tmp.path().to_path_buf());
        let summary = gen.generate().expect("generate should succeed");
        assert_eq!(summary.title, Some("My Novel".to_string()));
        assert_eq!(summary.chapters.len(), 3);
        assert!(summary.word_count > 0);
        assert!(!summary.summary_text.is_empty());
        assert!(summary.summary_text.contains("My Novel"));
        assert!(summary.summary_text.contains("Word count:"));
        assert!(summary.summary_text.len() <= 4096);
    }

    #[test]
    fn generate_empty_manuscript() {
        let tmp = TempDir::new().expect("temp dir");
        let gen = SummaryGenerator::new(tmp.path().to_path_buf());
        let summary = gen.generate().expect("generate should succeed");
        assert_eq!(summary.title, None);
        assert!(summary.chapters.is_empty());
        assert_eq!(summary.word_count, 0);
    }

    #[test]
    fn summary_text_within_limit() {
        let big_content = format!("# Title\n\n{}\n", "word ".repeat(5000));
        let tmp = create_test_manuscript(&[("big.md", &big_content)]);
        let gen = SummaryGenerator::new(tmp.path().to_path_buf());
        let summary = gen.generate().expect("generate should succeed");
        assert!(
            summary.summary_text.len() <= 4096,
            "summary text {} exceeds max {}",
            summary.summary_text.len(),
            4096
        );
    }

    #[test]
    fn scan_permission_denied_file_no_panic() {
        let tmp = create_test_manuscript(&[("readable.md", "# Readable\n\nContent.")]);
        let unreadable_path = tmp.path().join("unreadable.md");

        let mut f = fs::File::create(&unreadable_path).expect("create file");
        f.write_all(b"# Unreadable\n\nSecret content.")
            .expect("write file");

        #[cfg(unix)]
        {
            fs::set_permissions(&unreadable_path, fs::Permissions::from_mode(0o000))
                .expect("set permissions");
        }

        let gen = SummaryGenerator::new(tmp.path().to_path_buf());

        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| gen.scan_manuscript_dir()));

        #[cfg(unix)]
        {
            match result {
                Ok(scan_result) => {
                    assert!(
                        scan_result.is_err(),
                        "Expected error for permission-denied file, got {scan_result:?}"
                    );
                }
                Err(panic_info) => {
                    panic!(
                        "scan_manuscript_dir panicked on permission-denied file: {panic_info:?}"
                    );
                }
            }

            fs::set_permissions(&unreadable_path, fs::Permissions::from_mode(0o644))
                .expect("restore permissions");
        }

        #[cfg(not(unix))]
        {
            assert!(result.is_ok(), "scan_manuscript_dir should not panic");
        }
    }

    #[test]
    fn scan_symlink_to_file_no_panic() {
        let tmp =
            create_test_manuscript(&[("real.md", "# Real File\n\nThis is the real content.")]);
        let real_path = tmp.path().join("real.md");
        let symlink_path = tmp.path().join("symlink.md");

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink(&real_path, &symlink_path).expect("create symlink");
        }

        #[cfg(windows)]
        {
            if std::os::windows::fs::symlink_file(&real_path, &symlink_path).is_err() {
                return;
            }
        }

        let gen = SummaryGenerator::new(tmp.path().to_path_buf());

        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| gen.scan_manuscript_dir()));

        match result {
            Ok(scan_result) => match scan_result {
                Ok(files) => {
                    assert!(
                        !files.is_empty() && files.len() <= 2,
                        "Expected 1-2 files, got {}",
                        files.len()
                    );
                }
                Err(e) => {
                    eprintln!("scan returned error for symlink (acceptable): {e}");
                }
            },
            Err(panic_info) => {
                panic!("scan_manuscript_dir panicked on symlink: {panic_info:?}");
            }
        }
    }

    #[test]
    fn scan_symlink_to_directory_no_panic() {
        let tmp = TempDir::new().expect("temp dir");
        let real_dir = tmp.path().join("real_dir");
        fs::create_dir_all(&real_dir).expect("create real dir");

        let real_file = real_dir.join("file.md");
        let mut f = fs::File::create(&real_file).expect("create real file");
        f.write_all(b"# Real Dir File\n\nContent in symlinked dir.")
            .expect("write file");

        let symlink_dir = tmp.path().join("symlink_dir");

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink(&real_dir, &symlink_dir).expect("create symlink to dir");
        }

        #[cfg(windows)]
        {
            if std::os::windows::fs::symlink_dir(&real_dir, &symlink_dir).is_err() {
                return;
            }
        }

        let gen = SummaryGenerator::new(tmp.path().to_path_buf());

        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| gen.scan_manuscript_dir()));

        match result {
            Ok(scan_result) => match scan_result {
                Ok(files) => {
                    assert!(
                        !files.is_empty(),
                        "Expected at least 1 file from symlinked directory, got {}",
                        files.len()
                    );
                }
                Err(e) => {
                    eprintln!("scan returned error for symlinked directory (acceptable): {e}");
                }
            },
            Err(panic_info) => {
                panic!("scan_manuscript_dir panicked on symlinked directory: {panic_info:?}");
            }
        }
    }

    #[test]
    fn scan_broken_symlink_no_panic() {
        let tmp = TempDir::new().expect("temp dir");
        let broken_symlink = tmp.path().join("broken.md");

        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            let fake_target = tmp.path().join("nonexistent.md");
            symlink(&fake_target, &broken_symlink).expect("create broken symlink");
        }

        #[cfg(windows)]
        {
            if std::os::windows::fs::symlink_file(
                &tmp.path().join("nonexistent.md"),
                &broken_symlink,
            )
            .is_err()
            {
                return;
            }
        }

        let gen = SummaryGenerator::new(tmp.path().to_path_buf());

        let result =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| gen.scan_manuscript_dir()));

        match result {
            Ok(scan_result) => match scan_result {
                Ok(files) => {
                    assert_eq!(
                        files.len(),
                        0,
                        "Broken symlink should not produce files, got {}",
                        files.len()
                    );
                }
                Err(e) => {
                    eprintln!("scan returned error for broken symlink (acceptable): {e}");
                }
            },
            Err(panic_info) => {
                panic!("scan_manuscript_dir panicked on broken symlink: {panic_info:?}");
            }
        }
    }

    #[test]
    fn generate_with_permission_denied_file_no_panic() {
        let tmp = create_test_manuscript(&[("good.md", "# Good File\n\nReadable content.")]);
        let bad_file = tmp.path().join("bad.md");

        let mut f = fs::File::create(&bad_file).expect("create bad file");
        f.write_all(b"# Bad File\n\nUnreadable content.")
            .expect("write bad file");

        #[cfg(unix)]
        {
            fs::set_permissions(&bad_file, fs::Permissions::from_mode(0o000))
                .expect("set permissions");
        }

        let gen = SummaryGenerator::new(tmp.path().to_path_buf());

        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| gen.generate()));

        #[cfg(unix)]
        {
            match result {
                Ok(gen_result) => {
                    assert!(
                        gen_result.is_err(),
                        "Expected error for permission-denied file in generate(), got {gen_result:?}"
                    );
                }
                Err(panic_info) => {
                    panic!("generate() panicked on permission-denied file: {panic_info:?}");
                }
            }

            fs::set_permissions(&bad_file, fs::Permissions::from_mode(0o644))
                .expect("restore permissions");
        }

        #[cfg(not(unix))]
        {
            assert!(result.is_ok(), "generate() should not panic");
        }
    }
}
