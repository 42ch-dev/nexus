//! World KB direct-core service invocation — `creator world kb graph` +
//! `entity patch`.
//!
//! v1.189 P1-T3 (architecture §2.1): declarations and formatting live in the
//! parent KB module; this file owns ONLY the `nexus-core` invocation wiring.
//! Both cohorts take the same `CoreAccess::DirectWriter` path — no daemon HTTP
//! for these verbs.

use super::{render_graph_response, render_patch_response, BlockTypeArg};
use crate::config::{user_home_dir, CliConfig};
use crate::errors::{CliError, Result};
use nexus_contracts::{
    world_kb_patch_entity_request::{
        NexusWorldKbEntityPatch, NexusWorldKbEntityPatchModulesKey,
        NexusWorldKbEntityPatchModulesValue, NexusWorldKbEntityPatchTitle,
    },
    WorldKbPatchEntityRequest,
};
use nexus_core::{CoreAccess, CoreError, CoreOpenOptions, CoreService};
use std::collections::HashMap;

async fn open_direct_core(_config: &CliConfig) -> Result<CoreService> {
    let user_home = user_home_dir().map_err(|e| CliError::Config(e.to_string()))?;
    CoreService::open(CoreOpenOptions {
        user_home,
        access: CoreAccess::DirectWriter,
    })
    .await
    .map_err(map_core_error)
}

/// Map a core error to the CLI taxonomy.
///
/// `WorldKbConflict` is deliberately NOT handled here: it needs the caller's
/// `expected_version`, which only the patch leaf knows. See
/// [`map_patch_error`].
fn map_core_error(err: CoreError) -> CliError {
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
        CoreError::NotFound { resource } => CliError::Api {
            status: 404,
            message: resource,
        },
        CoreError::InvalidInput { field, reason } => {
            CliError::Other(format!("invalid input ({field}): {reason}"))
        }
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

/// Patch-leaf error mapping: the OCC conflict carries the caller's expected
/// version alongside the row's current version, so the reported pair is
/// actionable (`expected v1, current v2`) instead of echoing the current
/// version twice.
fn map_patch_error(err: CoreError, expected_version: u64) -> CliError {
    match err {
        CoreError::WorldKbConflict(conflict) => CliError::WorldKbConflict {
            current_version: conflict.current_version,
            expected_version,
            entity_id: conflict.entity_id,
            conflicting_path: conflict.conflicting_path,
            recovery_hint: conflict.recovery_hint,
        },
        other => map_core_error(other),
    }
}

/// Run `creator world kb entity patch`.
///
/// # Errors
///
/// Named, non-zero exit for every failure family: invalid input (no patch
/// field / unparseable `--body` / `--modules` / `--title`), and the core error
/// taxonomy — `world_kb_conflict` (exit 76), `world_kb_validation_failed`
/// (422), `not_found` (404), `forbidden` (403), writer-busy (exit 75).
#[allow(clippy::too_many_arguments)]
pub async fn run_entity_patch(
    config: &CliConfig,
    world_id: String,
    entity_id: String,
    expected_version: u64,
    title: Option<String>,
    body: Option<String>,
    aliases: Option<Vec<String>>,
    block_type: Option<BlockTypeArg>,
    modules: Option<String>,
    json: bool,
) -> Result<()> {
    let mut patch = NexusWorldKbEntityPatch {
        aliases: aliases.unwrap_or_default(),
        block_type: block_type.map(BlockTypeArg::to_generated),
        body: serde_json::Map::new(),
        modules: HashMap::new(),
        title: None,
    };
    if let Some(body_str) = body {
        let value: serde_json::Value = serde_json::from_str(&body_str)
            .map_err(|e| CliError::Other(format!("--body must be a JSON object: {e}")))?;
        let obj = value
            .as_object()
            .ok_or_else(|| CliError::Other("--body must be a JSON object".to_string()))?;
        patch.body.clone_from(obj);
    }
    if let Some(modules_str) = modules {
        let value: serde_json::Value = serde_json::from_str(&modules_str)
            .map_err(|e| CliError::Other(format!("--modules must be a JSON object: {e}")))?;
        let obj = value
            .as_object()
            .ok_or_else(|| CliError::Other("--modules must be a JSON object".to_string()))?;
        for (key, val) in obj {
            let k = key
                .parse::<NexusWorldKbEntityPatchModulesKey>()
                .map_err(|e| {
                    CliError::Other(format!(
                        "--modules key '{key}' is invalid (must match ^[a-z][a-z0-9_-]*$): {e}"
                    ))
                })?;
            let v = match val {
                serde_json::Value::Object(m) => {
                    NexusWorldKbEntityPatchModulesValue::Object(m.clone())
                }
                serde_json::Value::Array(a) => {
                    NexusWorldKbEntityPatchModulesValue::Array(a.clone())
                }
                _ => {
                    return Err(CliError::Other(format!(
                        "--modules value for '{key}' must be an object or array"
                    )));
                }
            };
            patch.modules.insert(k, v);
        }
    }
    if let Some(title_str) = title {
        patch.title = Some(
            title_str
                .parse::<NexusWorldKbEntityPatchTitle>()
                .map_err(|e| CliError::Other(format!("--title is invalid: {e}")))?,
        );
    }
    if patch.title.is_none()
        && patch.body.is_empty()
        && patch.aliases.is_empty()
        && patch.block_type.is_none()
        && patch.modules.is_empty()
    {
        return Err(CliError::Other(
            "at least one of --title/--body/--aliases/--block-type/--modules must be provided"
                .to_string(),
        ));
    }

    let req = WorldKbPatchEntityRequest {
        entity_id: entity_id.clone(),
        expected_version,
        patch,
    };

    let core = open_direct_core(config).await?;
    let principal = core.active_principal().await.map_err(map_core_error)?;
    let resp = core
        .patch_world_kb_entity(&principal, world_id.clone(), req)
        .await
        .map_err(|e| map_patch_error(e, expected_version))?;
    core.close().await.map_err(map_core_error)?;

    render_patch_response(&world_id, &entity_id, &resp, json)
}

/// Run `creator world kb graph`.
///
/// # Errors
/// Returns [`CliError`] when the direct-writer core cannot be opened, the
/// active creator/workspace is missing or stale
/// ([`CliError::CreatorNotSelected`]), the graph query fails, or closing the
/// core pool fails.
pub async fn run_graph(
    config: &CliConfig,
    world_id: String,
    include_suggested: bool,
    json: bool,
) -> Result<()> {
    let core = open_direct_core(config).await?;
    let principal = core.active_principal().await.map_err(map_core_error)?;
    let resp = core
        .world_kb_graph(&principal, world_id.clone(), include_suggested)
        .await
        .map_err(map_core_error)?;
    core.close().await.map_err(map_core_error)?;

    render_graph_response(&world_id, &resp, json)
}
