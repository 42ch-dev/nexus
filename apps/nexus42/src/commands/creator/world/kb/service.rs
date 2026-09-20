use super::{render_graph_response, render_patch_response, BlockTypeArg};
use crate::config::CliConfig;
use crate::core::{finish_direct, map_core_error, open_direct_core};
use crate::errors::{CliError, Result};
use nexus_contracts::{
    world_kb_patch_entity_request::{
        NexusWorldKbEntityPatch, NexusWorldKbEntityPatchAudience,
        NexusWorldKbEntityPatchAudienceCharacterPrivate, NexusWorldKbEntityPatchModulesKey,
        NexusWorldKbEntityPatchModulesValue, NexusWorldKbEntityPatchTitle,
    },
    WorldKbPatchEntityRequest,
};
use nexus_core::CoreError;
use std::collections::HashMap;

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
/// Map the closed `--audience` / `--audience-character` pair onto the frozen
/// wire audience (`world-kb-entity-patch` schema). The author supplies intent
/// only: core admission resolves the permitted identity and its holder against
/// stored state inside the authoring transaction.
///
/// # Errors
/// Returns [`CliError`] for an unknown audience, a `character-private` request
/// without `--audience-character`, or a mismatched pair.
pub fn audience_wire(
    audience: Option<&str>,
    character_id: Option<&str>,
) -> Result<Option<NexusWorldKbEntityPatchAudience>> {
    let Some(audience) = audience else {
        if character_id.is_some() {
            return Err(CliError::Other(
                "--audience-character requires --audience character-private".into(),
            ));
        }
        return Ok(None);
    };
    if audience != "character-private" && character_id.is_some() {
        return Err(CliError::Other(format!(
            "--audience-character is only meaningful with --audience character-private, \
             not '{audience}'"
        )));
    }
    match audience {
        "shared" => Ok(Some(NexusWorldKbEntityPatchAudience::Shared)),
        "author-only" => Ok(Some(NexusWorldKbEntityPatchAudience::AuthorOnly)),
        "character-private" => {
            let character_id = character_id.ok_or_else(|| {
                CliError::Other(
                    "--audience character-private requires --audience-character <CHARACTER_ID>"
                        .into(),
                )
            })?;
            let character_id =
                NexusWorldKbEntityPatchAudienceCharacterPrivate::try_from(character_id.to_string())
                    .map_err(|e| {
                        CliError::Other(format!("--audience-character is invalid: {e}"))
                    })?;
            Ok(Some(NexusWorldKbEntityPatchAudience::CharacterPrivate(
                character_id,
            )))
        }
        other => Err(CliError::Other(format!(
            "unknown --audience {other}; expected shared, author-only, or character-private"
        ))),
    }
}

/// # Errors
///
/// Returns [`CliError`] when the active creator/workspace cannot be resolved,
/// the core refuses the patch (admission, validation, `world_kb_conflict`, or
/// the linked-`WorldSheet` guard), or the request/response cannot be
/// serialized.
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
    audience: Option<NexusWorldKbEntityPatchAudience>,
    json: bool,
) -> Result<()> {
    let mut patch = NexusWorldKbEntityPatch {
        aliases: aliases.unwrap_or_default(),
        block_type: block_type.map(BlockTypeArg::to_generated),
        body: serde_json::Map::new(),
        modules: HashMap::new(),
        title: None,
        audience,
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
        && patch.audience.is_none()
    {
        return Err(CliError::Other(
            "at least one of --title/--body/--aliases/--block-type/--modules/--audience must be \
             provided"
                .to_string(),
        ));
    }

    let req = WorldKbPatchEntityRequest {
        entity_id: entity_id.clone(),
        expected_version,
        patch,
    };

    let core = open_direct_core(config).await?;
    // Admission and the mutation share one outcome: whichever fails, the
    // writer is closed before this command reports.
    let outcome = async {
        let principal = core.active_principal().await.map_err(map_core_error)?;
        core.patch_world_kb_entity(&principal, world_id.clone(), req)
            .await
            .map_err(|e| map_patch_error(e, expected_version))
    }
    .await;
    let resp = finish_direct(&core, outcome).await?;

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
    let outcome = async {
        let principal = core.active_principal().await.map_err(map_core_error)?;
        core.world_kb_graph(&principal, world_id.clone(), include_suggested)
            .await
            .map_err(map_core_error)
    }
    .await;
    let resp = finish_direct(&core, outcome).await?;

    render_graph_response(&world_id, &resp, json)
}
