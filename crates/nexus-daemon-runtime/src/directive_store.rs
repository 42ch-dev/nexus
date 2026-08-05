//! Composition-root [`DirectiveStore`] adapter over `nexus-local-db`,
//! shared by the CLI and the daemon directive route (V1.150 P1, DF-75;
//! relocated V1.151 P0, DF-76).
//!
//! The adapter implements the MCA [`DirectiveStore`] trait consumed by
//! `assemble_moment_with_directive` (scope resolution spec §3.2 +
//! post-injection lifecycle spec §3.3 of
//! `fl-l-w5-prompt-control-plane.md`). It cannot live in `nexus-local-db`
//! (that would create a `nexus-local-db` → MCA dependency cycle); the CLI
//! crate already depends on this crate (`apps/nexus42/Cargo.toml`), so this
//! is the shared composition root.
//!
//! # Product-local only (AC-I3)
//!
//! The directive is NEVER on the spoke wire: not a `modules.*` object, not a
//! `KnowledgeEntry`, never in `AssemblePacket` `placement[]` /
//! `activation_trace[]`, never in any pack export/import path.

use nexus_local_db::moment_directive::{
    clear_on_scene_change, decrement_ttl_by, get_active_for_work, get_active_for_world, get_by_id,
    get_chapter_anchor, update_lifecycle_anchor, upsert_chapter_anchor, MomentDirectiveRow,
};
use nexus_local_db::{get_work, is_novel_profile};
use nexus_moment_context_assembly::directive::{
    ActiveDirective, DirectiveDepth, DirectiveStore, DirectiveTtlKind,
};
use sqlx::SqlitePool;

// ── DirectiveStore adapter (spec §3.2 / §3.3) ─────────────────────────

/// Composition-root [`DirectiveStore`] over `nexus-local-db`, consumed by
/// `assemble_moment_with_directive` at the `platform context assemble-moment`
/// wiring site.
///
/// Cannot live in `nexus-local-db` (dependency cycle with MCA).
#[derive(Debug, Clone)]
pub struct LocalDirectiveStore {
    pool: SqlitePool,
}

impl LocalDirectiveStore {
    /// Create the adapter over a shared pool.
    #[must_use]
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl DirectiveStore for LocalDirectiveStore {
    async fn load_active(
        &self,
        creator_id: Option<&str>,
        work_id: Option<&str>,
        world_id: Option<&str>,
    ) -> Option<ActiveDirective> {
        let creator_id = creator_id?;
        let row = resolve_active_row(&self.pool, creator_id, work_id, world_id).await?;
        map_to_active_directive(row)
    }

    async fn after_injection(
        &self,
        directive_id: &str,
        event_id: Option<&str>,
        work_id: Option<&str>,
    ) {
        after_injection_lifecycle(&self.pool, directive_id, event_id, work_id).await;
    }
}

/// Read-only [`DirectiveStore`] for the inspector route (W-001).
///
/// Resolves + renders the active directive exactly like
/// [`LocalDirectiveStore`] but **never** runs the post-injection lifecycle —
/// [`DirectiveStore::after_injection`] is a no-op. The inspector is an
/// observation surface with a hard "no writes" contract
/// (`api/handlers/inspector.rs`): a poll must not burn TTL, reset the scene
/// anchor (`last_focused_event_id`), or write chapter anchors. The
/// `DirectiveStore` trait already separates `load_active` (read) from
/// `after_injection` (write), so this wrapper is the whole fix — the packet
/// shows the true remaining TTL as persisted at load and inspection never
/// mutates directive state.
#[derive(Debug, Clone)]
pub struct ReadOnlyDirectiveStore {
    pool: SqlitePool,
}

impl ReadOnlyDirectiveStore {
    /// Create the read-only adapter over a shared pool.
    #[must_use]
    pub const fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

impl DirectiveStore for ReadOnlyDirectiveStore {
    async fn load_active(
        &self,
        creator_id: Option<&str>,
        work_id: Option<&str>,
        world_id: Option<&str>,
    ) -> Option<ActiveDirective> {
        let creator_id = creator_id?;
        let row = resolve_active_row(&self.pool, creator_id, work_id, world_id).await?;
        map_to_active_directive(row)
    }

    async fn after_injection(
        &self,
        _directive_id: &str,
        _event_id: Option<&str>,
        _work_id: Option<&str>,
    ) {
        // Read-only: the inspector never advances the directive lifecycle.
    }
}

/// Scope resolution (spec §3.2 — Work wins / World-override fallback):
///
/// 1. If the Work has a Work-scoped directive, use it.
/// 2. Else if the Work's `world_id` has a World-scoped override, use it.
/// 3. Else no directive.
///
/// A World override never leaks across unrelated Worlds or to worldless
/// Works: the Work→World binding is verified against the `works` table, and
/// an unknown Work (binding unverifiable) resolves to no directive.
/// A raw world assembly (no Work context) applies the World override
/// directly to the focused World.
///
/// **Error isolation (QC2-F2):** a **failed** read is "no directive", never a
/// fall-through. Only a *confirmed* result (`Ok(None)`) — no Work directive,
/// or a verified World-bound Work — may fall through to the World override.
/// Otherwise a transient DB error could leak a World override into a Work
/// whose own directive state could not be verified. All DB-error degradation
/// paths warn (QC3-S001); failures degrade to "no directive", never fail the
/// assembly.
async fn resolve_active_row(
    pool: &SqlitePool,
    creator_id: &str,
    work_id: Option<&str>,
    world_id: Option<&str>,
) -> Option<MomentDirectiveRow> {
    if let Some(work_id) = work_id {
        match get_active_for_work(pool, creator_id, work_id).await {
            // Work-wins.
            Ok(Some(row)) => return Some(row),
            // Unverifiable Work-directive state: do NOT fall through to the
            // World override (QC2-F2).
            Err(e) => {
                tracing::warn!(creator_id, work_id, error = %e,
                    "moment directive: work-scoped read failed; resolving to no directive");
                return None;
            }
            // Confirmed no Work directive — the binding check may fall
            // through to the World override.
            Ok(None) => {}
        }
        match get_work(pool, creator_id, work_id).await {
            // Binding verified: a World-bound Work inherits the override.
            Ok(Some(work)) => match work.world_id {
                Some(world_id) => match get_active_for_world(pool, creator_id, &world_id).await {
                    Ok(row) => row,
                    Err(e) => {
                        tracing::warn!(creator_id, work_id, error = %e,
                            "moment directive: world-override read failed; resolving to no directive");
                        None
                    }
                },
                // Confirmed worldless Work — no override can apply.
                None => None,
            },
            // Unknown Work (Ok(None)) or unreadable binding (Err): no override.
            Ok(None) => None,
            Err(e) => {
                tracing::warn!(creator_id, work_id, error = %e,
                    "moment directive: work binding read failed; resolving to no directive");
                None
            }
        }
    } else {
        match get_active_for_world(pool, creator_id, world_id?).await {
            Ok(row) => row,
            Err(e) => {
                tracing::warn!(creator_id, error = %e,
                    "moment directive: world override read failed; resolving to no directive");
                None
            }
        }
    }
}

/// Map a stored row to the MCA payload. A corrupt row (empty body / unknown
/// depth / TTL kind strings) never injects — the adapter skips it with a
/// warning (failures degrade to "no directive", never fail the assembly).
fn map_to_active_directive(row: MomentDirectiveRow) -> Option<ActiveDirective> {
    if row.body.trim().is_empty() {
        // QC2-F5: a corrupt row with an empty body would render nothing yet
        // still count as an injection — skip it, and because `load_active`
        // returns `None` the post-injection TTL decrement never runs.
        tracing::warn!(directive_id = %row.directive_id,
            "moment directive row has an empty body; skipping injection");
        return None;
    }
    let Some(insert_depth) = DirectiveDepth::parse(&row.insert_depth) else {
        tracing::warn!(directive_id = %row.directive_id, depth = %row.insert_depth,
            "moment directive row has unknown insert_depth; skipping injection");
        return None;
    };
    let Some(ttl_kind) = DirectiveTtlKind::parse(&row.ttl_kind) else {
        tracing::warn!(directive_id = %row.directive_id, ttl_kind = %row.ttl_kind,
            "moment directive row has unknown ttl_kind; skipping injection");
        return None;
    };
    Some(ActiveDirective {
        directive_id: row.directive_id,
        body: row.body,
        insert_depth,
        ttl_kind,
        clear_on_scene_change: row.clear_on_scene_change,
        // V1.151 P0 (DF-76 spec §2 H6): carry the persisted metadata through
        // to MCA for the inspector packet — status/metadata only, never the
        // body (AC-I3). `ttl_remaining` is `i64` on the row; only non-
        // negative values surface (an active row's TTL never goes negative).
        // `u64` matches the wire input width (NonZeroU64) so counts above
        // u32::MAX render instead of silently nulling (QC3-S-1).
        ttl_remaining: u64::try_from(row.ttl_remaining).ok(),
        status: row.status,
        scope_kind: row.scope_kind,
        scope_id: row.scope_id,
    })
}

/// Post-injection lifecycle (spec §3.3) — run after a directive was actually
/// injected by `assemble_moment_with_directive`:
///
/// 1. **Scene clear**: when `clear_on_scene_change` is set and the focused
///    moment anchor (`MomentRequest.event_id`) changed between two injecting
///    assembles, soft-delete instead of decrementing. The first injection
///    (no previous anchor) never clears. Documented limitation (guide Q7):
///    no true scene concept exists; `event_id` is the V1.150 proxy.
/// 2. **TTL burn**: `generations` burns 1 on every injecting assemble.
///    `chapters` burns the **delta** of chapter advances since the last
///    injecting assemble **for the same work** — tracked per
///    (directive, work) in `moment_directive_chapter_anchors` so a
///    world-scoped directive burns independently per Work that uses it
///    (R-V1150P2-008); for essay/game-bible/script/worldless Works it
///    behaves identically to `generations` (documented fallback, spec §3.3).
/// 3. **Re-anchor**: store the observed `event_id` (directive row) and
///    chapter (per-work anchor table) so the next assemble can detect change.
///
/// Best-effort: failures are logged, never surfaced as assembly errors.
async fn after_injection_lifecycle(
    pool: &SqlitePool,
    directive_id: &str,
    event_id: Option<&str>,
    work_id: Option<&str>,
) {
    let Ok(Some(row)) = get_by_id(pool, directive_id).await else {
        return;
    };

    // Scene-change clear.
    if row.clear_on_scene_change {
        if let (Some(last), Some(current)) = (row.last_focused_event_id.as_deref(), event_id) {
            if last != current {
                if let Err(e) = clear_on_scene_change(pool, directive_id, now_ms()).await {
                    tracing::warn!(directive_id, error = %e, "moment directive scene-clear failed");
                }
                return;
            }
        }
    }

    // Chapter-advance TTL burn for novel Works with `chapters` TTL. The burn
    // is the chapter delta since this work's last injecting assemble — see
    // `decrement_ttl_by` in `nexus-local-db` for the write-failure threat
    // model (R-V1150P2-005 accepted: atomic via RETURNING; a write failure
    // on local-only SQLite indicates a broken DB, not a lost-update window).
    let (burn, chapter_anchor) = match (row.ttl_kind.as_str(), work_id) {
        ("chapters", Some(work_id)) => {
            match get_work(pool, &row.creator_id, work_id).await {
                Ok(Some(work)) if is_novel_profile(work.work_profile.as_deref()) => {
                    let chapter = i64::from(work.current_chapter);
                    let burn = match get_chapter_anchor(pool, directive_id, work_id).await {
                        // Delta since this work's last injecting assemble;
                        // never negative — a chapter rewind does not refund TTL.
                        Ok(Some(last)) => i64::max(0, chapter - last),
                        // First injecting assemble for this work: observe
                        // only, no burn (R-V1150P2-004).
                        Ok(None) => 0,
                        // Unreadable anchor: degrade like the non-novel
                        // fallback (burn 1, keep the directive moving toward
                        // expiry) — never fail the assembly (QC2-F2).
                        Err(e) => {
                            tracing::warn!(directive_id, work_id, error = %e,
                                "moment directive: chapter anchor read failed; burning 1");
                            1
                        }
                    };
                    (burn, Some((work_id, chapter)))
                }
                // Non-novel / unknown Work: `chapters` behaves like
                // `generations` (documented, not silent — spec §3.3).
                _ => (1, None),
            }
        }
        _ => (1, None),
    };

    // Anchor first, then burn: if the burn write fails, the next assemble
    // re-reads the row and re-attempts; if the directive expires at 0, the
    // anchors are already recorded for DF-76 inspection.
    if let Err(e) = update_lifecycle_anchor(pool, directive_id, event_id, now_ms()).await {
        tracing::warn!(directive_id, error = %e, "moment directive anchor update failed");
    }
    if let Some((work_id, chapter)) = chapter_anchor {
        if let Err(e) = upsert_chapter_anchor(pool, directive_id, work_id, chapter, now_ms()).await
        {
            tracing::warn!(directive_id, work_id, error = %e,
                "moment directive chapter anchor upsert failed");
        }
    }
    if burn > 0 {
        if let Err(e) = decrement_ttl_by(pool, directive_id, burn, now_ms()).await {
            tracing::warn!(directive_id, error = %e, "moment directive TTL decrement failed");
        }
    }
}
/// Unix epoch milliseconds — crate-level helper shared with
/// `api::handlers::directive` (QC3-S-2 dedupe).
pub(crate) fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use nexus_local_db::moment_directive::{scope_kind, set_active, NewMomentDirective};
    use nexus_local_db::{create_work, open_pool, run_migrations, seed_versions, WorkRecord};

    async fn fresh_pool() -> (SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool).await.unwrap();
        seed_versions(&pool).await.unwrap();
        (pool, dir)
    }

    async fn seed_creator(pool: &SqlitePool) {
        // SAFETY: test-only static INSERT with bind params against known schema.
        sqlx::query(
            "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
             VALUES ('ctr_test', 'Test', 'active', datetime('now'), '{}')",
        )
        .execute(pool)
        .await
        .unwrap();
    }

    async fn seed_world(pool: &SqlitePool, world_id: &str) {
        // SAFETY: test-only static INSERT with bind params against known schema.
        sqlx::query(
            "INSERT OR IGNORE INTO narrative_worlds \
             (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, time_policy, metadata_json) \
             VALUES (?, 'wrk_test', 'ctr_test', ?, ?, 'active', 'private', 'manual', '{}')",
        )
        .bind(world_id)
        .bind(world_id)
        .bind(world_id)
        .execute(pool)
        .await
        .unwrap();
    }

    /// Build a `WorkRecord` with sane defaults for the test DB.
    fn work_record(work_id: &str, world_id: Option<&str>, profile: Option<&str>) -> WorkRecord {
        WorkRecord {
            work_id: work_id.to_string(),
            creator_id: "ctr_test".to_string(),
            workspace_slug: "wrk_novel".to_string(),
            status: "active".to_string(),
            title: format!("Work {work_id}"),
            long_term_goal: "Write a novel.".to_string(),
            initial_idea: "An idea.".to_string(),
            creative_brief: None,
            intake_status: "complete".to_string(),
            world_id: world_id.map(str::to_string),
            story_ref: None,
            inspiration_log: "[]".to_string(),
            primary_preset_id: "novel-writing".to_string(),
            schedule_ids: "[]".to_string(),
            created_at: "2026-08-01T00:00:00Z".to_string(),
            updated_at: "2026-08-01T00:00:00Z".to_string(),
            current_stage: "produce".to_string(),
            stage_status: "complete".to_string(),
            work_profile: profile.map(str::to_string),
            work_ref: Some(work_id.to_string()),
            total_planned_chapters: Some(10),
            current_chapter: 1,
            auto_chain_enabled: true,
            driver_schedule_id: None,
            auto_chain_interrupted: false,
            auto_review_master_on_timeout: false,
            runtime_lock_holder: None,
            runtime_lock_acquired_at: None,
            completion_locked_at: None,
            novel_completion_status: None,
            lineage_from_work_id: None,
        }
    }

    async fn seed_work(pool: &SqlitePool, record: &WorkRecord) {
        create_work(pool, record).await.unwrap();
    }

    fn new_params<'a>(
        directive_id: &'a str,
        scope_kind: &'a str,
        scope_id: &'a str,
        ttl_kind: &'a str,
        ttl_remaining: i64,
    ) -> NewMomentDirective<'a> {
        NewMomentDirective {
            directive_id,
            creator_id: "ctr_test",
            scope_kind,
            scope_id,
            body: "Keep the prose terse.",
            insert_depth: "mid",
            ttl_kind,
            ttl_remaining,
            clear_on_scene_change: false,
            now: 1_780_000_000_000,
        }
    }

    // ── T2: scope resolution (spec §3.2) ──────────────────────────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn scope_resolution_work_directive_wins() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool).await;
        seed_world(&pool, "wld_1").await;
        seed_work(&pool, &work_record("wrk_1", Some("wld_1"), Some("novel"))).await;
        set_active(
            &pool,
            &new_params("dir_work", scope_kind::WORK, "wrk_1", "generations", 3),
        )
        .await
        .unwrap();
        set_active(
            &pool,
            &new_params("dir_world", scope_kind::WORLD, "wld_1", "generations", 3),
        )
        .await
        .unwrap();

        let store = LocalDirectiveStore::new(pool);
        let active = store
            .load_active(Some("ctr_test"), Some("wrk_1"), Some("wld_1"))
            .await
            .expect("work directive wins");
        assert_eq!(active.directive_id, "dir_work");
        assert_eq!(active.body, "Keep the prose terse.");
        assert_eq!(active.insert_depth, DirectiveDepth::Mid);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn scope_resolution_world_override_fallback() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool).await;
        seed_world(&pool, "wld_1").await;
        seed_work(&pool, &work_record("wrk_1", Some("wld_1"), Some("novel"))).await;
        set_active(
            &pool,
            &new_params("dir_world", scope_kind::WORLD, "wld_1", "generations", 3),
        )
        .await
        .unwrap();

        let store = LocalDirectiveStore::new(pool);
        let active = store
            .load_active(Some("ctr_test"), Some("wrk_1"), Some("wld_1"))
            .await
            .expect("world override fallback");
        assert_eq!(active.directive_id, "dir_world");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn scope_resolution_no_cross_world_leak() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool).await;
        seed_world(&pool, "wld_1").await;
        seed_world(&pool, "wld_2").await;
        seed_work(&pool, &work_record("wrk_1", Some("wld_2"), Some("novel"))).await;
        set_active(
            &pool,
            &new_params("dir_world", scope_kind::WORLD, "wld_1", "generations", 3),
        )
        .await
        .unwrap();

        let store = LocalDirectiveStore::new(pool);
        let active = store
            .load_active(Some("ctr_test"), Some("wrk_1"), Some("wld_2"))
            .await;
        assert!(
            active.is_none(),
            "World override must never leak across unrelated Worlds"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn scope_resolution_worldless_work_never_sees_world_override() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool).await;
        seed_world(&pool, "wld_1").await;
        seed_work(&pool, &work_record("wrk_1", None, Some("essay"))).await;
        set_active(
            &pool,
            &new_params("dir_world", scope_kind::WORLD, "wld_1", "generations", 3),
        )
        .await
        .unwrap();

        let store = LocalDirectiveStore::new(pool);
        let active = store
            .load_active(Some("ctr_test"), Some("wrk_1"), None)
            .await;
        assert!(
            active.is_none(),
            "worldless Work must never see a World override"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn scope_resolution_no_directive_anywhere() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool).await;
        seed_world(&pool, "wld_1").await;
        seed_work(&pool, &work_record("wrk_1", Some("wld_1"), Some("novel"))).await;

        let store = LocalDirectiveStore::new(pool);
        let active = store
            .load_active(Some("ctr_test"), Some("wrk_1"), Some("wld_1"))
            .await;
        assert!(active.is_none());
        // No creator ⇒ nothing can resolve.
        let active = store.load_active(None, Some("wrk_1"), Some("wld_1")).await;
        assert!(active.is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn scope_resolution_raw_world_assembly_applies_override() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool).await;
        seed_world(&pool, "wld_1").await;
        set_active(
            &pool,
            &new_params("dir_world", scope_kind::WORLD, "wld_1", "generations", 3),
        )
        .await
        .unwrap();

        let store = LocalDirectiveStore::new(pool);
        let active = store
            .load_active(Some("ctr_test"), None, Some("wld_1"))
            .await;
        assert_eq!(active.expect("world directive").directive_id, "dir_world");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn scope_resolution_unknown_work_gets_no_override() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool).await;
        seed_world(&pool, "wld_1").await;
        set_active(
            &pool,
            &new_params("dir_world", scope_kind::WORLD, "wld_1", "generations", 3),
        )
        .await
        .unwrap();

        let store = LocalDirectiveStore::new(pool);
        let active = store
            .load_active(Some("ctr_test"), Some("wrk_ghost"), Some("wld_1"))
            .await;
        assert!(
            active.is_none(),
            "an unverifiable Work binding must not resolve the World override"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn scope_resolution_work_read_error_never_leaks_world_override() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool).await;
        seed_world(&pool, "wld_1").await;
        seed_work(&pool, &work_record("wrk_1", Some("wld_1"), Some("novel"))).await;
        set_active(
            &pool,
            &new_params("dir_world", scope_kind::WORLD, "wld_1", "generations", 3),
        )
        .await
        .unwrap();

        // Simulate a broken Work-directive read: the `moment_directives` table
        // is dropped while `works` stays readable and the World override row is
        // (in principle) present. A fall-through on error would leak the World
        // override into a Work whose own directive state could not be verified;
        // the fixed logic treats any Work-read error as "no directive"
        // (QC2-F2) and `load_active` degrades to `None` instead of erroring.
        // SAFETY: test-only DDL against the scratch pool.
        sqlx::query("DROP TABLE moment_directives")
            .execute(&pool)
            .await
            .unwrap();

        let store = LocalDirectiveStore::new(pool);
        let active = store
            .load_active(Some("ctr_test"), Some("wrk_1"), Some("wld_1"))
            .await;
        assert!(
            active.is_none(),
            "a failed Work-directive read must not leak the World override"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn scope_resolution_empty_body_row_never_injects() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool).await;
        seed_world(&pool, "wld_1").await;
        seed_work(&pool, &work_record("wrk_1", Some("wld_1"), Some("novel"))).await;
        let mut params = new_params("dir_1", scope_kind::WORK, "wrk_1", "generations", 3);
        params.body = "   ";
        set_active(&pool, &params).await.unwrap();

        let store = LocalDirectiveStore::new(pool);
        let active = store
            .load_active(Some("ctr_test"), Some("wrk_1"), Some("wld_1"))
            .await;
        assert!(
            active.is_none(),
            "a corrupt empty-body row must not inject (QC2-F5)"
        );
    }

    // ── T4: post-injection lifecycle (spec §3.3) ──────────────────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn lifecycle_generations_decrements_each_assemble_and_expires() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool).await;
        seed_world(&pool, "wld_1").await;
        seed_work(&pool, &work_record("wrk_1", Some("wld_1"), Some("novel"))).await;
        set_active(
            &pool,
            &new_params("dir_1", scope_kind::WORK, "wrk_1", "generations", 2),
        )
        .await
        .unwrap();

        let store = LocalDirectiveStore::new(pool.clone());
        store
            .after_injection("dir_1", Some("evt_a"), Some("wrk_1"))
            .await;
        let row = get_by_id(&pool, "dir_1").await.unwrap().unwrap();
        assert_eq!(row.ttl_remaining, 1);
        assert_eq!(row.status, "active");
        assert_eq!(row.last_focused_event_id.as_deref(), Some("evt_a"));

        store
            .after_injection("dir_1", Some("evt_a"), Some("wrk_1"))
            .await;
        let row = get_by_id(&pool, "dir_1").await.unwrap().unwrap();
        assert_eq!(row.ttl_remaining, 0);
        assert_eq!(row.status, "expired", "TTL-0 ⇒ soft-delete");
        assert!(row.expires_at.is_some());
        assert!(
            get_active_for_work(&pool, "ctr_test", "wrk_1")
                .await
                .unwrap()
                .is_none(),
            "expired rows no longer inject"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn lifecycle_chapters_novel_decrements_on_advance_only() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool).await;
        seed_world(&pool, "wld_1").await;
        seed_work(&pool, &work_record("wrk_1", Some("wld_1"), Some("novel"))).await;
        set_active(
            &pool,
            &new_params("dir_1", scope_kind::WORK, "wrk_1", "chapters", 3),
        )
        .await
        .unwrap();

        let store = LocalDirectiveStore::new(pool.clone());
        // First injection: no previously observed chapter for this work ⇒ no burn.
        store.after_injection("dir_1", None, Some("wrk_1")).await;
        let row = get_by_id(&pool, "dir_1").await.unwrap().unwrap();
        assert_eq!(row.ttl_remaining, 3);
        assert_eq!(
            get_chapter_anchor(&pool, "dir_1", "wrk_1").await.unwrap(),
            Some(1),
            "first injection records the per-work chapter anchor"
        );

        // Same chapter again ⇒ still no burn.
        store.after_injection("dir_1", None, Some("wrk_1")).await;
        let row = get_by_id(&pool, "dir_1").await.unwrap().unwrap();
        assert_eq!(row.ttl_remaining, 3);

        // Chapter advances (workflow path bumps `current_chapter`) ⇒ burn 1.
        // SAFETY: test-only UPDATE with bind params against known schema.
        sqlx::query("UPDATE works SET current_chapter = 2 WHERE work_id = 'wrk_1'")
            .execute(&pool)
            .await
            .unwrap();

        store.after_injection("dir_1", None, Some("wrk_1")).await;
        let row = get_by_id(&pool, "dir_1").await.unwrap().unwrap();
        assert_eq!(
            row.ttl_remaining, 2,
            "chapter advance decrements chapters TTL"
        );
        assert_eq!(
            get_chapter_anchor(&pool, "dir_1", "wrk_1").await.unwrap(),
            Some(2)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn lifecycle_chapters_multi_advance_burns_delta_and_caps() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool).await;
        seed_world(&pool, "wld_1").await;
        seed_work(&pool, &work_record("wrk_1", Some("wld_1"), Some("novel"))).await;
        set_active(
            &pool,
            &new_params("dir_1", scope_kind::WORK, "wrk_1", "chapters", 5),
        )
        .await
        .unwrap();

        let store = LocalDirectiveStore::new(pool.clone());
        // Observe chapter 1 (no burn).
        store.after_injection("dir_1", None, Some("wrk_1")).await;
        assert_eq!(
            get_by_id(&pool, "dir_1")
                .await
                .unwrap()
                .unwrap()
                .ttl_remaining,
            5
        );

        // 3 chapter advances between assembles ⇒ 3 burns (R-V1150P2-004).
        // SAFETY: test-only UPDATE with bind params against known schema.
        sqlx::query("UPDATE works SET current_chapter = 4 WHERE work_id = 'wrk_1'")
            .execute(&pool)
            .await
            .unwrap();
        store.after_injection("dir_1", None, Some("wrk_1")).await;
        let row = get_by_id(&pool, "dir_1").await.unwrap().unwrap();
        assert_eq!(
            row.ttl_remaining, 2,
            "multi-advance between assembles burns the full delta, got {}",
            row.ttl_remaining
        );
        assert_eq!(row.status, "active");

        // 5 more advances but only 2 TTL left ⇒ capped at 0 and expires.
        // SAFETY: test-only UPDATE with bind params against known schema.
        sqlx::query("UPDATE works SET current_chapter = 9 WHERE work_id = 'wrk_1'")
            .execute(&pool)
            .await
            .unwrap();
        store.after_injection("dir_1", None, Some("wrk_1")).await;
        let row = get_by_id(&pool, "dir_1").await.unwrap().unwrap();
        assert_eq!(row.ttl_remaining, 0, "burn is bounded at remaining TTL");
        assert_eq!(row.status, "expired", "TTL-0 ⇒ soft-delete");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn lifecycle_chapters_world_scoped_burns_per_work() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool).await;
        seed_world(&pool, "wld_1").await;
        // Two novel Works sharing the world (R-V1150P2-008 cross-work case).
        seed_work(&pool, &work_record("wrk_a", Some("wld_1"), Some("novel"))).await;
        seed_work(&pool, &work_record("wrk_b", Some("wld_1"), Some("novel"))).await;
        set_active(
            &pool,
            &new_params("dir_w", scope_kind::WORLD, "wld_1", "chapters", 5),
        )
        .await
        .unwrap();

        let store = LocalDirectiveStore::new(pool.clone());
        // Both works' first injections observe only (no burn, no cross-work).
        store.after_injection("dir_w", None, Some("wrk_a")).await;
        store.after_injection("dir_w", None, Some("wrk_b")).await;
        let row = get_by_id(&pool, "dir_w").await.unwrap().unwrap();
        assert_eq!(row.ttl_remaining, 5, "first injections never burn");
        assert_eq!(
            get_chapter_anchor(&pool, "dir_w", "wrk_a").await.unwrap(),
            Some(1)
        );
        assert_eq!(
            get_chapter_anchor(&pool, "dir_w", "wrk_b").await.unwrap(),
            Some(1),
            "each work holds its own anchor"
        );

        // wrk_b advances 1 chapter ⇒ burns 1 for wrk_b only.
        // SAFETY: test-only UPDATE with bind params against known schema.
        sqlx::query("UPDATE works SET current_chapter = 2 WHERE work_id = 'wrk_b'")
            .execute(&pool)
            .await
            .unwrap();
        store.after_injection("dir_w", None, Some("wrk_b")).await;
        let row = get_by_id(&pool, "dir_w").await.unwrap().unwrap();
        assert_eq!(row.ttl_remaining, 4, "wrk_b's advance burns 1");

        // wrk_a has not advanced ⇒ its assemble burns nothing.
        store.after_injection("dir_w", None, Some("wrk_a")).await;
        let row = get_by_id(&pool, "dir_w").await.unwrap().unwrap();
        assert_eq!(
            row.ttl_remaining, 4,
            "assembling wrk_a must not burn wrk_b's share of the TTL"
        );

        // wrk_a advances 2 chapters ⇒ burns 2 (its own delta).
        // SAFETY: test-only UPDATE with bind params against known schema.
        sqlx::query("UPDATE works SET current_chapter = 3 WHERE work_id = 'wrk_a'")
            .execute(&pool)
            .await
            .unwrap();
        store.after_injection("dir_w", None, Some("wrk_a")).await;
        let row = get_by_id(&pool, "dir_w").await.unwrap().unwrap();
        assert_eq!(row.ttl_remaining, 2, "wrk_a burns its own 2-advance delta");
        assert_eq!(row.status, "active");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn lifecycle_chapters_non_novel_falls_back_to_generations() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool).await;
        seed_world(&pool, "wld_1").await;
        seed_work(&pool, &work_record("wrk_1", Some("wld_1"), Some("essay"))).await;
        set_active(
            &pool,
            &new_params("dir_1", scope_kind::WORK, "wrk_1", "chapters", 2),
        )
        .await
        .unwrap();

        let store = LocalDirectiveStore::new(pool.clone());
        store.after_injection("dir_1", None, Some("wrk_1")).await;
        store.after_injection("dir_1", None, Some("wrk_1")).await;
        let row = get_by_id(&pool, "dir_1").await.unwrap().unwrap();
        assert_eq!(
            row.status, "expired",
            "essay Work: chapters behaves like generations"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn lifecycle_scene_change_clears_same_anchor_decrements() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool).await;
        seed_world(&pool, "wld_1").await;
        seed_work(&pool, &work_record("wrk_1", Some("wld_1"), Some("novel"))).await;
        let mut params = new_params("dir_1", scope_kind::WORK, "wrk_1", "generations", 3);
        params.clear_on_scene_change = true;
        set_active(&pool, &params).await.unwrap();

        let store = LocalDirectiveStore::new(pool.clone());
        // Same anchor ⇒ decrements (and re-anchors).
        store
            .after_injection("dir_1", Some("evt_a"), Some("wrk_1"))
            .await;
        store
            .after_injection("dir_1", Some("evt_a"), Some("wrk_1"))
            .await;
        let row = get_by_id(&pool, "dir_1").await.unwrap().unwrap();
        assert_eq!(row.ttl_remaining, 1);
        assert_eq!(row.status, "active");

        // Anchor changes between injecting assembles ⇒ clear instead of decrement.
        store
            .after_injection("dir_1", Some("evt_b"), Some("wrk_1"))
            .await;
        let row = get_by_id(&pool, "dir_1").await.unwrap().unwrap();
        assert_eq!(row.status, "expired", "scene change clears the directive");
        assert_eq!(row.ttl_remaining, 1, "no decrement on scene-clear");
        assert!(row.expires_at.is_some());
        assert!(get_active_for_work(&pool, "ctr_test", "wrk_1")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn lifecycle_first_injection_never_scene_clears() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool).await;
        seed_world(&pool, "wld_1").await;
        seed_work(&pool, &work_record("wrk_1", Some("wld_1"), Some("novel"))).await;
        let mut params = new_params("dir_1", scope_kind::WORK, "wrk_1", "generations", 3);
        params.clear_on_scene_change = true;
        set_active(&pool, &params).await.unwrap();

        let store = LocalDirectiveStore::new(pool.clone());
        store
            .after_injection("dir_1", Some("evt_a"), Some("wrk_1"))
            .await;
        let row = get_by_id(&pool, "dir_1").await.unwrap().unwrap();
        assert_eq!(
            row.status, "active",
            "first injection has no previous anchor to compare"
        );
        assert_eq!(
            row.ttl_remaining, 2,
            "first injection decrements like a generation"
        );
        assert_eq!(row.last_focused_event_id.as_deref(), Some("evt_a"));
    }
}
