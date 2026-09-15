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
        CoreError::OwnerBusy | CoreError::Busy => CliError::Locked {
            holder_pid: 0,
            holder_name: "workspace writer".to_string(),
            stale: false,
        },
        CoreError::WriterFenced | CoreError::SchemaMismatch => CliError::Config(
            "workspace writer protocol mismatch — upgrade or restart host".to_string(),
        ),
        CoreError::Closing | CoreError::Interrupted => CliError::Other(err.to_string()),
        CoreError::Internal { category } => CliError::Other(category),
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
