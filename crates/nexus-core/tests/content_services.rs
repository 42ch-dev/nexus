//! Chapter/manuscript content and outline/chronology services on guarded
//! storage. The named selector `get_body_rejects_escaped_body_path` carries
//! AC-P1-T2: traversal/symlink paths outside the Work are denied before any
//! read/write, and a denied published-chapter mutation leaves content intact.
#![allow(clippy::too_many_lines)] // one end-to-end scenario per test

use std::num::NonZeroU64;
use std::path::PathBuf;

use nexus_contracts::{
    ChapterContentQuery, CreateWorkRequest, CreateWorldRequest, ListChaptersQuery,
    OutlinePatchChapterRequest, OutlinePatchStructureRequest, PatchChapterRequest,
    TimelinePatchEventRequest, WorkOutlineBeatsItemStatus, WorkOutlineScenesItemStatus,
};
use nexus_core::{CoreAccess, CoreChapterContentQuery, CoreError, CoreOpenOptions, CoreService};
use nexus_local_db::writer_protocol::init_guarded_pool;

fn select_creator(home: &std::path::Path, creator: &str, workspace: &str) {
    std::fs::write(
        home.join(".nexus42/config.toml"),
        format!("active_creator_id = \"{creator}\"\n[active_workspace_slug_by_creator]\n\"{creator}\" = \"{workspace}\"\n"),
    )
    .unwrap();
}

fn content_query() -> CoreChapterContentQuery {
    CoreChapterContentQuery::from(ChapterContentQuery { volume: None })
}

fn patch_request(value: serde_json::Value) -> PatchChapterRequest {
    serde_json::from_value(value).unwrap()
}

fn chapter_patch_request(value: serde_json::Value) -> OutlinePatchChapterRequest {
    serde_json::from_value(value).unwrap()
}

fn structure_request(value: serde_json::Value) -> OutlinePatchStructureRequest {
    serde_json::from_value(value).unwrap()
}

fn timeline_request(value: serde_json::Value) -> TimelinePatchEventRequest {
    serde_json::from_value(value).unwrap()
}

fn chapters_query(value: serde_json::Value) -> ListChaptersQuery {
    serde_json::from_value(value).unwrap()
}

struct Fixture {
    _temp: tempfile::TempDir,
    core: CoreService,
    principal: nexus_core::Principal,
    pool: sqlx::SqlitePool,
    work_id: String,
    creative_root: PathBuf,
}

impl Fixture {
    fn body_rel(chapter: u32) -> String {
        format!("Works/test-novel/Stories/ch{chapter:02}-ch{chapter:02}.md")
    }

    fn write_body(&self, chapter: u32, content: &str) {
        let path = self.creative_root.join(Self::body_rel(chapter));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, content).unwrap();
    }
}

async fn setup() -> Fixture {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path();
    std::fs::create_dir_all(home.join(".nexus42")).unwrap();
    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
        home, "author", "default",
    ))
    .unwrap();
    select_creator(home, "author", "default");
    let db = nexus_home_layout::workspace_state_db_path(home, "author", "default");
    {
        let seed = init_guarded_pool(&db, "author").await.unwrap();
        let pool = seed.clone_pool();
        nexus_local_db::creators::ensure_creator_row(&pool, "author", "Author")
            .await
            .unwrap();
        pool.close().await;
    }
    let core = CoreService::open(CoreOpenOptions {
        user_home: home.into(),
        access: CoreAccess::DirectWriter,
    })
    .await
    .unwrap();
    let principal = core.active_principal().await.unwrap();
    let world = core
        .create_world(
            &principal,
            serde_json::from_value::<CreateWorldRequest>(serde_json::json!({"title": "World"}))
                .unwrap(),
        )
        .await
        .unwrap()
        .world_id;
    let work_id = core
        .create_work(
            &principal,
            serde_json::from_value::<CreateWorkRequest>(serde_json::json!({
                "title": "Test Novel", "long_term_goal": "write", "initial_idea": "idea",
                "world_id": world
            }))
            .unwrap(),
        )
        .await
        .unwrap()
        .work_id;

    // Workspace creative root + deterministic Work directory reference.
    let creative_root = home.join("creative");
    std::fs::create_dir_all(&creative_root).unwrap();
    std::fs::write(
        nexus_home_layout::operational_workspace_dir(home, "author", "default").join("meta.json"),
        serde_json::to_vec(&serde_json::json!({"local_root": creative_root})).unwrap(),
    )
    .unwrap();

    let guarded = init_guarded_pool(&db, "author").await.unwrap();
    let pool = guarded.clone_pool();
    sqlx::query("UPDATE works SET work_ref = 'test-novel' WHERE work_id = ?")
        .bind(&work_id)
        .execute(&pool)
        .await
        .unwrap();
    let now = chrono::Utc::now().to_rfc3339();
    nexus_local_db::work_chapters::seed_chapters(&pool, &work_id, "test-novel", 3, &now)
        .await
        .unwrap();

    Fixture {
        _temp: temp,
        core,
        principal,
        pool,
        work_id,
        creative_root,
    }
}

/// AC-P1-T2 selector: the chapter body path guard denies `..` traversal into a
/// prefix-confusion sibling and a symlink escaping the workspace root before
/// the file is read; inside-root reads keep working.
#[tokio::test]
async fn get_body_rejects_escaped_body_path() {
    let fx = setup().await;
    fx.write_body(1, "body content");
    let evil_target = fx
        .creative_root
        .parent()
        .unwrap()
        .join("creative-evil/evil.md");
    std::fs::create_dir_all(evil_target.parent().unwrap()).unwrap();
    std::fs::write(&evil_target, "stolen").unwrap();

    // Sanity: the inside-root body reads normally.
    let body = fx
        .core
        .chapter_body(
            &fx.principal,
            fx.work_id.clone(),
            "1".into(),
            content_query(),
        )
        .await
        .unwrap();
    assert_eq!(body.content, "body content");
    assert!(body.read_only);

    // Traversal into a sibling whose name extends the root name must be
    // rejected on the read path before the file is opened.
    sqlx::query("UPDATE work_chapters SET body_path = '../creative-evil/evil.md' WHERE work_id = ? AND chapter = 1")
        .bind(&fx.work_id)
        .execute(&fx.pool)
        .await
        .unwrap();
    let Err(CoreError::InvalidInput { field, .. }) = fx
        .core
        .chapter_body(
            &fx.principal,
            fx.work_id.clone(),
            "1".into(),
            content_query(),
        )
        .await
    else {
        panic!("sibling traversal must be denied");
    };
    assert_eq!(field, "chapter_body_path_forbidden");

    // A symlink placed inside the Work pointing outside must be denied before
    // the target is read (canonicalize resolves it beyond the root).
    let link_dir = fx.creative_root.join("Works/test-novel/Stories");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&evil_target, link_dir.join("link.md")).unwrap();
    sqlx::query("UPDATE work_chapters SET body_path = 'Works/test-novel/Stories/link.md' WHERE work_id = ? AND chapter = 1")
        .bind(&fx.work_id)
        .execute(&fx.pool)
        .await
        .unwrap();
    let Err(CoreError::InvalidInput { field, .. }) = fx
        .core
        .chapter_body(
            &fx.principal,
            fx.work_id.clone(),
            "1".into(),
            content_query(),
        )
        .await
    else {
        panic!("symlink escape must be denied before read");
    };
    assert_eq!(field, "chapter_body_path_forbidden");
    assert_eq!(std::fs::read_to_string(&evil_target).unwrap(), "stolen");

    // The same guard denies detail-editability probes for escaped paths.
    sqlx::query("UPDATE work_chapters SET body_path = ?, outline_path = '../creative-evil/outline.md' WHERE work_id = ? AND chapter = 1")
        .bind(Fixture::body_rel(1))
        .bind(&fx.work_id)
        .execute(&fx.pool)
        .await
        .unwrap();
    let detail = fx
        .core
        .chapter_detail(
            &fx.principal,
            fx.work_id.clone(),
            "1".into(),
            content_query(),
        )
        .await
        .unwrap();
    assert!(!detail.can_edit_outline);
    fx.pool.close().await;
    fx.core.close().await.unwrap();
}

/// A published chapter rejects structural mutation and its stored content
/// survives the denial byte-for-byte; outline canvas edits are blocked too,
/// and escaped write paths are denied before any file is created.
#[tokio::test]
async fn published_chapter_mutation_blocked_and_content_survives() {
    let fx = setup().await;
    fx.write_body(1, "published prose");
    sqlx::query("UPDATE work_chapters SET status = 'published' WHERE work_id = ? AND chapter = 1")
        .bind(&fx.work_id)
        .execute(&fx.pool)
        .await
        .unwrap();

    let err = fx
        .core
        .patch_chapter(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            "1".into(),
            content_query(),
            patch_request(serde_json::json!({"slug": "new-slug"})),
        )
        .await
        .expect_err("published structural edit must be blocked");
    let CoreError::InvalidInput { field, .. } = err else {
        panic!("expected legacy BadRequest carrier, got {err:?}");
    };
    assert_eq!(field, "chapter_structure_edit_blocked");

    let body = fx
        .core
        .chapter_body(
            &fx.principal,
            fx.work_id.clone(),
            "1".into(),
            content_query(),
        )
        .await
        .unwrap();
    assert_eq!(
        body.content, "published prose",
        "content must survive the denial"
    );

    // Canvas outline node edit on the published chapter is blocked as well.
    let err = fx
        .core
        .patch_outline_chapter(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            "1".into(),
            chapter_patch_request(serde_json::json!({
                "work_id": fx.work_id, "base_revision": 0, "chapter_id": 1,
                "set": {"title": "Renamed"}
            })),
        )
        .await
        .expect_err("published outline edit must be blocked");
    let CoreError::InvalidInput { field, .. } = err else {
        panic!("expected legacy BadRequest carrier, got {err:?}");
    };
    assert_eq!(field, "chapter_structure_edit_blocked");

    // An outline write path that escapes the root is denied before the file
    // is created; the outside sibling stays untouched.
    let now = chrono::Utc::now().to_rfc3339();
    nexus_local_db::work_chapters::update_outline_path(
        &fx.pool,
        &fx.work_id,
        2,
        1,
        Some("../creative-evil/planted.md"),
        &now,
    )
    .await
    .unwrap();
    let err = fx
        .core
        .patch_outline_chapter(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            "2".into(),
            chapter_patch_request(serde_json::json!({
                "work_id": fx.work_id, "base_revision": 0, "chapter_id": 2,
                "set": {"content": "planted"}
            })),
        )
        .await
        .expect_err("escaped write path must be denied");
    let CoreError::InvalidInput { field, .. } = err else {
        panic!("expected legacy BadRequest carrier, got {err:?}");
    };
    assert_eq!(field, "chapter_path_forbidden");
    assert!(!fx
        .creative_root
        .parent()
        .unwrap()
        .join("creative-evil/planted.md")
        .exists());
    fx.pool.close().await;
    fx.core.close().await.unwrap();
}

/// Chapter metadata patch round-trip: display-only title rejection, slug
/// update, volume move (re-fetch parity — a moved row is returned, not 404)
/// and the V1.65 status transition grammar.
#[tokio::test]
async fn chapter_patch_updates_slug_volume_and_rejects_title() {
    let fx = setup().await;

    // The display-only title rejection must fire while the row is still
    // reachable at the queried (default) volume.
    let err = fx
        .core
        .patch_chapter(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            "1".into(),
            content_query(),
            patch_request(serde_json::json!({"title": "New Title"})),
        )
        .await
        .err()
        .unwrap();
    let CoreError::InvalidInput { field, .. } = err else {
        panic!("title rejection carrier drifted: {err:?}");
    };
    assert_eq!(field, "chapter_title_unsupported");

    let detail = fx
        .core
        .patch_chapter(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            "1".into(),
            content_query(),
            patch_request(serde_json::json!({"slug": "new-slug"})),
        )
        .await
        .unwrap();
    assert_eq!(detail.slug.as_deref(), Some("new-slug"));

    let detail = fx
        .core
        .patch_chapter(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            "1".into(),
            content_query(),
            patch_request(serde_json::json!({"volume": 2})),
        )
        .await
        .unwrap();
    assert_eq!(detail.volume, NonZeroU64::new(2).unwrap());

    let detail = fx
        .core
        .patch_chapter(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            "2".into(),
            content_query(),
            patch_request(serde_json::json!({"status": "outlined"})),
        )
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(detail.status).unwrap(),
        serde_json::json!("outlined")
    );

    let err = fx
        .core
        .patch_chapter(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            "2".into(),
            content_query(),
            patch_request(serde_json::json!({"status": "published"})),
        )
        .await
        .err()
        .unwrap();
    let CoreError::InvalidInput { field, .. } = err else {
        panic!("status grammar carrier drifted: {err:?}");
    };
    assert_eq!(field, "chapter_status_transition_invalid");
    fx.pool.close().await;
    fx.core.close().await.unwrap();
}

/// The `v2:` keyset chapter-list cursor grammar survives the service
/// extraction: bounded limit, cursor pages, and verbatim cursor rejection.
#[tokio::test]
async fn chapter_list_keyset_pagination() {
    let fx = setup().await;

    let all = fx
        .core
        .list_chapters(
            &fx.principal,
            fx.work_id.clone(),
            chapters_query(serde_json::json!({})),
        )
        .await
        .unwrap();
    assert_eq!(all.items.len(), 3);
    assert_eq!(all.pagination.limit, 50);

    let first = fx
        .core
        .list_chapters(
            &fx.principal,
            fx.work_id.clone(),
            chapters_query(serde_json::json!({"limit": 2})),
        )
        .await
        .unwrap();
    assert_eq!(first.items.len(), 2);
    assert!(first.pagination.has_more);
    let cursor = first.pagination.next_cursor.clone().unwrap();
    assert!(
        cursor.starts_with("v2:"),
        "keyset cursor grammar changed: {cursor}"
    );

    let second = fx
        .core
        .list_chapters(
            &fx.principal,
            fx.work_id.clone(),
            chapters_query(serde_json::json!({"limit": 2, "cursor": cursor})),
        )
        .await
        .unwrap();
    assert_eq!(second.items.len(), 1);
    assert!(!second.pagination.has_more);
    assert!(second.pagination.next_cursor.is_none());

    let Err(CoreError::InvalidInput { field, reason }) = fx
        .core
        .list_chapters(
            &fx.principal,
            fx.work_id.clone(),
            chapters_query(serde_json::json!({"cursor": "garbage"})),
        )
        .await
    else {
        panic!("invalid cursor must be rejected");
    };
    assert_eq!(field, "invalid_input");
    assert_eq!(
        reason,
        "invalid chapter_cursor; pass the next_cursor value unchanged"
    );
    fx.pool.close().await;
    fx.core.close().await.unwrap();
}

/// Outline revision CAS: a stale `base_revision` is a structured conflict and
/// a patch persists the body observed at the locked re-read, not a stale
/// pre-read snapshot.
#[tokio::test]
async fn outline_patch_conflict_and_locked_reread() {
    let fx = setup().await;
    let rel_path = "Works/test-novel/Outlines/outline.md";
    let outline_path = fx.creative_root.join(rel_path);
    std::fs::create_dir_all(outline_path.parent().unwrap()).unwrap();
    let frontmatter =
        "---\noutline_revision: 0\nvolumes: []\ntimeline_events: []\nforeshadows: []\nchapter_titles: {}\nupdated_at: \"2024-01-01T00:00:00Z\"\n---\n";
    std::fs::write(&outline_path, format!("{frontmatter}stale body\n")).unwrap();

    // Stale revision: rejected as a typed conflict carrying recovery fields.
    let err = fx
        .core
        .patch_outline_structure(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            structure_request(serde_json::json!({
                "work_id": fx.work_id, "base_revision": 5,
                "operation": "move_chapter", "chapter_id": 1, "volume_id": 2
            })),
        )
        .await
        .expect_err("stale base_revision must conflict");
    let CoreError::OutlineConflict(details) = err else {
        panic!("expected typed outline conflict, got {err:?}");
    };
    assert_eq!(details.current_revision, 0);
    assert_eq!(details.conflicting_path, "outline_revision");
    assert_eq!(
        details.recovery_hint,
        "refetch the work outline and reapply"
    );

    // A concurrent writer rewrites the body after our last read; the patch
    // must persist the fresh body observed at the locked re-read.
    std::fs::write(&outline_path, format!("{frontmatter}fresh body\n")).unwrap();
    fx.core
        .patch_outline_chapter(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            "1".into(),
            chapter_patch_request(serde_json::json!({
                "work_id": fx.work_id, "base_revision": 0, "chapter_id": 1,
                "set": {"title": "Renamed"}
            })),
        )
        .await
        .unwrap();

    let final_content = std::fs::read_to_string(&outline_path).unwrap();
    assert!(
        final_content.contains("fresh body\n"),
        "locked re-read body must win: {final_content}"
    );
    assert!(
        !final_content.contains("stale body"),
        "stale snapshot must not be persisted"
    );
    let outline = fx
        .core
        .work_outline(&fx.principal, fx.work_id.clone())
        .await
        .unwrap();
    assert_eq!(outline.outline_revision, 1);
    assert_eq!(
        outline.chapter_titles.get("1").map(String::as_str),
        Some("Renamed")
    );
    fx.pool.close().await;
    fx.core.close().await.unwrap();
}

/// F-06 regression: a Work whose `work_ref` and `story_ref` are both NULL —
/// the state right after `POST /works` — reads its outline as the in-memory
/// missing-file default (revision 0, one empty volume) instead of failing with
/// `WORK_REF_MISSING` (HTTP 500). The read fabricates no ref and no filesystem
/// path, and persists nothing.
#[tokio::test]
async fn ref_less_work_outline_read_degrades_to_default() {
    let fx = setup().await;
    let world_id: String = sqlx::query_scalar("SELECT world_id FROM works WHERE work_id = ?")
        .bind(&fx.work_id)
        .fetch_one(&fx.pool)
        .await
        .unwrap();
    let ref_less = fx
        .core
        .create_work(
            &fx.principal,
            serde_json::from_value::<CreateWorkRequest>(serde_json::json!({
                "title": "Fresh Novel", "long_term_goal": "write", "initial_idea": "idea",
                "world_id": world_id
            }))
            .unwrap(),
        )
        .await
        .unwrap()
        .work_id;

    // Precondition: the fresh Work really is ref-less (the F-06 repro state).
    let (work_ref, story_ref): (Option<String>, Option<String>) =
        sqlx::query_as("SELECT work_ref, story_ref FROM works WHERE work_id = ?")
            .bind(&ref_less)
            .fetch_one(&fx.pool)
            .await
            .unwrap();
    assert_eq!((work_ref, story_ref), (None, None));

    let outline = fx
        .core
        .work_outline(&fx.principal, ref_less.clone())
        .await
        .expect("a ref-less Work's outline read must degrade, not fail");
    assert_eq!(outline.work_id, ref_less);
    assert_eq!(outline.outline_revision, 0);
    assert_eq!(outline.volumes.len(), 1);
    assert_eq!(
        outline.volumes[0].chapter_ids,
        [] as [std::num::NonZero<u64>; 0]
    );
    assert!(outline.timeline_events.is_empty());
    assert!(outline.chapter_titles.is_empty());
    assert_ne!(outline.updated_at, "");

    fx.pool.close().await;
    fx.core.close().await.unwrap();
}

/// T1 direct proof (D7-A): a Work whose `work_ref` and `story_ref` are both
/// NULL is refused at the shared `resolve_work_ref` seam with the locked typed
/// shape — `CoreError::InvalidInput { field: "work_ref_missing" }` (wire 400
/// `invalid_input` + `details.field`) and a recovery hint naming `story_ref` —
/// instead of the former opaque `WORK_REF_MISSING` 500. Task 2 owns the full
/// three-route / ref'd-regression / read-write-chain matrix.
#[tokio::test]
async fn ref_less_work_outline_write_refuses_typed() {
    let fx = setup().await;
    let world_id: String = sqlx::query_scalar("SELECT world_id FROM works WHERE work_id = ?")
        .bind(&fx.work_id)
        .fetch_one(&fx.pool)
        .await
        .unwrap();
    let ref_less = fx
        .core
        .create_work(
            &fx.principal,
            serde_json::from_value::<CreateWorkRequest>(serde_json::json!({
                "title": "Fresh Novel", "long_term_goal": "write", "initial_idea": "idea",
                "world_id": world_id
            }))
            .unwrap(),
        )
        .await
        .unwrap()
        .work_id;

    let err = fx
        .core
        .patch_outline_structure(
            &fx.principal,
            "http",
            ref_less.clone(),
            structure_request(serde_json::json!({
                "work_id": ref_less, "base_revision": 0,
                "operation": "move_chapter", "chapter_id": 1, "volume_id": 2
            })),
        )
        .await
        .expect_err("a ref-less Work's outline write must refuse, never 500");
    let CoreError::InvalidInput { field, reason } = err else {
        panic!("expected the typed invalid-input refusal, got {err:?}");
    };
    assert_eq!(field, "work_ref_missing");
    assert!(
        reason.contains("story_ref"),
        "the refusal must name the recovery step: {reason}"
    );

    fx.pool.close().await;
    fx.core.close().await.unwrap();
}

/// Timeline chronology patches: add/link/unlink round-trip with revision
/// increments, self-foreshadow rejection and missing-edge 404 semantics.
#[tokio::test]
async fn timeline_patch_round_trip_and_guards() {
    let fx = setup().await;
    let patch = |base: u64, mut value: serde_json::Value| {
        value["work_id"] = serde_json::json!(fx.work_id.clone());
        value["base_revision"] = serde_json::json!(base);
        timeline_request(value)
    };

    fx.core
        .patch_timeline_event(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            patch(
                0,
                serde_json::json!({
                    "operation": "add_event", "title": "Plant", "realizes_chapter_id": 1
                }),
            ),
        )
        .await
        .unwrap();
    fx.core
        .patch_timeline_event(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            patch(
                1,
                serde_json::json!({
                    "operation": "add_event", "title": "Payoff", "realizes_chapter_id": 2
                }),
            ),
        )
        .await
        .unwrap();

    let outline = fx
        .core
        .work_outline(&fx.principal, fx.work_id.clone())
        .await
        .unwrap();
    assert_eq!(outline.timeline_events.len(), 2);
    assert_eq!(outline.outline_revision, 2);
    let (evt_a, evt_b) = (
        outline.timeline_events[0].event_id.clone(),
        outline.timeline_events[1].event_id.clone(),
    );

    fx.core
        .patch_timeline_event(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            patch(
                2,
                serde_json::json!({
                    "operation": "link_foreshadow",
                    "event_id": evt_a, "foreshadows_event_id": evt_b
                }),
            ),
        )
        .await
        .unwrap();
    let outline = fx
        .core
        .work_outline(&fx.principal, fx.work_id.clone())
        .await
        .unwrap();
    assert_eq!(outline.foreshadows.len(), 1);

    // Self-referential foreshadow is nonsense and must be rejected.
    let err = fx
        .core
        .patch_timeline_event(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            patch(
                3,
                serde_json::json!({
                    "operation": "link_foreshadow",
                    "event_id": evt_a, "foreshadows_event_id": evt_a
                }),
            ),
        )
        .await
        .err()
        .unwrap();
    let CoreError::InvalidInput { field, .. } = err else {
        panic!("self-foreshadow carrier drifted: {err:?}");
    };
    assert_eq!(field, "self_foreshadow_forbidden");

    // Reverse-direction unlink finds no edge and must 404, not no-op.
    let err = fx
        .core
        .patch_timeline_event(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            patch(
                3,
                serde_json::json!({
                    "operation": "unlink_foreshadow",
                    "event_id": evt_b, "foreshadows_event_id": evt_a
                }),
            ),
        )
        .await
        .err()
        .unwrap();
    let CoreError::NotFound { resource } = err else {
        panic!("missing-edge unlink must be NotFound, got {err:?}");
    };
    assert_eq!(resource, format!("foreshadow link {evt_b} → {evt_a}"));

    // Real unlink removes the edge.
    fx.core
        .patch_timeline_event(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            patch(
                3,
                serde_json::json!({
                    "operation": "unlink_foreshadow",
                    "event_id": evt_a, "foreshadows_event_id": evt_b
                }),
            ),
        )
        .await
        .unwrap();
    let outline = fx
        .core
        .work_outline(&fx.principal, fx.work_id.clone())
        .await
        .unwrap();
    assert!(outline.foreshadows.is_empty());
    fx.pool.close().await;
    fx.core.close().await.unwrap();
}

/// The Work's stored bound World (the only World authority a binding may use).
async fn work_world_id(fx: &Fixture) -> String {
    fx.core
        .get_work(&fx.principal, fx.work_id.clone())
        .await
        .expect("work read")
        .world_id
        .expect("the fixture Work is created with a bound World")
}

/// Create a World KB entity of `block_type` in `world_id` through the service.
async fn create_kb_entity(
    fx: &Fixture,
    world_id: &str,
    entity_id: &str,
    title: &str,
    block_type: &str,
) {
    fx.core
        .patch_world_kb_entity(
            &fx.principal,
            world_id.to_string(),
            serde_json::from_value::<nexus_contracts::WorldKbPatchEntityRequest>(
                serde_json::json!({
                    "entity_id": entity_id,
                    "expected_version": 0,
                    "patch": {"title": title, "block_type": block_type}
                }),
            )
            .unwrap(),
        )
        .await
        .expect("seed World KB entity");
}

/// World-event binding round trip: `bind_world_event` stores the referent that
/// the bound World's graph proves is an event, and `unbind_world_event` clears
/// it in one revision — including the explicitly accepted case of an event that
/// has no stored binding (unlike `unlink_foreshadow`'s absent-edge `NotFound`).
#[tokio::test]
async fn timeline_world_event_binding_round_trip_and_unbind() {
    let fx = setup().await;
    let world_id = work_world_id(&fx).await;
    let referent = "kb_0000e001";
    create_kb_entity(&fx, &world_id, referent, "Bound Event", "event").await;

    let patch = |base: u64, mut value: serde_json::Value| {
        value["work_id"] = serde_json::json!(fx.work_id.clone());
        value["base_revision"] = serde_json::json!(base);
        timeline_request(value)
    };

    fx.core
        .patch_timeline_event(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            patch(
                0,
                serde_json::json!({
                    "operation": "add_event", "title": "Realized", "realizes_chapter_id": 1
                }),
            ),
        )
        .await
        .unwrap();
    let event_id = event_id_by_title(&fx, "Realized").await;

    let bound = fx
        .core
        .patch_timeline_event(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            patch(
                1,
                serde_json::json!({
                    "operation": "bind_world_event",
                    "event_id": event_id, "world_event_id": referent
                }),
            ),
        )
        .await
        .unwrap();
    assert_eq!(bound.new_revision, NonZeroU64::new(2).unwrap());
    let outline = fx
        .core
        .work_outline(&fx.principal, fx.work_id.clone())
        .await
        .unwrap();
    assert_eq!(outline.outline_revision, 2);
    assert_eq!(outline.timeline_events.len(), 1);
    assert_eq!(
        outline.timeline_events[0].world_event_id.as_deref(),
        Some(referent),
        "bind stores the validated World KB referent"
    );

    // Unbind clears the stored referent with exactly one revision bump.
    let cleared = fx
        .core
        .patch_timeline_event(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            patch(
                2,
                serde_json::json!({
                    "operation": "unbind_world_event", "event_id": event_id
                }),
            ),
        )
        .await
        .unwrap();
    assert_eq!(cleared.new_revision, NonZeroU64::new(3).unwrap());
    let outline = fx
        .core
        .work_outline(&fx.principal, fx.work_id.clone())
        .await
        .unwrap();
    assert_eq!(outline.timeline_events[0].world_event_id, None);

    // An already-unbound valid event is accepted, with exactly one more bump.
    let repeated = fx
        .core
        .patch_timeline_event(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            patch(
                3,
                serde_json::json!({
                    "operation": "unbind_world_event", "event_id": event_id
                }),
            ),
        )
        .await
        .unwrap();
    assert_eq!(repeated.new_revision, NonZeroU64::new(4).unwrap());
    assert_eq!(outline_revision(&fx).await, 4);
    fx.pool.close().await;
    fx.core.close().await.unwrap();
}

/// A referent deleted from the World KB after the bind stays clearable: an
/// unbind never re-reads the previous referent.
#[tokio::test]
async fn timeline_world_event_unbind_ignores_a_deleted_referent() {
    let fx = setup().await;
    let world_id = work_world_id(&fx).await;
    let referent = "kb_0000e002";
    create_kb_entity(&fx, &world_id, referent, "Doomed Event", "event").await;

    let patch = |base: u64, mut value: serde_json::Value| {
        value["work_id"] = serde_json::json!(fx.work_id.clone());
        value["base_revision"] = serde_json::json!(base);
        timeline_request(value)
    };
    fx.core
        .patch_timeline_event(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            patch(
                0,
                serde_json::json!({
                    "operation": "add_event", "title": "Doomed", "realizes_chapter_id": 1
                }),
            ),
        )
        .await
        .unwrap();
    let event_id = event_id_by_title(&fx, "Doomed").await;
    fx.core
        .patch_timeline_event(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            patch(
                1,
                serde_json::json!({
                    "operation": "bind_world_event",
                    "event_id": event_id, "world_event_id": referent
                }),
            ),
        )
        .await
        .unwrap();

    sqlx::query("DELETE FROM kb_key_blocks WHERE key_block_id = ?")
        .bind(referent)
        .execute(&fx.pool)
        .await
        .unwrap();

    let cleared = fx
        .core
        .patch_timeline_event(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            patch(
                2,
                serde_json::json!({
                    "operation": "unbind_world_event", "event_id": event_id
                }),
            ),
        )
        .await
        .unwrap();
    assert_eq!(cleared.new_revision, NonZeroU64::new(3).unwrap());
    let outline = fx
        .core
        .work_outline(&fx.principal, fx.work_id.clone())
        .await
        .unwrap();
    assert_eq!(outline.timeline_events[0].world_event_id, None);
    fx.pool.close().await;
    fx.core.close().await.unwrap();
}

/// Every binding refusal is typed and leaves the outline content and revision
/// untouched: unknown event, missing / foreign-World / non-event referent,
/// missing members, and a Work with no bound World.
#[tokio::test]
async fn timeline_world_event_binding_refusals_leave_outline_unchanged() {
    let fx = setup().await;
    let rel_path = "Works/test-novel/Outlines/outline.md";
    let outline_path = fx.creative_root.join(rel_path);
    let world_id = work_world_id(&fx).await;
    let referent = "kb_0000e003";
    create_kb_entity(&fx, &world_id, referent, "Owned Event", "event").await;
    create_kb_entity(
        &fx,
        &world_id,
        "kb_0000c003",
        "Owned Character",
        "character",
    )
    .await;
    let foreign_world = fx
        .core
        .create_world(
            &fx.principal,
            serde_json::from_value::<CreateWorldRequest>(serde_json::json!({"title": "Foreign"}))
                .unwrap(),
        )
        .await
        .unwrap()
        .world_id;
    let foreign_referent = "kb_0000f003";
    create_kb_entity(
        &fx,
        &foreign_world,
        foreign_referent,
        "Foreign Event",
        "event",
    )
    .await;

    let patch = |base: u64, mut value: serde_json::Value| {
        value["work_id"] = serde_json::json!(fx.work_id.clone());
        value["base_revision"] = serde_json::json!(base);
        timeline_request(value)
    };
    fx.core
        .patch_timeline_event(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            patch(
                0,
                serde_json::json!({
                    "operation": "add_event", "title": "Anchor", "realizes_chapter_id": 1
                }),
            ),
        )
        .await
        .unwrap();
    let event_id = event_id_by_title(&fx, "Anchor").await;
    let revision = outline_revision(&fx).await;
    let content_before = std::fs::read_to_string(&outline_path).unwrap();

    // Unknown timeline event: the retained event NotFound.
    let err = fx
        .core
        .patch_timeline_event(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            patch(
                revision,
                serde_json::json!({
                    "operation": "bind_world_event",
                    "event_id": "evt_missing", "world_event_id": referent
                }),
            ),
        )
        .await
        .unwrap_err();
    let CoreError::NotFound { resource } = err else {
        panic!("unknown event must be NotFound, got {err:?}");
    };
    assert_eq!(resource, "event evt_missing");

    // A referent no World holds.
    assert_outline_validation(
        "missing referent",
        fx.core
            .patch_timeline_event(
                &fx.principal,
                "http",
                fx.work_id.clone(),
                patch(
                    revision,
                    serde_json::json!({
                        "operation": "bind_world_event",
                        "event_id": event_id, "world_event_id": "kb_0000dead"
                    }),
                ),
            )
            .await,
        "does not exist in the Work's bound World",
    );

    // A referent that exists as an event, but in a different World: the Work's
    // stored binding — never a payload claim — decides membership.
    assert_outline_validation(
        "foreign-World referent",
        fx.core
            .patch_timeline_event(
                &fx.principal,
                "http",
                fx.work_id.clone(),
                patch(
                    revision,
                    serde_json::json!({
                        "operation": "bind_world_event",
                        "event_id": event_id, "world_event_id": foreign_referent
                    }),
                ),
            )
            .await,
        "does not exist in the Work's bound World",
    );

    // A bound-World referent that is not an event entity.
    assert_outline_validation(
        "non-event referent",
        fx.core
            .patch_timeline_event(
                &fx.principal,
                "http",
                fx.work_id.clone(),
                patch(
                    revision,
                    serde_json::json!({
                        "operation": "bind_world_event",
                        "event_id": event_id, "world_event_id": "kb_0000c003"
                    }),
                ),
            )
            .await,
        "is a 'character' entity",
    );

    // Missing members are the retained 400 invalid_input channel.
    for (value, field) in [
        (
            serde_json::json!({"operation": "bind_world_event", "event_id": event_id}),
            "missing_world_event_id",
        ),
        (
            serde_json::json!({
                "operation": "bind_world_event", "world_event_id": referent
            }),
            "missing_event_id",
        ),
        (
            serde_json::json!({"operation": "unbind_world_event"}),
            "missing_event_id",
        ),
    ] {
        let err = fx
            .core
            .patch_timeline_event(
                &fx.principal,
                "http",
                fx.work_id.clone(),
                patch(revision, value),
            )
            .await
            .unwrap_err();
        let CoreError::InvalidInput { field: actual, .. } = err else {
            panic!("missing member must be invalid_input, got {err:?}");
        };
        assert_eq!(actual, field);
    }

    // A Work with no bound World refuses both binding operations.
    sqlx::query("UPDATE works SET world_id = NULL WHERE work_id = ?")
        .bind(&fx.work_id)
        .execute(&fx.pool)
        .await
        .unwrap();
    assert_outline_validation(
        "unbound World bind",
        fx.core
            .patch_timeline_event(
                &fx.principal,
                "http",
                fx.work_id.clone(),
                patch(
                    revision,
                    serde_json::json!({
                        "operation": "bind_world_event",
                        "event_id": event_id, "world_event_id": referent
                    }),
                ),
            )
            .await,
        "has no bound World",
    );
    assert_outline_validation(
        "unbound World unbind",
        fx.core
            .patch_timeline_event(
                &fx.principal,
                "http",
                fx.work_id.clone(),
                patch(
                    revision,
                    serde_json::json!({
                        "operation": "unbind_world_event", "event_id": event_id
                    }),
                ),
            )
            .await,
        "has no bound World",
    );

    // Not one refusal wrote anything.
    let outline = fx
        .core
        .work_outline(&fx.principal, fx.work_id.clone())
        .await
        .unwrap();
    assert_eq!(outline.outline_revision, revision);
    assert_eq!(outline.timeline_events[0].world_event_id, None);
    assert_eq!(
        std::fs::read_to_string(&outline_path).unwrap(),
        content_before,
        "refused bindings must not rewrite the outline file"
    );
    fx.pool.close().await;
    fx.core.close().await.unwrap();
}

/// A Work whose **stored** World belongs to another creator refuses both
/// binding operations with the typed ownership denial (rendered 403 at the
/// adapter boundary) before any World read, leaving the outline revision and
/// the file content untouched.
#[tokio::test]
async fn timeline_world_event_binding_refuses_a_foreign_stored_world() {
    let fx = setup().await;
    let rel_path = "Works/test-novel/Outlines/outline.md";
    let outline_path = fx.creative_root.join(rel_path);
    let world_id = work_world_id(&fx).await;
    let referent = "kb_0000e005";
    create_kb_entity(&fx, &world_id, referent, "Bystander Event", "event").await;

    let patch = |base: u64, mut value: serde_json::Value| {
        value["work_id"] = serde_json::json!(fx.work_id.clone());
        value["base_revision"] = serde_json::json!(base);
        timeline_request(value)
    };
    fx.core
        .patch_timeline_event(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            patch(
                0,
                serde_json::json!({
                    "operation": "add_event", "title": "Bystander", "realizes_chapter_id": 1
                }),
            ),
        )
        .await
        .unwrap();
    let event_id = event_id_by_title(&fx, "Bystander").await;
    let revision = outline_revision(&fx).await;
    let content_before = std::fs::read_to_string(&outline_path).unwrap();

    // The Work's stored `world_id` still names the World; only its owner is
    // reassigned to another creator. The Work's binding is never trusted from
    // the payload, and the guard refuses before the referent read.
    //
    // `narrative_worlds.owner_creator_id` is `NOT NULL`
    // (`crates/nexus-local-db/migrations/20260524_narrative_worlds.sql:10`), so
    // the guard's unowned branch is not constructible; the foreign-owner state
    // is the representable denial this proves.
    let other_creator = "other-author";
    nexus_local_db::creators::ensure_creator_row(&fx.pool, other_creator, "Other Author")
        .await
        .unwrap();
    sqlx::query("UPDATE narrative_worlds SET owner_creator_id = ? WHERE world_id = ?")
        .bind(other_creator)
        .bind(&world_id)
        .execute(&fx.pool)
        .await
        .unwrap();

    for (operation, value) in [
        (
            "bind",
            serde_json::json!({
                "operation": "bind_world_event",
                "event_id": event_id, "world_event_id": referent
            }),
        ),
        (
            "unbind",
            serde_json::json!({
                "operation": "unbind_world_event", "event_id": event_id
            }),
        ),
    ] {
        let err = fx
            .core
            .patch_timeline_event(
                &fx.principal,
                "http",
                fx.work_id.clone(),
                patch(revision, value),
            )
            .await
            .unwrap_err();
        let CoreError::WorldOwnerDenied {
            world_id: denied, ..
        } = err
        else {
            panic!(
                "a foreign stored World must refuse {operation} with the typed ownership \
                 denial, got {err:?}"
            );
        };
        assert_eq!(
            denied, world_id,
            "{operation} must name the Work's stored World"
        );
    }

    let outline = fx
        .core
        .work_outline(&fx.principal, fx.work_id.clone())
        .await
        .unwrap();
    assert_eq!(outline.outline_revision, revision);
    assert_eq!(outline.timeline_events[0].world_event_id, None);
    assert_eq!(
        std::fs::read_to_string(&outline_path).unwrap(),
        content_before,
        "an ownership refusal must not rewrite the outline file"
    );
    fx.pool.close().await;
    fx.core.close().await.unwrap();
}

/// The bind referent is proven by a targeted membership read, not a capped
/// graph scan: an event that exists in the bound World but sorts past
/// `GRAPH_ENTITY_CAP` (500) still binds instead of being refused as absent.
#[tokio::test]
async fn timeline_world_event_binding_accepts_a_referent_past_the_graph_cap() {
    let fx = setup().await;
    let world_id = work_world_id(&fx).await;
    let referent = "kb_0000e006";
    create_kb_entity(&fx, &world_id, referent, "Beyond Cap", "event").await;

    // 500 World-owned entities created before the referent: the graph read
    // (`created_at ASC LIMIT 500`) closes its window before the referent.
    for index in 0..500 {
        sqlx::query(
            r"INSERT INTO kb_key_blocks
                (key_block_id, owner_kind, world_id, block_type, canonical_name, status,
                 revision, created_at, updated_at)
              VALUES (?, 'world', ?, 'event', ?, 'confirmed', 0, ?, ?)",
        )
        .bind(format!("kb_cap_{index:05}"))
        .bind(&world_id)
        .bind(format!("Cap Filler {index}"))
        .bind("2000-01-01T00:00:00Z")
        .bind("2000-01-01T00:00:00Z")
        .execute(&fx.pool)
        .await
        .unwrap();
    }

    let patch = |base: u64, mut value: serde_json::Value| {
        value["work_id"] = serde_json::json!(fx.work_id.clone());
        value["base_revision"] = serde_json::json!(base);
        timeline_request(value)
    };
    fx.core
        .patch_timeline_event(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            patch(
                0,
                serde_json::json!({
                    "operation": "add_event", "title": "Past Cap", "realizes_chapter_id": 1
                }),
            ),
        )
        .await
        .unwrap();
    let event_id = event_id_by_title(&fx, "Past Cap").await;

    let bound = fx
        .core
        .patch_timeline_event(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            patch(
                1,
                serde_json::json!({
                    "operation": "bind_world_event",
                    "event_id": event_id, "world_event_id": referent
                }),
            ),
        )
        .await
        .expect("a referent past the graph cap must still bind");
    assert_eq!(bound.new_revision, NonZeroU64::new(2).unwrap());
    let outline = fx
        .core
        .work_outline(&fx.principal, fx.work_id.clone())
        .await
        .unwrap();
    assert_eq!(
        outline.timeline_events[0].world_event_id.as_deref(),
        Some(referent)
    );
    fx.pool.close().await;
    fx.core.close().await.unwrap();
}

/// The retained CAS guard refuses a binding attempt with the outline content
/// and revision untouched.
#[tokio::test]
async fn timeline_world_event_binding_stale_cas_leaves_outline_unchanged() {
    let fx = setup().await;
    let rel_path = "Works/test-novel/Outlines/outline.md";
    let outline_path = fx.creative_root.join(rel_path);
    let world_id = work_world_id(&fx).await;
    let referent = "kb_0000e004";
    create_kb_entity(&fx, &world_id, referent, "Guarded Event", "event").await;

    let patch = |base: u64, mut value: serde_json::Value| {
        value["work_id"] = serde_json::json!(fx.work_id.clone());
        value["base_revision"] = serde_json::json!(base);
        timeline_request(value)
    };
    fx.core
        .patch_timeline_event(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            patch(
                0,
                serde_json::json!({
                    "operation": "add_event", "title": "Guarded", "realizes_chapter_id": 1
                }),
            ),
        )
        .await
        .unwrap();
    let event_id = event_id_by_title(&fx, "Guarded").await;
    let revision = outline_revision(&fx).await;
    let content_before = std::fs::read_to_string(&outline_path).unwrap();

    // Stale base_revision: the typed OCC conflict, nothing written.
    let err = fx
        .core
        .patch_timeline_event(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            patch(
                0,
                serde_json::json!({
                    "operation": "bind_world_event",
                    "event_id": event_id, "world_event_id": referent
                }),
            ),
        )
        .await
        .unwrap_err();
    let CoreError::OutlineConflict(details) = err else {
        panic!("stale base_revision must be the typed outline conflict, got {err:?}");
    };
    assert_eq!(details.current_revision, revision);

    let outline = fx
        .core
        .work_outline(&fx.principal, fx.work_id.clone())
        .await
        .unwrap();
    assert_eq!(outline.outline_revision, revision);
    assert_eq!(outline.timeline_events[0].world_event_id, None);
    assert_eq!(
        std::fs::read_to_string(&outline_path).unwrap(),
        content_before
    );
    fx.pool.close().await;
    fx.core.close().await.unwrap();
}

/// The published-chapter binding arms at the public service entry: publishing
/// pins a chapter's **new** World-event bindings but not a stale one in place,
/// so a bind on a published chapter still refuses and names the sanctioned
/// unbind path, while that unbind clears the binding through
/// `CoreService::patch_timeline_event` — the remedy an author can reach.
///
/// The retained envelopes survive on that path: a stale `base_revision` is the
/// typed 409 conflict, a foreign stored World is the typed ownership denial
/// (403 at the adapter boundary), and no denial rewrites the outline file.
///
/// Nothing in this product publishes a chapter — `patch_outline_chapter`'s
/// lifecycle vocabulary stops at `finalized` — so the published row is
/// manufactured through the chapter-SSOT helper the reconcile path uses,
/// scoped to this test.
#[tokio::test]
async fn timeline_world_event_binding_on_a_published_chapter_keeps_the_unbind_remedy() {
    let fx = setup().await;
    let rel_path = "Works/test-novel/Outlines/outline.md";
    let outline_path = fx.creative_root.join(rel_path);
    let world_id = work_world_id(&fx).await;
    let referent = "kb_0000e007";
    create_kb_entity(&fx, &world_id, referent, "Pinned Event", "event").await;

    let patch = |base: u64, mut value: serde_json::Value| {
        value["work_id"] = serde_json::json!(fx.work_id.clone());
        value["base_revision"] = serde_json::json!(base);
        timeline_request(value)
    };
    fx.core
        .patch_timeline_event(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            patch(
                0,
                serde_json::json!({
                    "operation": "add_event", "title": "Pinned", "realizes_chapter_id": 1
                }),
            ),
        )
        .await
        .unwrap();
    let event_id = event_id_by_title(&fx, "Pinned").await;
    // Bind while the chapter is still editable, then publish it: the stored
    // referent is exactly what a re-typed World event leaves behind on a
    // published chapter.
    fx.core
        .patch_timeline_event(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            patch(
                1,
                serde_json::json!({
                    "operation": "bind_world_event",
                    "event_id": event_id, "world_event_id": referent
                }),
            ),
        )
        .await
        .unwrap();
    nexus_local_db::work_chapters::update_status(
        &fx.pool,
        &fx.work_id,
        1,
        1,
        "published",
        None,
        &chrono::Utc::now().to_rfc3339(),
    )
    .await
    .unwrap();

    let revision = outline_revision(&fx).await;
    assert_eq!(
        revision, 2,
        "a bind then a publish leaves the outline at revision 2"
    );
    let published_content = std::fs::read_to_string(&outline_path).unwrap();

    // A new binding on the published chapter still refuses, and the refusal
    // names the sanctioned unbind path.
    assert_outline_validation(
        "published bind",
        fx.core
            .patch_timeline_event(
                &fx.principal,
                "http",
                fx.work_id.clone(),
                patch(
                    revision,
                    serde_json::json!({
                        "operation": "bind_world_event",
                        "event_id": event_id, "world_event_id": referent
                    }),
                ),
            )
            .await,
        "unbind_world_event",
    );

    // The published path keeps the retained CAS envelope: a stale
    // `base_revision` is the typed conflict, not a published-chapter refusal.
    let err = fx
        .core
        .patch_timeline_event(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            patch(
                revision - 1,
                serde_json::json!({
                    "operation": "unbind_world_event", "event_id": event_id
                }),
            ),
        )
        .await
        .unwrap_err();
    let CoreError::OutlineConflict(details) = err else {
        panic!("stale base_revision must be the typed outline conflict, got {err:?}");
    };
    assert_eq!(details.current_revision, revision);

    // Neither refusal wrote anything.
    let outline = fx
        .core
        .work_outline(&fx.principal, fx.work_id.clone())
        .await
        .unwrap();
    assert_eq!(outline.outline_revision, revision);
    assert_eq!(
        outline.timeline_events[0].world_event_id.as_deref(),
        Some(referent)
    );
    assert_eq!(
        std::fs::read_to_string(&outline_path).unwrap(),
        published_content,
        "a refused mutation on a published chapter must not rewrite the outline file"
    );

    // The remedy: the author clears the stale binding through the public patch
    // path — one revision bump, no chapter content or structure touched.
    let cleared = fx
        .core
        .patch_timeline_event(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            patch(
                revision,
                serde_json::json!({
                    "operation": "unbind_world_event", "event_id": event_id
                }),
            ),
        )
        .await
        .expect("the sanctioned unbind must be reachable on a published chapter");
    assert_eq!(cleared.new_revision, NonZeroU64::new(3).unwrap());
    let outline = fx
        .core
        .work_outline(&fx.principal, fx.work_id.clone())
        .await
        .unwrap();
    assert_eq!(outline.outline_revision, 3);
    assert_eq!(outline.timeline_events[0].world_event_id, None);
    let unbound_content = std::fs::read_to_string(&outline_path).unwrap();
    assert!(
        !unbound_content.contains(referent),
        "the sanctioned unbind must clear the stored referent from the outline"
    );

    // A foreign stored World still refuses both operations on the published
    // chapter with the typed ownership denial before any chapter-status read,
    // leaving the outline revision and the file content untouched.
    let other_creator = "other-author";
    nexus_local_db::creators::ensure_creator_row(&fx.pool, other_creator, "Other Author")
        .await
        .unwrap();
    sqlx::query("UPDATE narrative_worlds SET owner_creator_id = ? WHERE world_id = ?")
        .bind(other_creator)
        .bind(&world_id)
        .execute(&fx.pool)
        .await
        .unwrap();
    for (operation, value) in [
        (
            "bind",
            serde_json::json!({
                "operation": "bind_world_event",
                "event_id": event_id, "world_event_id": referent
            }),
        ),
        (
            "unbind",
            serde_json::json!({
                "operation": "unbind_world_event", "event_id": event_id
            }),
        ),
    ] {
        let err = fx
            .core
            .patch_timeline_event(&fx.principal, "http", fx.work_id.clone(), patch(3, value))
            .await
            .unwrap_err();
        let CoreError::WorldOwnerDenied {
            world_id: denied, ..
        } = err
        else {
            panic!(
                "a foreign stored World must refuse {operation} with the typed ownership \
                 denial, got {err:?}"
            );
        };
        assert_eq!(
            denied, world_id,
            "{operation} must name the Work's stored World"
        );
    }
    let outline = fx
        .core
        .work_outline(&fx.principal, fx.work_id.clone())
        .await
        .unwrap();
    assert_eq!(outline.outline_revision, 3);
    assert_eq!(outline.timeline_events[0].world_event_id, None);
    assert_eq!(
        std::fs::read_to_string(&outline_path).unwrap(),
        unbound_content,
        "an ownership refusal must not rewrite the outline file"
    );
    fx.pool.close().await;
    fx.core.close().await.unwrap();
}

/// A content patch on a chapter with no stored outline path seeds the derived
/// path, writes the per-chapter prose durably, and the prose reads back.
#[tokio::test]
async fn outline_chapter_patch_writes_prose_and_seeds_path() {
    let fx = setup().await;
    fx.core
        .patch_outline_chapter(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            "1".into(),
            chapter_patch_request(serde_json::json!({
                "work_id": fx.work_id, "base_revision": 0, "chapter_id": 1,
                "set": {"content": "outline prose"}
            })),
        )
        .await
        .unwrap();

    let prose_path = fx
        .creative_root
        .join("Works/test-novel/Outlines/chapters/ch01-outline.md");
    assert_eq!(
        std::fs::read_to_string(&prose_path).unwrap(),
        "outline prose"
    );

    let detail = fx
        .core
        .chapter_detail(
            &fx.principal,
            fx.work_id.clone(),
            "1".into(),
            content_query(),
        )
        .await
        .unwrap();
    assert_eq!(
        detail.outline_path.as_deref(),
        Some("Works/test-novel/Outlines/chapters/ch01-outline.md")
    );
    assert!(detail.can_edit_outline);

    let outline = fx
        .core
        .chapter_outline(
            &fx.principal,
            fx.work_id.clone(),
            "1".into(),
            content_query(),
        )
        .await
        .unwrap();
    assert_eq!(outline.content, "outline prose");
    fx.pool.close().await;
    fx.core.close().await.unwrap();
}

/// Frontmatter delimiter edges (R-V172-GREPTILE-004): an indented `---` inside
/// a YAML block scalar is body content, not a closing delimiter, so the file
/// still parses and patches; a bare inline `---more` line is rejected instead
/// of splitting. (Unknown frontmatter keys are dropped by the pre-existing
/// typed frontmatter round-trip — behavior identical to the daemon era.)
#[tokio::test]
async fn outline_frontmatter_delimiter_edges() {
    let fx = setup().await;
    let rel_path = "Works/test-novel/Outlines/outline.md";
    let outline_path = fx.creative_root.join(rel_path);
    std::fs::create_dir_all(outline_path.parent().unwrap()).unwrap();
    std::fs::write(
        &outline_path,
        "---\noutline_revision: 0\nvolumes: []\ntimeline_events: []\nforeshadows: []\nchapter_titles: {}\nupdated_at: \"2024-01-01T00:00:00Z\"\nbody_intro: |\n  ---\n  multi-line\n---\nactual body\n",
    )
    .unwrap();

    // If the split mistook the indented `---` for the delimiter, the truncated
    // frontmatter would fail YAML parsing and the patch would be rejected.
    fx.core
        .patch_outline_chapter(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            "1".into(),
            chapter_patch_request(serde_json::json!({
                "work_id": fx.work_id, "base_revision": 0, "chapter_id": 1,
                "set": {"title": "Titled"}
            })),
        )
        .await
        .unwrap();

    let final_content = std::fs::read_to_string(&outline_path).unwrap();
    assert!(
        final_content.contains("actual body\n"),
        "body must survive: {final_content}"
    );

    // A bare `---more` line is not a delimiter: the split returns None and the
    // file falls back to a default frontmatter with the raw content preserved
    // as body (identical to the daemon-era `split_frontmatter` None path).
    std::fs::write(&outline_path, "---\ntitle: test\n---more\nbody").unwrap();
    fx.core
        .patch_outline_chapter(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            "1".into(),
            chapter_patch_request(serde_json::json!({
                "work_id": fx.work_id, "base_revision": 0, "chapter_id": 1,
                "set": {"title": "Other"}
            })),
        )
        .await
        .unwrap();
    let final_content = std::fs::read_to_string(&outline_path).unwrap();
    assert!(
        final_content.contains("---more\nbody"),
        "raw content must be preserved as body on the None-split fallback: {final_content}"
    );
    assert!(
        final_content.starts_with("---\noutline_revision: 1"),
        "default frontmatter must be re-seeded and bumped: {final_content}"
    );
    fx.pool.close().await;
    fx.core.close().await.unwrap();
}

/// A legacy `outline.md` written before the scene/beat carriers existed has no
/// `scenes`/`beats` keys. Both deserialize to empty arrays, so the Work still
/// opens and the outline revision is untouched.
#[tokio::test]
async fn outline_missing_scenes_and_beats_default_to_empty_arrays() {
    let fx = setup().await;
    let rel_path = "Works/test-novel/Outlines/outline.md";
    let outline_path = fx.creative_root.join(rel_path);
    std::fs::create_dir_all(outline_path.parent().unwrap()).unwrap();
    std::fs::write(
        &outline_path,
        "---\noutline_revision: 4\nvolumes: []\ntimeline_events: []\nforeshadows: []\nchapter_titles: {}\nupdated_at: \"2024-01-01T00:00:00Z\"\n---\nbody\n",
    )
    .unwrap();

    let outline = fx
        .core
        .work_outline(&fx.principal, fx.work_id.clone())
        .await
        .unwrap();
    assert_eq!(outline.outline_revision, 4);
    assert!(outline.scenes.is_empty(), "absent scenes default to empty");
    assert!(outline.beats.is_empty(), "absent beats default to empty");

    let json = serde_json::to_value(&outline).unwrap();
    assert_eq!(json["scenes"], serde_json::json!([]));
    assert_eq!(json["beats"], serde_json::json!([]));

    fx.pool.close().await;
    fx.core.close().await.unwrap();
}

/// Frontmatter-written scenes/beats are carried onto the wire verbatim, in
/// file order, with the per-item fields and status vocabulary intact.
#[tokio::test]
async fn outline_scenes_and_beats_round_trip_verbatim() {
    let fx = setup().await;
    let rel_path = "Works/test-novel/Outlines/outline.md";
    let outline_path = fx.creative_root.join(rel_path);
    std::fs::create_dir_all(outline_path.parent().unwrap()).unwrap();
    std::fs::write(
        &outline_path,
        "---\noutline_revision: 7\nvolumes: []\nscenes:\n  - scene_id: scn_alpha\n    chapter_id: 2\n    title: Opening Scene\n    status: drafted\nbeats:\n  - beat_id: bet_alpha\n    scene_id: scn_alpha\n    title: Inciting Moment\n    status: completed\ntimeline_events: []\nforeshadows: []\nchapter_titles: {}\nupdated_at: \"2024-01-01T00:00:00Z\"\n---\nbody\n",
    )
    .unwrap();

    let outline = fx
        .core
        .work_outline(&fx.principal, fx.work_id.clone())
        .await
        .unwrap();
    assert_eq!(outline.outline_revision, 7);

    let json = serde_json::to_value(&outline).unwrap();
    assert_eq!(
        json["scenes"],
        serde_json::json!([
            {
                "scene_id": "scn_alpha",
                "chapter_id": 2,
                "title": "Opening Scene",
                "status": "drafted"
            }
        ])
    );
    assert_eq!(
        json["beats"],
        serde_json::json!([
            {
                "beat_id": "bet_alpha",
                "scene_id": "scn_alpha",
                "title": "Inciting Moment",
                "status": "completed"
            }
        ])
    );

    fx.pool.close().await;
    fx.core.close().await.unwrap();
}

// ─── V1.200 DR-26: scene/beat authoring over the structure-patch path ────────

/// A core-minted scene/beat id is `<prefix><32 lowercase hex>` — the
/// convention the existing `evt_`/`wrk_` ids use, not a hyphenated UUID.
fn assert_minted_id(id: &str, prefix: &str) {
    let hex = id
        .strip_prefix(prefix)
        .unwrap_or_else(|| panic!("id '{id}' must start with '{prefix}'"));
    assert_eq!(hex.len(), 32, "id '{id}' must carry 32 hex characters");
    assert!(
        hex.chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
        "id '{id}' must be lowercase hex"
    );
}

/// Author a scene at `base_revision` and return the id the server minted.
///
/// The patch response is a revision envelope with empty side effects, so the
/// created id is observed through the canonical refetch — exactly how clients
/// obtain it.
async fn authored_scene(fx: &Fixture, base_revision: u64, chapter_id: u64, title: &str) -> String {
    fx.core
        .patch_outline_structure(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            structure_request(serde_json::json!({
                "work_id": fx.work_id, "base_revision": base_revision,
                "operation": "add_scene", "chapter_id": chapter_id, "title": title
            })),
        )
        .await
        .expect("add_scene must be accepted");
    fx.core
        .work_outline(&fx.principal, fx.work_id.clone())
        .await
        .expect("outline read")
        .scenes
        .into_iter()
        .find(|scene| scene.title == title)
        .unwrap_or_else(|| panic!("authored scene '{title}'"))
        .scene_id
}

/// Author a beat under `scene_id` at `base_revision`; returns its minted id.
async fn authored_beat(fx: &Fixture, base_revision: u64, scene_id: &str, title: &str) -> String {
    fx.core
        .patch_outline_structure(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            structure_request(serde_json::json!({
                "work_id": fx.work_id, "base_revision": base_revision,
                "operation": "add_beat", "scene_id": scene_id, "title": title
            })),
        )
        .await
        .expect("add_beat must be accepted");
    fx.core
        .work_outline(&fx.principal, fx.work_id.clone())
        .await
        .expect("outline read")
        .beats
        .into_iter()
        .find(|beat| beat.title == title)
        .unwrap_or_else(|| panic!("authored beat '{title}'"))
        .beat_id
}

/// `add_scene` / `add_beat` mint ids bound to their parent and each consume
/// only their own status member (`scene_status` / `beat_status`), one revision
/// bump per accepted operation, `drafted` as the omitted-value default and
/// `completed` as the other accepted value.
#[tokio::test]
async fn retained_outline_scene_beat_authoring_round_trip() {
    let fx = setup().await;
    assert_eq!(outline_revision(&fx).await, 0);

    let opening = authored_scene(&fx, 0, 1, "Opening Scene").await;
    assert_minted_id(&opening, "scn_");
    // F-3: a single accepted add persists exactly one revision bump — read back
    // from the canonical outline, not from the patch response envelope.
    assert_eq!(
        outline_revision(&fx).await,
        1,
        "exactly one persisted revision bump for one accepted add_scene"
    );
    let storm = authored_scene(&fx, 1, 2, "Storm Scene").await;
    assert_minted_id(&storm, "scn_");
    assert_ne!(opening, storm, "each add_scene mints a fresh id");

    let completed = fx
        .core
        .patch_outline_structure(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            structure_request(serde_json::json!({
                "work_id": fx.work_id, "base_revision": 2,
                "operation": "add_scene", "chapter_id": 3,
                "title": "Aftermath", "scene_status": "completed"
            })),
        )
        .await
        .expect("add_scene with an explicit status");
    assert_eq!(completed.new_revision, NonZeroU64::new(3).unwrap());

    let inciting = authored_beat(&fx, 3, &opening, "Inciting Moment").await;
    assert_minted_id(&inciting, "bet_");
    assert_eq!(
        outline_revision(&fx).await,
        4,
        "exactly one persisted revision bump for one accepted add_beat"
    );
    let reaction = fx
        .core
        .patch_outline_structure(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            structure_request(serde_json::json!({
                "work_id": fx.work_id, "base_revision": 4,
                "operation": "add_beat", "scene_id": storm,
                "title": "Reaction Beat", "beat_status": "completed"
            })),
        )
        .await
        .expect("add_beat with an explicit status");
    assert_eq!(reaction.new_revision, NonZeroU64::new(5).unwrap());

    let outline = fx
        .core
        .work_outline(&fx.principal, fx.work_id.clone())
        .await
        .expect("outline read");
    assert_eq!(
        outline.outline_revision, 5,
        "one revision bump per accepted operation"
    );
    assert_eq!(outline.scenes.len(), 3);
    assert_eq!(outline.beats.len(), 2);

    let opening_row = outline
        .scenes
        .iter()
        .find(|scene| scene.scene_id == opening)
        .expect("opening scene row");
    assert_eq!(opening_row.chapter_id, NonZeroU64::new(1).unwrap());
    assert_eq!(opening_row.title, "Opening Scene");
    assert_eq!(opening_row.status, WorkOutlineScenesItemStatus::Drafted);

    let aftermath = outline
        .scenes
        .iter()
        .find(|scene| scene.title == "Aftermath")
        .expect("aftermath scene row");
    assert_eq!(
        aftermath.status,
        WorkOutlineScenesItemStatus::Completed,
        "the declared completed value is persisted verbatim"
    );

    let inciting_row = outline
        .beats
        .iter()
        .find(|beat| beat.beat_id == inciting)
        .expect("inciting beat row");
    assert_eq!(inciting_row.scene_id, opening, "the beat keeps its parent");
    assert_eq!(inciting_row.status, WorkOutlineBeatsItemStatus::Drafted);
    let reaction_row = outline
        .beats
        .iter()
        .find(|beat| beat.title == "Reaction Beat")
        .expect("reaction beat row");
    assert_eq!(reaction_row.scene_id, storm);
    assert_eq!(
        reaction_row.status,
        WorkOutlineBeatsItemStatus::Completed,
        "beat_status lands on the authored beat"
    );
    let storm_row = outline
        .scenes
        .iter()
        .find(|scene| scene.scene_id == storm)
        .expect("storm scene row");
    assert_eq!(
        storm_row.status,
        WorkOutlineScenesItemStatus::Drafted,
        "beat_status never leaks onto the beat's parent scene"
    );

    fx.pool.close().await;
    fx.core.close().await.unwrap();
}

/// `remove_beat` drops exactly one beat; `remove_scene` cascades that scene's
/// beats in the same single revision bump.
#[tokio::test]
async fn retained_outline_scene_beat_removals() {
    let fx = setup().await;
    let scene_a = authored_scene(&fx, 0, 1, "Scene A").await;
    let scene_b = authored_scene(&fx, 1, 2, "Scene B").await;
    let beat_a1 = authored_beat(&fx, 2, &scene_a, "Beat A1").await;
    let beat_a2 = authored_beat(&fx, 3, &scene_a, "Beat A2").await;
    let surviving_beat = authored_beat(&fx, 4, &scene_b, "Beat B1").await;

    let removed_beat = fx
        .core
        .patch_outline_structure(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            structure_request(serde_json::json!({
                "work_id": fx.work_id, "base_revision": 5,
                "operation": "remove_beat", "beat_id": beat_a2
            })),
        )
        .await
        .expect("remove_beat");
    assert_eq!(removed_beat.new_revision, NonZeroU64::new(6).unwrap());
    let outline = fx
        .core
        .work_outline(&fx.principal, fx.work_id.clone())
        .await
        .expect("outline read");
    assert!(
        outline.beats.iter().all(|beat| beat.beat_id != beat_a2),
        "the removed beat is gone"
    );
    assert_eq!(outline.beats.len(), 2, "only that beat is removed");
    assert_eq!(outline.scenes.len(), 2, "beats never remove their scene");

    let removed_scene = fx
        .core
        .patch_outline_structure(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            structure_request(serde_json::json!({
                "work_id": fx.work_id, "base_revision": 6,
                "operation": "remove_scene", "scene_id": scene_a
            })),
        )
        .await
        .expect("remove_scene");
    assert_eq!(removed_scene.new_revision, NonZeroU64::new(7).unwrap());
    let outline = fx
        .core
        .work_outline(&fx.principal, fx.work_id.clone())
        .await
        .expect("outline read");
    assert_eq!(outline.outline_revision, 7, "one bump for the cascade");
    assert!(outline.scenes.iter().all(|scene| scene.scene_id != scene_a));
    assert!(
        outline.beats.iter().all(|beat| beat.scene_id != scene_a),
        "the removed scene's beats cascade with it: {:?}",
        outline.beats
    );
    assert_eq!(outline.beats.len(), 1);
    assert_eq!(outline.beats[0].beat_id, surviving_beat);
    assert!(outline.beats.iter().all(|beat| beat.beat_id != beat_a1));

    fx.pool.close().await;
    fx.core.close().await.unwrap();
}

/// Unknown chapter / scene / beat ids refuse with the not-found class, a blank
/// title with the structured validation class, and every refusal leaves the
/// canonical outline byte-identical.
#[tokio::test]
async fn retained_outline_scene_beat_unknown_ids_refuse() {
    let fx = setup().await;
    let scene = authored_scene(&fx, 0, 1, "Real Scene").await;
    let _beat = authored_beat(&fx, 1, &scene, "Real Beat").await;

    let before = serde_json::to_value(
        fx.core
            .work_outline(&fx.principal, fx.work_id.clone())
            .await
            .expect("outline read"),
    )
    .unwrap();

    let unknown_chapter = fx
        .core
        .patch_outline_structure(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            structure_request(serde_json::json!({
                "work_id": fx.work_id, "base_revision": 2,
                "operation": "add_scene", "chapter_id": 99, "title": "Orphan"
            })),
        )
        .await;
    assert!(
        matches!(unknown_chapter, Err(CoreError::NotFound { .. })),
        "add_scene on an unknown chapter: {unknown_chapter:?}"
    );

    let unknown_scene = fx
        .core
        .patch_outline_structure(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            structure_request(serde_json::json!({
                "work_id": fx.work_id, "base_revision": 2,
                "operation": "add_beat", "scene_id": "scn_missing", "title": "Orphan"
            })),
        )
        .await;
    assert!(
        matches!(unknown_scene, Err(CoreError::NotFound { .. })),
        "add_beat on an unknown scene: {unknown_scene:?}"
    );

    let unknown_removed_scene = fx
        .core
        .patch_outline_structure(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            structure_request(serde_json::json!({
                "work_id": fx.work_id, "base_revision": 2,
                "operation": "remove_scene", "scene_id": "scn_missing"
            })),
        )
        .await;
    assert!(
        matches!(unknown_removed_scene, Err(CoreError::NotFound { .. })),
        "remove_scene on an unknown scene: {unknown_removed_scene:?}"
    );

    let unknown_removed_beat = fx
        .core
        .patch_outline_structure(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            structure_request(serde_json::json!({
                "work_id": fx.work_id, "base_revision": 2,
                "operation": "remove_beat", "beat_id": "bet_missing"
            })),
        )
        .await;
    assert!(
        matches!(unknown_removed_beat, Err(CoreError::NotFound { .. })),
        "remove_beat on an unknown beat: {unknown_removed_beat:?}"
    );

    let blank_title = fx
        .core
        .patch_outline_structure(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            structure_request(serde_json::json!({
                "work_id": fx.work_id, "base_revision": 2,
                "operation": "add_scene", "chapter_id": 1, "title": "   "
            })),
        )
        .await;
    assert_outline_validation("blank scene title", blank_title, "must not be blank");

    let after = serde_json::to_value(
        fx.core
            .work_outline(&fx.principal, fx.work_id.clone())
            .await
            .expect("outline read"),
    )
    .unwrap();
    assert_eq!(after, before, "refusals never mutate the outline");

    fx.pool.close().await;
    fx.core.close().await.unwrap();
}

/// All four scene/beat operations resolve the owning chapter and refuse on a
/// published one — a beat cannot bypass the guard through its scene id.
#[tokio::test]
async fn retained_outline_scene_beat_published_chapter_guard() {
    let fx = setup().await;
    let scene = authored_scene(&fx, 0, 1, "Frozen Scene").await;
    let beat = authored_beat(&fx, 1, &scene, "Frozen Beat").await;
    sqlx::query("UPDATE work_chapters SET status = 'published' WHERE work_id = ? AND chapter = 1")
        .bind(&fx.work_id)
        .execute(&fx.pool)
        .await
        .unwrap();

    let before = serde_json::to_value(
        fx.core
            .work_outline(&fx.principal, fx.work_id.clone())
            .await
            .expect("outline read"),
    )
    .unwrap();

    let add_scene = fx
        .core
        .patch_outline_structure(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            structure_request(serde_json::json!({
                "work_id": fx.work_id, "base_revision": 2,
                "operation": "add_scene", "chapter_id": 1, "title": "Blocked Scene"
            })),
        )
        .await;
    assert_outline_validation("published add_scene", add_scene, "published chapter 1");

    let add_beat = fx
        .core
        .patch_outline_structure(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            structure_request(serde_json::json!({
                "work_id": fx.work_id, "base_revision": 2,
                "operation": "add_beat", "scene_id": scene, "title": "Blocked Beat"
            })),
        )
        .await;
    assert_outline_validation(
        "published add_beat through a scene id",
        add_beat,
        "published chapter 1",
    );

    let remove_beat = fx
        .core
        .patch_outline_structure(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            structure_request(serde_json::json!({
                "work_id": fx.work_id, "base_revision": 2,
                "operation": "remove_beat", "beat_id": beat
            })),
        )
        .await;
    assert_outline_validation(
        "published remove_beat through a scene id",
        remove_beat,
        "published chapter 1",
    );

    let remove_scene = fx
        .core
        .patch_outline_structure(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            structure_request(serde_json::json!({
                "work_id": fx.work_id, "base_revision": 2,
                "operation": "remove_scene", "scene_id": scene
            })),
        )
        .await;
    assert_outline_validation(
        "published remove_scene",
        remove_scene,
        "published chapter 1",
    );

    let after = serde_json::to_value(
        fx.core
            .work_outline(&fx.principal, fx.work_id.clone())
            .await
            .expect("outline read"),
    )
    .unwrap();
    assert_eq!(after, before, "refusals never mutate the outline");

    fx.pool.close().await;
    fx.core.close().await.unwrap();
}

/// A stale `base_revision` is the retained CAS conflict, and the refusal
/// leaves the persisted scene/beat carrier untouched.
#[tokio::test]
async fn retained_outline_scene_beat_stale_revision_conflicts() {
    let fx = setup().await;
    let _scene = authored_scene(&fx, 0, 1, "Scene").await;

    let before = serde_json::to_value(
        fx.core
            .work_outline(&fx.principal, fx.work_id.clone())
            .await
            .expect("outline read"),
    )
    .unwrap();

    let stale = fx
        .core
        .patch_outline_structure(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            structure_request(serde_json::json!({
                "work_id": fx.work_id, "base_revision": 0,
                "operation": "add_scene", "chapter_id": 2, "title": "Stale Scene"
            })),
        )
        .await
        .expect_err("a stale base_revision must conflict");
    let CoreError::OutlineConflict(details) = stale else {
        panic!("expected a typed outline conflict, got {stale:?}");
    };
    assert_eq!(details.current_revision, 1);
    assert_eq!(details.conflicting_path, "outline_revision");

    let after = serde_json::to_value(
        fx.core
            .work_outline(&fx.principal, fx.work_id.clone())
            .await
            .expect("outline read"),
    )
    .unwrap();
    assert_eq!(after, before, "the conflict left the outline untouched");

    fx.pool.close().await;
    fx.core.close().await.unwrap();
}

/// Wording-independent lock-conflict assertion (PM addition): a refused
/// mutation must report the `work_locked` class for this Work and name the
/// holder that actually holds it. No holder spelling is a contract.
fn assert_lock_conflict(resource: &str, work_id: &str, holder: &str) {
    assert!(
        resource.starts_with("work_locked:"),
        "lock conflict class: {resource}"
    );
    assert!(
        resource.contains(work_id),
        "lock conflict must name the Work: {resource}"
    );
    assert!(
        resource.contains(holder),
        "lock conflict must name the current holder: {resource}"
    );
}

/// A Work held by a runtime lock refuses the content mutation as a lock
/// conflict — whatever holder label the holder and the contender carry — and
/// leaves the durable chapter row untouched; releasing the holder restores
/// writability and the mutator's own holder is gone once it returns.
#[tokio::test]
async fn locked_work_refuses_conflicting_holder() {
    let fx = setup().await;
    // First lock attempt wins; a later attempt under a different holder label is
    // refused as a lock conflict naming the holder that already owns the Work.
    // Neither label is asserted — only the conflict class is.
    let holder = nexus_local_db::cli_holder("core");
    let acquired = nexus_local_db::acquire_runtime_lock(
        &fx.pool,
        fx.principal.creator_id(),
        &fx.work_id,
        &holder,
        nexus_local_db::ttl_from_env(),
        false,
    )
    .await
    .unwrap();
    assert!(matches!(
        acquired,
        nexus_local_db::AcquireResult::Acquired { .. }
    ));
    let contender = nexus_local_db::cli_holder("http");
    match nexus_local_db::acquire_runtime_lock(
        &fx.pool,
        fx.principal.creator_id(),
        &fx.work_id,
        &contender,
        nexus_local_db::ttl_from_env(),
        false,
    )
    .await
    .unwrap()
    {
        nexus_local_db::AcquireResult::Locked {
            holder: existing, ..
        } => assert_eq!(existing, holder, "conflict must name the current holder"),
        nexus_local_db::AcquireResult::Acquired { .. } => {
            panic!("a second lock attempt must be refused as a lock conflict");
        }
    }

    let err = fx
        .core
        .patch_chapter(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            "1".into(),
            content_query(),
            patch_request(serde_json::json!({"slug": "blocked-by-lock"})),
        )
        .await
        .expect_err("locked Work must reject the patch");
    let CoreError::Forbidden { resource } = err else {
        panic!("expected locked carrier, got {err:?}");
    };
    assert_lock_conflict(&resource, &fx.work_id, &holder);

    // Refusals leave the durable row consistent: the seeded slug survives and
    // the holder is still the one that acquired the lock.
    let unchanged = fx
        .core
        .chapter_detail(
            &fx.principal,
            fx.work_id.clone(),
            "1".into(),
            content_query(),
        )
        .await
        .unwrap();
    assert_eq!(unchanged.slug.as_deref(), Some("ch01"), "slug untouched");
    assert_eq!(
        fx.core
            .get_work(&fx.principal, fx.work_id.clone())
            .await
            .unwrap()
            .runtime_lock_holder
            .as_deref(),
        Some(holder.as_str())
    );

    // Releasing the holder restores writability, and the mutator's own holder
    // is gone once it returns.
    assert!(nexus_local_db::release_runtime_lock(
        &fx.pool,
        fx.principal.creator_id(),
        &fx.work_id,
        &holder,
    )
    .await
    .unwrap());
    let unlocked = fx
        .core
        .patch_chapter(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            "1".into(),
            content_query(),
            patch_request(serde_json::json!({"slug": "unlocked-slug"})),
        )
        .await
        .unwrap();
    assert_eq!(unlocked.slug.as_deref(), Some("unlocked-slug"));
    assert_eq!(
        fx.core
            .get_work(&fx.principal, fx.work_id.clone())
            .await
            .unwrap()
            .runtime_lock_holder,
        None,
        "the mutator's own lock must be released"
    );
    fx.pool.close().await;
    fx.core.close().await.unwrap();
}

/// Fix-round regression (`database_error` carrier): a real storage fault on the
/// Work-lookup path (`works` table dropped) rides the core lowercase
/// `database_error: …` category that the daemon adapter re-classifies as the
/// legacy `DATABASE_ERROR`.
#[tokio::test]
async fn work_lookup_db_fault_rides_lowercase_carrier() {
    let fx = setup().await;
    sqlx::query("DROP TABLE works")
        .execute(&fx.pool)
        .await
        .unwrap();

    let err = fx
        .core
        .list_chapters(
            &fx.principal,
            fx.work_id.clone(),
            chapters_query(serde_json::json!({})),
        )
        .await
        .expect_err("storage fault must fail the lookup");
    let CoreError::Internal { category } = err else {
        panic!("expected internal carrier, got {err:?}");
    };
    let message = category
        .strip_prefix("database_error: ")
        .expect("lowercase local_db_err carrier");
    assert!(!message.is_empty(), "fault message preserved: {category}");
    fx.pool.close().await;
    fx.core.close().await.unwrap();
}

/// Fix-round addition (chronology core read): `CoreService::work_chronology`
/// resolves a Work by ref slug or id and projects the auto-chronology flag,
/// matching the CLI `chronology show` read semantics.
#[tokio::test]
async fn work_chronology_projects_flag_by_ref_or_id() {
    let fx = setup().await;
    let now = chrono::Utc::now().to_rfc3339();

    // Default state reads false by both ref slug and work_id.
    let by_ref = fx
        .core
        .work_chronology(&fx.principal, "test-novel")
        .await
        .unwrap();
    assert_eq!(by_ref.work_id, fx.work_id);
    assert!(!by_ref.auto_chronology);
    let by_id = fx
        .core
        .work_chronology(&fx.principal, &fx.work_id)
        .await
        .unwrap();
    assert_eq!(by_id, by_ref);

    nexus_local_db::works::set_auto_chronology(&fx.pool, &fx.work_id, true, &now)
        .await
        .unwrap();
    let enabled = fx
        .core
        .work_chronology(&fx.principal, "test-novel")
        .await
        .unwrap();
    assert!(enabled.auto_chronology);

    let err = fx
        .core
        .work_chronology(&fx.principal, "no-such-work")
        .await
        .expect_err("unknown ref must 404");
    let CoreError::NotFound { resource } = err else {
        panic!("expected NotFound, got {err:?}");
    };
    assert_eq!(resource, "work no-such-work");
    fx.pool.close().await;
    fx.core.close().await.unwrap();
}

// ─── Outline canvas + chapter content: retained domain assertions migrated
// from the retired daemon runtime fixtures (`outline_api.rs`,
// `outline_patch.rs`, `chapters_api.rs`). ───────────────────────────────────

/// Current work-level outline revision (the OCC base of every canvas patch).
async fn outline_revision(fx: &Fixture) -> u64 {
    fx.core
        .work_outline(&fx.principal, fx.work_id.clone())
        .await
        .expect("outline read")
        .outline_revision
}

/// The `event_id` of the projected timeline event carrying `title`.
async fn event_id_by_title(fx: &Fixture, title: &str) -> String {
    fx.core
        .work_outline(&fx.principal, fx.work_id.clone())
        .await
        .expect("outline read")
        .timeline_events
        .iter()
        .find(|event| event.title == title)
        .unwrap_or_else(|| panic!("timeline event '{title}'"))
        .event_id
        .clone()
}

/// The structured outline-validation refusal must name the broken rule (a
/// bare variant match would also pass on an unrelated validation failure).
fn assert_outline_validation<T: std::fmt::Debug>(
    context: &str,
    result: Result<T, CoreError>,
    needle: &str,
) {
    match result {
        Err(CoreError::OutlineValidation(error)) => assert!(
            error.errors.iter().any(|message| message.contains(needle)),
            "{context}: the refusal must name '{needle}': {:?}",
            error.errors
        ),
        other => panic!("{context}: expected an outline-validation refusal, got {other:?}"),
    }
}

/// Outline read: an unpatched Work derives its frontmatter from the chapter
/// SSOT — revision 0, one default volume holding every seeded chapter and no
/// timeline / foreshadow / title entries yet.
#[tokio::test]
async fn retained_outline_read_derives_default_frontmatter_from_chapters() {
    let fx = setup().await;

    let outline = fx
        .core
        .work_outline(&fx.principal, fx.work_id.clone())
        .await
        .expect("outline read");
    assert_eq!(
        outline.outline_revision, 0,
        "unpatched outline is revision 0"
    );
    assert_eq!(outline.volumes.len(), 1, "default derivation: one volume");
    assert_eq!(outline.volumes[0].volume_id, NonZeroU64::new(1).unwrap());
    assert_eq!(
        outline.volumes[0].chapter_ids,
        vec![
            NonZeroU64::new(1).unwrap(),
            NonZeroU64::new(2).unwrap(),
            NonZeroU64::new(3).unwrap()
        ],
        "every seeded chapter lands in the default volume"
    );
    assert!(outline.timeline_events.is_empty());
    assert!(outline.foreshadows.is_empty());
    assert!(outline.chapter_titles.is_empty());
    fx.pool.close().await;
    fx.core.close().await.unwrap();
}

/// Canvas authoring round trip: `move_chapter` re-binds the chapter's volume
/// and bumps the revision, and a chapter `set {title, status}` patch lands in
/// the frontmatter / chapter projection the next read serves.
#[tokio::test]
async fn retained_outline_structure_and_chapter_patch_round_trip() {
    let fx = setup().await;

    let moved = fx
        .core
        .patch_outline_structure(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            structure_request(serde_json::json!({
                "work_id": fx.work_id, "base_revision": 0,
                "operation": "move_chapter", "chapter_id": 1, "volume_id": 2
            })),
        )
        .await
        .expect("move chapter");
    assert_eq!(moved.new_revision, NonZeroU64::new(1).unwrap());

    let outline = fx
        .core
        .work_outline(&fx.principal, fx.work_id.clone())
        .await
        .unwrap();
    assert_eq!(outline.outline_revision, 1);
    let target = outline
        .volumes
        .iter()
        .find(|volume| volume.volume_id == NonZeroU64::new(2).unwrap())
        .expect("created Volume 2");
    assert_eq!(target.chapter_ids, vec![NonZeroU64::new(1).unwrap()]);
    // The chapter moved out of Volume 1; its siblings stayed.
    let source = outline
        .volumes
        .iter()
        .find(|volume| volume.volume_id == NonZeroU64::new(1).unwrap())
        .expect("Volume 1 kept for the remaining chapters");
    assert_eq!(
        source.chapter_ids,
        vec![NonZeroU64::new(2).unwrap(), NonZeroU64::new(3).unwrap()]
    );

    let patched = fx
        .core
        .patch_outline_chapter(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            "2".into(),
            chapter_patch_request(serde_json::json!({
                "work_id": fx.work_id, "base_revision": 1, "chapter_id": 2,
                "set": {"title": "Second Scene", "status": "outlined"}
            })),
        )
        .await
        .expect("chapter patch");
    assert_eq!(patched.new_revision, NonZeroU64::new(2).unwrap());

    let outline = fx
        .core
        .work_outline(&fx.principal, fx.work_id.clone())
        .await
        .unwrap();
    assert_eq!(
        outline.chapter_titles.get("2").map(String::as_str),
        Some("Second Scene")
    );
    let detail = fx
        .core
        .chapter_detail(
            &fx.principal,
            fx.work_id.clone(),
            "2".into(),
            content_query(),
        )
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(detail.status).unwrap(),
        serde_json::json!("outlined")
    );
    fx.pool.close().await;
    fx.core.close().await.unwrap();
}

/// Slug rules (B1): a kebab slug passes, an uppercase / spaced / over-long
/// slug and a Work-wide duplicate are the structured `OutlineValidation`
/// refusal, re-asserting a chapter's own slug is not a collision, and a
/// refusal never advances the outline revision.
#[tokio::test]
async fn retained_outline_slug_validation_rules() {
    let fx = setup().await;
    let long_slug = "a".repeat(81);

    for (label, slug, needle) in [
        ("uppercase", "Opening-Scene".to_string(), "kebab-case"),
        ("spaces", "opening scene".to_string(), "kebab-case"),
        ("too long", long_slug, "must be 1..="),
    ] {
        let result = fx
            .core
            .patch_outline_chapter(
                &fx.principal,
                "http",
                fx.work_id.clone(),
                "1".into(),
                chapter_patch_request(serde_json::json!({
                    "work_id": fx.work_id, "base_revision": 0, "chapter_id": 1,
                    "set": {"slug": slug}
                })),
            )
            .await;
        assert_outline_validation(label, result, needle);
    }

    // Work-wide uniqueness: chapter 2 cannot take chapter 1's slug.
    let duplicate = fx
        .core
        .patch_outline_chapter(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            "2".into(),
            chapter_patch_request(serde_json::json!({
                "work_id": fx.work_id, "base_revision": 0, "chapter_id": 2,
                "set": {"slug": "ch01"}
            })),
        )
        .await;
    assert_outline_validation(
        "duplicate slug",
        duplicate,
        "already used by another chapter",
    );
    assert_eq!(outline_revision(&fx).await, 0, "refusals never advance");

    // Re-asserting the chapter's own slug passes (not a collision with itself).
    let unchanged = fx
        .core
        .patch_outline_chapter(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            "1".into(),
            chapter_patch_request(serde_json::json!({
                "work_id": fx.work_id, "base_revision": 0, "chapter_id": 1,
                "set": {"slug": "ch01"}
            })),
        )
        .await
        .expect("re-asserting the same chapter's slug");
    assert_eq!(unchanged.new_revision, NonZeroU64::new(1).unwrap());
    fx.pool.close().await;
    fx.core.close().await.unwrap();
}

/// Volume rules (B2) and the published-chapter structural guard (B4): an
/// existing volume and the immediate next sequential volume are legal move
/// targets, an arbitrary out-of-range volume is refused on both the structure
/// and the chapter patch path, a published chapter cannot move while a draft
/// chapter can, and every refusal leaves the revision untouched.
#[tokio::test]
async fn retained_outline_volume_targets_and_published_structure_guard() {
    let fx = setup().await;

    let out_of_range = fx
        .core
        .patch_outline_structure(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            structure_request(serde_json::json!({
                "work_id": fx.work_id, "base_revision": 0,
                "operation": "attach_to_volume", "chapter_id": 1, "volume_id": 999
            })),
        )
        .await;
    assert_outline_validation(
        "arbitrary structure volume",
        out_of_range,
        "next sequential volume",
    );

    let out_of_range_chapter = fx
        .core
        .patch_outline_chapter(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            "1".into(),
            chapter_patch_request(serde_json::json!({
                "work_id": fx.work_id, "base_revision": 0, "chapter_id": 1,
                "set": {"volume": 999}
            })),
        )
        .await;
    assert_outline_validation(
        "arbitrary chapter volume",
        out_of_range_chapter,
        "next sequential volume",
    );
    assert_eq!(outline_revision(&fx).await, 0, "refusals never advance");

    // An existing volume is a legal target.
    let existing = fx
        .core
        .patch_outline_structure(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            structure_request(serde_json::json!({
                "work_id": fx.work_id, "base_revision": 0,
                "operation": "attach_to_volume", "chapter_id": 1, "volume_id": 1
            })),
        )
        .await
        .expect("attach to the existing volume");
    assert_eq!(existing.new_revision, NonZeroU64::new(1).unwrap());

    // The immediate next sequential volume is the legitimate "create N+1" flow.
    let sequential = fx
        .core
        .patch_outline_structure(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            structure_request(serde_json::json!({
                "work_id": fx.work_id, "base_revision": 1,
                "operation": "move_chapter", "chapter_id": 1, "volume_id": 2
            })),
        )
        .await
        .expect("move to the next sequential volume");
    assert_eq!(sequential.new_revision, NonZeroU64::new(2).unwrap());

    // A published chapter is frozen for structural moves …
    sqlx::query("UPDATE work_chapters SET status = 'published' WHERE work_id = ? AND chapter = 2")
        .bind(&fx.work_id)
        .execute(&fx.pool)
        .await
        .unwrap();
    let blocked = fx
        .core
        .patch_outline_structure(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            structure_request(serde_json::json!({
                "work_id": fx.work_id, "base_revision": 2,
                "operation": "move_chapter", "chapter_id": 2, "volume_id": 3
            })),
        )
        .await;
    assert_outline_validation("published chapter move", blocked, "published chapter 2");
    assert_eq!(
        outline_revision(&fx).await,
        2,
        "the refusal left the revision untouched"
    );

    // … while a draft chapter (no published release yet) still moves.
    sqlx::query("UPDATE work_chapters SET status = 'draft' WHERE work_id = ? AND chapter = 3")
        .bind(&fx.work_id)
        .execute(&fx.pool)
        .await
        .unwrap();
    let draft = fx
        .core
        .patch_outline_structure(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            structure_request(serde_json::json!({
                "work_id": fx.work_id, "base_revision": 2,
                "operation": "move_chapter", "chapter_id": 3, "volume_id": 3
            })),
        )
        .await
        .expect("a draft chapter moves");
    assert_eq!(draft.new_revision, NonZeroU64::new(3).unwrap());
    fx.pool.close().await;
    fx.core.close().await.unwrap();
}

/// Foreshadow temporal order (B3): a source realizing an earlier chapter links,
/// a source realizing a later chapter is refused, and a source with no
/// realization at all cannot be ordered — each refusal is the structured
/// `OutlineValidation` and leaves the revision untouched.
#[tokio::test]
async fn retained_foreshadow_temporal_order_guards() {
    let fx = setup().await;
    let patch = |base_revision: u64, value: serde_json::Value| {
        let mut value = value;
        value["work_id"] = serde_json::json!(fx.work_id.clone());
        value["base_revision"] = serde_json::json!(base_revision);
        timeline_request(value)
    };

    fx.core
        .patch_timeline_event(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            patch(
                0,
                serde_json::json!({"operation": "add_event", "title": "Plant", "realizes_chapter_id": 1}),
            ),
        )
        .await
        .expect("plant event");
    fx.core
        .patch_timeline_event(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            patch(
                1,
                serde_json::json!({"operation": "add_event", "title": "Payoff", "realizes_chapter_id": 3}),
            ),
        )
        .await
        .expect("payoff event");

    // 1 ≤ 3: the edge is orderable and links.
    fx.core
        .patch_timeline_event(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            patch(
                2,
                serde_json::json!({
                    "operation": "link_foreshadow",
                    "event_id": event_id_by_title(&fx, "Plant").await,
                    "foreshadows_event_id": event_id_by_title(&fx, "Payoff").await
                }),
            ),
        )
        .await
        .expect("source-before-target foreshadow links");
    assert_eq!(outline_revision(&fx).await, 3);

    // A source realizing a LATER chapter than its target is a violation.
    fx.core
        .patch_timeline_event(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            patch(3, serde_json::json!({"operation": "add_event", "title": "Late", "realizes_chapter_id": 3})),
        )
        .await
        .expect("late event");
    fx.core
        .patch_timeline_event(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            patch(4, serde_json::json!({"operation": "add_event", "title": "Early", "realizes_chapter_id": 1})),
        )
        .await
        .expect("early event");
    let backwards = fx
        .core
        .patch_timeline_event(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            patch(
                5,
                serde_json::json!({
                    "operation": "link_foreshadow",
                    "event_id": event_id_by_title(&fx, "Late").await,
                    "foreshadows_event_id": event_id_by_title(&fx, "Early").await
                }),
            ),
        )
        .await;
    assert_outline_validation("source after target", backwards, "which is after");
    assert_eq!(outline_revision(&fx).await, 5, "the refusal never advances");

    // An unscheduled source has no realization to order against.
    fx.core
        .patch_timeline_event(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            patch(
                5,
                serde_json::json!({"operation": "add_event", "title": "Unscheduled"}),
            ),
        )
        .await
        .expect("unscheduled event");
    let unordered = fx
        .core
        .patch_timeline_event(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            patch(
                6,
                serde_json::json!({
                    "operation": "link_foreshadow",
                    "event_id": event_id_by_title(&fx, "Unscheduled").await,
                    "foreshadows_event_id": event_id_by_title(&fx, "Early").await
                }),
            ),
        )
        .await;
    assert_outline_validation(
        "unrealized source",
        unordered,
        "requires both source and target events",
    );
    assert_eq!(outline_revision(&fx).await, 6);
    fx.pool.close().await;
    fx.core.close().await.unwrap();
}

/// Outline-prose content patch (V1.75 A2): the per-chapter prose file is
/// replaced, the work-level `outline_revision` CAS bump rides the same write,
/// and the chapter `body_path` column plus the body file bytes stay
/// byte-identical (body ownership is never touched by an outline patch).
#[tokio::test]
async fn retained_outline_content_patch_revision_and_body_ownership() {
    let fx = setup().await;
    let rel_outline = "Works/test-novel/Outlines/chapters/ch01-outline.md";
    let rel_body = "Works/test-novel/Stories/ch01-ch01.md";
    let outline_abs = fx.creative_root.join(rel_outline);
    std::fs::create_dir_all(outline_abs.parent().unwrap()).unwrap();
    std::fs::write(&outline_abs, "# Old outline\n").unwrap();
    let body_abs = fx.creative_root.join(rel_body);
    fx.write_body(
        1,
        "# Chapter body\n\nThe AI owns this prose. It must not change.\n",
    );
    let body_bytes_before = std::fs::read(&body_abs).unwrap();

    let patched = fx
        .core
        .patch_outline_chapter(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            "1".into(),
            chapter_patch_request(serde_json::json!({
                "work_id": fx.work_id, "base_revision": 0, "chapter_id": 1,
                "set": {"content": "## Scene beats\n\n- Open on the harbor"}
            })),
        )
        .await
        .expect("content patch");
    assert_eq!(patched.new_revision, NonZeroU64::new(1).unwrap());
    assert_eq!(outline_revision(&fx).await, 1);

    let on_disk = std::fs::read_to_string(&outline_abs).unwrap();
    assert!(
        on_disk.contains("## Scene beats"),
        "the patched prose is durable: {on_disk}"
    );
    assert!(
        !on_disk.contains("Old outline"),
        "stale prose must be replaced: {on_disk}"
    );

    // Body ownership: the column and the file bytes are untouched.
    let stored_body_path: Option<String> =
        sqlx::query_scalar("SELECT body_path FROM work_chapters WHERE work_id = ? AND chapter = 1")
            .bind(&fx.work_id)
            .fetch_one(&fx.pool)
            .await
            .unwrap();
    assert_eq!(stored_body_path.as_deref(), Some(rel_body));
    assert_eq!(
        std::fs::read(&body_abs).unwrap(),
        body_bytes_before,
        "the body file must be byte-identical after an outline content patch"
    );
    fx.pool.close().await;
    fx.core.close().await.unwrap();
}

/// Chapter protection + lookup: the body projection reports its stored
/// relative path and read-only flag, an unknown chapter / unknown Work keeps
/// the typed not-found, and a finalized chapter needs the explicit structural
/// confirmation before a metadata edit lands.
#[tokio::test]
async fn retained_chapter_protection_lookup_and_body_projection() {
    let fx = setup().await;

    fx.write_body(1, "chapter one body");
    let body = fx
        .core
        .chapter_body(
            &fx.principal,
            fx.work_id.clone(),
            "1".into(),
            content_query(),
        )
        .await
        .expect("body read");
    assert_eq!(body.body_path, "Works/test-novel/Stories/ch01-ch01.md");
    assert_eq!(body.content, "chapter one body");
    assert!(body.read_only, "the body surface is read-only");

    let err = fx
        .core
        .chapter_body(
            &fx.principal,
            fx.work_id.clone(),
            "99".into(),
            content_query(),
        )
        .await
        .expect_err("unknown chapter");
    assert!(matches!(err, CoreError::NotFound { .. }), "{err:?}");

    let err = fx
        .core
        .list_chapters(
            &fx.principal,
            "wrk_unknown".into(),
            chapters_query(serde_json::json!({})),
        )
        .await
        .expect_err("unknown work");
    assert!(matches!(err, CoreError::NotFound { .. }), "{err:?}");

    // The V1.65 transition grammar is one-way: outlined → not_started is refused.
    fx.core
        .patch_chapter(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            "2".into(),
            content_query(),
            patch_request(serde_json::json!({"status": "outlined"})),
        )
        .await
        .expect("not_started → outlined");
    let reverse = fx
        .core
        .patch_chapter(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            "2".into(),
            content_query(),
            patch_request(serde_json::json!({"status": "not_started"})),
        )
        .await
        .expect_err("outlined → not_started must be refused");
    let CoreError::InvalidInput { field, .. } = reverse else {
        panic!("expected the transition carrier, got {reverse:?}");
    };
    assert_eq!(field, "chapter_status_transition_invalid");

    // Finalized chapters are protected until the structural edit is confirmed.
    sqlx::query("UPDATE work_chapters SET status = 'finalized' WHERE work_id = ? AND chapter = 1")
        .bind(&fx.work_id)
        .execute(&fx.pool)
        .await
        .unwrap();
    let err = fx
        .core
        .patch_chapter(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            "1".into(),
            content_query(),
            patch_request(serde_json::json!({"slug": "new-slug"})),
        )
        .await
        .expect_err("finalized edit without confirmation");
    let CoreError::InvalidInput { field, .. } = err else {
        panic!("expected the confirmation carrier, got {err:?}");
    };
    assert_eq!(field, "chapter_structure_confirmation_required");

    let confirmed = fx
        .core
        .patch_chapter(
            &fx.principal,
            "http",
            fx.work_id.clone(),
            "1".into(),
            content_query(),
            patch_request(serde_json::json!({
                "slug": "new-slug", "confirm_structural_edit": true
            })),
        )
        .await
        .expect("confirmed finalized edit");
    assert_eq!(confirmed.slug.as_deref(), Some("new-slug"));
    fx.pool.close().await;
    fx.core.close().await.unwrap();
}
