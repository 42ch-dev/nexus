//! Moment Directive author surface (V1.150 P1, DF-75) — `creator
//! moment-directive set|show|clear` (direct-core retarget v1.193 P0-T8).
//!
//! CLI-only author surface per the Q3 lock (spec `fl-l-w5-prompt-control-plane.md`
//! §1.2 / §3): a short author-written instruction injected by MCA into the
//! reserved `moment.directive` slot (above lore, below system/personality).
//! Persistence is `nexus-local-db` (`moment_directives` table); observation
//! is the existing `platform context assemble-moment` output.
//!
//! Every verb runs on the typed core seam ([`CoreService::moment_directive`],
//! `context.rs`) through the shared direct-call helper ([`crate::core`]): the
//! core owns scope ownership (403 for a foreign Work/World — a foreign scope
//! never leaks directive state), the validation, the Work-wins / World-
//! override inheritance and the soft clear. This module owns only the CLI
//! half — mapping flags onto the typed request, resolving the implicit Work
//! scope, and rendering the settled row after the writer is released.
//!
//! # Product-local only (AC-I3)
//!
//! The directive is NEVER on the spoke wire: not a `modules.*` object, not a
//! `KnowledgeEntry`, never in `AssemblePacket` `placement[]` /
//! `activation_trace[]`, never in any pack export/import path.

use clap::{Args, Subcommand};

use crate::commands::creator::works::active_work_id_core;
use crate::config::CliConfig;
use crate::core::{finish_direct, map_core_error, open_direct_core};
use crate::errors::{CliError, Result};
use nexus_contracts::daemon_api::inspector::moment_directive_request::{
    MomentDirectiveRequest, MomentDirectiveRequestAction, MomentDirectiveRequestInsertDepth,
    MomentDirectiveRequestScope, MomentDirectiveRequestScopeKind, MomentDirectiveRequestTtlKind,
};
use nexus_contracts::daemon_api::inspector::moment_directive_response::{
    MomentDirectiveResponse, NexusDaemonMomentDirectiveResponseDirectiveScopeKind,
};
use nexus_core::{CoreService, Principal};
use std::fmt::Write as _;
use std::num::NonZeroU64;

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
    pub depth: MomentDirectiveRequestInsertDepth,

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

/// Parse `--depth` into the typed insert-depth enum.
fn parse_depth(value: &str) -> std::result::Result<MomentDirectiveRequestInsertDepth, String> {
    value
        .parse()
        .map_err(|_| format!("unknown insert depth {value:?} (expected head | mid | tail)"))
}

/// Run the `creator moment-directive` command against the selected workspace.
///
/// The writer is released by [`finish_direct`] before any line is printed, so
/// the author never reads a reported outcome the core could not settle.
///
/// # Errors
///
/// Returns [`CliError`] when no creator/workspace is selected, a flag is
/// invalid, the resolved Work has no bound World for `--world`, or the mapped
/// core refusal (403 for a foreign scope, 409 for a `set` over an active
/// directive without `--replace`, storage failure), plus any cleanup refusal
/// from [`finish_direct`].
pub async fn run(command: MomentDirectiveCommand, config: &CliConfig) -> Result<()> {
    let core = open_direct_core(config).await?;
    let outcome = async {
        let principal = core.active_principal().await.map_err(map_core_error)?;
        match command {
            MomentDirectiveCommand::Set(args) => handle_set(&core, &principal, &args).await,
            MomentDirectiveCommand::Show(args) => handle_show(&core, &principal, &args).await,
            MomentDirectiveCommand::Clear(args) => handle_clear(&core, &principal, &args).await,
        }
    }
    .await;
    println!("{}", finish_direct(&core, outcome).await?);
    Ok(())
}

/// `creator moment-directive set` handler (spec §3.1 / §3.3).
///
/// # Errors
///
/// Returns a named [`CliError::Config`] for an empty `--body`, a missing /
/// non-positive TTL, or a World scope on a Work with no bound World; the
/// mapped core refusal otherwise.
async fn handle_set(
    core: &CoreService,
    principal: &Principal,
    args: &MomentDirectiveSetArgs,
) -> Result<String> {
    // ── Flag mapping (spec §3.1 / §3.3 "Write") ─────────────────────────
    let body = args.body.trim();
    if body.is_empty() {
        return Err(CliError::Config(
            "--body must be non-empty (after trimming whitespace)".to_string(),
        ));
    }
    let (ttl_kind, ttl_remaining) = map_ttl(args)?;
    let (scope_kind, scope_id, scope_label) = resolve_set_scope(core, principal, args).await?;

    let response = core
        .moment_directive(
            principal,
            MomentDirectiveRequest {
                action: MomentDirectiveRequestAction::Set,
                body: Some(body.to_string()),
                clear_on_scene_change: Some(args.clear_on_scene_change),
                insert_depth: Some(args.depth),
                replace: Some(args.replace),
                ttl_kind: Some(ttl_kind),
                ttl_remaining: Some(ttl_remaining),
                scope: MomentDirectiveRequestScope {
                    id: scope_id,
                    kind: scope_kind,
                },
            },
        )
        .await
        .map_err(map_core_error)?;

    // `set` writes a row, so the core answers the `Directive` branch; the
    // empty branch would mean the settled row could not be decoded.
    let MomentDirectiveResponse::Directive {
        directive_id,
        insert_depth,
        ttl_kind,
        ttl_remaining,
        clear_on_scene_change,
        ..
    } = response
    else {
        return Err(CliError::Other(
            "core returned no directive for a set operation".to_string(),
        ));
    };

    let mut out = String::new();
    let _ = writeln!(out, "✓ Moment Directive set for {scope_label}");
    let _ = writeln!(out, "  id: {directive_id}");
    let _ = writeln!(out, "  depth: {insert_depth}");
    let _ = writeln!(out, "  ttl: {ttl_remaining} {ttl_kind}");
    if clear_on_scene_change {
        let _ = writeln!(out, "  clear_on_scene_change: yes");
    }
    Ok(out)
}

/// `creator moment-directive show` handler — displays the **effective**
/// directive for the requested scope (spec §3.2, QC2-F8): for a Work the
/// author sees the directive that actually injects (the Work's own, or the
/// inherited World override, which the settled row's scope names), with the
/// source scope called out explicitly.
///
/// # Errors
///
/// Returns the mapped core refusal (403 for a foreign scope, storage
/// failure).
async fn handle_show(
    core: &CoreService,
    principal: &Principal,
    args: &MomentDirectiveScopeArgs,
) -> Result<String> {
    let (scope_kind, scope_id) = resolve_scope(core, principal, args).await?;
    let response = core
        .moment_directive(
            principal,
            scoped_request(
                MomentDirectiveRequestAction::Show,
                scope_kind,
                &scope_id,
            ),
        )
        .await
        .map_err(map_core_error)?;

    let MomentDirectiveResponse::Directive {
        body,
        clear_on_scene_change,
        directive_id,
        insert_depth,
        scope_id: settled_scope_id,
        scope_kind: settled_scope_kind,
        ttl_kind,
        ttl_remaining,
        ..
    } = response
    else {
        return Ok("No active Moment Directive for this scope.".to_string());
    };

    let effective_for = effective_label(
        scope_kind,
        &scope_id,
        settled_scope_kind,
        &settled_scope_id,
    );
    let mut out = String::new();
    let _ = writeln!(out, "Directive: {directive_id}");
    let _ = writeln!(out, "Scope: {settled_scope_kind} {settled_scope_id}");
    let _ = writeln!(out, "Effective for: {effective_for}");
    let _ = writeln!(out, "Depth: {insert_depth}");
    let _ = writeln!(out, "TTL: {ttl_remaining} remaining ({ttl_kind})");
    if clear_on_scene_change {
        let _ = writeln!(out, "Clear on scene change: yes");
    }
    let _ = writeln!(out, "Body:");
    let _ = write!(out, "{body}");
    Ok(out)
}

/// `creator moment-directive clear` handler — soft-delete the active row.
///
/// The core's `clear` answer is the empty branch (no row body), so the
/// retired CLI distinction between an expired row and an already-empty scope
/// is no longer observable from one response — and it never was reliable,
/// because a Work scope inherits a World override. The message therefore
/// names the soft-delete the command performed.
///
/// # Errors
///
/// Returns the mapped core refusal (403 for a foreign scope, storage
/// failure).
async fn handle_clear(
    core: &CoreService,
    principal: &Principal,
    args: &MomentDirectiveScopeArgs,
) -> Result<String> {
    let (scope_kind, scope_id) = resolve_scope(core, principal, args).await?;
    core.moment_directive(
        principal,
        scoped_request(
            MomentDirectiveRequestAction::Clear,
            scope_kind,
            &scope_id,
        ),
    )
    .await
    .map_err(map_core_error)?;
    Ok(format!(
        "✓ Moment Directive cleared (soft-deleted) for {scope_kind} {scope_id}."
    ))
}

/// A request with no optional field set — the `show` / `clear` shape.
fn scoped_request(
    action: MomentDirectiveRequestAction,
    scope_kind: MomentDirectiveRequestScopeKind,
    scope_id: &str,
) -> MomentDirectiveRequest {
    MomentDirectiveRequest {
        action,
        body: None,
        clear_on_scene_change: None,
        insert_depth: None,
        replace: None,
        scope: MomentDirectiveRequestScope {
            id: scope_id.to_string(),
            kind: scope_kind,
        },
        ttl_kind: None,
        ttl_remaining: None,
    }
}

/// Map `--ttl-generations` / `--ttl-chapters` onto the typed TTL pair.
///
/// The flag pair is mutually exclusive in clap, and the wire carries one
/// kind plus a `NonZeroU64` count, so the CLI owns the two flag-vocabulary
/// refusals before the core re-validates the pair on its own authority.
///
/// # Errors
///
/// Returns [`CliError::Config`] when neither kind is given, both are given
/// (unreachable through clap, kept for direct callers), or the count is not
/// a positive integer.
fn map_ttl(
    args: &MomentDirectiveSetArgs,
) -> Result<(MomentDirectiveRequestTtlKind, NonZeroU64)> {
    match (args.ttl_generations, args.ttl_chapters) {
        (Some(count), None) => Ok((
            MomentDirectiveRequestTtlKind::Generations,
            positive_ttl(count, "--ttl-generations")?,
        )),
        (None, Some(count)) => Ok((
            MomentDirectiveRequestTtlKind::Chapters,
            positive_ttl(count, "--ttl-chapters")?,
        )),
        (None, None) => Err(CliError::Config(
            "exactly one of --ttl-generations / --ttl-chapters is required".to_string(),
        )),
        (Some(_), Some(_)) => Err(CliError::Config(
            "--ttl-generations and --ttl-chapters are mutually exclusive".to_string(),
        )),
    }
}

/// Parse a TTL count into the wire's positive count.
///
/// # Errors
///
/// Returns [`CliError::Config`] naming `flag` when the count is zero or
/// negative.
fn positive_ttl(count: i64, flag: &str) -> Result<NonZeroU64> {
    u64::try_from(count)
        .ok()
        .and_then(NonZeroU64::new)
        .ok_or_else(|| {
            CliError::Config(format!(
                "{flag} must be a positive integer (>= 1), got {count}"
            ))
        })
}

/// Resolve the `set` scope (spec §3.2): both branches resolve the Work first
/// (explicit `--work` or active) — the Work-scoped directive targets the Work
/// itself, the World override targets the Work's bound World — and return the
/// scope plus its human label.
///
/// # Errors
///
/// Returns [`CliError::Config`] when no Work resolves, the Work read fails, or
/// `--world` is passed on a Work with no bound World.
async fn resolve_set_scope(
    core: &CoreService,
    principal: &Principal,
    args: &MomentDirectiveSetArgs,
) -> Result<(MomentDirectiveRequestScopeKind, String, String)> {
    let work_id = resolve_work_id(core, principal, args.work.as_deref()).await?;
    if !args.world {
        return Ok((
            MomentDirectiveRequestScopeKind::Work,
            work_id.clone(),
            format!("work {work_id}"),
        ));
    }
    let work = core
        .get_work(principal, work_id.clone())
        .await
        .map_err(map_core_error)?;
    let world_id = work.world_id.ok_or_else(|| {
        CliError::Config(format!(
            "Work {work_id} is not bound to a World; a World-scoped Moment Directive needs a World. \
             Set a Work-scoped directive instead."
        ))
    })?;
    let label = format!("world {world_id}");
    Ok((MomentDirectiveRequestScopeKind::World, world_id, label))
}

/// Resolve the `(scope_kind, scope_id)` for a `show`/`clear` selection:
/// `--world <id>` selects the World scope directly; otherwise the Work scope
/// (explicit `--work <id>` or the active Work).
///
/// # Errors
///
/// Returns [`CliError::Config`] when the implicit Work selection finds no
/// active Work and the mapped core refusal when the bounded Work query fails.
async fn resolve_scope(
    core: &CoreService,
    principal: &Principal,
    args: &MomentDirectiveScopeArgs,
) -> Result<(MomentDirectiveRequestScopeKind, String)> {
    if let Some(world_id) = args.world.as_deref() {
        return Ok((
            MomentDirectiveRequestScopeKind::World,
            world_id.to_string(),
        ));
    }
    let work_id = resolve_work_id(core, principal, args.work.as_deref()).await?;
    Ok((MomentDirectiveRequestScopeKind::Work, work_id))
}

/// Resolve a Work: explicit `--work <id>` (the core's ownership gate verifies
/// it) or the active Work through the same bounded core selection every other
/// Work leaf uses ([`active_work_id_core`]).
///
/// # Errors
///
/// Returns [`CliError::Config`] when no active Work exists and the mapped core
/// error when the bounded query fails.
async fn resolve_work_id(
    core: &CoreService,
    principal: &Principal,
    explicit: Option<&str>,
) -> Result<String> {
    match explicit {
        Some(work_id) => Ok(work_id.to_string()),
        None => active_work_id_core(core, principal).await,
    }
}

/// The scope label `show` reports for the settled row: a World request is the
/// override itself; a Work request is the Work's own directive or the bound
/// World's inherited override (spec §3.2, Work-wins / World-override).
fn effective_label(
    requested: MomentDirectiveRequestScopeKind,
    requested_id: &str,
    settled: NexusDaemonMomentDirectiveResponseDirectiveScopeKind,
    settled_id: &str,
) -> String {
    match (requested, settled) {
        (MomentDirectiveRequestScopeKind::World, _) => format!("world {settled_id}"),
        (
            MomentDirectiveRequestScopeKind::Work,
            NexusDaemonMomentDirectiveResponseDirectiveScopeKind::Work,
        ) => format!("work {requested_id} (own directive)"),
        (
            MomentDirectiveRequestScopeKind::Work,
            NexusDaemonMomentDirectiveResponseDirectiveScopeKind::World,
        ) => format!("work {requested_id} (inherited from world {settled_id})"),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use nexus_contracts::{
        CoreRegisterCreatorRequest, CreateWorkRequest, SetActiveWorkspaceRequest,
    };
    use nexus_core::{CoreAccess, CoreHomeService, CoreOpenOptions, LocalDirectiveStore};
    use nexus_home_layout::{operational_workspace_dir, workspace_state_db_path};
    use nexus_local_db::moment_directive::{get_active_for_work, get_by_id, scope_kind, set_active};
    use nexus_local_db::writer_protocol::release_retained_writer_guards;
    use nexus_local_db::{
        WorkRecord, create_work, ensure_creator_row, open_pool, run_migrations, seed_versions,
    };
    use sqlx::SqlitePool;
    use std::path::PathBuf;

    /// The creator the module fixture registers (a persistent local identity).
    const CREATOR_NAME: &str = "Directive Fixture Author";
    /// Workspace the fixture materializes and selects.
    const WORKSPACE_SLUG: &str = "default";
    /// Work title the bound Work is created under.
    const BOUND_WORK_TITLE: &str = "Directive Bound Novel";
    /// Deterministic `story_ref` for the bound Work.
    const BOUND_STORY_REF: &str = "directive-bound-novel";

    /// A direct-core fixture: one home, one creator/workspace, one owned World
    /// with one Work bound to it, plus one stored Work that predates the World
    /// requirement the core enforces on creation.
    struct Env {
        _home: tempfile::TempDir,
        db_path: PathBuf,
        creator_id: String,
        core: CoreService,
        principal: Principal,
        world_id: String,
        work_id: String,
        /// A `works` row carrying no `world_id` — reachable from the CLI (a
        /// `--world` scope then has no World to target) but not from
        /// [`CoreService::create_work`], which requires a World binding
        /// (V1.40+), so it is seeded at the local-db layer.
        worldless_work_id: String,
    }

    impl Env {
        async fn new() -> Self {
            let home = tempfile::tempdir().unwrap();
            let user_home = home.path().to_path_buf();
            let selector = CoreHomeService::open(user_home.clone()).unwrap();
            let creator = selector
                .register_creator(CoreRegisterCreatorRequest {
                    display_name: Some(CREATOR_NAME.parse().unwrap()),
                    platform_creator_id: None,
                })
                .await
                .unwrap();
            let creator_id = creator.creator_id;
            std::fs::create_dir_all(operational_workspace_dir(
                &user_home,
                &creator_id,
                WORKSPACE_SLUG,
            ))
            .unwrap();
            selector
                .select_workspace(SetActiveWorkspaceRequest {
                    creator_id: Some(creator_id.clone()),
                    workspace_slug: WORKSPACE_SLUG.to_string(),
                })
                .await
                .unwrap();

            let db_path = workspace_state_db_path(&user_home, &creator_id, WORKSPACE_SLUG);
            release_retained_writer_guards(&db_path);

            // A `works` row with no bound World: the core refuses to *create*
            // one (V1.40+), but stored rows from that path still exist, and the
            // `--world` scope must refuse them rather than write a directive no
            // assembly could inject.
            let worldless_work_id = "wrk_stored_worldless".to_string();
            {
                let pool = nexus_local_db::init_engine_pool(&db_path)
                    .await
                    .unwrap()
                    .clone_pool();
                let mut record = work_record(&worldless_work_id, None, Some("essay"));
                record.creator_id = creator_id.clone();
                record.workspace_slug = WORKSPACE_SLUG.to_string();
                create_work(&pool, &record).await.unwrap();
                pool.close().await;
            }
            release_retained_writer_guards(&db_path);

            // The core resolves workspace files against the operational
            // `meta.json` `local_root` (the same key the workspace
            // registration writes); the Work directory needs it.
            let creative_root = user_home.join("creative");
            std::fs::create_dir_all(&creative_root).unwrap();
            std::fs::write(
                operational_workspace_dir(&user_home, &creator_id, WORKSPACE_SLUG)
                    .join("meta.json"),
                serde_json::to_string(&serde_json::json!({
                    "schema_version": 1,
                    "creator_id": creator_id,
                    "workspace_slug": WORKSPACE_SLUG,
                    "local_root": creative_root,
                    "created_at": "2020-01-01T00:00:00Z"
                }))
                .unwrap(),
            )
            .unwrap();

            let core = CoreService::open(CoreOpenOptions {
                user_home,
                access: CoreAccess::DirectWriter,
            })
            .await
            .unwrap();
            let principal = core.active_principal().await.unwrap();
            let world_id = core
                .create_world(
                    &principal,
                    serde_json::from_value(serde_json::json!({
                        "title": "Directive Test World"
                    }))
                    .unwrap(),
                )
                .await
                .unwrap()
                .world_id;
            let work_id = core
                .create_work(
                    &principal,
                    CreateWorkRequest {
                        client_request_id: None,
                        initial_idea: "An idea.".to_string(),
                        lineage_from_work_id: None,
                        long_term_goal: "Keep the prose terse.".to_string(),
                        primary_preset_id: None,
                        set_pool_active: None,
                        story_ref: Some(BOUND_STORY_REF.to_string()),
                        title: BOUND_WORK_TITLE.to_string(),
                        work_profile: Some("novel".to_string()),
                        world_id: Some(world_id.clone()),
                    },
                )
                .await
                .unwrap()
                .work_id;

            Self {
                _home: home,
                db_path,
                creator_id,
                core,
                principal,
                world_id,
                work_id,
                worldless_work_id,
            }
        }

        /// A read-only pool over the fixture's `state.db`.
        async fn pool(&self) -> SqlitePool {
            open_pool(&self.db_path).await.unwrap()
        }
    }

    fn set_args(
        body: &str,
        depth: MomentDirectiveRequestInsertDepth,
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

    fn scope_args(work: Option<&str>, world: Option<&str>) -> MomentDirectiveScopeArgs {
        MomentDirectiveScopeArgs {
            work: work.map(str::to_string),
            world: world.map(str::to_string),
        }
    }

    // ── Handler round trip: set → show → clear → show ─────────────────

    /// The three verbs map onto the three typed actions: `set` writes the
    /// trimmed body for the resolved (implicit) Work scope, `show` reads the
    /// effective row, and `clear` soft-deletes it — the next `show` reports
    /// none while the expired row stays for inspection.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn set_show_clear_map_onto_the_typed_actions() {
        let env = Env::new().await;
        let mut args = set_args(
            "  Keep the prose terse.  ",
            MomentDirectiveRequestInsertDepth::Head,
            Some(5),
            None,
        );
        args.clear_on_scene_change = true;

        // set — no --work: the active Work resolves implicitly.
        let set = handle_set(&env.core, &env.principal, &args)
            .await
            .unwrap();
        assert!(
            set.contains(&format!("✓ Moment Directive set for work {}", env.work_id)),
            "set must name the resolved Work scope: {set}"
        );
        assert!(set.contains("depth: head"), "{set}");
        assert!(set.contains("ttl: 5 generations"), "{set}");
        assert!(set.contains("clear_on_scene_change: yes"), "{set}");

        // show — the Work's own row is the effective directive.
        let show = handle_show(&env.core, &env.principal, &scope_args(None, None))
            .await
            .unwrap();
        assert!(show.contains("Scope: work "), "{show}");
        assert!(
            show.contains(&format!("Effective for: work {} (own directive)", env.work_id)),
            "{show}"
        );
        assert!(show.contains("TTL: 5 remaining (generations)"), "{show}");
        assert!(
            show.contains("Keep the prose terse."),
            "the body must be author-visible on show: {show}"
        );
        assert!(!show.contains("  Keep"), "the body is trimmed at write: {show}");

        // clear — soft-delete, then show reports none.
        let clear = handle_clear(&env.core, &env.principal, &scope_args(None, None))
            .await
            .unwrap();
        assert!(
            clear.contains("cleared (soft-deleted)"),
            "clear must report the soft-delete: {clear}"
        );
        let after = handle_show(&env.core, &env.principal, &scope_args(None, None))
            .await
            .unwrap();
        assert_eq!(after, "No active Moment Directive for this scope.");

        // … and the cleared row is retained, not hard-deleted.
        let pool = env.pool().await;
        let row = get_active_for_work(&pool, &env.creator_id, &env.work_id)
            .await
            .unwrap();
        assert!(row.is_none(), "no active row remains");
        let expired: Vec<(String, String)> = sqlx::query_as(
            "SELECT directive_id, status FROM moment_directives WHERE scope_kind = 'work'",
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(expired.len(), 1, "the row is retained: {expired:?}");
        assert_eq!(expired[0].1, "expired");
        pool.close().await;
    }

    /// A Work with no own directive inherits the bound World's override, and
    /// `show` names that inherited source (spec §3.2 Work-wins /
    /// World-override).
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn show_names_the_inherited_world_override() {
        let env = Env::new().await;
        let mut args = set_args(
            "British spelling always.",
            MomentDirectiveRequestInsertDepth::Tail,
            None,
            Some(4),
        );
        args.world = true;
        args.work = Some(env.work_id.clone());
        let set = handle_set(&env.core, &env.principal, &args)
            .await
            .unwrap();
        assert!(
            set.contains(&format!("✓ Moment Directive set for world {}", env.world_id)),
            "the World override names the bound World: {set}"
        );

        let show = handle_show(&env.core, &env.principal, &scope_args(Some(&env.work_id), None))
            .await
            .unwrap();
        assert!(show.contains(&format!("Scope: world {}", env.world_id)), "{show}");
        assert!(
            show.contains(&format!(
                "Effective for: work {} (inherited from world {})",
                env.work_id, env.world_id
            )),
            "{show}"
        );
        assert!(show.contains("TTL: 4 remaining (chapters)"), "{show}");
        assert!(show.contains("British spelling always."), "{show}");

        // The World scope itself is what `--world` selects, no Work needed.
        let direct = handle_show(&env.core, &env.principal, &scope_args(None, Some(&env.world_id)))
            .await
            .unwrap();
        assert!(
            direct.contains(&format!("Effective for: world {}", env.world_id)),
            "{direct}"
        );
    }

    /// `--world` on a Work with no bound World refuses instead of writing a
    /// directive no assembly could inject.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn set_world_scope_requires_a_world_bound_work() {
        let env = Env::new().await;
        let mut args = set_args(
            "Body.",
            MomentDirectiveRequestInsertDepth::Mid,
            Some(3),
            None,
        );
        args.world = true;
        args.work = Some(env.worldless_work_id.clone());
        let err = handle_set(&env.core, &env.principal, &args)
            .await
            .unwrap_err();
        assert!(
            format!("{err}").contains("not bound to a World"),
            "a World override needs a bound World, got: {err}"
        );

        // The Work scope itself still writes for that stored row.
        args.world = false;
        let set = handle_set(&env.core, &env.principal, &args)
            .await
            .unwrap();
        assert!(
            set.contains(&format!(
                "✓ Moment Directive set for work {}",
                env.worldless_work_id
            )),
            "{set}"
        );
    }

    /// An already-active directive in the same scope needs `--replace`: the
    /// core answers 409 and the superseded row is retained with `replaced_by`
    /// pointing at the new one.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn set_requires_replace_and_retains_the_superseded_row() {
        let env = Env::new().await;
        let mut first = set_args("First directive.", MomentDirectiveRequestInsertDepth::Mid, Some(3), None);
        first.work = Some(env.work_id.clone());
        handle_set(&env.core, &env.principal, &first).await.unwrap();
        let first_id = {
            let pool = env.pool().await;
            let row = get_active_for_work(&pool, &env.creator_id, &env.work_id)
                .await
                .unwrap()
                .expect("the first directive is active");
            pool.close().await;
            row.directive_id
        };

        let err = handle_set(&env.core, &env.principal, &first)
            .await
            .unwrap_err();
        assert!(
            matches!(err, CliError::Api { status: 409, .. }),
            "a second active directive is a 409 conflict, got: {err}"
        );
        assert!(format!("{err}").contains("already active"), "{err}");

        let mut replacement = set_args(
            "Second directive.",
            MomentDirectiveRequestInsertDepth::Tail,
            Some(7),
            None,
        );
        replacement.work = Some(env.work_id.clone());
        replacement.replace = true;
        handle_set(&env.core, &env.principal, &replacement)
            .await
            .unwrap();

        let pool = env.pool().await;
        let active = get_active_for_work(&pool, &env.creator_id, &env.work_id)
            .await
            .unwrap()
            .expect("the replacement is active");
        assert_eq!(active.body, "Second directive.");
        assert_eq!(active.ttl_remaining, 7);
        let superseded = get_by_id(&pool, &first_id)
            .await
            .unwrap()
            .expect("the superseded row is retained");
        assert_eq!(superseded.status, "expired");
        assert_eq!(superseded.body, "First directive.");
        assert_eq!(
            superseded.replaced_by.as_deref(),
            Some(active.directive_id.as_str()),
            "the superseded row points at its replacement"
        );
        assert!(superseded.expires_at.is_some());
        pool.close().await;
    }

    /// Flag-vocabulary refusals stay CLI-owned: an empty body, a missing TTL
    /// kind, a non-positive count, and a World scope on a worldless Work.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn set_rejects_bad_flags() {
        let env = Env::new().await;

        let err = handle_set(
            &env.core,
            &env.principal,
            &set_args("   ", MomentDirectiveRequestInsertDepth::Mid, Some(1), None),
        )
        .await
        .unwrap_err();
        assert!(format!("{err}").contains("--body"), "{err}");

        let err = handle_set(
            &env.core,
            &env.principal,
            &set_args("Body.", MomentDirectiveRequestInsertDepth::Mid, None, None),
        )
        .await
        .unwrap_err();
        assert!(format!("{err}").contains("exactly one"), "{err}");

        let err = handle_set(
            &env.core,
            &env.principal,
            &set_args("Body.", MomentDirectiveRequestInsertDepth::Mid, Some(0), None),
        )
        .await
        .unwrap_err();
        assert!(
            format!("{err}").contains("positive"),
            "a zero count is refused: {err}"
        );

        let err = handle_set(
            &env.core,
            &env.principal,
            &set_args("Body.", MomentDirectiveRequestInsertDepth::Mid, Some(1), Some(1)),
        )
        .await
        .unwrap_err();
        assert!(format!("{err}").contains("mutually exclusive"), "{err}");
    }

    // ── MCA injection journey through the core-owned directive store ───

    async fn fresh_pool() -> (SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool).await.unwrap();
        seed_versions(&pool).await.unwrap();
        (pool, dir)
    }

    async fn seed_creator(pool: &SqlitePool) {
        ensure_creator_row(pool, "ctr_test", "Test").await.unwrap();
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
    ) -> nexus_local_db::moment_directive::NewMomentDirective<'a> {
        nexus_local_db::moment_directive::NewMomentDirective {
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
