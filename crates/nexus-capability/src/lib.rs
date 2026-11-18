//! Capability port and error contract.
//!
//! Design: `docs/specs/orchestration/orchestration-engine.md` §5.1–5.2.
//! The registry, builtins and admission/scan/watch machinery remain in
//! `nexus-orchestration`; this crate owns only the port/error surface.

use async_trait::async_trait;
use serde_json::Value;
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
/// caller lane with `ToolsApiCallerKind::Schedule` audit differentiation).
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

    /// The absolute workspace root whose `Works/` tree this executor commits.
    ///
    /// A builtin resolves its `Works/<work_ref>/` bookkeeping against this
    /// root, so existence checks, directory creation, the port commit and
    /// rollback all address one tree instead of the process cwd.
    ///
    /// Default refuses: an executor with no bound workspace cannot resolve a
    /// target, so no caller falls back to a relative path.
    ///
    /// # Errors
    /// [`CapabilityError::WorkerUnavailable`] when no workspace is bound.
    fn workspace_root(&self) -> Result<std::path::PathBuf, CapabilityError> {
        Err(CapabilityError::WorkerUnavailable)
    }

    /// Whether this executor records Work artifact writes instead of landing
    /// them.
    ///
    /// A scaffold creates the `Works/<work_ref>/` directory skeleton itself,
    /// outside `commit_work_artifact`, so it asks this before touching the
    /// disk: recording bytes that never land must not leave the empty tree
    /// behind them.
    ///
    /// Default is production: an executor that does not say otherwise lands
    /// its writes, so no existing implementation changes behaviour.
    fn records_work_artifacts(&self) -> bool {
        false
    }

    /// Commit one Work artifact byte write (`Works/<work_ref>/<rel_child>`).
    ///
    /// The single production implementation delegates to the core byte-write
    /// port, so an orchestration caller writes Work bytes under the port's
    /// Work lock and durability path instead of opening its own writer.
    ///
    /// Default refuses: every impl that does not write Work bytes stays
    /// untouched and cannot silently acquire a second writer.
    ///
    /// # Errors
    /// [`CapabilityError::InputInvalid`] when the executor carries no Work
    /// artifact port (default), [`CapabilityError::Forbidden`] when the Work
    /// row, creator, workspace or path boundary refuses the write, and
    /// [`CapabilityError::Internal`] on a lookup or write failure.
    async fn commit_work_artifact(
        &self,
        _input: nexus_contracts::local::orchestration::WorkArtifactCommitInput,
    ) -> Result<nexus_contracts::local::orchestration::WorkArtifactCommitOutput, CapabilityError>
    {
        Err(CapabilityError::InputInvalid(
            "work artifact commit not wired".into(),
        ))
    }
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

// ---------------------------------------------------------------------------
// Capability trait
// ---------------------------------------------------------------------------

/// Provenance of a capability (AR-40).
///
/// Marker-only: `Builtin` is the default (zero edits to the ~34 builtin
/// impls); `User` marks a locally-installed user capability. The enum stays
/// in the private capability contract and NEVER crosses into
/// `nexus-contracts` (dependency direction, AR-40) — the wire layer maps it
/// to the `"builtin"` / `"user"` string enum itself.
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
