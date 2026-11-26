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
//! 2. a remote peer row ([`nexus_connect::peer_tools`]),
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

#[cfg(any(test, feature = "test-hooks"))]
use std::sync::atomic::{AtomicUsize, Ordering};

use super::work_artifact::{
    classify_work_path, FsWorkArtifactPort, RecordedMutation, RecordingWorkArtifactPort,
    WorkArtifactPort, WorkArtifactWrite, WorkPath,
};
use crate::content::resolve_guarded_path_async;
use crate::error::CoreError as NexusApiError;
use crate::error::{CoreError, CoreResult};
use crate::service::CoreService;
use crate::tool_catalog::{
    host_tool_catalog, lookup_host_tool, Access, AdmissionGate, HostToolId, HostToolMetadata,
};
use crate::{ArchivePoolRequest, PromotePoolRequest, WorkDetails, WorkPatchRequest};
use nexus_connect::peer_tools;
use nexus_contracts::{FindingsApi, ToolsApi};
use nexus_home_layout::active_context::{read_active_creator_id, read_active_workspace_slug};
use nexus_local_db::works;
use nexus_narrative::NarrativeGateway;
use nexus_spoke_adapter::{
    parse_tool_capability_id, KnowledgeEntryPort, SpokeResult, StubKnowledgeEntryMutation,
    StubKnowledgeEntryPort,
};
use serde_json::Value;
use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::LazyLock;

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
    pub(crate) user_capabilities: Option<nexus_capability::CapabilityRegistryHolder>,
    /// The Simulate plane's Work artifact port, when this context is one.
    ///
    /// `None` is production: every Work byte write commits through
    /// [`FsWorkArtifactPort`]. `Some` records the intended `Works/` writes
    /// instead, so a dry run leaves the `Works/` tree byte-identical; it is
    /// also the simulated-context marker that makes `execute_write_file`
    /// refuse every non-`Works/` fs write rather than touching disk.
    pub(crate) recording_work_artifact: Option<Arc<RecordingWorkArtifactPort>>,
    /// The Simulate plane's knowledge port, when this context is one.
    ///
    /// `None` is production: knowledge writes commit through `SqliteKbStore`.
    /// `Some` records the intended writes instead, so a dry run leaves
    /// `kb_key_blocks` byte-identical.
    pub(crate) recording_knowledge_entry: Option<Arc<StubKnowledgeEntryPort>>,
    /// The prompt executor this process composed onto its execution owner.
    ///
    /// `None` is every context whose transport attached no model — a core-only
    /// caller, a Simulate plane, a daemon with no provider selected. A spine
    /// tool that needs a model passes exactly this, so the tool cannot reach a
    /// second Host client the engine does not know about, and cannot invent a
    /// heuristic substitute when the transport supplied none.
    pub(crate) prompt_executor: Option<Arc<dyn nexus_capability::PromptExecutor>>,
}

impl ToolContext {
    /// Compose a tool context from its owned inputs.
    ///
    /// The fields stay crate-private so a caller cannot mutate the authority
    /// under a running dispatch; this constructor is the public composition
    /// point a transport (or a test) uses.
    ///
    /// Production: Work artifact writes commit through the filesystem port.
    #[must_use]
    pub const fn new(
        pool: sqlx::SqlitePool,
        nexus_home: std::path::PathBuf,
        workspace_path: Option<String>,
        runtime_facts: ToolRuntimeFacts,
        core: Option<Arc<CoreService>>,
        user_capabilities: Option<nexus_capability::CapabilityRegistryHolder>,
    ) -> Self {
        Self {
            pool,
            nexus_home,
            workspace_path,
            runtime_facts,
            core,
            user_capabilities,
            recording_work_artifact: None,
            recording_knowledge_entry: None,
            prompt_executor: None,
        }
    }

    /// The same context with the transport's prompt executor attached.
    ///
    /// The executor is the one the engine drives, so a tool's model round and a
    /// preset's model round use one Host client and one provider selection.
    #[must_use]
    pub fn with_prompt_executor(
        mut self,
        prompt_executor: Option<Arc<dyn nexus_capability::PromptExecutor>>,
    ) -> Self {
        self.prompt_executor = prompt_executor;
        self
    }

    /// The same context, with Work artifact writes recorded instead of landed.
    ///
    /// A dry run over a workspace whose `Works/` tree already holds real
    /// chapter bytes must not rewrite them; selecting the recording port here is
    /// what makes that true for every Work write this context can reach.
    ///
    /// The knowledge stub is selected by the same call for the same reason: a
    /// preview that reaches `nexus.kb_snapshot.write` must not insert rows. The
    /// same marker makes `fs/write_text_file` record only `Works/` artifact
    /// writes and refuse every other fs write — there is no recorded seam for
    /// plain paths, so a simulated context must not claim one.
    #[must_use]
    pub fn new_simulated(
        pool: sqlx::SqlitePool,
        nexus_home: std::path::PathBuf,
        workspace_path: Option<String>,
        runtime_facts: ToolRuntimeFacts,
        core: Option<Arc<CoreService>>,
        user_capabilities: Option<nexus_capability::CapabilityRegistryHolder>,
    ) -> Self {
        Self {
            recording_work_artifact: Some(Arc::new(RecordingWorkArtifactPort::new())),
            recording_knowledge_entry: Some(Arc::new(StubKnowledgeEntryPort::new())),
            ..Self::new(
                pool,
                nexus_home,
                workspace_path,
                runtime_facts,
                core,
                user_capabilities,
            )
        }
    }

    /// The Work writes this context recorded, or `None` for a production one.
    ///
    /// `None` is not an empty log: a production context commits real bytes and
    /// records nothing, so reporting `Some(vec![])` would claim a plane that
    /// does not exist.
    #[must_use]
    pub fn recorded_work_artifact_mutations(&self) -> Option<Vec<RecordedMutation>> {
        self.recording_work_artifact
            .as_ref()
            .map(|port| port.mutations())
    }

    /// The knowledge port intents this context recorded, or `None` for a
    /// production one.
    ///
    /// `None` is not an empty log: a production context commits real rows
    /// through `SqliteKbStore` and records nothing, so reporting `Some(vec![])`
    /// would claim a plane that does not exist.
    #[must_use]
    pub fn recorded_knowledge_entry_mutations(&self) -> Option<Vec<StubKnowledgeEntryMutation>> {
        self.recording_knowledge_entry
            .as_ref()
            .map(|port| port.mutations())
    }

    /// Commit one Work artifact byte write through the selected port.
    ///
    /// The recording port performs no filesystem, SQLite or lock operation, so
    /// a simulated dispatch claims Work-byte isolation only — the surrounding
    /// capability keeps its own path guards and post-commit database behaviour.
    async fn commit_work_artifact(
        &self,
        write: WorkArtifactWrite<'_>,
    ) -> Result<(), NexusApiError> {
        match self.recording_work_artifact.as_deref() {
            Some(port) => port.commit(write).await,
            None => {
                FsWorkArtifactPort::new(self.pool.clone(), self.nexus_home.clone())
                    .commit(write)
                    .await
            }
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
        holder: Option<nexus_capability::CapabilityRegistryHolder>,
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
    pub fn user_capabilities(&self) -> Option<Arc<nexus_capability::CapabilityRegistry>> {
        self.user_capabilities
            .as_ref()
            .and_then(nexus_capability::CapabilityRegistryHolder::get)
    }

    /// The prompt executor this context's transport attached, if any.
    #[must_use]
    pub fn prompt_executor(&self) -> Option<&dyn nexus_capability::PromptExecutor> {
        self.prompt_executor.as_deref()
    }
}

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
    request: &ToolsApi,
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
        CoreError::Coded { code, .. }
        | CoreError::PeerDenied { code, .. }
        | CoreError::MemoryProviderRefused { code, .. } => code,
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
    req: &ToolsApi,
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

        let workspace_path_str = context.workspace_path().unwrap_or_default();
        if !workspace_path_str.is_empty() {
            if let Some(policy) = load_permission_policy(&workspace_path_str) {
                check_nexus_tool_permission(&req.tool_name, &policy)?;
            }
        }

        return Ok((creator_id, workspace_slug));
    }

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

    if let Some(policy) = load_permission_policy(&workspace_path_str) {
        check_fs_tool_permission(&req.tool_name, &policy)?;
    }

    // execute_read_file / execute_write_file call resolve_guarded_path_async
    // before any FS access, making them the single resolution site.

    Ok((creator_id.unwrap_or_default(), String::new()))
}

/// Check permission for a `nexus.*` tool against policy (Gate 4).
fn check_nexus_tool_permission(
    tool_name: &str,
    policy: &WorkspacePermissionPolicy,
) -> Result<(), NexusApiError> {
    let is_write = lookup_host_tool(tool_name).is_some_and(|row| row.access == Access::Write);

    let allowed = if is_write {
        is_nexus_write_granted(tool_name, policy)
    } else {
        is_nexus_read_granted(tool_name, policy)
    };

    if allowed {
        return Ok(());
    }

    let reason = if is_write {
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

//
// V1.53 P0 Sub-phase 3: Old `dispatch_tool()` match table removed.
// All dispatch now routes through `CapabilityRegistry` (see
// `HostToolExecutor::registry_dispatch()` and `capability_registry.rs`).
// The handler functions below remain as they are referenced by the
// `pub(crate)` registry wrapper functions.

/// `nexus.context.whoami` — return active `creator_id` and workspace slug.
fn execute_context_whoami(
    _req: &ToolsApi,
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
    _req: &ToolsApi,
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
    req: &ToolsApi,
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

    let record = works::get_work(context.pool(), creator_id, work_id)
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
    req: &ToolsApi,
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

    let params = req
        .parameters
        .as_object()
        .ok_or_else(|| NexusApiError::InvalidInput {
            field: "parameters".into(),
            reason: "must be a JSON object".into(),
        })?;

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

    // V1.34 minimal: accepted but stored as-is in inspiration_log as a metadata entry.
    if let Some(metadata) = params.get("stage_metadata") {
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

    let details = core.get_work(&principal, work_id.to_string()).await?;
    let dto = details;
    Ok(serde_json::to_value(dto).unwrap_or_else(|_| serde_json::json!({})))
}

/// `nexus.orchestration.schedule_status` — return schedules linked to `work_id`.
async fn execute_schedule_status(
    req: &ToolsApi,
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

/// `nexus.strategy.generate` — one committed candidate for `parameters.strategy_id`.
///
/// The tool surface of the generate kernel and nothing more: it decodes the two address fields a
/// tool call has to name, parses `parameters.request` into the SAME [`GenerateRequest`] the CLI
/// parses arguments into and the HTTP body deserializes, and passes the context's executor as the
/// corrector. Lint exhaustion comes back as the D20 payload (`code: lint_exhausted`) rather than a
/// bare refusal, because the model reads `correction` and the spine's error surface carries no
/// `details`: a refusal here would hand the model less than the payload it needs to act on.
///
/// Authoring-only: the canonical `nexus.strategy.generate` row stays reserved without this
/// build's `strategy-authoring` feature, and [`build_registry`] leaves it unbound.
#[cfg(feature = "strategy-authoring")]
async fn execute_strategy_generate(
    req: &ToolsApi,
    context: &ToolContext,
    _creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    let strategy_id =
        req.parameters["strategy_id"]
            .as_str()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "parameters.strategy_id".into(),
                reason: "must be a string".into(),
            })?;
    let base_revision =
        req.parameters["base_revision"]
            .as_u64()
            .ok_or_else(|| NexusApiError::InvalidInput {
                field: "parameters.base_revision".into(),
                reason: "must be an unsigned integer".into(),
            })?;
    let request: crate::GenerateRequest = serde_json::from_value(req.parameters["request"].clone())
        .map_err(|error| NexusApiError::InvalidInput {
            field: "parameters.request".into(),
            reason: error.to_string(),
        })?;

    let core = context.core()?;
    let principal = core.active_principal().await?;
    let outcome = core
        .generate_strategy_candidate(
            &principal,
            strategy_id,
            base_revision,
            request,
            context.prompt_executor(),
        )
        .await;
    match outcome {
        Ok(outcome) => Ok(outcome.wire()),
        Err(CoreError::InputValidation { details })
            if details.get("code").and_then(Value::as_str) == Some("lint_exhausted") =>
        {
            Ok(details)
        }
        Err(error) => Err(error),
    }
}

/// `nexus.context.assemble` — local assemble-moment or `POLICY_BLOCKED` (spec §4.1).
async fn execute_context_assemble(
    req: &ToolsApi,
    context: &ToolContext,
    creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    let requires_platform = req.parameters["requires_platform"]
        .as_bool()
        .unwrap_or(false);

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

    Ok(serde_json::json!({
        "mode": "local",
        "creator_id": creator_id,
        "assembled_at": chrono::Utc::now().to_rfc3339()
    }))
}

/// A case-variant `Works` spelling: neither half routes it.
fn case_variant_works_refusal(path_str: &str) -> NexusApiError {
    NexusApiError::Forbidden {
        resource: format!("file: path '{path_str}' must spell the Work root 'Works' byte-exactly"),
    }
}

/// `Works/<work_ref>` itself: a directory, never a readable or writable file.
fn work_root_refusal(path_str: &str, work_ref: &str) -> NexusApiError {
    NexusApiError::Forbidden {
        resource: format!(
            "file: path '{path_str}' is the Work root directory 'Works/{work_ref}', not an artifact file"
        ),
    }
}

/// A `..` inside the Work subtree: lexically inside, physically not.
fn escaped_work_refusal(path_str: &str, work_ref: &str) -> NexusApiError {
    NexusApiError::Forbidden {
        resource: format!("file: path '{path_str}' escapes the Works/{work_ref}/ boundary"),
    }
}

/// Classify the raw path and refuse the three never-routed Work spellings.
///
/// Both halves call this before the path guard, so a spelling is judged the
/// same way whether the target exists, is missing, or resolves through a
/// symlink: the guard's canonicalization never gets to answer first.
fn classify_and_refuse_work_path(path_str: &str) -> Result<WorkPath<'_>, NexusApiError> {
    let work_path = classify_work_path(path_str);
    match &work_path {
        WorkPath::WorkRoot { work_ref } => Err(work_root_refusal(path_str, work_ref)),
        WorkPath::EscapedWork { work_ref } => Err(escaped_work_refusal(path_str, work_ref)),
        WorkPath::CaseVariantWorks => Err(case_variant_works_refusal(path_str)),
        WorkPath::Artifact { .. } | WorkPath::NonWork => Ok(work_path),
    }
}

/// Split a workspace-relative Work artifact path into the port's
/// `(work_ref, rel_child)` pair.
///
/// The three never-routed Work spellings keep their shared refusals, and a
/// non-Work path is refused here: a manuscript body is a Work artifact by
/// contract, so it may not fall back to an unguarded write.
fn work_artifact_parts(path_str: &str) -> Result<(&str, &Path), NexusApiError> {
    match classify_work_path(path_str) {
        WorkPath::Artifact {
            work_ref,
            rel_child,
        } => Ok((work_ref, rel_child)),
        WorkPath::WorkRoot { work_ref } => Err(work_root_refusal(path_str, work_ref)),
        WorkPath::EscapedWork { work_ref } => Err(escaped_work_refusal(path_str, work_ref)),
        WorkPath::CaseVariantWorks => Err(case_variant_works_refusal(path_str)),
        WorkPath::NonWork => Err(NexusApiError::InvalidInput {
            field: "body_path".into(),
            reason: "must name a Work artifact below Works/<work_ref>/".into(),
        }),
    }
}

/// Map the `fs/read_text_file` path-guard error to its wire variant. The
/// classification contract this boundary promises: `not_found` (real
/// absence) → `NotFound`; `permission_denied` → `Forbidden`; kinds meaning a
/// malformed request (`not_a_directory`) → `InvalidInput` (a 400-class
/// request error); `root_unresolvable` (the workspace root itself failed to
/// canonicalize, emitted before the ancestor probe) → a root-level
/// `Internal`, never a file-level absence/escape verdict;
/// everything else → the quiet `Internal` fallback. The exact legacy
/// unsuffixed field keeps its historical `NotFound` mapping, and
/// `chapter_path_forbidden` stays the non-revealing outside-workspace shape.
/// Pulled out of `execute_read_file` so the contract is assertable without
/// filesystem tricks on any runner.
fn read_guard_error(e: NexusApiError, path_str: &str) -> NexusApiError {
    match e {
        NexusApiError::InvalidInput { ref field, .. } if field == "chapter_path_unresolvable" => {
            NexusApiError::NotFound {
                resource: format!("file: path '{path_str}' does not exist"),
            }
        }
        NexusApiError::InvalidInput { ref field, .. }
            if field.starts_with("chapter_path_unresolvable:") =>
        {
            match field.strip_prefix("chapter_path_unresolvable:") {
                Some("not_found") => NexusApiError::NotFound {
                    resource: format!("file: path '{path_str}' does not exist"),
                },
                Some("permission_denied") => NexusApiError::Forbidden {
                    resource: format!("file: path '{path_str}' cannot be read (permission denied)"),
                },
                Some("not_a_directory") => NexusApiError::InvalidInput {
                    field: "path".into(),
                    reason: format!(
                        "path '{path_str}' cannot resolve: a component is not a directory"
                    ),
                },
                // The workspace root, not the requested file, is unavailable:
                // no file-level absence/escape verdict may be derived from it.
                Some(crate::content::ROOT_UNRESOLVABLE_CODE) => NexusApiError::Internal {
                    category: "WORKSPACE_ROOT_UNRESOLVABLE: workspace root is unavailable".into(),
                },
                _ => NexusApiError::Internal {
                    category: format!(
                        "FILE_RESOLVE_FAILED: {}",
                        format_args!("cannot resolve file path '{path_str}'")
                    ),
                },
            }
        }
        NexusApiError::InvalidInput { .. } => NexusApiError::Forbidden {
            resource: format!("file: path '{path_str}' is outside the workspace root"),
        },
        other => other,
    }
}

/// Execute `fs/read_text_file` tool.
///
/// V1.86 T5 (R-V156P0-M004): all blocking `std::fs` operations run on the
/// tokio blocking pool so the async runtime is not stalled by local disk I/O.
#[allow(clippy::similar_names)] // the paired names are the domain vocabulary here
async fn execute_read_file(
    req: &ToolsApi,
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

    // ADR #485 §4.4: the read half refuses every classification the write half
    // refuses, and judges the raw spelling before the guard — the same order the
    // write half uses — so the same path gets the same answer whether or not it
    // exists and wherever a symlink sends it.
    classify_and_refuse_work_path(&path_str)?;

    let workspace_path = Path::new(&workspace_path_str).to_path_buf();
    let resolved = resolve_guarded_path_async(workspace_path, path_str.clone(), true)
        .await
        .map_err(|e| read_guard_error(e, &path_str))?;

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
    req: &ToolsApi,
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

    // ADR #485 §4.4: judge the raw spelling before the guard, exactly as the
    // read half does, so the same path gets the same refusal on both sides.
    let work_path = classify_and_refuse_work_path(&path_str)?;

    let workspace_path = Path::new(&workspace_path_str).to_path_buf();
    let resolved = resolve_guarded_path_async(workspace_path, path_str.clone(), false)
        .await
        .map_err(|e| match e {
            NexusApiError::InvalidInput { .. } => NexusApiError::Forbidden {
                resource: format!("file: path '{path_str}' is outside the workspace root"),
            },
            other => other,
        })?;

    // ADR #485 §4.4: `Works/<work_ref>/…` is a Work artifact byte write — one
    // port, one lock, one durability path. Every other workspace path keeps
    // the guarded write below — except under a simulated context, which
    // records only `Works/` artifact writes and refuses the rest.
    if let WorkPath::Artifact {
        work_ref,
        rel_child,
    } = work_path
    {
        context
            .commit_work_artifact(WorkArtifactWrite {
                workspace_root: Path::new(&workspace_path_str),
                work_ref,
                rel_child,
                content,
            })
            .await?;
    } else if context.recording_work_artifact.is_some() {
        // `recording_work_artifact` marks a simulated context: it never touches
        // disk, and outside `Works/` there is no recorded seam to claim.
        //
        // `Forbidden` is reused deliberately: the cause is a context
        // restriction, not an authorization decision. The tool error vocabulary
        // is the closed generated `CoreErrorCode`
        // (`schemas/core/core-error.schema.json`), which has no simulator code:
        // `not_supported` is the unknown-tool answer
        // (`CapabilityRegistry::dispatch`), `InvalidInput` would call a
        // well-formed request malformed, and `Coded` exists for finer codes a
        // retained surface already promises — minting one here is a new wire
        // value without a schema entry. Within this tool `Forbidden` already
        // means "this path/context is refused" (the out-of-root mapping above),
        // and its `resource` is free text, so the caller reads the simulator
        // cause below, not a bare denial.
        //
        // Audit cost, accepted: `execute()` audits every refusal as
        // `denied:<err_code>` into `acp_tool_audit_log`, and `err_code` maps
        // `Forbidden` to "forbidden", so the row reads `denied:forbidden` — the
        // simulator cause survives only in this returned `resource`.
        return Err(NexusApiError::Forbidden {
            resource: format!(
                "fs/write_text_file: simulated context — non-Works writes are not performed (production writes normally); requested '{path_str}'"
            ),
        });
    } else {
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
    }

    Ok(serde_json::json!({
        "written": true
    }))
}

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
        .map_or_default(table_keys);
    let deny = policy
        .get("deny")
        .and_then(|v| v.as_table())
        .map_or_default(table_keys);

    Some(WorkspacePermissionPolicy {
        default,
        grant,
        deny,
    })
}

/// Audit tool execution to `SQLite` (Gate 5).
pub(crate) async fn audit_tool_execution(
    req: &ToolsApi,
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

    let param_summary: Vec<String> = req
        .parameters
        .as_object()
        .map_or_default(|obj| obj.keys().cloned().collect::<Vec<_>>());

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
    req: &ToolsApi,
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

    let gw = nexus_local_db::narrative_gateway::SqliteNarrativeGateway::new(context.pool().clone());
    let world_state = gw.get_world_state(world_id).await.map_err(
        |e: nexus_narrative::NarrativeError| match e {
            nexus_narrative::NarrativeError::NotFound { resource } => {
                NexusApiError::NotFound { resource }
            }
            other => NexusApiError::Internal {
                category: format!("NARRATIVE_ERROR: {other}"),
            },
        },
    )?;

    Ok(serde_json::to_value(world_state).unwrap_or_else(|_| serde_json::json!({})))
}

/// `nexus.timeline.recent.get` — fetch recent timeline events for continuity.
async fn execute_timeline_recent_get(
    req: &ToolsApi,
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

    let limit: usize = req.parameters["limit"]
        .as_u64()
        .and_then(|v| usize::try_from(v).ok())
        .unwrap_or(100)
        .min(500);

    let gw = nexus_local_db::narrative_gateway::SqliteNarrativeGateway::new(context.pool().clone());
    let mut events = gw.get_timeline(world_id, None, Some(limit)).await.map_err(
        |e: nexus_narrative::NarrativeError| NexusApiError::Internal {
            category: format!("NARRATIVE_ERROR: {e}"),
        },
    )?;

    events.reverse();
    Ok(serde_json::to_value(&events).unwrap_or_else(|_| serde_json::json!([])))
}

/// `nexus.kb_snapshot.read` — focused KB snapshot read for a world.
async fn execute_kb_snapshot_read(
    req: &ToolsApi,
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

    let selection = crate::actor_knowledge::ActorKnowledgeViewService::new(pool.clone())
        .actor_view_scope(
            creator_id,
            &crate::actors::AdmittedActor::Creator {
                creator_id: creator_id.to_string(),
            },
            world_id,
            None,
        )
        .await?;

    let kb_store = nexus_local_db::kb_store::SqliteKbStore::new(pool.clone());
    let blocks = kb_store
        .list_by_world_admitted(world_id, false, &selection)
        .await
        .map_err(
            |e: nexus_knowledge::world_kb::store::KbStoreError| NexusApiError::Internal {
                category: format!("KB_STORE_ERROR: {e}"),
            },
        )?;

    Ok(serde_json::to_value(&blocks).unwrap_or_else(|_| serde_json::json!([])))
}

/// `nexus.manuscript.chapter.get` — read a single manuscript chapter record.
async fn execute_manuscript_chapter_get(
    req: &ToolsApi,
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
    _req: &ToolsApi,
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
    req: &'a ToolsApi,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    let result = execute_context_whoami(req, context, creator_id);
    Box::pin(async move { Ok(result) })
}

/// Registry wrapper: `nexus.workspace.info` — sync → async wrapper.
pub(crate) fn registry_workspace_info<'a>(
    req: &'a ToolsApi,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    let result = execute_workspace_info(req, context, creator_id);
    Box::pin(async move { Ok(result) })
}

/// Registry wrapper: `nexus.work.get` — async passthrough.
pub(crate) fn registry_work_get<'a>(
    req: &'a ToolsApi,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_work_get(req, context, creator_id))
}

/// Registry wrapper: `nexus.work.patch` — async passthrough.
pub(crate) fn registry_work_patch<'a>(
    req: &'a ToolsApi,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_work_patch(req, context, creator_id))
}

/// Registry wrapper: `nexus.orchestration.schedule_status` — async passthrough.
pub(crate) fn registry_schedule_status<'a>(
    req: &'a ToolsApi,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_schedule_status(req, context, creator_id))
}

/// Registry wrapper: `nexus.strategy.generate` — async passthrough.
#[cfg(feature = "strategy-authoring")]
pub(crate) fn registry_strategy_generate<'a>(
    req: &'a ToolsApi,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_strategy_generate(req, context, creator_id))
}

/// Registry wrapper: `nexus.context.assemble` — async passthrough.
pub(crate) fn registry_context_assemble<'a>(
    req: &'a ToolsApi,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_context_assemble(req, context, creator_id))
}

/// Registry wrapper: `fs/read_text_file` — async passthrough (ignores `creator_id`).
pub(crate) fn registry_read_file<'a>(
    req: &'a ToolsApi,
    context: &'a ToolContext,
    _creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_read_file(req, context))
}

/// Registry wrapper: `fs/write_text_file` — async passthrough (ignores `creator_id`).
pub(crate) fn registry_write_file<'a>(
    req: &'a ToolsApi,
    context: &'a ToolContext,
    _creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_write_file(req, context))
}

/// Registry wrapper: `nexus.world.snapshot.get` — async passthrough.
pub(crate) fn registry_world_snapshot_get<'a>(
    req: &'a ToolsApi,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_world_snapshot_get(req, context, creator_id))
}

/// Registry wrapper: `nexus.timeline.recent.get` — async passthrough.
pub(crate) fn registry_timeline_recent_get<'a>(
    req: &'a ToolsApi,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_timeline_recent_get(req, context, creator_id))
}

/// Registry wrapper: `nexus.kb_snapshot.read` — async passthrough.
pub(crate) fn registry_kb_snapshot_read<'a>(
    req: &'a ToolsApi,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_kb_snapshot_read(req, context, creator_id))
}

/// Registry wrapper: `nexus.manuscript.chapter.get` — async passthrough.
pub(crate) fn registry_manuscript_chapter_get<'a>(
    req: &'a ToolsApi,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_manuscript_chapter_get(req, context, creator_id))
}

/// Registry wrapper: `nexus.observability.daemon.health` — sync → async wrapper.
pub(crate) fn registry_daemon_health<'a>(
    req: &'a ToolsApi,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    let result = execute_daemon_health(req, context, creator_id);
    Box::pin(async move { Ok(result) })
}

/// `nexus.registry.refresh` — return the registry snapshot (synthetic or CDN).
async fn execute_registry_refresh(
    _req: &ToolsApi,
    _context: &ToolContext,
    _creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    use nexus_capability::Capability;
    let cap = nexus_capability::builtins::RegistryRefresh::new();
    let input = serde_json::json!({"force": false});
    cap.run(input).await.map_err(|e| NexusApiError::Internal {
        category: format!(
            "REGISTRY_REFRESH_FAILED: {}",
            format_args!("registry.refresh failed: {e}")
        ),
    })
}

pub(crate) fn registry_registry_refresh<'a>(
    req: &'a ToolsApi,
    context: &'a ToolContext,

    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_registry_refresh(req, context, creator_id))
}

/// The refusal a kb store gate reports — the one shape both the production
/// INSERT and the simulated pre-check answer with.
fn kb_store_refusal(e: &nexus_knowledge::world_kb::store::KbStoreError) -> NexusApiError {
    NexusApiError::Internal {
        category: format!("KB_STORE_ERROR: {e}"),
    }
}

/// `nexus.kb_snapshot.write` — upsert key blocks for a world.
///
/// A production context commits every block through `SqliteKbStore`. A
/// simulated one hands each block to the context's `KnowledgeEntryPort` stub,
/// which records the `Put` intent and writes nothing.
///
/// Its response carries `written: 0` and `suppressed_writes` — one object per
/// recorded `Put`, in input order. Each block must first pass the store's own
/// record gates (`SqliteKbStore::validate_key_block`) and is refused with the
/// store's refusal otherwise: `knowledge_record_to_spoke` asserts those gates,
/// so converting first panics on caller input that production refuses. It must
/// then pass the store's `key_block_id` primary-key predicate
/// (`SqliteKbStore::check_key_block_id_free`) — the same check the INSERT
/// collides on — so an id that already exists is refused here instead of at
/// apply time. Zero blocks yield the explicit empty list, `Get` never appears
/// (a read mutates nothing), and a production context carries neither the list
/// nor an empty-state claim.
///
/// shortcut: the preview predicts no other write failure. A block whose
/// `(world_id, block_type, canonical_name)` is already held by a live row still
/// previews as writable and collides on `idx_kb_key_blocks_active_unique` at
/// apply, as does a `key_block_id` repeated by a sibling block of the same
/// request. upgrade: only with the DDL-derived design pass `I-000515` requires.
async fn execute_kb_snapshot_write(
    req: &ToolsApi,
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

    let mut records = Vec::with_capacity(blocks.len());
    for block_val in blocks {
        let kb: nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord =
            serde_json::from_value(block_val.clone()).map_err(|e| NexusApiError::InvalidInput {
                field: "parameters.blocks[]".into(),
                reason: format!("invalid key block: {e}"),
            })?;
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
        records.push(kb);
    }

    let kb_store = nexus_local_db::kb_store::SqliteKbStore::new(pool.clone());

    if let Some(port) = context.recording_knowledge_entry.as_deref() {
        let mut suppressed = Vec::with_capacity(records.len());
        for kb in records {
            kb_store
                .validate_key_block(&kb, None)
                .map_err(|e| kb_store_refusal(&e))?;
            kb_store
                .check_key_block_id_free(&kb)
                .await
                .map_err(|e| kb_store_refusal(&e))?;
            let entry = nexus_spoke_adapter::conversion::knowledge_record_to_spoke(&kb);
            if let SpokeResult::Reject(reject) = port.put_knowledge_entry(entry, None).await {
                return Err(NexusApiError::Internal {
                    category: format!("KB_PORT_ERROR: {reject:?}"),
                });
            }
            suppressed.push(serde_json::json!({ "entry_id": kb.entry_id }));
        }
        return Ok(serde_json::json!({
            "written": 0,
            "world_id": world_id,
            "suppressed_writes": suppressed,
        }));
    }

    let mut written: usize = 0;
    let mut tx = pool.begin().await.map_err(|e| NexusApiError::Internal {
        category: format!("DATABASE_ERROR: {e}"),
    })?;

    for kb in records {
        kb_store
            .insert_key_block_in_tx(&mut tx, kb)
            .await
            .map_err(|e| kb_store_refusal(&e))?;
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
    req: &ToolsApi,
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

    let now = chrono::Utc::now().to_rfc3339();
    let body_path: Option<String> = if let Some(content) = req.parameters["content"].as_str() {
        let workspace_root = context
            .workspace_path()
            .ok_or_else(|| NexusApiError::Internal {
                category: format!("WORKSPACE_PATH_ERROR: {}", "workspace path not available"),
            })?;
        let canonical_path = chapter_exists
            .as_ref()
            .and_then(|cr| cr.body_path.clone())
            .unwrap_or_else(|| {
                let ch_nn = format!("ch{chapter:02}");
                let wr = work_record.work_ref.as_deref().unwrap_or(work_id);
                format!("Works/{wr}/Stories/{ch_nn}-{ch_nn}.md")
            });

        resolve_guarded_path_async(
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

        // ADR #485 §6.1 P2 row 1: the final body bytes commit through the Work
        // artifact port (Work lock + durable atomic write). The port resolves
        // the work row on its own connection, so this call stays OUTSIDE the
        // update transaction below; a later DB failure rolls back the row
        // while the bytes stay landed, as before.
        let (body_work_ref, body_rel_child) = work_artifact_parts(&canonical_path)?;
        context
            .commit_work_artifact(WorkArtifactWrite {
                workspace_root: Path::new(&workspace_root),
                work_ref: body_work_ref,
                rel_child: body_rel_child,
                content: content.to_string(),
            })
            .await?;
        Some(canonical_path)
    } else {
        None
    };

    // C-002: the bytes already committed through the port above; the DB update
    // still runs in its own transaction so a failure rolls the row back.
    if let Some(bp) = &body_path {
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
        tx.commit().await.map_err(|e| NexusApiError::Internal {
            category: format!(
                "DATABASE_ERROR: {}",
                format_args!("chapter update tx commit: {e}")
            ),
        })?;
    }

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
    req: &ToolsApi,
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
    req: &ToolsApi,
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
    req: &ToolsApi,
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

    core.update_finding(
        &principal,
        finding_id.to_string(),
        FindingsApi {
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
    req: &ToolsApi,
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

/// Registry wrapper: `nexus.kb_snapshot.write` — async passthrough.
pub(crate) fn registry_kb_snapshot_write<'a>(
    req: &'a ToolsApi,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_kb_snapshot_write(req, context, creator_id))
}

/// Registry wrapper: `nexus.manuscript.chapter.update` — async passthrough.
pub(crate) fn registry_manuscript_chapter_update<'a>(
    req: &'a ToolsApi,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_manuscript_chapter_update(req, context, creator_id))
}

/// Registry wrapper: `nexus.world.configure` — async passthrough.
pub(crate) fn registry_world_configure<'a>(
    req: &'a ToolsApi,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_world_configure(req, context, creator_id))
}

/// Registry wrapper: `nexus.work.schedule.set` — async passthrough.
pub(crate) fn registry_work_schedule_set<'a>(
    req: &'a ToolsApi,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_work_schedule_set(req, context, creator_id))
}

/// Registry wrapper: `nexus.finding.resolve` — async passthrough.
pub(crate) fn registry_finding_resolve<'a>(
    req: &'a ToolsApi,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_finding_resolve(req, context, creator_id))
}

/// Registry wrapper: `nexus.pool.entry.manage` — async passthrough.
pub(crate) fn registry_pool_entry_manage<'a>(
    req: &'a ToolsApi,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_pool_entry_manage(req, context, creator_id))
}

//
// 9 catalog-only → shipped host tools (Track A). Each handler follows the
// existing execute_* + registry_* wrapper pattern. See
// the v1.59 DF-47 manuscript-and-misc capability batch (local plan record).

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
    _req: &ToolsApi,
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

/// Map the manuscript body guard error at the N-API boundary. The shared
/// strip runs first so an internal `chapter_path_unresolvable:<kind>` can
/// never reach `details.field` on the wire: the exact legacy field is all
/// this surface exports.
fn manuscript_guard_error(e: NexusApiError) -> NexusApiError {
    match crate::content::strip_chapter_path_kind(e) {
        NexusApiError::Coded { message, .. } => NexusApiError::InvalidInput {
            field: "body_path".into(),
            reason: message,
        },
        other => other,
    }
}

/// T2: `nexus.manuscript.read_range` — read a bounded content range from a chapter body.
#[allow(clippy::similar_names)] // the paired names are the domain vocabulary here
async fn execute_manuscript_read_range(
    req: &ToolsApi,
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

    let must_exist = abs_body.exists();
    let abs_body = resolve_guarded_path_async(
        workspace_root_path.to_path_buf(),
        body_path.clone(),
        must_exist,
    )
    .await
    .map_err(manuscript_guard_error)?;

    let content =
        tokio::fs::read_to_string(&abs_body)
            .await
            .map_err(|e| NexusApiError::Internal {
                category: format!(
                    "FILE_READ_FAILED: {}",
                    format_args!("failed to read manuscript body: {e}")
                ),
            })?;

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
                .map_or_default(<[&str]>::to_vec);
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
    req: &ToolsApi,
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

    let content_bytes = content.len();
    if content_bytes > MANUSCRIPT_WRITE_MAX_BYTES {
        return Err(NexusApiError::InvalidInput {
            field: "parameters.content".into(),
            reason: format!(
                "content size {content_bytes} exceeds maximum {MANUSCRIPT_WRITE_MAX_BYTES} bytes"
            ),
        });
    }

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

    // ADR #485 §6.1 P2 row 2: the final body bytes commit through the Work
    // artifact port (Work lock + durable atomic write) BEFORE the word-count
    // row update below. The port resolves the Work row on its own connection,
    // so it cannot join that transaction: if the update fails, the bytes stay
    // landed and only the metadata rolls back. That window is accepted — a
    // retried write re-commits the same bytes and the next successful update
    // restores the count — and it is why the byte commit comes first: the
    // reverse order would leave a row describing bytes that never landed.
    let (body_work_ref, body_rel_child) = work_artifact_parts(&body_path)?;
    context
        .commit_work_artifact(WorkArtifactWrite {
            workspace_root: workspace_root_path,
            work_ref: body_work_ref,
            rel_child: body_rel_child,
            content: content.to_string(),
        })
        .await?;

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
    req: &ToolsApi,
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
    req: &ToolsApi,
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
    _req: &ToolsApi,
    context: &ToolContext,
    _creator_id: &str,
) -> Result<serde_json::Value, NexusApiError> {
    let workspace_root = context
        .workspace_path()
        .ok_or_else(|| NexusApiError::InvalidInput {
            field: "workspace".into(),
            reason: "workspace not initialized; run `nexus42 workspace init` first".into(),
        })?;

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

/// T8: `nexus.runtime.health` — agent-visible runtime health (distinct from
/// `nexus.observability.daemon.health` which exposes uptime + lifecycle).
///
/// Returns registry reachability, sync context, and cloud-enabled flag.
fn execute_runtime_health(
    _req: &ToolsApi,
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
    req: &ToolsApi,
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

/// Registry wrapper: `nexus.manuscript.list` — async passthrough.
pub(crate) fn registry_manuscript_list<'a>(
    req: &'a ToolsApi,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_manuscript_list(req, context, creator_id))
}

/// Registry wrapper: `nexus.manuscript.read_range` — async passthrough.
pub(crate) fn registry_manuscript_read_range<'a>(
    req: &'a ToolsApi,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_manuscript_read_range(req, context, creator_id))
}

/// Registry wrapper: `nexus.manuscript.write` — async passthrough.
pub(crate) fn registry_manuscript_write<'a>(
    req: &'a ToolsApi,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_manuscript_write(req, context, creator_id))
}

/// Registry wrapper: `nexus.manuscript.phase.get` — async passthrough.
pub(crate) fn registry_manuscript_phase_get<'a>(
    req: &'a ToolsApi,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_manuscript_phase_get(req, context, creator_id))
}

/// Registry wrapper: `nexus.manuscript.phase.set` — async passthrough.
pub(crate) fn registry_manuscript_phase_set<'a>(
    req: &'a ToolsApi,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    Box::pin(execute_manuscript_phase_set(req, context, creator_id))
}

/// Registry wrapper: `nexus.workspace.paths` — sync → async wrapper.
pub(crate) fn registry_workspace_paths<'a>(
    req: &'a ToolsApi,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    let result = execute_workspace_paths(req, context, creator_id);
    Box::pin(async move { result })
}

/// Registry wrapper: `nexus.runtime.health` — sync → async wrapper.
pub(crate) fn registry_runtime_health<'a>(
    req: &'a ToolsApi,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    let result = execute_runtime_health(req, context, creator_id);
    Box::pin(async move { Ok(result) })
}

/// Registry wrapper: `nexus.trace.correlation` — sync → async wrapper.
pub(crate) fn registry_trace_correlation<'a>(
    req: &'a ToolsApi,
    context: &'a ToolContext,
    creator_id: &'a str,
) -> Pin<Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>> {
    let result = execute_trace_correlation(req, context, creator_id);
    Box::pin(async move { Ok(result) })
}

/// Unified handler function signature for all registered capabilities.
///
/// Takes references to the tool request, workspace context, and creator id,
/// returns a boxed future resolving to `Result<serde_json::Value, NexusApiError>`.
pub type RegistryHandlerFn = for<'a> fn(
    &'a ToolsApi,
    &'a ToolContext,
    &'a str,
) -> Pin<
    Box<dyn Future<Output = Result<serde_json::Value, NexusApiError>> + Send + 'a>,
>;

/// One bound registry row: the canonical metadata plus its real handler.
#[derive(Clone, Copy)]
pub struct CapabilityRow {
    /// Canonical row (id, access, admission gates, schemas, failure, test).
    pub metadata: &'static HostToolMetadata,
    /// Handler function binding.
    pub handler: RegistryHandlerFn,
}

/// Central registry for `nexus.*` host tool capabilities.
///
/// Built once at daemon startup. Provides O(1) lookup by
/// capability id and a unified `dispatch()` method that
/// mirrors the old `dispatch_tool()` behavior.
pub struct CapabilityRegistry {
    rows: HashMap<&'static str, CapabilityRow>,
    /// Consultations of the peer/user fallback lanes.
    ///
    /// Compiled IN by `test-hooks`, which this crate's integration tests enable
    /// through their dev-dependency self-reference; the flag must stay out of
    /// shipped feature selections, so a normal build carries no counter at all.
    /// Both entry paths increment it (`note_fallback_consultation`), so a test
    /// can prove the canonical reservation never reaches those lanes.
    #[cfg(any(test, feature = "test-hooks"))]
    fallback_consultations: AtomicUsize,
}

impl CapabilityRegistry {
    /// Create an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            rows: HashMap::new(),
            #[cfg(any(test, feature = "test-hooks"))]
            fallback_consultations: AtomicUsize::new(0),
        }
    }

    /// How many dispatches consulted the peer/user fallback lanes.
    ///
    /// `test-hooks` only: counts lane consultations (a refused lane still
    /// records one), which is the invariant the canonical reservation owns.
    /// The allowlist path (`spine_resolves`) and `dispatch` both feed it.
    #[cfg(any(test, feature = "test-hooks"))]
    #[must_use]
    pub fn fallback_consultations(&self) -> usize {
        self.fallback_consultations.load(Ordering::Relaxed)
    }

    /// Records one peer/user fallback-lane consultation.
    ///
    /// The single increment site: both entry paths call it immediately before
    /// they ask a lane, so the probe measures the same decision in the
    /// allowlist and in dispatch. It observes; it changes no behaviour.
    fn note_fallback_consultation(&self) {
        #[cfg(any(test, feature = "test-hooks"))]
        self.fallback_consultations.fetch_add(1, Ordering::Relaxed);
    }

    /// Whether `id` is a canonical host tool this registry binds no row for.
    ///
    /// The one canonical-miss decision shared by the allowlist
    /// ([`Self::spine_resolves`]) and [`Self::dispatch`]: the namespace reserves
    /// the id, so neither path may ask the peer or user lanes about it. A bound
    /// canonical row is resolved by `lookup` and never reaches this test.
    #[must_use]
    fn canonical_miss(&self, id: &str) -> bool {
        self.lookup(id).is_none() && lookup_host_tool(id).is_some()
    }

    /// Register a capability row.
    ///
    /// # Panics
    ///
    /// Panics if a row with the same `id` is already registered
    /// (duplicate capability ids are a programmer error).
    pub fn register(&mut self, row: CapabilityRow) {
        let id = row.metadata.id.as_str();
        assert!(
            !self.rows.contains_key(id),
            "duplicate capability id in registry: {id}"
        );
        self.rows.insert(id, row);
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
    ///
    /// A canonical id this registry binds no row for is NOT dispatchable, and
    /// the lanes are never asked about it: this is the same
    /// [`Self::canonical_miss`] decision [`Self::dispatch`] uses, so the
    /// caller-facing allowlist cannot report a reserved id as reachable while
    /// dispatch refuses it.
    #[must_use]
    pub fn spine_resolves(&self, context: &ToolContext, id: &str) -> bool {
        if self.lookup(id).is_some() {
            return true;
        }
        if self.canonical_miss(id) {
            return false;
        }
        self.note_fallback_consultation();
        if peer_tools::peer_tool_registry().get(id).is_some() {
            return true;
        }
        context
            .user_capabilities()
            .is_some_and(|reg| user_cap_catalog_admission(reg.lookup(id)).is_ok())
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
    /// if the tool is not registered, or if it is a canonical host tool this
    /// build binds no handler for (the peer and user lanes are never consulted
    /// for a reserved builtin id). Individual handlers may return
    /// other error variants (e.g. `Forbidden`, `InvalidInput`).
    pub async fn dispatch(
        &self,
        req: &ToolsApi,
        context: &ToolContext,
        creator_id: &str,
    ) -> Result<serde_json::Value, NexusApiError> {
        let row = self.lookup(&req.tool_name);
        if row.is_none() {
            if !self.canonical_miss(&req.tool_name) {
                self.note_fallback_consultation();
                // Peer arm (AR-68 #4): reverse-invoke the owning responder.
                if let Some(entry) = peer_tools::peer_tool_registry().get(&req.tool_name) {
                    return dispatch_peer_tool(&entry, req).await;
                }
                // User-capability arm (AR-68 #6): `Capability::run(arguments)`.
                if let Some(reg) = context.user_capabilities() {
                    if let Ok(cap) = user_cap_catalog_admission(reg.lookup(&req.tool_name)) {
                        return dispatch_user_cap(cap, req).await;
                    }
                }
            }
            return Err(NexusApiError::Coded {
                code: "not_supported".to_string(),
                message: format!("unsupported tool: {}", req.tool_name),
            });
        }
        let row = row.expect("row present");

        for gate in row.metadata.admission {
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
                row.metadata.id.as_str()
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
    entry: &peer_tools::PeerToolEntry,
    req: &ToolsApi,
) -> Result<Value, NexusApiError> {
    use peer_tools::PeerInvokeError;
    match peer_tools::invoke_peer_tool(entry, req.parameters.clone()).await {
        Ok(value) => Ok(value),
        Err(PeerInvokeError::Denied {
            wire_code: None,
            message,
        }) => Err(NexusApiError::Coded {
            code: "invalid_input".to_string(),
            message,
        }),
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
    cap: &dyn nexus_capability::Capability,
    req: &ToolsApi,
) -> Result<Value, NexusApiError> {
    use nexus_capability::CapabilityError;
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
        CapabilityError::WorkerUnavailable => NexusApiError::Coded {
            code: "service_unavailable".to_string(),
            message: format!("capability '{}' has no executor wired", cap.name()),
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
    cap: Option<&dyn nexus_capability::Capability>,
) -> Result<&dyn nexus_capability::Capability, UserCapCatalogRefusal> {
    let cap = cap.ok_or(UserCapCatalogRefusal::NotUserCapability)?;
    if cap.origin() != nexus_capability::CapabilityOrigin::User {
        return Err(UserCapCatalogRefusal::NotUserCapability);
    }
    let name = cap.name();
    if name.starts_with("nexus.") {
        return Err(UserCapCatalogRefusal::ReservedNamespace);
    }
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

/// Create a registry pre-populated by binding every canonical catalog row.
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
pub fn build_registry() -> CapabilityRegistry {
    build_registry_inner(None)
}

/// The ONE registration path: every canonical row this build binds a handler
/// for, except the optional `unbound` one.
///
/// `unbound` is the test-only shape (a canonical tool the fixture deliberately
/// leaves unbound). Production always passes `None`; the `test-hooks` wrapper
/// below is how a test names the row, so the fixture can never drift from the
/// real registration rules. `handler_for` returning `None` is the feature-gated
/// shape: the row stays canonical and reserved, but this build binds nothing.
fn build_registry_inner(unbound: Option<HostToolId>) -> CapabilityRegistry {
    let mut reg = CapabilityRegistry::new();
    for metadata in host_tool_catalog() {
        if unbound == Some(metadata.id) {
            continue;
        }
        let Some(handler) = handler_for(metadata.id) else {
            continue;
        };
        reg.register(CapabilityRow { metadata, handler });
    }
    reg
}

/// Builds the registry with one canonical tool deliberately left unbound.
///
/// The id stays canonical — reserved by the catalog — but has no handler row, so
/// it is absent from this registry's runnable ids and binds nothing in the
/// catalog's listing. That is the state `dispatch` and `spine_resolves` must
/// refuse instead of resolving through the peer or user lane.
#[cfg(any(test, feature = "test-hooks"))]
#[must_use]
pub fn build_registry_without(unbound: HostToolId) -> CapabilityRegistry {
    build_registry_inner(Some(unbound))
}

/// The real handler for one canonical tool, or `None` when this build links no
/// binding for it.
///
/// Exhaustive over [`HostToolId`], so a canonical row cannot ship without a
/// handler decision; the feature-gated arms decide with a real `Option`.
fn handler_for(id: HostToolId) -> Option<RegistryHandlerFn> {
    let handler = match id {
        HostToolId::ContextWhoami => registry_context_whoami,
        HostToolId::WorkspaceInfo => registry_workspace_info,
        HostToolId::WorkGet => registry_work_get,
        HostToolId::WorkPatch => registry_work_patch,
        HostToolId::OrchestrationScheduleStatus => registry_schedule_status,
        HostToolId::StrategyGenerate => return STRATEGY_GENERATE_HANDLER,
        HostToolId::ContextAssemble => registry_context_assemble,
        HostToolId::WorldSnapshotGet => registry_world_snapshot_get,
        HostToolId::TimelineRecentGet => registry_timeline_recent_get,
        HostToolId::KbSnapshotRead => registry_kb_snapshot_read,
        HostToolId::ManuscriptChapterGet => registry_manuscript_chapter_get,
        HostToolId::ObservabilityDaemonHealth => registry_daemon_health,
        HostToolId::KbSnapshotWrite => registry_kb_snapshot_write,
        HostToolId::ManuscriptChapterUpdate => registry_manuscript_chapter_update,
        HostToolId::WorldConfigure => registry_world_configure,
        HostToolId::WorkScheduleSet => registry_work_schedule_set,
        HostToolId::FindingResolve => registry_finding_resolve,
        HostToolId::PoolEntryManage => registry_pool_entry_manage,
        HostToolId::RegistryRefresh => registry_registry_refresh,
        HostToolId::ManuscriptList => registry_manuscript_list,
        HostToolId::ManuscriptReadRange => registry_manuscript_read_range,
        HostToolId::ManuscriptWrite => registry_manuscript_write,
        HostToolId::ManuscriptPhaseGet => registry_manuscript_phase_get,
        HostToolId::ManuscriptPhaseSet => registry_manuscript_phase_set,
        HostToolId::WorkspacePaths => registry_workspace_paths,
        HostToolId::RuntimeHealth => registry_runtime_health,
        HostToolId::TraceCorrelation => registry_trace_correlation,
        HostToolId::FsReadTextFile => registry_read_file,
        HostToolId::FsWriteTextFile => registry_write_file,
    };
    Some(handler)
}

/// The Generate binding: present only where the authoring feature links the
/// kernel. Without it the canonical id keeps its reservation and dispatch
/// refuses before any peer or user lane is asked.
#[cfg(feature = "strategy-authoring")]
const STRATEGY_GENERATE_HANDLER: Option<RegistryHandlerFn> = Some(registry_strategy_generate);

#[cfg(not(feature = "strategy-authoring"))]
const STRATEGY_GENERATE_HANDLER: Option<RegistryHandlerFn> = None;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use nexus_local_db::works::WorkRecord;
    use sqlx::SqlitePool;
    use tempfile::TempDir;

    const CREATOR: &str = "ctr_test";
    const WORK_ID: &str = "wrk_demo";
    const WORK_REF: &str = "demo";

    struct Fixture {
        _home: TempDir,
        workspace: TempDir,
        pool: SqlitePool,
        context: ToolContext,
    }

    fn work_record() -> WorkRecord {
        WorkRecord {
            work_id: WORK_ID.to_string(),
            creator_id: CREATOR.to_string(),
            workspace_slug: "default".to_string(),
            status: "draft".to_string(),
            title: "My Novel".to_string(),
            long_term_goal: "Write a great novel".to_string(),
            initial_idea: "A sci-fi thriller".to_string(),
            creative_brief: None,
            intake_status: "pending".to_string(),
            world_id: None,
            story_ref: None,
            inspiration_log: "[]".to_string(),
            primary_preset_id: "novel-writing".to_string(),
            schedule_ids: "[]".to_string(),
            created_at: "2026-06-04T10:00:00Z".to_string(),
            updated_at: "2026-06-04T10:00:00Z".to_string(),
            current_stage: "intake".to_string(),
            stage_status: "pending".to_string(),
            work_profile: None,
            work_ref: Some(WORK_REF.to_string()),
            total_planned_chapters: None,
            current_chapter: 0,
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

    async fn fixture(seed_work: bool) -> Fixture {
        let home = tempfile::tempdir().expect("temp home");
        let workspace = tempfile::tempdir().expect("temp workspace");
        std::fs::write(
            home.path().join("config.toml"),
            format!(
                "active_creator_id = \"{CREATOR}\"\n\
                 active_workspace_slug_by_creator = {{ {CREATOR} = \"default\" }}\n"
            ),
        )
        .expect("cli config");
        let pool = nexus_local_db::init_engine_pool(&home.path().join("state.db"))
            .await
            .expect("engine pool")
            .clone_pool();
        if seed_work {
            works::create_work(&pool, &work_record())
                .await
                .expect("works row");
        }
        let context = ToolContext::new(
            pool.clone(),
            home.path().to_path_buf(),
            Some(workspace.path().display().to_string()),
            ToolRuntimeFacts::default(),
            None,
            None,
        );
        Fixture {
            _home: home,
            workspace,
            pool,
            context,
        }
    }

    fn write_request(path: &str, content: &str) -> ToolsApi {
        ToolsApi {
            caller_kind: None,
            parameters: serde_json::json!({ "path": path, "content": content }),
            request_id: None,
            session_id: None,
            tool_name: "fs/write_text_file".to_string(),
        }
    }

    fn read_request(path: &str) -> ToolsApi {
        ToolsApi {
            caller_kind: None,
            parameters: serde_json::json!({ "path": path }),
            request_id: None,
            session_id: None,
            tool_name: "fs/read_text_file".to_string(),
        }
    }

    fn assert_case_variant_works_refusal(error: NexusApiError, path: &str) {
        match error {
            NexusApiError::Forbidden { resource } => assert!(
                resource.contains("must spell the Work root 'Works' byte-exactly"),
                "{path}: {resource}"
            ),
            other => panic!("expected Forbidden for {path}, got {other:?}"),
        }
    }

    async fn holder(pool: &SqlitePool) -> Option<String> {
        sqlx::query_scalar("SELECT runtime_lock_holder FROM works WHERE work_id = ?")
            .bind(WORK_ID)
            .fetch_one(pool)
            .await
            .expect("holder read")
    }

    #[tokio::test]
    async fn execute_write_file_writes_work_artifacts_through_the_port() {
        let fixture = fixture(true).await;

        let result = execute_write_file(
            &write_request(&format!("Works/{WORK_REF}/Stories/ch1.md"), "chapter body"),
            &fixture.context,
        )
        .await
        .expect("the Work artifact write succeeds");

        assert_eq!(result, serde_json::json!({ "written": true }));
        let target = fixture
            .workspace
            .path()
            .join(format!("Works/{WORK_REF}/Stories/ch1.md"));
        assert_eq!(
            std::fs::read_to_string(&target).expect("read"),
            "chapter body"
        );
        assert_eq!(
            std::fs::read_dir(target.parent().expect("parent"))
                .expect("read_dir")
                .count(),
            1,
            "the atomic temp file is cleaned up"
        );
        assert_eq!(holder(&fixture.pool).await, None, "the lock is released");
    }

    /// Every entry below `root`, ordered by relative path, with a file's bytes.
    fn entry_snapshot(root: &std::path::Path) -> Vec<(String, Option<Vec<u8>>)> {
        fn walk(
            root: &std::path::Path,
            dir: &std::path::Path,
            out: &mut Vec<(String, Option<Vec<u8>>)>,
        ) {
            let mut paths: Vec<_> = std::fs::read_dir(dir)
                .expect("read_dir")
                .map(|entry| entry.expect("dir entry").path())
                .collect();
            paths.sort();
            for path in paths {
                let rel = path
                    .strip_prefix(root)
                    .expect("entry below the snapshot root")
                    .display()
                    .to_string();
                if path.is_dir() {
                    out.push((rel, None));
                    walk(root, &path, out);
                } else {
                    out.push((rel, Some(std::fs::read(&path).expect("read entry"))));
                }
            }
        }
        let mut out = Vec::new();
        walk(root, root, &mut out);
        out
    }

    /// I-000460: a dry run over a workspace that already holds real chapter
    /// bytes must record the write and leave the file untouched. Removing the
    /// recording port from `ToolContext::new_simulated` rewrites the chapter, so
    /// this test is the negative control for that selection.
    #[tokio::test]
    async fn simulated_fs_write_records_and_preserves_an_existing_chapter() {
        use sha2::{Digest, Sha256};

        let fixture = fixture(true).await;
        let chapter = format!("Works/{WORK_REF}/Stories/ch1.md");
        let target = fixture.workspace.path().join(&chapter);
        std::fs::create_dir_all(target.parent().expect("chapter dir")).expect("chapter dir");
        std::fs::write(&target, "the original chapter\n").expect("seed the chapter");
        let before = entry_snapshot(fixture.workspace.path());

        let context = ToolContext::new_simulated(
            fixture.pool.clone(),
            fixture.context.nexus_home.clone(),
            fixture.context.workspace_path(),
            ToolRuntimeFacts::default(),
            None,
            None,
        );
        execute_write_file(&write_request(&chapter, "a rewritten chapter\n"), &context)
            .await
            .expect("the simulated write succeeds");

        assert_eq!(
            std::fs::read_to_string(&target).expect("read the chapter"),
            "the original chapter\n",
            "a dry run must leave the existing chapter bytes alone"
        );
        assert_eq!(
            entry_snapshot(fixture.workspace.path()),
            before,
            "a dry run must not add, remove or rewrite any tree entry"
        );

        let mutations = context
            .recorded_work_artifact_mutations()
            .expect("a simulated context returns its log");
        assert_eq!(mutations.len(), 1, "one Work write was intended");
        assert_eq!(mutations[0].work_ref, WORK_REF);
        assert_eq!(mutations[0].rel_child, "Stories/ch1.md");
        assert_eq!(mutations[0].bytes, "a rewritten chapter\n".len());
        assert_eq!(
            mutations[0].sha256,
            hex::encode(Sha256::digest(b"a rewritten chapter\n")),
            "the log carries the digest of the bytes it would have written"
        );
        assert!(
            fixture.context.recorded_work_artifact_mutations().is_none(),
            "a production context reports no log rather than an empty one"
        );
    }

    /// I-000514: a simulated context refuses any non-`Works/` fs write and
    /// never creates or modifies the target. Removing the refusal branch lets
    /// the guarded `std::fs::write` land the file, so this test is the negative
    /// control for that guard.
    #[tokio::test]
    async fn simulated_fs_write_refuses_non_work_paths() {
        let fixture = fixture(true).await;
        let context = ToolContext::new_simulated(
            fixture.pool.clone(),
            fixture.context.nexus_home.clone(),
            fixture.context.workspace_path(),
            ToolRuntimeFacts::default(),
            None,
            None,
        );

        let path = "notes/outline.md".to_string();
        let error = execute_write_file(&write_request(&path, "spilled bytes\n"), &context)
            .await
            .expect_err("a simulated context must refuse a non-Works write");
        match error {
            NexusApiError::Forbidden { resource } => assert!(
                resource.contains(&path),
                "the refusal names the requested path: {resource}"
            ),
            other => panic!("expected Forbidden, got {other:?}"),
        }

        assert!(
            !fixture.workspace.path().join(&path).exists(),
            "the target must never be created"
        );
        let mutations = context
            .recorded_work_artifact_mutations()
            .expect("a simulated context returns its log");
        assert!(mutations.is_empty(), "no Work mutations were recorded");
    }

    fn kb_write_request(blocks: &serde_json::Value) -> ToolsApi {
        ToolsApi {
            caller_kind: None,
            parameters: serde_json::json!({ "world_id": "wld_sim", "blocks": blocks }),
            request_id: None,
            session_id: None,
            tool_name: "nexus.kb_snapshot.write".to_string(),
        }
    }

    /// Every `kb_key_blocks` row exactly as stored, one JSON array per row,
    /// ordered by primary key.
    ///
    /// The column list is read from the schema, so every persisted column takes
    /// part. Each column contributes its `typeof` tag plus a lossless rendering:
    /// `printf('%!.17g')` for `real` (17 significant digits round-trip a double,
    /// where `CAST(real AS BLOB)` truncates through 15-digit text), and `hex` of
    /// the raw bytes otherwise. `NULL`, the empty string, a string with an
    /// embedded NUL and a BLOB sharing that prefix all render differently, and no
    /// text conversion can truncate the value. Any insert, update or delete
    /// changes this comparison.
    async fn kb_rows(pool: &SqlitePool) -> Vec<String> {
        let columns: Vec<String> =
            sqlx::query_scalar("SELECT name FROM pragma_table_info('kb_key_blocks') ORDER BY cid")
                .fetch_all(pool)
                .await
                .expect("kb columns");
        let rendered = columns
            .iter()
            .map(|column| {
                format!(
                    "typeof({column}), CASE typeof({column}) WHEN 'real' \
                     THEN printf('%!.17g', {column}) ELSE hex(CAST({column} AS BLOB)) END"
                )
            })
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!("SELECT json_array({rendered}) FROM kb_key_blocks ORDER BY key_block_id");
        sqlx::query_scalar::<_, String>(sqlx::AssertSqlSafe(sql))
            .fetch_all(pool)
            .await
            .expect("kb rows")
    }

    /// The world the simulated knowledge tests write into, plus one known
    /// `kb_key_blocks` row.
    async fn seed_kb_fixture(pool: &SqlitePool) {
        nexus_local_db::ensure_creator_row(pool, CREATOR, "Test")
            .await
            .expect("creator row");
        sqlx::query(
            "INSERT INTO narrative_worlds (world_id, workspace_id, owner_creator_id, title, \
             slug, status, visibility, time_policy, metadata_json) \
             VALUES ('wld_sim', 'wrk', ?, 't', 's', 'active', 'private', 'manual', '{}')",
        )
        .bind(CREATOR)
        .execute(pool)
        .await
        .expect("world row");
        sqlx::query(
            "INSERT INTO kb_key_blocks (key_block_id, world_id, block_type, canonical_name, \
             status, revision, created_at, updated_at) \
             VALUES ('kb_sim_existing', 'wld_sim', 'character', 'existing_entry', 'confirmed', 3, \
             datetime('now'), datetime('now'))",
        )
        .execute(pool)
        .await
        .expect("known kb row");
    }

    /// The knowledge stub is selected by `ToolContext::new_simulated`: a preview
    /// that reaches `nexus.kb_snapshot.write` records the intended put and leaves
    /// `kb_key_blocks` byte-identical. Removing that selection inserts the row for
    /// real, so this test is the negative control for it.
    #[tokio::test]
    async fn simulated_knowledge_write_records_and_preserves_the_store() {
        use nexus_contracts::BlockType;
        use nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord;

        let fixture = fixture(false).await;
        seed_kb_fixture(&fixture.pool).await;

        let mut block = KnowledgeEntryRecord::new("wld_sim", BlockType::Character, "sim_probe");
        block.entry_id = "kb_sim_probe".to_string();
        let block = serde_json::to_value(&block).expect("serialize the block");

        let simulated = ToolContext::new_simulated(
            fixture.pool.clone(),
            fixture.context.nexus_home.clone(),
            fixture.context.workspace_path(),
            ToolRuntimeFacts::default(),
            None,
            None,
        );
        let before = kb_rows(&fixture.pool).await;
        assert_eq!(before.len(), 1, "the fixture seeds one known row");
        assert!(
            before[0].contains(&hex::encode("existing_entry").to_uppercase()),
            "the snapshot carries the seeded row's bytes: {}",
            before[0]
        );

        let result = execute_kb_snapshot_write(
            &kb_write_request(&serde_json::json!([block.clone()])),
            &simulated,
            CREATOR,
        )
        .await
        .expect("the simulated write succeeds");

        assert_eq!(
            kb_rows(&fixture.pool).await,
            before,
            "a dry run must leave every persisted kb_key_blocks row byte-identical"
        );
        assert_eq!(
            result["written"],
            serde_json::json!(0),
            "a preview commits nothing"
        );
        assert_eq!(
            result["suppressed_writes"],
            serde_json::json!([{ "entry_id": "kb_sim_probe" }]),
            "the preview names the written intent, with the record's own fields"
        );
        assert_eq!(
            simulated.recorded_knowledge_entry_mutations(),
            Some(vec![StubKnowledgeEntryMutation::Put {
                entry_id: "kb_sim_probe".to_string(),
                expected_base_revision: None,
            }]),
            "the attempt is recorded, not swallowed"
        );

        let empty = execute_kb_snapshot_write(
            &kb_write_request(&serde_json::json!([])),
            &simulated,
            CREATOR,
        )
        .await
        .expect("the empty simulated write succeeds");
        assert_eq!(empty["written"], serde_json::json!(0));
        assert_eq!(empty["suppressed_writes"], serde_json::json!([]));

        assert!(
            fixture
                .context
                .recorded_knowledge_entry_mutations()
                .is_none(),
            "a production context reports no log rather than an empty one"
        );
        let live = execute_kb_snapshot_write(
            &kb_write_request(&serde_json::json!([block])),
            &fixture.context,
            CREATOR,
        )
        .await
        .expect("the production write succeeds");
        assert_eq!(live["written"], serde_json::json!(1));
        assert!(
            live.get("suppressed_writes").is_none(),
            "a production write carries no suppression report"
        );
        assert_eq!(kb_rows(&fixture.pool).await.len(), before.len() + 1);
    }

    /// A simulated write must answer the refusal production would give for the
    /// same block: these fields are `Option<String>`/`String` on the wire, so
    /// `{"disclosure": ""}` or `{"canonical_name": ""}` deserializes and reaches
    /// the store. Only the store's record gates refuse them — and
    /// `knowledge_record_to_spoke` asserts those gates with `.expect`, so a
    /// simulated branch that converts without running them panics inside the
    /// request path where production returns a refusal.
    #[tokio::test]
    async fn simulated_knowledge_write_refuses_what_production_refuses() {
        use nexus_contracts::BlockType;
        use nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord;

        let fixture = fixture(false).await;
        seed_kb_fixture(&fixture.pool).await;
        let simulated = ToolContext::new_simulated(
            fixture.pool.clone(),
            fixture.context.nexus_home.clone(),
            fixture.context.workspace_path(),
            ToolRuntimeFacts::default(),
            None,
            None,
        );
        let before = kb_rows(&fixture.pool).await;

        let cases = [
            (
                "disclosure",
                "",
                "disclosure must be a nonempty stored value",
            ),
            (
                "holder_entry_id",
                "",
                "holder_entry_id must be a nonempty resolved holder id",
            ),
            ("canonical_name", "", "canonical_name must not be empty"),
            (
                "disclosure",
                "shared",
                "unknown disclosure vocabulary \"shared\"; only \"owner-private\" is native",
            ),
        ];

        for (field, value, reason) in cases {
            let mut block =
                KnowledgeEntryRecord::new("wld_sim", BlockType::Character, "sim_refused");
            block.entry_id = "kb_sim_refused".to_string();
            let mut payload = serde_json::to_value(&block).expect("serialize the block");
            payload[field] = serde_json::json!(value);

            let live = execute_kb_snapshot_write(
                &kb_write_request(&serde_json::json!([payload.clone()])),
                &fixture.context,
                CREATOR,
            )
            .await
            .expect_err("production refuses this block");

            let refused = execute_kb_snapshot_write(
                &kb_write_request(&serde_json::json!([payload])),
                &simulated,
                CREATOR,
            )
            .await
            .expect_err("the simulation refuses what production refuses");

            assert_eq!(
                format!("{refused:?}"),
                format!("{live:?}"),
                "{field}={value:?} must be refused exactly as production refuses it"
            );
            match &refused {
                NexusApiError::Internal { category } => assert!(
                    category.starts_with("KB_STORE_ERROR: validation error: ")
                        && category.contains(reason),
                    "the refusal must carry the store's own reason {reason:?}: {category}"
                ),
                other => {
                    panic!("expected the store's refusal for {field}={value:?}, got {other:?}")
                }
            }
        }

        assert_eq!(
            simulated.recorded_knowledge_entry_mutations(),
            Some(vec![]),
            "a refused block never reaches the port"
        );
        assert_eq!(
            kb_rows(&fixture.pool).await,
            before,
            "a refused batch writes nothing, in either branch"
        );
    }

    /// A `key_block_id` an existing row already holds is the primary-key
    /// collision `insert_key_block_in_tx` reports, so the simulated branch must
    /// answer with the same refusal instead of previewing the block as writable;
    /// the same request with a free id must still preview as writable.
    /// `canonical_name` differs from every stored row, so the primary key is the
    /// only guard in play.
    #[tokio::test]
    async fn simulated_knowledge_write_refuses_a_taken_key_block_id() {
        use nexus_contracts::BlockType;
        use nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord;

        let fixture = fixture(false).await;
        seed_kb_fixture(&fixture.pool).await;
        let simulated = ToolContext::new_simulated(
            fixture.pool.clone(),
            fixture.context.nexus_home.clone(),
            fixture.context.workspace_path(),
            ToolRuntimeFacts::default(),
            None,
            None,
        );
        let before = kb_rows(&fixture.pool).await;

        let mut colliding =
            KnowledgeEntryRecord::new("wld_sim", BlockType::Character, "sim_pk_probe");
        colliding.entry_id = "kb_sim_existing".to_string();
        let payload = serde_json::to_value(&colliding).expect("serialize the block");

        let live = execute_kb_snapshot_write(
            &kb_write_request(&serde_json::json!([payload.clone()])),
            &fixture.context,
            CREATOR,
        )
        .await
        .expect_err("production refuses a taken key_block_id");
        let refused = execute_kb_snapshot_write(
            &kb_write_request(&serde_json::json!([payload])),
            &simulated,
            CREATOR,
        )
        .await
        .expect_err("a taken key_block_id must not preview as writable");

        assert_eq!(
            format!("{refused:?}"),
            format!("{live:?}"),
            "the preview must refuse a taken key_block_id exactly as production refuses it"
        );

        let mut fresh = KnowledgeEntryRecord::new("wld_sim", BlockType::Character, "sim_pk_fresh");
        fresh.entry_id = "kb_sim_fresh".to_string();
        let result = execute_kb_snapshot_write(
            &kb_write_request(&serde_json::json!([
                serde_json::to_value(&fresh).expect("serialize the block")
            ])),
            &simulated,
            CREATOR,
        )
        .await
        .expect("a free key_block_id still previews as writable");
        assert_eq!(result["written"], serde_json::json!(0));
        assert_eq!(
            result["suppressed_writes"],
            serde_json::json!([{ "entry_id": "kb_sim_fresh" }]),
            "the free id is the only recorded intent"
        );
        assert_eq!(
            simulated.recorded_knowledge_entry_mutations(),
            Some(vec![StubKnowledgeEntryMutation::Put {
                entry_id: "kb_sim_fresh".to_string(),
                expected_base_revision: None,
            }]),
            "the refused id never reached the port"
        );
        assert_eq!(
            kb_rows(&fixture.pool).await,
            before,
            "the refusal and the preview both write nothing"
        );
    }

    /// The snapshot must see every stored byte: `quote(TEXT)` stops at an
    /// embedded NUL, so a text column that gains a suffix after one would leave
    /// a `quote`-rendered snapshot unchanged. This is the renderer's own check —
    /// the type tag plus hex of the raw bytes must distinguish the two values.
    #[tokio::test]
    async fn kb_rows_sees_a_suffix_after_an_embedded_nul() {
        let fixture = fixture(false).await;
        seed_kb_fixture(&fixture.pool).await;
        let before = kb_rows(&fixture.pool).await;

        sqlx::query(
            "UPDATE kb_key_blocks SET canonical_name = ? WHERE key_block_id = 'kb_sim_existing'",
        )
        .bind("existing_entry\0changed")
        .execute(&fixture.pool)
        .await
        .expect("mutate the name after a NUL");

        let after = kb_rows(&fixture.pool).await;
        assert_ne!(
            after, before,
            "a suffix after an embedded NUL changes the stored bytes and must change the snapshot"
        );
        assert!(
            after[0].contains(&hex::encode("existing_entry\0changed").to_uppercase()),
            "the whole value, NUL included, reaches the snapshot: {}",
            after[0]
        );
    }

    /// The real branch must be lossless too: `CAST(REAL AS BLOB)` renders through
    /// 15-digit decimal text, so two doubles that differ past that precision share
    /// their encoding. `revision` is INTEGER-declared but this table is not
    /// `STRICT`, so a REAL is an admissible stored value — exactly the shape that
    /// would slip through a `CAST`-only snapshot.
    #[tokio::test]
    async fn kb_rows_sees_two_reals_that_differ_past_text_precision() {
        let fixture = fixture(false).await;
        seed_kb_fixture(&fixture.pool).await;

        let set_revision = |value: f64| {
            let pool = fixture.pool.clone();
            async move {
                sqlx::query(
                    "UPDATE kb_key_blocks SET revision = ? \
                     WHERE key_block_id = 'kb_sim_existing'",
                )
                .bind(value)
                .execute(&pool)
                .await
                .expect("set revision");
            }
        };

        set_revision(1.000_000_000_000_001).await;
        let first = kb_rows(&fixture.pool).await;
        set_revision(1.000_000_000_000_002).await;
        let second = kb_rows(&fixture.pool).await;

        assert!(
            first[0].contains("real"),
            "the seeded value reaches the real branch: {}",
            first[0]
        );
        assert_ne!(
            second, first,
            "doubles that differ past 15-digit text precision must change the snapshot"
        );
    }

    #[tokio::test]
    async fn execute_write_file_refuses_work_paths_without_a_works_row() {
        let fixture = fixture(false).await;

        let error = execute_write_file(
            &write_request(&format!("Works/{WORK_REF}/Stories/ch1.md"), "body"),
            &fixture.context,
        )
        .await
        .expect_err("a missing works row refuses");

        match error {
            NexusApiError::Forbidden { resource } => {
                assert!(resource.contains("has no works row"), "{resource}");
            }
            other => panic!("expected Forbidden, got {other:?}"),
        }
        assert!(!fixture
            .workspace
            .path()
            .join(format!("Works/{WORK_REF}"))
            .exists());
    }

    #[tokio::test]
    async fn execute_write_file_refuses_a_lock_held_elsewhere() {
        let fixture = fixture(true).await;
        nexus_local_db::acquire_runtime_lock(
            &fixture.pool,
            CREATOR,
            WORK_ID,
            "cli:http:foreign",
            nexus_local_db::DEFAULT_RUNTIME_LOCK_TTL_SECS,
            false,
        )
        .await
        .expect("foreign lock");

        let error = execute_write_file(
            &write_request(&format!("Works/{WORK_REF}/Stories/ch1.md"), "body"),
            &fixture.context,
        )
        .await
        .expect_err("a held lock refuses");

        match error {
            NexusApiError::Forbidden { resource } => {
                assert!(resource.contains("work_locked"), "{resource}");
            }
            other => panic!("expected Forbidden, got {other:?}"),
        }
        assert_eq!(
            holder(&fixture.pool).await.as_deref(),
            Some("cli:http:foreign"),
            "the foreign holder is untouched"
        );
    }

    #[tokio::test]
    async fn execute_write_file_treats_a_leading_curdir_work_path_as_the_artifact() {
        let fixture = fixture(true).await;

        execute_write_file(
            &write_request(
                &format!("./Works/{WORK_REF}/Stories/ch1.md"),
                "chapter body",
            ),
            &fixture.context,
        )
        .await
        .expect("a `.`-prefixed Work artifact write succeeds");

        let target = fixture
            .workspace
            .path()
            .join(format!("Works/{WORK_REF}/Stories/ch1.md"));
        assert_eq!(
            std::fs::read_to_string(&target).expect("read"),
            "chapter body"
        );
        assert_eq!(
            std::fs::read_dir(target.parent().expect("parent"))
                .expect("read_dir")
                .count(),
            1,
            "the atomic temp file is cleaned up"
        );
        assert_eq!(holder(&fixture.pool).await, None, "the lock is released");

        nexus_local_db::acquire_runtime_lock(
            &fixture.pool,
            CREATOR,
            WORK_ID,
            "cli:http:foreign",
            nexus_local_db::DEFAULT_RUNTIME_LOCK_TTL_SECS,
            false,
        )
        .await
        .expect("foreign lock");

        let error = execute_write_file(
            &write_request(&format!("./Works/{WORK_REF}/Stories/ch2.md"), "body"),
            &fixture.context,
        )
        .await
        .expect_err("a held lock refuses the `.`-prefixed write");

        match error {
            NexusApiError::Forbidden { resource } => {
                assert!(resource.contains("work_locked"), "{resource}");
            }
            other => panic!("expected Forbidden, got {other:?}"),
        }
        assert!(!fixture
            .workspace
            .path()
            .join(format!("Works/{WORK_REF}/Stories/ch2.md"))
            .exists());
    }

    #[tokio::test]
    async fn fs_leading_curdir_case_variant_spelling_is_refused_on_write_and_read() {
        let fixture = fixture(true).await;
        for tree in ["works", "Works"] {
            let dir = fixture
                .workspace
                .path()
                .join(format!("{tree}/{WORK_REF}/Stories"));
            std::fs::create_dir_all(&dir).expect("work dir");
            std::fs::write(dir.join("ch1.md"), "chapter body").expect("seed artifact");
        }
        let case_variant = format!("./works/{WORK_REF}/Stories/ch1.md");

        let write_error =
            execute_write_file(&write_request(&case_variant, "rewritten"), &fixture.context)
                .await
                .expect_err("a `.`-prefixed case-variant write refuses");
        let read_error = execute_read_file(&read_request(&case_variant), &fixture.context)
            .await
            .expect_err("a `.`-prefixed case-variant read refuses");

        assert_case_variant_works_refusal(write_error, &case_variant);
        assert_case_variant_works_refusal(read_error, &case_variant);
        assert_eq!(holder(&fixture.pool).await, None, "no lock was taken");
        assert_eq!(
            std::fs::read_to_string(
                fixture
                    .workspace
                    .path()
                    .join(format!("works/{WORK_REF}/Stories/ch1.md"))
            )
            .expect("read"),
            "chapter body",
            "the refused write left the case-variant artifact untouched"
        );
    }

    fn manuscript_write_request(content: &str, chapter: i32) -> ToolsApi {
        ToolsApi {
            caller_kind: None,
            parameters: serde_json::json!({
                "work_id": WORK_ID,
                "chapter": chapter,
                "content": content,
            }),
            request_id: None,
            session_id: None,
            tool_name: "nexus.manuscript.write".to_string(),
        }
    }

    /// ADR #485 §6.1 P2 row 2: the DB pointer and the file bytes agree.
    ///
    /// The word count the row records describes exactly the bytes that landed
    /// at the `body_path` the same row names.
    #[tokio::test]
    async fn manuscript_write_keeps_the_db_pointer_and_the_file_bytes_consistent() {
        let fixture = fixture(true).await;
        nexus_local_db::work_chapters::seed_chapters(
            &fixture.pool,
            WORK_ID,
            WORK_REF,
            1,
            "2026-06-04T10:00:00Z",
        )
        .await
        .expect("seed chapters");

        let content = "The locket opened on the tide.\n\nAria stepped through.";
        let result = execute_manuscript_write(
            &manuscript_write_request(content, 1),
            &fixture.context,
            CREATOR,
        )
        .await
        .expect("manuscript write");

        let expected_words = content.split_whitespace().count();
        assert_eq!(result["word_count"], expected_words);

        let chapter = nexus_local_db::work_chapters::get_chapter(&fixture.pool, WORK_ID, 1, 1)
            .await
            .expect("read chapter")
            .expect("chapter row");
        let body_path = chapter.body_path.expect("body_path recorded");
        assert_eq!(
            body_path,
            format!("Works/{WORK_REF}/Stories/ch01-ch01.md"),
            "the recorded pointer is the seeded canonical path"
        );
        assert_eq!(
            chapter.actual_word_count,
            Some(i32::try_from(expected_words).expect("word count fits i32")),
            "the word count describes the committed bytes"
        );

        let written =
            std::fs::read_to_string(fixture.workspace.path().join(&body_path)).expect("body file");
        assert_eq!(written, content, "the pointers' file holds the body");
        assert!(fixture
            .workspace
            .path()
            .join(format!("{body_path}.tmp"))
            .metadata()
            .is_err());
        assert_eq!(holder(&fixture.pool).await, None, "the lock is released");
    }

    /// ADR #485 §6.1 P2 row 1: the chapter update writes the bytes the row
    /// then points at, and the update transaction leaves no lock behind.
    #[tokio::test]
    async fn chapter_update_records_the_body_path_its_bytes_landed_in() {
        let fixture = fixture(true).await;
        nexus_local_db::work_chapters::seed_chapters(
            &fixture.pool,
            WORK_ID,
            WORK_REF,
            1,
            "2026-06-04T10:00:00Z",
        )
        .await
        .expect("seed chapters");

        let content = "Rewritten chapter body.";
        let mut request = manuscript_write_request(content, 1);
        request.tool_name = "nexus.manuscript.chapter.update".to_string();

        execute_manuscript_chapter_update(&request, &fixture.context, CREATOR)
            .await
            .expect("chapter update");

        let chapter = nexus_local_db::work_chapters::get_chapter(&fixture.pool, WORK_ID, 1, 1)
            .await
            .expect("read chapter")
            .expect("chapter row");
        let body_path = chapter.body_path.expect("body_path recorded");
        let written = std::fs::read_to_string(fixture.workspace.path().join(&body_path))
            .expect("the recorded pointer names an existing file");
        assert_eq!(
            written, content,
            "the recorded pointer names the written bytes"
        );
        assert_eq!(
            chapter.actual_word_count,
            Some(i32::try_from(content.split_whitespace().count()).expect("fits i32"))
        );
        assert_eq!(holder(&fixture.pool).await, None, "the lock is released");
    }

    #[tokio::test]
    async fn execute_write_file_is_not_reentrant_for_tool_fs_write() {
        let fixture = fixture(true).await;
        nexus_local_db::acquire_runtime_lock(
            &fixture.pool,
            CREATOR,
            WORK_ID,
            &nexus_local_db::cli_holder("tool:fs_write"),
            nexus_local_db::DEFAULT_RUNTIME_LOCK_TTL_SECS,
            false,
        )
        .await
        .expect("in-flight tool lock");

        let error = execute_write_file(
            &write_request(&format!("Works/{WORK_REF}/Stories/ch1.md"), "body"),
            &fixture.context,
        )
        .await
        .expect_err("a concurrent tool write refuses");

        match error {
            NexusApiError::Forbidden { resource } => {
                assert!(resource.contains("work_locked"), "{resource}");
            }
            other => panic!("expected Forbidden, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn execute_write_file_refuses_the_work_root_and_lexical_escapes() {
        let fixture = fixture(true).await;

        for (path, expected) in [
            (format!("Works/{WORK_REF}"), "is the Work root directory"),
            (
                format!("Works/{WORK_REF}/../plain.md"),
                "escapes the Works/demo/ boundary",
            ),
        ] {
            let error = execute_write_file(&write_request(&path, "body"), &fixture.context)
                .await
                .expect_err("the classification refuses");

            match error {
                NexusApiError::Forbidden { resource } => {
                    assert!(resource.contains(expected), "{path}: {resource}");
                }
                other => panic!("expected Forbidden for {path}, got {other:?}"),
            }
            assert_eq!(holder(&fixture.pool).await, None, "no lock was taken");
        }
        assert!(!fixture.workspace.path().join("plain.md").exists());
    }

    #[tokio::test]
    async fn fs_case_variant_works_spelling_is_refused_on_write_and_read() {
        let fixture = fixture(true).await;
        let case_variant = format!("works/{WORK_REF}/Stories/ch1.md");
        let exact = format!("Works/{WORK_REF}/Stories/ch1.md");
        std::fs::create_dir_all(
            fixture
                .workspace
                .path()
                .join(format!("works/{WORK_REF}/Stories")),
        )
        .expect("case-variant work dir");
        std::fs::create_dir_all(
            fixture
                .workspace
                .path()
                .join(format!("Works/{WORK_REF}/Stories")),
        )
        .expect("exact work dir");
        std::fs::write(fixture.workspace.path().join(&case_variant), "chapter body")
            .expect("seed case variant");
        std::fs::write(fixture.workspace.path().join(&exact), "chapter body")
            .expect("seed exact spelling");

        let write_error =
            execute_write_file(&write_request(&case_variant, "rewritten"), &fixture.context)
                .await
                .expect_err("a case-variant write refuses");
        assert_case_variant_works_refusal(write_error, &case_variant);
        assert_eq!(holder(&fixture.pool).await, None, "no lock was taken");
        assert_eq!(
            std::fs::read_to_string(fixture.workspace.path().join(&case_variant)).expect("read"),
            "chapter body",
            "the refused write left the existing artifact untouched"
        );

        let read_error = execute_read_file(&read_request(&case_variant), &fixture.context)
            .await
            .expect_err("a case-variant read refuses");
        assert_case_variant_works_refusal(read_error, &case_variant);

        let missing = format!("works/{WORK_REF}/Stories/missing.md");
        let missing_error = execute_read_file(&read_request(&missing), &fixture.context)
            .await
            .expect_err("a missing case-variant read is still refused");
        assert_case_variant_works_refusal(missing_error, &missing);

        let read = execute_read_file(&read_request(&exact), &fixture.context)
            .await
            .expect("the exact spelling reads");
        assert_eq!(read, serde_json::json!({ "content": "chapter body" }));

        let exact_missing = format!("Works/{WORK_REF}/Stories/absent.md");
        let exact_missing_error =
            execute_read_file(&read_request(&exact_missing), &fixture.context)
                .await
                .expect_err("a missing exact-spelling artifact is not found");
        assert!(
            matches!(exact_missing_error, NexusApiError::NotFound { .. }),
            "{exact_missing}: {exact_missing_error:?}"
        );
    }

    #[tokio::test]
    async fn fs_work_escape_is_refused_identically_on_write_and_read() {
        let fixture = fixture(true).await;
        let escaped = format!("Works/{WORK_REF}/../works/{WORK_REF}/x.md");
        let expected = format!("escapes the Works/{WORK_REF}/ boundary");

        let write_error = execute_write_file(&write_request(&escaped, "body"), &fixture.context)
            .await
            .expect_err("the escaped write refuses");
        let read_error = execute_read_file(&read_request(&escaped), &fixture.context)
            .await
            .expect_err("the escaped read refuses");

        for error in [write_error, read_error] {
            match error {
                NexusApiError::Forbidden { resource } => {
                    assert!(resource.contains(&expected), "{escaped}: {resource}");
                }
                other => panic!("expected Forbidden for {escaped}, got {other:?}"),
            }
        }
        assert_eq!(holder(&fixture.pool).await, None, "no lock was taken");
        assert!(!fixture
            .workspace
            .path()
            .join(format!("works/{WORK_REF}/x.md"))
            .exists());
    }

    #[tokio::test]
    async fn fs_case_variant_spelling_refusal_outranks_a_symlinked_target() {
        let fixture = fixture(true).await;
        let outside = tempfile::tempdir().expect("outside root");
        std::fs::create_dir_all(outside.path().join(WORK_REF)).expect("outside work dir");
        std::fs::write(
            outside.path().join(format!("{WORK_REF}/x.md")),
            "outside body",
        )
        .expect("outside artifact");
        std::os::unix::fs::symlink(outside.path(), fixture.workspace.path().join("works"))
            .expect("case-variant symlink");

        let case_variant = format!("works/{WORK_REF}/x.md");
        let write_error =
            execute_write_file(&write_request(&case_variant, "rewritten"), &fixture.context)
                .await
                .expect_err("a case-variant write refuses before the guard");
        let read_error = execute_read_file(&read_request(&case_variant), &fixture.context)
            .await
            .expect_err("a case-variant read refuses before the guard");

        assert_case_variant_works_refusal(write_error, &case_variant);
        assert_case_variant_works_refusal(read_error, &case_variant);
        assert_eq!(holder(&fixture.pool).await, None, "no lock was taken");
        assert_eq!(
            std::fs::read_to_string(outside.path().join(format!("{WORK_REF}/x.md")))
                .expect("outside artifact read"),
            "outside body",
            "the refused write stayed inside the refusal"
        );
    }

    #[tokio::test]
    async fn execute_write_file_keeps_the_outside_workspace_refusal() {
        let fixture = fixture(true).await;
        let absolute_outside = fixture
            .workspace
            .path()
            .parent()
            .expect("parent")
            .join("outside.md");

        for path in [
            "../outside.md".to_string(),
            absolute_outside.display().to_string(),
        ] {
            let error = execute_write_file(&write_request(&path, "body"), &fixture.context)
                .await
                .expect_err("a path outside the workspace refuses");

            match error {
                NexusApiError::Forbidden { resource } => {
                    assert!(
                        resource.contains("is outside the workspace root"),
                        "{path}: {resource}"
                    );
                }
                other => panic!("expected Forbidden for {path}, got {other:?}"),
            }
        }
    }

    #[tokio::test]
    async fn execute_write_file_keeps_the_guarded_write_for_non_work_paths() {
        let fixture = fixture(true).await;

        let result = execute_write_file(
            &write_request("notes/plain.md", "plain body"),
            &fixture.context,
        )
        .await
        .expect("a non-Work path writes");

        assert_eq!(result, serde_json::json!({ "written": true }));
        let target = fixture.workspace.path().join("notes/plain.md");
        assert_eq!(
            std::fs::read_to_string(&target).expect("read"),
            "plain body"
        );
    }

    #[tokio::test]
    async fn execute_read_file_reports_a_missing_path_as_not_found() {
        let fixture = fixture(true).await;

        let error = execute_read_file(&read_request("notes/absent.md"), &fixture.context)
            .await
            .expect_err("a missing path is not found");

        assert!(
            matches!(error, NexusApiError::NotFound { .. }),
            "real absence stays NotFound: {error:?}"
        );
    }

    // Unix-only: chmod-000 is the construction that makes canonicalize fail
    // with PermissionDenied; Windows access control has no portable
    // equivalent here, so the test skips rather than fails there.
    #[cfg(unix)]
    #[tokio::test]
    async fn execute_read_file_reports_a_permission_denial_as_forbidden_not_not_found() {
        use std::os::unix::fs::PermissionsExt;

        let fixture = fixture(true).await;
        let locked = fixture.workspace.path().join("locked");
        std::fs::create_dir_all(&locked).expect("locked dir");
        std::fs::write(locked.join("secret.md"), "secret body").expect("seed secret");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000))
            .expect("chmod 000");

        // Probe before asserting: on a root / privileged-container runner a
        // read below a chmod-000 directory succeeds, so there is no denial to
        // classify — skip rather than fail the suite there.
        let probe = std::fs::read(locked.join("secret.md"));
        let denied = matches!(
            probe.as_ref().map_err(std::io::Error::kind),
            Err(std::io::ErrorKind::PermissionDenied)
        );

        let outcome = if denied {
            Some(
                execute_read_file(&read_request("locked/secret.md"), &fixture.context)
                    .await
                    .expect_err("an unreadable path refuses"),
            )
        } else {
            eprintln!(
                "skipping permission-denial assertion: chmod 000 does not deny reads \
                 on this runner (root or privileged container)"
            );
            None
        };

        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700))
            .expect("restore perms");

        if let Some(error) = outcome {
            assert!(
                matches!(error, NexusApiError::Forbidden { .. }),
                "a permission denial is truthful, not a false absence: {error:?}"
            );
        }
    }

    // Unix-only: needs a symlink to place an ancestor outside the root.
    #[cfg(unix)]
    #[tokio::test]
    async fn execute_read_file_outside_workspace_unresolvable_path_is_non_revealing() {
        use std::os::unix::fs::PermissionsExt;

        let fixture = fixture(true).await;
        let outside = tempfile::tempdir().expect("outside root");
        std::fs::write(outside.path().join("hidden.md"), "outside body").expect("seed outside");
        std::os::unix::fs::symlink(outside.path(), fixture.workspace.path().join("escape"))
            .expect("escape symlink");

        // Both a missing target and (via chmod) a permission-denied lookup
        // below the escaping ancestor must collapse to the same non-revealing
        // Forbidden shape — no existence metadata leaks outside the root.
        std::fs::set_permissions(outside.path(), std::fs::Permissions::from_mode(0o000))
            .expect("chmod 000 outside");
        let denied_probe = matches!(
            std::fs::read(outside.path().join("hidden.md"))
                .as_ref()
                .map_err(std::io::Error::kind),
            Err(std::io::ErrorKind::PermissionDenied)
        );
        std::fs::set_permissions(outside.path(), std::fs::Permissions::from_mode(0o700))
            .expect("restore outside perms");

        let missing_error = execute_read_file(&read_request("escape/absent.md"), &fixture.context)
            .await
            .expect_err("a missing path below the escape refuses");
        assert!(
            matches!(missing_error, NexusApiError::Forbidden { .. }),
            "outside absence is non-revealing, never NotFound: {missing_error:?}"
        );

        if denied_probe {
            std::fs::set_permissions(outside.path(), std::fs::Permissions::from_mode(0o000))
                .expect("chmod 000 outside");
            let denied_error =
                execute_read_file(&read_request("escape/hidden.md"), &fixture.context)
                    .await
                    .expect_err("an unreadable path below the escape refuses");
            std::fs::set_permissions(outside.path(), std::fs::Permissions::from_mode(0o700))
                .expect("restore outside perms");
            match (&missing_error, &denied_error) {
                (
                    NexusApiError::Forbidden { resource: missing },
                    NexusApiError::Forbidden { resource: denied },
                ) => {
                    for resource in [missing, denied] {
                        assert!(
                            !resource.contains("does not exist")
                                && !resource.contains("permission denied"),
                            "no existence metadata leaks: {resource}"
                        );
                    }
                    assert!(
                        denied.replace("hidden.md", "absent.md") == *missing,
                        "absence and permission are indistinguishable: {missing} vs {denied}"
                    );
                }
                _ => panic!("both outcomes must be Forbidden"),
            }
        } else {
            eprintln!(
                "skipping outside-permission probe: chmod 000 does not deny reads \
                 on this runner (root or privileged container)"
            );
        }
    }

    #[tokio::test]
    async fn execute_read_file_reports_not_a_directory_as_invalid_input_not_not_found() {
        let fixture = fixture(true).await;
        std::fs::write(fixture.workspace.path().join("plain.md"), "plain body")
            .expect("seed plain file");

        // canonicalize fails with NotADirectory: the parent component is a
        // file — a malformed request, so the boundary renders a 400-class
        // InvalidInput, never a false absence.
        let error = execute_read_file(&read_request("plain.md/child.md"), &fixture.context)
            .await
            .expect_err("a file-as-parent path refuses");

        assert!(
            matches!(error, NexusApiError::InvalidInput { .. }),
            "a malformed path is a request error, not a false absence: {error:?}"
        );
    }

    #[test]
    fn read_guard_error_classification_is_assertable_on_any_runner() {
        let unresolvable = |kind: &str| NexusApiError::InvalidInput {
            field: format!("chapter_path_unresolvable:{kind}"),
            reason: "synthetic".into(),
        };

        // The chmod-independent seam: the suffix→variant contract is asserted
        // directly, so a privileged CI worker still fails on a regression.
        assert!(
            matches!(
                read_guard_error(unresolvable("permission_denied"), "x"),
                NexusApiError::Forbidden { .. }
            ),
            "permission_denied classifies as Forbidden"
        );
        assert!(
            matches!(
                read_guard_error(unresolvable("not_found"), "x"),
                NexusApiError::NotFound { .. }
            ),
            "not_found classifies as NotFound"
        );
        assert!(
            matches!(
                read_guard_error(unresolvable("not_a_directory"), "x"),
                NexusApiError::InvalidInput { .. }
            ),
            "not_a_directory classifies as InvalidInput"
        );
        assert!(
            matches!(
                read_guard_error(unresolvable("other"), "x"),
                NexusApiError::Internal { .. }
            ),
            "other kinds fall back to the quiet Internal shape"
        );
        let legacy = NexusApiError::InvalidInput {
            field: "chapter_path_unresolvable".into(),
            reason: "legacy".into(),
        };
        assert!(
            matches!(
                read_guard_error(legacy, "x"),
                NexusApiError::NotFound { .. }
            ),
            "the exact legacy field keeps its historical NotFound mapping"
        );

        // A workspace-root failure is never a file-level verdict: it must
        // not claim the requested file is absent or outside the workspace.
        let root_error =
            read_guard_error(unresolvable(crate::content::ROOT_UNRESOLVABLE_CODE), "x");
        assert!(
            !matches!(root_error, NexusApiError::NotFound { .. })
                && !matches!(root_error, NexusApiError::Forbidden { .. }),
            "root_unresolvable is neither NotFound nor the escape Forbidden: {root_error:?}"
        );
        match &root_error {
            NexusApiError::Internal { category } => {
                assert!(
                    !category.contains("file") && !category.contains("outside"),
                    "the root category is not file-level: {category}"
                );
            }
            other => panic!("expected Internal for root_unresolvable, got {other:?}"),
        }
    }

    #[test]
    fn manuscript_guard_error_strips_the_kind_suffix_to_the_legacy_field() {
        for kind in ["not_found", "permission_denied", "other"] {
            let error = manuscript_guard_error(NexusApiError::InvalidInput {
                field: format!("chapter_path_unresolvable:{kind}"),
                reason: "cannot resolve chapter path: io error".into(),
            });
            match error {
                NexusApiError::InvalidInput { field, reason } => {
                    assert_eq!(
                        field, "chapter_path_unresolvable",
                        "{kind}: the suffix never reaches the wire"
                    );
                    assert!(
                        reason.contains("cannot resolve chapter path"),
                        "{kind}: the human reason survives: {reason}"
                    );
                }
                other => panic!("{kind}: expected InvalidInput, got {other:?}"),
            }
        }

        // The exact legacy field passes through untouched, and so do
        // unrelated errors.
        let legacy = manuscript_guard_error(NexusApiError::InvalidInput {
            field: "chapter_path_unresolvable".into(),
            reason: "legacy".into(),
        });
        assert!(
            matches!(legacy, NexusApiError::InvalidInput { ref field, .. } if field == "chapter_path_unresolvable"),
            "exact legacy field is unchanged: {legacy:?}"
        );
        let forbidden = manuscript_guard_error(NexusApiError::InvalidInput {
            field: "chapter_path_forbidden".into(),
            reason: "escapes workspace root".into(),
        });
        assert!(
            matches!(forbidden, NexusApiError::InvalidInput { ref field, .. } if field == "chapter_path_forbidden"),
            "forbidden is unchanged: {forbidden:?}"
        );
    }

    #[tokio::test]
    async fn execute_manuscript_read_range_missing_body_stays_non_not_found() {
        let fixture = fixture(true).await;
        nexus_local_db::work_chapters::seed_chapters(
            &fixture.pool,
            WORK_ID,
            WORK_REF,
            1,
            "2026-06-04T10:00:00Z",
        )
        .await
        .expect("seed chapters");
        // The seeded row names a body_path whose file was never written:
        // the read must surface a truthful failure, never a chapter absence.
        let request = ToolsApi {
            caller_kind: None,
            parameters: serde_json::json!({
                "work_id": WORK_ID,
                "chapter": 1,
                "start_line": 1,
                "end_line": 5,
            }),
            request_id: None,
            session_id: None,
            tool_name: "nexus.manuscript.read_range".to_string(),
        };

        let error = execute_manuscript_read_range(&request, &fixture.context, CREATOR)
            .await
            .expect_err("a missing body file refuses");

        assert!(
            !matches!(error, NexusApiError::NotFound { .. }),
            "a missing body is never reported as a missing chapter: {error:?}"
        );
        if let NexusApiError::InvalidInput { ref field, .. } = error {
            assert!(
                !field.starts_with("chapter_path_unresolvable:"),
                "the internal kind suffix never reaches the wire: {field}"
            );
        }
    }
}
