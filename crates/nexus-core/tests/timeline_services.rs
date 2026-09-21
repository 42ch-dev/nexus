//! P0-T3 `timeline` read projection contract tests (core service
//! semantics). Ports the named retired daemon timeline-events fixture's
//! keyset-cursor behavior plus its malformed-cursor / foreign-world sibling
//! regressions, the W-1 `limit=0` invariant, branch-default resolution, the
//! modules/extensions row mapping, and the overview projection contract
//! (counts, `last_event_at`, pagination, cursor validation) migrated from
//! the retired daemon handler tests.

use nexus_contracts::daemon_api::timeline::ListTimelineEventsResponse;
use nexus_core::{
    CoreAccess, CoreError, CoreOpenOptions, CoreService, CoreTimelineEventsQuery,
    CoreTimelineOverviewQuery, Principal,
};
use nexus_local_db::writer_protocol::{init_engine_pool, GuardedPoolOptions};
use sqlx::SqlitePool;
use tempfile::TempDir;

const CREATOR: &str = "test_creator";
const OTHER: &str = "other_creator";
const SLUG: &str = "default";
const WORLD: &str = "wld_tl";
const FOREIGN: &str = "wld_tl_foreign";
const ROOT: &str = "fbk_root";
const OTHER_BRANCH: &str = "fbk_other";

struct Fixture {
    _tmp: TempDir,
    core: CoreService,
    principal: Principal,
    pool: SqlitePool,
}

async fn fixture() -> Fixture {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path().to_path_buf();
    let nexus_home = home.join(".nexus42");
    std::fs::create_dir_all(&nexus_home).expect("nexus home");
    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
        &home, CREATOR, SLUG,
    ))
    .expect("workspace dir");
    std::fs::write(
        nexus_home.join("config.toml"),
        format!("active_creator_id = \"{CREATOR}\"\n[active_workspace_slug_by_creator]\n\"{CREATOR}\" = \"{SLUG}\"\n"),
    )
    .expect("config.toml");
    let db_path = nexus_home_layout::workspace_state_db_path(&home, CREATOR, SLUG);
    let guarded = init_engine_pool(&db_path, CREATOR, GuardedPoolOptions::default())
        .await
        .expect("engine pool");
    let pool = guarded.clone_pool();
    for creator in [CREATOR, OTHER] {
        sqlx::query(
            "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
             VALUES (?, 'Test', 'active', datetime('now'), '{}')",
        )
        .bind(creator)
        .execute(&pool)
        .await
        .expect("seed creator");
    }
    let core = CoreService::open(CoreOpenOptions {
        user_home: home,
        access: CoreAccess::EngineOwner,
    })
    .await
    .expect("core service");
    let principal = core.active_principal().await.expect("principal");
    Fixture {
        _tmp: tmp,
        core,
        principal,
        pool,
    }
}

async fn seed_world(pool: &SqlitePool, world_id: &str, owner: &str, root_branch: Option<&str>) {
    sqlx::query(
        "INSERT INTO narrative_worlds \
         (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
          time_policy, metadata_json, root_fork_branch_id, created_at) \
         VALUES (?, 'ws', ?, ?, ?, 'active', 'private', 'manual', '{}', ?, datetime('now'))",
    )
    .bind(world_id)
    .bind(owner)
    .bind(format!("Title {world_id}"))
    .bind(format!("slug-{world_id}"))
    .bind(root_branch)
    .execute(pool)
    .await
    .expect("seed world");
}

#[allow(clippy::too_many_arguments)]
async fn seed_event(
    pool: &SqlitePool,
    world_id: &str,
    branch_id: &str,
    event_type: &str,
    status: &str,
    sequence_no: i64,
    extensions_nexus_json: Option<&str>,
    modules_json: Option<&str>,
) {
    sqlx::query(
        "INSERT INTO narrative_timeline_events \
         (timeline_event_id, world_id, branch_id, event_type, status, sequence_no, \
          title, summary, metadata_json, extensions_nexus_json, modules_json, created_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, '{}', ?, ?, '2026-07-31T00:00:00Z')",
    )
    .bind(format!("evt_{world_id}_{branch_id}_{sequence_no}"))
    .bind(world_id)
    .bind(branch_id)
    .bind(event_type)
    .bind(status)
    .bind(sequence_no)
    .bind(format!("Event {sequence_no}"))
    .bind(format!("summary of {sequence_no}"))
    .bind(extensions_nexus_json)
    .bind(modules_json)
    .execute(pool)
    .await
    .expect("seed event");
}

async fn seed_kb_block(
    pool: &SqlitePool,
    world_id: &str,
    name: &str,
    block_type: &str,
    status: &str,
    created_at: &str,
) {
    let id = format!("kb_{world_id}_{name}_{status}");
    sqlx::query(
        "INSERT INTO kb_key_blocks \
         (key_block_id, world_id, block_type, canonical_name, status, created_at) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(world_id)
    .bind(block_type)
    .bind(name)
    .bind(status)
    .bind(created_at)
    .execute(pool)
    .await
    .expect("seed kb block");
}

const fn events_query(
    branch_id: Option<String>,
    status: Option<String>,
    event_type: Option<String>,
    limit: Option<u32>,
    cursor: Option<String>,
) -> CoreTimelineEventsQuery {
    CoreTimelineEventsQuery {
        branch_id,
        status,
        event_type,
        limit,
        cursor,
    }
}

async fn list(
    core: &CoreService,
    principal: &Principal,
    world_id: &str,
    query: CoreTimelineEventsQuery,
) -> ListTimelineEventsResponse {
    core.list_timeline_events(principal, world_id.to_string(), query)
        .await
        .expect("list_timeline_events")
}

fn sequences(resp: &ListTimelineEventsResponse) -> Vec<u64> {
    resp.items.iter().map(|item| item.sequence_no).collect()
}

async fn overview(
    core: &CoreService,
    principal: &Principal,
    cursor: Option<String>,
) -> nexus_contracts::TimelineOverviewResponse {
    core.timeline_overview(principal, CoreTimelineOverviewQuery { cursor })
        .await
        .expect("timeline_overview")
}

/// Port of the retired daemon timeline-events fixture's keyset-cursor case:
/// cursor pages on (`branch_id`, `sequence_no`) come back in deterministic
/// ascending order with no overlap, and re-requesting a cursor reproduces the
/// same page.
#[tokio::test]
async fn cursor_pagination() {
    let f = fixture().await;
    seed_world(&f.pool, WORLD, CREATOR, Some(ROOT)).await;
    for seq in 0..5 {
        seed_event(
            &f.pool,
            WORLD,
            ROOT,
            "story_advance",
            "canon",
            seq,
            None,
            None,
        )
        .await;
    }

    let page1 = list(
        &f.core,
        &f.principal,
        WORLD,
        events_query(None, None, None, Some(2), None),
    )
    .await;
    assert_eq!(sequences(&page1), vec![0, 1]);
    assert!(page1.has_more);
    let cursor1 = page1.next_cursor.clone().expect("page 1 cursor");

    let page2 = list(
        &f.core,
        &f.principal,
        WORLD,
        events_query(None, None, None, Some(2), Some(cursor1.clone())),
    )
    .await;
    assert_eq!(sequences(&page2), vec![2, 3]);
    assert!(page2.has_more);
    let cursor2 = page2.next_cursor.clone().expect("page 2 cursor");
    assert_ne!(cursor1, cursor2);

    let page3 = list(
        &f.core,
        &f.principal,
        WORLD,
        events_query(None, None, None, Some(2), Some(cursor2)),
    )
    .await;
    assert_eq!(sequences(&page3), vec![4]);
    assert!(!page3.has_more);
    assert!(page3.next_cursor.is_none());

    // Deterministic: the same cursor reproduces the same page.
    let again = list(
        &f.core,
        &f.principal,
        WORLD,
        events_query(None, None, None, Some(2), Some(cursor1)),
    )
    .await;
    assert_eq!(sequences(&again), vec![2, 3]);

    f.core.close().await.expect("close");
    f.pool.close().await;
}

#[allow(clippy::too_many_lines)] // one end-to-end timeline scenario
/// Cursor pages preserve the `status` / `event_type` filters: each page is a
/// filtered keyset continuation, never a re-scan of unfiltered rows.
#[tokio::test]
async fn cursor_pages_preserve_filters() {
    let f = fixture().await;
    seed_world(&f.pool, WORLD, CREATOR, Some(ROOT)).await;
    seed_event(
        &f.pool,
        WORLD,
        ROOT,
        "story_advance",
        "canon",
        0,
        None,
        None,
    )
    .await;
    seed_event(
        &f.pool,
        WORLD,
        ROOT,
        "story_advance",
        "provisional",
        1,
        None,
        None,
    )
    .await;
    seed_event(
        &f.pool,
        WORLD,
        ROOT,
        "compute_result",
        "canon",
        2,
        None,
        None,
    )
    .await;
    seed_event(
        &f.pool,
        WORLD,
        ROOT,
        "story_advance",
        "canon",
        3,
        None,
        None,
    )
    .await;
    seed_event(
        &f.pool,
        WORLD,
        ROOT,
        "compute_result",
        "provisional",
        4,
        None,
        None,
    )
    .await;

    // Default status filter is `canon`.
    let canon = list(
        &f.core,
        &f.principal,
        WORLD,
        events_query(None, None, None, None, None),
    )
    .await;
    assert_eq!(sequences(&canon), vec![0, 2, 3]);
    // Explicit provisional filter selects only provisional rows.
    let provisional = list(
        &f.core,
        &f.principal,
        WORLD,
        events_query(None, Some("provisional".into()), None, None, None),
    )
    .await;
    assert_eq!(sequences(&provisional), vec![1, 4]);
    // `event_type` exact match (with the default canon status).
    let compute = list(
        &f.core,
        &f.principal,
        WORLD,
        events_query(None, None, Some("compute_result".into()), None, None),
    )
    .await;
    assert_eq!(sequences(&compute), vec![2]);
    // Unknown event type → empty page, not an error.
    let none = list(
        &f.core,
        &f.principal,
        WORLD,
        events_query(None, None, Some("no_such_type".into()), None, None),
    )
    .await;
    assert!(none.items.is_empty());
    assert!(!none.has_more);

    // Filtered paging: canon pages of two walk 0,2 then 3 — no overlap, no
    // unfiltered rows leaking in.
    let page1 = list(
        &f.core,
        &f.principal,
        WORLD,
        events_query(None, Some("canon".into()), None, Some(2), None),
    )
    .await;
    assert_eq!(sequences(&page1), vec![0, 2]);
    let page2 = list(
        &f.core,
        &f.principal,
        WORLD,
        events_query(
            None,
            Some("canon".into()),
            None,
            Some(2),
            page1.next_cursor.clone(),
        ),
    )
    .await;
    assert_eq!(sequences(&page2), vec![3]);
    assert!(!page2.has_more);
    assert!(page2.next_cursor.is_none());

    // Invalid status filter is rejected.
    let err = f
        .core
        .list_timeline_events(
            &f.principal,
            WORLD.to_string(),
            events_query(None, Some("draft".into()), None, None, None),
        )
        .await
        .expect_err("invalid status");
    assert!(matches!(err, CoreError::InvalidInput { ref field, .. } if field == "status"));

    f.core.close().await.expect("close");
    f.pool.close().await;
}

/// Malformed cursors stay `invalid_input` sibling regressions (garbage,
/// missing separator, empty branch, non-numeric/negative sequence, overlong).
#[tokio::test]
async fn malformed_cursors_are_invalid_input() {
    let f = fixture().await;
    seed_world(&f.pool, WORLD, CREATOR, Some(ROOT)).await;
    seed_event(
        &f.pool,
        WORLD,
        ROOT,
        "story_advance",
        "canon",
        0,
        None,
        None,
    )
    .await;

    let long = format!("ev1:{}", "a".repeat(600));
    for bad in [
        "garbage",
        "ev1:",
        "ev1:noseparator",
        "ev1:fbk_root:notanumber",
        "ev1:fbk_root:-1",
        "ev1::1",
        long.as_str(),
    ] {
        let err = f
            .core
            .list_timeline_events(
                &f.principal,
                WORLD.to_string(),
                events_query(None, None, None, None, Some(bad.to_string())),
            )
            .await
            .expect_err("cursor must be rejected");
        assert!(
            matches!(err, CoreError::InvalidInput { ref field, .. } if field == "cursor"),
            "cursor '{bad}' must be InvalidInput(cursor), got {err:?}"
        );
    }

    f.core.close().await.expect("close");
    f.pool.close().await;
}

/// Foreign-world and missing-world reads keep their distinctions
/// (`Forbidden` / `NotFound`).
#[tokio::test]
async fn foreign_world_forbidden_and_missing_world_not_found() {
    let f = fixture().await;
    seed_world(&f.pool, WORLD, CREATOR, Some(ROOT)).await;
    seed_world(&f.pool, FOREIGN, OTHER, Some(ROOT)).await;

    let err = f
        .core
        .list_timeline_events(
            &f.principal,
            FOREIGN.to_string(),
            events_query(None, None, None, None, None),
        )
        .await
        .expect_err("foreign world");
    assert!(matches!(err, CoreError::WorldOwnerDenied { .. }));

    let err = f
        .core
        .list_timeline_events(
            &f.principal,
            "wld_missing".to_string(),
            events_query(None, None, None, None, None),
        )
        .await
        .expect_err("missing world");
    assert!(matches!(err, CoreError::NotFound { .. }));

    f.core.close().await.expect("close");
    f.pool.close().await;
}

/// W-1 (QC): `limit=0` must not report `has_more` — a `has_more=true,
/// next_cursor=null` response would growth-loop a keyset client.
#[tokio::test]
async fn limit_zero_returns_no_more_pages() {
    let f = fixture().await;
    seed_world(&f.pool, WORLD, CREATOR, Some(ROOT)).await;
    for seq in 0..3 {
        seed_event(
            &f.pool,
            WORLD,
            ROOT,
            "story_advance",
            "canon",
            seq,
            None,
            None,
        )
        .await;
    }

    let page = list(
        &f.core,
        &f.principal,
        WORLD,
        events_query(None, None, None, Some(0), None),
    )
    .await;
    assert!(page.items.is_empty(), "limit=0 must return an empty page");
    assert!(!page.has_more, "limit=0 must not report has_more");
    assert!(page.next_cursor.is_none(), "limit=0 must not continue");

    f.core.close().await.expect("close");
    f.pool.close().await;
}

/// The branch filter defaults to the World's current branch
/// (`root_fork_branch_id`), an explicit `branch_id` narrows to that branch,
/// and an unset root falls back to `fbk_root`.
#[tokio::test]
async fn branch_filter_narrows_and_defaults_to_world_current_branch() {
    let f = fixture().await;
    seed_world(&f.pool, WORLD, CREATOR, Some("fbk_main")).await;
    seed_event(
        &f.pool,
        WORLD,
        "fbk_main",
        "story_advance",
        "canon",
        0,
        None,
        None,
    )
    .await;
    seed_event(
        &f.pool,
        WORLD,
        OTHER_BRANCH,
        "story_advance",
        "canon",
        0,
        None,
        None,
    )
    .await;

    let default_page = list(
        &f.core,
        &f.principal,
        WORLD,
        events_query(None, None, None, None, None),
    )
    .await;
    assert_eq!(sequences(&default_page), vec![0]);
    assert_eq!(default_page.items[0].branch_id, "fbk_main");

    let other = list(
        &f.core,
        &f.principal,
        WORLD,
        events_query(Some(OTHER_BRANCH.to_string()), None, None, None, None),
    )
    .await;
    assert_eq!(sequences(&other), vec![0]);
    assert_eq!(other.items[0].branch_id, OTHER_BRANCH);

    // A world with no recorded root branch falls back to `fbk_root`.
    seed_world(&f.pool, "wld_tl_noroot", CREATOR, None).await;
    seed_event(
        &f.pool,
        "wld_tl_noroot",
        ROOT,
        "story_advance",
        "canon",
        0,
        None,
        None,
    )
    .await;
    let fallback = list(
        &f.core,
        &f.principal,
        "wld_tl_noroot",
        events_query(None, None, None, None, None),
    )
    .await;
    assert_eq!(sequences(&fallback), vec![0]);
    assert_eq!(fallback.items[0].branch_id, ROOT);

    f.core.close().await.expect("close");
    f.pool.close().await;
}

/// `modules_json` / `extensions_nexus_json` are carried verbatim; unrecorded
/// columns degrade to empty map / `None` instead of failing the page.
#[tokio::test]
async fn modules_and_extensions_are_carried_verbatim() {
    let f = fixture().await;
    seed_world(&f.pool, WORLD, CREATOR, Some(ROOT)).await;
    seed_event(
        &f.pool,
        WORLD,
        ROOT,
        "story_advance",
        "canon",
        0,
        None,
        None,
    )
    .await;
    seed_event(
        &f.pool,
        WORLD,
        ROOT,
        "compute_result",
        "canon",
        1,
        Some(r#"{"compute":{"module_id":"basic-combat","module_version":"1.0.0","run_id":"run_1","source_kind":"direct_invoke"}}"#),
        None,
    )
    .await;
    seed_event(
        &f.pool,
        WORLD,
        ROOT,
        "story_advance",
        "canon",
        2,
        None,
        Some(r#"{"observation":{"observers":["kb_char_1","kb_char_2"],"access":{"line_of_sight":true}}}"#),
    )
    .await;

    let page = list(
        &f.core,
        &f.principal,
        WORLD,
        events_query(None, None, None, None, None),
    )
    .await;
    assert_eq!(page.items.len(), 3);

    let plain = &page.items[0];
    assert!(
        plain.modules.is_empty(),
        "unrecorded modules degrade to empty"
    );
    assert!(plain.extensions.is_none());

    let compute = &page.items[1];
    let extensions = compute.extensions.as_ref().expect("compute extensions");
    assert_eq!(extensions["compute"]["module_id"], "basic-combat");
    assert_eq!(extensions["compute"]["run_id"], "run_1");

    let observed = &page.items[2];
    assert_eq!(
        observed.modules["observation"]["observers"],
        serde_json::json!(["kb_char_1", "kb_char_2"])
    );
    assert_eq!(
        observed.modules["observation"]["access"]["line_of_sight"],
        true
    );

    f.core.close().await.expect("close");
    f.pool.close().await;
}

/// Overview: worlds are ordered by `world_id`, pages are bounded at 20, the
/// keyset cursor continues deterministically, and `total_worlds` counts all
/// worlds regardless of paging.
#[tokio::test]
async fn overview_pages_worlds_deterministically() {
    let f = fixture().await;

    let empty = overview(&f.core, &f.principal, None).await;
    assert!(empty.worlds.is_empty());
    assert!(empty.cursor.is_none());
    assert_eq!(empty.total_worlds, 0);

    for i in 0..26 {
        seed_world(&f.pool, &format!("wld_{i:03}"), CREATOR, Some(ROOT)).await;
    }

    let page1 = overview(&f.core, &f.principal, None).await;
    assert_eq!(page1.worlds.len(), 20);
    assert_eq!(page1.worlds[0].world_id, "wld_000");
    assert_eq!(page1.worlds[19].world_id, "wld_019");
    assert!(page1.cursor.is_some(), "page 1 must continue");
    assert_eq!(page1.total_worlds, 26);

    let page2 = overview(&f.core, &f.principal, page1.cursor.clone()).await;
    assert_eq!(page2.worlds.len(), 6);
    assert_eq!(page2.worlds[0].world_id, "wld_020");
    assert_eq!(page2.worlds[5].world_id, "wld_025");
    assert!(page2.cursor.is_none(), "page 2 must be final");
    assert_eq!(page2.total_worlds, 26);

    // Deterministic: page 1 repeats identically.
    let again = overview(&f.core, &f.principal, None).await;
    assert_eq!(again.cursor, page1.cursor);
    assert_eq!(again.worlds[0].world_id, "wld_000");

    f.core.close().await.expect("close");
    f.pool.close().await;
}

/// Overview counts aggregate only live key blocks (`deleted` / `merged` /
/// `deprecated` excluded) and `last_event_at` parses the latest event
/// timestamp.
#[tokio::test]
async fn overview_counts_exclude_retired_blocks_and_parse_last_event() {
    let f = fixture().await;
    seed_world(&f.pool, "wld_a", CREATOR, Some(ROOT)).await;
    seed_world(&f.pool, "wld_b", CREATOR, Some(ROOT)).await;
    seed_world(&f.pool, "wld_c", CREATOR, Some(ROOT)).await;
    seed_kb_block(
        &f.pool,
        "wld_a",
        "the_fall",
        "era",
        "confirmed",
        "2026-06-01T00:00:00Z",
    )
    .await;
    seed_kb_block(
        &f.pool,
        "wld_a",
        "the_war",
        "era",
        "confirmed",
        "2026-06-02T00:00:00Z",
    )
    .await;
    seed_kb_block(
        &f.pool,
        "wld_a",
        "first_blood",
        "event",
        "confirmed",
        "2026-06-03T00:00:00Z",
    )
    .await;
    seed_kb_block(
        &f.pool,
        "wld_a",
        "the_fall",
        "era",
        "deleted",
        "2026-06-05T00:00:00Z",
    )
    .await;
    seed_kb_block(
        &f.pool,
        "wld_b",
        "last_rite",
        "event",
        "confirmed",
        "2026-06-04T00:00:00Z",
    )
    .await;

    let resp = overview(&f.core, &f.principal, None).await;
    assert_eq!(resp.total_worlds, 3);

    let wld_a = resp
        .worlds
        .iter()
        .find(|w| w.world_id == "wld_a")
        .expect("wld_a");
    assert_eq!(wld_a.era_count, 2, "deleted era must be excluded");
    assert_eq!(wld_a.event_count, 1);
    assert_eq!(
        wld_a.last_event_at,
        Some("2026-06-03T00:00:00Z".parse().expect("timestamp"))
    );

    let wld_b = resp
        .worlds
        .iter()
        .find(|w| w.world_id == "wld_b")
        .expect("wld_b");
    assert_eq!(wld_b.era_count, 0);
    assert_eq!(wld_b.event_count, 1);
    assert_eq!(
        wld_b.last_event_at,
        Some("2026-06-04T00:00:00Z".parse().expect("timestamp"))
    );

    let wld_c = resp
        .worlds
        .iter()
        .find(|w| w.world_id == "wld_c")
        .expect("wld_c");
    assert_eq!(wld_c.era_count, 0);
    assert_eq!(wld_c.event_count, 0);
    assert!(wld_c.last_event_at.is_none());

    f.core.close().await.expect("close");
    f.pool.close().await;
}

/// Overview cursor validation: overlong, prefix-only and foreign-format
/// cursors are `invalid_input`.
#[tokio::test]
async fn overview_cursor_errors_are_invalid_input() {
    let f = fixture().await;

    let long = format!("tl:{}", "a".repeat(1000));
    let err = overview_err(&f, Some(long)).await;
    assert!(
        matches!(err, CoreError::InvalidInput { ref field, ref reason } if field == "cursor" && reason == "cursor too long")
    );

    let err = overview_err(&f, Some("tl:".to_string())).await;
    assert!(
        matches!(err, CoreError::InvalidInput { ref field, ref reason } if field == "cursor" && reason == "cursor is empty")
    );

    let err = overview_err(&f, Some("invalid-cursor".to_string())).await;
    assert!(matches!(err, CoreError::InvalidInput { ref field, .. } if field == "cursor"));

    f.core.close().await.expect("close");
    f.pool.close().await;
}

async fn overview_err(f: &Fixture, cursor: Option<String>) -> CoreError {
    f.core
        .timeline_overview(&f.principal, CoreTimelineOverviewQuery { cursor })
        .await
        .expect_err("overview cursor must error")
}
