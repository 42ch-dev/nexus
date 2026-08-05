//! Moment Directive author surface (V1.150 P1, DF-75) — `creator
//! moment-directive set|show|clear`.
//!
//! CLI-only author surface per the Q3 lock (spec `fl-l-w5-prompt-control-plane.md`
//! §1.2 / §3): a short author-written instruction injected by MCA into the
//! reserved `moment.directive` slot (above lore, below system/personality).
//! Persistence is `nexus-local-db` (`moment_directives` table); observation
//! is the existing `platform context assemble-moment` output.
//!
//! The [`LocalDirectiveStore`] composition-root [`DirectiveStore`] adapter
//! (relocated V1.151 P0, DF-76) lives in `nexus-daemon-runtime`
//! (`directive_store`), shared by this CLI surface and the daemon directive
//! route. It cannot live in `nexus-local-db` (that would create a
//! `nexus-local-db` → MCA dependency cycle), and `nexus42` already depends
//! on `nexus-daemon-runtime`.
//!
//! # Product-local only (AC-I3)
//!
//! The directive is NEVER on the spoke wire: not a `modules.*` object, not a
//! `KnowledgeEntry`, never in `AssemblePacket` `placement[]` /
//! `activation_trace[]`, never in any pack export/import path.

use clap::{Args, Subcommand};
use sqlx::SqlitePool;

use crate::config::CliConfig;
use crate::errors::{CliError, Result};
use nexus_local_db::moment_directive::{
    clear, get_active_for_work, get_active_for_world, replace_active, scope_kind, set_active,
    MomentDirectiveRow, NewMomentDirective,
};
use nexus_local_db::{get_work, list_works, WorkListFilters};
use nexus_moment_context_assembly::directive::DirectiveDepth;

/// `creator moment-directive` subcommands (V1.150 P1, DF-75).
#[derive(Debug, Subcommand)]
pub enum MomentDirectiveCommand {
    /// Set (or `--replace`) the active Moment Directive for a scope
    ///
    /// Writes a Work-scoped directive by default; `--world` writes the World
    /// override for the Work's bound World. An already-active directive in
    /// the same scope requires `--replace` (no silent overwrite, spec §3.1).
    Set(MomentDirectiveSetArgs),

    /// Show the active Moment Directive for a scope
    Show(MomentDirectiveScopeArgs),

    /// Clear (soft-delete) the active Moment Directive for a scope
    ///
    /// Expires the row immediately (`status='expired'`, `expires_at` set);
    /// the row is retained for DF-76 inspection, not hard-deleted.
    Clear(MomentDirectiveScopeArgs),
}

/// `creator moment-directive set` arguments.
#[derive(Debug, Args)]
pub struct MomentDirectiveSetArgs {
    /// Author instruction text (non-empty after trimming whitespace)
    #[arg(long)]
    pub body: String,

    /// Insert depth within the directive region: `head` (nearest system),
    /// `mid`, `tail` (nearest lore)
    #[arg(long, value_parser = parse_depth)]
    pub depth: DirectiveDepth,

    /// TTL in generations — count-down by 1 on every injecting assemble.
    /// Exactly one TTL kind is required.
    #[arg(long, conflicts_with = "ttl_chapters")]
    pub ttl_generations: Option<i64>,

    /// TTL in chapters — count-down by the number of chapter advances since
    /// the last injecting assemble (novel Works; R-V1150P2-004/R-V1150P2-008:
    /// per-(directive, work) delta) or per injecting assemble
    /// (essay/game-bible/script/worldless Works). Exactly one TTL kind is
    /// required.
    #[arg(long, conflicts_with = "ttl_generations")]
    pub ttl_chapters: Option<i64>,

    /// Clear when the focused moment anchor (`event_id`) changes between
    /// injecting assembles (scene-change proxy; guide Q7)
    #[arg(long)]
    pub clear_on_scene_change: bool,

    /// Work id for the Work scope (default: the active Work)
    #[arg(long)]
    pub work: Option<String>,

    /// Write a World-scoped override for the Work's bound World instead
    #[arg(long)]
    pub world: bool,

    /// Supersede an already-active directive in the same scope (old row is
    /// soft-deleted with `replaced_by` set to the new id)
    #[arg(long)]
    pub replace: bool,
}

/// `creator moment-directive show|clear` scope selection.
#[derive(Debug, Args)]
pub struct MomentDirectiveScopeArgs {
    /// Work id for the Work scope (default: the active Work)
    #[arg(long, conflicts_with = "world")]
    pub work: Option<String>,

    /// World id for the World scope
    #[arg(long, conflicts_with = "work")]
    pub world: Option<String>,
}

/// Parse `--depth` into a [`DirectiveDepth`].
fn parse_depth(value: &str) -> std::result::Result<DirectiveDepth, String> {
    DirectiveDepth::parse(value)
        .ok_or_else(|| format!("unknown insert depth {value:?} (expected head | mid | tail)"))
}

/// Run the `creator moment-directive` command against the local `state.db`.
///
/// # Errors
///
/// Returns `CliError` if no creator is active, the DB cannot be opened, or a
/// validation / persistence error occurs.
pub async fn run(command: MomentDirectiveCommand, config: &CliConfig) -> Result<()> {
    let creator_id = config
        .active_creator_id
        .as_deref()
        .ok_or(CliError::CreatorNotSelected)?;
    let db_path = crate::config::resolve_state_db_path(config)?;
    let pool = crate::db::Schema::init(&db_path).await?;
    let workspace_slug = config.workspace_slug_for_creator(creator_id);
    match command {
        MomentDirectiveCommand::Set(args) => {
            handle_set(&pool, creator_id, workspace_slug, &args).await
        }
        MomentDirectiveCommand::Show(args) => {
            handle_show(&pool, creator_id, workspace_slug, &args).await
        }
        MomentDirectiveCommand::Clear(args) => {
            handle_clear(&pool, creator_id, workspace_slug, &args).await
        }
    }
}

/// `creator moment-directive set` handler (spec §3.1 / §3.3).
async fn handle_set(
    pool: &SqlitePool,
    creator_id: &str,
    workspace_slug: &str,
    args: &MomentDirectiveSetArgs,
) -> Result<()> {
    // ── Validation (spec §3.1 / §3.3 "Write") ──────────────────────────
    let body = args.body.trim();
    if body.is_empty() {
        return Err(CliError::Config(
            "--body must be non-empty (after trimming whitespace)".to_string(),
        ));
    }
    let (ttl_kind, ttl_remaining) = match (args.ttl_generations, args.ttl_chapters) {
        (None, None) => {
            return Err(CliError::Config(
                "exactly one of --ttl-generations / --ttl-chapters is required".to_string(),
            ));
        }
        (Some(_), Some(_)) => {
            return Err(CliError::Config(
                "--ttl-generations and --ttl-chapters are mutually exclusive".to_string(),
            ));
        }
        (Some(n), None) if n >= 1 => ("generations", n),
        (None, Some(n)) if n >= 1 => ("chapters", n),
        _ => {
            return Err(CliError::Config(
                "TTL count must be a positive integer (>= 1)".to_string(),
            ));
        }
    };

    // ── Scope resolution (spec §3.2) ───────────────────────────────────
    // Both branches resolve the Work first (explicit `--work` or active):
    // the Work-scoped directive targets the Work itself, the World override
    // targets the Work's bound World.
    let work = resolve_work(pool, creator_id, workspace_slug, args.work.as_deref()).await?;
    let (scope_kind, scope_id, scope_label) = if args.world {
        let world_id = work.world_id.clone().ok_or_else(|| {
            CliError::Config(format!(
                "Work {} is not bound to a World; a World-scoped Moment Directive needs a World. \
                 Set a Work-scoped directive instead.",
                work.work_id
            ))
        })?;
        let scope_label = format!("world {world_id}");
        (scope_kind::WORLD, world_id, scope_label)
    } else {
        (
            scope_kind::WORK,
            work.work_id.clone(),
            format!("work {}", work.work_id),
        )
    };

    let new = NewMomentDirective {
        directive_id: &generate_directive_id(),
        creator_id,
        scope_kind,
        scope_id: &scope_id,
        body,
        insert_depth: args.depth.as_str(),
        ttl_kind,
        ttl_remaining,
        clear_on_scene_change: args.clear_on_scene_change,
        now: now_ms(),
    };

    let inserted = if args.replace {
        replace_active(pool, &new).await?
    } else {
        match set_active(pool, &new).await {
            Ok(row) => row,
            Err(nexus_local_db::LocalDbError::Sqlx(sqlx::Error::Database(db_err)))
                if db_err.is_unique_violation() =>
            {
                return Err(CliError::Config(
                    "A Moment Directive is already active for this scope. \
                     Pass --replace to supersede it (the old directive is retained with \
                     `replaced_by` set to the new id)."
                        .to_string(),
                ));
            }
            Err(e) => return Err(e.into()),
        }
    };

    println!("✓ Moment Directive set for {scope_label}");
    println!("  id: {}", inserted.directive_id);
    println!("  depth: {}", inserted.insert_depth);
    println!("  ttl: {} {}", inserted.ttl_remaining, inserted.ttl_kind);
    if inserted.clear_on_scene_change {
        println!("  clear_on_scene_change: yes");
    }
    Ok(())
}

/// `creator moment-directive show` handler — displays the **effective**
/// directive for the requested scope (spec §3.2, QC2-F8): for a Work the
/// author sees the directive that actually injects (the Work's own, or the
/// inherited World override), with the source scope called out explicitly.
async fn handle_show(
    pool: &SqlitePool,
    creator_id: &str,
    workspace_slug: &str,
    args: &MomentDirectiveScopeArgs,
) -> Result<()> {
    let Some((row, effective_for)) =
        resolve_effective_for_show(pool, creator_id, workspace_slug, args).await?
    else {
        println!("No active Moment Directive for this scope.");
        return Ok(());
    };
    println!("Directive: {}", row.directive_id);
    println!("Scope: {} {}", row.scope_kind, row.scope_id);
    println!("Effective for: {effective_for}");
    println!("Depth: {}", row.insert_depth);
    println!("TTL: {} remaining ({})", row.ttl_remaining, row.ttl_kind);
    if row.clear_on_scene_change {
        println!("Clear on scene change: yes");
    }
    println!("Body:");
    println!("{}", row.body);
    Ok(())
}

/// `creator moment-directive clear` handler — soft-delete the active row.
async fn handle_clear(
    pool: &SqlitePool,
    creator_id: &str,
    workspace_slug: &str,
    args: &MomentDirectiveScopeArgs,
) -> Result<()> {
    let (scope_kind, scope_id) = resolve_scope_ids(pool, creator_id, workspace_slug, args).await?;
    let cleared = clear(pool, creator_id, scope_kind, &scope_id, now_ms()).await?;
    if cleared {
        println!("✓ Moment Directive cleared (soft-deleted) for {scope_kind} {scope_id}.");
    } else {
        println!("No active Moment Directive to clear for {scope_kind} {scope_id}.");
    }
    Ok(())
}

/// Resolve the **effective** directive for `show` (spec §3.2, Work-wins /
/// World-override, QC2-F8) together with a human label of the scope it came
/// from:
///
/// - `--world <id>`: the World override itself (it is what a raw world
///   assembly would inject).
/// - Work selection: the Work's own directive wins; otherwise the bound
///   World's override is inherited (reported as such). No directive when the
///   Work is worldless/unbound and has no own directive.
async fn resolve_effective_for_show(
    pool: &SqlitePool,
    creator_id: &str,
    workspace_slug: &str,
    args: &MomentDirectiveScopeArgs,
) -> Result<Option<(MomentDirectiveRow, String)>> {
    if let Some(world_id) = args.world.as_deref() {
        let row = get_active_for_world(pool, creator_id, world_id).await?;
        return Ok(row.map(|r| (r, format!("world {world_id}"))));
    }
    let work = resolve_work(pool, creator_id, workspace_slug, args.work.as_deref()).await?;
    if let Some(row) = get_active_for_work(pool, creator_id, &work.work_id).await? {
        return Ok(Some((
            row,
            format!("work {} (own directive)", work.work_id),
        )));
    }
    if let Some(world_id) = work.world_id {
        if let Some(row) = get_active_for_world(pool, creator_id, &world_id).await? {
            return Ok(Some((
                row,
                format!("work {} (inherited from world {world_id})", work.work_id),
            )));
        }
    }
    Ok(None)
}

/// Resolve the `(scope_kind, scope_id)` for a `show`/`clear` selection:
/// `--world <id>` selects the World scope directly; otherwise the Work scope
/// (explicit `--work <id>` or the active Work).
async fn resolve_scope_ids(
    pool: &SqlitePool,
    creator_id: &str,
    workspace_slug: &str,
    args: &MomentDirectiveScopeArgs,
) -> Result<(&'static str, String)> {
    if let Some(world_id) = args.world.as_deref() {
        return Ok((scope_kind::WORLD, world_id.to_string()));
    }
    let work = resolve_work(pool, creator_id, workspace_slug, args.work.as_deref()).await?;
    Ok((scope_kind::WORK, work.work_id))
}

/// Resolve a Work: explicit `--work <id>` (verified against the DB) or the
/// active Work (most recently updated `status='active'` row, mirroring the
/// daemon's `works?limit=1&status=active` resolution).
async fn resolve_work(
    pool: &SqlitePool,
    creator_id: &str,
    workspace_slug: &str,
    explicit: Option<&str>,
) -> Result<nexus_local_db::WorkRecord> {
    let work_id = if let Some(id) = explicit {
        id.to_string()
    } else {
        let filters = WorkListFilters {
            status: Some("active".to_string()),
            limit: Some(1),
            ..WorkListFilters::default()
        };
        // `list_works` returns most-recently-updated first.
        let works = list_works(pool, creator_id, workspace_slug, &filters).await?;
        works.first().map(|w| w.work_id.clone()).ok_or_else(|| {
            CliError::Config(
                "No active Work found. Pass --work <id> or activate a Work first \
                     (`nexus42 creator works use <work_id>`)."
                    .to_string(),
            )
        })?
    };
    get_work(pool, creator_id, &work_id).await?.ok_or_else(|| {
        CliError::Config(format!(
            "Work {work_id} not found for creator {creator_id}."
        ))
    })
}

/// Generate a stable directive id (`dir_<uuid v4>`).
fn generate_directive_id() -> String {
    format!("dir_{}", uuid::Uuid::new_v4())
}

/// Unix epoch milliseconds.
fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}


#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use nexus_daemon_runtime::directive_store::LocalDirectiveStore;
    use nexus_local_db::moment_directive::get_by_id;
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

    fn set_args(
        body: &str,
        depth: DirectiveDepth,
        ttl_generations: Option<i64>,
        ttl_chapters: Option<i64>,
    ) -> MomentDirectiveSetArgs {
        MomentDirectiveSetArgs {
            body: body.to_string(),
            depth,
            ttl_generations,
            ttl_chapters,
            clear_on_scene_change: false,
            work: None,
            world: false,
            replace: false,
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cli_show_work_displays_effective_inherited_world_override() {
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

        let scope = MomentDirectiveScopeArgs {
            work: Some("wrk_1".to_string()),
            world: None,
        };
        let (shown, source) = resolve_effective_for_show(&pool, "ctr_test", "wrk_novel", &scope)
            .await
            .unwrap()
            .expect("effective directive resolves through the World override");
        assert_eq!(shown.directive_id, "dir_world");
        assert_eq!(shown.scope_kind, "world");
        assert!(
            source.contains("inherited from world wld_1"),
            "show must name the inherited source scope, got: {source}"
        );
    }


    // ── T5: CLI author surface (set/show/clear) ───────────────────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cli_round_trip_set_show_clear_show() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool).await;
        seed_world(&pool, "wld_1").await;
        seed_work(&pool, &work_record("wrk_1", Some("wld_1"), Some("novel"))).await;

        // set
        let args = MomentDirectiveSetArgs {
            body: "  Keep the prose terse.  ".to_string(),
            depth: DirectiveDepth::Head,
            ttl_generations: Some(5),
            ttl_chapters: None,
            clear_on_scene_change: true,
            work: Some("wrk_1".to_string()),
            world: false,
            replace: false,
        };
        handle_set(&pool, "ctr_test", "wrk_novel", &args)
            .await
            .unwrap();

        // show — the effective directive for the Work is its own row.
        let scope = MomentDirectiveScopeArgs {
            work: Some("wrk_1".to_string()),
            world: None,
        };
        let (shown, source) = resolve_effective_for_show(&pool, "ctr_test", "wrk_novel", &scope)
            .await
            .unwrap()
            .expect("show finds the active directive");
        assert_eq!(shown.body, "Keep the prose terse.", "body trimmed at write");
        assert_eq!(shown.insert_depth, "head");
        assert_eq!(shown.ttl_kind, "generations");
        assert_eq!(shown.ttl_remaining, 5);
        assert!(shown.clear_on_scene_change);
        assert!(
            source.contains("own directive"),
            "the Work's own directive is the effective source, got: {source}"
        );

        // clear
        handle_clear(&pool, "ctr_test", "wrk_novel", &scope)
            .await
            .unwrap();
        assert!(
            resolve_effective_for_show(&pool, "ctr_test", "wrk_novel", &scope)
                .await
                .unwrap()
                .is_none()
        );

        // show again → no active directive
        handle_show(&pool, "ctr_test", "wrk_novel", &scope)
            .await
            .unwrap();
        assert!(
            resolve_effective_for_show(&pool, "ctr_test", "wrk_novel", &scope)
                .await
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cli_set_requires_replace_when_active() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool).await;
        seed_world(&pool, "wld_1").await;
        seed_work(&pool, &work_record("wrk_1", Some("wld_1"), Some("novel"))).await;

        let args = set_args("First directive.", DirectiveDepth::Mid, Some(3), None);
        handle_set(&pool, "ctr_test", "wrk_novel", &args)
            .await
            .unwrap();

        let err = handle_set(&pool, "ctr_test", "wrk_novel", &args)
            .await
            .unwrap_err();
        assert!(
            format!("{err}").contains("--replace"),
            "an active directive must require --replace, got: {err}"
        );

        let mut replace_args = set_args("Second directive.", DirectiveDepth::Tail, Some(7), None);
        replace_args.replace = true;
        handle_set(&pool, "ctr_test", "wrk_novel", &replace_args)
            .await
            .unwrap();

        let active = get_active_for_work(&pool, "ctr_test", "wrk_1")
            .await
            .unwrap()
            .expect("new directive active");
        assert_eq!(active.body, "Second directive.");
        assert_eq!(active.ttl_remaining, 7);
        // The superseded row is retained with `replaced_by` (audit chain).
        // SAFETY: test-only SELECT with bind params against known schema.
        let old_rows: Vec<MomentDirectiveRow> = sqlx::query_as::<_, MomentDirectiveRow>(
            "SELECT directive_id, creator_id, scope_kind, scope_id, body, insert_depth,
                    ttl_kind, ttl_remaining, clear_on_scene_change, status,
                    last_focused_event_id, created_at, updated_at,
                    expires_at, replaced_by
             FROM moment_directives WHERE status = 'expired' AND scope_kind = 'work' AND scope_id = 'wrk_1'",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(old_rows.len(), 1);
        assert_eq!(old_rows[0].body, "First directive.");
        assert_eq!(
            old_rows[0].replaced_by.as_deref(),
            Some(active.directive_id.as_str())
        );
        assert!(old_rows[0].expires_at.is_some());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cli_set_validation_rejects_bad_input() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool).await;
        seed_world(&pool, "wld_1").await;
        seed_work(&pool, &work_record("wrk_1", Some("wld_1"), Some("novel"))).await;

        // Empty body (after trim).
        let err = handle_set(
            &pool,
            "ctr_test",
            "wrk_novel",
            &set_args("   ", DirectiveDepth::Mid, Some(1), None),
        )
        .await
        .unwrap_err();
        assert!(
            format!("{err}").contains("--body"),
            "empty body rejected: {err}"
        );

        // Missing TTL kind.
        let err = handle_set(
            &pool,
            "ctr_test",
            "wrk_novel",
            &set_args("Body.", DirectiveDepth::Mid, None, None),
        )
        .await
        .unwrap_err();
        assert!(
            format!("{err}").contains("exactly one"),
            "missing TTL rejected: {err}"
        );

        // Non-positive TTL.
        let err = handle_set(
            &pool,
            "ctr_test",
            "wrk_novel",
            &set_args("Body.", DirectiveDepth::Mid, Some(0), None),
        )
        .await
        .unwrap_err();
        assert!(
            format!("{err}").contains("positive"),
            "zero TTL rejected: {err}"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cli_set_world_requires_world_bound_work() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool).await;
        seed_work(&pool, &work_record("wrk_1", None, Some("essay"))).await;

        let mut args = set_args("Body.", DirectiveDepth::Mid, Some(3), None);
        args.world = true;
        let err = handle_set(&pool, "ctr_test", "wrk_novel", &args)
            .await
            .unwrap_err();
        assert!(
            format!("{err}").contains("not bound to a World"),
            "world override on a worldless Work must error, got: {err}"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cli_world_scope_set_and_show() {
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool).await;
        seed_world(&pool, "wld_1").await;
        seed_work(&pool, &work_record("wrk_1", Some("wld_1"), Some("novel"))).await;

        let mut args = set_args(
            "British spelling always.",
            DirectiveDepth::Tail,
            None,
            Some(4),
        );
        args.world = true;
        handle_set(&pool, "ctr_test", "wrk_novel", &args)
            .await
            .unwrap();

        let scope = MomentDirectiveScopeArgs {
            work: None,
            world: Some("wld_1".to_string()),
        };
        let (shown, source) = resolve_effective_for_show(&pool, "ctr_test", "wrk_novel", &scope)
            .await
            .unwrap()
            .expect("world override visible");
        assert_eq!(shown.scope_kind, "world");
        assert_eq!(shown.ttl_kind, "chapters");
        assert_eq!(shown.ttl_remaining, 4);
        assert_eq!(source, "world wld_1");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn cli_show_and_clear_world_scope_work_independent() {
        // R-007 matrix coverage: `show --world <id>` / `clear --world <id>`
        // select the World scope directly — no Work resolution required
        // (deliberately no Work seeded here).
        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool).await;
        seed_world(&pool, "wld_1").await;
        set_active(
            &pool,
            &new_params("dir_world", scope_kind::WORLD, "wld_1", "generations", 3),
        )
        .await
        .unwrap();

        // show --world: the World override row itself, no Work in play.
        let scope = MomentDirectiveScopeArgs {
            work: None,
            world: Some("wld_1".to_string()),
        };
        handle_show(&pool, "ctr_test", "wrk_novel", &scope)
            .await
            .unwrap();
        let (shown, source) = resolve_effective_for_show(&pool, "ctr_test", "wrk_novel", &scope)
            .await
            .unwrap()
            .expect("world directive shown");
        assert_eq!(shown.directive_id, "dir_world");
        assert_eq!(shown.scope_kind, "world");
        assert_eq!(source, "world wld_1");

        // clear --world: soft-deletes the World row; a second show reports none.
        handle_clear(&pool, "ctr_test", "wrk_novel", &scope)
            .await
            .unwrap();
        let row = get_by_id(&pool, "dir_world")
            .await
            .unwrap()
            .expect("row retained for inspection");
        assert_eq!(
            row.status, "expired",
            "clear --world must soft-delete the World-scoped row"
        );
        assert!(
            resolve_effective_for_show(&pool, "ctr_test", "wrk_novel", &scope)
                .await
                .unwrap()
                .is_none(),
            "show --world after clear reports no active directive"
        );
    }

    // ── T6/T7: end-to-end injection through `assemble_moment_with_directive`

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn assemble_renders_then_expires_directive() {
        use nexus_moment_context_assembly::{
            assemble_moment_with_directive, MomentRequest, Stage0Assembly,
        };

        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool).await;
        seed_world(&pool, "wld_1").await;
        seed_work(&pool, &work_record("wrk_1", Some("wld_1"), Some("novel"))).await;
        nexus_local_db::narrative_gateway::seed::event(
            &pool,
            "evt_e2e",
            "wld_1",
            "fbk_root",
            "story_advance",
            1,
        )
        .await;
        nexus_local_db::kb_store::seed::knowledge_entry(
            &pool,
            "kb_e2e",
            "wld_1",
            "Character",
            "Hero",
            "confirmed",
        )
        .await;
        set_active(
            &pool,
            &new_params("dir_e2e", scope_kind::WORK, "wrk_1", "generations", 1),
        )
        .await
        .unwrap();

        let narrative =
            nexus_local_db::narrative_gateway::SqliteNarrativeGateway::new(pool.clone());
        let kb = nexus_local_db::kb_store::SqliteKbStore::new(pool.clone());
        let knowledge = nexus_local_db::SqliteKnowledgeStore::new(pool.clone());
        let directives = LocalDirectiveStore::new(pool.clone());

        let stage0 = Stage0Assembly {
            personality: "Test personality.".to_string(),
            ..Stage0Assembly::default()
        };
        let request = MomentRequest::new(stage0)
            .with_world("wld_1")
            .with_work("wrk_1")
            .with_creator("ctr_test")
            .with_event("evt_e2e");

        // First assemble: the directive injects and its TTL drops to 0.
        let ctx =
            assemble_moment_with_directive(&request, &narrative, &kb, &knowledge, &directives)
                .await;
        assert_eq!(
            ctx.moment_directive.as_deref(),
            Some("Keep the prose terse.")
        );
        assert!(ctx.to_full_context().contains("## Moment Directive"));
        assert!(ctx.to_full_context().contains("Keep the prose terse."));
        let row = get_by_id(&pool, "dir_e2e").await.unwrap().unwrap();
        assert_eq!(row.ttl_remaining, 0);
        assert_eq!(row.status, "expired");

        // Second assemble: the directive is gone (TTL-0 ⇒ no injection) and
        // the output is byte-identical to the no-directive path (AC-I1b).
        let ctx2 =
            assemble_moment_with_directive(&request, &narrative, &kb, &knowledge, &directives)
                .await;
        assert!(ctx2.moment_directive.is_none());
        assert!(!ctx2.to_full_context().contains("## Moment Directive"));
        assert_eq!(
            ctx2.to_full_context(),
            ctx.to_full_context()
                .replace("## Moment Directive\n\nKeep the prose terse.\n\n", "",)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn assemble_ttl_three_injects_exactly_three_then_stops() {
        // V1.150 P2 dogfood (T4): `--ttl-generations 3` ⇒ the directive
        // injects on exactly 3 `assemble_moment` calls, then stops. Counting
        // one generation = one injecting assemble (spec §3.3).
        use nexus_moment_context_assembly::{
            assemble_moment_with_directive, MomentRequest, Stage0Assembly,
        };

        let (pool, _dir) = fresh_pool().await;
        seed_creator(&pool).await;
        seed_world(&pool, "wld_1").await;
        seed_work(&pool, &work_record("wrk_1", Some("wld_1"), Some("novel"))).await;
        nexus_local_db::narrative_gateway::seed::event(
            &pool,
            "evt_e2e",
            "wld_1",
            "fbk_root",
            "story_advance",
            1,
        )
        .await;
        nexus_local_db::kb_store::seed::knowledge_entry(
            &pool,
            "kb_e2e",
            "wld_1",
            "Character",
            "Hero",
            "confirmed",
        )
        .await;
        set_active(
            &pool,
            &new_params("dir_ttl3", scope_kind::WORK, "wrk_1", "generations", 3),
        )
        .await
        .unwrap();

        let narrative =
            nexus_local_db::narrative_gateway::SqliteNarrativeGateway::new(pool.clone());
        let kb = nexus_local_db::kb_store::SqliteKbStore::new(pool.clone());
        let knowledge = nexus_local_db::SqliteKnowledgeStore::new(pool.clone());
        let directives = LocalDirectiveStore::new(pool.clone());

        let stage0 = Stage0Assembly {
            personality: "Test personality.".to_string(),
            ..Stage0Assembly::default()
        };
        let request = MomentRequest::new(stage0)
            .with_world("wld_1")
            .with_work("wrk_1")
            .with_creator("ctr_test")
            .with_event("evt_e2e");

        // Calls 1–3: the directive injects and the TTL counts down 3→2→1.
        for expected_remaining in [2, 1, 0] {
            let ctx =
                assemble_moment_with_directive(&request, &narrative, &kb, &knowledge, &directives)
                    .await;
            assert_eq!(
                ctx.moment_directive.as_deref(),
                Some("Keep the prose terse."),
                "directive must inject on call {expected_remaining} (remaining {expected_remaining})"
            );
            assert!(ctx.to_full_context().contains("## Moment Directive"));
            let row = get_by_id(&pool, "dir_ttl3").await.unwrap().unwrap();
            assert_eq!(row.ttl_remaining, expected_remaining);
        }
        // After the 3rd injection the TTL is 0 → row expired.
        let expired = get_by_id(&pool, "dir_ttl3").await.unwrap().unwrap();
        assert_eq!(expired.status, "expired");

        // Call 4: no injection (expired rows never inject, spec §3.3).
        let ctx4 =
            assemble_moment_with_directive(&request, &narrative, &kb, &knowledge, &directives)
                .await;
        assert!(
            ctx4.moment_directive.is_none(),
            "4th assemble must not inject the expired directive"
        );
        assert!(!ctx4.to_full_context().contains("## Moment Directive"));
    }
}
