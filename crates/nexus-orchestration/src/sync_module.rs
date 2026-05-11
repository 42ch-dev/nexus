//! Novel-writing sync module contract.
//!
//! Defines how workspace artifacts map to platform sync bundles.
//! This module only defines types and discovery logic — actual sync
//! transport is handled by `nexus-sync` crate.

use std::path::Path;

use sha2::{Digest, Sha256};

/// Content hash type (SHA-256 hex).
pub type ContentHash = String;

/// A single chapter ready for sync.
#[derive(Debug, Clone)]
pub struct ChapterContent {
    /// Chapter filename (e.g. `ch01-introduction.md`).
    pub filename: String,
    /// SHA-256 hex digest of `content`.
    pub content_hash: ContentHash,
    /// Raw file content.
    pub content: String,
}

/// A complete story bundle ready for platform sync.
#[derive(Debug, Clone)]
pub struct StoryBundle {
    /// Parent world identifier.
    pub world_id: String,
    /// Story directory name (the `<story_ref>`).
    pub story_ref: String,
    /// Ordered chapter contents.
    pub chapters: Vec<ChapterContent>,
    /// `outline.md` content, if present.
    pub outline: Option<String>,
    /// Number of chapters (redundant with `chapters.len()` for wire convenience).
    pub chapter_count: u32,
    /// ISO 8601 timestamp of when the bundle was built.
    pub synced_at: String,
}

/// Discovered story metadata from workspace scan.
#[derive(Debug, Clone)]
pub struct DiscoveredStory {
    /// Story directory name (`<story_ref>`).
    pub story_ref: String,
    /// Chapter filenames sorted alphabetically.
    pub chapters: Vec<String>,
    /// Whether `outline.md` was found.
    pub has_outline: bool,
}

/// Discover stories in the workspace.
///
/// Scans `<workspace_dir>/Stories/` for subdirectories containing `.md` files.
/// Hidden directories (starting with `.`) and hidden files are skipped.
/// `outline.md` is recorded separately from chapters.
/// Returns stories sorted alphabetically by `story_ref`.
#[must_use]
pub fn discover_stories(workspace_dir: &Path) -> Vec<DiscoveredStory> {
    let stories_dir = workspace_dir.join("Stories");
    if !stories_dir.is_dir() {
        return Vec::new();
    }

    let mut stories = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&stories_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            // Skip hidden dirs
            if name.starts_with('.') {
                continue;
            }

            let mut chapters = Vec::new();
            let mut has_outline = false;

            if let Ok(files) = std::fs::read_dir(&path) {
                for file in files.flatten() {
                    let file_path = file.path();
                    if !file_path.is_file() {
                        continue;
                    }
                    if file_path.extension().is_none_or(|ext| ext != "md") {
                        continue;
                    }
                    let fname = file.file_name().to_string_lossy().to_string();
                    if fname == "outline.md" {
                        has_outline = true;
                    } else {
                        chapters.push(fname);
                    }
                }
            }

            chapters.sort();
            if !chapters.is_empty() || has_outline {
                stories.push(DiscoveredStory {
                    story_ref: name,
                    chapters,
                    has_outline,
                });
            }
        }
    }

    stories.sort_by(|a, b| a.story_ref.cmp(&b.story_ref));
    stories
}

/// Build a story bundle from a discovered story and its workspace directory.
///
/// Reads each chapter file and computes its SHA-256 content hash.
/// Reads `outline.md` if the story reported one.
/// Returns `None` if neither chapters nor outline yield any content.
#[must_use]
pub fn build_story_bundle(
    world_id: &str,
    story: &DiscoveredStory,
    workspace_dir: &Path,
) -> Option<StoryBundle> {
    let story_path = workspace_dir.join("Stories").join(&story.story_ref);

    let chapters: Vec<ChapterContent> = story
        .chapters
        .iter()
        .filter_map(|filename| {
            let content = std::fs::read_to_string(story_path.join(filename)).ok()?;
            let hash = format!("{:x}", Sha256::digest(content.as_bytes()));
            Some(ChapterContent {
                filename: filename.clone(),
                content_hash: hash,
                content,
            })
        })
        .collect();

    let outline = if story.has_outline {
        std::fs::read_to_string(story_path.join("outline.md")).ok()
    } else {
        None
    };

    if chapters.is_empty() && outline.is_none() {
        return None;
    }

    // u32 is sufficient: a story with >4 billion chapters is not a real scenario.
    #[allow(clippy::cast_possible_truncation)]
    let chapter_count = chapters.len() as u32;

    Some(StoryBundle {
        world_id: world_id.to_string(),
        story_ref: story.story_ref.clone(),
        chapters,
        outline,
        chapter_count,
        synced_at: chrono::Utc::now().to_rfc3339(),
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    // Helper: create a temp workspace with a Stories structure.
    fn setup_workspace() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    fn write_file(dir: &Path, name: &str, content: &str) {
        fs::write(dir.join(name), content).expect("write file");
    }

    fn create_story_dir(workspace: &Path, story_ref: &str) -> PathBuf {
        let dir = workspace.join("Stories").join(story_ref);
        fs::create_dir_all(&dir).expect("create story dir");
        dir
    }

    // -----------------------------------------------------------------------
    // 1. No Stories/ directory
    // -----------------------------------------------------------------------
    #[test]
    fn test_discover_stories_empty() {
        let workspace = setup_workspace();
        let result = discover_stories(workspace.path());
        assert!(result.is_empty(), "no Stories/ dir should yield empty vec");
    }

    // -----------------------------------------------------------------------
    // 2. Stories with chapters
    // -----------------------------------------------------------------------
    #[test]
    fn test_discover_stories_with_chapters() {
        let workspace = setup_workspace();
        let story_dir = create_story_dir(workspace.path(), "my-novel");
        write_file(&story_dir, "ch01.md", "Chapter 1 content");
        write_file(&story_dir, "ch02.md", "Chapter 2 content");

        let result = discover_stories(workspace.path());
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].story_ref, "my-novel");
        assert_eq!(result[0].chapters, vec!["ch01.md", "ch02.md"]);
        assert!(!result[0].has_outline);
    }

    // -----------------------------------------------------------------------
    // 3. Hidden directories skipped
    // -----------------------------------------------------------------------
    #[test]
    fn test_discover_stories_skips_hidden() {
        let workspace = setup_workspace();
        let hidden_dir = create_story_dir(workspace.path(), ".hidden-story");
        write_file(&hidden_dir, "ch01.md", "Hidden content");

        let visible_dir = create_story_dir(workspace.path(), "visible-story");
        write_file(&visible_dir, "ch01.md", "Visible content");

        let result = discover_stories(workspace.path());
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].story_ref, "visible-story");
    }

    // -----------------------------------------------------------------------
    // 4. Non-.md files skipped
    // -----------------------------------------------------------------------
    #[test]
    fn test_discover_stories_skips_non_md() {
        let workspace = setup_workspace();
        let story_dir = create_story_dir(workspace.path(), "story1");
        write_file(&story_dir, "ch01.txt", "Text file");
        write_file(&story_dir, "ch01.md", "Markdown file");

        let result = discover_stories(workspace.path());
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].chapters, vec!["ch01.md"]);
    }

    // -----------------------------------------------------------------------
    // 5. Outline detected
    // -----------------------------------------------------------------------
    #[test]
    fn test_discover_stories_with_outline() {
        let workspace = setup_workspace();
        let story_dir = create_story_dir(workspace.path(), "story1");
        write_file(&story_dir, "outline.md", "# Outline");
        write_file(&story_dir, "ch01.md", "Content");

        let result = discover_stories(workspace.path());
        assert_eq!(result.len(), 1);
        assert!(result[0].has_outline);
        // outline.md should NOT appear in chapters
        assert_eq!(result[0].chapters, vec!["ch01.md"]);
    }

    // -----------------------------------------------------------------------
    // 6. Full round-trip: discover → build bundle
    // -----------------------------------------------------------------------
    #[test]
    fn test_build_story_bundle() {
        let workspace = setup_workspace();
        let story_dir = create_story_dir(workspace.path(), "my-novel");
        write_file(&story_dir, "ch01.md", "Hello world");
        write_file(&story_dir, "outline.md", "# Plan");

        let stories = discover_stories(workspace.path());
        assert_eq!(stories.len(), 1);

        let bundle = build_story_bundle("world-42", &stories[0], workspace.path())
            .expect("bundle should be Some");

        assert_eq!(bundle.world_id, "world-42");
        assert_eq!(bundle.story_ref, "my-novel");
        assert_eq!(bundle.chapter_count, 1);
        assert_eq!(bundle.chapters[0].filename, "ch01.md");
        assert_eq!(bundle.chapters[0].content, "Hello world");
        assert!(bundle.outline.is_some());
        assert_eq!(bundle.outline.as_deref(), Some("# Plan"));
        assert!(!bundle.synced_at.is_empty());
    }

    // -----------------------------------------------------------------------
    // 7. Content hash is SHA-256
    // -----------------------------------------------------------------------
    #[test]
    fn test_build_story_bundle_content_hash() {
        let workspace = setup_workspace();
        let story_dir = create_story_dir(workspace.path(), "hash-test");
        write_file(&story_dir, "ch01.md", "test content");

        let stories = discover_stories(workspace.path());
        let bundle = build_story_bundle("w1", &stories[0], workspace.path()).expect("bundle");

        // Verify hash matches manual SHA-256 of "test content"
        let expected = format!("{:x}", Sha256::digest(b"test content"));
        assert_eq!(bundle.chapters[0].content_hash, expected);
        // SHA-256 hex is 64 chars
        assert_eq!(bundle.chapters[0].content_hash.len(), 64);
    }

    // -----------------------------------------------------------------------
    // 8. Empty story (no files) returns None
    // -----------------------------------------------------------------------
    #[test]
    fn test_build_story_bundle_empty_story_skipped() {
        let workspace = setup_workspace();
        // Create story dir with no files — discover_stories won't return it
        // because we check !chapters.is_empty() || has_outline. But let's
        // also verify build_story_bundle handles an empty DiscoveredStory.
        let empty_story = DiscoveredStory {
            story_ref: "nonexistent".to_string(),
            chapters: vec![],
            has_outline: false,
        };

        let result = build_story_bundle("w1", &empty_story, workspace.path());
        assert!(result.is_none(), "empty story should yield None");
    }
}
