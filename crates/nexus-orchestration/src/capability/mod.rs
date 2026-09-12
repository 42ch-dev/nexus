//! Capability trait + registry.
//!
//! Design: `.mstar/specs/orchestration-engine.md` §5.1–5.2.

pub mod admission;
pub mod builtins;
pub mod scan;
#[cfg(test)]
pub(crate) mod test_support;
pub mod user_capability;
pub mod watch;

use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors produced by capability execution.
#[derive(Error, Debug)]
pub enum CapabilityError {
    #[error("invalid input: {0}")]
    InputInvalid(String),
    #[error("transient external error: {0}")]
    TransientExternal(String),
    #[error("permanent external error: {0}")]
    PermanentExternal(String),
    #[error("worker unavailable")]
    WorkerUnavailable,
    #[error("ACP session lost")]
    AcpSessionLost,
    #[error("cancelled")]
    Cancelled,
    #[error("run cancellation unavailable: {0}")]
    CancellationUnavailable(String),
    #[error("forbidden: {0}")]
    Forbidden(String),
    #[error("internal error: {0}")]
    Internal(String),
}

/// Resolve a run's coordinator cancellation token from the shared per-run
/// map (A1).
///
/// **Fail-closed**: a run with no registered token returns
/// [`CapabilityError::CancellationUnavailable`]. Callers MUST never mint a
/// fresh token — a token that no coordinator knows about can never be
/// cancelled, so the prompt would execute without a cancellation path.
/// Registration happens at run admission (engine start/spawn/recovery paths);
/// the future P2 coordinator shares this map and the per-run admission lock.
///
/// # Errors
/// Returns [`CapabilityError::CancellationUnavailable`] when `session_id` has
/// no registered coordinator token, or [`CapabilityError::Internal`] when the
/// shared map lock is poisoned.
#[allow(clippy::implicit_hasher)] // DefaultHasher session-cancel map; hashing is not hot on this lookup path
pub fn resolve_session_cancellation(
    session_cancels: &std::sync::RwLock<
        std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
    >,
    session_id: &str,
) -> Result<tokio_util::sync::CancellationToken, CapabilityError> {
    let map = session_cancels
        .read()
        .map_err(|e| CapabilityError::Internal(format!("session cancels lock: {e}")))?;
    map.get(session_id).cloned().ok_or_else(|| {
        CapabilityError::CancellationUnavailable(format!(
            "no coordinator cancellation token registered for run '{session_id}'"
        ))
    })
}

// ---------------------------------------------------------------------------
// PromptExecutor — injected production prompt seam (A1)
// ---------------------------------------------------------------------------

/// Tool permission policy for a prompt operation (A1).
///
/// Orchestration maps its existing policy to a narrowing [`PromptPermissionScope`]
/// that the Host intersects with its own configuration per operation. The scope
/// can narrow but never elevate Host configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolPolicy {
    /// All tools auto-granted (V1.0 behavior).
    AutoGrantAll,
    /// Reads allowed, writes require upcall.
    AutoGrantReadOnly,
    /// No tools allowed.
    DenyAll,
    /// Every tool triggers upcall.
    RequestPolicy,
}

impl std::str::FromStr for ToolPolicy {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "auto_grant_all" => Ok(Self::AutoGrantAll),
            "deny_all" => Ok(Self::DenyAll),
            "request_policy" => Ok(Self::RequestPolicy),
            _ => Ok(Self::AutoGrantReadOnly), // safe default
        }
    }
}

impl ToolPolicy {
    /// Serialize to the string form used in IPC.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::AutoGrantAll => "auto_grant_all",
            Self::AutoGrantReadOnly => "auto_grant_read_only",
            Self::DenyAll => "deny_all",
            Self::RequestPolicy => "request_policy",
        }
    }

    /// Map this orchestration policy to the narrowing Host permission scope (A1).
    ///
    /// The scope is intersected with Host configuration per operation; it can
    /// narrow but never elevate. `None` preserves the standalone Host/Character
    /// policy and never means unrestricted.
    #[must_use]
    pub const fn permission_scope(&self) -> Option<PromptPermissionScope> {
        match self {
            Self::AutoGrantAll => Some(PromptPermissionScope {
                allow_read: true,
                allow_write: true,
                allow_destructive: true,
            }),
            Self::AutoGrantReadOnly => Some(PromptPermissionScope {
                allow_read: true,
                allow_write: false,
                allow_destructive: false,
            }),
            Self::DenyAll => Some(PromptPermissionScope {
                allow_read: false,
                allow_write: false,
                allow_destructive: false,
            }),
            Self::RequestPolicy => None,
        }
    }
}

/// Narrowing permission scope for a Host prompt operation (A1).
///
/// Intersected with Host configuration per operation; a provider unable to
/// honor a requested scope refuses `not_supported` rather than silently
/// discarding the policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PromptPermissionScope {
    /// Read-only tools allowed.
    pub allow_read: bool,
    /// Write tools allowed.
    pub allow_write: bool,
    /// Destructive tools allowed.
    pub allow_destructive: bool,
}

/// Typed prompt request (A1).
#[derive(Debug, Clone)]
pub struct PromptRequest {
    /// Trusted run identity.
    pub run_id: String,
    /// Task id the prompt belongs to.
    pub task_id: String,
    /// Optional agent role reference.
    pub agent_ref: Option<String>,
    /// The prompt text.
    pub prompt: String,
    /// Tool policy for this prompt.
    pub tool_policy: ToolPolicy,
    /// Coordinator cancellation token.
    pub cancellation: tokio_util::sync::CancellationToken,
}

/// Typed prompt result (A1).
///
/// Non-EndTurn stop reasons are errors, never success fields.
#[derive(Debug, Clone)]
pub struct PromptResult {
    /// Full message text collected from `MessageDelta` events.
    pub full_text: String,
    /// Host session id the prompt executed on.
    pub host_session_id: String,
    /// Host operation id that produced the result.
    pub operation_id: String,
}

/// Provider trait for production prompt execution (A1).
///
/// Implemented by the daemon-owned `HostPromptExecutor` over the existing
/// `HostFacade`. `nexus-orchestration` stays Host-agnostic: SDK/provider
/// types never enter graph tasks. When absent, capabilities operate in
/// standalone/test mode with clear error messages.
#[async_trait]
pub trait PromptExecutor: Send + Sync {
    /// Execute one prompt through the Host plane.
    ///
    /// Returns the typed result on success, or a [`CapabilityError`] on
    /// refusal/EOF/denial/timeout/cancellation — never a partial-output
    /// success.
    async fn execute(&self, request: PromptRequest) -> Result<PromptResult, CapabilityError>;

    /// Finalize a run's owned Host sessions after the run reached a
    /// confirmed terminal state (A5). Waits (bounded) for any in-flight
    /// operation's cleanup to complete, then shuts down and evicts every
    /// `(run, role)` session. Returns `Ok(())` only when cleanup is
    /// confirmed; `Err` means cleanup-unconfirmed (the run must remain
    /// non-terminal/actionable). Default no-op for non-Host executors.
    async fn finalize_run(&self, _run_id: &str) -> Result<(), CapabilityError> {
        Ok(())
    }
}

/// Provider trait for daemon-side `nexus.*` tool dispatch (DF-47 production wiring).
///
/// Implemented by `nexus-daemon-runtime`'s `DaemonToolDispatchAdapter`, which
/// dispatches through `HostToolExecutor::dispatch_for_schedule` (the Schedule
/// caller lane with `HostToolCallerKind::Schedule` audit differentiation).
/// Injected into `HostToolCallTask` so the orchestration engine can invoke
/// `nexus.*` tools on a schedule tick in-process.
///
/// Design: `agent-nexus-tool-bridge.md` §7.4, V1.42 P3.
#[async_trait]
pub trait DaemonToolDispatch: Send + Sync {
    /// Dispatch a `nexus.*` tool call through the daemon's unified registry.
    ///
    /// Returns the tool result JSON on success, or a `CapabilityError` on failure.
    async fn dispatch_tool(
        &self,
        tool_name: &str,
        args: &serde_json::Value,
        request_id: &str,
    ) -> Result<serde_json::Value, CapabilityError>;
}


/// Provider trait for production workspace open/commit (v1.188 P3).
#[async_trait]
pub trait WorkspaceExecutor: Send + Sync {
    async fn open(
        &self,
        input: nexus_contracts::local::orchestration::WorkspaceOpenInput,
    ) -> Result<nexus_contracts::local::orchestration::WorkspaceOpenOutput, CapabilityError>;

    async fn commit(
        &self,
        input: nexus_contracts::local::orchestration::WorkspaceCommitInput,
    ) -> Result<nexus_contracts::local::orchestration::WorkspaceCommitOutput, CapabilityError>;
}

/// Provider for live workspace session state (v1.188 P3).
///
/// Preset conditional edges may reference `_context.workspace.<field>`. The
/// graph task resolves that object from this provider at
/// expression-evaluation time, so the branch sees the REAL durable workspace
/// state owned by the daemon's workspace authority — never a synthetic
/// placeholder. `None` (no provider) leaves `_context.workspace` absent.
#[async_trait]
pub trait WorkspaceStateProvider: Send + Sync {
    /// Latest durable workspace state, or `None` when the workspace has no
    /// committed session yet.
    async fn workspace_state(&self) -> Option<serde_json::Value>;
}

/// Runtime dependencies injected through `CapabilityRegistry::with_runtime_deps`.
///
/// Groups pool and prompt executor so daemon boot can construct a single
/// struct and pass it to the registry factory.
///
/// `Clone` (V1.176 P1, AR-92 #3): the hot-reload watcher retains a cloned
/// copy from boot to rebuild fresh registries on the same admission path.
#[derive(Clone)]
pub struct CapabilityRuntimeDeps {
    /// Pool for pool-backed capabilities (`kb.extract_work`, etc.).
    pub pool: Option<sqlx::SqlitePool>,
    /// Production prompt executor for LLM-backed capabilities (`judge.llm`,
    /// `context.summarize`, `acp.prompt`, `nexus.llm.extract`).
    pub prompt_executor: Option<std::sync::Arc<dyn PromptExecutor>>,
    /// Per-run coordinator cancellation tokens (A1) — the executor listens to
    /// the token for the current run concurrently with the Host stream.
    pub session_cancels: std::sync::Arc<
        std::sync::RwLock<std::collections::HashMap<String, tokio_util::sync::CancellationToken>>,
    >,
    /// Daemon-side tool dispatch for `nexus.*` tools (DF-47, V1.42 P3).
    pub daemon_tool_dispatch: Option<std::sync::Arc<dyn DaemonToolDispatch>>,
    /// CDN fetch config for `registry.refresh` (V1.57 P1 — constructor-injected).
    pub cdn_config: Option<builtins::CdnConfig>,
    /// Production workspace executor for `workspace.open` / `workspace.commit`.
    pub workspace_executor: Option<std::sync::Arc<dyn WorkspaceExecutor>>,
}



// ---------------------------------------------------------------------------
// Capability trait
// ---------------------------------------------------------------------------

/// Provenance of a capability (AR-40).
///
/// Marker-only: `Builtin` is the default (zero edits to the ~34 builtin
/// impls); `User` marks a locally-installed user capability. The enum stays
/// in `nexus-orchestration` and NEVER crosses into `nexus-contracts`
/// (dependency direction, AR-40) — the wire layer maps it to the
/// `"builtin"` / `"user"` string enum itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityOrigin {
    /// Shipped with the engine (default).
    Builtin,
    /// Installed by a developer at `~/.nexus42/capabilities/<name>/`.
    User,
}

/// A capability that can be invoked as a graph-flow Task node.
///
/// Per the design spec, every capability ships its own input/output JSON Schema
/// as `&'static str` constants. These are **local** types, not wire contracts.
#[async_trait]
pub trait Capability: Send + Sync {
    /// Dot-separated capability name, e.g. `"sync.pull"`.
    fn name(&self) -> &'static str;

    /// JSON Schema (draft 2020-12) describing valid inputs.
    fn input_schema(&self) -> &'static str;

    /// JSON Schema (draft 2020-12) describing the output shape.
    fn output_schema(&self) -> &'static str;

    /// Provenance marker (AR-40). Defaults to [`CapabilityOrigin::Builtin`] —
    /// only user-installed capabilities override it.
    fn origin(&self) -> CapabilityOrigin {
        CapabilityOrigin::Builtin
    }

    /// Execute the capability with the given input.
    ///
    /// Returns a JSON `Value` on success or a [`CapabilityError`].
    async fn run(&self, input: Value) -> Result<Value, CapabilityError>;
}

// ---------------------------------------------------------------------------
// CapabilityRegistry
// ---------------------------------------------------------------------------

/// Registry of available capabilities.
///
/// Built at daemon startup and **rebuilt as a fresh instance** on
/// user-capability hot reload (V1.176 P1, AR-92): the live generation is
/// held by the shared [`CapabilityRegistryHolder`] and the watcher swaps in
/// a rebuilt registry on scan-dir changes. An instance is immutable after
/// construction.
///
/// Capabilities are stored in a `Vec` for ordered iteration, with a `HashMap`
/// index for O(1) lookup by name. The index is built eagerly by every
/// constructor and append path — never lazily.
pub struct CapabilityRegistry {
    capabilities: Vec<Box<dyn Capability>>,
    /// Eager index: `name` → position in `capabilities`. Built by every
    /// constructor and append path (never on-demand).
    index: Option<std::collections::HashMap<&'static str, usize>>,
}

impl CapabilityRegistry {
    /// Create a registry pre-populated with all built-in capabilities.
    ///
    /// Built-ins: `sync.pull`, `sync.push`, `outbox.flush`, `outbox.compact`,
    /// `workspace.open`, `workspace.commit`, `registry.refresh`,
    /// `creator.read_memory`, `creator.write_memory`, `creator.inject_prompt`,
    /// `creator.write_brief`, `judge.rule`, `acp.prompt`, `judge.llm`,
    /// `context.summarize`, `kb.extract_work`,
    /// `nexus.llm.extract`, `soul.experience.aggregate`,
    /// `narrative.compute`.
    ///
    /// `kb.extract_work` is created without a pool (placeholder mode).
    /// Use [`with_builtins_and_pool`] for full e2e support.
    #[must_use]
    pub fn with_builtins() -> Self {
        let caps: Vec<Box<dyn Capability>> = vec![
            Box::new(builtins::SyncPull),
            Box::new(builtins::SyncPush),
            Box::new(builtins::OutboxFlush::new()),
            Box::new(builtins::OutboxCompact::new()),
            Box::new(builtins::WorkspaceOpen::new()),
            Box::new(builtins::WorkspaceCommit::new()),
            Box::new(builtins::RegistryRefresh::new()),
            Box::new(builtins::CreatorReadMemory::new()),
            Box::new(builtins::CreatorWriteMemory::new()),
            Box::new(builtins::CreatorInjectPrompt::new()),
            Box::new(builtins::CreatorWriteBrief::new()),
            Box::new(builtins::JudgeRule),
            Box::new(builtins::AcpPrompt::new()),
            Box::new(builtins::JudgeLlm::new()),
            Box::new(builtins::ContextSummarize::new()),
            Box::new(builtins::KbExtractWork::new()),
            Box::new(builtins::LlmExtract::new()),
            Box::new(builtins::SoulExperienceAggregate),
            // F6 (C-001): register novel.project_scaffold in the
            // pool-less registry so embedded preset validation can
            // resolve it. The pool-bound variant is registered via
            // [`with_builtins_and_pool`] for runtime use.
            Box::new(builtins::NovelProjectScaffold::new()),
            // P3 (T3): register novel.chapter_transition for chapter
            // status transitions (DB + frontmatter).
            Box::new(builtins::NovelChapterTransition::new()),
            // V1.52 T-A P2: register essay.project_scaffold for
            // embedded preset validation.
            Box::new(builtins::EssayProjectScaffold::new()),
            // V1.63 P2: register essay.draft_status.finalize for
            // essay-writing preset finalize_commit state.
            Box::new(builtins::EssayDraftStatusFinalize::new()),
            // V1.54 P1: register game_bible.project_scaffold for
            // embedded preset validation.
            Box::new(builtins::GameBibleProjectScaffold::new()),
            // V1.55 P3: register script.project_scaffold for
            // embedded preset validation.
            Box::new(builtins::ScriptProjectScaffold::new()),
            // V1.56 P-last R-V155P2-F002: game_bible.section_status.update
            Box::new(builtins::GameBibleSectionStatusUpdate::new()),
            // V1.67 P2 (R-V160P1-QC1-W001): script.section_status.update
            Box::new(builtins::ScriptSectionStatusUpdate::new()),
            // V1.58 P1: nexus.reference.refresh (pool-less; returns WorkerUnavailable)
            Box::new(builtins::ReferenceRefresh::new()),
            // V1.60 P0: DF-46 local parity — 5 orchestration-scope capabilities
            // (pool-less; return WorkerUnavailable without a pool).
            Box::new(builtins::WorldStateQuery::new()),
            Box::new(builtins::WorldDeltaPropose::new()),
            Box::new(builtins::WorldDeltaApply::new()),
            Box::new(builtins::TimelineEventAppend::new()),
            Box::new(builtins::ForkCreate::new()),
            // V1.61 P3: narrative.compute — sandboxed WASM compute for world state.
            Box::new(builtins::NarrativeCompute::new()),
        ];
        let mut reg = Self {
            capabilities: caps,
            index: None,
        };
        reg.build_index();
        reg
    }

    /// Create a registry with built-in capabilities and a pool.
    ///
    /// Same as [`with_builtins`] but `kb.extract_work` receives the pool
    /// for full e2e lifecycle, and all creator capabilities receive a
    /// [`builtins::CreatorCapabilityStore`] for real memory I/O
    /// and prompt injection persistence.
    #[must_use]
    pub fn with_builtins_and_pool(pool: sqlx::SqlitePool) -> Self {
        let creator_store = Arc::new(builtins::CreatorCapabilityStore::new(pool.clone()));
        let caps: Vec<Box<dyn Capability>> = vec![
            Box::new(builtins::SyncPull),
            Box::new(builtins::SyncPush),
            Box::new(builtins::OutboxFlush::with_pool(pool.clone())),
            Box::new(builtins::OutboxCompact::with_pool(pool.clone())),
            Box::new(builtins::WorkspaceOpen::new()),
            Box::new(builtins::WorkspaceCommit::new()),
            Box::new(builtins::RegistryRefresh::new()),
            Box::new(builtins::CreatorReadMemory::with_store(
                creator_store.clone(),
            )),
            Box::new(builtins::CreatorWriteMemory::with_store(
                creator_store.clone(),
            )),
            Box::new(builtins::CreatorInjectPrompt::with_store(
                creator_store.clone(),
            )),
            Box::new(builtins::CreatorWriteBrief::with_store(creator_store)),
            Box::new(builtins::JudgeRule),
            Box::new(builtins::AcpPrompt::new()),
            Box::new(builtins::JudgeLlm::new()),
            Box::new(builtins::ContextSummarize::new()),
            Box::new(builtins::KbExtractWork::with_pool(pool.clone())),
            Box::new(builtins::LlmExtract::new()),
            Box::new(builtins::SoulExperienceAggregate),
            Box::new(builtins::NovelProjectScaffold::with_pool(pool.clone())),
            Box::new(builtins::NovelChapterTransition::with_pool(pool.clone())),
            // V1.52 T-A P2: essay.project_scaffold with pool.
            Box::new(builtins::EssayProjectScaffold::with_pool(pool.clone())),
            // V1.63 P2: essay.draft_status.finalize (pool-less; FS-only).
            Box::new(builtins::EssayDraftStatusFinalize::new()),
            // V1.54 P1: game_bible.project_scaffold with pool.
            Box::new(builtins::GameBibleProjectScaffold::with_pool(pool.clone())),
            // V1.55 P3: script.project_scaffold with pool.
            Box::new(builtins::ScriptProjectScaffold::with_pool(pool.clone())),
            // V1.56 P-last R-V155P2-F002: game_bible.section_status.update
            Box::new(builtins::GameBibleSectionStatusUpdate::new()),
            // V1.67 P2 (R-V160P1-QC1-W001): script.section_status.update
            Box::new(builtins::ScriptSectionStatusUpdate::new()),
            // V1.58 P1: nexus.reference.refresh with pool
            Box::new(builtins::ReferenceRefresh::with_pool(pool.clone())),
            // V1.60 P0: DF-46 local parity — 5 orchestration-scope capabilities.
            Box::new(builtins::WorldStateQuery::with_pool(pool.clone())),
            Box::new(builtins::WorldDeltaPropose::with_pool(pool.clone())),
            Box::new(builtins::WorldDeltaApply::with_pool(pool.clone())),
            Box::new(builtins::TimelineEventAppend::with_pool(pool.clone())),
            Box::new(builtins::ForkCreate::with_pool(pool.clone())),
            // V1.61 P3: narrative.compute with pool.
            Box::new(builtins::NarrativeCompute::with_pool(pool)),
        ];
        let mut reg = Self {
            capabilities: caps,
            index: None,
        };
        reg.build_index();
        reg
    }

    /// Create a registry with runtime dependencies injected.
    ///
    /// Production daemon boot should use this constructor when both a pool
    /// and worker provider are available. Capabilities without runtime deps
    /// are constructed in their default (standalone) form.
    ///
    /// `narrative.compute` is constructed via [`builtins::NarrativeCompute::with_pool`],
    /// which builds its own `WasmEngine` + per-instance module cache. For the
    /// daemon-wide singleton engine + cache (P-last T1, closes
    /// R-V161P3-PERF-001), use [`CapabilityRegistry::with_runtime_deps_and_wasm`].
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn with_runtime_deps(deps: &CapabilityRuntimeDeps) -> Self {
        let narrative_compute = deps
            .pool
            .as_ref()
            .map_or_else(builtins::NarrativeCompute::new, |pool| {
                builtins::NarrativeCompute::with_pool(pool.clone())
            });
        Self::build_with_narrative_compute(deps, narrative_compute)
    }

    /// Create a registry with runtime dependencies **and** a daemon-wide
    /// singleton `WasmEngine` + `ModuleCache` injected into `narrative.compute`
    /// (P-last T1/T4 — closes R-V161P3-PERF-001/002).
    ///
    /// The daemon builds exactly one engine + one cache at boot (pre-warmed
    /// with embedded and user-installed modules) and passes them here so module
    /// compilation happens once process-wide and is reused by every compute
    /// invocation. When `deps.pool` is absent, `narrative.compute` falls back
    /// to its standalone (`WorkerUnavailable`) form.
    #[must_use]
    pub fn with_runtime_deps_and_wasm(
        deps: &CapabilityRuntimeDeps,
        engine: std::sync::Arc<nexus_wasm_host::WasmEngine>,
        module_cache: std::sync::Arc<nexus_wasm_host::ModuleCache>,
    ) -> Self {
        let narrative_compute =
            deps.pool
                .as_ref()
                .map_or_else(builtins::NarrativeCompute::new, |pool| {
                    builtins::NarrativeCompute::with_pool_and_engine(
                        pool.clone(),
                        engine,
                        module_cache,
                    )
                });
        Self::build_with_narrative_compute(deps, narrative_compute)
    }

    /// Create a registry with runtime dependencies, the daemon-wide singleton
    /// `WasmEngine` + `ModuleCache`, **and** user capabilities scanned from
    /// `scan_dir` (V1.172 P0, DR-10; AR-36).
    ///
    /// Builds the base via [`with_runtime_deps_and_wasm`], appends the
    /// admitted user capabilities **after** builtins, then rebuilds the eager
    /// name index. The scan is fail-safe by contract (AR-35): a missing
    /// directory or a bad descriptor never fails boot — bad entries land in
    /// `outcome.skipped` with named reasons (already `warn!`-logged).
    #[must_use]
    pub fn with_runtime_deps_and_wasm_and_user_caps(
        deps: &CapabilityRuntimeDeps,
        engine: std::sync::Arc<nexus_wasm_host::WasmEngine>,
        module_cache: std::sync::Arc<nexus_wasm_host::ModuleCache>,
        scan_dir: &std::path::Path,
    ) -> (Self, scan::ScanOutcome) {
        let engine_handle = engine.clone();
        let module_cache_handle = module_cache.clone();
        let mut reg = Self::with_runtime_deps_and_wasm(deps, engine, module_cache);
        let outcome =
            reg.append_user_caps(scan_dir, Some(&engine_handle), Some(&module_cache_handle));
        (reg, outcome)
    }

    /// Create a registry with runtime dependencies **and** user capabilities
    /// scanned from `scan_dir`, on the engine-less boot path (V1.172 P0,
    /// DR-10; AR-36/AR-44).
    ///
    /// Engine-less arm of the AR-36 pair: user capabilities still register
    /// (discoverable); their stub `run()` returns `WorkerUnavailable` until P1
    /// wires the executor.
    #[must_use]
    pub fn with_runtime_deps_and_user_caps(
        deps: &CapabilityRuntimeDeps,
        scan_dir: &std::path::Path,
    ) -> (Self, scan::ScanOutcome) {
        let mut reg = Self::with_runtime_deps(deps);
        let outcome = reg.append_user_caps(scan_dir, None, None);
        (reg, outcome)
    }

    /// Append admitted user capabilities after builtins and rebuild the eager
    /// index (AR-36). Shared by the two user-capability constructors.
    ///
    /// The engine arm passes the daemon-wide [`WasmEngine`] + [`ModuleCache`]
    /// (some/some) so each admitted capability's real executor (AR-37) can
    /// compile/run; the engine-less arm passes `None`/`None` (AR-44) so
    /// `run()` returns `WorkerUnavailable`. Handles are forwarded into the
    /// scan so `UserCapability::new` carries them from construction.
    ///
    /// Builtin collision (AR-43 gate 1): the scan admits against the
    /// registry's builtin name set (`self.capabilities` — the builtins built
    /// by the constructor), so a user capability whose name equals a builtin
    /// is **skipped inside the scan** (builtin wins — AR-36/AR-43). Each
    /// collision lands in `outcome.skipped` with the named `NameCollision`
    /// reason and is `warn!`-logged before the append.
    fn append_user_caps(
        &mut self,
        scan_dir: &std::path::Path,
        engine: Option<&std::sync::Arc<nexus_wasm_host::WasmEngine>>,
        module_cache: Option<&std::sync::Arc<nexus_wasm_host::ModuleCache>>,
    ) -> scan::ScanOutcome {
        let builtin_names: std::collections::HashSet<&str> =
            self.capabilities.iter().map(|c| c.name()).collect();
        let outcome = scan::scan_user_capabilities(scan_dir, &builtin_names, engine, module_cache);
        // V1.176 P1 (AR-92 #4): the outcome keeps its concrete admitted
        // entries (cloned into the registry) so the boot site can seed the
        // watcher's last-good mirror from them — the scan is the single
        // admission path for both boot and hot reload. The boot-site
        // aggregate log (`log_scan_outcome`) therefore reports the admitted
        // count it was documented to report.
        self.append_user_cap_entries(&outcome.admitted);
        outcome
    }

    /// Box and append `admitted` after builtins and rebuild the eager index —
    /// the single registry-append seam shared by the boot constructors
    /// ([`append_user_caps`]) and the hot-reload watcher
    /// (`rebuild_registry_with_merge`, V1.176 P1 M-3) so the boxing stays in
    /// one place.
    fn append_user_cap_entries(
        &mut self,
        admitted: &[crate::capability::user_capability::UserCapability],
    ) {
        for cap in admitted {
            self.capabilities
                .push(Box::new(cap.clone()) as Box<dyn Capability>);
        }
        self.build_index();
    }

    /// Shared body of [`with_runtime_deps`] / [`with_runtime_deps_and_wasm`],
    /// parameterized only by the `narrative.compute` instance to register.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    fn build_with_narrative_compute(
        deps: &CapabilityRuntimeDeps,
        narrative_compute: builtins::NarrativeCompute,
    ) -> Self {
        let kb = deps
            .pool
            .as_ref()
            .map_or_else(builtins::KbExtractWork::new, |pool| {
                builtins::KbExtractWork::with_pool(pool.clone())
            });

        let judge_llm =
            deps.prompt_executor
                .as_ref()
                .map_or_else(builtins::JudgeLlm::new, |executor| {
                    builtins::JudgeLlm::with_prompt_executor(executor.clone())
                        .with_session_cancels(deps.session_cancels.clone())
                });

        let context_summarize = deps.prompt_executor.as_ref().map_or_else(
            builtins::ContextSummarize::new,
            |executor| {
                builtins::ContextSummarize::with_prompt_executor(executor.clone())
                    .with_session_cancels(deps.session_cancels.clone())
            },
        );

        // V1.51 T-A P0: nexus.llm.extract reuses the same prompt executor as
        // judge.llm / context.summarize / acp.prompt (compass §0.1 #7).
        let llm_extract =
            deps.prompt_executor
                .as_ref()
                .map_or_else(builtins::LlmExtract::new, |executor| {
                    builtins::LlmExtract::with_prompt_executor(executor.clone())
                        .with_session_cancels(deps.session_cancels.clone())
                });

        let acp_prompt =
            deps.prompt_executor
                .as_ref()
                .map_or_else(builtins::AcpPrompt::new, |executor| {
                    builtins::AcpPrompt::with_prompt_executor(executor.clone())
                        .with_session_cancels(deps.session_cancels.clone())
                });

        // V1.57 P1: cdn_config is constructor-injected (no global state).
        let registry_refresh = deps
            .cdn_config
            .as_ref()
            .map_or_else(builtins::RegistryRefresh::new, |cdn| {
                builtins::RegistryRefresh::with_cdn(cdn.clone())
            });

        let creator_store = deps.pool.as_ref().map(|pool| {
            std::sync::Arc::new(builtins::CreatorCapabilityStore::from_arc(
                std::sync::Arc::new(pool.clone()),
            ))
        });

        let creator_read = creator_store
            .as_ref()
            .map_or_else(builtins::CreatorReadMemory::new, |store| {
                builtins::CreatorReadMemory::with_store(store.clone())
            });
        let creator_write = creator_store
            .as_ref()
            .map_or_else(builtins::CreatorWriteMemory::new, |store| {
                builtins::CreatorWriteMemory::with_store(store.clone())
            });
        let creator_inject = creator_store
            .as_ref()
            .map_or_else(builtins::CreatorInjectPrompt::new, |store| {
                builtins::CreatorInjectPrompt::with_store(store.clone())
            });
        let creator_write_brief = creator_store
            .as_ref()
            .map_or_else(builtins::CreatorWriteBrief::new, |store| {
                builtins::CreatorWriteBrief::with_store(store.clone())
            });

        let outbox_flush = deps
            .pool
            .as_ref()
            .map_or_else(builtins::OutboxFlush::new, |pool| {
                builtins::OutboxFlush::with_pool(pool.clone())
            });
        let outbox_compact = deps
            .pool
            .as_ref()
            .map_or_else(builtins::OutboxCompact::new, |pool| {
                builtins::OutboxCompact::with_pool(pool.clone())
            });

        let workspace_open = deps.workspace_executor.as_ref().map_or_else(
            builtins::WorkspaceOpen::new,
            |executor| builtins::WorkspaceOpen::with_workspace_executor(executor.clone()),
        );
        let workspace_commit = deps.workspace_executor.as_ref().map_or_else(
            builtins::WorkspaceCommit::new,
            |executor| builtins::WorkspaceCommit::with_workspace_executor(executor.clone()),
        );

        let caps: Vec<Box<dyn Capability>> = vec![
            Box::new(builtins::SyncPull),
            Box::new(builtins::SyncPush),
            Box::new(outbox_flush),
            Box::new(outbox_compact),
            Box::new(workspace_open),
            Box::new(workspace_commit),
            Box::new(registry_refresh),
            Box::new(creator_read),
            Box::new(creator_write),
            Box::new(creator_inject),
            Box::new(creator_write_brief),
            Box::new(builtins::JudgeRule),
            Box::new(acp_prompt),
            Box::new(judge_llm),
            Box::new(context_summarize),
            Box::new(kb),
            Box::new(llm_extract),
            Box::new(builtins::SoulExperienceAggregate),
            Box::new(
                deps.pool
                    .as_ref()
                    .map_or_else(builtins::NovelProjectScaffold::new, |pool| {
                        builtins::NovelProjectScaffold::with_pool(pool.clone())
                    }),
            ),
            Box::new(
                deps.pool
                    .as_ref()
                    .map_or_else(builtins::NovelChapterTransition::new, |pool| {
                        builtins::NovelChapterTransition::with_pool(pool.clone())
                    }),
            ),
            // V1.52 T-A P2: essay.project_scaffold with runtime deps.
            Box::new(
                deps.pool
                    .as_ref()
                    .map_or_else(builtins::EssayProjectScaffold::new, |pool| {
                        builtins::EssayProjectScaffold::with_pool(pool.clone())
                    }),
            ),
            // V1.54 P1: game_bible.project_scaffold with runtime deps.
            Box::new(
                deps.pool
                    .as_ref()
                    .map_or_else(builtins::GameBibleProjectScaffold::new, |pool| {
                        builtins::GameBibleProjectScaffold::with_pool(pool.clone())
                    }),
            ),
            // V1.55 P3: script.project_scaffold with runtime deps.
            Box::new(
                deps.pool
                    .as_ref()
                    .map_or_else(builtins::ScriptProjectScaffold::new, |pool| {
                        builtins::ScriptProjectScaffold::with_pool(pool.clone())
                    }),
            ),
            // V1.56 P-last R-V155P2-F002: game_bible.section_status.update
            Box::new(builtins::GameBibleSectionStatusUpdate::new()),
            // V1.67 P2 (R-V160P1-QC1-W001): script.section_status.update
            Box::new(builtins::ScriptSectionStatusUpdate::new()),
            // V1.58 P1: nexus.reference.refresh with pool from runtime deps
            Box::new(
                deps.pool
                    .as_ref()
                    .map_or_else(builtins::ReferenceRefresh::new, |pool| {
                        builtins::ReferenceRefresh::with_pool(pool.clone())
                    }),
            ),
            // V1.60 P0: DF-46 local parity — 5 orchestration-scope capabilities
            // (pool-conditional; pool-less returns WorkerUnavailable).
            Box::new(
                deps.pool
                    .as_ref()
                    .map_or_else(builtins::WorldStateQuery::new, |pool| {
                        builtins::WorldStateQuery::with_pool(pool.clone())
                    }),
            ),
            Box::new(
                deps.pool
                    .as_ref()
                    .map_or_else(builtins::WorldDeltaPropose::new, |pool| {
                        builtins::WorldDeltaPropose::with_pool(pool.clone())
                    }),
            ),
            Box::new(
                deps.pool
                    .as_ref()
                    .map_or_else(builtins::WorldDeltaApply::new, |pool| {
                        builtins::WorldDeltaApply::with_pool(pool.clone())
                    }),
            ),
            Box::new(
                deps.pool
                    .as_ref()
                    .map_or_else(builtins::TimelineEventAppend::new, |pool| {
                        builtins::TimelineEventAppend::with_pool(pool.clone())
                    }),
            ),
            Box::new(
                deps.pool
                    .as_ref()
                    .map_or_else(builtins::ForkCreate::new, |pool| {
                        builtins::ForkCreate::with_pool(pool.clone())
                    }),
            ),
            // V1.61 P3: narrative.compute — injected by the caller
            // (`with_runtime_deps` builds a per-instance engine + cache;
            // `with_runtime_deps_and_wasm` injects the daemon-wide singleton).
            Box::new(narrative_compute),
        ];
        let mut reg = Self {
            capabilities: caps,
            index: None,
        };
        reg.build_index();
        reg
    }

    /// Create an empty registry (for testing).
    #[must_use]
    pub fn empty() -> Self {
        let mut reg = Self {
            capabilities: Vec::new(),
            index: None,
        };
        reg.build_index();
        reg
    }

    /// Build the name-to-index `HashMap` for O(1) lookups.
    ///
    /// Called by every constructor and by the append seam
    /// ([`append_user_cap_entries`]) after `capabilities` is populated —
    /// the index is eager, never built on demand (M-4).
    fn build_index(&mut self) {
        let mut idx = std::collections::HashMap::with_capacity(self.capabilities.len());
        for (i, cap) in self.capabilities.iter().enumerate() {
            idx.insert(cap.name(), i);
        }
        self.index = Some(idx);
    }

    /// Look up a capability by its dot-separated name.
    ///
    /// Uses the pre-built `HashMap` index for O(1) amortized lookups.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&dyn Capability> {
        let idx = self.index.as_ref()?;
        let pos = idx.get(name)?;
        Some(self.capabilities[*pos].as_ref())
    }

    /// Iterate over all registered capabilities.
    pub fn iter(&self) -> impl Iterator<Item = &dyn Capability> {
        self.capabilities.iter().map(std::convert::AsRef::as_ref)
    }

    /// Return the number of registered capabilities.
    #[must_use]
    pub fn len(&self) -> usize {
        self.capabilities.len()
    }

    /// Return whether the registry is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.capabilities.is_empty()
    }
}

// ---------------------------------------------------------------------------
// CapabilityRegistryHolder — hot-reload swap seam (V1.176 P1, AR-92)
// ---------------------------------------------------------------------------

/// Shared holder for the live capability registry (AR-92).
///
/// A hot reload rebuilds a **fresh** [`CapabilityRegistry`] on the same
/// scan/admission path as boot and atomically swaps it into this holder —
/// never an in-place mutation of a live registry (races `iter()`/`get()`
/// borrowers) and never a second registry lane (single-spine invariant,
/// AR-92 #1/#8).
///
/// Lock/re-entrancy contract (AR-92 #7): readers clone the inner `Arc`
/// under the read lock and release immediately — no reader holds the lock
/// across an `.await`, and wasm execution never runs under the holder lock.
/// An in-flight dispatch that cloned the pre-swap `Arc` finishes against
/// last-good (no abort, no half-call — PL-9). The write lock is held only
/// for the pointer write; swaps are serialized by construction (one watcher
/// task, one tick at a time).
///
/// Boot creates exactly **one** holder and shares it with `WorkspaceState`,
/// the engine, and the watcher (AR-92 #2/#6).
#[derive(Clone, Default)]
pub struct CapabilityRegistryHolder {
    inner: std::sync::Arc<std::sync::RwLock<Option<std::sync::Arc<CapabilityRegistry>>>>,
}

impl CapabilityRegistryHolder {
    /// Create an empty holder. The boot registry is swapped in before any
    /// reader, the engine, or a graph build touches it.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Clone the current registry under the read lock, if one has been
    /// swapped in.
    #[must_use]
    pub fn get(&self) -> Option<std::sync::Arc<CapabilityRegistry>> {
        self.inner
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner) // poison-recovery (crate policy)
            .clone()
    }

    /// Atomically swap in a freshly rebuilt registry (write lock; held only
    /// for the pointer write — AR-92 #7). The previous generation is dropped
    /// AFTER the write lock is released so a last-reference drop (tearing
    /// down a full builtin+user registry) never stalls readers (M-1).
    pub fn swap(&self, registry: std::sync::Arc<CapabilityRegistry>) {
        let mut guard = self
            .inner
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let previous = (*guard).replace(registry);
        drop(guard);
        drop(previous);
    }

    /// Create a holder pre-populated with `registry` — boot convenience:
    /// the boot registry is swapped in before readers, the engine, and the
    /// watcher exist, and every later generation arrives via [`swap`](Self::swap).
    #[must_use]
    pub fn with_registry(registry: std::sync::Arc<CapabilityRegistry>) -> Self {
        let holder = Self::new();
        holder.swap(registry);
        holder
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::test_support::write_capability_dir;

    #[test]
    fn registry_has_33_builtins() {
        // V1.186 removed the obsolete `acp.session_load` placeholder from the prior 34.
        let reg = CapabilityRegistry::with_builtins();
        assert_eq!(reg.len(), 33);
    }

    #[test]
    fn registry_lookup_each_builtin() {
        let reg = CapabilityRegistry::with_builtins();
        for name in [
            "sync.pull",
            "sync.push",
            "outbox.flush",
            "outbox.compact",
            "workspace.open",
            "workspace.commit",
            "registry.refresh",
            "creator.read_memory",
            "creator.write_memory",
            "creator.inject_prompt",
            "creator.write_brief",
            "judge.rule",
            "acp.prompt",
            "judge.llm",
            "context.summarize",
            "kb.extract_work",
            "nexus.llm.extract",
            "soul.experience.aggregate",
            "novel.project_scaffold",
            "novel.chapter_transition",
            "essay.project_scaffold",
            "essay.draft_status.finalize",
            "game_bible.project_scaffold",
            "script.project_scaffold",
            "game_bible.section_status.update",
            "script.section_status.update",
            "nexus.reference.refresh",
            // V1.60 P0: DF-46 orchestration capabilities (full nexus.* names).
            "nexus.world.state.query",
            "nexus.world.delta.propose",
            "nexus.world.delta.apply",
            "nexus.timeline.event.append",
            "nexus.fork.create",
            "narrative.compute",
        ] {
            assert!(
                reg.get(name).is_some(),
                "expected builtin '{name}' to be registered"
            );
        }
    }

    #[test]
    fn registry_lookup_missing_returns_none() {
        let reg = CapabilityRegistry::with_builtins();
        assert!(reg.get("nonexistent").is_none());
    }

    #[test]
    fn builtin_capability_origin_defaults_to_builtin() {
        // AR-40: the default `origin()` is `Builtin` — zero edits to the ~34
        // builtin impls; a builtin capability must report `Builtin`.
        let reg = CapabilityRegistry::with_builtins();
        let cap = reg
            .get("sync.pull")
            .expect("sync.pull builtin must be registered");
        assert!(matches!(cap.origin(), super::CapabilityOrigin::Builtin));
    }

    #[tokio::test]
    async fn registry_iter_returns_all() {
        let reg = CapabilityRegistry::with_builtins();
        let names: Vec<&str> = reg.iter().map(super::Capability::name).collect();
        // V1.186 removed the obsolete `acp.session_load` placeholder from the prior 34.
        assert_eq!(names.len(), 33);
        assert!(names.contains(&"sync.pull"));
        assert!(names.contains(&"judge.rule"));
        assert!(names.contains(&"acp.prompt"));
        assert!(names.contains(&"judge.llm"));
        assert!(names.contains(&"context.summarize"));
        assert!(names.contains(&"kb.extract_work"));
        assert!(names.contains(&"nexus.llm.extract"));
        assert!(names.contains(&"soul.experience.aggregate"));
        assert!(names.contains(&"novel.project_scaffold"));
        assert!(names.contains(&"novel.chapter_transition"));
        // V1.60 P0 DF-46 orchestration capabilities.
        assert!(names.contains(&"nexus.world.state.query"));
        assert!(names.contains(&"nexus.fork.create"));
        // V1.61 P3 narrative.compute.
        assert!(names.contains(&"narrative.compute"));
        // V1.63 P2 essay.draft_status.finalize.
        assert!(names.contains(&"essay.draft_status.finalize"));
        // V1.67 P2 script.section_status.update.
        assert!(names.contains(&"script.section_status.update"));
    }

    // ── User capability constructors (T2 / AR-36) ───────────────────────
    // The trio fixture is the shared `test_support::write_capability_dir`
    // (qc1 S-1): one writer for the in-crate test modules.

    #[test]
    fn with_runtime_deps_and_user_caps_appends_after_builtins() {
        let tmp = tempfile::tempdir().unwrap();
        write_capability_dir(tmp.path(), "demo.pull");
        let deps = CapabilityRuntimeDeps {
            pool: None,
            prompt_executor: None,
            session_cancels: std::sync::Arc::new(std::sync::RwLock::new(
                std::collections::HashMap::new(),
            )),
            daemon_tool_dispatch: None,
            cdn_config: None,
        workspace_executor: None,
        };
        // Base builtin count of the runtime-deps constructor (33 — the shared
        // `build_with_narrative_compute` vec; `essay.draft_status.finalize`
        // is only in with_builtins/with_builtins_and_pool).
        let base = CapabilityRegistry::with_runtime_deps(&deps);
        let base_len = base.len();
        let (reg, outcome) = CapabilityRegistry::with_runtime_deps_and_user_caps(&deps, tmp.path());
        assert!(
            outcome.skipped.is_empty(),
            "no skips: {:?}",
            outcome.skipped
        );
        assert_eq!(reg.len(), base_len + 1, "base builtins + 1 user capability");
        // Builtins first, then the appended user capability (AR-36 order).
        let names: Vec<&str> = reg.iter().map(Capability::name).collect();
        let base_names: Vec<&str> = base.iter().map(Capability::name).collect();
        assert_eq!(
            &names[..base_len],
            &base_names[..],
            "prefix must equal the base registry's builtin order (S-005 tightened assertion)"
        );
        assert_eq!(names[base_len], "demo.pull");
        // Eager index rebuilt: lookup works for both.
        assert!(reg.get("narrative.compute").is_some());
        assert!(reg.get("demo.pull").is_some());
        // Registered through the index (not just iterated).
        let cap = reg.get("demo.pull").expect("user cap indexed");
        assert_eq!(cap.input_schema(), r#"{"type":"object"}"#);
    }

    #[test]
    fn user_cap_colliding_with_builtin_is_skipped_builtin_wins() {
        // P1 T4 (AR-43 gate 1): a user capability named like a builtin
        // (here `sync.pull`) is skipped inside the scan with a named
        // `NameCollision` reason — the builtin keeps serving `get()` and the
        // catalog lists exactly one row (builtin wins, AR-36/AR-43).
        let tmp = tempfile::tempdir().unwrap();
        write_capability_dir(tmp.path(), "sync.pull");
        let deps = CapabilityRuntimeDeps {
            pool: None,
            prompt_executor: None,
            session_cancels: std::sync::Arc::new(std::sync::RwLock::new(
                std::collections::HashMap::new(),
            )),
            daemon_tool_dispatch: None,
            cdn_config: None,
        workspace_executor: None,
        };
        let (reg, outcome) = CapabilityRegistry::with_runtime_deps_and_user_caps(&deps, tmp.path());
        assert_eq!(outcome.admitted.len(), 0, "colliding user cap not admitted");
        assert_eq!(outcome.skipped.len(), 1, "one skip with named reason");
        assert_eq!(outcome.skipped[0].name, "sync.pull");
        assert!(
            outcome.skipped[0].reason.contains("NameCollision")
                || outcome.skipped[0]
                    .reason
                    .contains("collides with a builtin"),
            "named NameCollision reason, got: {:?}",
            outcome.skipped[0].reason
        );
        // Builtin still serves `get()` through the eager index.
        let cap = reg.get("sync.pull").expect("builtin still indexed");
        assert_eq!(cap.input_schema(), builtins::SyncPull.input_schema());
        // Catalog has exactly one row for the name.
        let rows = reg.iter().filter(|c| c.name() == "sync.pull").count();
        assert_eq!(rows, 1, "no duplicate catalog row");
    }

    #[tokio::test]
    async fn with_runtime_deps_and_wasm_and_user_caps_indexes_and_executor_wired() {
        let tmp = tempfile::tempdir().unwrap();
        write_capability_dir(tmp.path(), "demo.pull");
        let deps = CapabilityRuntimeDeps {
            pool: None,
            prompt_executor: None,
            session_cancels: std::sync::Arc::new(std::sync::RwLock::new(
                std::collections::HashMap::new(),
            )),
            daemon_tool_dispatch: None,
            cdn_config: None,
        workspace_executor: None,
        };
        let engine = std::sync::Arc::new(nexus_wasm_host::WasmEngine::new().unwrap());
        let cache = std::sync::Arc::new(nexus_wasm_host::ModuleCache::new());
        let (reg, outcome) = CapabilityRegistry::with_runtime_deps_and_wasm_and_user_caps(
            &deps,
            engine,
            cache,
            tmp.path(),
        );
        assert!(
            outcome.skipped.is_empty(),
            "no skips: {:?}",
            outcome.skipped
        );
        let cap = reg.get("demo.pull").expect("user cap indexed");
        // PL-10: the admitted-with-engine path no longer returns the P0
        // stub's WorkerUnavailable — the real executor (AR-37) runs. The
        // fixture's module pair is hash-consistent (admitted) but its wasm
        // bytes are not valid wasm, so the first `run()` fails fail-closed
        // at module load as PermanentExternal (AR-37 table: InvalidModule →
        // PermanentExternal).
        let err = cap.run(serde_json::json!({})).await.unwrap_err();
        assert!(
            matches!(err, CapabilityError::PermanentExternal(_)),
            "expected PermanentExternal from the real executor, got {err:?}"
        );
        assert!(
            err.to_string().contains("module fault"),
            "named module message, got: {err}"
        );
    }
}
