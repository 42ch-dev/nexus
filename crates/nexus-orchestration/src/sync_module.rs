//! Novel-writing sync module contract.
//!
//! Defines how workspace artifacts map to platform sync bundles.
//! This module only defines types and discovery logic — actual sync
//! transport is handled by `nexus-cloud-sync` crate.
//!
//! V1.36: scans `Works/<work_ref>/Stories/` for chapter正文.
//! Legacy workspace-root `Stories/` scan is removed (pre-1.0).

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// Content hash type (SHA-256 hex).
pub type ContentHash = String;

#[must_use]
pub fn works_dir(workspace_dir: &Path, work_ref: &str) -> PathBuf {
    workspace_dir.join("Works").join(work_ref)
}

/// Returns `Works/<work_ref>/Stories/<filename>` under the workspace root.
#[must_use]
pub fn chapter_body_path(workspace_dir: &Path, work_ref: &str, filename: &str) -> PathBuf {
    works_dir(workspace_dir, work_ref)
        .join("Stories")
        .join(filename)
}

#[derive(Debug, Clone)]
pub struct ChapterContent {
    /// Chapter filename (e.g. `ch01-introduction.md`).
    pub filename: String,
    /// SHA-256 hex digest of `content`.
    pub content_hash: ContentHash,
    /// Raw file content.
    pub content: String,
    /// Chapter status from `work_chapters` table (when DB is available).
    pub status: Option<String>,
    /// Actual word count from `work_chapters` table (when DB is available).
    pub actual_word_count: Option<u32>,
}

/// A complete story bundle ready for platform sync.
///
/// Wire name `StoryBundle` is unchanged for contract stability.
#[derive(Debug, Clone)]
pub struct StoryBundle {
    /// Parent world identifier (empty string when worldless).
    pub world_id: String,
    /// Work identifier from `works` table.
    pub work_id: String,
    /// Work directory name under `Works/` (stable slug).
    pub work_ref: String,
    /// Ordered chapter contents.
    pub chapters: Vec<ChapterContent>,
    /// Number of chapters (redundant with `chapters.len()` for wire convenience).
    pub chapter_count: u32,
    /// ISO 8601 timestamp of when the bundle was built.
    pub synced_at: String,
}

/// Discovered work metadata from workspace scan.
#[derive(Debug, Clone)]
pub struct DiscoveredWork {
    /// Work directory name under `Works/`.
    pub work_ref: String,
    /// Chapter filenames sorted alphabetically (only `Stories/*.md`).
    pub chapters: Vec<String>,
}

const SKIP_FILES: &[&str] = &["README.md", "foreshadowing.md", "event-index.md"];

/// Discover works with novel chapters in the workspace.
///
/// Scans `<workspace_dir>/Works/<work_ref>/Stories/*.md` for chapter files.
/// Hidden files (starting with `.`) are skipped.
/// `README.md`, `Outlines/**`, `Logs/**`, `Rules/**`, `foreshadowing.md`, `event-index.md`
/// are never chapter candidates (V1.39 P3: Rules/** explicitly excluded per DF-65).
/// Returns works sorted alphabetically by `work_ref`.
#[must_use]
pub fn discover_works(workspace_dir: &Path) -> Vec<DiscoveredWork> {
    let works_root = workspace_dir.join("Works");
    if !works_root.is_dir() {
        return Vec::new();
    }

    let mut works = Vec::new();

    if let Ok(entries) = std::fs::read_dir(&works_root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let name = path
                .file_name()
                .map_or_default(|n| n.to_string_lossy().to_string());

            if name.starts_with('.') {
                continue;
            }

            let stories_dir = path.join("Stories");
            if !stories_dir.is_dir() {
                continue;
            }

            let mut chapters = Vec::new();

            if let Ok(files) = std::fs::read_dir(&stories_dir) {
                for file in files.flatten() {
                    let file_path = file.path();
                    if !file_path.is_file() {
                        continue;
                    }
                    if file_path.extension().is_none_or(|ext| ext != "md") {
                        continue;
                    }
                    let fname = file.file_name().to_string_lossy().to_string();

                    if fname.starts_with('.') {
                        continue;
                    }
                    if SKIP_FILES.contains(&fname.as_str()) {
                        continue;
                    }

                    chapters.push(fname);
                }
            }

            chapters.sort();
            works.push(DiscoveredWork {
                work_ref: name,
                chapters,
            });
        }
    }

    works.sort_by(|a, b| a.work_ref.cmp(&b.work_ref));
    works
}

///
/// Reads each chapter file and computes its SHA-256 content hash.
/// Returns `None` if no chapters yield any content.
#[must_use]
pub fn build_story_bundle(
    world_id: &str,
    work_id: &str,
    work: &DiscoveredWork,
    workspace_dir: &Path,
) -> Option<StoryBundle> {
    let stories_path = works_dir(workspace_dir, &work.work_ref).join("Stories");

    let chapters: Vec<ChapterContent> = work
        .chapters
        .iter()
        .filter_map(|filename| {
            let content = std::fs::read_to_string(stories_path.join(filename)).ok()?;
            let hash = hex::encode(Sha256::digest(content.as_bytes()));
            Some(ChapterContent {
                filename: filename.clone(),
                content_hash: hash,
                content,
                status: None,
                actual_word_count: None,
            })
        })
        .collect();

    if chapters.is_empty() {
        return None;
    }

    let chapter_count = u32::try_from(chapters.len()).ok()?;

    Some(StoryBundle {
        world_id: world_id.to_string(),
        work_id: work_id.to_string(),
        work_ref: work.work_ref.clone(),
        chapters,
        chapter_count,
        synced_at: chrono::Utc::now().to_rfc3339(),
    })
}

/// Build a story bundle enriched with `work_chapters` metadata.
///
/// When `db_pool` is provided, queries the `work_chapters` table for
/// chapter `status` and `actual_word_count`. Falls back to filesystem-only
/// when `db_pool` is `None`.
///
/// Returns `None` if no chapters yield any content.
pub async fn build_story_bundle_with_db(
    world_id: &str,
    work_id: &str,
    work: &DiscoveredWork,
    workspace_dir: &Path,
    db_pool: Option<&sqlx::SqlitePool>,
) -> Option<StoryBundle> {
    let mut bundle = build_story_bundle(world_id, work_id, work, workspace_dir)?;

    if let Some(pool) = db_pool {
        use sqlx::Row;
        // SAFETY: SELECT against work_chapters — runtime query.
        let rows = sqlx::query(
            "SELECT chapter, status, actual_word_count FROM work_chapters WHERE work_id = ?",
        )
        .bind(work_id)
        .fetch_all(pool)
        .await
        .ok()?;

        let mut meta_map = std::collections::HashMap::new();
        for row in &rows {
            let chapter: i32 = row.get("chapter");
            let status: String = row.get("status");
            let actual_word_count: Option<i32> = row.get("actual_word_count");
            meta_map.insert(chapter, (status, actual_word_count));
        }

        for ch in &mut bundle.chapters {
            let ch_num = parse_chapter_number(&ch.filename);
            if let Some(ch_num) = ch_num {
                if let Some((status, wc)) = meta_map.get(&ch_num) {
                    ch.status = Some(status.clone());
                    ch.actual_word_count = wc.map(|v| u32::try_from(v).unwrap_or(0));
                }
            }
        }
    }

    Some(bundle)
}

/// Parse chapter number from a filename like `ch01-introduction.md`.
/// Returns `None` if the filename doesn't start with `ch` followed by digits.
fn parse_chapter_number(filename: &str) -> Option<i32> {
    let name = filename.strip_suffix(".md")?;
    let digits = name.strip_prefix("ch")?;
    let num_str = digits.split('-').next()?;
    num_str.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup_workspace() -> tempfile::TempDir {
        tempfile::tempdir().expect("tempdir")
    }

    fn write_file(dir: &Path, name: &str, content: &str) {
        fs::write(dir.join(name), content).expect("write file");
    }

    fn create_work_dir(workspace: &Path, work_ref: &str) -> PathBuf {
        let dir = works_dir(workspace, work_ref).join("Stories");
        fs::create_dir_all(&dir).expect("create Stories dir");
        dir
    }

    #[test]
    fn test_discover_works_empty() {
        let workspace = setup_workspace();
        let result = discover_works(workspace.path());
        assert!(result.is_empty(), "no Works/ dir should yield empty vec");
    }

    #[test]
    fn test_discover_works_with_chapters() {
        let workspace = setup_workspace();
        let stories = create_work_dir(workspace.path(), "my-novel");
        write_file(&stories, "ch01-intro.md", "Chapter 1 content");
        write_file(&stories, "ch02-rising.md", "Chapter 2 content");

        let result = discover_works(workspace.path());
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].work_ref, "my-novel");
        assert_eq!(result[0].chapters, vec!["ch01-intro.md", "ch02-rising.md"]);
    }

    #[test]
    fn test_discover_works_skips_hidden_dirs() {
        let workspace = setup_workspace();
        let hidden = works_dir(workspace.path(), ".hidden-work").join("Stories");
        fs::create_dir_all(&hidden).expect("create hidden Stories");
        write_file(&hidden, "ch01.md", "Hidden content");

        let visible = create_work_dir(workspace.path(), "visible-work");
        write_file(&visible, "ch01.md", "Visible content");

        let result = discover_works(workspace.path());
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].work_ref, "visible-work");
    }

    #[test]
    fn test_discover_works_skips_hidden_files() {
        let workspace = setup_workspace();
        let stories = create_work_dir(workspace.path(), "my-novel");
        write_file(&stories, ".hidden.md", "Hidden file content");
        write_file(&stories, "ch01-intro.md", "Visible content");

        let result = discover_works(workspace.path());
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].chapters, vec!["ch01-intro.md"]);
    }

    #[test]
    fn test_discover_works_skips_non_chapter_files() {
        let workspace = setup_workspace();
        let stories = create_work_dir(workspace.path(), "my-novel");
        write_file(&stories, "ch01-intro.md", "Chapter content");
        write_file(&stories, "README.md", "Work readme");
        write_file(&stories, "foreshadowing.md", "Foreshadowing index");
        write_file(&stories, "event-index.md", "Event index");
        write_file(&stories, "notes.txt", "Text file");

        let result = discover_works(workspace.path());
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].chapters, vec!["ch01-intro.md"]);
    }

    #[test]
    fn test_discover_works_ignores_outlines_and_logs() {
        let workspace = setup_workspace();
        let work_root = works_dir(workspace.path(), "my-novel");
        let stories = work_root.join("Stories");
        let outlines = work_root.join("Outlines");
        let logs = work_root.join("Logs");
        fs::create_dir_all(&stories).expect("Stories");
        fs::create_dir_all(&outlines).expect("Outlines");
        fs::create_dir_all(&logs).expect("Logs");

        write_file(&stories, "ch01-intro.md", "Chapter 1");
        write_file(&outlines, "volume-outline.md", "Volume outline");
        write_file(&outlines, "foreshadowing.md", "Foreshadowing");
        write_file(&logs, "draft-log.md", "Log entry");

        let result = discover_works(workspace.path());
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].chapters, vec!["ch01-intro.md"]);
    }

    #[test]
    fn test_discover_works_multiple_works() {
        let workspace = setup_workspace();
        let stories1 = create_work_dir(workspace.path(), "alpha-novel");
        write_file(&stories1, "ch01.md", "A1");
        let stories2 = create_work_dir(workspace.path(), "beta-novel");
        write_file(&stories2, "ch01.md", "B1");

        let result = discover_works(workspace.path());
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].work_ref, "alpha-novel");
        assert_eq!(result[1].work_ref, "beta-novel");
    }

    #[test]
    fn test_discover_works_alphabetical_ordering() {
        let workspace = setup_workspace();
        let stories1 = create_work_dir(workspace.path(), "zebra-novel");
        write_file(&stories1, "ch01.md", "Z1");
        let stories2 = create_work_dir(workspace.path(), "apple-novel");
        write_file(&stories2, "ch01.md", "A1");

        let result = discover_works(workspace.path());
        assert_eq!(result[0].work_ref, "apple-novel");
        assert_eq!(result[1].work_ref, "zebra-novel");
    }

    #[test]
    fn test_build_story_bundle() {
        let workspace = setup_workspace();
        let stories = create_work_dir(workspace.path(), "my-novel");
        write_file(&stories, "ch01-intro.md", "Hello world");
        write_file(&stories, "ch02-body.md", "Chapter 2 body");

        let works = discover_works(workspace.path());
        assert_eq!(works.len(), 1);

        let bundle = build_story_bundle("world-42", "wrk_001", &works[0], workspace.path())
            .expect("bundle should be Some");

        assert_eq!(bundle.world_id, "world-42");
        assert_eq!(bundle.work_id, "wrk_001");
        assert_eq!(bundle.work_ref, "my-novel");
        assert_eq!(bundle.chapter_count, 2);
        assert_eq!(bundle.chapters[0].filename, "ch01-intro.md");
        assert_eq!(bundle.chapters[0].content, "Hello world");
        assert!(bundle.synced_at.contains('T'));
    }

    #[test]
    fn test_build_story_bundle_content_hash() {
        let workspace = setup_workspace();
        let stories = create_work_dir(workspace.path(), "hash-test");
        write_file(&stories, "ch01.md", "test content");

        let works = discover_works(workspace.path());
        let bundle =
            build_story_bundle("w1", "wrk_002", &works[0], workspace.path()).expect("bundle");

        let expected = hex::encode(Sha256::digest(b"test content"));
        assert_eq!(bundle.chapters[0].content_hash, expected);
        assert_eq!(bundle.chapters[0].content_hash.len(), 64);
    }

    #[test]
    fn test_build_story_bundle_empty_stories() {
        let workspace = setup_workspace();
        create_work_dir(workspace.path(), "empty-work");

        let works = discover_works(workspace.path());
        assert_eq!(works.len(), 1);
        assert_eq!(works[0].chapters, [] as [std::string::String; 0]);

        let result = build_story_bundle("w1", "wrk_003", &works[0], workspace.path());
        assert!(result.is_none(), "empty stories should yield None");
    }

    #[test]
    fn test_parse_chapter_number() {
        assert_eq!(parse_chapter_number("ch01-intro.md"), Some(1));
        assert_eq!(parse_chapter_number("ch12-the-climax.md"), Some(12));
        assert_eq!(parse_chapter_number("ch99.md"), Some(99));
        assert_eq!(parse_chapter_number("outline.md"), None);
        assert_eq!(parse_chapter_number("readme.md"), None);
    }

    #[test]
    fn test_path_helpers() {
        let ws = Path::new("/tmp/workspace");
        assert_eq!(
            works_dir(ws, "my-novel"),
            PathBuf::from("/tmp/workspace/Works/my-novel")
        );
        assert_eq!(
            chapter_body_path(ws, "my-novel", "ch01-intro.md"),
            PathBuf::from("/tmp/workspace/Works/my-novel/Stories/ch01-intro.md")
        );
    }

    #[test]
    fn test_discover_works_skips_work_without_stories() {
        let workspace = setup_workspace();
        let work_root = works_dir(workspace.path(), "partial-work");
        fs::create_dir_all(work_root.join("Outlines")).expect("Outlines");

        let result = discover_works(workspace.path());
        assert!(result.is_empty(), "work without Stories/ should be skipped");
    }

    #[test]
    fn test_discover_works_ignores_rules_and_logs_subdirs() {
        let workspace = setup_workspace();
        let work_root = works_dir(workspace.path(), "my-novel");
        let stories = work_root.join("Stories");
        let rules = work_root.join("Rules");
        let logs_write = work_root.join("Logs").join("write");
        let logs_review = work_root.join("Logs").join("review");
        fs::create_dir_all(&stories).expect("Stories");
        fs::create_dir_all(&rules).expect("Rules");
        fs::create_dir_all(&logs_write).expect("Logs/write");
        fs::create_dir_all(&logs_review).expect("Logs/review");

        write_file(&stories, "ch01-intro.md", "Chapter 1");
        write_file(&rules, "novel-rules.md", "# Rules\n- POV: first");
        write_file(&rules, "novel-rules-history.md", "| ts | actor | reason |");
        write_file(&logs_write, "draft-session-1.md", "Draft notes");
        write_file(&logs_review, "review-notes.md", "Review notes");

        let result = discover_works(workspace.path());
        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].chapters,
            vec!["ch01-intro.md"],
            "only Stories/*.md should appear — Rules/ and Logs/ excluded"
        );
    }

    #[test]
    fn sync_module_skips_foreshadowing_index_file() {
        assert!(
            SKIP_FILES.contains(&"foreshadowing.md"),
            "foreshadowing.md must be in SKIP_FILES"
        );
        assert!(
            SKIP_FILES.contains(&"event-index.md"),
            "event-index.md must be in SKIP_FILES"
        );

        let workspace = setup_workspace();
        let work_root = works_dir(workspace.path(), "my-novel");
        let stories = work_root.join("Stories");
        let outlines = work_root.join("Outlines");
        fs::create_dir_all(&stories).expect("Stories");
        fs::create_dir_all(&outlines).expect("Outlines");

        write_file(&stories, "ch01-intro.md", "Chapter 1");
        write_file(
            &outlines,
            "foreshadowing.md",
            "# Foreshadowing Index\n\n| ID | Description |\n| --- | --- |\n| F001 | x |\n",
        );
        write_file(&outlines, "event-index.md", "# Event Index\n");

        let result = discover_works(workspace.path());
        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].chapters,
            vec!["ch01-intro.md"],
            "index files in Outlines/ must never be discovered as chapters"
        );
    }
}
