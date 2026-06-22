//! V1.58 P0 fix-wave (QC2 H-3 regression guard): assert the `.sqlx/`
//! compile-time query cache is present and non-empty in the working tree.
//!
//! Background: the V1.58 P1 merge ran `cargo sqlx prepare` without the
//! `--tests` flag, which deleted 137 of 138 `query-*.json` artifacts
//! (R-V156P1-CACHE-01). `SQLX_OFFLINE=true cargo check --workspace --tests`
//! then failed with 83+ "no cached statement" errors. The PM restored the
//! cache in commit `af82ad39`. This test guards against accidental mass
//! deletion recurring — it does NOT validate query correctness (that is the
//! job of `SQLX_OFFLINE=true cargo check --workspace --tests` in CI).
//!
//! Threshold is intentionally conservative (>= 50) so adding/removing a few
//! queries does not trip it, but a mass deletion (137 lost) would.

#![allow(clippy::unwrap_used)]

use std::path::Path;

/// Locate the workspace `.sqlx/` directory from this crate's manifest dir.
///
/// `CARGO_MANIFEST_DIR` = `.../nexus/crates/nexus-local-db` at compile time.
/// The workspace root is two levels up; `.sqlx/` lives at the workspace root.
fn workspace_sqlx_dir() -> std::path::PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    // crates/nexus-local-db → workspace root is ../../
    manifest_dir
        .join("../../.sqlx")
        // Normalized for cross-platform comparison; if the workspace is
        // relocated this test still resolves correctly via CARGO_MANIFEST_DIR.
        .canonicalize()
        .unwrap_or_else(|_| manifest_dir.join("../../.sqlx"))
}

/// Conservative floor on the number of `query-*.json` artifacts that must be
/// committed. The V1.58 tree has ~140; the P1 incident dropped it to 1.
/// A threshold of 50 catches mass deletion without being brittle to normal
/// query add/remove churn.
const SQLX_CACHE_MIN_FILES: usize = 50;

#[test]
fn sqlx_cache_is_present_and_non_empty() {
    let sqlx_dir = workspace_sqlx_dir();
    assert!(
        sqlx_dir.exists(),
        ".sqlx/ cache directory not found at {} — the compile-time query \
         cache must be committed. Run \
         `DATABASE_URL=\\\"sqlite:.sqlx/state.db?mode=rwc\\\" cargo sqlx \
         prepare --workspace -- --tests` and commit the query-*.json files. \
         See daemon-runtime.md §V1.58 P0 overlay (.sqlx cache hygiene).",
        sqlx_dir.display()
    );

    let count = std::fs::read_dir(&sqlx_dir)
        .unwrap_or_else(|e| panic!("read .sqlx/ dir at {}: {e}", sqlx_dir.display()))
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_name().to_string_lossy().starts_with("query-")
                && entry.file_name().to_string_lossy().ends_with(".json")
        })
        .count();

    assert!(
        count >= SQLX_CACHE_MIN_FILES,
        ".sqlx/ cache has only {count} query-*.json files (expected >= \
         {SQLX_CACHE_MIN_FILES}). This indicates accidental mass deletion — \
         the V1.58 P1 incident (R-V156P1-CACHE-01) dropped the count from \
         138 to 1 by running `cargo sqlx prepare` without `--tests`. Re-run \
         with `--tests` and commit the regenerated artifacts."
    );
}
