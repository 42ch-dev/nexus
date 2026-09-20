//! Server-free CLI integration tests — `creator world kb entity patch`,
//! `creator world kb graph` and `creator world kb pack import`
//! (V1.175 P1 Task 4, group 4; direct-core retarget v1.193 P0-T3/T4).
//!
//! All three verbs run on the `nexus-core` direct-writer seam, so these tests
//! drive the REAL `nexus42` binary over an isolated raw `HOME` — no daemon
//! fixture, no Node child, no live provider (AR-83 #6 / AR-85), and therefore
//! also no server that could stand in for the core's own admission. The family
//! seeds its own World through the CLI itself (`creator world create --title`)
//! and its entity/relationship rows through a released-seed state-DB write.
//!
//! Retained coverage: the entity-patch CAS conflict (a stale
//! `--expected-version` names both sides, exit 76), the empty-patch fast-fail,
//! the verbatim `--json` DTO projections, the relationship-cap note, and the
//! pack-import holder-governance accident guard (an unmapped foreign holder is
//! quarantined — never published as the importing Creator's own fact — and the
//! batch is reviewable and adoptable through the CLI alone).

#[path = "common/direct.rs"]
mod direct;

use direct::DirectFixture;
use nexus_home_layout::{nexus_root_from_home, workspace_state_db_path};
use nexus_knowledge::world_kb::knowledge_entry::{
    KnowledgeEntryBody, KnowledgeEntryRecord, DISCLOSURE_OWNER_PRIVATE,
};
use nexus_knowledge::world_kb::KbStore;
use nexus_local_db::kb_store::SqliteKbStore;
use nexus_local_db::writer_protocol::release_retained_writer_guards;
use std::path::PathBuf;
use std::process::Output;

/// The fixture's workspace slug (mirrors `common/direct.rs`).
const WORKSPACE_SLUG: &str = "default";
/// The foreign holder the pack's private row claims — never this workspace's.
const FOREIGN_HOLDER: &str = "hld_foreign_peer";

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// The payload of the first output line carrying `label`.
fn labelled(out: &str, label: &str) -> String {
    out.lines()
        .find_map(|line| line.split_once(label).map(|(_, rest)| rest.trim()))
        .map_or_else(
            || panic!("no '{label}' line in output:\n{out}"),
            str::to_string,
        )
}

/// The batch id the quarantine report tells the operator to review with.
fn reported_batch_id(report: &str) -> String {
    report
        .lines()
        .find_map(|line| line.split_once("--review-import ").map(|(_, rest)| rest))
        .map_or_else(
            || panic!("no review hint carrying the batch id in:\n{report}"),
            |rest| rest.trim_end_matches([':', ')']).trim().to_string(),
        )
}

/// The fixture home holds exactly one creator; its id is the directory name
/// under `~/.nexus42/creators/`.
fn fixture_creator_id(fixture: &DirectFixture) -> String {
    let creators_root = nexus_root_from_home(fixture.home.path()).join("creators");
    let mut entries: Vec<_> = std::fs::read_dir(&creators_root)
        .expect("read fixture creators root")
        .map(|entry| entry.expect("creator dir entry").file_name())
        .collect();
    assert_eq!(entries.len(), 1, "fixture registers exactly one creator");
    entries
        .pop()
        .expect("one creator")
        .to_string_lossy()
        .into_owned()
}

/// The fixture's workspace state DB (`.../creators/<id>/workspaces/default/state.db`).
fn fixture_state_db(fixture: &DirectFixture) -> PathBuf {
    workspace_state_db_path(
        fixture.home.path(),
        &fixture_creator_id(fixture),
        WORKSPACE_SLUG,
    )
}

/// Run one `creator world kb` invocation against the fixture home.
fn kb_cli(fixture: &DirectFixture, args: &[&str]) -> Output {
    let mut full = vec!["creator", "world", "kb"];
    full.extend_from_slice(args);
    fixture
        .command()
        .args(&full)
        .output()
        .expect("spawn nexus42 world kb")
}

/// Create one owned World through the real CLI; returns its `wld_` id.
fn create_world(fixture: &DirectFixture) -> String {
    let out = fixture
        .command()
        .args(["creator", "world", "create", "--title", "KB CLI Test World"])
        .output()
        .expect("spawn nexus42 world create");
    assert!(
        out.status.success(),
        "world create failed: {}",
        stderr(&out)
    );
    let world_id = labelled(&stdout(&out), "World created:");
    assert!(world_id.starts_with("wld_"), "{world_id}");
    world_id
}

/// Seed one `KnowledgeEntryRecord` at revision 0 into the fixture's state DB
/// and return its `entry_id`.
///
/// The row is written through a registered engine writer (the fixture's
/// released seed writer), so the guarded schema admits it exactly as the CLI's
/// own writer would.
async fn seed_entity(fixture: &DirectFixture, world_id: &str) -> String {
    let db_path = fixture_state_db(fixture);
    let pool = nexus_local_db::init_engine_pool(&db_path)
        .await
        .expect("open fixture state db")
        .clone_pool();
    let store = SqliteKbStore::new(pool.clone());
    let mut kb = KnowledgeEntryRecord::new(world_id, nexus_contracts::BlockType::Character, "Hero");
    kb.body = Some(KnowledgeEntryBody {
        summary: Some("Original summary".to_string()),
        attributes: Some(serde_json::json!({"novel_category": "character"})),
        tags: Some(vec!["novel".to_string()]),
        ..Default::default()
    });
    let result = store.insert_knowledge_entry(kb).await.expect("seed entity");
    pool.close().await;
    release_retained_writer_guards(&db_path);
    result.entry_id
}

/// Seed `count` stored relationships between `entity_id` and itself through the
/// fixture's released seed writer (the graph projection's cap note needs 1000
/// rows). Both endpoints must exist, so they name the seeded entity.
async fn seed_relationships(
    fixture: &DirectFixture,
    world_id: &str,
    entity_id: &str,
    count: usize,
) {
    let db_path = fixture_state_db(fixture);
    let pool = nexus_local_db::init_engine_pool(&db_path)
        .await
        .expect("open fixture state db")
        .clone_pool();
    let now = chrono::Utc::now().to_rfc3339();
    for i in 0..count {
        // SAFETY: test-only INSERT into the known kb_relationships schema.
        sqlx::query(
            "INSERT INTO kb_relationships \
             (relationship_id, world_id, source_entity_id, target_entity_id, \
              relation_type, symmetric, created_at, updated_at, revision, \
              needs_review, source) \
             VALUES (?, ?, ?, ?, 'mentions', 0, ?, ?, 0, 0, 'manual')",
        )
        .bind(format!("rel_seed_{i}"))
        .bind(world_id)
        .bind(entity_id)
        .bind(entity_id)
        .bind(&now)
        .bind(&now)
        .execute(&pool)
        .await
        .expect("seed relationship");
    }
    pool.close().await;
    release_retained_writer_guards(&db_path);
}

/// Read one stored row's `(holder_entry_id, disclosure)` — the governance a
/// pack atom landed under, or `None` when the entry was never stored.
async fn stored_governance(
    fixture: &DirectFixture,
    entry_id: &str,
) -> Option<(Option<String>, Option<String>)> {
    let pool = nexus_local_db::open_pool_read_only(&fixture_state_db(fixture))
        .await
        .expect("open fixture state db read-only");
    let row: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT holder_entry_id, disclosure FROM kb_key_blocks WHERE key_block_id = ?",
    )
    .bind(entry_id)
    .fetch_optional(&pool)
    .await
    .expect("read stored governance");
    pool.close().await;
    row
}

/// Read one stored entry's `body.summary` (the dry-run no-write proof: an
/// overwrite plan would have replaced it).
async fn stored_summary(fixture: &DirectFixture, entry_id: &str) -> Option<String> {
    let pool = nexus_local_db::open_pool_read_only(&fixture_state_db(fixture))
        .await
        .expect("open fixture state db read-only");
    let body: Option<String> =
        sqlx::query_scalar("SELECT body_json FROM kb_key_blocks WHERE key_block_id = ?")
            .bind(entry_id)
            .fetch_optional(&pool)
            .await
            .expect("read stored body");
    pool.close().await;
    body.and_then(|raw| {
        serde_json::from_str::<serde_json::Value>(&raw)
            .ok()
            .and_then(|value| {
                value
                    .get("summary")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
            })
    })
}

// ── entity patch ───────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn entity_patch_updates_title_and_bumps_version() {
    let fixture = DirectFixture::new().await;
    let world_id = create_world(&fixture);
    let entity_id = seed_entity(&fixture, &world_id).await;

    let out = kb_cli(
        &fixture,
        &[
            "entity",
            "patch",
            "--world-id",
            &world_id,
            "--entity-id",
            &entity_id,
            "--expected-version",
            "0",
            "--title",
            "Renamed Hero",
        ],
    );
    assert!(out.status.success(), "patch failed: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("new version 1"), "{text}");
    assert!(text.contains("Renamed Hero"), "{text}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn entity_patch_json_emits_dto_verbatim() {
    let fixture = DirectFixture::new().await;
    let world_id = create_world(&fixture);
    let entity_id = seed_entity(&fixture, &world_id).await;

    let out = kb_cli(
        &fixture,
        &[
            "entity",
            "patch",
            "--world-id",
            &world_id,
            "--entity-id",
            &entity_id,
            "--expected-version",
            "0",
            "--title",
            "Json Hero",
            "--json",
        ],
    );
    assert!(out.status.success(), "patch failed: {}", stderr(&out));
    let parsed: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    assert_eq!(parsed["version"], 1);
    assert_eq!(parsed["entity"]["key_block_id"], entity_id);
    assert_eq!(parsed["entity"]["canonical_name"], "Json Hero");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn entity_patch_stale_version_surfaces_conflict() {
    let fixture = DirectFixture::new().await;
    let world_id = create_world(&fixture);
    let entity_id = seed_entity(&fixture, &world_id).await;

    // Bump the entity to revision 1 via a first patch.
    let first = kb_cli(
        &fixture,
        &[
            "entity",
            "patch",
            "--world-id",
            &world_id,
            "--entity-id",
            &entity_id,
            "--expected-version",
            "0",
            "--title",
            "First",
        ],
    );
    assert!(
        first.status.success(),
        "first patch failed: {}",
        stderr(&first)
    );

    // Replay with the stale version 0 → 409 world_kb_conflict.
    let out = kb_cli(
        &fixture,
        &[
            "entity",
            "patch",
            "--world-id",
            &world_id,
            "--entity-id",
            &entity_id,
            "--expected-version",
            "0",
            "--title",
            "Stale",
        ],
    );
    assert!(!out.status.success(), "stale patch should fail");
    let err = stderr(&out);
    // v1.189 P1-T3: graph/patch now ride the core direct-writer, not the
    // daemon HTTP route. The locked direct-core error contract keeps the same
    // structured `world_kb_conflict` surface: the stable code, the row's
    // current version, the caller's expected version (so a stale CAS names
    // BOTH sides), the entity, and the recovery hint. Exit code stays 76.
    assert!(err.contains("world_kb_conflict"), "code missing: {err}");
    assert!(
        err.contains("current_version:"),
        "current_version missing: {err}"
    );
    assert!(
        err.contains("expected_version: 0"),
        "caller's expected version missing: {err}"
    );
    assert!(err.contains(&entity_id), "entity_id missing: {err}");
    assert!(
        err.contains("recovery_hint"),
        "recovery hint missing: {err}"
    );
    assert_eq!(out.status.code(), Some(76), "exit code must stay 76: {err}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn entity_patch_empty_patch_fails_fast() {
    let fixture = DirectFixture::new().await;
    let world_id = create_world(&fixture);
    let entity_id = seed_entity(&fixture, &world_id).await;

    let out = kb_cli(
        &fixture,
        &[
            "entity",
            "patch",
            "--world-id",
            &world_id,
            "--entity-id",
            &entity_id,
            "--expected-version",
            "0",
        ],
    );
    assert!(!out.status.success(), "empty patch should fail");
    let err = stderr(&out);
    assert!(
        err.contains("at least one of"),
        "named fast-fail message missing: {err}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn entity_patch_help_documents_expected_version_retry() {
    let fixture = DirectFixture::new().await;
    let out = kb_cli(&fixture, &["entity", "patch", "--help"]);
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(text.contains("--expected-version"), "{text}");
    assert!(text.contains("world_kb_conflict"), "{text}");
    assert!(text.contains("refetch the graph"), "{text}");
}

// ── kb graph ──────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn kb_graph_lists_entity_with_version() {
    let fixture = DirectFixture::new().await;
    let world_id = create_world(&fixture);
    let entity_id = seed_entity(&fixture, &world_id).await;

    let out = kb_cli(&fixture, &["graph", "--world-id", &world_id]);
    assert!(out.status.success(), "graph failed: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains(&entity_id), "{text}");
    assert!(text.contains("Hero"), "{text}");
    assert!(text.contains("1 entities"), "{text}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn kb_graph_json_emits_dto_verbatim() {
    let fixture = DirectFixture::new().await;
    let world_id = create_world(&fixture);
    let entity_id = seed_entity(&fixture, &world_id).await;

    let out = kb_cli(&fixture, &["graph", "--world-id", &world_id, "--json"]);
    assert!(out.status.success(), "graph failed: {}", stderr(&out));
    let parsed: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("valid JSON");
    let entities = parsed["entities"].as_array().expect("entities array");
    assert_eq!(entities.len(), 1);
    assert_eq!(entities[0]["key_block_id"], entity_id);
    assert_eq!(entities[0]["version"], 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn kb_graph_notes_relationship_cap_when_hit() {
    // qc3 W-002: the graph read caps the relationship projection at 1000 stored
    // relationships with no wire `truncated` flag; when the count hits the cap
    // the human output must state plainly that the graph may be truncated (the
    // note itself is honest about the missing flag).
    let fixture = DirectFixture::new().await;
    let world_id = create_world(&fixture);
    let entity_id = seed_entity(&fixture, &world_id).await;
    seed_relationships(&fixture, &world_id, &entity_id, 1000).await;

    let out = kb_cli(&fixture, &["graph", "--world-id", &world_id]);
    assert!(out.status.success(), "graph failed: {}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains(&entity_id), "{text}");
    assert!(
        text.contains("1000 relationships"),
        "relationship count missing: {text}"
    );
    assert!(
        text.contains("may be truncated"),
        "cap note missing: {text}"
    );
    assert!(text.contains("no wire `truncated` flag"), "{text}");
}

// ── pack export + dry run: the CLI surface itself ─────────────────────────

/// `creator world kb pack export` and `pack import --dry-run` run end to end
/// against the real binary on the direct-core seam: the export writes the
/// admitted pack and the dry run reports its plan without touching a row.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pack_export_and_dry_run_without_server() {
    let fixture = DirectFixture::new().await;
    let world_id = create_world(&fixture);
    let entity_id = seed_entity(&fixture, &world_id).await;

    let pack_dir = tempfile::tempdir().expect("pack dir");
    let out_path = pack_dir.path().join("world_pack.json");
    let out = kb_cli(
        &fixture,
        &[
            "pack",
            "export",
            &world_id,
            "--out",
            out_path.to_str().unwrap(),
        ],
    );
    assert!(out.status.success(), "export failed: {}", stderr(&out));
    let text = stdout(&out);
    assert!(
        text.contains("Knowledge pack exported:"),
        "export summary missing: {text}"
    );
    assert!(text.contains("Entries:   1"), "entry count missing: {text}");

    let pack: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&out_path).expect("read pack"))
            .expect("pack is JSON");
    let entries = pack["entries"].as_array().expect("entries array");
    assert_eq!(entries.len(), 1, "the seeded entity must be exported");
    assert_eq!(entries[0]["entry_id"], entity_id);
    // Private facts of the only admitted holder are not exported without the
    // explicit `--include-owned-private` intent.
    assert!(
        text.contains("shared rows only"),
        "scope line missing: {text}"
    );

    // Dry run with the overwrite policy: the plan says the row would be
    // replaced, and nothing is written — the body still reads as stored.
    let dry = kb_cli(
        &fixture,
        &[
            "pack",
            "import",
            &world_id,
            "--in",
            out_path.to_str().unwrap(),
            "--dry-run",
            "--conflict",
            "overwrite",
        ],
    );
    assert!(dry.status.success(), "dry run failed: {}", stderr(&dry));
    let dry_text = stdout(&dry);
    assert!(
        dry_text.contains(
            "[dry-run] would create: 0, would skip: 0, would rename: 0, would overwrite: 1"
        ),
        "dry-run plan missing: {dry_text}"
    );
    assert_eq!(
        stored_summary(&fixture, &entity_id).await.as_deref(),
        Some("Original summary"),
        "the dry run must not rewrite the stored row"
    );
}

// ── pack import: holder governance without a server ───────────────────────

/// A pack atom governed by an unmapped foreign holder must stay quarantined —
/// **not** published as the importing Creator's own private fact — and the
/// batch must be reviewable and then adoptable through the CLI alone.
///
/// This is the accident guard for the v1.193 P0-T4 wrapper removal: the retired
/// adapter handed a raw CLI pool to the core, so the governance decision had to
/// survive the move onto the typed `CoreService::import_world_pack` seam with no
/// daemon (and no pool) in between. An implementation that quietly remapped the
/// foreign holder onto the local creator holder — the "publication by
/// accident" failure — would store the row and pass every count assertion.
#[allow(clippy::too_many_lines)] // one governance journey asserted end to end
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn pack_import_quarantines_unmapped_holder_without_server() {
    let fixture = DirectFixture::new().await;
    let world_id = create_world(&fixture);

    let pack_dir = tempfile::tempdir().expect("pack dir");
    let pack_path = pack_dir.path().join("governed_pack.json");
    let pack = serde_json::json!({
        "modules": { "pack": { "title": "Governed", "version": "0.1.0", "creator": "packAuthor" } },
        "entries": [{
            "schema_version": 1,
            "entry_id": "kb_governed_foreign",
            "entry_type": "character",
            "canonical_name": "Foreign Row",
            "status": "confirmed",
            "body": { "summary": "Foreign Row summary" },
            "owner": FOREIGN_HOLDER,
            "disclosure": DISCLOSURE_OWNER_PRIVATE,
            "extensions": { "nexus": { "world_id": world_id } }
        }],
        "relations": []
    });
    std::fs::write(
        &pack_path,
        serde_json::to_string_pretty(&pack).expect("pack json"),
    )
    .expect("write pack");

    // ── Unmapped import: quarantined, never stored ─────────────────────
    let out = kb_cli(
        &fixture,
        &[
            "pack",
            "import",
            &world_id,
            "--in",
            pack_path.to_str().unwrap(),
        ],
    );
    let text = stdout(&out);
    let err = stderr(&out);
    assert!(
        text.contains("quarantined 1 atom(s) in batch "),
        "quarantine report missing: {text}"
    );
    assert!(
        text.contains("kb_governed_foreign"),
        "quarantined atom not named: {text}"
    );
    assert!(
        text.contains("unresolved_holder"),
        "quarantine reason missing: {text}"
    );
    assert!(
        text.contains(FOREIGN_HOLDER),
        "original foreign holder must be reported verbatim: {text}"
    );
    assert!(
        err.contains("rejected"),
        "a quarantined atom is not a plain success: {err}"
    );
    // The atom stayed OUT of the KB stores: nothing was published under the
    // importing Creator's own holder (and nothing under any other).
    assert_eq!(
        stored_governance(&fixture, "kb_governed_foreign").await,
        None,
        "an unmapped foreign atom must never be stored"
    );

    // The reported batch id is the one the review arm accepts.
    let batch_id = reported_batch_id(&text);
    assert!(batch_id.starts_with("pib_"), "{batch_id}");

    // ── Review arm: read-only, original governance verbatim ────────────
    let review = kb_cli(
        &fixture,
        &["pack", "import", &world_id, "--review-import", &batch_id],
    );
    assert!(
        review.status.success(),
        "review failed: {}",
        stderr(&review)
    );
    let review_text = stdout(&review);
    assert!(
        review_text.contains("quarantined atoms in batch"),
        "{review_text}"
    );
    assert!(review_text.contains("kb_governed_foreign"), "{review_text}");
    assert!(review_text.contains(FOREIGN_HOLDER), "{review_text}");
    assert!(
        review_text.contains(DISCLOSURE_OWNER_PRIVATE),
        "{review_text}"
    );
    assert!(
        review_text.contains("Foreign Row summary"),
        "the immutable original atom JSON must be printed verbatim: {review_text}"
    );
    assert_eq!(
        stored_governance(&fixture, "kb_governed_foreign").await,
        None,
        "the review arm never writes"
    );

    // ── Explicit adoption: the operator's own mapping lands the row ─────
    let adopted = kb_cli(
        &fixture,
        &[
            "pack",
            "import",
            &world_id,
            "--in",
            pack_path.to_str().unwrap(),
            "--holder-map",
            &format!("{FOREIGN_HOLDER}=author-only"),
        ],
    );
    assert!(
        adopted.status.success(),
        "adoption import failed: {}",
        stderr(&adopted)
    );
    let creator_id = fixture_creator_id(&fixture);
    let local_holder = nexus_local_db::holders::creator_holder_entry_id(&creator_id);
    assert_eq!(
        stored_governance(&fixture, "kb_governed_foreign").await,
        Some((
            Some(local_holder),
            Some(DISCLOSURE_OWNER_PRIVATE.to_string())
        )),
        "the mapped atom is adopted onto the local creator holder, keeping its disclosure"
    );
}
