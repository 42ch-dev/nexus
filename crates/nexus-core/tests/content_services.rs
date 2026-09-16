//! Chapter/manuscript content and outline/chronology services on guarded
//! storage. The named selector `get_body_rejects_escaped_body_path` carries
//! AC-P1-T2: traversal/symlink paths outside the Work are denied before any
//! read/write, and a denied published-chapter mutation leaves content intact.

use std::num::NonZeroU64;
use std::path::PathBuf;

use nexus_contracts::{
    ChapterContentQuery, CreateWorkRequest, CreateWorldRequest, ListChaptersQuery,
    OutlinePatchChapterRequest, OutlinePatchStructureRequest, PatchChapterRequest,
    TimelinePatchEventRequest,
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
    fn body_rel(&self, chapter: u32) -> String {
        format!("Works/test-novel/Stories/ch{chapter:02}-ch{chapter:02}.md")
    }

    fn write_body(&self, chapter: u32, content: &str) {
        let path = self.creative_root.join(self.body_rel(chapter));
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
        .bind(fx.body_rel(1))
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

/// Fix-round regression (lock-holder label): a lock acquired through the
/// daemon-holder content route reports the legacy `cli:http:<uuid>` holder
/// verbatim in the observable 423 `Locked.reason` (`work … is locked by
/// 'cli:http:…'`).
#[tokio::test]
async fn locked_work_reports_http_holder_in_reason() {
    let fx = setup().await;
    let holder = nexus_local_db::cli_holder("http");
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
    let reason = resource
        .strip_prefix("work_locked:")
        .expect("locked resource prefix");
    let marker = "'cli:http:";
    let start = reason
        .find(marker)
        .expect("legacy cli:http holder in reason");
    let holder_tail = &reason[start + marker.len()..];
    let uuid = holder_tail.split('\'').next().expect("closing quote");
    assert_eq!(uuid.len(), 36, "holder uuid shape: {reason}");
    assert!(
        uuid.chars().filter(|c| *c == '-').count() == 4,
        "uuid dashes"
    );
    assert!(
        !reason.contains("cli:core:"),
        "core label must not leak into the HTTP surface: {reason}"
    );

    // The route's own holder also releases cleanly (label round-trips).
    let released = nexus_local_db::release_runtime_lock(
        &fx.pool,
        fx.principal.creator_id(),
        &fx.work_id,
        &holder,
    )
    .await
    .unwrap();
    assert!(released);
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
