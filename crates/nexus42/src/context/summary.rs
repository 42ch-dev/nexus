//! Summary generation from local manuscript files.
//!
//! V1.0 uses basic extraction: title (from front-matter or first heading),
//! chapter list (from heading structure), word count, and opening excerpt.
//! No LLM call required.

use std::fs;
use std::path::{Path, PathBuf};

/// Maximum summary text length (characters).
const MAX_SUMMARY_CHARS: usize = 4096;

/// Maximum opening excerpt length (characters).
const MAX_EXCERPT_CHARS: usize = 500;

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
    /// Final summary text suitable for StoryManifest.summary_text.
    pub summary_text: String,
}

/// Summary generator for local manuscript files.
pub struct SummaryGenerator {
    /// Path to the manuscript root (e.g., `Stories/<world_ref>/`).
    manuscript_root: PathBuf,
    /// Maximum summary length in characters.
    max_summary_chars: usize,
    /// Maximum excerpt length in characters.
    max_excerpt_chars: usize,
}

impl SummaryGenerator {
    /// Create a new summary generator for the given manuscript root.
    pub fn new(manuscript_root: PathBuf) -> Self {
        Self {
            manuscript_root,
            max_summary_chars: MAX_SUMMARY_CHARS,
            max_excerpt_chars: MAX_EXCERPT_CHARS,
        }
    }

    /// Scan the manuscript directory for recognized file types.
    ///
    /// Walks `Stories/<world_ref>/` for `.md` and `.txt` files.
    /// Ignores `References/` tree and non-recognized extensions.
    pub fn scan_manuscript_dir(&self) -> std::io::Result<Vec<ManuscriptFile>> {
        let mut files = Vec::new();
        if !self.manuscript_root.exists() {
            return Ok(files);
        }
        self.scan_recursive(&self.manuscript_root, &mut files)?;
        Ok(files)
    }

    fn scan_recursive(&self, dir: &Path, files: &mut Vec<ManuscriptFile>) -> std::io::Result<()> {
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
                self.scan_recursive(&path, files)?;
            } else {
                let extension = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if matches!(extension, "md" | "txt") {
                    let relative = path
                        .strip_prefix(&self.manuscript_root)
                        .unwrap_or(&path)
                        .to_path_buf();
                    let content = fs::read_to_string(&path)?;
                    files.push(ManuscriptFile {
                        relative_path: relative,
                        content,
                    });
                }
            }
        }
        Ok(())
    }

    /// Generate a basic summary from manuscript files.
    pub fn generate(&self) -> std::io::Result<GeneratedSummary> {
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
            summary_parts.push(format!("Title: {}", t));
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
        summary_parts.push(format!("Word count: {}", total_words));
        if let Some(excerpt) = &first_body_excerpt {
            summary_parts.push(format!("Opening: {}", excerpt));
        }

        let mut summary_text = summary_parts.join("\n");
        // Truncate to max length
        if summary_text.len() > self.max_summary_chars {
            summary_text.truncate(self.max_summary_chars.saturating_sub(3));
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
            let value = trimmed.strip_prefix("# ").unwrap().trim().to_string();
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
        // Truncate at word boundary near max_chars
        let mut end = max_chars;
        while end > 0 && !body_text.is_char_boundary(end) {
            end -= 1;
        }
        // Try to break at a space
        if let Some(space_pos) = body_text[..end].rfind(' ') {
            end = space_pos;
        }
        Some(format!("{}...", &body_text[..end]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
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
    fn extract_opening_excerpt() {
        let content =
            "# Title\n\nThis is the opening paragraph of the story.\n\nIt continues here.";
        let excerpt = super::extract_opening_excerpt(content, 100).expect("should have excerpt");
        assert!(excerpt.contains("This is the opening paragraph"));
    }

    #[test]
    fn extract_opening_excerpt_truncates() {
        let long_body = "a".repeat(1000);
        let content = format!("# Title\n\n{}", long_body);
        let excerpt = super::extract_opening_excerpt(&content, 50).expect("should have excerpt");
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
        // Create a large file that would produce a long summary
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
}
