//! Capability dispatch: the typed tool surface and its spine (v1.190 P3-T3).
//!
//! Moved out of the daemon's `api/handlers/host_tool_executor.rs` +
//! `host_tool_handlers.rs` + `capability_registry.rs` so a non-HTTP host (the
//! TS service, an independent product) drives the SAME dispatch authority
//! instead of a second registry.
//!
//! # The spine
//!
//! One table resolves a tool id, in order:
//!
//! 1. a static `nexus.*` builtin row,
//! 2. a remote peer row ([`crate::execution::peer_tools`]),
//! 3. a user capability in the live registry.
//!
//! An id that resolves nowhere is `not_supported` with ZERO side effects —
//! the same refusal an unknown builtin yields. There is exactly one spine; a
//! second one is how the builtin set and the peer set silently diverge.
//!
//! # What replaced `WorkspaceState`
//!
//! The daemon's handlers took `&WorkspaceState`, a transport aggregate that
//! exposes the pool, the nexus home, the workspace path, the lifecycle HSM
//! and the WASM engine. Core cannot name that type (it lives in the daemon),
//! and a universal JSON/`Value` RPC would dissolve the typed contract.
//!
//! [`ToolContext`] is the narrow alternative: the OWNED inputs each handler
//! actually consumes, already resolved by the caller. A handler sees a pool,
//! a home, a path and a few scalars — never a transport it could reach back
//! into.
//!
//! # Schema-validated arguments
//!
//! Tool arguments stay untyped JSON because that IS the capability contract:
//! a capability declares a JSON Schema and receives arguments matching it.
//! Validation therefore happens against the declared schema (see
//! [`validate_user_cap_arguments`]), never by inventing a Rust type per tool.
//!
//! # Error codes
//!
//! Several retained codes (`policy_blocked` → 403, `not_supported` → 400,
//! `invalid_input` → 422) are part of the public contract and are carried on
//! [`CoreError::Coded`]; the adapter renders the status from its own table, so
//! one refusal reads identically on every transport.

#![allow(clippy::missing_errors_doc)]
#![allow(clippy::too_many_lines)]

use crate::content::resolve_guarded_path_async;
use crate::error::{CoreError, CoreResult};
// The handlers below were extracted from the daemon, where the refusal type is
// named `NexusApiError`. It IS the same type as `CoreError` — the daemon's
// adapter renders the HTTP envelope — so the alias keeps the extracted bodies
// readable without a 200-site rename that would obscure the diff.
use crate::error::CoreError as NexusApiError;
use crate::service::CoreService;
// The request DTOs live at the crate root, not in `works`.
use crate::{
    ArchivePoolRequest, PromotePoolRequest, UpdateFindingRequest, WorkDetails, WorkPatchRequest,
};
use nexus_home_layout::active_context::{read_active_creator_id, read_active_workspace_slug};
use nexus_knowledge::world_kb::KbStore;
use nexus_local_db::works;
use nexus_narrative::NarrativeGateway;
use nexus_spoke_adapter::{parse_tool_capability_id, SpokeResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::LazyLock;

/// A tool invocation request.
///
/// Shared by HTTP, the internal agent-host route and schedule dispatch.
///
/// NOTE: this type is currently MOVED from the daemon verbatim (P3-T3
/// decision (B)) because its schema destination
/// (`schemas/core/tools-api.schema.json`) does not exist yet. When the schema
/// lane publishes it, this definition is replaced by the generated type and
/// the move is deleted — tracked as a binding obligation in the task report.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ToolExecuteRequest {
    /// The tool id to dispatch.
    pub tool_name: String,
    /// Tool arguments (validated against the capability's declared schema).
    pub parameters: serde_json::Value,
    /// Session this call belongs to, when the caller has one.
    #[serde(default)]
    pub session_id: Option<String>,
    /// Caller-supplied request id, for audit correlation.
    #[serde(default)]
    pub request_id: Option<String>,
    /// Who is calling, when it is not the ordinary HTTP path.
    #[serde(default)]
    pub caller_kind: Option<HostToolCallerKind>,
}

/// Who is calling the tool registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostToolCallerKind {
    /// An in-process schedule tick.
    Schedule,
}

impl std::fmt::Display for HostToolCallerKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Schedule => write!(f, "schedule"),
        }
    }
}

/// The tool result envelope.
#[derive(Debug, serde::Serialize)]
pub struct ToolExecuteResponse {
    /// Whether the handler succeeded.
    pub success: bool,
    /// The handler's JSON result.
    pub result: serde_json::Value,
}

/// Fields allowed in `nexus.work.patch`.
pub const PATCH_ALLOWED_FIELDS: &[&str] = &["title", "inspiration_log", "stage_metadata"];

/// Fields explicitly rejected in `nexus.work.patch`.
pub const PATCH_REJECTED_FIELDS: &[&str] = &[
    "current_stage",
    "stage",
    "stage_status",
    "stage_started_at",
    "stage_completed_at",
    "creator_id",
    "workspace_id",
    "work_id",
    "run_intents",
];

/// Sub-fields allowed inside `stage_metadata`.
pub const STAGE_METADATA_ALLOWED_KEYS: &[&str] = &[
    "agent_notes",
    "research_summary_ref",
    "draft_outline_ref",
    "review_summary_ref",
    "last_agent_tool_request_id",
];

/// Process-level facts the health tools report.
///
/// Supplied by whoever owns the process (the transport at boot). The defaults
/// describe a local-only process that has just started, which is the honest
/// answer for a caller that supplied no facts.
#[derive(Debug, Clone)]
pub struct ToolRuntimeFacts {
    /// The runtime mode.
    pub runtime_mode: nexus_contracts::local::domain::RuntimeMode,
    /// Whether the workspace is initialized.
    pub is_initialized: bool,
    /// The lifecycle state, as its display string.
    pub lifecycle_state: String,
    /// Process start time, RFC 3339.
    pub started_at: String,
    /// Process uptime in seconds.
    pub uptime_seconds: u64,
}

impl Default for ToolRuntimeFacts {
    fn default() -> Self {
        Self {
            runtime_mode: nexus_contracts::local::domain::RuntimeMode::LocalOnly,
            is_initialized: false,
            lifecycle_state: "Starting".to_string(),
            started_at: String::new(),
            uptime_seconds: 0,
        }
    }
}

impl ToolRuntimeFacts {
    /// The runtime mode as its wire string.
    #[must_use]
    pub const fn runtime_mode_as_str(&self) -> &'static str {
        self.runtime_mode.as_str()
    }
}

/// The narrow owned inputs a tool dispatch consumes.
///
/// Every field is an already-resolved value or an already-constructed
/// collaborator — never a transport aggregate. This is what makes the
/// dispatch path usable outside the daemon without either naming
/// `WorkspaceState` or falling back to a universal `Value` RPC.
#[derive(Clone)]
pub struct ToolContext {
    /// The Creator DB pool.
    pub(crate) pool: sqlx::SqlitePool,
    /// The nexus home (`~/.nexus42`), for active-creator/workspace reads.
    pub(crate) nexus_home: std::path::PathBuf,
    /// The active workspace path, when one is attached.
    pub(crate) workspace_path: Option<String>,
    /// Process-level facts the two health tools report.
    ///
    /// These are genuinely process state (runtime mode, HSM state, start
    /// time, uptime, initialization) that a domain service cannot derive.
    /// They are therefore SUPPLIED, never fabricated: a core-only caller that
    /// has no such facts supplies [`ToolRuntimeFacts::default`], and the
    /// health tools report the defaults honestly rather than inventing a
    /// running daemon.
    pub(crate) runtime_facts: ToolRuntimeFacts,
    /// The engine-owner core, when one is open.
    pub(crate) core: Option<Arc<CoreService>>,
    /// The live user-capability registry, when one is published.
    ///
    /// Read LIVE at each dispatch (never cached on the context) so a
    /// hot-reloaded registry is visible to the very next call — the same
    /// discipline the engine uses through the shared holder.
    pub(crate) user_capabilities: Option<nexus_orchestration::CapabilityRegistryHolder>,
}

impl ToolContext {
    /// Compose a tool context from its owned inputs.
    ///
    /// The fields stay crate-private so a caller cannot mutate the authority
    /// under a running dispatch; this constructor is the public composition
    /// point a transport (or a test) uses.
    #[must_use]
    pub const fn new(
        pool: sqlx::SqlitePool,
        nexus_home: std::path::PathBuf,
        workspace_path: Option<String>,
        runtime_facts: ToolRuntimeFacts,
        core: Option<Arc<CoreService>>,
        user_capabilities: Option<nexus_orchestration::CapabilityRegistryHolder>,
    ) -> Self {
        Self {
            pool,
            nexus_home,
            workspace_path,
            runtime_facts,
            core,
            user_capabilities,
        }
    }

    /// Replace the active workspace path.
    ///
    /// The permission policy is read from `<workspace>/.nexus42/permissions.toml`,
    /// so a caller that resolves a different workspace must be able to point
    /// the context at it (a test also uses this to exercise a read-only policy).
    pub fn set_workspace_path(&mut self, path: Option<String>) {
        self.workspace_path = path;
    }

    /// Replace the live user-capability holder.
    ///
    /// A dispatch reads the holder LIVE, so swapping it here is visible to the
    /// very next call — the same hot-reload discipline the engine uses.
    pub fn set_user_capabilities(
        &mut self,
        holder: Option<nexus_orchestration::CapabilityRegistryHolder>,
    ) {
        self.user_capabilities = holder;
    }

    /// The Creator DB pool.
    #[must_use]
    #[allow(clippy::missing_const_for_fn)] // const here would suppress auto-deref for callers
    pub fn pool(&self) -> &sqlx::SqlitePool {
        &self.pool
    }

    /// The active workspace path, when one is attached.
    #[must_use]
    pub fn workspace_path(&self) -> Option<String> {
        self.workspace_path.clone()
    }

    /// The runtime mode value.
    #[must_use]
    pub const fn runtime_mode(&self) -> &nexus_contracts::local::domain::RuntimeMode {
        &self.runtime_facts.runtime_mode
    }

    /// The engine-owner core, or a refusal.
    ///
    /// A tool that needs the family services (Work patch, findings, pool
    /// entries) cannot run without one: reporting a missing core is the honest
    /// failure, not a silent no-op.
    ///
    /// # Errors
    /// `Uninitialized` when no engine-owner core is open.
    pub fn core(&self) -> CoreResult<Arc<CoreService>> {
        self.core.clone().ok_or(CoreError::Uninitialized)
    }

    /// The live user-capability registry, if one is published.
    #[must_use]
    pub fn user_capabilities(&self) -> Option<Arc<nexus_orchestration::CapabilityRegistry>> {
        self.user_capabilities
            .as_ref()
            .and_then(nexus_orchestration::CapabilityRegistryHolder::get)
    }
}

// ---------------------------------------------------------------------------
// Admission and handlers
// ---------------------------------------------------------------------------

/// Dispatch one tool request through the spine and write the audit row.
///
/// The single caller entry point both transports use: the admission pipeline
/// runs first (five gates), then the registry resolves and invokes, then the
/// outcome is audited. A refusal at ANY stage still writes its audit row —
/// an unattempted-looking denial is exactly the accountability hole the audit
/// gate exists to close.
///
/// # Errors
/// The admission refusal at gate, the handler's own refusal, or `Coded`
/// `not_supported` for an id the spine cannot resolve.
pub async fn execute_tool(
    context: &ToolContext,
    request: &ToolExecuteRequest,
) -> CoreResult<serde_json::Value> {
    tracing::info!(
        tool_name = %request.tool_name,
        caller_kind = ?request.caller_kind,
        "tool dispatch through the capability spine"
    );

    let (creator_id, _workspace_slug) = match admission_pipeline(request, context).await {
        Ok(pair) => pair,
        Err(err) => {
            audit_tool_execution(request, "denied", Some(err_code(&err)), context).await?;
            return Err(err);
        }
    };

    let reg = host_tool_registry();
    let result = reg.dispatch(request, context, &creator_id).await;

    match &result {
        Ok(_) => audit_tool_execution(request, "success", None, context).await?,
        Err(err) => {
            audit_tool_execution(request, "denied", Some(err_code(err)), context).await?;
        }
    }

    result
}

/// The audit label for a refusal.
///
/// The retained audit contract records the SAME lowercase code the caller
/// receives (the daemon's `error_code()`), so a `Coded` refusal reports its own
/// code rather than a generic bucket — an operator grepping the audit log for
/// `not_supported` must find it.
fn err_code(err: &CoreError) -> &str {
    match err {
        CoreError::Coded { code, .. } | CoreError::PeerDenied { code, .. } => code,
        CoreError::Forbidden { .. } | CoreError::WorldOwnerDenied { .. } => "forbidden",
        CoreError::NotFound { .. } => "not_found",
        CoreError::InvalidInput { .. } => "invalid_input",
        CoreError::Uninitialized => "uninitialized",
        CoreError::AuthRequired => "auth_required",
        CoreError::Busy | CoreError::OwnerBusy => "busy",
        CoreError::Closing => "closing",
        CoreError::Interrupted => "interrupted",
        _ => "internal",
    }
}

// ─── Admission pipeline (spec §4.3) ───────────────────────────────────────
/// Run the five-gate admission pipeline.
///
/// Gates:
/// 1. Tool ID allowlist (derived dynamically from `CapabilityRegistry`; V1.57 P3)
/// 2. Active creator (for `nexus.*` tools)
/// 3. Workspace bounds
/// 4. `permissions.toml` / policy
/// 5. Audit log (written by caller `execute()`, not here)
///
/// Returns `(creator_id, workspace_slug)` if all gates pass.
#[allow(clippy::unused_async)] // async is the await-symmetric public signature; the body is store-only today
pub(crate) async fn admission_pipeline(
    req: &ToolExecuteRequest,
    context: &ToolContext,
) -> Result<(String, String), NexusApiError> {
    // Gate 1: tool id allowlist — derived dynamically from the single
    // dispatch spine (V1.57 P3 + AR-68 #4/#6). The spine resolves static
    // rows → PeerToolTable → user capabilities; unknown IDs return
    // NOT_SUPPORTED exactly like an unknown builtin.
    let reg = host_tool_registry();
    if !reg.spine_resolves(context, &req.tool_name) {
        return Err(NexusApiError::Coded {
            code: "not_supported".to_string(),
            message: format!("unsupported tool: {}", req.tool_name),
        });
    }

    // Peer tools (`tools.*`) and user capabilities are dispatched without
    // the nexus.*/fs/* workspace gates — their admission is bound at
    // ingestion (AR-68 #2/#6), not per-call. The spine already proved the
    // id resolves (static | peer | user-cap), so anything that is not a
    // `nexus.*` static row is a peer/user tool.
    let is_nexus_tool = req.tool_name.starts_with("nexus.");
    if !is_nexus_tool && !req.tool_name.starts_with("fs/") {
        return Ok((String::new(), String::new()));
    }
    let creator_id = read_active_creator_id(&context.nexus_home);

    // Gate 2: active creator (for nexus.* tools)
    if is_nexus_tool {
        let creator_id = creator_id.ok_or_else(|| NexusApiError::Forbidden {
            resource: format!(
                "{}: {}",
                "tool_execution", "active creator required for nexus.* tools",
            ),
        })?;
        let workspace_slug = read_active_workspace_slug(&context.nexus_home, &creator_id)
            .ok_or_else(|| NexusApiError::Forbidden {
                resource: format!(
                    "{}: {}",
                    "tool_execution", "active workspace required for nexus.* tools",
                ),
            })?;

        // Gate 3: workspace bounds — verified per-handler for entity lookups
        // (Work, schedule, etc. include creator/workspace predicates in SQL).
        // Path-based bounds for fs/* tools are checked below.

        // Gate 4: permissions.toml / policy
        let workspace_path_str = context.workspace_path().unwrap_or_default();
        if !workspace_path_str.is_empty() {
            if let Some(policy) = load_permission_policy(&workspace_path_str) {
                check_nexus_tool_permission(&req.tool_name, &policy)?;
            }
        }

        return Ok((creator_id, workspace_slug));
    }

    // For fs/* tools: existing V1.33 permission + path validation
    let workspace_path = context.workspace_path();
    let workspace_path_str = workspace_path.unwrap_or_default();
    if workspace_path_str.is_empty() {
        return Err(NexusApiError::Forbidden {
            resource: format!(
                "{}: {}",
                "tool_execution", "fs/* tools require an active workspace with defined bounds",
            ),
        });
    }

    // Gate 4: permissions
    if let Some(policy) = load_permission_policy(&workspace_path_str) {
        check_fs_tool_permission(&req.tool_name, &policy)?;
    }

    // Gate 3 workspace bounds check intentionally skipped for fs/* tools:
    // execute_read_file / execute_write_file call resolve_guarded_path_async
    // before any FS access, making them the single resolution site.

    Ok((creator_id.unwrap_or_default(), String::new()))
}

/// Check permission for a `nexus.*` tool against policy (Gate 4).
fn check_nexus_tool_permission(
    tool_name: &str,
    policy: &WorkspacePermissionPolicy,
) -> Result<(), NexusApiError> {
    const NEXUS_WRITE_TOOLS: &[&str] = &[
        "nexus.work.patch",
        "nexus.kb_snapshot.write",
        "nexus.manuscript.chapter.update",
        "nexus.world.configure",
        "nexus.work.schedule.set",
        "nexus.finding.resolve",
        "nexus.pool.entry.manage",
        // Writes DB state (refresh status + content hash), rewrites the
        // on-disk `body.md`, and performs a network fetch — a read-policy
        // grant must not authorize it.
        "nexus.reference.refresh",
        // Writes the chapter body to disk (temp file + fsync + atomic rename).
        "nexus.manuscript.write",
        // Mutates the Work's persisted stage/stage_status.
        "nexus.manuscript.phase.set",
    ];

    let allowed = if NEXUS_WRITE_TOOLS.contains(&tool_name) {
        is_nexus_write_granted(tool_name, policy)
    } else {
        is_nexus_read_granted(tool_name, policy)
    };

    if allowed {
        return Ok(());
    }

    let reason = if NEXUS_WRITE_TOOLS.contains(&tool_name) {
        "write tool not granted"
    } else {
        "no nexus read grant"
    };

    Err(NexusApiError::Coded {
        code: "policy_blocked".to_string(),
        message: format!("tool '{tool_name}' denied by permissions.toml policy ({reason})"),
    })
}

/// Check permission for `fs/*` tools (V1.33 baseline behavior).
fn check_fs_tool_permission(
    tool_name: &str,
    policy: &WorkspacePermissionPolicy,
) -> Result<(), NexusApiError> {
    let category = match tool_name {
        "fs/read_text_file" => "file_system.read",
        "fs/write_text_file" => "file_system.write",
        _ => return Ok(()),
    };

    if is_capability_granted(category, policy) {
        return Ok(());
    }

    Err(NexusApiError::Coded {
        code: "policy_blocked".to_string(),
        message: format!(
            "tool '{tool_name}' denied by permissions.toml policy (missing '{category}' grant)"
        ),
    })
}

// ─── nexus.* Handlers ─────────────────────────────────────────────────────
//
// V1.53 P0 Sub-phase 3: Old `dispatch_tool()` match table removed.
// All dispatch now routes through `CapabilityRegistry` (see
// `HostToolExecutor::registry_dispatch()` and `capability_registry.rs`).
// The handler functions below remain as they are referenced by the
// `pub(crate)` registry wrapper functions.

/// `nexus.context.whoami` — return active `creator_id` and workspace slug.
fn execute_context_whoami(
    _req: &ToolExecuteRequest,
    context: &ToolContext,
    creator_id: &str,
) -> serde_json::Value {
    let workspace_slug =
        read_active_workspace_slug(&context.nexus_home, creator_id).unwrap_or_default();
    serde_json::json!({
        "creator_id": creator_id,
        "workspace_slug": workspace_slug
    })
}

/// `nexus.workspace.info` — return workspace roots, flags, linked world ref.
fn execute_workspace_info(
    _req: &ToolExecuteRequest,
    context: &ToolContext,
    creator_id: &str,
) -> serde_json::Value {
    let workspace_slug =
        read_active_workspace_slug(&context.nexus_home, creator_id).unwrap_or_default();
    let workspace_path = context.workspace_path().unwrap_or_default();
    serde_json::json!({
        "creator_id": creator_id,
        "workspace_slug": workspace_slug,
        "workspace_path": workspace_path,
        "runtime_mode": context.runtime_facts.runtime_mode_as_str(),
        "initialized": context.runtime_facts.is_initialized
    })
}

/// `nexus.work.get` — return Work row + stage fields for active creator's work.
async fn execute_work_get(
    req: &ToolExecuteRequest,
    context: &ToolContext,
    creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    let work_id =
        req.parameters["work_id"]
            .as_str()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "parameters.work_id".into(),
                reason: "must be a string".into(),
            })?;

    // Entity lookup includes creator predicate (spec §12.5)
    let record = works::get_work(context.pool(), creator_id, work_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            category: format!("DATABASE_ERROR: {e}"),
        })?
        .ok_or_else(|| {
            // Could be not found OR cross-creator — return FORBIDDEN for safety
            NexusApiError::Forbidden {
                resource: format!(
                    "{}: {}",
                    "work", "work not found or cross-creator access denied",
                ),
            }
        })?;

    let dto = WorkDetails::from(record);
    Ok(serde_json::to_value(dto).unwrap_or_else(|_| serde_json::json!({})))
}

/// `nexus.work.patch` — append inspiration + allowed metadata fields (spec §4.4).
///
/// Multi-field patches (`title` + `inspiration_log` + `stage_metadata`) are applied
/// sequentially within the same handler invocation. All mutations go through the
/// [`CoreService`] Work authority (principal/write gates, owned
/// requests, shared lock and error boundary); this boundary only validates the
/// tool payload and maps transport errors (QC1-F-001).
async fn execute_work_patch(
    req: &ToolExecuteRequest,
    context: &ToolContext,
    _creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    let work_id =
        req.parameters["work_id"]
            .as_str()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "parameters.work_id".into(),
                reason: "must be a string".into(),
            })?;

    let core = context.core()?;
    let principal = core.active_principal().await?;

    // Validate patch fields (spec §4.4)
    let params = req
        .parameters
        .as_object()
        .ok_or_else(|| NexusApiError::InvalidInput {
            field: "parameters".into(),
            reason: "must be a JSON object".into(),
        })?;

    // Reject forbidden fields
    for key in params.keys() {
        if key == "work_id" {
            continue; // work_id is a parameter, not a patch field
        }
        if PATCH_REJECTED_FIELDS.contains(&key.as_str()) {
            return Err(NexusApiError::Coded {
                code: "invalid_input".to_string(),
                message: format!("field '{key}' is not allowed in nexus.work.patch (spec §4.4)"),
            });
        }
        if !PATCH_ALLOWED_FIELDS.contains(&key.as_str()) {
            return Err(NexusApiError::Coded {
                code: "invalid_input".to_string(),
                message: format!("unknown patch field '{key}'"),
            });
        }
    }

    // Handle inspiration_log append
    if let Some(inspiration) = params.get("inspiration_log") {
        let entries = inspiration
            .as_array()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "parameters.inspiration_log".into(),
                reason: "must be an array of entries".into(),
            })?;

        for entry in entries {
            let note = entry["text"]
                .as_str()
                .or_else(|| entry["note"].as_str())
                .ok_or_else(|| NexusApiError::InvalidInput {
                    field: "parameters.inspiration_log[].text".into(),
                    reason: "each entry must include a 'text' or 'note' field".into(),
                })?;

            // The core composes the canonical inspiration entry
            // (`{"at", "note"}`); per-entry `source` sidecar values from the
            // legacy host-tool payload are normalized away by the authority.
            core.append_work_inspiration(
                &principal,
                work_id.to_string(),
                "http",
                nexus_contracts::AppendInspirationRequest {
                    note: note.to_string(),
                },
            )
            .await?;
        }
    }

    // Handle title patch
    if let Some(title) = params.get("title") {
        let title_str = title.as_str().ok_or_else(|| NexusApiError::InvalidInput {
            field: "parameters.title".into(),
            reason: "must be a string".into(),
        })?;
        if title_str.trim().is_empty() {
            return Err(NexusApiError::InvalidInput {
                field: "parameters.title".into(),
                reason: "must not be empty".into(),
            });
        }

        core.patch_work(
            &principal,
            work_id.to_string(),
            "http",
            WorkPatchRequest {
                title: Some(title_str.to_string()),
                ..Default::default()
            },
        )
        .await?;
    }

    // Handle stage_metadata patch — validate sub-field allowlist (spec §4.4).
    // V1.34 minimal: accepted but stored as-is in inspiration_log as a metadata entry.
    if let Some(metadata) = params.get("stage_metadata") {
        // Validate stage_metadata sub-field allowlist
        let metadata_obj = metadata
            .as_object()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "parameters.stage_metadata".into(),
                reason: "must be a JSON object".into(),
            })?;
        for key in metadata_obj.keys() {
            if PATCH_REJECTED_FIELDS.contains(&key.as_str()) {
                return Err(NexusApiError::Coded { code: "invalid_input".to_string(), message: format!(
                        "stage_metadata key '{key}' is not allowed (spec §4.4: stage control fields must use stage-advance path)"
                    ) });
            }
            if !STAGE_METADATA_ALLOWED_KEYS.contains(&key.as_str()) {
                return Err(NexusApiError::Coded {
                    code: "invalid_input".to_string(),
                    message: format!(
                        "stage_metadata key '{key}' is not in the allowed list (spec §4.4: {})",
                        STAGE_METADATA_ALLOWED_KEYS.join(", ")
                    ),
                });
            }
        }

        let note = format!(
            "[stage_metadata] {}",
            serde_json::to_string(metadata).unwrap_or_default()
        );
        core.append_work_inspiration(
            &principal,
            work_id.to_string(),
            "http",
            nexus_contracts::AppendInspirationRequest { note },
        )
        .await?;
    }

    // Return the updated Work through the same core authority.
    let details = core.get_work(&principal, work_id.to_string()).await?;
    let dto = details;
    Ok(serde_json::to_value(dto).unwrap_or_else(|_| serde_json::json!({})))
}

/// `nexus.orchestration.schedule_status` — return schedules linked to `work_id`.
async fn execute_schedule_status(
    req: &ToolExecuteRequest,
    context: &ToolContext,
    creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    let work_id =
        req.parameters["work_id"]
            .as_str()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "parameters.work_id".into(),
                reason: "must be a string".into(),
            })?;

    // Verify work ownership first
    let record = works::get_work(context.pool(), creator_id, work_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            category: format!("DATABASE_ERROR: {e}"),
        })?
        .ok_or_else(|| NexusApiError::Forbidden {
            resource: format!(
                "{}: {}",
                "work", "work not found or cross-creator access denied"
            ),
        })?;

    let schedule_ids: Vec<serde_json::Value> =
        serde_json::from_str(&record.schedule_ids).unwrap_or_default();

    Ok(serde_json::json!({
        "work_id": work_id,
        "schedule_ids": schedule_ids,
        "count": schedule_ids.len()
    }))
}

/// `nexus.context.assemble` — local assemble-moment or `POLICY_BLOCKED` (spec §4.1).
async fn execute_context_assemble(
    req: &ToolExecuteRequest,
    context: &ToolContext,
    creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    let requires_platform = req.parameters["requires_platform"]
        .as_bool()
        .unwrap_or(false);

    // Check platform_integration state
    if requires_platform
        && matches!(
            context.runtime_mode(),
            nexus_contracts::local::domain::RuntimeMode::LocalOnly
        )
    {
        return Err(NexusApiError::Coded {
            code: "policy_blocked".to_string(),
            message: "PLATFORM_PAUSED: platform-only assembly not available in local-only mode"
                .to_string(),
        });
    }

    // If work_id is provided, verify ownership
    if let Some(work_id) = req.parameters["work_id"].as_str() {
        let _record = works::get_work(context.pool(), creator_id, work_id)
            .await
            .map_err(|e| NexusApiError::Internal {
                category: format!("DATABASE_ERROR: {e}"),
            })?
            .ok_or_else(|| NexusApiError::Forbidden {
                resource: format!(
                    "{}: {}",
                    "work", "work not found or cross-creator access denied"
                ),
            })?;
    }

    // Local-only assembly subset
    Ok(serde_json::json!({
        "mode": "local",
        "creator_id": creator_id,
        "assembled_at": chrono::Utc::now().to_rfc3339()
    }))
}

// ─── fs/* Baseline handlers (V1.33, unchanged behavior) ───────────────────

/// Execute `fs/read_text_file` tool.
///
/// V1.86 T5 (R-V156P0-M004): all blocking `std::fs` operations run on the
/// tokio blocking pool so the async runtime is not stalled by local disk I/O.
#[allow(clippy::similar_names)] // the paired names are the domain vocabulary here
async fn execute_read_file(
    req: &ToolExecuteRequest,
    context: &ToolContext,
) -> Result<serde_json::Value, NexusApiError> {
    let path_str = req.parameters["path"]
        .as_str()
        .ok_or_else(|| NexusApiError::InvalidInput {
            field: "parameters.path".into(),
            reason: "must be a string".into(),
        })?
        .to_string();

    let workspace_path_str = context
        .workspace_path()
        .ok_or_else(|| NexusApiError::Forbidden {
            resource: format!(
                "{}: {}",
                "tool_execution", "fs/* tools require an active workspace"
            ),
        })?;

    let workspace_path = Path::new(&workspace_path_str).to_path_buf();
    let resolved = resolve_guarded_path_async(workspace_path, path_str.clone(), true)
        .await
        .map_err(|e| match e {
            NexusApiError::InvalidInput { .. } => NexusApiError::Forbidden {
                resource: format!("file: path '{path_str}' is outside the workspace root"),
            },
            other => other,
        })?;

    let content = tokio::task::spawn_blocking({
        let resolved = resolved.clone();
        move || std::fs::read_to_string(&resolved)
    })
    .await
    .map_err(|e| NexusApiError::Internal {
        category: format!(
            "FILE_READ_PANIC: {}",
            format_args!("file read task panicked: {e}")
        ),
    })?
    .map_err(|e| NexusApiError::Internal {
        category: format!(
            "FILE_READ_FAILED: {}",
            format_args!("failed to read file {}: {e}", resolved.display())
        ),
    })?;

    Ok(serde_json::json!({
        "content": content
    }))
}

/// Execute `fs/write_text_file` tool.
///
/// V1.86 T5 (R-V156P0-M004): all blocking `std::fs` operations run on the
/// tokio blocking pool so the async runtime is not stalled by local disk I/O.
#[allow(clippy::similar_names)] // the paired names are the domain vocabulary here
async fn execute_write_file(
    req: &ToolExecuteRequest,
    context: &ToolContext,
) -> Result<serde_json::Value, NexusApiError> {
    let path_str = req.parameters["path"]
        .as_str()
        .ok_or_else(|| NexusApiError::InvalidInput {
            field: "parameters.path".into(),
            reason: "must be a string".into(),
        })?
        .to_string();

    let content = req.parameters["content"]
        .as_str()
        .ok_or_else(|| NexusApiError::InvalidInput {
            field: "parameters.content".into(),
            reason: "must be a string".into(),
        })?
        .to_string();

    let workspace_path_str = context
        .workspace_path()
        .ok_or_else(|| NexusApiError::Forbidden {
            resource: format!(
                "{}: {}",
                "tool_execution", "fs/* tools require an active workspace"
            ),
        })?;

    let workspace_path = Path::new(&workspace_path_str).to_path_buf();
    let resolved = resolve_guarded_path_async(workspace_path, path_str.clone(), false)
        .await
        .map_err(|e| match e {
            NexusApiError::InvalidInput { .. } => NexusApiError::Forbidden {
                resource: format!("file: path '{path_str}' is outside the workspace root"),
            },
            other => other,
        })?;

    let write_result = tokio::task::spawn_blocking(move || {
        if let Some(parent) = resolved.parent() {
            std::fs::create_dir_all(parent).map_err(|e| NexusApiError::Internal {
                category: format!(
                    "DIR_CREATE_FAILED: {}",
                    format_args!("failed to create directory {}: {}", parent.display(), e)
                ),
            })?;
        }
        std::fs::write(&resolved, content).map_err(|e| NexusApiError::Internal {
            category: format!(
                "FILE_WRITE_FAILED: {}",
                format_args!("failed to write file {}: {e}", resolved.display())
            ),
        })
    })
    .await
    .map_err(|e| NexusApiError::Internal {
        category: format!(
            "FILE_WRITE_PANIC: {}",
            format_args!("file write task panicked: {e}")
        ),
    })?;
    write_result?;

    Ok(serde_json::json!({
        "written": true
    }))
}

// ─── Permission / path helpers ────────────────────────────────────────────
// ─── Permission / path helpers ────────────────────────────────────────────

/// Mirrors `nexus-acp-host::PermissionPolicy::evaluate` without linking that crate
/// (daemon-runtime linkage matrix forbids `nexus-acp-host`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PolicyDecision {
    Grant,
    Deny,
    Ask,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum DefaultPolicySetting {
    #[default]
    Ask,
    Grant,
    Deny,
}

#[derive(Debug, Clone)]
struct WorkspacePermissionPolicy {
    default: DefaultPolicySetting,
    grant: std::collections::HashSet<String>,
    deny: std::collections::HashSet<String>,
}

impl WorkspacePermissionPolicy {
    fn evaluate(&self, permission_name: &str) -> PolicyDecision {
        if self.grant.contains(permission_name) {
            return PolicyDecision::Grant;
        }
        if self.deny.contains(permission_name) {
            return PolicyDecision::Deny;
        }
        match self.default {
            DefaultPolicySetting::Grant => PolicyDecision::Grant,
            DefaultPolicySetting::Deny => PolicyDecision::Deny,
            DefaultPolicySetting::Ask => PolicyDecision::Ask,
        }
    }
}

fn table_keys(table: &toml::Table) -> std::collections::HashSet<String> {
    table.keys().cloned().collect()
}

fn is_nexus_write_granted(tool_name: &str, policy: &WorkspacePermissionPolicy) -> bool {
    if matches!(policy.evaluate(tool_name), PolicyDecision::Deny) {
        return false;
    }
    matches!(policy.evaluate(tool_name), PolicyDecision::Grant)
        || matches!(policy.evaluate("nexus.*"), PolicyDecision::Grant)
}

fn is_nexus_read_granted(tool_name: &str, policy: &WorkspacePermissionPolicy) -> bool {
    if matches!(policy.evaluate(tool_name), PolicyDecision::Deny) {
        return false;
    }
    is_capability_granted(tool_name, policy)
        || is_capability_granted("nexus.*", policy)
        || is_capability_granted("nexus.*.read", policy)
}

fn is_capability_granted(capability: &str, policy: &WorkspacePermissionPolicy) -> bool {
    match policy.evaluate(capability) {
        PolicyDecision::Grant => true,
        PolicyDecision::Deny => false,
        PolicyDecision::Ask => matches!(policy.default, DefaultPolicySetting::Grant),
    }
}

/// Load permission policy from workspace if available.
///
/// Returns `None` if no policy file exists (all tools permitted).
fn load_permission_policy(workspace_path: &str) -> Option<WorkspacePermissionPolicy> {
    let policy_path = Path::new(workspace_path)
        .join(".nexus42")
        .join("permissions.toml");
    if !policy_path.exists() {
        return None;
    }

    let content = std::fs::read_to_string(&policy_path).ok()?;
    let policy: toml::Value = toml::from_str(&content).ok()?;

    let default = match policy.get("default").and_then(|v| v.as_str()) {
        Some("grant") => DefaultPolicySetting::Grant,
        Some("deny") => DefaultPolicySetting::Deny,
        _ => DefaultPolicySetting::Ask,
    };

    let grant = policy
        .get("grant")
        .and_then(|v| v.as_table())
        .map(table_keys)
        .unwrap_or_default();
    let deny = policy
        .get("deny")
        .and_then(|v| v.as_table())
        .map(table_keys)
        .unwrap_or_default();

    Some(WorkspacePermissionPolicy {
        default,
        grant,
        deny,
    })
}

// ─── Audit logging (spec §12.6) ───────────────────────────────────────────

/// Audit tool execution to `SQLite` (Gate 5).
pub(crate) async fn audit_tool_execution(
    req: &ToolExecuteRequest,
    decision: &str,
    error_code: Option<&str>,
    context: &ToolContext,
) -> Result<(), NexusApiError> {
    let tool_name = req.tool_name.clone();
    let session_id = req.session_id.clone().unwrap_or_default();
    let _request_id = req.request_id.clone().unwrap_or_default();
    let caller_kind = req
        .caller_kind
        .map_or_else(|| "http".to_string(), |k| k.to_string());

    // Redact parameter summary — only include top-level keys
    let param_summary: Vec<String> = req
        .parameters
        .as_object()
        .map(|obj| obj.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();

    let outcome = if decision == "success" {
        "success".to_string()
    } else {
        format!("denied:{}", error_code.unwrap_or("UNKNOWN"))
    };

    // SAFETY: audit log INSERT — column names are static.
    sqlx::query(
        "INSERT INTO acp_tool_audit_log (tool_name, path, outcome, agent_id, session_id)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&tool_name)
    .bind(param_summary.join(","))
    .bind(&outcome)
    .bind(&caller_kind)
    .bind(&session_id)
    .execute(context.pool())
    .await
    .map_err(|e| NexusApiError::Internal {
        category: format!(
            "AUDIT_LOG_FAILED: {}",
            format_args!("failed to write audit log: {e}")
        ),
    })?;

    Ok(())
}

// ─── V1.53 P1: DF-46 read-heavy nexus.* handlers ─────────────────────────
// ─── V1.53 P1: DF-46 read-heavy nexus.* handlers ─────────────────────────

/// Verify that `creator_id` owns `world_id` by querying `narrative_worlds`.
///
/// V1.67 P2 (R-V160P0-QC2-W001): delegates to the shared
/// `nexus_local_db::narrative_write::is_world_owned` gate so the ownership
/// check is no longer duplicated in the orchestration crate. Returns
/// `Forbidden { resource: "world" }` on mismatch/missing; `Internal` on DB errors.
async fn ensure_world_accessible_for_creator(
    pool: &sqlx::SqlitePool,
    creator_id: &str,
    world_id: &str,
) -> Result<(), NexusApiError> {
    match nexus_local_db::narrative_write::is_world_owned(pool, creator_id, world_id).await {
        Ok(true) => Ok(()),
        Ok(false) => Err(NexusApiError::Forbidden {
            resource: format!(
                "{}: {}",
                "world", "world not found or cross-creator access denied",
            ),
        }),
        Err(e) => Err(NexusApiError::Internal {
            category: format!(
                "DATABASE_ERROR: {}",
                format_args!("world ownership check: {e}")
            ),
        }),
    }
}

/// `nexus.world.snapshot.get` — consistent read of structured world snapshot.
async fn execute_world_snapshot_get(
    req: &ToolExecuteRequest,
    context: &ToolContext,
    creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    let world_id =
        req.parameters["world_id"]
            .as_str()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "parameters.world_id".into(),
                reason: "must be a string".into(),
            })?;

    ensure_world_accessible_for_creator(context.pool(), creator_id, world_id).await?;

    // The gateway is a thin projection over the same pool this context carries,
    // so the core composes its own rather than reaching into a transport.
    let gw = nexus_local_db::narrative_gateway::SqliteNarrativeGateway::new(context.pool().clone());
    let world_state =
        gw.get_world_state(world_id)
            .await
            .map_err(|e: nexus_narrative::NarrativeError| {
                if e.to_string().contains("not found") {
                    NexusApiError::NotFound {
                        resource: world_id.to_string(),
                    }
                } else {
                    NexusApiError::Internal {
                        category: format!("NARRATIVE_ERROR: {e}"),
                    }
                }
            })?;

    Ok(serde_json::to_value(world_state).unwrap_or_else(|_| serde_json::json!({})))
}

/// `nexus.timeline.recent.get` — fetch recent timeline events for continuity.
async fn execute_timeline_recent_get(
    req: &ToolExecuteRequest,
    context: &ToolContext,
    creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    let world_id =
        req.parameters["world_id"]
            .as_str()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "parameters.world_id".into(),
                reason: "must be a string".into(),
            })?;

    ensure_world_accessible_for_creator(context.pool(), creator_id, world_id).await?;

    // Default limit 100, clamp to max 500
    let limit: usize = req.parameters["limit"]
        .as_u64()
        .and_then(|v| usize::try_from(v).ok())
        .unwrap_or(100)
        .min(500);

    // The gateway is a thin projection over the same pool this context carries,
    // so the core composes its own rather than reaching into a transport.
    let gw = nexus_local_db::narrative_gateway::SqliteNarrativeGateway::new(context.pool().clone());
    let mut events = gw.get_timeline(world_id, None, Some(limit)).await.map_err(
        |e: nexus_narrative::NarrativeError| NexusApiError::Internal {
            category: format!("NARRATIVE_ERROR: {e}"),
        },
    )?;

    // SQL returns DESC order when limit is set; reverse to ASC for display.
    events.reverse();
    Ok(serde_json::to_value(&events).unwrap_or_else(|_| serde_json::json!([])))
}

/// `nexus.kb_snapshot.read` — focused KB snapshot read for a world.
async fn execute_kb_snapshot_read(
    req: &ToolExecuteRequest,
    context: &ToolContext,
    creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    let world_id =
        req.parameters["world_id"]
            .as_str()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "parameters.world_id".into(),
                reason: "must be a string".into(),
            })?;

    let pool = context.pool();

    ensure_world_accessible_for_creator(pool, creator_id, world_id).await?;

    let kb_store = nexus_local_db::kb_store::SqliteKbStore::new(pool.clone());
    let blocks = kb_store.list_by_world(world_id).await.map_err(
        |e: nexus_knowledge::world_kb::store::KbStoreError| NexusApiError::Internal {
            category: format!("KB_STORE_ERROR: {e}"),
        },
    )?;

    Ok(serde_json::to_value(&blocks).unwrap_or_else(|_| serde_json::json!([])))
}

/// `nexus.manuscript.chapter.get` — read a single manuscript chapter record.
async fn execute_manuscript_chapter_get(
    req: &ToolExecuteRequest,
    context: &ToolContext,
    creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    let work_id =
        req.parameters["work_id"]
            .as_str()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "parameters.work_id".into(),
                reason: "must be a string".into(),
            })?;

    let chapter: i32 = req.parameters["chapter"]
        .as_i64()
        .ok_or_else(|| NexusApiError::InvalidInput {
            field: "parameters.chapter".into(),
            reason: "must be an integer".into(),
        })?
        .try_into()
        .map_err(|_| NexusApiError::InvalidInput {
            field: "parameters.chapter".into(),
            reason: "must be a valid i32 chapter number".into(),
        })?;

    let volume: i32 = req.parameters["volume"]
        .as_i64()
        .map_or(1, |v| i32::try_from(v).unwrap_or(1));

    let pool = context.pool();

    // Verify work ownership first (reuse existing pattern from execute_work_get)
    let _record = works::get_work(pool, creator_id, work_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            category: format!("DATABASE_ERROR: {e}"),
        })?
        .ok_or_else(|| NexusApiError::Forbidden {
            resource: format!(
                "{}: {}",
                "work", "work not found or cross-creator access denied",
            ),
        })?;

    let chapter_record = nexus_local_db::work_chapters::get_chapter(pool, work_id, chapter, volume)
        .await
        .map_err(|e| NexusApiError::Internal {
            category: format!("DATABASE_ERROR: {e}"),
        })?;

    chapter_record.map_or_else(
        || {
            Err(NexusApiError::NotFound {
                resource: format!("{work_id}/ch{chapter}"),
            })
        },
        |ch| Ok(serde_json::to_value(&ch).unwrap_or_else(|_| serde_json::json!({}))),
    )
}

/// `nexus.observability.daemon.health` — agent-visible daemon health status.
///
/// **R-V153P1QC2-003 (V1.54 closure):** The `registry_ids` field exposes all
/// tool IDs to authorized callers. This is acceptable for daemon-local
/// observability (V1.53–V1.54) because the handler requires active creator
/// and passes the Allowlist + `PermissionPolicy` admission gates. When
/// agent-facing observability is exposed beyond daemon-local scope
/// (V1.55+), an additional audit-level policy gate should be added and
/// `registry_ids` may need to be stripped for low-trust callers.
fn execute_daemon_health(
    _req: &ToolExecuteRequest,
    context: &ToolContext,
    _creator_id: &str,
) -> serde_json::Value {
    let reg = host_tool_registry();
    serde_json::json!({
        "uptime_seconds": context.runtime_facts.uptime_seconds,
        "started_at": &context.runtime_facts.started_at,
        "runtime_mode": context.runtime_facts.runtime_mode_as_str(),
        "lifecycle_state": context.runtime_facts.lifecycle_state.clone(),
        "registry_size": reg.len(),
        "registry_ids": reg.ids().collect::<Vec<_>>(),
        "pool_healthy": true
    })
}

// ─── Registry handler wrappers (V1.53 P0) ─────────────────────────────────
//
// These `pub(crate)` wrappers adapt the existing private handler functions
// to the `RegistryHandlerFn` signature used by `CapabilityRegistry`.
// They exist so the registry can reference the same handler implementations
// without duplicating logic.
//
// Each wrapper uses an explicit named lifetime `'a` to satisfy the
// higher-ranked trait bound `for<'a> fn(&'a ..., &'a ..., &'a str) -> ...`.

/// Registry wrapper: `nexus.context.whoami` — sync → async wrapper.
pub(crate) fn registry_context_whoami<'a>(
    req: &'a ToolExecuteRequest,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    let result = execute_context_whoami(req, context, creator_id);
    Box::pin(async move { Ok(result) })
}

/// Registry wrapper: `nexus.workspace.info` — sync → async wrapper.
pub(crate) fn registry_workspace_info<'a>(
    req: &'a ToolExecuteRequest,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    let result = execute_workspace_info(req, context, creator_id);
    Box::pin(async move { Ok(result) })
}

/// Registry wrapper: `nexus.work.get` — async passthrough.
pub(crate) fn registry_work_get<'a>(
    req: &'a ToolExecuteRequest,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_work_get(req, context, creator_id))
}

/// Registry wrapper: `nexus.work.patch` — async passthrough.
pub(crate) fn registry_work_patch<'a>(
    req: &'a ToolExecuteRequest,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_work_patch(req, context, creator_id))
}

/// Registry wrapper: `nexus.orchestration.schedule_status` — async passthrough.
pub(crate) fn registry_schedule_status<'a>(
    req: &'a ToolExecuteRequest,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_schedule_status(req, context, creator_id))
}

/// Registry wrapper: `nexus.context.assemble` — async passthrough.
pub(crate) fn registry_context_assemble<'a>(
    req: &'a ToolExecuteRequest,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_context_assemble(req, context, creator_id))
}

/// Registry wrapper: `fs/read_text_file` — async passthrough (ignores `creator_id`).
pub(crate) fn registry_read_file<'a>(
    req: &'a ToolExecuteRequest,
    context: &'a ToolContext,
    _creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_read_file(req, context))
}

/// Registry wrapper: `fs/write_text_file` — async passthrough (ignores `creator_id`).
pub(crate) fn registry_write_file<'a>(
    req: &'a ToolExecuteRequest,
    context: &'a ToolContext,
    _creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_write_file(req, context))
}

// ─── V1.53 P1: Registry wrappers for DF-46 read-heavy tools ───────────────

/// Registry wrapper: `nexus.world.snapshot.get` — async passthrough.
pub(crate) fn registry_world_snapshot_get<'a>(
    req: &'a ToolExecuteRequest,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_world_snapshot_get(req, context, creator_id))
}

/// Registry wrapper: `nexus.timeline.recent.get` — async passthrough.
pub(crate) fn registry_timeline_recent_get<'a>(
    req: &'a ToolExecuteRequest,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_timeline_recent_get(req, context, creator_id))
}

/// Registry wrapper: `nexus.kb_snapshot.read` — async passthrough.
pub(crate) fn registry_kb_snapshot_read<'a>(
    req: &'a ToolExecuteRequest,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_kb_snapshot_read(req, context, creator_id))
}

/// Registry wrapper: `nexus.manuscript.chapter.get` — async passthrough.
pub(crate) fn registry_manuscript_chapter_get<'a>(
    req: &'a ToolExecuteRequest,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_manuscript_chapter_get(req, context, creator_id))
}

/// Registry wrapper: `nexus.observability.daemon.health` — sync → async wrapper.
pub(crate) fn registry_daemon_health<'a>(
    req: &'a ToolExecuteRequest,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    let result = execute_daemon_health(req, context, creator_id);
    Box::pin(async move { Ok(result) })
}

// ─── V1.58 P3: nexus.reference.refresh ──────────────────────────────────────

/// `nexus.reference.refresh` — refresh a reference source body via the daemon.
///
/// Delegates to `nexus_orchestration::capability::builtins::ReferenceRefresh`.
/// Requires an active creator (admission gate) and a pool. Creator context
/// (home dir + `creator_id`) is wired so the handler can write refreshed body
/// content to the on-disk `body.md`.
async fn execute_reference_refresh(
    req: &ToolExecuteRequest,
    context: &ToolContext,
    creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    use nexus_orchestration::capability::Capability;
    let cap = nexus_orchestration::capability::builtins::ReferenceRefresh::with_pool(
        context.pool().clone(),
    )
    .with_creator_context(context.nexus_home.clone(), creator_id.to_string());

    let input = req.parameters.clone();
    cap.run(input).await.map_err(|e| NexusApiError::Internal {
        category: format!(
            "REFERENCE_REFRESH_FAILED: {}",
            format_args!("nexus.reference.refresh failed: {e}")
        ),
    })
}

pub(crate) fn registry_reference_refresh<'a>(
    req: &'a ToolExecuteRequest,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_reference_refresh(req, context, creator_id))
}

// ─── V1.56 P1: nexus.registry.refresh ──────────────────────────────────────

/// `nexus.registry.refresh` — return the registry snapshot (synthetic or CDN).
async fn execute_registry_refresh(
    _req: &ToolExecuteRequest,
    _context: &ToolContext,
    _creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    use nexus_orchestration::capability::Capability;
    let cap = nexus_orchestration::capability::builtins::RegistryRefresh::new();
    let input = serde_json::json!({"force": false});
    cap.run(input).await.map_err(|e| NexusApiError::Internal {
        category: format!(
            "REGISTRY_REFRESH_FAILED: {}",
            format_args!("registry.refresh failed: {e}")
        ),
    })
}

pub(crate) fn registry_registry_refresh<'a>(
    req: &'a ToolExecuteRequest,
    context: &'a ToolContext,

    // ─── V1.56 P1 + V1.54 P0: registry.refresh + write tools ─────────────────
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_registry_refresh(req, context, creator_id))
}

// ─── V1.54 P0: DF-46 write tool handlers ──────────────────────────────────

/// `nexus.kb_snapshot.write` — upsert key blocks for a world.
async fn execute_kb_snapshot_write(
    req: &ToolExecuteRequest,
    context: &ToolContext,
    creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    let world_id =
        req.parameters["world_id"]
            .as_str()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "parameters.world_id".into(),
                reason: "must be a string".into(),
            })?;

    let pool = context.pool();

    ensure_world_accessible_for_creator(pool, creator_id, world_id).await?;

    let blocks =
        req.parameters["blocks"]
            .as_array()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "parameters.blocks".into(),
                reason: "must be an array of key blocks".into(),
            })?;

    let kb_store = nexus_local_db::kb_store::SqliteKbStore::new(pool.clone());
    let mut written: usize = 0;
    let mut tx = pool.begin().await.map_err(|e| NexusApiError::Internal {
        category: format!("DATABASE_ERROR: {e}"),
    })?;

    for block_val in blocks {
        let kb: nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord =
            serde_json::from_value(block_val.clone()).map_err(|e| NexusApiError::InvalidInput {
                field: "parameters.blocks[]".into(),
                reason: format!("invalid key block: {e}"),
            })?;
        // C-001: reject blocks whose embedded world_id does not match the
        // request-level world_id (prevents cross-world block payload bypass).
        if kb.world_id() != Some(world_id) {
            return Err(NexusApiError::Forbidden {
                resource: format!(
                    "{}: {}",
                    "knowledge_entry.world_id",
                    format_args!(
                        "block {} targets world '{}' but request targets world '{}'",
                        kb.entry_id,
                        kb.world_id().unwrap_or_default(),
                        world_id
                    ),
                ),
            });
        }
        kb_store
            .insert_key_block_in_tx(&mut tx, kb)
            .await
            .map_err(|e| NexusApiError::Internal {
                category: format!("KB_STORE_ERROR: {e}"),
            })?;
        written += 1;
    }

    tx.commit().await.map_err(|e| NexusApiError::Internal {
        category: format!("DATABASE_ERROR: {e}"),
    })?;

    Ok(serde_json::json!({
        "written": written,
        "world_id": world_id
    }))
}

/// `nexus.manuscript.chapter.update` — update chapter content and metadata.
async fn execute_manuscript_chapter_update(
    req: &ToolExecuteRequest,
    context: &ToolContext,
    creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    let work_id =
        req.parameters["work_id"]
            .as_str()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "parameters.work_id".into(),
                reason: "must be a string".into(),
            })?;

    let chapter: i32 = req.parameters["chapter"]
        .as_i64()
        .ok_or_else(|| NexusApiError::InvalidInput {
            field: "parameters.chapter".into(),
            reason: "must be an integer".into(),
        })?
        .try_into()
        .map_err(|_| NexusApiError::InvalidInput {
            field: "parameters.chapter".into(),
            reason: "must be a valid i32 chapter number".into(),
        })?;

    let volume: i32 = req.parameters["volume"]
        .as_i64()
        .map_or(1, |v| i32::try_from(v).unwrap_or(1));

    let pool = context.pool();

    // Verify work ownership first (keep record for work_ref used in W-003 path).
    let work_record = works::get_work(pool, creator_id, work_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            category: format!("DATABASE_ERROR: {e}"),
        })?
        .ok_or_else(|| NexusApiError::Forbidden {
            resource: format!(
                "{}: {}",
                "work", "work not found or cross-creator access denied",
            ),
        })?;

    // Check chapter exists
    let chapter_exists = nexus_local_db::work_chapters::get_chapter(pool, work_id, chapter, volume)
        .await
        .map_err(|e| NexusApiError::Internal {
            category: format!("DATABASE_ERROR: {e}"),
        })?;

    if chapter_exists.is_none() {
        return Err(NexusApiError::NotFound {
            resource: format!("{work_id}/ch{chapter}/v{volume}"),
        });
    }

    // Update body content if provided
    let now = chrono::Utc::now().to_rfc3339();
    let body_path: Option<String> = if let Some(content) = req.parameters["content"].as_str() {
        let workspace_root = context
            .workspace_path()
            .ok_or_else(|| NexusApiError::Internal {
                category: format!("WORKSPACE_PATH_ERROR: {}", "workspace path not available"),
            })?;
        // W-003: use the canonical body_path from the existing chapter record
        // (set by seed_chapters), which follows Works/{work_ref}/Stories/{slug}.md.
        // Fall back to constructing the path if the chapter has no body_path yet.
        let canonical_path = chapter_exists
            .as_ref()
            .and_then(|cr| cr.body_path.clone())
            .unwrap_or_else(|| {
                let ch_nn = format!("ch{chapter:02}");
                let wr = work_record.work_ref.as_deref().unwrap_or(work_id);
                format!("Works/{wr}/Stories/{ch_nn}-{ch_nn}.md")
            });

        // W-002: defense-in-depth path guard — ensure the resolved body path
        // stays inside the workspace root before any FS op. Mirrors the chapter
        // PUT handler's resolve_guarded_path behavior.
        let body_file = resolve_guarded_path_async(
            Path::new(&workspace_root).to_path_buf(),
            canonical_path.clone(),
            false,
        )
        .await
        .map_err(|e| match e {
            NexusApiError::InvalidInput { field, .. } if field == "chapter_path_forbidden" => {
                NexusApiError::InvalidInput {
                    field: "body_path".into(),
                    reason: "body path outside workspace root".into(),
                }
            }
            other => other,
        })?;

        if let Some(parent) = body_file.parent() {
            // C-002: use tokio::fs to avoid blocking the async runtime.
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| NexusApiError::Internal {
                    category: format!(
                        "DIR_CREATE_FAILED: {}",
                        format_args!("failed to create chapter dir: {e}")
                    ),
                })?;
        }
        // C-002: write to a temp file first, then atomically rename within a
        // DB transaction to prevent orphaned files on crash between write and
        // DB commit.
        let tmp_file = body_file.with_extension("md.tmp");
        tokio::fs::write(&tmp_file, content)
            .await
            .map_err(|e| NexusApiError::Internal {
                category: format!(
                    "FILE_WRITE_FAILED: {}",
                    format_args!("failed to write chapter body: {e}")
                ),
            })?;
        // Durability: fsync temp file before the atomic rename.
        let tmp_handle =
            tokio::fs::File::open(&tmp_file)
                .await
                .map_err(|e| NexusApiError::Internal {
                    category: format!(
                        "FILE_SYNC_FAILED: {}",
                        format_args!("failed to open temp file for fsync: {e}")
                    ),
                })?;
        tmp_handle
            .sync_all()
            .await
            .map_err(|e| NexusApiError::Internal {
                category: format!(
                    "FILE_SYNC_FAILED: {}",
                    format_args!("failed to fsync temp file: {e}")
                ),
            })?;
        // W-003: store the relative canonical path in the DB, matching
        // the seed_chapters convention (Works/{work_ref}/Stories/{slug}.md).
        Some(canonical_path)
    } else {
        None
    };

    // Update chapter DB row if body_path or word count changed.
    // C-002: wrap DB update + file rename in a single transaction so the
    // DB row is only updated when the final file is in place.
    if let Some(ref bp) = body_path {
        // W-003: bp is a relative canonical path; resolve to absolute for FS ops.
        // W-002: re-apply the path guard before the FS rename so the tx block
        // cannot be reached with an escaped path even if the earlier guard were
        // bypassed.
        let workspace_root = context
            .workspace_path()
            .ok_or_else(|| NexusApiError::Internal {
                category: format!("WORKSPACE_PATH_ERROR: {}", "workspace path not available"),
            })?;
        let abs_body =
            resolve_guarded_path_async(Path::new(&workspace_root).to_path_buf(), bp.clone(), false)
                .await
                .map_err(|e| match e {
                    NexusApiError::InvalidInput { field, .. }
                        if field == "chapter_path_forbidden" =>
                    {
                        NexusApiError::InvalidInput {
                            field: "body_path".into(),
                            reason: "body path outside workspace root".into(),
                        }
                    }
                    other => other,
                })?;
        let abs_tmp = abs_body.with_extension("md.tmp");
        let word_count = req.parameters["content"]
            .as_str()
            .map_or(0, |c| c.split_whitespace().count());
        let mut tx = pool.begin().await.map_err(|e| NexusApiError::Internal {
            category: format!(
                "DATABASE_ERROR: {}",
                format_args!("chapter update tx begin: {e}")
            ),
        })?;
        // SAFETY: dynamic SQL for chapter update — runtime fields.
        sqlx::query(
            "UPDATE work_chapters SET body_path = ?, actual_word_count = ?, updated_at = ? \
             WHERE work_id = ? AND chapter = ? AND volume = ?",
        )
        .bind(bp)
        .bind(
            i64::try_from(word_count).map_err(|_| NexusApiError::Internal {
                category: format!(
                    "WORK_WORD_COUNT_OVERFLOW: {}",
                    format_args!("word_count {word_count} exceeds i64")
                ),
            })?,
        )
        .bind(&now)
        .bind(work_id)
        .bind(chapter)
        .bind(volume)
        .execute(&mut *tx)
        .await
        .map_err(|e| NexusApiError::Internal {
            category: format!("DATABASE_ERROR: {}", format_args!("chapter update: {e}")),
        })?;
        // Atomically rename temp → final (after DB update succeeds inside tx).
        tokio::fs::rename(&abs_tmp, &abs_body).await.map_err(|e| {
            // Best-effort cleanup: remove temp file on rename failure.
            let _ = std::fs::remove_file(&abs_tmp);
            NexusApiError::Internal {
                category: format!(
                    "FILE_RENAME_FAILED: {}",
                    format_args!("failed to finalize chapter file: {e}")
                ),
            }
        })?;
        // Durability: fsync the final file after the atomic rename so a crash
        // after rename() returns does not leave the rename unflushed.
        let final_handle =
            tokio::fs::File::open(&abs_body)
                .await
                .map_err(|e| NexusApiError::Internal {
                    category: format!(
                        "FILE_SYNC_FAILED: {}",
                        format_args!("failed to open final file for fsync: {e}")
                    ),
                })?;
        final_handle
            .sync_all()
            .await
            .map_err(|e| NexusApiError::Internal {
                category: format!(
                    "FILE_SYNC_FAILED: {}",
                    format_args!("failed to fsync final file: {e}")
                ),
            })?;
        // Durability: fsync the parent directory so the renamed entry is
        // committed to disk (QC3-S3).
        if let Some(parent) = abs_body.parent() {
            let dir = tokio::fs::File::open(parent)
                .await
                .map_err(|e| NexusApiError::Internal {
                    category: format!(
                        "DIR_SYNC_FAILED: {}",
                        format_args!("failed to open parent dir for fsync: {e}")
                    ),
                })?;
            dir.sync_all().await.map_err(|e| NexusApiError::Internal {
                category: format!(
                    "DIR_SYNC_FAILED: {}",
                    format_args!("failed to fsync parent dir: {e}")
                ),
            })?;
        }
        tx.commit().await.map_err(|e| NexusApiError::Internal {
            category: format!(
                "DATABASE_ERROR: {}",
                format_args!("chapter update tx commit: {e}")
            ),
        })?;
    }

    // Read back updated chapter
    let updated = nexus_local_db::work_chapters::get_chapter(pool, work_id, chapter, volume)
        .await
        .map_err(|e| NexusApiError::Internal {
            category: format!("DATABASE_ERROR: {e}"),
        })?;

    updated.map_or_else(
        || {
            Err(NexusApiError::NotFound {
                resource: format!("{work_id}/ch{chapter}"),
            })
        },
        |ch| Ok(serde_json::to_value(&ch).unwrap_or_else(|_| serde_json::json!({}))),
    )
}

/// `nexus.world.configure` — update world metadata.
#[allow(clippy::useless_let_if_seq)] // three independent if-let accumulations on `updated`
async fn execute_world_configure(
    req: &ToolExecuteRequest,
    context: &ToolContext,
    creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    let world_id =
        req.parameters["world_id"]
            .as_str()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "parameters.world_id".into(),
                reason: "must be a string".into(),
            })?;

    let pool = context.pool();

    ensure_world_accessible_for_creator(pool, creator_id, world_id).await?;

    let now = chrono::Utc::now().to_rfc3339();
    let mut updated = false;

    // Update title if provided
    if let Some(title) = req.parameters["title"].as_str() {
        if title.trim().is_empty() {
            return Err(NexusApiError::InvalidInput {
                field: "parameters.title".into(),
                reason: "must not be empty".into(),
            });
        }
        // SAFETY: dynamic SQL — runtime field updates on narrative_worlds.
        sqlx::query("UPDATE narrative_worlds SET title = ?, updated_at = ? WHERE world_id = ?")
            .bind(title)
            .bind(&now)
            .bind(world_id)
            .execute(pool)
            .await
            .map_err(|e| NexusApiError::Internal {
                category: format!(
                    "DATABASE_ERROR: {}",
                    format_args!("world title update: {e}")
                ),
            })?;
        updated = true;
    }

    // Update visibility if provided
    if let Some(visibility) = req.parameters["visibility"].as_str() {
        let valid = ["public", "private", "invited"].contains(&visibility);
        if !valid {
            return Err(NexusApiError::InvalidInput {
                field: "parameters.visibility".into(),
                reason: "must be one of: public, private, invited".into(),
            });
        }
        // SAFETY: dynamic SQL for visibility update.
        sqlx::query(
            "UPDATE narrative_worlds SET visibility = ?, updated_at = ? WHERE world_id = ?",
        )
        .bind(visibility)
        .bind(&now)
        .bind(world_id)
        .execute(pool)
        .await
        .map_err(|e| NexusApiError::Internal {
            category: format!(
                "DATABASE_ERROR: {}",
                format_args!("world visibility update: {e}")
            ),
        })?;
        updated = true;
    }

    // Update time_policy if provided
    if let Some(time_policy) = req.parameters["time_policy"].as_str() {
        let valid = ["manual", "auto_advance"].contains(&time_policy);
        if !valid {
            return Err(NexusApiError::InvalidInput {
                field: "parameters.time_policy".into(),
                reason: "must be one of: manual, auto_advance".into(),
            });
        }
        // SAFETY: dynamic SQL for time_policy update.
        sqlx::query(
            "UPDATE narrative_worlds SET time_policy = ?, updated_at = ? WHERE world_id = ?",
        )
        .bind(time_policy)
        .bind(&now)
        .bind(world_id)
        .execute(pool)
        .await
        .map_err(|e| NexusApiError::Internal {
            category: format!(
                "DATABASE_ERROR: {}",
                format_args!("world time_policy update: {e}")
            ),
        })?;
        updated = true;
    }

    Ok(serde_json::json!({
        "world_id": world_id,
        "updated": updated
    }))
}

/// `nexus.work.schedule.set` — link/unlink schedule ids to a work.
///
/// The mutation routes through the core Work authority (QC1-F-001 class, same
/// as the patch/finding-resolve/pool executors); the boundary keeps payload
/// validation and transport error mapping only.
async fn execute_work_schedule_set(
    req: &ToolExecuteRequest,
    context: &ToolContext,
    _creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    let work_id =
        req.parameters["work_id"]
            .as_str()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "parameters.work_id".into(),
                reason: "must be a string".into(),
            })?;

    let schedule_ids =
        req.parameters["schedule_ids"]
            .as_array()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "parameters.schedule_ids".into(),
                reason: "must be an array of schedule id strings".into(),
            })?;

    // Validate all entries are strings and collect the typed list for core.
    let mut ids = Vec::with_capacity(schedule_ids.len());
    for (i, id) in schedule_ids.iter().enumerate() {
        match id.as_str() {
            Some(id) => ids.push(id.to_string()),
            None => {
                return Err(NexusApiError::InvalidInput {
                    field: format!("parameters.schedule_ids[{i}]"),
                    reason: "must be a string".into(),
                });
            }
        }
    }

    let core = context.core()?;
    let principal = core.active_principal().await?;
    core.patch_work(
        &principal,
        work_id.to_string(),
        "http",
        WorkPatchRequest {
            schedule_ids: Some(ids),
            ..Default::default()
        },
    )
    .await?;

    Ok(serde_json::json!({
        "work_id": work_id,
        "schedule_ids": schedule_ids
    }))
}

/// `nexus.finding.resolve` — resolve/close a finding.
async fn execute_finding_resolve(
    req: &ToolExecuteRequest,
    context: &ToolContext,
    _creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    let finding_id =
        req.parameters["finding_id"]
            .as_str()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "parameters.finding_id".into(),
                reason: "must be a string".into(),
            })?;

    let resolution = req.parameters["resolution"]
        .as_str()
        .unwrap_or("resolved via tool");

    let core = context.core()?;
    let principal = core.active_principal().await?;

    // Single findings authority (QC1-F-001): principal verification, the
    // work-write gate, lifecycle transition validation and the not-found
    // mapping all live in the core; this boundary keeps only the payload.
    core.update_finding(
        &principal,
        finding_id.to_string(),
        UpdateFindingRequest {
            status: Some("resolved".to_string()),
            description: Some(format!("Resolved via tool: {resolution}")),
            ..Default::default()
        },
    )
    .await?;

    Ok(serde_json::json!({
        "finding_id": finding_id,
        "resolved": true
    }))
}

/// `nexus.pool.entry.manage` — add/remove/promote pool entries.
async fn execute_pool_entry_manage(
    req: &ToolExecuteRequest,
    context: &ToolContext,
    creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    let work_id =
        req.parameters["work_id"]
            .as_str()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "parameters.work_id".into(),
                reason: "must be a string".into(),
            })?;

    let action = req.parameters["action"]
        .as_str()
        .ok_or_else(|| NexusApiError::InvalidInput {
            field: "parameters.action".into(),
            reason: "must be a string (add, remove, promote, archive)".into(),
        })?;

    let core = context.core()?;
    let principal = core.active_principal().await?;

    match action {
        "add" | "promote" => {
            // Core authority owns the work-ownership precheck
            // (`resolve_owned_work`) and the write gate for promotion.
            core.promote_work_pool_entry(
                &principal,
                PromotePoolRequest {
                    work_id: work_id.to_string(),
                    set_default: None,
                },
            )
            .await?;
        }
        "remove" | "archive" => {
            // Creator-scoped read at the boundary resolves the entry id; the
            // archive mutation itself goes through the core authority.
            let entry = nexus_local_db::novel_pool_entries::get_pool_entry_by_work(
                context.pool(),
                creator_id,
                work_id,
            )
            .await
            .map_err(|e| NexusApiError::Internal {
                category: format!("POOL_ERROR: {e}"),
            })?
            .ok_or_else(|| NexusApiError::NotFound {
                resource: work_id.to_string(),
            })?;

            core.archive_work_pool_entry(
                &principal,
                ArchivePoolRequest {
                    entry_id: entry.entry_id,
                },
            )
            .await?;
        }
        _ => {
            return Err(NexusApiError::InvalidInput {
                field: "parameters.action".into(),
                reason: "must be one of: add, remove, promote, archive".into(),
            });
        }
    }

    Ok(serde_json::json!({
        "work_id": work_id,
        "action": action,
        "success": true
    }))
}

// ─── V1.54 P0: Registry wrappers for DF-46 write tools ─────────────────────

/// Registry wrapper: `nexus.kb_snapshot.write` — async passthrough.
pub(crate) fn registry_kb_snapshot_write<'a>(
    req: &'a ToolExecuteRequest,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_kb_snapshot_write(req, context, creator_id))
}

/// Registry wrapper: `nexus.manuscript.chapter.update` — async passthrough.
pub(crate) fn registry_manuscript_chapter_update<'a>(
    req: &'a ToolExecuteRequest,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_manuscript_chapter_update(req, context, creator_id))
}

/// Registry wrapper: `nexus.world.configure` — async passthrough.
pub(crate) fn registry_world_configure<'a>(
    req: &'a ToolExecuteRequest,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_world_configure(req, context, creator_id))
}

/// Registry wrapper: `nexus.work.schedule.set` — async passthrough.
pub(crate) fn registry_work_schedule_set<'a>(
    req: &'a ToolExecuteRequest,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_work_schedule_set(req, context, creator_id))
}

/// Registry wrapper: `nexus.finding.resolve` — async passthrough.
pub(crate) fn registry_finding_resolve<'a>(
    req: &'a ToolExecuteRequest,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_finding_resolve(req, context, creator_id))
}

/// Registry wrapper: `nexus.pool.entry.manage` — async passthrough.
pub(crate) fn registry_pool_entry_manage<'a>(
    req: &'a ToolExecuteRequest,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_pool_entry_manage(req, context, creator_id))
}

// ─── V1.59 P0: DF-47 manuscript & misc capability parity batch ────────────
//
// 9 catalog-only → shipped host tools (Track A). Each handler follows the
// existing execute_* + registry_* wrapper pattern. See
// `.mstar/plans/2026-06-22-v1.59-df47-manuscript-and-misc-capabilities.md`.

/// Maximum body size for a single `nexus.manuscript.write` call (1 MiB).
const MANUSCRIPT_WRITE_MAX_BYTES: usize = 1024 * 1024;

/// Canonical manuscript phases for `nexus.manuscript.phase.set` (spec §4).
///
/// Ordered: `brainstorm` → `draft` → `review` → `finalize`.
/// Backward transitions are rejected unless `force = true`.
const MANUSCRIPT_PHASES: &[&str] = &["brainstorm", "draft", "review", "finalize"];

fn phase_index(phase: &str) -> Option<usize> {
    MANUSCRIPT_PHASES.iter().position(|p| *p == phase)
}

/// T1: `nexus.manuscript.list` — list manuscripts (works) for the active creator.
async fn execute_manuscript_list(
    _req: &ToolExecuteRequest,
    context: &ToolContext,
    creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    let workspace_slug =
        read_active_workspace_slug(&context.nexus_home, creator_id).ok_or_else(|| {
            NexusApiError::Forbidden {
                resource: format!("{}: {}", "manuscript.list", "active workspace required"),
            }
        })?;

    let filters = works::WorkListFilters::default();
    let records = works::list_works(context.pool(), creator_id, &workspace_slug, &filters)
        .await
        .map_err(|e| NexusApiError::Internal {
            category: format!("DATABASE_ERROR: {e}"),
        })?;

    let manuscripts: Vec<serde_json::Value> = records
        .iter()
        .map(|w| {
            serde_json::json!({
                "work_id": w.work_id,
                "title": w.title,
                "work_ref": w.work_ref,
                "work_profile": w.work_profile,
                "current_stage": w.current_stage,
                "stage_status": w.stage_status,
                "total_planned_chapters": w.total_planned_chapters,
                "current_chapter": w.current_chapter,
            })
        })
        .collect();

    Ok(serde_json::json!({
        "manuscripts": manuscripts,
        "count": records.len(),
    }))
}

/// T2: `nexus.manuscript.read_range` — read a bounded content range from a chapter body.
#[allow(clippy::similar_names)] // the paired names are the domain vocabulary here
async fn execute_manuscript_read_range(
    req: &ToolExecuteRequest,
    context: &ToolContext,
    creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    let work_id =
        req.parameters["work_id"]
            .as_str()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "parameters.work_id".into(),
                reason: "must be a string".into(),
            })?;

    let chapter: i32 = req.parameters["chapter"]
        .as_i64()
        .ok_or_else(|| NexusApiError::InvalidInput {
            field: "parameters.chapter".into(),
            reason: "must be an integer".into(),
        })?
        .try_into()
        .map_err(|_| NexusApiError::InvalidInput {
            field: "parameters.chapter".into(),
            reason: "must be a valid i32 chapter number".into(),
        })?;

    let volume: i32 = req.parameters["volume"]
        .as_i64()
        .map_or(1, |v| i32::try_from(v).unwrap_or(1));

    // Verify work ownership (fail closed for cross-creator access).
    let pool = context.pool();
    let _work = works::get_work(pool, creator_id, work_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            category: format!("DATABASE_ERROR: {e}"),
        })?
        .ok_or_else(|| NexusApiError::Forbidden {
            resource: format!(
                "{}: {}",
                "work", "work not found or cross-creator access denied",
            ),
        })?;

    let chapter_record = nexus_local_db::work_chapters::get_chapter(pool, work_id, chapter, volume)
        .await
        .map_err(|e| NexusApiError::Internal {
            category: format!("DATABASE_ERROR: {e}"),
        })?
        .ok_or_else(|| NexusApiError::NotFound {
            resource: format!("{work_id}/ch{chapter}"),
        })?;

    let body_path = chapter_record
        .body_path
        .ok_or_else(|| NexusApiError::NotFound {
            resource: format!("{work_id}/ch{chapter}/body"),
        })?;

    let workspace_root = context
        .workspace_path()
        .ok_or_else(|| NexusApiError::Internal {
            category: format!("WORKSPACE_PATH_ERROR: {}", "workspace path not available"),
        })?;
    let workspace_root_path = Path::new(&workspace_root);
    let abs_body = workspace_root_path.join(&body_path);

    // W-002: defense-in-depth path guard — ensure body_path (DB-sourced) resolves
    // within the workspace root before reading. Delegates to the same canonical
    // helper used by fs/* and the manuscript write path. `must_exist` preserves
    // the existing read-path semantics: an existing file is canonicalized before
    // the component-wise prefix check; a missing-but-in-bounds file falls through
    // to the existing FILE_READ_FAILED behavior below.
    let must_exist = abs_body.exists();
    let abs_body = resolve_guarded_path_async(
        workspace_root_path.to_path_buf(),
        body_path.clone(),
        must_exist,
    )
    .await
    .map_err(|e| match e {
        // The core names the refusal; the chapter lane reports it as a
        // body-path validation failure.
        NexusApiError::Coded { message, .. } => NexusApiError::InvalidInput {
            field: "body_path".into(),
            reason: message,
        },
        other => other,
    })?;

    let content =
        tokio::fs::read_to_string(&abs_body)
            .await
            .map_err(|e| NexusApiError::Internal {
                category: format!(
                    "FILE_READ_FAILED: {}",
                    format_args!("failed to read manuscript body: {e}")
                ),
            })?;

    // Apply optional line range [start_line, end_line] (1-indexed inclusive).
    let start_line = req.parameters["start_line"]
        .as_i64()
        .map(|v| usize::try_from(v.max(1)).unwrap_or(1));
    let end_line = req.parameters["end_line"]
        .as_i64()
        .map(|v| usize::try_from(v.max(1)).unwrap_or(1));

    let (ranged_content, truncated, total_lines) =
        if let (Some(start), Some(end)) = (start_line, end_line) {
            let lines: Vec<&str> = content.lines().collect();
            let total = lines.len();
            let start_idx = (start - 1).min(total);
            let end_idx = end.min(total);
            let selected: Vec<&str> = lines
                .get(start_idx..end_idx)
                .map(<[&str]>::to_vec)
                .unwrap_or_default();
            (selected.join("\n"), end < total, total)
        } else {
            let total = content.lines().count();
            (content, false, total)
        };

    Ok(serde_json::json!({
        "work_id": work_id,
        "chapter": chapter,
        "volume": volume,
        "content": ranged_content,
        "range": {
            "start_line": start_line.unwrap_or(1),
            "end_line": end_line.unwrap_or(total_lines),
        },
        "total_lines": total_lines,
        "truncated": truncated,
    }))
}

/// T3: `nexus.manuscript.write` — write manuscript content within size quotas.
#[allow(clippy::similar_names)] // the paired names are the domain vocabulary here
async fn execute_manuscript_write(
    req: &ToolExecuteRequest,
    context: &ToolContext,
    creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    let work_id =
        req.parameters["work_id"]
            .as_str()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "parameters.work_id".into(),
                reason: "must be a string".into(),
            })?;

    let chapter: i32 = req.parameters["chapter"]
        .as_i64()
        .ok_or_else(|| NexusApiError::InvalidInput {
            field: "parameters.chapter".into(),
            reason: "must be an integer".into(),
        })?
        .try_into()
        .map_err(|_| NexusApiError::InvalidInput {
            field: "parameters.chapter".into(),
            reason: "must be a valid i32 chapter number".into(),
        })?;

    let volume: i32 = req.parameters["volume"]
        .as_i64()
        .map_or(1, |v| i32::try_from(v).unwrap_or(1));

    let content =
        req.parameters["content"]
            .as_str()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "parameters.content".into(),
                reason: "must be a string".into(),
            })?;

    // Size quota check (spec §4: "within whitelist paths and size quotas").
    let content_bytes = content.len();
    if content_bytes > MANUSCRIPT_WRITE_MAX_BYTES {
        return Err(NexusApiError::InvalidInput {
            field: "parameters.content".into(),
            reason: format!(
                "content size {content_bytes} exceeds maximum {MANUSCRIPT_WRITE_MAX_BYTES} bytes"
            ),
        });
    }

    // Verify work ownership.
    let pool = context.pool();
    let _work = works::get_work(pool, creator_id, work_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            category: format!("DATABASE_ERROR: {e}"),
        })?
        .ok_or_else(|| NexusApiError::Forbidden {
            resource: format!(
                "{}: {}",
                "work", "work not found or cross-creator access denied",
            ),
        })?;

    // Chapter must exist (manuscript.write does not create chapters).
    let chapter_record = nexus_local_db::work_chapters::get_chapter(pool, work_id, chapter, volume)
        .await
        .map_err(|e| NexusApiError::Internal {
            category: format!("DATABASE_ERROR: {e}"),
        })?
        .ok_or_else(|| NexusApiError::NotFound {
            resource: format!("{work_id}/ch{chapter}"),
        })?;

    let body_path = chapter_record
        .body_path
        .ok_or_else(|| NexusApiError::Internal {
            category: format!(
                "CHAPTER_BODY_MISSING: {}",
                format_args!("chapter {work_id}/ch{chapter} has no body_path")
            ),
        })?;

    let workspace_root = context
        .workspace_path()
        .ok_or_else(|| NexusApiError::Internal {
            category: format!("WORKSPACE_PATH_ERROR: {}", "workspace path not available"),
        })?;
    let workspace_root_path = Path::new(&workspace_root);

    // W-002: defense-in-depth path guard — ensure body_path (DB-sourced) resolves
    // within the workspace root before any FS op. Uses the same canonicalize +
    // component-wise prefix-check helper as the chapter PUT handler.
    let abs_body =
        resolve_guarded_path_async(workspace_root_path.to_path_buf(), body_path.clone(), false)
            .await
            .map_err(|e| match e {
                NexusApiError::InvalidInput { field, .. } if field == "chapter_path_forbidden" => {
                    NexusApiError::InvalidInput {
                        field: "body_path".into(),
                        reason: "body path outside workspace root".into(),
                    }
                }
                other => other,
            })?;

    // Ensure parent directory exists.
    if let Some(parent) = abs_body.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| NexusApiError::Internal {
                category: format!(
                    "DIR_CREATE_FAILED: {}",
                    format_args!("failed to create manuscript dir: {e}")
                ),
            })?;
    }

    // Stage content in a temp file (not durable until rename succeeds).
    let tmp_file = abs_body.with_extension("md.tmp");
    tokio::fs::write(&tmp_file, content)
        .await
        .map_err(|e| NexusApiError::Internal {
            category: format!(
                "FILE_WRITE_FAILED: {}",
                format_args!("failed to write manuscript body: {e}")
            ),
        })?;
    // Durability: fsync temp file before the atomic rename.
    let tmp_handle =
        tokio::fs::File::open(&tmp_file)
            .await
            .map_err(|e| NexusApiError::Internal {
                category: format!(
                    "FILE_SYNC_FAILED: {}",
                    format_args!("failed to open temp file for fsync: {e}")
                ),
            })?;
    tmp_handle
        .sync_all()
        .await
        .map_err(|e| NexusApiError::Internal {
            category: format!(
                "FILE_SYNC_FAILED: {}",
                format_args!("failed to fsync temp file: {e}")
            ),
        })?;

    // W-001: wrap the DB word-count update + atomic rename in a single
    // transaction so the metadata update only commits when the final file is in
    // place. Mirrors the execute_manuscript_chapter_update C-002 pattern
    // (UPDATE → rename → commit); a failed rename returns early and the
    // dropped transaction rolls back the word-count UPDATE.
    let word_count = content.split_whitespace().count();
    let now = chrono::Utc::now().to_rfc3339();
    let mut tx = pool.begin().await.map_err(|e| NexusApiError::Internal {
        category: format!(
            "DATABASE_ERROR: {}",
            format_args!("manuscript.write tx begin: {e}")
        ),
    })?;
    // SAFETY: UPDATE against work_chapters — runtime query.
    sqlx::query(
        "UPDATE work_chapters SET actual_word_count = ?, updated_at = ? \
         WHERE work_id = ? AND chapter = ? AND volume = ?",
    )
    .bind(
        i64::try_from(word_count).map_err(|_| NexusApiError::Internal {
            category: format!(
                "WORK_WORD_COUNT_OVERFLOW: {}",
                format_args!("word_count {word_count} exceeds i64")
            ),
        })?,
    )
    .bind(&now)
    .bind(work_id)
    .bind(chapter)
    .bind(volume)
    .execute(&mut *tx)
    .await
    .map_err(|e| NexusApiError::Internal {
        category: format!(
            "DATABASE_ERROR: {}",
            format_args!("manuscript.write word-count update: {e}")
        ),
    })?;
    // Atomically rename temp → final inside the tx (after the UPDATE succeeds).
    tokio::fs::rename(&tmp_file, &abs_body).await.map_err(|e| {
        // Best-effort cleanup: remove temp file on rename failure; the dropped
        // tx rolls back the word-count UPDATE above.
        let _ = std::fs::remove_file(&tmp_file);
        NexusApiError::Internal {
            category: format!(
                "FILE_RENAME_FAILED: {}",
                format_args!("failed to finalize manuscript file: {e}")
            ),
        }
    })?;
    // Durability: fsync the final file after the atomic rename so a crash
    // after rename() returns does not leave the rename unflushed.
    let final_handle =
        tokio::fs::File::open(&abs_body)
            .await
            .map_err(|e| NexusApiError::Internal {
                category: format!(
                    "FILE_SYNC_FAILED: {}",
                    format_args!("failed to open final file for fsync: {e}")
                ),
            })?;
    final_handle
        .sync_all()
        .await
        .map_err(|e| NexusApiError::Internal {
            category: format!(
                "FILE_SYNC_FAILED: {}",
                format_args!("failed to fsync final file: {e}")
            ),
        })?;
    // Durability: fsync the parent directory so the renamed entry is committed
    // to disk (QC3-S3).
    if let Some(parent) = abs_body.parent() {
        let dir = tokio::fs::File::open(parent)
            .await
            .map_err(|e| NexusApiError::Internal {
                category: format!(
                    "DIR_SYNC_FAILED: {}",
                    format_args!("failed to open parent dir for fsync: {e}")
                ),
            })?;
        dir.sync_all().await.map_err(|e| NexusApiError::Internal {
            category: format!(
                "DIR_SYNC_FAILED: {}",
                format_args!("failed to fsync parent dir: {e}")
            ),
        })?;
    }
    tx.commit().await.map_err(|e| NexusApiError::Internal {
        category: format!(
            "DATABASE_ERROR: {}",
            format_args!("manuscript.write tx commit: {e}")
        ),
    })?;

    Ok(serde_json::json!({
        "written": true,
        "work_id": work_id,
        "chapter": chapter,
        "volume": volume,
        "word_count": word_count,
        "bytes_written": content_bytes,
    }))
}

/// T4: `nexus.manuscript.phase.get` — read current manuscript phase.
async fn execute_manuscript_phase_get(
    req: &ToolExecuteRequest,
    context: &ToolContext,
    creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    let work_id =
        req.parameters["work_id"]
            .as_str()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "parameters.work_id".into(),
                reason: "must be a string".into(),
            })?;

    let (current_stage, stage_status) = works::get_work_stage(context.pool(), creator_id, work_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            category: format!("DATABASE_ERROR: {e}"),
        })?
        .ok_or_else(|| NexusApiError::Forbidden {
            resource: format!(
                "{}: {}",
                "work", "work not found or cross-creator access denied",
            ),
        })?;

    Ok(serde_json::json!({
        "work_id": work_id,
        "phase": current_stage,
        "stage_status": stage_status,
    }))
}

/// T5: `nexus.manuscript.phase.set` — move between brainstorm/draft/review/finalize.
async fn execute_manuscript_phase_set(
    req: &ToolExecuteRequest,
    context: &ToolContext,
    creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    let work_id =
        req.parameters["work_id"]
            .as_str()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "parameters.work_id".into(),
                reason: "must be a string".into(),
            })?;

    let new_phase =
        req.parameters["phase"]
            .as_str()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "parameters.phase".into(),
                reason: "must be a string".into(),
            })?;

    // Runtime check: phase must be in canonical set.
    let new_idx = phase_index(new_phase).ok_or_else(|| NexusApiError::InvalidInput {
        field: "parameters.phase".into(),
        reason: format!("phase '{new_phase}' is not in canonical set {MANUSCRIPT_PHASES:?}"),
    })?;

    let force = req.parameters["force"].as_bool().unwrap_or(false);

    let pool = context.pool();

    let (current_stage, _stage_status) = works::get_work_stage(pool, creator_id, work_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            category: format!("DATABASE_ERROR: {e}"),
        })?
        .ok_or_else(|| NexusApiError::Forbidden {
            resource: format!(
                "{}: {}",
                "work", "work not found or cross-creator access denied",
            ),
        })?;

    let previous_phase = current_stage.clone();

    // Runtime check: backward transitions require explicit force.
    // Unknown current phase is treated as index 0 (brainstorm).
    let current_idx = phase_index(&current_stage).unwrap_or(0);
    if new_idx < current_idx && !force {
        return Err(NexusApiError::InvalidInput {
            field: "parameters.phase".into(),
            reason: format!(
                "backward transition '{previous_phase}' → '{new_phase}' requires force=true"
            ),
        });
    }

    let now = chrono::Utc::now().to_rfc3339();
    let updated = works::update_work_stage(pool, creator_id, work_id, new_phase, "active", &now)
        .await
        .map_err(|e| NexusApiError::Internal {
            category: format!("DATABASE_ERROR: {e}"),
        })?;

    Ok(serde_json::json!({
        "work_id": work_id,
        "previous_phase": previous_phase,
        "current_phase": updated.current_stage,
        "stage_status": updated.stage_status,
        "transitioned": previous_phase != updated.current_stage,
    }))
}

/// T6: `nexus.workspace.paths` — enumerate allowed roots from active workspace.
fn execute_workspace_paths(
    _req: &ToolExecuteRequest,
    context: &ToolContext,
    _creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    let workspace_root = context
        .workspace_path()
        .ok_or_else(|| NexusApiError::InvalidInput {
            field: "workspace".into(),
            reason: "workspace not initialized; run `nexus42 workspace init` first".into(),
        })?;

    // Allowed roots mirror the standard manuscript/workspace layout.
    let allowed_roots = vec![
        format!("{workspace_root}/Works"),
        format!("{workspace_root}/Worlds"),
        format!("{workspace_root}/References"),
        format!("{workspace_root}/.nexus42"),
    ];

    Ok(serde_json::json!({
        "workspace_root": workspace_root,
        "allowed_roots": allowed_roots,
        "preset_id": "default",
    }))
}

/// T7: `nexus.research.query` — query local-only `ReferenceSource` index.
async fn execute_research_query(
    req: &ToolExecuteRequest,
    context: &ToolContext,
    creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    // Direct lookup by reference_source_id takes precedence.
    let pool = context.pool();
    if let Some(id) = req.parameters["reference_source_id"].as_str() {
        // Scoped lookup: a source belonging to another creator must be
        // indistinguishable from one that does not exist, so the refusal is
        // the SAME `NotFound` either way — a 403 would confirm existence.
        let row = nexus_local_db::reference_source::find_by_id_for_creator(pool, id, creator_id)
            .await
            .map_err(|e| NexusApiError::Internal {
                category: format!("DATABASE_ERROR: {e}"),
            })?
            .ok_or_else(|| NexusApiError::NotFound {
                resource: id.to_string(),
            })?;

        return Ok(serde_json::json!({
            "results": [serde_json::json!({
                "reference_source_id": row.reference_source_id,
                "title": row.title,
                "uri": row.uri,
                "source_type": row.source_type,
                "tags": row.tags,
                "scan_status": row.scan_status,
            })],
            "count": 1,
        }));
    }

    let limit = req.parameters["limit"]
        .as_i64()
        .map_or(50, |v| v.clamp(1, 1000));

    // Scoped list: an unscoped call returns every creator's rows.
    let rows = nexus_local_db::reference_source::list(pool, Some(limit), None, Some(creator_id))
        .await
        .map_err(|e| NexusApiError::Internal {
            category: format!("DATABASE_ERROR: {e}"),
        })?;

    // Optional client-side tag filter.
    let tag_filter: Option<&str> = req.parameters["tags"].as_str();
    let filtered: Vec<_> = tag_filter.map_or_else(
        || rows.iter().collect(),
        |tag| {
            rows.iter()
                .filter(|r| {
                    r.tags
                        .as_deref()
                        .is_some_and(|t| t.split(',').any(|x| x.trim() == tag))
                })
                .collect()
        },
    );

    let results: Vec<serde_json::Value> = filtered
        .iter()
        .map(|r| {
            serde_json::json!({
                "reference_source_id": r.reference_source_id,
                "title": r.title,
                "uri": r.uri,
                "source_type": r.source_type,
                "tags": r.tags,
                "scan_status": r.scan_status,
            })
        })
        .collect();

    Ok(serde_json::json!({
        "results": results,
        "count": results.len(),
    }))
}

/// T8: `nexus.runtime.health` — agent-visible runtime health (distinct from
/// `nexus.observability.daemon.health` which exposes uptime + lifecycle).
///
/// Returns registry reachability, sync context, and cloud-enabled flag.
fn execute_runtime_health(
    _req: &ToolExecuteRequest,
    context: &ToolContext,
    _creator_id: &str,
) -> serde_json::Value {
    let reg = host_tool_registry();
    let runtime_mode = context.runtime_facts.runtime_mode_as_str();
    let cloud_enabled = !matches!(
        *context.runtime_mode(),
        nexus_contracts::local::domain::RuntimeMode::LocalOnly
    );

    serde_json::json!({
        "runtime_mode": runtime_mode,
        "registry_reachable": true,
        "registry_size": reg.len(),
        "sync_state": if cloud_enabled { "idle" } else { "disabled" },
        "cloud_enabled": cloud_enabled,
        "pool_healthy": true,
    })
}

/// T9: `nexus.trace.correlation` — propagate correlation IDs across tool calls.
///
/// Echoes the incoming `correlation_id` (or generates one if absent) so agents
/// can thread trace context through multi-step tool chains.
fn execute_trace_correlation(
    req: &ToolExecuteRequest,
    _context: &ToolContext,
    _creator_id: &str,
) -> serde_json::Value {
    let correlation_id = req.parameters["correlation_id"].as_str().map_or_else(
        || format!("corr_{}", uuid::Uuid::new_v4().simple()),
        str::to_string,
    );
    let session_id = req
        .session_id
        .clone()
        .or_else(|| req.parameters["session_id"].as_str().map(str::to_string));
    let parent_request_id = req.request_id.clone();
    let trace_timestamp = chrono::Utc::now().to_rfc3339();

    serde_json::json!({
        "correlation_id": correlation_id,
        "session_id": session_id,
        "parent_request_id": parent_request_id,
        "trace_timestamp": trace_timestamp,
        "propagated": true,
    })
}

// ─── V1.59 P0: Registry wrappers for the 9 new tools ──────────────────────

/// Registry wrapper: `nexus.manuscript.list` — async passthrough.
pub(crate) fn registry_manuscript_list<'a>(
    req: &'a ToolExecuteRequest,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_manuscript_list(req, context, creator_id))
}

/// Registry wrapper: `nexus.manuscript.read_range` — async passthrough.
pub(crate) fn registry_manuscript_read_range<'a>(
    req: &'a ToolExecuteRequest,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_manuscript_read_range(req, context, creator_id))
}

/// Registry wrapper: `nexus.manuscript.write` — async passthrough.
pub(crate) fn registry_manuscript_write<'a>(
    req: &'a ToolExecuteRequest,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_manuscript_write(req, context, creator_id))
}

/// Registry wrapper: `nexus.manuscript.phase.get` — async passthrough.
pub(crate) fn registry_manuscript_phase_get<'a>(
    req: &'a ToolExecuteRequest,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_manuscript_phase_get(req, context, creator_id))
}

/// Registry wrapper: `nexus.manuscript.phase.set` — async passthrough.
pub(crate) fn registry_manuscript_phase_set<'a>(
    req: &'a ToolExecuteRequest,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_manuscript_phase_set(req, context, creator_id))
}

/// Registry wrapper: `nexus.workspace.paths` — sync → async wrapper.
pub(crate) fn registry_workspace_paths<'a>(
    req: &'a ToolExecuteRequest,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    let result = execute_workspace_paths(req, context, creator_id);
    Box::pin(async move { result })
}

/// Registry wrapper: `nexus.research.query` — async passthrough.
pub(crate) fn registry_research_query<'a>(
    req: &'a ToolExecuteRequest,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_research_query(req, context, creator_id))
}

/// Registry wrapper: `nexus.runtime.health` — sync → async wrapper.
pub(crate) fn registry_runtime_health<'a>(
    req: &'a ToolExecuteRequest,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    let result = execute_runtime_health(req, context, creator_id);
    Box::pin(async move { Ok(result) })
}

/// Registry wrapper: `nexus.trace.correlation` — sync → async wrapper.
pub(crate) fn registry_trace_correlation<'a>(
    req: &'a ToolExecuteRequest,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    let result = execute_trace_correlation(req, context, creator_id);
    Box::pin(async move { Ok(result) })
}

// ---------------------------------------------------------------------------
// The capability registry (builtin rows + spine)
// ---------------------------------------------------------------------------

// ─── Registry types ────────────────────────────────────────────────────────

/// Unified handler function signature for all registered capabilities.
///
/// Takes references to the tool request, workspace context, and creator id,
/// returns a boxed future resolving to `Result<serde_json::Value, NexusApiError>`.
pub type RegistryHandlerFn = for<'a> fn(
    &'a ToolExecuteRequest,
    &'a ToolContext,
    &'a str,
) -> Pin<
    Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>,
>;

// ─── Field types ───────────────────────────────────────────────────────────

/// Access classification for a capability row.
///
/// Used by admission gates and audit to determine the
/// risk profile of a capability at dispatch time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Access {
    /// Read-only; no side effects.
    Read,
    /// Mutation-capable; may write to DB, filesystem, or state.
    Write,
    /// Access depends on runtime policy (e.g. `permissions.toml`
    /// or DA-005 `ContextPermissionGrant`).
    PolicyGated,
}

/// Ordered fail-closed admission gate before handler dispatch.
///
/// Each gate must pass (or be explicitly skipped for a given
/// capability) before the handler is invoked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdmissionGate {
    /// Tool ID must be in the allowlist.
    Allowlist,
    /// Active creator must exist (for `nexus.*` tools).
    ActiveCreator,
    /// Operation must be within workspace bounds.
    WorkspaceBounds,
    /// `permissions.toml` / policy must grant the capability.
    PermissionPolicy,
    /// World must exist and be owned by the active creator.
    RequireWorldOwnership,
    /// Audit log entry must be written (always last gate).
    AuditLog,
}

/// Catalog descriptor for a capability (AR-78, DF-89).
///
/// Carries the authored tool summary plus real draft-2020-12 JSON-Schema
/// text for the input (and, where pinned, output) shape. `&'static str`
/// literals fit the `LazyLock` static-row design exactly — no parse at
/// registry build, no new dependency. The schema text is the single source
/// of truth: it flows registry → catalog route → MCP child parse.
#[derive(Debug, Clone)]
pub struct CatalogDescriptor {
    /// Authored tool summary for LLM/script consumers (replaces the
    /// `TestVector.description` reuse that ended with AR-78 #4).
    pub description: &'static str,
    /// Real draft-2020-12 JSON-Schema text (root `"type":"object"`), or
    /// `None` when the input schema is not yet authored — the catalog then
    /// emits the named placeholder (`NAMED_PLACEHOLDER_INPUT`) and the id
    /// MUST appear in `SCHEMA_REMAINDER_LEDGER` (lockstep-pinned).
    pub input_schema: Option<&'static str>,
    /// Real draft-2020-12 JSON-Schema text for the success shape when the
    /// handler's output is a stable object worth pinning; `None` otherwise
    /// (omission is honest and rule-based — no ledger entry for outputs).
    pub output_schema: Option<&'static str>,
}

/// Stable failure mode contract for a capability.
///
/// Defines the error surface a caller can expect when
/// the capability is denied or fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailureMode {
    /// Capability is not supported in this runtime configuration.
    NotSupported,
    /// Policy (permissions or admission gate) blocked execution.
    PolicyBlocked,
    /// Authentication/authorization failed.
    Forbidden,
    /// Input validation failed.
    InvalidInput,
    /// Internal error (database, filesystem, etc.).
    Internal,
}

/// Test vector descriptor for a capability row.
///
/// Each row must have at least one success and one
/// failure test proving the handler works correctly.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestVector {
    /// Human-readable description of what the test covers.
    pub description: &'static str,
    /// Expected outcome: "success", "`failure:policy_blocked`", etc.
    pub expected_outcome: &'static str,
    /// Name of the test function (for grep-ability).
    pub test_fn_name: &'static str,
}

// ─── Capability row ────────────────────────────────────────────────────────

/// A single row in the capability registry.
///
/// Bundles all 7 fields: id, access, admission gates,
/// handler binding, catalog descriptor, failure mode contract,
/// and test vector.
#[derive(Clone)]
pub struct CapabilityRow {
    /// Stable `nexus.*` capability id (e.g. `"nexus.work.get"`).
    pub id: &'static str,
    /// Access classification.
    pub access: Access,
    /// Ordered fail-closed admission gates (&'static since V1.54 P0 T5).
    pub admission: &'static [AdmissionGate],
    /// Handler function binding.
    pub handler: RegistryHandlerFn,
    /// Catalog descriptor (authored description + real draft-2020-12
    /// schemas; AR-78 — replaces the removed `AcpWire`).
    pub catalog: CatalogDescriptor,
    /// Expected failure mode when denied.
    pub failure_mode: FailureMode,
    /// Test vector descriptor.
    pub handler_test_vector: TestVector,
}

// ─── Schema remainder ledger (AR-78 #6) ────────────────────────────────────

/// Named input-schema placeholder emitted by the catalog for a builtin row
/// whose input schema is not yet authored (`input_schema: None`).
///
/// Draft-2020-12-valid (`$comment` is ignorable by validators) and
/// machine-distinguishable from a real schema — never the silent
/// `{"type":"object"}` placeholder of V1.174.
pub const NAMED_PLACEHOLDER_INPUT: &str =
    r#"{"type":"object","$comment":"nexus42:schema-pending"}"#;

/// Registry-source ledger of builtin ids whose input schema is not yet
/// authored. Lockstep pin: `row.catalog.input_schema.is_none() ⇔ id ∈ LEDGER`
/// (unit-tested both directions). Task 2 converted all 30 static rows, so
/// the ledger is empty — the pin stays as the guard against future rows
/// being added without a schema. Test-only: the catalog route reads
/// `NAMED_PLACEHOLDER_INPUT`, never this ledger.
#[cfg(test)]
#[allow(dead_code)] // lockstep pin: catalog rows with unauthored schemas must be recorded here
pub(crate) const SCHEMA_REMAINDER_LEDGER: &[&str] = &[];

// ─── Registry ──────────────────────────────────────────────────────────────

/// Central registry for `nexus.*` host tool capabilities.
///
/// Built once at daemon startup. Provides O(1) lookup by
/// capability id and a unified `dispatch()` method that
/// mirrors the old `dispatch_tool()` behavior.
pub struct CapabilityRegistry {
    rows: HashMap<&'static str, CapabilityRow>,
}

impl CapabilityRegistry {
    /// Create an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            rows: HashMap::new(),
        }
    }

    /// Register a capability row.
    ///
    /// # Panics
    ///
    /// Panics if a row with the same `id` is already registered
    /// (duplicate capability ids are a programmer error).
    pub fn register(&mut self, row: CapabilityRow) {
        assert!(
            !self.rows.contains_key(row.id),
            "duplicate capability id in registry: {}",
            row.id
        );
        self.rows.insert(row.id, row);
    }

    /// Look up a capability row by id.
    #[must_use]
    pub fn lookup(&self, id: &str) -> Option<&CapabilityRow> {
        self.rows.get(id)
    }

    /// Iterate over all registered capability ids.
    pub fn ids(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.rows.keys().copied()
    }

    /// Number of registered capabilities.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Return whether the registry is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Whether `id` resolves anywhere in the dispatch spine.
    ///
    /// Single-table spine resolution (AR-68 #4/#6): static rows →
    /// `PeerToolTable` (behind `connect-client`) → orchestration user
    /// capabilities (`origin() == User` only). An id is dispatchable iff it
    /// resolves here; unknown ids yield `not_supported` exactly like an
    /// unknown builtin.
    #[must_use]
    pub fn spine_resolves(&self, context: &ToolContext, id: &str) -> bool {
        if self.lookup(id).is_some() {
            return true;
        }
        if super::peer_tools::peer_tool_registry().get(id).is_some() {
            return true;
        }
        context
            .user_capabilities()
            .is_some_and(|reg| user_cap_catalog_admission(reg.get(id)).is_ok())
    }

    /// Dispatch a tool request through the registry.
    ///
    /// Looks up the capability by `tool_name`, iterates the declared
    /// `AdmissionGate` slice as a centralized accountability checkpoint,
    /// then invokes the registered handler.
    ///
    /// **Gate enforcement split** (W-001 fix):
    /// - Gates 1-4 (`Allowlist`, `ActiveCreator`, `WorkspaceBounds`,
    ///   `PermissionPolicy`) are enforced by `admission_pipeline` before
    ///   `dispatch` is called.
    /// - `RequireWorldOwnership` is enforced by per-handler checks
    ///   (e.g. `ensure_world_accessible_for_creator`).
    /// - `AuditLog` is enforced by the caller (`audit_tool_execution`
    ///   in `registry_dispatch`).
    ///
    /// The invariant test `registry_all_admission_gates_have_enforcement`
    /// proves every gate in every row has a corresponding runtime check.
    ///
    /// # Panics
    ///
    /// Never in practice: the `expect` after the early-return lookup is
    /// unreachable because registry rows are insert-only and the
    /// not-found arm returns above.
    ///
    /// # Errors
    ///
    /// Returns a `Coded` refusal with code `not_supported`
    /// if the tool is not registered. Individual handlers may return
    /// other error variants (e.g. `Forbidden`, `InvalidInput`).
    pub async fn dispatch(
        &self,
        req: &ToolExecuteRequest,
        context: &ToolContext,
        creator_id: &str,
    ) -> Result<serde_json::Value, NexusApiError> {
        let row = self.lookup(&req.tool_name);
        if row.is_none() {
            // Peer arm (AR-68 #4): reverse-invoke the owning responder.
            if let Some(entry) = super::peer_tools::peer_tool_registry().get(&req.tool_name) {
                return dispatch_peer_tool(&entry, req).await;
            }
            // User-capability arm (AR-68 #6): `Capability::run(arguments)`.
            if let Some(reg) = context.user_capabilities() {
                if let Ok(cap) = user_cap_catalog_admission(reg.get(&req.tool_name)) {
                    return dispatch_user_cap(cap, req).await;
                }
            }
            return Err(NexusApiError::Coded {
                code: "not_supported".to_string(),
                message: format!("unsupported tool: {}", req.tool_name),
            });
        }
        let row = row.expect("row present");

        // Centralized admission-gate accountability checkpoint.
        // Each gate type MUST have a corresponding enforcement path (pipeline,
        // handler, or caller). The invariant test below validates this mapping
        // at registration time.
        for gate in row.admission {
            debug_assert!(
                matches!(
                    gate,
                    AdmissionGate::Allowlist
                        | AdmissionGate::ActiveCreator
                        | AdmissionGate::WorkspaceBounds
                        | AdmissionGate::PermissionPolicy
                        | AdmissionGate::RequireWorldOwnership
                        | AdmissionGate::AuditLog
                ),
                "unhandled admission gate {gate:?} for capability {}",
                row.id
            );
            let _ = gate; // Readability: gate is accounted for by the match above.
        }

        (row.handler)(req, context, creator_id).await
    }
}

impl Default for CapabilityRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Spine peer + user-capability arms (AR-68 #4/#6) ──────────────────────

/// Peer dispatch arm: structural argument gate via
/// `validate_tool_arguments`, then reverse-invoke the owning responder./// Peer dispatch arm (AR-68 #4): structural argument gate, then the
/// reverse-invoke port.
///
/// The gate runs BEFORE any transport I/O — a malformed call is refused
/// locally and never reaches the peer. The port's own failures map onto the
/// honest-refusal matrix: a timeout and a mid-invoke disconnect are
/// distinguishable transport faults (never a peer deny), and every other
/// refusal keeps the peer's original lowercase wire code verbatim in
/// `details.wire_code`, so the spine's own `not_supported` never overwrites
/// the peer's more precise reason.
async fn dispatch_peer_tool(
    entry: &super::peer_tools::PeerToolEntry,
    req: &ToolExecuteRequest,
) -> Result<Value, NexusApiError> {
    use super::peer_tools::PeerInvokeError;
    match super::peer_tools::invoke_peer_tool(entry, req.parameters.clone()).await {
        Ok(value) => Ok(value),
        // A structural argument refusal never reached the peer; it is a
        // plain client-input error.
        Err(PeerInvokeError::Denied {
            wire_code: None,
            message,
        }) => Err(NexusApiError::Coded {
            code: "invalid_input".to_string(),
            message,
        }),
        // A peer-side deny keeps the peer's own lowercase code verbatim while
        // the public code stays the spine's `not_supported`.
        Err(PeerInvokeError::Denied {
            wire_code: Some(wire_code),
            message,
        }) => Err(NexusApiError::PeerDenied {
            code: "not_supported".to_string(),
            wire_code,
            message,
        }),
        Err(PeerInvokeError::Timeout { message }) => Err(NexusApiError::Internal {
            category: format!("PEER_TOOL_TIMEOUT: {message}"),
        }),
        Err(PeerInvokeError::Disconnected { message }) => Err(NexusApiError::Internal {
            category: format!("PEER_TOOL_DISCONNECTED: {message}"),
        }),
        Err(PeerInvokeError::Internal { message }) => Err(NexusApiError::Internal {
            category: format!("PEER_TOOL_FAILED: {message}"),
        }),
    }
}

/// Structural argument gate for the user-cap arm (AR-76 #2/#4, W-A):
/// the SAME spoke-granularity semantics as the peer arm's
/// `spoke_operations::validate_tool_arguments` — arguments must be a JSON
/// object and every declared top-level `required` key must be present;
/// otherwise `invalid_input` BEFORE any adapter I/O (WASM module load and
/// execution count as adapter I/O). No deeper JSON-Schema checking (V1.172
/// AR-37 posture). The peer arm keeps its grammar-typed spoke helper; this
/// helper implements the same refusal vocabulary against the capability's
/// declared `input_schema()` string.
fn validate_user_cap_arguments(schema_json: &str, arguments: &Value) -> Result<(), String> {
    let Some(object) = arguments.as_object() else {
        return Err("Tool arguments must be a JSON object".to_string());
    };
    let Ok(schema) = serde_json::from_str::<Value>(schema_json) else {
        // The catalog admission gate already proved the schema parses as a
        // JSON object; a parse failure here means the descriptor changed
        // after admission — fail closed as invalid_input rather than
        // dispatching unvalidated.
        return Err("capability input schema is not a JSON object".to_string());
    };
    if schema.get("type") != Some(&Value::String("object".to_string())) {
        return Ok(());
    }
    let Some(Value::Array(required)) = schema.get("required") else {
        return Ok(());
    };
    let missing: Vec<&str> = required
        .iter()
        .filter_map(Value::as_str)
        .filter(|key| !object.contains_key(*key))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    Err(format!(
        "Missing required tool arguments: {}",
        missing.join(", ")
    ))
}

/// User-capability dispatch arm (AR-68 #6): structural gate, then
/// `Capability::run(arguments)`. `CapabilityError` maps to the closest
/// `NexusApiError` variant.
async fn dispatch_user_cap(
    cap: &dyn nexus_orchestration::capability::Capability,
    req: &ToolExecuteRequest,
) -> Result<Value, NexusApiError> {
    use nexus_orchestration::capability::CapabilityError;
    // AR-76 #2/#4 (W-A): the structural gate fires before any adapter I/O —
    // same refusal vocabulary as the peer arm.
    if let Err(message) = validate_user_cap_arguments(cap.input_schema(), &req.parameters) {
        return Err(NexusApiError::Coded {
            code: "invalid_input".to_string(),
            message,
        });
    }
    cap.run(req.parameters.clone()).await.map_err(|e| match e {
        CapabilityError::InputInvalid(msg) => NexusApiError::Coded {
            code: "invalid_input".to_string(),
            message: msg,
        },
        CapabilityError::Forbidden(msg) => NexusApiError::Forbidden {
            resource: format!("{}: {}", "tool_execution", msg),
        },
        CapabilityError::WorkerUnavailable => NexusApiError::Internal {
            category: format!(
                "SERVICE_UNAVAILABLE: {}",
                format_args!("capability '{}' has no executor wired", cap.name()),
            ),
        },
        other => NexusApiError::Internal {
            category: format!(
                "CAPABILITY_RUN_FAILED: capability '{}' failed: {other}",
                cap.name()
            ),
        },
    })
}

/// Catalog admission for a user capability (AR-68 #6).
///
/// The name must not start with `nexus.` and must not match the peer grammar
/// `^tools\.…`; the declared `input_schema()` must parse as a JSON object.
/// Fail-closed — a non-admitted capability is neither dispatchable nor
/// listed.
pub fn user_cap_catalog_admission(
    cap: Option<&dyn nexus_orchestration::capability::Capability>,
) -> Result<&dyn nexus_orchestration::capability::Capability, UserCapCatalogRefusal> {
    let cap = cap.ok_or(UserCapCatalogRefusal::NotUserCapability)?;
    if cap.origin() != nexus_orchestration::capability::CapabilityOrigin::User {
        return Err(UserCapCatalogRefusal::NotUserCapability);
    }
    let name = cap.name();
    if name.starts_with("nexus.") {
        return Err(UserCapCatalogRefusal::ReservedNamespace);
    }
    // QC-fix S-d: the spoke-operations helper IS the grammar (single source
    // of truth with the peer arm — the local `matches_tools_grammar` mirror
    // was removed; spoke grammar is `tools.<ns>.<tool_id>` with ns
    // `^[a-z][a-z0-9_-]*$` and tool_id `^[a-z0-9][a-z0-9_-]*$`).
    if matches!(parse_tool_capability_id(name), SpokeResult::Ok(_)) {
        return Err(UserCapCatalogRefusal::ReservedNamespace);
    }
    if serde_json::from_str::<Value>(cap.input_schema())
        .ok()
        .and_then(|v| v.as_object().map(|_| ()))
        .is_none()
    {
        return Err(UserCapCatalogRefusal::InputSchemaNotObject);
    }
    Ok(cap)
}

/// AR-70 §3 inclusion rule for MCP `output_schema` payloads.
///
/// A JSON-Schema string is carried only when it parses and declares a root
/// `type: "object"` (MCP requires an object root; non-object outputs are
/// omitted, never invented, never wrapped). Shared by the peer merge
/// (connect-client) and the user-cap branch of the catalog.
#[must_use]
pub fn json_schema_has_object_root(raw: &str) -> bool {
    serde_json::from_str::<Value>(raw)
        .ok()
        .and_then(|v| {
            v.get("type")
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned)
        })
        .as_deref()
        == Some("object")
}

/// Named catalog refusal for a user capability (AR-68 #6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UserCapCatalogRefusal {
    /// Not a user capability (builtin or absent).
    NotUserCapability,
    /// Name starts with `nexus.` or matches the peer grammar.
    ReservedNamespace,
    /// `input_schema()` does not parse as a JSON object.
    InputSchemaNotObject,
}

// ─── Registry constructor ──────────────────────────────────────────────────

// ─── Registry constructor ──────────────────────────────────────────────────

/// Static admission gate arrays (defined once, referenced by all 19 rows).
const ADMISSION_READ_CONTEXT: &[AdmissionGate] = &[
    AdmissionGate::Allowlist,
    AdmissionGate::ActiveCreator,
    AdmissionGate::PermissionPolicy,
    AdmissionGate::AuditLog,
];

const ADMISSION_READ_WORKSPACE: &[AdmissionGate] = &[
    AdmissionGate::Allowlist,
    AdmissionGate::ActiveCreator,
    AdmissionGate::WorkspaceBounds,
    AdmissionGate::PermissionPolicy,
    AdmissionGate::AuditLog,
];

const ADMISSION_READ_WORLD: &[AdmissionGate] = &[
    AdmissionGate::Allowlist,
    AdmissionGate::ActiveCreator,
    AdmissionGate::RequireWorldOwnership,
    AdmissionGate::PermissionPolicy,
    AdmissionGate::AuditLog,
];

const ADMISSION_WRITE_WORKSPACE: &[AdmissionGate] = &[
    AdmissionGate::Allowlist,
    AdmissionGate::ActiveCreator,
    AdmissionGate::WorkspaceBounds,
    AdmissionGate::PermissionPolicy,
    AdmissionGate::AuditLog,
];

const ADMISSION_WRITE_WORLD: &[AdmissionGate] = &[
    AdmissionGate::Allowlist,
    AdmissionGate::ActiveCreator,
    AdmissionGate::RequireWorldOwnership,
    AdmissionGate::PermissionPolicy,
    AdmissionGate::AuditLog,
];

const ADMISSION_FS_READ: &[AdmissionGate] = &[
    AdmissionGate::Allowlist,
    AdmissionGate::WorkspaceBounds,
    AdmissionGate::PermissionPolicy,
    AdmissionGate::AuditLog,
];

const ADMISSION_FS_WRITE: &[AdmissionGate] = &[
    AdmissionGate::Allowlist,
    AdmissionGate::WorkspaceBounds,
    AdmissionGate::PermissionPolicy,
    AdmissionGate::AuditLog,
];

const ADMISSION_POOL_WRITE: &[AdmissionGate] = &[
    AdmissionGate::Allowlist,
    AdmissionGate::ActiveCreator,
    AdmissionGate::PermissionPolicy,
    AdmissionGate::AuditLog,
];

/// Create a registry pre-populated with all 30 host tools (28 `nexus.*` + 2
/// `fs/*`; V1.34 + V1.53 P1 + V1.54 P0 + V1.56 P1 + V1.58 P3 + V1.59 P0).
///
/// V1.54 P0 T5: Converted to `LazyLock` singleton to eliminate per-dispatch
/// allocation. All admission gates are `&'static [AdmissionGate]` references.
#[must_use]
pub fn host_tool_registry() -> &'static CapabilityRegistry {
    static REGISTRY: LazyLock<CapabilityRegistry> = LazyLock::new(build_registry);
    &REGISTRY
}

/// Builds the full registry (called once by `LazyLock`).
/// Marked `pub` so benchmarks can measure cold-path initialization;
/// external callers should use `host_tool_registry()` instead.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn build_registry() -> CapabilityRegistry {
    let mut reg = CapabilityRegistry::new();

    // ── nexus.* tools (V1.34) ──
    reg.register(CapabilityRow {
        id: "nexus.context.whoami",
        access: Access::Read,
        admission: ADMISSION_READ_CONTEXT,
        handler: registry_context_whoami,
        catalog: CatalogDescriptor {
            description: "Return the active creator id and workspace slug for the current session.",
            input_schema: Some(r#"{"type":"object","properties":{}}"#),
            output_schema: Some(
                r#"{"type":"object","properties":{"creator_id":{"type":"string"},"workspace_slug":{"type":"string"}},"required":["creator_id","workspace_slug"]}"#,
            ),
        },
        failure_mode: FailureMode::Forbidden,
        handler_test_vector: TestVector {
            description: "whoami returns active creator_id and workspace_slug",
            expected_outcome: "success",
            test_fn_name: "whoami_returns_active_creator",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.workspace.info",
        access: Access::Read,
        admission: ADMISSION_READ_CONTEXT,
        handler: registry_workspace_info,
        catalog: CatalogDescriptor {
            description: "Return workspace details: creator id, slug, path, runtime mode, and initialization state.",
            input_schema: Some(r#"{"type":"object","properties":{}}"#),
            output_schema: Some(
                r#"{"type":"object","properties":{"creator_id":{"type":"string"},"workspace_slug":{"type":"string"},"workspace_path":{"type":"string"},"runtime_mode":{"type":"string"},"initialized":{"type":"boolean"}},"required":["creator_id","workspace_slug","workspace_path","runtime_mode","initialized"]}"#,
            ),
        },
        failure_mode: FailureMode::Forbidden,
        handler_test_vector: TestVector {
            description: "workspace info returns workspace details",
            expected_outcome: "success",
            test_fn_name: "workspace_info_returns_details",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.work.get",
        access: Access::Read,
        admission: ADMISSION_READ_WORKSPACE,
        handler: registry_work_get,
        catalog: CatalogDescriptor {
            description: "Return the Work record fields exposed by the catalog: status, title, current stage, and stage status.",
            input_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"}},"required":["work_id"]}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"},"status":{"type":"string"},"title":{"type":"string"},"current_stage":{"type":"string"},"stage_status":{"type":"string"}},"required":["work_id","status","title","current_stage","stage_status"]}"#,
            ),
        },
        failure_mode: FailureMode::Forbidden,
        handler_test_vector: TestVector {
            description: "work get returns Work row for active creator",
            expected_outcome: "success",
            test_fn_name: "work_get_happy_path",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.work.patch",
        access: Access::Write,
        admission: ADMISSION_WRITE_WORKSPACE,
        handler: registry_work_patch,
        catalog: CatalogDescriptor {
            description: "Patch a work's title, inspiration log, or stage metadata (stage field itself is rejected).",
            input_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"},"title":{"type":"string"},"inspiration_log":{"type":"array","items":{"type":"object","properties":{"text":{"type":"string"},"note":{"type":"string"},"source":{"type":"string"}},"anyOf":[{"required":["text"]},{"required":["note"]}]}},"stage_metadata":{"type":"object","properties":{"agent_notes":{"type":"string"},"research_summary_ref":{"type":"string"},"draft_outline_ref":{"type":"string"},"review_summary_ref":{"type":"string"},"last_agent_tool_request_id":{"type":"string"}},"additionalProperties":false}},"required":["work_id"],"additionalProperties":false}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"},"status":{"type":"string"},"title":{"type":"string"},"current_stage":{"type":"string"},"stage_status":{"type":"string"}},"required":["work_id","status","title","current_stage","stage_status"]}"#,
            ),
        },
        failure_mode: FailureMode::Forbidden,
        handler_test_vector: TestVector {
            description: "work patch rejects stage field per spec §4.4",
            expected_outcome: "failure:invalid_input",
            test_fn_name: "work_patch_rejects_stage_field",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.orchestration.schedule_status",
        access: Access::Read,
        admission: ADMISSION_READ_WORKSPACE,
        handler: registry_schedule_status,
        catalog: CatalogDescriptor {
            description: "Return the schedule ids linked to a work and their count.",
            input_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"}},"required":["work_id"]}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"},"schedule_ids":{"type":"array","items":{"type":"string"}},"count":{"type":"integer"}},"required":["work_id","schedule_ids","count"]}"#,
            ),
        },
        failure_mode: FailureMode::Forbidden,
        handler_test_vector: TestVector {
            description: "schedule status returns schedule ids for work",
            expected_outcome: "success",
            test_fn_name: "schedule_status_happy_path",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.context.assemble",
        access: Access::Read,
        admission: ADMISSION_READ_CONTEXT,
        handler: registry_context_assemble,
        catalog: CatalogDescriptor {
            description: "Assemble the current context moment; requires_platform requests platform-integrated assembly.",
            input_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"},"requires_platform":{"type":"boolean"}}}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"mode":{"type":"string"},"creator_id":{"type":"string"},"assembled_at":{"type":"string"}},"required":["mode","creator_id","assembled_at"]}"#,
            ),
        },
        failure_mode: FailureMode::PolicyBlocked,
        handler_test_vector: TestVector {
            description: "context assemble returns policy_blocked in local-only mode with requires_platform",
            expected_outcome: "failure:policy_blocked",
            test_fn_name: "context_assemble_policy_blocked_when_local_only",
        },
    });

    // ── nexus.* tools (V1.53 P1: DF-46 read-heavy slice) ──
    reg.register(CapabilityRow {
        id: "nexus.world.snapshot.get",
        access: Access::Read,
        admission: ADMISSION_READ_WORLD,
        handler: registry_world_snapshot_get,
        catalog: CatalogDescriptor {
            description: "Return the world snapshot fields exposed by the catalog: title, slug, status, fork flag, and creation time.",
            input_schema: Some(
                r#"{"type":"object","properties":{"world_id":{"type":"string"}},"required":["world_id"]}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"world_id":{"type":"string"},"title":{"type":"string"},"slug":{"type":"string"},"status":{"type":"string"},"is_fork":{"type":"boolean"},"created_at":{"type":"string"}},"required":["world_id","title","slug","status","is_fork","created_at"]}"#,
            ),
        },
        failure_mode: FailureMode::InvalidInput,
        handler_test_vector: TestVector {
            description: "world snapshot get returns world state for valid world_id",
            expected_outcome: "success",
            test_fn_name: "world_snapshot_get_returns_world_state",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.timeline.recent.get",
        access: Access::Read,
        admission: ADMISSION_READ_WORLD,
        handler: registry_timeline_recent_get,
        catalog: CatalogDescriptor {
            description: "Return the most recent timeline events for a world (default 100, clamped to 500).",
            input_schema: Some(
                r#"{"type":"object","properties":{"world_id":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":500}},"required":["world_id"]}"#,
            ),
            // Output is an event array, not a stable object — omitted per
            // AR-78 #5 (output schema pinned only for stable object shapes).
            output_schema: None,
        },
        failure_mode: FailureMode::InvalidInput,
        handler_test_vector: TestVector {
            description: "timeline recent get returns recent events for valid world_id",
            expected_outcome: "success",
            test_fn_name: "timeline_recent_get_returns_recent_events",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.kb_snapshot.read",
        access: Access::Read,
        admission: ADMISSION_READ_WORLD,
        handler: registry_kb_snapshot_read,
        catalog: CatalogDescriptor {
            description: "Return the knowledge-base key blocks for an owned world.",
            input_schema: Some(
                r#"{"type":"object","properties":{"world_id":{"type":"string"}},"required":["world_id"]}"#,
            ),
            // Output is a key-block array, not a stable object — omitted per
            // AR-78 #5 (output schema pinned only for stable object shapes).
            output_schema: None,
        },
        failure_mode: FailureMode::InvalidInput,
        handler_test_vector: TestVector {
            description: "kb snapshot read returns key blocks for valid world_id",
            expected_outcome: "success",
            test_fn_name: "kb_snapshot_read_returns_key_blocks",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.manuscript.chapter.get",
        access: Access::Read,
        admission: ADMISSION_READ_WORKSPACE,
        handler: registry_manuscript_chapter_get,
        catalog: CatalogDescriptor {
            description: "Return the manuscript chapter fields exposed by the catalog: status and planned word count.",
            input_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"},"chapter":{"type":"integer","minimum":1},"volume":{"type":"integer","minimum":1}},"required":["work_id","chapter"]}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"},"chapter":{"type":"integer"},"volume":{"type":"integer"},"status":{"type":"string"},"planned_word_count":{"type":"integer"}},"required":["work_id","chapter","status","planned_word_count"]}"#,
            ),
        },
        failure_mode: FailureMode::InvalidInput,
        handler_test_vector: TestVector {
            description:
                "manuscript chapter get returns chapter record for valid work_id + chapter",
            expected_outcome: "success",
            test_fn_name: "manuscript_chapter_get_returns_chapter_record",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.observability.daemon.health",
        access: Access::Read,
        admission: ADMISSION_READ_CONTEXT,
        handler: registry_daemon_health,
        catalog: CatalogDescriptor {
            description: "Return daemon runtime health: uptime, lifecycle context, registry size and ids, pool health.",
            input_schema: Some(r#"{"type":"object","properties":{}}"#),
            output_schema: Some(
                r#"{"type":"object","properties":{"uptime_seconds":{"type":"integer"},"started_at":{"type":"string"},"runtime_mode":{"type":"string"},"lifecycle_state":{"type":"string"},"registry_size":{"type":"integer"},"registry_ids":{"type":"array","items":{"type":"string"}},"pool_healthy":{"type":"boolean"}},"required":["uptime_seconds","started_at","runtime_mode","lifecycle_state","registry_size","registry_ids","pool_healthy"]}"#,
            ),
        },
        failure_mode: FailureMode::Forbidden,
        handler_test_vector: TestVector {
            description: "daemon health returns runtime status and registry size",
            expected_outcome: "success",
            test_fn_name: "daemon_health_returns_registry_status",
        },
    });

    // ── V1.54 P0: DF-46 write tools ──
    reg.register(CapabilityRow {
        id: "nexus.kb_snapshot.write",
        access: Access::Write,
        admission: ADMISSION_WRITE_WORLD,
        handler: registry_kb_snapshot_write,
        catalog: CatalogDescriptor {
            description: "Upsert knowledge-base key blocks for an owned world.",
            input_schema: Some(
                r#"{"type":"object","properties":{"world_id":{"type":"string"},"blocks":{"type":"array","items":{"type":"object","properties":{"schema_version":{"type":"integer"},"entry_id":{"type":"string"},"world_id":{"type":"string"},"block_type":{"type":"string","enum":["character","ability","scene","organization","item","conflict","info_point","event","species","faction","magic_system","technology","deity","level","economy_tier","dialogue","beat","act","era"]},"canonical_name":{"type":"string"},"status":{"type":"string"},"revision":{"type":"integer"},"body":{"type":"object"},"source_anchor":{"type":"object"},"created_from_command_id":{"type":"string"},"created_at":{"type":"string"},"updated_at":{"type":"string"},"source_work_id":{"type":"string"},"source_chapter":{"type":"integer"},"source_provenance_kind":{"type":"string"},"extensions_nexus_extras":{"type":"object"},"modules":{"type":"object"}},"required":["schema_version","entry_id","world_id","block_type","canonical_name","status","created_at"]}}},"required":["world_id","blocks"]}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"written":{"type":"integer"},"world_id":{"type":"string"}},"required":["written","world_id"]}"#,
            ),
        },
        failure_mode: FailureMode::Forbidden,
        handler_test_vector: TestVector {
            description: "kb snapshot write upserts key blocks for owned world",
            expected_outcome: "success",
            test_fn_name: "kb_snapshot_write_upserts_key_blocks",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.manuscript.chapter.update",
        access: Access::Write,
        admission: ADMISSION_WRITE_WORKSPACE,
        handler: registry_manuscript_chapter_update,
        catalog: CatalogDescriptor {
            description: "Update a manuscript chapter's content for a work.",
            input_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"},"chapter":{"type":"integer","minimum":1},"volume":{"type":"integer","minimum":1},"content":{"type":"string"}},"required":["work_id","chapter"]}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"},"chapter":{"type":"integer"},"volume":{"type":"integer"},"slug":{"type":"string"},"planned_word_count":{"type":"integer"},"actual_word_count":{"type":"integer"},"status":{"type":"string"},"outline_path":{"type":"string"},"body_path":{"type":"string"},"created_at":{"type":"string"},"updated_at":{"type":"string"}},"required":["work_id","chapter","planned_word_count","status","created_at","updated_at"]}"#,
            ),
        },
        failure_mode: FailureMode::InvalidInput,
        handler_test_vector: TestVector {
            description: "manuscript chapter update writes chapter content for valid work",
            expected_outcome: "success",
            test_fn_name: "manuscript_chapter_update_writes_content",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.world.configure",
        access: Access::Write,
        admission: ADMISSION_WRITE_WORLD,
        handler: registry_world_configure,
        catalog: CatalogDescriptor {
            description: "Update metadata (title, visibility, time policy) for an owned world.",
            input_schema: Some(
                r#"{"type":"object","properties":{"world_id":{"type":"string"},"title":{"type":"string"},"visibility":{"type":"string","enum":["public","private","invited"]},"time_policy":{"type":"string","enum":["manual","auto_advance"]}},"required":["world_id"]}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"world_id":{"type":"string"},"updated":{"type":"boolean"}},"required":["world_id","updated"]}"#,
            ),
        },
        failure_mode: FailureMode::Forbidden,
        handler_test_vector: TestVector {
            description: "world configure updates world metadata for owned world",
            expected_outcome: "success",
            test_fn_name: "world_configure_updates_metadata",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.work.schedule.set",
        access: Access::Write,
        admission: ADMISSION_WRITE_WORKSPACE,
        handler: registry_work_schedule_set,
        catalog: CatalogDescriptor {
            description: "Link schedule ids to a work.",
            input_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"},"schedule_ids":{"type":"array","items":{"type":"string"}}},"required":["work_id","schedule_ids"]}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"},"schedule_ids":{"type":"array","items":{"type":"string"}}},"required":["work_id","schedule_ids"]}"#,
            ),
        },
        failure_mode: FailureMode::Forbidden,
        handler_test_vector: TestVector {
            description: "work schedule set links schedule ids to work",
            expected_outcome: "success",
            test_fn_name: "work_schedule_set_links_schedules",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.finding.resolve",
        access: Access::Write,
        admission: ADMISSION_WRITE_WORKSPACE,
        handler: registry_finding_resolve,
        catalog: CatalogDescriptor {
            description: "Mark a finding as resolved, optionally with a resolution note.",
            input_schema: Some(
                r#"{"type":"object","properties":{"finding_id":{"type":"string"},"resolution":{"type":"string"}},"required":["finding_id"]}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"finding_id":{"type":"string"},"resolved":{"type":"boolean"}},"required":["finding_id","resolved"]}"#,
            ),
        },
        failure_mode: FailureMode::Forbidden,
        handler_test_vector: TestVector {
            description: "finding resolve marks finding as resolved",
            expected_outcome: "success",
            test_fn_name: "finding_resolve_marks_resolved",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.pool.entry.manage",
        access: Access::Write,
        admission: ADMISSION_POOL_WRITE,
        handler: registry_pool_entry_manage,
        catalog: CatalogDescriptor {
            description: "Add, remove, promote, or archive a work entry in the selection pool.",
            input_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"},"action":{"type":"string","enum":["add","remove","promote","archive"]}},"required":["work_id","action"]}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"},"action":{"type":"string"},"success":{"type":"boolean"}},"required":["work_id","action","success"]}"#,
            ),
        },
        failure_mode: FailureMode::Forbidden,
        handler_test_vector: TestVector {
            description: "pool entry manage adds work to selection pool",
            expected_outcome: "success",
            test_fn_name: "pool_entry_manage_adds_to_pool",
        },
    });

    // ── V1.56 P1: nexus.registry.refresh ──
    reg.register(CapabilityRow {
        id: "nexus.registry.refresh",
        access: Access::Read,
        admission: ADMISSION_READ_CONTEXT,
        handler: registry_registry_refresh,
        catalog: CatalogDescriptor {
            description: "Return the capability registry snapshot (synthetic or CDN-backed).",
            input_schema: Some(
                r#"{"type":"object","properties":{},"required":[],"additionalProperties":false}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"cacheAgeMs":{"type":"integer","minimum":0},"capabilityCount":{"type":"integer","minimum":0},"source":{"type":"string","enum":["synthetic","cdn","synthetic_fallback"]},"snapshotVersion":{"type":"string"},"generatedAt":{"type":"string","format":"date-time"},"fetchTimeoutMs":{"type":"integer","minimum":0},"maxRetries":{"type":"integer","minimum":0},"retryCount":{"type":"integer","minimum":0},"fallbackReason":{"type":"string"}},"required":["cacheAgeMs","capabilityCount","source","snapshotVersion","generatedAt"],"additionalProperties":false}"#,
            ),
        },
        failure_mode: FailureMode::NotSupported,
        handler_test_vector: TestVector {
            description: "registry refresh returns synthetic output by default",
            expected_outcome: "success",
            test_fn_name: "registry_refresh_synthetic_smoke",
        },
    });

    // ── V1.58 P3: nexus.reference.refresh ──
    reg.register(CapabilityRow {
        id: "nexus.reference.refresh",
        access: Access::Write,
        admission: ADMISSION_WRITE_WORKSPACE,
        handler: registry_reference_refresh,
        catalog: CatalogDescriptor {
            description: "Refresh a reference source's body content and update its content hash.",
            input_schema: Some(
                r#"{"type":"object","properties":{"reference_source_id":{"type":"string","description":"Registry ID of the reference source to refresh"},"url":{"type":"string","description":"Optional override URL for ad-hoc refresh"}},"required":["reference_source_id"],"additionalProperties":false}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"reference_source_id":{"type":"string"},"refreshed":{"type":"boolean"},"content_changed":{"type":"boolean"},"new_content_hash":{"type":"string"},"refreshed_at":{"type":"string","format":"date-time"},"status":{"type":"string","enum":["fresh","stale","not_modified","policy_blocked","error"]},"bytes_fetched":{"type":"integer","minimum":0}},"required":["reference_source_id","refreshed","content_changed","status"],"additionalProperties":false}"#,
            ),
        },
        failure_mode: FailureMode::InvalidInput,
        handler_test_vector: TestVector {
            description:
                "reference refresh updates content hash and writes body.md for owned source",
            expected_outcome: "success",
            test_fn_name: "reference_refresh_happy_path",
        },
    });

    // ── V1.59 P0: DF-47 manuscript & misc capability parity batch (9 tools) ──

    reg.register(CapabilityRow {
        id: "nexus.manuscript.list",
        access: Access::Read,
        admission: ADMISSION_READ_WORKSPACE,
        handler: registry_manuscript_list,
        catalog: CatalogDescriptor {
            description: "List all manuscripts (works) for the active creator.",
            input_schema: Some(r#"{"type":"object","properties":{}}"#),
            output_schema: Some(
                r#"{"type":"object","properties":{"manuscripts":{"type":"array","items":{"type":"object"}},"count":{"type":"integer"}},"required":["manuscripts","count"]}"#,
            ),
        },
        failure_mode: FailureMode::Forbidden,
        handler_test_vector: TestVector {
            description: "manuscript list returns manuscripts for active creator",
            expected_outcome: "success",
            test_fn_name: "manuscript_list_returns_manuscripts",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.manuscript.read_range",
        access: Access::Read,
        admission: ADMISSION_READ_WORKSPACE,
        handler: registry_manuscript_read_range,
        catalog: CatalogDescriptor {
            description: "Read a bounded line range from a manuscript chapter body.",
            input_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"},"chapter":{"type":"integer","minimum":1},"volume":{"type":"integer","minimum":1},"start_line":{"type":"integer","minimum":1},"end_line":{"type":"integer","minimum":1}},"required":["work_id","chapter"]}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"},"chapter":{"type":"integer"},"volume":{"type":"integer"},"content":{"type":"string"},"range":{"type":"object","properties":{"start_line":{"type":"integer"},"end_line":{"type":"integer"}}},"total_lines":{"type":"integer"},"truncated":{"type":"boolean"}},"required":["work_id","chapter","volume","content","range","total_lines","truncated"]}"#,
            ),
        },
        failure_mode: FailureMode::InvalidInput,
        handler_test_vector: TestVector {
            description: "manuscript read_range returns bounded content for valid chapter",
            expected_outcome: "success",
            test_fn_name: "manuscript_read_range_returns_bounded_content",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.manuscript.write",
        access: Access::Write,
        admission: ADMISSION_WRITE_WORKSPACE,
        handler: registry_manuscript_write,
        catalog: CatalogDescriptor {
            description: "Write manuscript body content for a chapter within the size quota.",
            input_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"},"chapter":{"type":"integer","minimum":1},"volume":{"type":"integer","minimum":1},"content":{"type":"string"}},"required":["work_id","chapter","content"]}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"written":{"type":"boolean"},"work_id":{"type":"string"},"chapter":{"type":"integer"},"volume":{"type":"integer"},"word_count":{"type":"integer"},"bytes_written":{"type":"integer"}},"required":["written","work_id","chapter","volume","word_count","bytes_written"]}"#,
            ),
        },
        failure_mode: FailureMode::InvalidInput,
        handler_test_vector: TestVector {
            description: "manuscript write writes body content for valid chapter within size quota",
            expected_outcome: "success",
            test_fn_name: "manuscript_write_writes_content",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.manuscript.phase.get",
        access: Access::Read,
        admission: ADMISSION_READ_WORKSPACE,
        handler: registry_manuscript_phase_get,
        catalog: CatalogDescriptor {
            description: "Return the current manuscript phase and stage status for a work.",
            input_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"}},"required":["work_id"]}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"},"phase":{"type":"string"},"stage_status":{"type":"string"}},"required":["work_id","phase","stage_status"]}"#,
            ),
        },
        failure_mode: FailureMode::Forbidden,
        handler_test_vector: TestVector {
            description: "manuscript phase get returns current phase for owned work",
            expected_outcome: "success",
            test_fn_name: "manuscript_phase_get_returns_current_phase",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.manuscript.phase.set",
        access: Access::Write,
        admission: ADMISSION_WRITE_WORKSPACE,
        handler: registry_manuscript_phase_set,
        catalog: CatalogDescriptor {
            description: "Move a work forward to the next manuscript phase.",
            input_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"},"phase":{"type":"string","enum":["brainstorm","draft","review","finalize"]},"force":{"type":"boolean"}},"required":["work_id","phase"]}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"work_id":{"type":"string"},"previous_phase":{"type":"string"},"current_phase":{"type":"string"},"stage_status":{"type":"string"},"transitioned":{"type":"boolean"}},"required":["work_id","previous_phase","current_phase","stage_status","transitioned"]}"#,
            ),
        },
        failure_mode: FailureMode::InvalidInput,
        handler_test_vector: TestVector {
            description: "manuscript phase set moves work forward to next phase",
            expected_outcome: "success",
            test_fn_name: "manuscript_phase_set_advances_phase",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.workspace.paths",
        access: Access::Read,
        admission: ADMISSION_READ_CONTEXT,
        handler: registry_workspace_paths,
        catalog: CatalogDescriptor {
            description: "Return the workspace root and the allowed roots (Works, Worlds, References, .nexus42).",
            input_schema: Some(r#"{"type":"object","properties":{}}"#),
            output_schema: Some(
                r#"{"type":"object","properties":{"workspace_root":{"type":"string"},"allowed_roots":{"type":"array","items":{"type":"string"}},"preset_id":{"type":"string"}},"required":["workspace_root","allowed_roots","preset_id"]}"#,
            ),
        },
        failure_mode: FailureMode::InvalidInput,
        handler_test_vector: TestVector {
            description: "workspace paths returns allowed roots from active workspace",
            expected_outcome: "success",
            test_fn_name: "workspace_paths_returns_allowed_roots",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.research.query",
        access: Access::Read,
        admission: ADMISSION_READ_WORKSPACE,
        handler: registry_research_query,
        catalog: CatalogDescriptor {
            description: "Query the local reference-source index by id, tag, or a bounded limit.",
            input_schema: Some(
                r#"{"type":"object","properties":{"reference_source_id":{"type":"string"},"tags":{"type":"string"},"limit":{"type":"integer","minimum":1,"maximum":1000}}}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"results":{"type":"array","items":{"type":"object"}},"count":{"type":"integer"}},"required":["results","count"]}"#,
            ),
        },
        failure_mode: FailureMode::InvalidInput,
        handler_test_vector: TestVector {
            description: "research query returns reference sources from local index",
            expected_outcome: "success",
            test_fn_name: "research_query_returns_reference_sources",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.runtime.health",
        access: Access::Read,
        admission: ADMISSION_READ_CONTEXT,
        handler: registry_runtime_health,
        catalog: CatalogDescriptor {
            description: "Return agent-visible runtime health: mode, registry reachability, sync context, cloud flag.",
            input_schema: Some(r#"{"type":"object","properties":{}}"#),
            output_schema: Some(
                r#"{"type":"object","properties":{"runtime_mode":{"type":"string"},"registry_reachable":{"type":"boolean"},"registry_size":{"type":"integer"},"sync_state":{"type":"string"},"cloud_enabled":{"type":"boolean"},"pool_healthy":{"type":"boolean"}},"required":["runtime_mode","registry_reachable","registry_size","sync_state","cloud_enabled","pool_healthy"]}"#,
            ),
        },
        failure_mode: FailureMode::Forbidden,
        handler_test_vector: TestVector {
            description: "runtime health returns agent-visible health and registry reachability",
            expected_outcome: "success",
            test_fn_name: "runtime_health_returns_agent_visible_status",
        },
    });

    reg.register(CapabilityRow {
        id: "nexus.trace.correlation",
        access: Access::Read,
        admission: ADMISSION_READ_CONTEXT,
        handler: registry_trace_correlation,
        catalog: CatalogDescriptor {
            description: "Propagate a correlation id (and optional session id) across tool calls.",
            input_schema: Some(
                r#"{"type":"object","properties":{"correlation_id":{"type":"string"},"session_id":{"type":"string"}}}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"correlation_id":{"type":"string"},"session_id":{"type":"string"},"parent_request_id":{"type":"string"},"trace_timestamp":{"type":"string"},"propagated":{"type":"boolean"}},"required":["correlation_id","trace_timestamp","propagated"]}"#,
            ),
        },
        failure_mode: FailureMode::Forbidden,
        handler_test_vector: TestVector {
            description: "trace correlation propagates correlation id across tool calls",
            expected_outcome: "success",
            test_fn_name: "trace_correlation_propagates_correlation_id",
        },
    });

    // ── fs/* baseline (V1.33) ──
    reg.register(CapabilityRow {
        id: "fs/read_text_file",
        access: Access::Read,
        admission: ADMISSION_FS_READ,
        handler: registry_read_file,
        catalog: CatalogDescriptor {
            description: "Read a text file within the workspace root and return its content.",
            input_schema: Some(
                r#"{"type":"object","properties":{"path":{"type":"string"}},"required":["path"]}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"content":{"type":"string"}},"required":["content"]}"#,
            ),
        },
        failure_mode: FailureMode::InvalidInput,
        handler_test_vector: TestVector {
            description: "read file returns content for valid path",
            expected_outcome: "success",
            test_fn_name: "execute_read_file_succeeds",
        },
    });

    reg.register(CapabilityRow {
        id: "fs/write_text_file",
        access: Access::Write,
        admission: ADMISSION_FS_WRITE,
        handler: registry_write_file,
        catalog: CatalogDescriptor {
            description: "Write text content to a file within the workspace root.",
            input_schema: Some(
                r#"{"type":"object","properties":{"path":{"type":"string"},"content":{"type":"string"}},"required":["path","content"]}"#,
            ),
            output_schema: Some(
                r#"{"type":"object","properties":{"written":{"type":"boolean"}},"required":["written"]}"#,
            ),
        },
        failure_mode: FailureMode::InvalidInput,
        handler_test_vector: TestVector {
            description: "write file writes content and returns success",
            expected_outcome: "success",
            test_fn_name: "execute_write_file_succeeds",
        },
    });

    reg
}

// ─── Tests ─────────────────────────────────────────────────────────────────
