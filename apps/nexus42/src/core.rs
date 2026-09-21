//! Shared direct-call seam for the daemon-free authoring leaves (v1.193
//! P0-T1).
//!
//! Every authoring command that owns local writes opens the same `nexus-core`
//! direct-writer service, maps the same core error taxonomy, and closes that
//! service on success **and** failure. This module is that single seam:
//! [`open_direct_core`], [`map_core_error`] and [`finish_direct`].
//!
//! Moved — not duplicated — from `commands/creator/world/kb/service.rs`
//! (v1.189 P1-T3), which stays the cited exemplar for the tables below: they
//! mirror the daemon adapter's status and `[code] message` envelope tables
//! variant for variant, so one domain failure reads identically on either
//! transport.
//!
//! No `EngineOwner`, server probe, Node child or live provider is involved:
//! the raw user home plus [`CoreAccess::DirectWriter`] are the whole authority,
//! and the caller awaits [`finish_direct`] before reporting anything.
//!
//! Opening refuses a selection that names no materialized workspace before the
//! writer pool is admitted ([`require_materialized_workspace`]), so an
//! ephemeral/anonymous identity reports the declared selection refusal instead
//! of a raw migration error or an implicitly created workspace.

use std::path::Path;

use crate::config::{user_home_dir, CliConfig};
use crate::errors::{CliError, Result};
use nexus_contracts::CoreCloseReport;
use nexus_core::{CoreAccess, CoreError, CoreOpenOptions, CoreService};

/// Open the direct-writer core over the raw user home.
///
/// # Errors
///
/// Returns [`CliError::Config`] when the home directory cannot be resolved and
/// the mapped core error ([`map_core_error`]) when no creator/workspace is
/// selected, the selected workspace was never materialized, its path cannot be
/// probed, or the writer pool cannot be admitted.
pub async fn open_direct_core(config: &CliConfig) -> Result<CoreService> {
    let user_home = user_home_dir().map_err(|e| CliError::Config(e.to_string()))?;
    require_materialized_workspace_from_home(config, &user_home)?;
    CoreService::open(CoreOpenOptions {
        user_home,
        access: CoreAccess::DirectWriter,
    })
    .await
    .map_err(map_core_error)
}

/// The same admission, for the entrances that open the **legacy** writer pool
/// instead of the direct core.
///
/// `open_workspace_pool` runs `Schema::init`, which migrates — and therefore
/// creates — the selected workspace `state.db`. An entrance that fronts a core
/// route with that pool (`creator world kb`'s local leaves, including the pack
/// branch, which hands that pool to a core call) therefore owes the pre-flight
/// [`open_direct_core`] runs, before its pool open.
///
/// # Errors
///
/// Returns [`CliError::Config`] when the home directory cannot be resolved and
/// the mapped core error when no creator is selected, the selected workspace
/// was never materialized, or its metadata cannot be read.
pub fn require_materialized_workspace(config: &CliConfig) -> Result<()> {
    let user_home = user_home_dir().map_err(|e| CliError::Config(e.to_string()))?;
    require_materialized_workspace_from_home(config, &user_home)
}

/// Refuse a selection whose workspace was never materialized, before the
/// writer can touch storage.
///
/// [`CoreService::open`] resolves the selected workspace `state.db` and hands
/// it to the guarded writer pool, which migrates — and therefore creates — the
/// file. A selection that names no workspace (the anonymous identity bootstrap
/// activates an ephemeral creator without initializing one) would thus either
/// leak the pool's raw I/O error for the missing directory
/// (`…/state.db.migration.lock: No such file or directory`) or silently
/// materialize a workspace the identity never had. Both are refusals of the
/// same declared class, so this pre-flight reports exactly what
/// `CoreService::open` reports for an unset selection: [`CoreError::AuthRequired`]
/// through the shared mapper, never a storage error.
///
/// The selection facts are the ones this leaf's `config` already carries (the
/// same `config.toml` the core re-reads), and a workspace whose `state.db`
/// exists keeps its own storage errors untouched — this only refuses the
/// never-materialized case.
///
/// The probe reads the metadata instead of asking [`Path::exists`], which
/// answers `false` for **every** failed lookup — so an unreadable `state.db`
/// (a metadata error on the file or an ancestor of an existing, materialized
/// workspace) would be reported as the selection refusal. Only `NotFound` is
/// that refusal; every other metadata error keeps the storage class the core's
/// own open path reports (`database_error: …`), and a readable entry is
/// admitted untouched.
fn require_materialized_workspace_from_home(config: &CliConfig, user_home: &Path) -> Result<()> {
    let db_path = match config.active_creator_id.as_deref() {
        Some(creator_id) => crate::paths::state_db_path(
            user_home,
            creator_id,
            config.workspace_slug_for_creator(creator_id),
        ),
        None => return Err(map_core_error(CoreError::AuthRequired)),
    };
    match std::fs::metadata(&db_path) {
        Ok(_) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            Err(map_core_error(CoreError::AuthRequired))
        }
        Err(err) => Err(map_core_error(CoreError::Internal {
            category: format!("database_error: {}: {err}", db_path.display()),
        })),
    }
}

/// Map a core error to the CLI taxonomy.
///
/// `WorldKbConflict` is deliberately NOT handled here: it needs the caller's
/// `expected_version`, which only the patch leaf knows — that leaf keeps its
/// own `map_patch_error` wrapper around this mapper.
pub fn map_core_error(err: CoreError) -> CliError {
    match err {
        CoreError::Uninitialized | CoreError::AuthRequired => CliError::CreatorNotSelected,
        CoreError::Forbidden { resource } => CliError::Api {
            status: 403,
            message: resource,
        },
        // World-owner scoping denial. Mirrors the daemon adapter
        // (`errors.rs`): `Forbidden { resource: "world {world_id}", reason }`
        // at status 403 — so the direct-core caller sees the same family the
        // HTTP route emits.
        CoreError::WorldOwnerDenied { world_id, reason } => CliError::Api {
            status: 403,
            message: format!("world {world_id}: {reason}"),
        },
        // Preset/strategy authoring failures surfaced through the Core
        // authority. Mirrors the daemon adapter's per-variant status and
        // `[code] message` envelope, which is exactly what
        // `DaemonClient::parse_error_response` renders for the `preset`
        // command family over the wire — one preset error must not read
        // differently depending on which transport reached it.
        CoreError::Preset(error) => map_preset_error(error),
        // The daemon adapter renders this class as `[not_found] Not found:
        // {resource}` at 404 (`NexusApiError::NotFound`), and
        // `DaemonClient::parse_error_response` is what every HTTP caller read.
        // A direct-core caller keeps the same named code and wording, so the
        // deterministic error classification does not change with the
        // transport that reached the same core failure.
        CoreError::NotFound { resource } => CliError::Api {
            status: 404,
            message: format!("[not_found] Not found: {resource}"),
        },
        CoreError::InvalidInput { field, reason } => {
            CliError::Other(format!("invalid input ({field}): {reason}"))
        }
        // Compute input validation (V1.147 P3 F2): the core carries the
        // structured per-entry object and the daemon renders it as a 422
        // `invalid_input` envelope, so a direct-core caller reports the same
        // status and code with the object verbatim.
        CoreError::InputValidation { details } => CliError::Api {
            status: coded_status("invalid_input"),
            message: format!("[invalid_input] input validation failed: {details}"),
        },
        CoreError::WorldKbConflict(conflict) => CliError::WorldKbConflict {
            current_version: conflict.current_version,
            expected_version: conflict.current_version,
            entity_id: conflict.entity_id,
            conflicting_path: conflict.conflicting_path,
            recovery_hint: conflict.recovery_hint,
        },
        CoreError::WorldKbValidation(v) => CliError::Api {
            status: 422,
            message: format!(
                "world_kb_validation_failed: {}",
                v.validation_summary.errors.join("; ")
            ),
        },
        CoreError::OutlineConflict(conflict) => CliError::Api {
            status: 409,
            message: format!(
                "outline_conflict: current_revision {}, node_id {}, conflicting_path {}, recovery_hint {}",
                conflict.current_revision, conflict.node_id, conflict.conflicting_path, conflict.recovery_hint
            ),
        },
        CoreError::OutlineValidation(v) => CliError::Api {
            status: 422,
            message: format!(
                "outline_validation_failed: {}",
                v.errors.join("; ")
            ),
        },
        CoreError::OwnerBusy | CoreError::Busy => CliError::Locked {
            holder_pid: 0,
            holder_name: "workspace writer".to_string(),
            stale: false,
        },
        // A coded refusal carries the transport-retained lowercase code. The
        // daemon adapter renders it at the status its own table assigns; a
        // direct-core caller sees the same code and family.
        CoreError::Coded { code, message } => CliError::Api {
            status: coded_status(&code),
            message: format!("[{code}] {message}"),
        },
        // A peer's refusal: the daemon renders it through its own
        // `PeerToolDenied` variant at 400, with the peer's finer wire code in
        // `details.wire_code`. A direct-core caller gets the same public code
        // and status, and the wire code is kept in the message so it is not
        // silently dropped — mirroring how `DaemonClient` surfaces it.
        CoreError::PeerDenied {
            code,
            wire_code,
            message,
        } => CliError::Api {
            status: 400,
            message: format!("[{code}] {message} (peer code: {wire_code})"),
        },
        CoreError::WriterFenced | CoreError::SchemaMismatch => CliError::Config(
            "workspace writer protocol mismatch — upgrade or restart host".to_string(),
        ),
        // Retained bearer-memory / directive wire shapes (v1.190 P2-T2/T3).
        // Each mirrors the daemon adapter's status and envelope
        // (`api/errors.rs`): a reason-carrying 403, the truthful 503, the
        // narrative-quality 422, the plain 409, and the coded actor
        // conflict/invalid-input families.
        CoreError::ForbiddenReason { resource, reason } => CliError::Api {
            status: 403,
            message: format!("{resource}: {reason}"),
        },
        CoreError::ServiceUnavailable(message) => CliError::Api {
            status: 503,
            message,
        },
        CoreError::NarrativeRejected(message) => CliError::Api {
            status: 422,
            message,
        },
        CoreError::Conflict(message) => CliError::Api {
            status: 409,
            message,
        },
        CoreError::ActorConflict { code, message } => CliError::Api {
            status: 409,
            message: format!("[{code}] {message}"),
        },
        CoreError::ActorInput(message) => CliError::Api {
            status: coded_status("invalid_input"),
            message,
        },
        CoreError::Closing | CoreError::Interrupted => CliError::Other(err.to_string()),
        CoreError::Internal { category } => CliError::Other(category),
    }
}

/// The HTTP status the daemon adapter assigns to a coded refusal.
///
/// Mirrors the daemon's own tables (`api/errors.rs`): `conflict` is a 409;
/// the semantic-validation and compute-budget codes are 422; everything else
/// falls back to 400. Keeps one coded refusal reading the same on either
/// transport.
fn coded_status(code: &str) -> u16 {
    match code {
        "conflict" => 409,
        "invalid_state"
        | "invalid_transition"
        | "invalid_input"
        | "world_id_required"
        | "invalid_world_id"
        | "world_clear_forbidden"
        | "too_many_findings"
        | "strategy_self_loop"
        | "strategy_transition_duplicate"
        | "compute_fuel_exhausted"
        | "compute_wall_time_exceeded"
        | "compute_memory_cap_exceeded"
        | "compute_module_trapped"
        | "compute_module_error" => 422,
        "policy_blocked" => 403,
        // Every other carrier (including `not_supported`) is a client-fault 400.
        _ => 400,
    }
}

/// Preset/strategy error mapping, mirroring the daemon adapter's
/// `From<PresetError> for NexusApiError` variant for variant.
///
/// The `preset` command family reaches the same domain failures over the
/// daemon HTTP transport, where `DaemonClient::parse_error_response` renders
/// the daemon's `[code] message` envelope into `CliError::Api`. Reusing that
/// shape here keeps one preset error reading identically on both transports.
/// `StrategyConflict` stays structured (`[strategy_conflict]` + named fields,
/// AR-83 #5 / PL-5) instead of being flattened into the generic envelope.
fn map_preset_error(error: nexus_core::PresetError) -> CliError {
    use nexus_core::PresetError;
    match error {
        // The daemon adapter routes `Rejected` to `BadRequest { code, message }`,
        // whose status and public code are both code-driven. Mirroring the same
        // two tables keeps the outcome identical on either transport.
        PresetError::Rejected { code, message } => CliError::Api {
            status: preset_rejected_status(&code),
            message: format!("[{}] Bad request: {message}", preset_rejected_code(&code)),
        },
        // `InvalidInput { reason }` displays without the field; the daemon's
        // `details.field` is appended by `parse_error_response`.
        PresetError::InvalidInput { field, reason } => CliError::Api {
            status: 400,
            message: format!("[invalid_input] Invalid input: {reason} (field: {field})"),
        },
        PresetError::NotFound(resource) => CliError::Api {
            status: 404,
            message: format!("[not_found] Not found: {resource}"),
        },
        PresetError::Conflict(message) => CliError::Api {
            status: 409,
            message: format!("[conflict] Conflict: {message}"),
        },
        // `Forbidden { resource, reason }` displays only the reason; `resource`
        // travels in `details` (not rendered by the CLI envelope).
        PresetError::Forbidden { reason, .. } => CliError::Api {
            status: 403,
            message: format!("[forbidden] Forbidden: {reason}"),
        },
        // `Internal` always reports the public code `internal` — the inner
        // `code` is internal classification, deliberately not leaked.
        PresetError::Internal { message, .. } => CliError::Api {
            status: 500,
            message: format!("[internal] Internal error: {message}"),
        },
        // The 409 CAS conflict stays structured: code + the four named fields
        // the daemon envelope carries, rendered exactly as
        // `parse_error_response` renders them for the daemon transport
        // (AR-83 #5 / PL-5 — never swallowed).
        PresetError::StrategyConflict(conflict) => {
            let current_revision = conflict.current_revision;
            let node_id = conflict.node_id;
            let conflicting_path = conflict.conflicting_path;
            let recovery_hint = conflict.recovery_hint;
            CliError::Api {
                status: 409,
                message: format!(
                    "[strategy_conflict] Strategy conflict: {conflicting_path} \
                     (current_revision: {current_revision}) (node_id: {node_id}) \
                     (conflicting_path: {conflicting_path}) (recovery_hint: {recovery_hint})"
                ),
            }
        }
        PresetError::StrategyValidation(summary) => CliError::Api {
            status: 422,
            message: {
                use std::fmt::Write as _;
                let mut message =
                    String::from("[strategy_validation_failed] Strategy validation failed");
                for error in &summary.errors {
                    let _ = write!(message, " (validation: {error})");
                }
                message
            },
        },
    }
}

/// HTTP status for a `PresetError::Rejected` code, mirroring the daemon
/// adapter's `BadRequest` table (`status_code`): semantic validation codes are
/// 422; everything else is 400. `policy_blocked` → 403 is unreachable here —
/// preset rejection never emits it.
fn preset_rejected_status(code: &str) -> u16 {
    match code {
        "world_id_required"
        | "invalid_world_id"
        | "world_clear_forbidden"
        | "invalid_transition"
        | "invalid_input"
        | "invalid_state"
        | "too_many_findings"
        | "strategy_self_loop"
        | "strategy_transition_duplicate" => 422,
        _ => 400,
    }
}

/// Public error code for a `PresetError::Rejected` code, mirroring the daemon
/// adapter's `error_code` table: canonical codes surface verbatim; anything
/// else degrades to `bad_request` so internal classification never leaks.
fn preset_rejected_code(code: &str) -> &str {
    match code {
        "policy_blocked"
        | "not_supported"
        | "invalid_input"
        | "invalid_state"
        | "invalid_transition"
        | "too_many_findings"
        | "world_id_required"
        | "invalid_world_id"
        | "world_clear_forbidden"
        | "strategy_self_loop"
        | "strategy_transition_duplicate" => code,
        _ => "bad_request",
    }
}

/// Close `core` and resolve the caller's `outcome` against the close report.
///
/// The close is awaited on BOTH paths: a failed admission or mutation is not a
/// reason to keep the workspace writer admitted, and the release happens in
/// this process before the caller can report anything.
///
/// A cleanup that did not settle — a failed close, or a report that does not
/// confirm `cleanup_confirmed` — is printed on stderr without replacing the
/// operation's own error; a successful operation is refused instead, because a
/// command may not report an outcome it could not settle. [`resolve_direct`]
/// owns that precedence table.
///
/// Callers render only after this returns, so nothing is printed ahead of a
/// close that failed.
pub async fn finish_direct<T>(core: &CoreService, outcome: Result<T>) -> Result<T> {
    let (result, warning) = resolve_direct(outcome, core.close().await.map_err(map_core_error));
    if let Some(warning) = warning {
        eprintln!("warning: {warning}");
    }
    result
}

/// Resolve an operation's `outcome` against the result of closing its core.
///
/// Returns the command result plus an optional cleanup warning for the caller
/// to print. One precedence rule orders every combination: the operation's own
/// error stays primary (cleanup is reported alongside it, never substituted for
/// it), a close that fails is itself the outcome of a successful operation, and
/// an incomplete close report never yields success.
fn resolve_direct<T>(
    outcome: Result<T>,
    closed: Result<CoreCloseReport>,
) -> (Result<T>, Option<String>) {
    match (outcome, closed) {
        (Ok(value), Ok(report)) if report.cleanup_confirmed => (Ok(value), None),
        // Success is refused: the report says the core did not settle.
        (Ok(_), Ok(report)) => (Err(CliError::Other(incomplete_cleanup(&report))), None),
        // A close that fails IS the command's outcome: it cannot claim success.
        (Ok(_), Err(close_error)) => (Err(close_error), None),
        (Err(primary), Ok(report)) if report.cleanup_confirmed => (Err(primary), None),
        // The failed operation stays primary; incomplete cleanup is reported.
        (Err(primary), Ok(report)) => (Err(primary), Some(incomplete_cleanup(&report))),
        // Same precedence for a close that failed outright.
        (Err(primary), Err(close_error)) => (
            Err(primary),
            Some(format!("direct core cleanup failed: {close_error}")),
        ),
    }
}

/// The one wording for an incomplete close report, shared by the refusal that
/// replaces success and the warning that accompanies a failed operation.
fn incomplete_cleanup(report: &CoreCloseReport) -> String {
    format!(
        "direct core close reported incomplete cleanup: pending operations {:?}",
        report.pending_operations
    )
}

/// The close/operation precedence table, without a database.
#[cfg(test)]
mod tests {
    use super::resolve_direct;
    use crate::errors::CliError;
    use nexus_contracts::{CoreCloseReport, CoreCloseReportState};

    /// A close report that does not confirm cleanup, naming one pending
    /// operation so the report is actionable.
    fn incomplete_report() -> CoreCloseReport {
        CoreCloseReport {
            state: CoreCloseReportState::Closed,
            cleanup_confirmed: false,
            pending_operations: vec!["flush_outbox".to_string()],
            reason: None,
        }
    }

    /// A close report that settled.
    fn confirmed_report() -> CoreCloseReport {
        CoreCloseReport {
            state: CoreCloseReportState::Closed,
            cleanup_confirmed: true,
            pending_operations: Vec::new(),
            reason: None,
        }
    }

    fn primary() -> CliError {
        CliError::Other("primary operation failed".to_string())
    }

    /// A failed operation stays the primary error, and the cleanup it could not
    /// settle is still reported (frozen contract §2).
    #[test]
    fn incomplete_cleanup_is_reported_without_replacing_the_primary_error() {
        let (result, warning) = resolve_direct::<u32>(Err(primary()), Ok(incomplete_report()));

        assert_eq!(
            result
                .expect_err("the operation error stays primary")
                .to_string(),
            "primary operation failed"
        );
        let warning = warning.expect("incomplete cleanup is reported");
        assert!(
            warning.contains("flush_outbox"),
            "the report names the pending operation: {warning}"
        );
    }

    /// The same precedence when the close itself failed.
    #[test]
    fn failed_cleanup_is_reported_without_replacing_the_primary_error() {
        let (result, warning) = resolve_direct::<u32>(
            Err(primary()),
            Err(CliError::Other("pool close failed".to_string())),
        );

        assert_eq!(
            result
                .expect_err("the operation error stays primary")
                .to_string(),
            "primary operation failed"
        );
        let warning = warning.expect("the failed cleanup is reported");
        assert!(warning.contains("pool close failed"), "{warning}");
    }

    /// A successful operation may not report success it could not settle; a
    /// confirmed close returns the value untouched.
    #[test]
    fn an_unsettled_close_refuses_success_that_a_confirmed_close_returns() {
        let (refused, warning) = resolve_direct(Ok(7_u32), Ok(incomplete_report()));
        let refused = refused.expect_err("success over an unsettled close is refused");
        assert!(
            refused.to_string().contains("flush_outbox"),
            "the refusal carries the report: {refused}"
        );
        assert!(warning.is_none(), "nothing is left to report: {warning:?}");

        let (failed, warning) =
            resolve_direct::<u32>(Ok(7), Err(CliError::Other("pool close failed".to_string())));
        assert_eq!(
            failed
                .expect_err("a failed close is the command's outcome")
                .to_string(),
            "pool close failed"
        );
        assert!(warning.is_none(), "nothing is left to report: {warning:?}");

        let (value, warning) = resolve_direct(Ok(7_u32), Ok(confirmed_report()));
        assert_eq!(value.expect("a confirmed close returns the value"), 7);
        assert!(warning.is_none(), "nothing is left to report: {warning:?}");
    }
}

/// The direct writer is released **in this process**, before the seam returns.
///
/// The CLI journey in `tests/direct_core_lifecycle.rs` observes reopen after a
/// rejected mutation across process boundaries, where a leaked writer would be
/// released by process exit anyway. This module closes that gap: it holds the
/// seam in-process and then asks for the store's exclusive fence, which is
/// grantable only when no writer this process admitted is still live.
///
/// In-crate test module of the `cli` cohort (this module is compiled for that
/// cohort only): the direct-writer store stack (`nexus-local-db`) is what the
/// cohort's features link.
#[cfg(test)]
mod direct_writer_lifetime {
    use super::{finish_direct, map_core_error};
    use crate::errors::CliError;
    use nexus_contracts::world_kb_patch_entity_request::{
        NexusWorldKbEntityPatch, NexusWorldKbEntityPatchBlockType, NexusWorldKbEntityPatchTitle,
    };
    use nexus_contracts::{
        CoreRegisterCreatorRequest, CreateWorldRequest, SetActiveWorkspaceRequest,
        WorldKbPatchEntityRequest,
    };
    use nexus_core::{CoreAccess, CoreError, CoreHomeService, CoreOpenOptions, CoreService};
    use nexus_home_layout::{operational_workspace_dir, workspace_state_db_path};
    use nexus_local_db::writer_protocol::{
        acquire_store_reset_fence, release_retained_writer_guards,
    };
    use std::path::{Path, PathBuf};

    const WORKSPACE_SLUG: &str = "default";
    const ENTITY_ID: &str = "kb_0f1e2d3c4b5a";
    const SEEDED_TITLE: &str = "Seeded Hero";
    const STALE_TITLE: &str = "Stale Overwrite";
    const REOPENED_TITLE: &str = "Reopened Hero";

    /// An isolated raw home with one selected creator/workspace and no writer
    /// left in this process: selection retains its own guard, so the seed
    /// writer is released explicitly rather than left to `Drop`.
    async fn selected_home() -> (tempfile::TempDir, PathBuf) {
        let home = tempfile::tempdir().expect("temp home");
        let user_home = home.path().to_path_buf();
        let selector = CoreHomeService::open(user_home.clone()).expect("home entry opens");

        let creator = selector
            .register_creator(CoreRegisterCreatorRequest {
                display_name: Some(
                    "Direct Lifetime Author"
                        .parse()
                        .expect("valid display name"),
                ),
                platform_creator_id: None,
            })
            .await
            .expect("register creator");
        let creator_id = creator.creator_id;
        std::fs::create_dir_all(operational_workspace_dir(
            &user_home,
            &creator_id,
            WORKSPACE_SLUG,
        ))
        .expect("materialize workspace dir");
        selector
            .select_workspace(SetActiveWorkspaceRequest {
                creator_id: Some(creator_id.clone()),
                workspace_slug: WORKSPACE_SLUG.to_string(),
            })
            .await
            .expect("select workspace");

        let db_path = workspace_state_db_path(&user_home, &creator_id, WORKSPACE_SLUG);
        release_retained_writer_guards(&db_path);
        (home, db_path)
    }

    async fn open_writer(user_home: &Path) -> CoreService {
        CoreService::open(CoreOpenOptions {
            user_home: user_home.to_path_buf(),
            access: CoreAccess::DirectWriter,
        })
        .await
        .expect("the direct writer opens on the seeded home")
    }

    fn patch(expected_version: u64, title: &str) -> WorldKbPatchEntityRequest {
        WorldKbPatchEntityRequest {
            entity_id: ENTITY_ID.to_string(),
            expected_version,
            patch: NexusWorldKbEntityPatch {
                title: Some(
                    NexusWorldKbEntityPatchTitle::try_from(title.to_string())
                        .expect("valid canonical name"),
                ),
                block_type: Some(NexusWorldKbEntityPatchBlockType::Character),
                ..NexusWorldKbEntityPatch::default()
            },
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn rejected_mutation_releases_the_direct_writer_in_process() {
        let (home, db_path) = selected_home().await;
        let core = open_writer(home.path()).await;

        // Positive control: the probe below observes a live writer, so its
        // later success cannot be vacuous.
        assert!(
            acquire_store_reset_fence(&db_path).is_err(),
            "a live direct writer refuses the store's exclusive fence"
        );

        let principal = core.active_principal().await.expect("active principal");
        let world_id = core
            .create_world(
                &principal,
                serde_json::from_value::<CreateWorldRequest>(
                    serde_json::json!({ "title": "Direct Lifetime" }),
                )
                .expect("world request shape"),
            )
            .await
            .expect("create world")
            .world_id;
        let seeded = core
            .patch_world_kb_entity(&principal, world_id.clone(), patch(0, SEEDED_TITLE))
            .await
            .expect("seed entity");
        assert!(seeded.version > 0, "a created entity settles at a revision");

        // A stale CAS is rejected by the core; the seam must still close.
        let outcome = core
            .patch_world_kb_entity(
                &principal,
                world_id.clone(),
                patch(seeded.version - 1, STALE_TITLE),
            )
            .await
            .map_err(map_core_error);
        let Err(rejected) = finish_direct(&core, outcome).await else {
            panic!("a stale expected_version is rejected");
        };
        assert!(
            matches!(rejected, CliError::WorldKbConflict { .. }),
            "the rejected operation stays the primary error: {rejected:?}"
        );
        assert!(
            matches!(core.active_principal().await, Err(CoreError::Closing)),
            "the core is closed when finish_direct returns"
        );

        // The observation this module exists for. `close` released the
        // process-local writer, so dropping the closed service leaves no live
        // admission on the store and the exclusive fence is granted. A rejected
        // path that skipped the close would leave the guard retained and keep
        // this refused — for the life of the process, not just this call.
        drop(core);
        let fence = acquire_store_reset_fence(&db_path)
            .expect("finish_direct released the direct writer in this process");
        drop(fence);

        // Nothing changed on the rejected path: the next same-process writer
        // commits the revision the seeded one left behind.
        let reopened = open_writer(home.path()).await;
        let principal = reopened.active_principal().await.expect("active principal");
        let committed = reopened
            .patch_world_kb_entity(
                &principal,
                world_id.clone(),
                patch(seeded.version, REOPENED_TITLE),
            )
            .await
            .expect("a writer reopens after a rejected mutation");
        assert_eq!(
            committed.version,
            seeded.version + 1,
            "the reopened writer commits the next revision"
        );
        finish_direct(&reopened, Ok(committed))
            .await
            .expect("the success path reports the committed outcome");
    }
}
