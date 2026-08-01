//! Filesystem walker shared between `build.rs` and the crate's unit tests.
//!
//! Cargo's `cargo:rerun-if-changed=<dir>` directive does **not** recurse into
//! subdirectories — edits to nested module source files (e.g.
//! `modules/basic-combat/src/lib.rs`) do not re-trigger the build script, so
//! the embedded `.wasm` blobs go stale (R-V1147P0-001). `build.rs` therefore
//! emits one `rerun-if-changed` directive per file, using [`walk_files`].
//!
//! The module is compiled twice: from `build.rs` (as `mod build_walk;`) and,
//! under `#[cfg(test)]`, from the crate's lib test harness (via
//! `#[path = "../build_walk.rs"]`) so the walker's unit tests run under
//! `cargo test`.

use std::fs;
use std::path::{Path, PathBuf};

/// Recursively collect every file under `dir` into `out` (depth-first).
///
/// Unreadable entries are skipped defensively; the build script's own
/// freshness check (`is_fresh`) and the per-module compile step surface real
/// source problems loudly.
pub fn walk_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = path.metadata() else {
            continue;
        };
        if meta.is_dir() {
            walk_files(&path, out);
        } else if meta.is_file() {
            out.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, contents: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    /// R-V1147P0-001 regression: the emitted `rerun-if-changed` list must
    /// cover files in nested subdirectories, not just direct children.
    #[test]
    fn walk_files_covers_nested_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(&root.join("src/lib.rs"), "pub fn a() {}");
        write(&root.join("src/combat/hit.rs"), "pub fn b() {}");
        write(&root.join("src/combat/damage/calc.rs"), "pub fn c() {}");
        write(&root.join("Cargo.toml"), "[package]");

        let mut files = Vec::new();
        walk_files(root, &mut files);
        let names: Vec<String> = files
            .iter()
            .map(|p| p.strip_prefix(root).unwrap().to_string_lossy().into_owned())
            .collect();
        assert!(names.contains(&"src/lib.rs".to_string()), "names={names:?}");
        assert!(
            names.contains(&"src/combat/hit.rs".to_string()),
            "nested file must be covered: {names:?}"
        );
        assert!(
            names.contains(&"src/combat/damage/calc.rs".to_string()),
            "deeply nested file must be covered: {names:?}"
        );
        assert!(names.contains(&"Cargo.toml".to_string()), "names={names:?}");
    }

    #[test]
    fn walk_files_empty_or_missing_dir_yields_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let mut files = Vec::new();
        walk_files(&dir.path().join("does-not-exist"), &mut files);
        assert!(files.is_empty());

        let empty = dir.path().join("empty");
        fs::create_dir_all(&empty).unwrap();
        walk_files(&empty, &mut files);
        assert!(files.is_empty());
    }

    #[test]
    fn walk_files_returns_files_only() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(&root.join("nested/deep/file.txt"), "x");
        let mut files = Vec::new();
        walk_files(root, &mut files);
        let names: Vec<String> = files
            .iter()
            .map(|p| p.strip_prefix(root).unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["nested/deep/file.txt".to_string()]);
    }
}
