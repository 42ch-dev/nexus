//! Source guard: `kb_key_blocks` write statements live only in `crates/nexus-local-db/src/kb_store.rs`.
//!
//! Classification mirrors `.mstar/sdd/v233-knowledge-ir-p1/inventory.md` §3: `tests/` and
//! `migrations/` are fixtures by path, a file's last exact `#[cfg(test)]` line cuts its test module,
//! and a module declared behind a `#[cfg(test…)]` attribute in its parent `mod.rs` is test-only.

use std::path::{Path, PathBuf};

const SCAN_ROOTS: [&str; 2] = ["crates", "apps"];
const WRITER_HOME: &str = "crates/nexus-local-db/src/kb_store.rs";
/// inventory.md §5 F3: shared disposable-home seeding module. Every caller is a `#[cfg(test)]` module,
/// a `#[tokio::test]` body, or the `native-wire-fixture-seed` bin that only Node proof scripts spawn.
const TEST_SUPPORT_FILES: [&str; 1] = ["crates/nexus-core-node/src/wire_fixture.rs"];

fn rust_sources(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("readable workspace directory") {
        let path = entry.expect("readable directory entry").path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name != "target") {
                rust_sources(&path, files);
            }
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
}

/// The file text before its last line that is exactly `#[cfg(test)]`, i.e. without the test module.
fn production_prefix(text: &str) -> &str {
    let mut cut = text.len();
    let mut offset = 0;
    for line in text.split_inclusive('\n') {
        if line.trim_end_matches(['\r', '\n']) == "#[cfg(test)]" {
            cut = offset;
        }
        offset += line.len();
    }
    &text[..cut]
}

/// True when the sibling `mod.rs` declares this module behind a `#[cfg(test…)]` attribute.
fn declared_behind_cfg_test(path: &Path) -> bool {
    let (Some(stem), Some(dir)) = (path.file_stem().and_then(|s| s.to_str()), path.parent()) else {
        return false;
    };
    let Ok(parent) = std::fs::read_to_string(dir.join("mod.rs")) else {
        return false;
    };
    let lines: Vec<&str> = parent.lines().collect();
    let declaration = format!("mod {stem};");
    (1..lines.len()).any(|index| {
        lines[index].trim() == declaration && {
            let attribute = lines[index - 1].trim();
            attribute.starts_with("#[cfg(test") || attribute.starts_with("#[cfg(all(test")
        }
    })
}

/// Token equality is the delimiter guard: `kb_key_blocks_new` and `my_kb_key_blocks` do not match.
fn writes_kb_key_blocks(line: &str) -> bool {
    let tokens: Vec<&str> = line
        .split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|token| !token.is_empty())
        .collect();
    tokens
        .windows(2)
        .any(|pair| pair == ["UPDATE", "kb_key_blocks"])
        || tokens.windows(3).any(|triple| {
            triple == ["INSERT", "INTO", "kb_key_blocks"]
                || triple == ["DELETE", "FROM", "kb_key_blocks"]
        })
}

#[test]
fn kb_key_blocks_writes_live_only_in_the_store() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("the crate sits two levels below the workspace root")
        .to_path_buf();

    let mut sources = Vec::new();
    for root in SCAN_ROOTS {
        rust_sources(&workspace.join(root), &mut sources);
    }

    let mut offenders = Vec::new();
    for path in sources {
        let relative = path
            .strip_prefix(&workspace)
            .expect("every scanned file sits below the workspace root")
            .to_string_lossy()
            .replace('\\', "/");
        if relative == WRITER_HOME
            || TEST_SUPPORT_FILES.contains(&relative.as_str())
            || relative
                .split('/')
                .any(|component| component == "tests" || component == "migrations")
            || declared_behind_cfg_test(&path)
        {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("readable source file");
        for (index, line) in production_prefix(&text).lines().enumerate() {
            if writes_kb_key_blocks(line) {
                offenders.push(format!("{relative}:{}", index + 1));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "production `kb_key_blocks` writes outside `{WRITER_HOME}`: {offenders:?} — \
         converge the writer onto a `kb_store.rs` primitive (inventory.md §3 rule 8)"
    );
}
