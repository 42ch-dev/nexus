//! V1.50 T-B P2 — `creator kb rescan` hermetic round-trip.
//!
//! Plan: `.mstar/plans/2026-06-18-v1.50-kb-refreshable-scan.md`
//! Spec: `.mstar/specs/entity-scope-model.md` §5.5
//!
//! Drives `nexus42::commands::creator::kb::rescan::kb_rescan_hermetic` against
//! a fresh temp DB + temp workspace so the rescan can be exercised without
//! `$HOME` or a daemon.
//!
//! Covers:
//! - **AC1**: idempotent re-run on unchanged text produces an empty diff.
//! - **AC2/AC5**: edited chapter text triggers candidate upsert + KB refresh;
//!   rescan with no edit produces an empty diff.
//! - **AC3**: `--dry-run` shows the diff without writing.
//! - **AC4**: cross-author attempt returns `403` (`WORLD_KB_FORBIDDEN`).
//!
//! v1.193 P0-T13 retires the deprecated `creator kb --scope world` forwarding
//! and the `creator reference refresh` entrance. Those two contracts are pinned
//! here through the real binary against a hermetic direct-core home
//! (`common/direct.rs`): the refused legacy scope leaves the home untouched,
//! `reference refresh` is unknown, and the retained local registry plus the
//! User-knowledge leaves stay callable.
//!
//! Run with: cargo test -p nexus42 --test `kb_rescan_cli`

#![allow(clippy::unwrap_used)]

#[path = "common/direct.rs"]
mod direct;

use direct::DirectFixture;
use nexus_home_layout::{creator_kb_dir, nexus_root_from_home};
use nexus42::commands::creator::kb::rescan::{kb_rescan_hermetic, WORLD_KB_FORBIDDEN_CODE};
use nexus42::commands::creator::world::kb::kb_adopt;
use nexus42::errors::CliError;
use nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryBody;
use nexus_knowledge::world_kb::KbStore;
use nexus_local_db::kb_extract_job::{insert_pending, list_pending_for_world};
use nexus_local_db::kb_store::SqliteKbStore;
use std::path::Path;

const OWNER: &str = "ctr_owner";
const OTHER: &str = "ctr_other";
const WORLD: &str = "wld_rescan";
const WORK_ID: &str = "wrk_rescan";
/// Human-readable `work_ref` (= `works.story_ref`) the CLI targets.
const WORK_REF: &str = "rescan-novel";
const CHAPTER_BODY_REL: &str = "Works/rescan-novel/Stories/01-chapter.md";

/// Build a fresh migrated pool + seed a world (owned by OWNER), a works row
/// (with `story_ref = WORK_REF` + `world_id = WORLD`), and a `work_chapters`
/// row pointing at `CHAPTER_BODY_REL`.
async fn fresh_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("state.db");
    let pool = nexus_local_db::init_engine_pool(&db_path)
        .await
        .unwrap()
        .clone_pool();
    nexus_local_db::kb_store::seed::world(
        &pool,
        WORLD,
        OWNER,
        "Rescan World",
        "rescan-world",
        "private",
        "manual",
    )
    .await;
    seed_work_and_chapter(&pool).await;
    (pool, dir)
}

async fn seed_work_and_chapter(pool: &sqlx::SqlitePool) {
    // SAFETY: test-only seed against the known works table schema.
    sqlx::query(
        "INSERT OR IGNORE INTO works \
         (work_id, creator_id, workspace_slug, status, title, long_term_goal, \
          initial_idea, intake_status, world_id, story_ref, created_at, updated_at) \
         VALUES (?, ?, 'ws', 'active', 'Rescan Novel', 'goal', 'idea', 'complete', \
                 ?, ?, datetime('now'), datetime('now'))",
    )
    .bind(WORK_ID)
    .bind(OWNER)
    .bind(WORLD)
    .bind(WORK_REF)
    .execute(pool)
    .await
    .unwrap();

    // Seed the chapter row the rescan reads.
    sqlx::query(
        "INSERT OR IGNORE INTO work_chapters \
         (work_id, chapter, volume, slug, planned_word_count, status, body_path, \
          created_at, updated_at) \
         VALUES (?, 1, 1, 'chapter', 100, 'finalized', ?, datetime('now'), datetime('now'))",
    )
    .bind(WORK_ID)
    .bind(CHAPTER_BODY_REL)
    .execute(pool)
    .await
    .unwrap();
}

/// Write chapter prose to `<ws_dir>/<CHAPTER_BODY_REL>`.
fn write_chapter_prose(ws_dir: &Path, prose: &str) {
    let path = ws_dir.join(CHAPTER_BODY_REL);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, prose).unwrap();
}

fn target() -> String {
    format!("{WORK_REF}/1")
}

// ── AC1: idempotent re-run on unchanged text → empty diff ──────────────────

#[tokio::test]
async fn rescan_idempotent_rerun_produces_empty_diff() {
    let (pool, dir) = fresh_pool().await;
    write_chapter_prose(dir.path(), "Lin Xia walked into the tavern.");

    let first = kb_rescan_hermetic(&pool, OWNER, Some(dir.path()), &target(), false)
        .await
        .unwrap();
    assert!(
        first.candidates_inserted.iter().any(|n| n == "Lin Xia"),
        "first scan should insert 'Lin Xia': {:?}",
        first.candidates_inserted
    );

    // Re-run on unchanged text → no candidate changes, no kb updates.
    let second = kb_rescan_hermetic(&pool, OWNER, Some(dir.path()), &target(), false)
        .await
        .unwrap();
    assert!(
        second.is_empty(),
        "idempotent re-run must produce an empty diff (AC1), got: {second:?}"
    );
    assert_eq!(second.candidates_unchanged, 1);
}

// ── AC2/AC5: edited text → candidate upsert + KB refresh; no edit → empty ──

#[tokio::test]
async fn rescan_after_chapter_edit_updates_candidate_rows() {
    let (pool, dir) = fresh_pool().await;
    write_chapter_prose(dir.path(), "Lin Xia walked into the tavern.");

    kb_rescan_hermetic(&pool, OWNER, Some(dir.path()), &target(), false)
        .await
        .unwrap();

    // Edit the chapter: drop "Lin Xia", add "Marcus Vale".
    write_chapter_prose(dir.path(), "Marcus Vale surveyed the quiet harbor.");

    let after_edit = kb_rescan_hermetic(&pool, OWNER, Some(dir.path()), &target(), false)
        .await
        .unwrap();
    assert!(
        after_edit
            .candidates_inserted
            .iter()
            .any(|n| n == "Marcus Vale"),
        "edited text should insert 'Marcus Vale': {:?}",
        after_edit.candidates_inserted
    );
    assert!(
        after_edit.candidates_removed.iter().any(|n| n == "Lin Xia"),
        "edited text should remove stale 'Lin Xia': {:?}",
        after_edit.candidates_removed
    );

    // KB rows reflect the new extraction: Marcus Vale is advisory-new (no
    // KnowledgeEntryRecord yet), Lin Xia is advisory-removed (no KnowledgeEntryRecord either, since it
    // was only ever a pending candidate here).
    assert!(after_edit
        .candidates_inserted
        .iter()
        .any(|n| n == "Marcus Vale"));

    // No-edit re-run → empty diff (AC5 second half).
    let idle = kb_rescan_hermetic(&pool, OWNER, Some(dir.path()), &target(), false)
        .await
        .unwrap();
    assert!(idle.is_empty(), "no-edit rerun must be empty: {idle:?}");
}

#[tokio::test]
async fn rescan_refreshes_out_of_sync_confirmed_keyblock_body() {
    let (pool, dir) = fresh_pool().await;
    write_chapter_prose(dir.path(), "Lin Xia walked into the tavern.");

    // First scan + adopt → confirmed KnowledgeEntryRecord carrying the heuristic payload.
    let scan = kb_rescan_hermetic(&pool, OWNER, Some(dir.path()), &target(), false)
        .await
        .unwrap();
    let pending = list_pending_for_world(&pool, WORLD, None).await.unwrap();
    let lin_xia = pending
        .iter()
        .find(|r| r.canonical_name_guess.as_deref() == Some("Lin Xia"))
        .unwrap();
    kb_adopt(&pool, OWNER, &lin_xia.job_id, None, false)
        .await
        .unwrap();
    let _ = scan;

    // Manually drift the confirmed KnowledgeEntryRecord body away from the chapter's
    // extraction (simulates a body that fell out of sync with the source text).
    let store = SqliteKbStore::new(pool.clone());
    let mut blocks = store.list_by_world(WORLD).await.unwrap();
    let mut kb = blocks.remove(0);
    kb.body = Some(KnowledgeEntryBody {
        summary: Some("stale hand-edited body".to_string()),
        attributes: Some(serde_json::json!({"novel_category": "character"})),
        tags: None,
        ..Default::default()
    });
    kb.updated_at = Some(chrono::Utc::now().to_rfc3339());
    store.update_knowledge_entry(kb).await.unwrap();

    // Rescan → diff_and_apply refreshes the confirmed body back to the
    // extraction, so KB rows reflect the current chapter text (AC2/AC5).
    let rescan = kb_rescan_hermetic(&pool, OWNER, Some(dir.path()), &target(), false)
        .await
        .unwrap();
    assert!(
        rescan.kb_updated.iter().any(|n| n == "Lin Xia"),
        "rescan should refresh the out-of-sync 'Lin Xia' KnowledgeEntryRecord: {:?}",
        rescan.kb_updated
    );

    // The stored body now matches the heuristic extraction again.
    let after = SqliteKbStore::new(pool.clone())
        .list_by_world(WORLD)
        .await
        .unwrap();
    assert!(
        after[0]
            .body
            .as_ref()
            .and_then(|b| b.summary.as_deref())
            .unwrap_or("")
            .contains("Lin Xia"),
        "refreshed body should reflect the chapter extraction: {:?}",
        after[0].body
    );

    // No-edit re-run → empty diff (the body is back in sync).
    let idle = kb_rescan_hermetic(&pool, OWNER, Some(dir.path()), &target(), false)
        .await
        .unwrap();
    assert!(idle.is_empty(), "no-edit rerun must be empty: {idle:?}");
}

// ── AC3: dry-run shows diff without writing ───────────────────────────────

#[tokio::test]
async fn dry_run_shows_diff_without_writing() {
    let (pool, dir) = fresh_pool().await;
    write_chapter_prose(dir.path(), "Lin Xia walked into the tavern.");

    let dry = kb_rescan_hermetic(&pool, OWNER, Some(dir.path()), &target(), true)
        .await
        .unwrap();
    assert!(dry.dry_run);
    assert!(
        dry.candidates_inserted.iter().any(|n| n == "Lin Xia"),
        "dry-run should preview the 'Lin Xia' insert: {:?}",
        dry.candidates_inserted
    );
    assert!(
        dry.kb_inserted_advisory.iter().any(|n| n == "Lin Xia"),
        "dry-run should preview the advisory KB insert: {:?}",
        dry.kb_inserted_advisory
    );

    // Nothing was actually written: no pending candidate, no KnowledgeEntryRecord.
    let pending = list_pending_for_world(&pool, WORLD, None).await.unwrap();
    assert!(
        pending.is_empty(),
        "dry-run must not write candidates, got: {pending:?}"
    );
    let blocks = SqliteKbStore::new(pool.clone())
        .list_by_world(WORLD)
        .await
        .unwrap();
    assert!(blocks.is_empty(), "dry-run must not write KnowledgeEntries");
}

// ── AC4: cross-author attempt returns 403 ──────────────────────────────────

#[tokio::test]
async fn rescan_cross_author_returns_403() {
    let (pool, dir) = fresh_pool().await;
    write_chapter_prose(dir.path(), "Lin Xia walked into the tavern.");

    let err = kb_rescan_hermetic(&pool, OTHER, Some(dir.path()), &target(), false)
        .await
        .unwrap_err();
    match err {
        CliError::Api { status, message } => {
            assert_eq!(status, 403);
            assert!(
                message.contains(WORLD_KB_FORBIDDEN_CODE),
                "expected {WORLD_KB_FORBIDDEN_CODE} in: {message}"
            );
        }
        other => panic!("expected Api 403, got: {other:?}"),
    }

    // No candidate was written (the gate fires before any upsert).
    let pending = list_pending_for_world(&pool, WORLD, None).await.unwrap();
    assert!(pending.is_empty());
}

// ── Error handling: malformed target / missing work ────────────────────────

#[tokio::test]
async fn malformed_target_returns_clean_error() {
    let (pool, dir) = fresh_pool().await;
    let err = kb_rescan_hermetic(&pool, OWNER, Some(dir.path()), "no-slash-here", false)
        .await
        .unwrap_err();
    match err {
        CliError::Other(msg) => assert!(msg.contains("<work_ref>/<chapter>")),
        other => panic!("expected Other error for malformed target, got: {other:?}"),
    }
}

#[tokio::test]
async fn missing_work_returns_clean_error() {
    let (pool, dir) = fresh_pool().await;
    write_chapter_prose(dir.path(), "Lin Xia walked into the tavern.");
    let err = kb_rescan_hermetic(&pool, OWNER, Some(dir.path()), "no-such-work/1", false)
        .await
        .unwrap_err();
    match err {
        CliError::Other(msg) => assert!(msg.contains("no-such-work")),
        other => panic!("expected Other error for missing work, got: {other:?}"),
    }
}

// ── Sanity: a pre-existing pending candidate is reused, not duplicated ─────

#[tokio::test]
async fn rescan_reuses_preexisting_pending_candidate_without_duplicate() {
    let (pool, dir) = fresh_pool().await;
    write_chapter_prose(dir.path(), "Lin Xia walked into the tavern.");

    // Seed a pending candidate exactly as the review-time hook would.
    let payload = serde_json::json!({
        "summary": "Candidate extracted from chapter prose: Lin Xia",
        "attributes": {"novel_category": "character", "aliases": ["Lin Xia"]},
        "tags": ["novel", "heuristic-extracted"],
    })
    .to_string();
    insert_pending(
        &pool,
        OWNER,
        "ws",
        WORLD,
        Some(WORK_ID),
        Some(1),
        "character",
        "Lin Xia",
        &payload,
    )
    .await
    .unwrap();

    let report = kb_rescan_hermetic(&pool, OWNER, Some(dir.path()), &target(), false)
        .await
        .unwrap();
    // The pre-existing candidate is reused (unchanged), not re-inserted.
    assert!(
        report.candidates_inserted.is_empty(),
        "should not duplicate the pre-existing candidate: {report:?}"
    );
    assert_eq!(report.candidates_unchanged, 1);

    // Still exactly one row.
    let pending = list_pending_for_world(&pool, WORLD, None).await.unwrap();
    assert_eq!(
        pending
            .iter()
            .filter(|r| r.canonical_name_guess.as_deref() == Some("Lin Xia"))
            .count(),
        1
    );
}

// ═══════════════════════════════════════════════════════════════════════
// v1.193 P0-T13 — obsolete entrances retired, retained neighbours intact
// ═══════════════════════════════════════════════════════════════════════

/// Every file under `root` as `(relative path, byte length, mtime)`, sorted.
///
/// A refused parser invocation must leave this identical: the retired
/// world-scope branch opened — and therefore migrated/created — the selected
/// workspace before it could fail.
fn snapshot_tree(root: &Path) -> Vec<(std::path::PathBuf, u64, Option<std::time::SystemTime>)> {
    fn walk(
        dir: &Path,
        root: &Path,
        out: &mut Vec<(std::path::PathBuf, u64, Option<std::time::SystemTime>)>,
    ) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let relative = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
            let metadata = std::fs::metadata(&path).ok();
            out.push((
                relative,
                metadata.as_ref().map_or(0, std::fs::Metadata::len),
                metadata.and_then(|m| m.modified().ok()),
            ));
            if path.is_dir() {
                walk(&path, root, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

/// The retired `creator kb --scope world` spelling is refused by the parser,
/// and the refused invocation writes nothing.
///
/// Regression: the compatibility branch must not survive as a parseable no-op
/// (`--scope world` was a forwarding alias whose target surface never gained a
/// `search`/`add` leaf), and a refused invocation must not materialize — or
/// migrate — the selected workspace the way the old forwarding arm did. The
/// canonical World KB behavior itself is unchanged and stays covered by
/// `world_kb_alias.rs` (`kb_list` / `kb_show` / `kb_delete`).
#[tokio::test]
async fn work_kb_scope_rejects_legacy_world_without_mutation() {
    let fixture = DirectFixture::new().await;
    let before = snapshot_tree(fixture.home.path());

    let output = fixture
        .command()
        .args([
            "creator", "kb", "list", "--scope", "world", "--world-id", "wld_legacy",
        ])
        .output()
        .expect("spawn the real nexus42 binary");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(2),
        "the legacy world scope must be a usage error, got {output:?}"
    );
    assert!(
        stderr.contains("--scope"),
        "the refusal must name the removed flag, got:\n{stderr}"
    );
    assert_eq!(
        snapshot_tree(fixture.home.path()),
        before,
        "a refused legacy-world invocation must not write anything"
    );
}

/// The retired reference refresh entrance is unknown.
///
/// Regression: `creator reference refresh` must not remain a parseable no-op
/// (it had no complete direct CLI operation), so the parser refuses it before
/// any handler — and therefore before any registry write — runs.
#[tokio::test]
async fn retired_reference_refresh_is_unknown() {
    let fixture = DirectFixture::new().await;

    let output = fixture
        .command()
        .args(["creator", "reference", "refresh", "all"])
        .output()
        .expect("spawn the real nexus42 binary");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(
        output.status.code(),
        Some(2),
        "`creator reference refresh` must be an unknown subcommand, got {output:?}"
    );
    assert!(
        stderr.contains("refresh"),
        "the refusal must name the retired subcommand, got:\n{stderr}"
    );
}

/// The surviving help surfaces render, and neither advertises a retired leaf.
#[tokio::test]
async fn reference_and_knowledge_help_survive_without_retired_leaves() {
    let fixture = DirectFixture::new().await;

    let reference_help = fixture
        .command()
        .args(["creator", "reference", "--help"])
        .output()
        .expect("spawn the real nexus42 binary");
    assert!(
        reference_help.status.success(),
        "`creator reference --help` must still render: {}",
        String::from_utf8_lossy(&reference_help.stderr)
    );
    let reference_help_text = String::from_utf8_lossy(&reference_help.stdout).into_owned();
    let reference_subcommands: Vec<&str> = subcommands(&reference_help_text).collect();
    for subcommand in ["register", "list", "show"] {
        assert!(
            reference_subcommands.contains(&subcommand),
            "`creator reference --help` must still offer '{subcommand}', got:\n{reference_help_text}"
        );
    }
    assert!(
        !reference_subcommands.contains(&"refresh"),
        "`creator reference --help` must not advertise the retired leaf, got:\n{reference_help_text}"
    );

    let knowledge_help = fixture
        .command()
        .args(["creator", "knowledge", "--help"])
        .output()
        .expect("spawn the real nexus42 binary");
    assert!(
        knowledge_help.status.success(),
        "`creator knowledge --help` must still render: {}",
        String::from_utf8_lossy(&knowledge_help.stderr)
    );
    let knowledge_help_text = String::from_utf8_lossy(&knowledge_help.stdout).into_owned();
    let knowledge_subcommands: Vec<&str> = subcommands(&knowledge_help_text).collect();
    for subcommand in ["add", "list", "search"] {
        assert!(
            knowledge_subcommands.contains(&subcommand),
            "`creator knowledge --help` must still offer '{subcommand}', got:\n{knowledge_help_text}"
        );
    }
}

/// The retained reference registry stays callable end to end without a daemon.
#[tokio::test]
async fn retained_reference_registry_round_trips_without_a_daemon() {
    let fixture = DirectFixture::new().await;

    let register = fixture
        .command()
        .args([
            "creator",
            "reference",
            "register",
            "--source",
            "https://example.invalid/t13-notes",
            "--source-type",
            "url",
            "--title",
            "Retained reference",
            "--body",
            "Reference body retained by the local registry.",
        ])
        .output()
        .expect("spawn the real nexus42 binary");
    assert!(
        register.status.success(),
        "`reference register` must stay callable: {}",
        String::from_utf8_lossy(&register.stderr)
    );
    let reference_id = String::from_utf8_lossy(&register.stdout)
        .lines()
        .find_map(|line| line.trim().strip_prefix("✓ Reference registered: "))
        .expect("`reference register` must report the new reference id")
        .trim()
        .to_string();

    let list = fixture
        .command()
        .args(["creator", "reference", "list"])
        .output()
        .expect("spawn the real nexus42 binary");
    assert!(
        list.status.success(),
        "`reference list` must stay callable: {}",
        String::from_utf8_lossy(&list.stderr)
    );
    assert!(
        String::from_utf8_lossy(&list.stdout).contains(&reference_id),
        "the registered reference must be listed"
    );

    let show = fixture
        .command()
        .args(["creator", "reference", "show", &reference_id])
        .output()
        .expect("spawn the real nexus42 binary");
    assert!(
        show.status.success(),
        "`reference show` must stay callable: {}",
        String::from_utf8_lossy(&show.stderr)
    );
    assert!(
        String::from_utf8_lossy(&show.stdout).contains("Retained reference"),
        "`reference show` must render the registered title"
    );
}

/// The retained User-knowledge leaves stay callable through the typed core.
#[tokio::test]
async fn retained_knowledge_leaves_round_trip_through_the_core() {
    let fixture = DirectFixture::new().await;

    let add = fixture
        .command()
        .args([
            "creator",
            "knowledge",
            "add",
            "Retained knowledge entry",
            "--tags",
            "t13,smoke",
        ])
        .output()
        .expect("spawn the real nexus42 binary");
    assert!(
        add.status.success(),
        "`knowledge add` must stay callable: {}",
        String::from_utf8_lossy(&add.stderr)
    );
    assert!(
        String::from_utf8_lossy(&add.stdout).contains("Knowledge entry added"),
        "`knowledge add` must confirm the stored entry"
    );

    let search = fixture
        .command()
        .args(["creator", "knowledge", "search", "Retained"])
        .output()
        .expect("spawn the real nexus42 binary");
    assert!(
        search.status.success(),
        "`knowledge search` must stay callable: {}",
        String::from_utf8_lossy(&search.stderr)
    );
    assert!(
        String::from_utf8_lossy(&search.stdout).contains("Retained knowledge entry"),
        "`knowledge search` must find the entry just added through the core"
    );
}

/// The retained work-scope index leaves round-trip through the direct core.
///
/// Regression: `list|search|show|add|remove` must keep working against the
/// local file index with no daemon probe and no HTTP fallback — the same
/// surface the core owns through `list_kb_entries` / `add_kb_entry` /
/// `get_kb_entry` / `delete_kb_entry`. Covered together because the id the
/// `add` leaf reports is the only handle the other three accept.
#[tokio::test]
async fn work_kb_index_leaves_round_trip_without_a_daemon() {
    let fixture = DirectFixture::new().await;
    let source = fixture.home.path().join("retained-note.md");
    std::fs::write(&source, "Chapter notes for the retained work index.\n").unwrap();
    let source_path = source.to_str().unwrap();

    let add = fixture
        .command()
        .args([
            "creator", "kb", "add", "--file", source_path, "--title", "Retained note",
        ])
        .output()
        .expect("spawn the real nexus42 binary");
    assert!(
        add.status.success(),
        "`kb add` must stay callable: {}",
        String::from_utf8_lossy(&add.stderr)
    );
    let entry_id = String::from_utf8_lossy(&add.stdout)
        .lines()
        .find_map(|line| line.trim().strip_prefix("✓ Local work entry added: "))
        .expect("`kb add` must report the stored entry id")
        .trim()
        .to_string();
    assert!(
        entry_id.starts_with("kb_") && entry_id.len() == 15,
        "the core's entry-id contract is `kb_` + 12 hex chars, got {entry_id:?}"
    );

    let list = fixture
        .command()
        .args(["creator", "kb", "list"])
        .output()
        .expect("spawn the real nexus42 binary");
    assert!(
        list.status.success(),
        "`kb list` must stay callable: {}",
        String::from_utf8_lossy(&list.stderr)
    );
    let list_stdout = String::from_utf8_lossy(&list.stdout).into_owned();
    assert!(
        list_stdout.contains(&entry_id) && list_stdout.contains("Retained note"),
        "the added entry must be listed, got:\n{list_stdout}"
    );

    let search = fixture
        .command()
        .args(["creator", "kb", "search", "Retained"])
        .output()
        .expect("spawn the real nexus42 binary");
    assert!(
        search.status.success(),
        "`kb search` must stay callable: {}",
        String::from_utf8_lossy(&search.stderr)
    );
    assert!(
        String::from_utf8_lossy(&search.stdout).contains(&entry_id),
        "`kb search` must find the entry by title"
    );

    let show = fixture
        .command()
        .args(["creator", "kb", "show", &entry_id])
        .output()
        .expect("spawn the real nexus42 binary");
    assert!(
        show.status.success(),
        "`kb show` must stay callable: {}",
        String::from_utf8_lossy(&show.stderr)
    );
    assert!(
        String::from_utf8_lossy(&show.stdout).contains("Chapter notes for the retained work index."),
        "`kb show` must render the stored entry body"
    );

    let remove = fixture
        .command()
        .args(["creator", "kb", "remove", &entry_id])
        .output()
        .expect("spawn the real nexus42 binary");
    assert!(
        remove.status.success(),
        "`kb remove` must stay callable: {}",
        String::from_utf8_lossy(&remove.stderr)
    );

    let after = fixture
        .command()
        .args(["creator", "kb", "list"])
        .output()
        .expect("spawn the real nexus42 binary");
    assert!(
        after.status.success(),
        "`kb list` must stay callable after a removal"
    );
    assert!(
        !String::from_utf8_lossy(&after.stdout).contains(&entry_id),
        "the removed entry must be gone from the index"
    );
}

/// A corrupt local work index is reported as empty, not as an error.
///
/// Retained behavior (R-KB-001) now served by the core's own index reader: the
/// leaf must stay callable and show an empty index instead of failing. The
/// former CLI-side unit tests covered a duplicate reader that this task removes.
#[tokio::test]
async fn corrupt_work_index_is_reported_as_empty_not_an_error() {
    let fixture = DirectFixture::new().await;
    let creators_root = nexus_root_from_home(fixture.home.path()).join("creators");
    let creator_id = std::fs::read_dir(&creators_root)
        .expect("the fixture created the creators root")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .next()
        .expect("the fixture registered exactly one creator");

    let kb_dir = creator_kb_dir(fixture.home.path(), &creator_id, "default");
    std::fs::create_dir_all(&kb_dir).expect("create the kb dir");
    std::fs::write(kb_dir.join("index.json"), "not json at all {{{").expect("write corrupt index");

    let list = fixture
        .command()
        .args(["creator", "kb", "list"])
        .output()
        .expect("spawn the real nexus42 binary");
    assert!(
        list.status.success(),
        "a corrupt index must not fail the leaf: {}",
        String::from_utf8_lossy(&list.stderr)
    );
    assert!(
        String::from_utf8_lossy(&list.stdout).contains("No local work entries in workspace default"),
        "a corrupt index must read as empty, got:\n{}",
        String::from_utf8_lossy(&list.stdout)
    );
}

/// The subcommand names a clap help text lists under its `Commands:` section.
fn subcommands(help_text: &str) -> impl Iterator<Item = &str> {
    help_text
        .lines()
        .skip_while(|line| line.trim_end() != "Commands:")
        .skip(1)
        .take_while(|line| !line.trim().is_empty())
        .filter_map(|line| line.split_whitespace().next())
}
