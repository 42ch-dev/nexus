//! Sync module contract tests for `Works/<work_ref>/Stories/` layout (V1.36 P2).
//!
//! Verifies the sync module scans only the correct directories and
//! excludes non-chapter artifacts per novel-writing/sync-contract.md.

use std::fs;
use std::path::{Path, PathBuf};

/// Create a temp workspace with `Works/<work_ref>/Stories/` structure.
fn create_work_stories(workspace: &Path, work_ref: &str) -> PathBuf {
    let stories = workspace.join("Works").join(work_ref).join("Stories");
    fs::create_dir_all(&stories).expect("create Stories dir");
    stories
}

fn write_file(dir: &Path, name: &str, content: &str) {
    fs::write(dir.join(name), content).expect("write file");
}

// ---------------------------------------------------------------------------
// 1. discover_works empty when no Works/
// ---------------------------------------------------------------------------
#[test]
fn test_discover_works_empty_no_works_dir() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let result = nexus_orchestration::sync_module::discover_works(workspace.path());
    assert!(result.is_empty());
}

// ---------------------------------------------------------------------------
// 2. N work_ref subdirs → N entries
// ---------------------------------------------------------------------------
#[test]
fn test_discover_works_multiple_entries() {
    let workspace = tempfile::tempdir().expect("tempdir");
    create_work_stories(workspace.path(), "novel-a");
    create_work_stories(workspace.path(), "novel-b");
    create_work_stories(workspace.path(), "novel-c");

    let result = nexus_orchestration::sync_module::discover_works(workspace.path());
    assert_eq!(result.len(), 3);
    // Sorted alphabetically
    assert_eq!(result[0].work_ref, "novel-a");
    assert_eq!(result[1].work_ref, "novel-b");
    assert_eq!(result[2].work_ref, "novel-c");
}

// ---------------------------------------------------------------------------
// 3. Per-work chapter scan: only Stories/*.md
// ---------------------------------------------------------------------------
#[test]
fn test_discover_works_only_scans_stories_md() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let stories = create_work_stories(workspace.path(), "my-novel");
    write_file(&stories, "ch01-intro.md", "Chapter 1");
    write_file(&stories, "ch02-body.md", "Chapter 2");

    let result = nexus_orchestration::sync_module::discover_works(workspace.path());
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].chapters, vec!["ch01-intro.md", "ch02-body.md"]);
}

// ---------------------------------------------------------------------------
// 4. Exclude README.md, Outlines/**, Logs/**
// ---------------------------------------------------------------------------
#[test]
fn test_discover_works_excludes_readme_outlines_logs() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let work_root = workspace.path().join("Works").join("my-novel");
    let stories = work_root.join("Stories");
    let outlines = work_root.join("Outlines");
    let logs = work_root.join("Logs");
    fs::create_dir_all(&stories).expect("Stories");
    fs::create_dir_all(&outlines).expect("Outlines");
    fs::create_dir_all(&logs).expect("Logs");

    // Write chapters in Stories/
    write_file(&stories, "ch01-intro.md", "Chapter 1");
    // Write non-chapter files in Stories/ (excluded)
    write_file(&stories, "README.md", "Work readme");
    write_file(&stories, "foreshadowing.md", "Foreshadowing");
    write_file(&stories, "event-index.md", "Events");
    // Write files in Outlines/ and Logs/ (never scanned)
    write_file(&outlines, "volume-outline.md", "Volume outline");
    write_file(&outlines, "foreshadowing.md", "Foreshadowing outline");
    write_file(&logs, "draft-log.md", "Log entry");

    let result = nexus_orchestration::sync_module::discover_works(workspace.path());
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].chapters, vec!["ch01-intro.md"]);
}

// ---------------------------------------------------------------------------
// 5. Hidden files skipped
// ---------------------------------------------------------------------------
#[test]
fn test_discover_works_skips_hidden_files() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let stories = create_work_stories(workspace.path(), "my-novel");
    write_file(&stories, ".hidden-chapter.md", "Hidden");
    write_file(&stories, "ch01-visible.md", "Visible");

    let result = nexus_orchestration::sync_module::discover_works(workspace.path());
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].chapters, vec!["ch01-visible.md"]);
}

// ---------------------------------------------------------------------------
// 6. Alphabetical chapter ordering
// ---------------------------------------------------------------------------
#[test]
fn test_discover_works_chapters_alphabetical() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let stories = create_work_stories(workspace.path(), "my-novel");
    write_file(&stories, "ch03-end.md", "C3");
    write_file(&stories, "ch01-intro.md", "C1");
    write_file(&stories, "ch02-body.md", "C2");

    let result = nexus_orchestration::sync_module::discover_works(workspace.path());
    assert_eq!(
        result[0].chapters,
        vec!["ch01-intro.md", "ch02-body.md", "ch03-end.md"]
    );
}

// ---------------------------------------------------------------------------
// 7. No runtime fallback to workspace-root Stories/
// ---------------------------------------------------------------------------
#[test]
fn test_no_workspace_root_stories_fallback() {
    let workspace = tempfile::tempdir().expect("tempdir");

    // Create legacy workspace-root Stories/ (pre-1.0 layout)
    let legacy_stories = workspace.path().join("Stories").join("old-novel");
    fs::create_dir_all(&legacy_stories).expect("legacy Stories");
    write_file(&legacy_stories, "ch01.md", "Legacy chapter");

    let result = nexus_orchestration::sync_module::discover_works(workspace.path());
    assert!(
        result.is_empty(),
        "legacy workspace-root Stories/ should not be scanned"
    );
}

// ---------------------------------------------------------------------------
// 8. build_story_bundle round-trip with content hash
// ---------------------------------------------------------------------------
#[test]
fn test_build_story_bundle_round_trip() {
    let workspace = tempfile::tempdir().expect("tempdir");
    let stories = create_work_stories(workspace.path(), "test-novel");
    write_file(&stories, "ch01-intro.md", "Chapter 1 body content");
    write_file(&stories, "ch02-rising.md", "Chapter 2 rising action");

    let works = nexus_orchestration::sync_module::discover_works(workspace.path());
    assert_eq!(works.len(), 1);

    let bundle = nexus_orchestration::sync_module::build_story_bundle(
        "world-42",
        "wrk_test_001",
        &works[0],
        workspace.path(),
    )
    .expect("bundle should be Some");

    assert_eq!(bundle.work_id, "wrk_test_001");
    assert_eq!(bundle.work_ref, "test-novel");
    assert_eq!(bundle.chapter_count, 2);
    assert_eq!(bundle.chapters[0].content, "Chapter 1 body content");
    assert_eq!(bundle.chapters[1].content, "Chapter 2 rising action");
    // SHA-256 hashes are 64 hex chars
    assert_eq!(bundle.chapters[0].content_hash.len(), 64);
    assert_eq!(bundle.chapters[1].content_hash.len(), 64);
}

// ---------------------------------------------------------------------------
// 9. Path helpers produce correct paths
// ---------------------------------------------------------------------------
#[test]
fn test_path_helpers() {
    use nexus_orchestration::sync_module::{chapter_body_path, works_dir};
    let ws = std::path::Path::new("/tmp/workspace");

    assert_eq!(
        works_dir(ws, "my-novel"),
        std::path::PathBuf::from("/tmp/workspace/Works/my-novel")
    );
    assert_eq!(
        chapter_body_path(ws, "my-novel", "ch01-intro.md"),
        std::path::PathBuf::from("/tmp/workspace/Works/my-novel/Stories/ch01-intro.md")
    );
}
