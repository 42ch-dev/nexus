# Agent-Host Architecture Spec v1

## Document Metadata

- **Date**: 2026-05-18
- **Status**: Active SSOT — **implementation detail** for `nexus-agent-host` subsystem; HTTP route inventory aligned with **V1.20 Shipped** ([v1.20-delivery-compass-v1.md](../iterations/v1.20-delivery-compass-v1.md) §4)
- **Normative upstream** (architecture boundaries): `nexus-platform` `.agents/designs/v1-spec/local/agent-host-v1.md`, **ADR-026**, **ADR-027**
- **Scope**: `nexus` repository — `crates/nexus-agent-host`, ACP integration, native CLI providers, discovery, policy, streaming
- **Supersedes**: V1.18 compass §3–§5 (architecture, research, PM notes), V1.19 compass §3–§4 (item details referencing OpenDesign/Multica patterns)
- **Referenced by**:
  - [V1.18 Delivery Compass](../iterations/v1.18-delivery-compass-v1.md) — original implementation context
  - [V1.19 Delivery Compass](../iterations/v1.19-delivery-compass-v1.md) — hardening context
  - [V1.20 Delivery Compass](../iterations/v1.20-delivery-compass-v1.md) — API layer redesign
- **External references**:
  - Multica: <https://github.com/multica-ai/multica>
  - OpenDesign: <https://github.com/nexu-io/open-design>

---

## 1. Overview

### 1.1 Purpose

Single source of truth for the `nexus-agent-host` subsystem architecture. This document consolidates all design decisions, provider implementation patterns, streaming transport design, policy/security model, and reference architecture research (Multica, OpenDesign) from V1.18 and V1.19 into one authoritative location.

### 1.2 Design principles

1. **The daemon runtime stays a supervisor/client**, not an ACP server or provider-specific implementation host.
2. **`nexus-agent-host` is the orchestration/facade layer** above `nexus-acp-host` and native CLI process adapters.
3. **ACP-first remains preferred**, while Hybrid means the host can also launch selected native CLIs under a narrower capability contract.
4. **Managed-only is the safety boundary**: every session and provider process is host-owned, observable, cancellable, and shut down by lifecycle hooks.
5. **Provider-native streaming, unified relay**: each provider uses its own transport internally; the daemon exposes a single `HostEvent`-based SSE endpoint to HTTP clients.
6. **Tool execution is always mediated through a host layer**, never exposed as raw provider protocol to external clients (borrowed from OpenDesign connector system).

### 1.3 Crate relationship

```text
nexus42 CLI
  └─ nexus-daemon-runtime          (local HTTP API + lifecycle supervisor)
       ├─ lifecycle: AgentHostSubsystem
       ├─ Axum routes: /v1/local/agent-host/*
       └─ Arc<dyn HostFacade>
            └─ nexus-agent-host     (orchestration/facade — this spec)
                 ├─ core: HostManager, SessionRegistry, OpRegistry
                 ├─ capability: normalized ops + negotiation + risk
                 ├─ discovery: config + PATH + ACP registry
                 ├─ policy: admission + permission delegation
                 ├─ providers/acp: official SDK via nexus-acp-host
                 ├─ providers/native_cli/claude: Wave 1 native adapter
                 └─ telemetry: structured host events
```

---

## 2. Crate Architecture

### 2.1 Crate structure

```text
crates/nexus-agent-host/
├── AGENTS.md
├── Cargo.toml
└── src/
    ├── lib.rs                    # public facade traits and DTO re-exports
    ├── error.rs                  # HostError and HostResult
    ├── ids.rs                    # ProviderId, HostSessionId, HostOperationId
    ├── config.rs                 # AgentHostConfig, ProviderConfig, TimeoutConfig
    ├── core/
    │   ├── mod.rs
    │   ├── manager.rs            # HostManager implementation
    │   ├── session.rs            # SessionRegistry + state transitions
    │   ├── operation.rs          # OpRegistry + op admission/cancel state
    │   └── lifecycle.rs          # startup/shutdown/drain helpers
    ├── capability/
    │   ├── mod.rs
    │   ├── model.rs              # HostOperation, HostEvent, CapabilityDescriptor
    │   ├── negotiation.rs        # ACP/native capability mapping
    │   └── risk.rs               # read/write/destructive tool risk classes
    ├── discovery/
    │   ├── mod.rs
    │   ├── catalog.rs            # ProviderCatalog
    │   ├── config.rs             # explicit configured providers
    │   ├── path_scan.rs          # known command lookup
    │   └── acp_registry.rs       # RegistryClient adapter
    ├── policy/
    │   ├── mod.rs
    │   ├── admission.rs          # provider/capability/session limits
    │   └── permission.rs         # ACP permission outcome selection
    ├── providers/
    │   ├── mod.rs
    │   ├── acp.rs                # AcpProvider using nexus-acp-host
    │   └── native_cli/
    │       ├── mod.rs
    │       ├── common.rs         # managed process helpers
    │       └── claude.rs         # Wave 1 native Claude CLI adapter
    └── telemetry/
        ├── mod.rs
        └── events.rs             # structured host event helpers
```

### 2.2 Workspace dependencies

- Root `Cargo.toml`: `crates/nexus-agent-host` is a workspace member.
- `nexus-agent-host/Cargo.toml`: depends on `nexus-acp-host`, `nexus-contracts`, `nexus-home-layout`, `tokio`, `async-trait`, `futures-util`, `serde`, `serde_json`, `thiserror`, `tracing`, `uuid`, `chrono`, `toml`, and `reqwest` (only if registry paths require direct HTTP access).
- `nexus-daemon-runtime/Cargo.toml`: depends on `nexus-agent-host` only after facade compiles.
- `nexus42/Cargo.toml`: no new direct dependency for Wave 1.

### 2.3 DTO sourcing

Use owned request/response DTOs in `nexus-agent-host`. Prefer generated `nexus-contracts` DTOs for API wire types once schemas exist; do not duplicate generated contracts in runtime API handlers after codegen.

---

## 3. Core Abstractions

### 3.1 IDs and error types

```rust
pub type HostResult<T> = Result<T, HostError>;
pub type HostEventStream = Pin<Box<dyn Stream<Item = HostResult<HostEvent>> + Send + 'static>>;

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct ProviderId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct HostSessionId(pub uuid::Uuid);

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct HostOperationId(pub uuid::Uuid);
```

### 3.2 Core traits

```rust
#[async_trait]
pub trait ProviderAdapter: Send + Sync {
    fn descriptor(&self) -> ProviderDescriptor;
    async fn probe(&self, request: ProbeRequest) -> HostResult<ProviderHealth>;
    async fn launch(&self, spec: LaunchSpec) -> HostResult<ManagedSessionHandle>;
    async fn execute(
        &self,
        session: &ManagedSessionHandle,
        op: HostOperation,
    ) -> HostResult<HostEventStream>;
    async fn cancel(
        &self,
        session: &ManagedSessionHandle,
        op_id: HostOperationId,
    ) -> HostResult<()>;
    async fn shutdown(&self, session: ManagedSessionHandle) -> HostResult<()>;
    fn capabilities(&self) -> CapabilityDescriptor;
}

#[async_trait]
pub trait HostFacade: Send + Sync {
    async fn start(&self, config: HostStartConfig) -> HostResult<()>;
    async fn create_session(&self, request: CreateSessionRequest) -> HostResult<HostSession>;
    async fn exec(&self, session_id: HostSessionId, op: HostOperation) -> HostResult<HostEventStream>;
    async fn cancel(&self, op_id: HostOperationId) -> HostResult<()>;
    async fn health(&self) -> HostResult<HostHealth>;
    async fn shutdown(&self) -> HostResult<()>;
}

#[async_trait]
pub trait ProviderDiscovery: Send + Sync {
    async fn discover(&self, config: &AgentHostConfig) -> HostResult<ProviderCatalog>;
}
```

**Design note (R-002)**: Cancel and Health are control-plane actions, not execution operations. `HostOperation` contains only execution-scoped variants (`Prompt`, `SetModel`, `SetMode`). Cancel flows through `HostFacade::cancel()` / `ProviderAdapter::cancel()`. Health is a separate `HostFacade::health()` query.

### 3.3 Request/response DTOs

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct HostStartConfig {
    pub config_path: PathBuf,
    pub workspace_root: PathBuf,
    pub max_sessions: usize,
    pub max_ops_per_session: usize,
    pub timeouts: TimeoutConfig,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CreateSessionRequest {
    pub provider_id: ProviderId,
    pub cwd: PathBuf,
    pub model: Option<String>,
    pub mode: Option<String>,
    pub mcp_servers: Vec<McpServerConfig>,
    pub metadata: serde_json::Value,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum HostOperation {
    Prompt {
        op_id: HostOperationId,
        content: Vec<HostContentBlock>,
    },
    SetModel {
        model: String,
    },
    SetMode {
        mode: String,
    },
}
```

### 3.4 HostEvent enum

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub enum HostEvent {
    SessionCreated(SessionCreatedEvent),
    OpStarted(OperationStartedEvent),
    ThoughtDelta(TextDeltaEvent),
    MessageDelta(TextDeltaEvent),
    ToolCall(ToolCallEvent),
    ToolCallUpdate(ToolCallUpdateEvent),
    PlanUpdate(PlanUpdateEvent),
    Status(StatusEvent),
    OpFinished(OperationFinishedEvent),
    OpFailed(OperationFailedEvent),
    SessionStopped(SessionStoppedEvent),
}
```

### 3.5 Capability descriptor

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CapabilityDescriptor {
    pub text_prompt: bool,
    pub streaming: bool,
    pub cancellation: bool,
    pub session_restore: bool,
    pub structured_tool_calls: bool,
    pub mcp_http: bool,
    pub mcp_sse: bool,
    pub mcp_stdio: bool,
    pub images: bool,
    pub audio: bool,
    pub embedded_context: bool,
    pub set_model: bool,
    pub set_mode: bool,
    pub diagnostics: bool,
}
```

Negotiation happens at session creation:
1. Merge static provider descriptor with ACP initialize response or native descriptor.
2. Apply host policy deny/allow toggles.
3. Validate requested `CreateSessionRequest` features.
4. Store negotiated descriptor on `HostSession`; all later ops check it before provider dispatch.

**Truthfulness invariant (D-003)**: The descriptor must never claim capabilities that the adapter cannot deliver. If `AcpProvider` cannot implement `SetModel`/`SetMode`, the static descriptor must not claim them. See §4.1.4 for graceful fallback.

### 3.6 Provider catalog entry

```rust
pub struct ProviderCatalogEntry {
    pub provider_id: ProviderId,
    pub display_name: String,
    pub protocol_kind: ProtocolKind, // Acp | NativeCli
    pub launch: LaunchStrategy,
    pub source: DiscoverySource,     // Config | PathScan | AcpRegistry
    pub trust: TrustLevel,           // Explicit | Registry | LocalPath
    pub capabilities: CapabilityDescriptor,
    pub health: ProviderHealth,
}
```

---

## 4. Provider Implementations

### 4.1 ACP provider

The ACP provider uses `nexus-acp-host` primitives — the official Rust SDK via `nexus-acp-host`, not a hand-rolled JSON-RPC parser.

#### 4.1.1 Lifecycle (10 steps)

1. **Resolve provider**: launch strategy from explicit config or registry entry.
2. **Spawn provider**: `nexus_acp_host::transport::AgentSpawner`.
3. **Build SDK connection**: `nexus_acp_host::AcpSdkAdapter::with_connection(...)` from child stdio.
4. **Initialize**: send `initialize` with latest supported protocol version, client info `nexus42` + crate version, and client capabilities for filesystem, terminal, and MCP as policy allows.
5. **Capability mapping**: convert initialize response into `CapabilityDescriptor` — `load_session` from `agentCapabilities.loadSession`, prompt modality from prompt capabilities, MCP transport from MCP capabilities, auth methods from response.
6. **Session creation**: default `session/new` for managed-only reproducibility. If request explicitly asks restore and `load_session` is supported, allow `session/load` only for host-owned persisted sessions.
7. **Optional model/mode**: use SDK config-option APIs where available. If setting fails and policy allows fallback, emit `Status` warning and continue with default. If policy disallows, return `CapabilityUnsupported`.
8. **Execute prompt**: translate `HostContentBlock` into ACP content blocks. Stream ACP `session/update` messages into `HostEvent` variants. Emit exactly one terminal `OpFinished` or `OpFailed`.
9. **Permission handling**: on `session/request_permission`, evaluate via `nexus-acp-host::PermissionPolicy::evaluate_for_agent()` with tool context and risk classification. See §7.2.
10. **Shutdown**: cancel active op if any, wait for configured graceful timeout, terminate child, then force kill if needed.

#### 4.1.2 Streaming transport

ACP is JSON-RPC 2.0 over newline-delimited stdio. The `AcpProvider` adapter manages this internally via `ActiveSession::read_update()`.

**Known gap (R-001)**: `NexusAcpClient::prompt()` returns `AcpResult<NexusPromptCompleted>` (one-shot), not a stream. The SDK's `ActiveSession::read_update()` can read streaming `session/update` messages but is not exposed through the public trait. Resolution: wrap `ActiveSession::read_update()` into a stream type within `nexus-acp-host`, bridging `!Send` via `LocalSetBridge`.

**ACP event → HostEvent mapping (D-006)**:

| ACP `session/update` content | HostEvent variant |
|---|---|
| `thinking` | `ThoughtDelta` |
| `text` | `MessageDelta` |
| `tool_use` | `ToolCall` |
| `tool_result` | `ToolCallUpdate` |
| `plan` / `reasoning` | `PlanUpdate` |

Handle partial line buffering for JSON-RPC newline-delimited messages. Each prompt produces exactly one terminal `OpFinished` or `OpFailed`.

#### 4.1.3 Permission handling (D-002)

ACP providers can request permissions (`session/request_permission`) with allow/reject options. Host policy must choose deterministic outcomes in managed-only mode:

1. The ACP provider's event stream monitors for permission request messages.
2. On receipt, extract tool name, description, and risk classification.
3. Call through to `nexus_acp_host::PermissionPolicy::evaluate_for_agent()` with tool context and session identity.
4. If `AutoToolRiskClassifier` (§7.2) is available, use it to determine risk level; otherwise fall back to `StaticToolRiskClassifier`.
5. Permission outcome preference order: `approve_for_session > allow_always > allow_once`; reject otherwise (borrowed from OpenDesign `choosePermissionOutcome()`).
6. Send the permission response back through the ACP SDK.
7. Emit a `ToolCallUpdate` host event so the caller knows a permission decision was made.

**Integration note (R-003)**: `nexus-agent-host`'s permission handling for ACP providers **delegates** to `nexus-acp-host::PermissionPolicy`. The host-level `policy/admission.rs` adds provider/capability/session admission gates on top (e.g., "is this provider allowed at all?"), but once an ACP `session/request_permission` arrives, the outcome selection calls through to `PermissionPolicy::evaluate_for_agent()`. Native CLI providers use the host-level risk classification only (no ACP permission delegation).

#### 4.1.4 SetModel/SetMode graceful fallback (D-003)

When a provider does not support model/mode changes at the SDK level:
- **Option A (implement with fallback)**: use ACP SDK config-option APIs. If the provider rejects with protocol error (-32603 or -32602), apply graceful fallback — emit `Status` warning and continue with default (only when `allow_model_fallback = true`). Borrowed from OpenDesign `acp.ts` model switching pattern.
- **Option B (remove claim)**: set `set_model = false` and `set_mode = false` in the static descriptor. Only enable after successful ACP initialize negotiation confirms support.

**Key constraint**: the descriptor must never claim what the adapter cannot deliver.

### 4.2 Native CLI provider (Wave 1: Claude Code)

#### 4.2.1 Multi-turn session support (D-001)

`ClaudeCliProvider` must maintain a persistent child process across `execute()` calls within the same session:

1. `launch()` spawns the child process and stores it in a session-scoped `NativeSession { child, stdin, stdout }`.
2. `stdin` pipe stays open across `execute()` calls.
3. Each `execute()` writes a new prompt to `stdin` and reads the response from `stdout`.
4. `shutdown()` closes stdin (EOF), waits for process exit with `shutdown_ms` timeout, then force-kills.
5. Claude CLI `--print` mode is single-turn by design — multi-turn requires interactive/repl mode or `--resume`/`--session-id` flags.

**Open question**: Does Claude CLI `--print` support multi-turn via persistent stdin, or does it require `--resume`/`--session-id`? If not, scope reduces to "best-effort session continuity via resume flags" with a note that full multi-turn requires ACP provider.

#### 4.2.2 Streaming normalization

- Normalize stdout lines/chunks to `MessageDelta`.
- Stderr lines become `Status` warnings unless the process exits non-zero.
- Tool-call visibility: native CLI output is unstructured in Wave 1; report `structured_tool_calls = false` in `CapabilityDescriptor`.
- Cancellation: send child kill/termination sequence; map to `OperationCancelled` when host-initiated.

Each native adapter should follow the per-provider stream adapter pattern (borrowed from OpenDesign's `claude-stream.ts`, `copilot-stream.ts`, `qoder-stream.ts`): spawn → attach → stream events → handle error/timeout → cleanup.

#### 4.2.3 Wave 1 constraints

- Default command lookup: `claude` on PATH, overridable in config.
- Default mode: non-interactive single-turn prompt through configured args (verified: `--print`).
- Wave 1 capability reporting is intentionally narrower than ACP: managed subprocess ownership, prompt execution, cancellation by process termination, stdout/stderr event normalization, health/probe, and configured model/args/env.
- Future native providers (Codex, Gemini, OpenCode, etc.) follow the same per-provider adapter pattern with their own normalization modules.

---

## 5. Discovery

### 5.1 Three-source discovery

Discovery is deterministic and ordered:

1. **Static config** from `{NEXUS_HOME}/agent-host/config.toml`: explicit provider IDs, protocol kind, command template, args/env, allow/deny state, timeout/concurrency overrides.
2. **PATH scan** for known native commands: Wave 1: `claude` only. Later waves: `codex`, `gemini`, `opencode`, `cursor`, `kimi`, etc. Use cross-platform probe (V1.19 D-009: replaced Unix-only `which` with `which::which()` crate or platform-conditional `Command::new`).
3. **ACP registry** via `nexus_acp_host::RegistryClient`: include registry entries with runnable distributions for current platform, annotate as `protocol_kind = acp`, preserve registry metadata and trust source.

### 5.2 Deduplication rules

- Explicit config wins over PATH, PATH wins over registry aliases.
- Disabled config entries suppress matching auto-discovered entries.
- **ACP-vs-native coexistence (R-004, R-008)**: dedupe applies per `provider_id`, not per agent identity. ACP registry and PATH scan produce different provider IDs (e.g., `claude` from registry vs `claude-native` from PATH), so both can coexist. When the same agent has both ACP and native entries and no explicit config, ACP registry entry takes priority (protocol-rich). Explicit config can suppress either or both.

### 5.3 Per-agent config

Each provider can have: model, custom args/env, MCP config, and concurrency limits. This pattern is validated by Multica's per-agent configuration model (see §11).

---

## 6. Session and Operation Lifecycle

### 6.1 Session state machine

```text
Created
  -> Starting
  -> Ready
  -> Busy(op_id)
  -> Ready
  -> Stopping
  -> Stopped

Busy(op_id) -> Cancelling(op_id) -> Ready | Stopped
Starting | Busy | Cancelling | Stopping -> ErrorRecoverable | ErrorTerminal
ErrorRecoverable -> Starting     # only when restart policy permits
ErrorTerminal -> Stopped         # new explicit session required
```

Rules:
- One active op per session in Wave 1.
- `exec(Prompt)` requires `Ready` and transitions to `Busy`.
- `cancel(op_id)` requires `Busy(op_id)` and transitions to `Cancelling`.
- Provider process exit during `Busy` is `ErrorRecoverable` only if no host state is corrupted and retry caps allow; otherwise `ErrorTerminal`.
- Every path out of `Busy` emits one terminal host event.
- `shutdown()` transitions all sessions through `Stopping` and drains with configured grace timeout.

### 6.2 Error handling taxonomy

| Host error | ACP mapping | Native mapping |
|---|---|---|
| `ProviderUnavailable` | registry entry unavailable, missing command, health probe failed | command absent on PATH |
| `LaunchFailed` | spawn/stdio/SDK connection setup failed | process spawn failed |
| `CapabilityUnsupported` | initialize capabilities or config options lack required feature | native descriptor lacks operation |
| `PolicyDenied` | permission outcome denied by host policy | command/provider/capability denied |
| `OperationTimeout` | stage timeout around initialize/session/prompt/shutdown | process timeout |
| `OperationCancelled` | ACP cancel acknowledged or host cancelled stream | host killed/cancelled process |
| `ProviderProtocolError` | invalid ACP response, SDK protocol error, unexpected stop | malformed native output |
| `InternalHostError` | registry/session/op invariant violation | same |

`HostError` carries `provider_id`, optional `session_id`, optional `op_id`, `category`, `message`, and optional `source` string safe for local logs.

### 6.3 HostManager shutdown (D-007)

`HostManager::shutdown()` must call `ProviderAdapter::shutdown()` for all active sessions before clearing state:

1. Iterate all active sessions.
2. For each session, call the corresponding `ProviderAdapter::shutdown()`.
3. Wait up to `shutdown_ms` per session (from `TimeoutConfig`).
4. Force-kill any surviving processes.
5. Only then clear session registry and provider mappings.

### 6.4 AdmissionPolicy enforcement (D-008)

`AdmissionPolicy` methods must be called in `create_session()` and `exec()`:

**In `create_session()`**:
- `admission.check_provider(&request.provider_id)` — verify provider is allowed.
- `admission.check_session_limit(session_count)` — enforce `max_sessions`.

**In `exec()`**:
- `admission.check_ops_per_session(active_ops)` — enforce `max_ops_per_session`.
- `admission.check_before_exec(&session, &op)` — verify capability and policy.

On denial, return `HostError::PolicyDenied` with descriptive message. Default policy is permissive (`unknown_provider = "allow"`); enforcement only activates when explicitly configured.

---

## 7. Policy and Security

### 7.1 Admission policy

Host-level admission gates run before provider launch and before operation dispatch:
- Provider allow/deny
- Capability allow/deny
- Workspace root check
- Concurrency limits (max sessions, max ops per session)

### 7.2 Tool risk classification (D-005)

`AutoToolRiskClassifier` classifies unknown tools by name pattern (borrowed from OpenDesign `classifyConnectorToolSafety()`):

| Pattern | Risk class |
|---|---|
| `drop\|truncate\|purge\|erase\|wipe\|rm \|force\|kill\|destroy` | `Destructive` |
| `create\|update\|delete\|send\|manage\|write\|insert\|modify\|edit\|remove` | `Write` |
| `get\|list\|search\|fetch\|view\|read\|show\|find\|query\|check\|describe` | `Read` |
| Default (unknown tools) | `Write` |

Compiled regex patterns (not runtime-compiled per call). `HostManager` uses `AutoToolRiskClassifier` as default; `StaticToolRiskClassifier` overrides for specific tools when configured.

**Extension point**: `ToolRiskClassifier` trait supports static declaration (manual registration) and auto-classification (regex heuristic). See `capability/risk.rs`.

### 7.3 Path traversal protection (D-011)

Config paths are validated against directory traversal attacks:

1. `workspace_root` in `CreateSessionRequest`: must be absolute, must not escape above a configured trust boundary (default: current user's home directory or the workspace root passed to `HostManager::start()`).
2. `config_path` in `HostStartConfig`: must be under `{NEXUS_HOME}/agent-host/` or an explicitly allowed path.
3. Use `std::path::canonicalize()` for resolution and prefix checking.
4. Return `HostError::PolicyDenied` for traversal attempts.

### 7.4 API input validation (D-010)

Parse session ID path parameters as `uuid::Uuid` at the handler boundary. Return 400 for invalid IDs, 404 for valid UUIDs that don't match any session.

---

## 8. Streaming and Event Transport

### 8.1 Design

The daemon exposes a single `HostEvent`-based SSE endpoint (`GET /v1/local/agent-host/sessions/{id}/events`) that relays provider-side events to HTTP clients. The transport between provider and daemon is provider-specific:

- **ACP provider**: JSON-RPC 2.0 over newline-delimited stdio via `ActiveSession::read_update()`. The `AcpProvider` adapter manages this internally. The daemon relay consumes `HostEventStream` and publishes as SSE.
- **Non-ACP provider (e.g., native CLI)**: provider-adapter-specific transport (subprocess stdio, named pipes, etc.). Each native adapter normalizes its output into `HostEvent` variants per the per-provider stream adapter pattern (§4.2.2).

Both paths converge on the same `HostEventStream` type, so the SSE relay is provider-agnostic.

### 8.2 Streaming adaptation status

The `into_event_stream` scaffold exists in `acp.rs` but the full ACP event → `HostEvent` mapping is not wired for all ACP event types (D-006). The mapping is defined in §4.1.2.

For V1.18/Wave 1, the HTTP endpoint may return buffered event arrays if streaming transport is not yet implemented, but the internal `HostFacade::exec` must already use `HostEventStream` to avoid painting the design into a non-streaming corner.

### 8.3 Timeout enforcement (D-004)

All provider operations are bounded by `TimeoutConfig` values via `tokio::time::timeout()`:

| Operation | Timeout config key |
|---|---|
| `probe()` | `launch_ms` |
| `launch()` | `launch_ms` + `initialize_ms` (two-stage) |
| `execute()` | `prompt_ms` |
| `shutdown()` | `shutdown_ms` (drain timeout with force-kill fallback) |

On timeout: cancel the operation and emit `OpFailed` with `error_category = "timeout"`.

---

## 9. Configuration

### 9.1 Config location

```text
{NEXUS_HOME}/agent-host/config.toml
{NEXUS_HOME}/agent-host/sessions.json       # reserved for future host-level session persistence
{NEXUS_HOME}/agent-host/events/             # optional JSONL event traces by run/session
```

Use `nexus-home-layout` helpers for path resolution. Do not hardcode paths.

### 9.2 Config contract

```toml
max_sessions = 4
max_ops_per_session = 1

[timeouts]
launch_ms = 15000
initialize_ms = 15000
session_ms = 30000
prompt_ms = 180000
shutdown_ms = 5000

[policy]
unknown_provider = "deny"        # deny | allow
unknown_tool_risk = "deny"       # deny | ask | allow
allow_model_fallback = true

[[providers]]
id = "claude-native"
protocol = "native_cli"
command = "claude"
args = ["--print"]
enabled = true
```

Stage-level timeout defaults (15s init, 180s prompt) are validated production defaults borrowed from OpenDesign `acp.ts`.

### 9.3 Session persistence

Wave 1 uses in-memory session tracking only. ACP sessions may optionally use `nexus-acp-host::SessionManager` for ACP-level session restore. Native sessions have no persistence in Wave 1. The `sessions.json` path is reserved for future use.

---

## 10. Daemon-Runtime Integration

### 10.1 Facade boundary

Integration is via narrow facade only:
- `WorkspaceState` stores `Option<Arc<dyn HostFacade>>` plus setters/getters.
- `boot.rs` constructs `HostManager`, loads config, calls `start`, stores the facade, and adds `AgentHostSubsystem` to lifecycle.
- Runtime handlers call `HostFacade` only — they do not import provider modules.

### 10.2 API routes

| Method | Route | Purpose |
|---|---|---|
| `GET` | `/v1/local/agent-host/providers` | list discovered providers and negotiated/static capabilities |
| `POST` | `/v1/local/agent-host/sessions` | create managed session |
| `GET` | `/v1/local/agent-host/sessions` | list managed sessions and health |
| `POST` | `/v1/local/agent-host/sessions/{id}/operations` | execute prompt or config op |
| `POST` | `/v1/local/agent-host/operations/{op_id}:cancel` | cancel active op |
| `GET` | `/v1/local/agent-host/sessions/{id}/events` | SSE event stream |
| `DELETE` | `/v1/local/agent-host/sessions/{id}` | shut down one session (must not shut down entire host) |

V1.20 redesign extends these routes with additional agent-host endpoints (see V1.20 spec §4.3).

---

## 11. Reference: Multica

**Source**: <https://github.com/multica-ai/multica> (Go server + local daemon)

Multica is an open-source AI-native team collaboration platform that wraps multiple coding agents (Claude Code, Codex, Gemini CLI, etc.) with a Linear-style Kanban UI.

### 11.1 Daemon architecture

- Local daemon scans `PATH` for known agent CLI commands on startup.
- Registers the machine as a "Runtime" (compute environment) with the cloud server.
- **Dual transport**: WebSocket (real-time priority) + HTTP polling (reliable fallback). This informs Nexus's SSE + polling approach.
- Receives task assignments from server → claims tasks → executes via local agent CLIs → streams progress → reports completion/failure.

### 11.2 Task lifecycle

- `POST /api/daemon/runtimes/{id}/tasks/claim` — atomically claim next queued task.
- `POST /api/daemon/tasks/{id}/start` — mark task as running.
- `POST /api/daemon/tasks/{id}/progress` — report execution progress (streaming updates).
- `POST /api/daemon/tasks/{id}/messages` — batch report agent execution messages.
- `POST /api/daemon/tasks/{id}/complete` — mark done with output, branch, session info.
- `POST /api/daemon/tasks/{id}/fail` — mark failed with error and failure reason.
- `POST /api/daemon/tasks/{id}/session` — persist session_id + work_dir mid-execution (crash recovery).
- `GET /api/daemon/tasks/{id}/status` — check if task was cancelled during execution.
- `POST /api/daemon/runtimes/{id}/recover-orphans` — mark previously-running tasks as failed (daemon restart recovery).

### 11.3 Patterns borrowed

| Pattern | Nexus mapping |
|---|---|
| Local daemon as session orchestrator | `nexus-daemon-runtime` + `HostManager` |
| PATH scan discovery for known CLI commands | `discovery/path_scan.rs` |
| Provider catalog with trust metadata | `ProviderCatalogEntry` |
| Managed task/session state with deterministic terminal events | Session state machine (§6.1) |
| Per-agent config (model, env, args, MCP, concurrency) | `[[providers]]` TOML config |
| Orphan recovery (daemon restart) | `HostManager::shutdown()` drain (§6.3) |
| Task lease + atomic claim | Future multi-session concurrent execution |
| Skills injection (Markdown instructions) | Future "pre-session context injection" |

### 11.4 Architecture mapping

```
Multica Runtime          ≈ nexus-daemon-runtime  (local API + lifecycle)
Multica Agent Protocol   ≈ ProviderAdapter        (ACP SDK / native CLI)
Multica event model     ≈ HostEvent + SSE relay  (unified event stream)
Multica task lifecycle   ≈ HostSession state machine (managed-only)
```

### 11.5 Known supported CLI agents

Claude Code (`claude`), Codex CLI, GitHub Copilot CLI, Gemini CLI, Kimi CLI, Kiro CLI.

---

## 12. Reference: OpenDesign

**Source**: <https://github.com/nexu-io/open-design/tree/main/apps/daemon/src>

OpenDesign is a TypeScript daemon implementing multi-agent ACP communication with a rich connector/tool ecosystem.

### 12.1 ACP adapter (`acp.ts`)

- `detectAcpModels()`: spawns agent → initialize → session/new → reads `models` from response → returns model list.
- `attachAcpSession()`: full session lifecycle manager — initialize → session/new → optional set_model → session/prompt → stream updates → result.
- `createJsonLineStream()`: newline-delimited JSON-RPC parser that feeds chunks and handles partial lines.
- `buildAcpSessionNewParams()`: constructs `session/new` params with resolved `cwd` and normalized MCP server configs.
- **Stage-level timeouts**: `DEFAULT_TIMEOUT_MS = 15_000` (initialization), `DEFAULT_STAGE_TIMEOUT_MS = 180_000` (prompt/execution). Each RPC stage gets its own timer.
- **Model switching graceful fallback**: if `session/set_model` fails with -32603 or -32602, falls back to default model and proceeds with prompt. This is the pattern adopted for Nexus D-003.
- **Permission auto-approval**: `choosePermissionOutcome()` tries `approve_for_session > allow_always > allow_once`. This is the preference order adopted for Nexus ACP permission handling (§4.1.3).

### 12.2 Multi-provider stream adapters

- `claude-stream.ts`, `copilot-stream.ts`, `qoder-stream.ts`: per-provider stream normalization.
- All adapters follow the same contract: spawn → attach → stream events → handle error/timeout → cleanup.
- This is the pattern adopted for Nexus native CLI provider normalization (§4.2.2).

### 12.3 Connector system (`connectors/`)

- `catalog.ts`: connector catalog with tool safety classification.
  - `ConnectorToolSafety { sideEffect, approval, reason }` — three axes: `read`/`write`/`destructive` × `auto`/`confirm`/`disabled`.
  - `classifyConnectorToolSafety()`: regex-based auto-classification. Checks tool name/description for destructive hints → disabled; write hints → confirm; read hints → auto. Default is write/confirm for unknown tools.
  - `isRefreshEligibleConnectorToolSafety()`: only `read + auto` tools can be auto-refreshed.
- `service.ts`: connector service managing lifecycle and state.
- `routes.ts`: API routes for connector CRUD.

This is the pattern adopted for Nexus `AutoToolRiskClassifier` (§7.2).

### 12.4 Provider-agent architecture (`agents.ts`)

- Agent definitions with provider identity, capabilities, and configuration.
- Multi-agent concurrent execution support.

### 12.5 Patterns borrowed

| Pattern | Nexus mapping |
|---|---|
| Stage-level timeouts (15s init, 180s prompt) | `TimeoutConfig` defaults (§9.2) |
| Graceful model fallback | D-003 SetModel/SetMode (§4.1.4) |
| Auto tool-risk classification | `AutoToolRiskClassifier` (§7.2) |
| Centralized permission policy | ACP permission delegation via `PermissionPolicy` (§4.1.3) |
| Per-provider stream normalization | Native CLI adapter pattern (§4.2.2) |
| Tool execution via host mediation | `HostToolExecutor` trait (V1.20 spec §4.4) |
| One process per managed provider session | `ManagedSessionHandle` + session state machine |

---

## 13. ACP Protocol Findings

### 13.1 Transport

ACP is JSON-RPC 2.0 over newline-delimited stdio in current mainstream deployments. This repo must use the official Rust SDK via `nexus-acp-host`, not a hand-rolled parser.

### 13.2 Lifecycle

Real lifecycle: `initialize → session/new | session/load → session/prompt → session/update* → stop reason`.

### 13.3 Initialize negotiation

Negotiates: `protocolVersion`, `clientCapabilities`, `clientInfo`, `agentCapabilities`, `agentInfo`, and `authMethods`.

Runtime-relevant ACP capabilities:
- Session loading (`loadSession`)
- Prompt content modalities
- MCP transport capabilities
- Session config options for mode/model
- Cancellation
- Permission requests

### 13.4 Permission requests

ACP providers can request permissions (`session/request_permission`) with allow/reject options. Host policy must choose deterministic outcomes in managed-only mode. See §4.1.3 for the full handling flow.

### 13.5 ACP registry

- Many mainstream agents are listed: Claude Agent, Codex CLI, Gemini CLI, Cursor, Cline, OpenCode, Kimi CLI, Qwen Code, Kiro CLI, OpenHands, Docker cagent, etc.
- `nexus-acp-host::registry::RegistryClient` fetches and caches `https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json`.
- `nexus-agent-host` consumes the registry via discovery, not by duplicating registry models.

---

## 14. Design Review Decisions

These decisions were made during V1.18 PM review and remain authoritative.

### R-001: ACP streaming gap [MEDIUM]

`NexusAcpClient::prompt()` returns one-shot, not a stream. `ActiveSession::read_update()` can read streaming messages but is not exposed through the public trait.

**Resolution**: Wrap `ActiveSession::read_update()` into a stream type within `nexus-acp-host`, bridging `!Send` via `LocalSetBridge`. This may add scope to provider implementation.

### R-002: Cancel and Health are not operations [MEDIUM — RESOLVED]

Removed `HostOperation::Cancel` and `HostOperation::Health` from the enum. Cancel flows through `HostFacade::cancel()`. Health is `HostFacade::health()`.

### R-003: Permission delegation to existing policy [MEDIUM — RESOLVED]

ACP permissions delegate to `nexus-acp-host::PermissionPolicy`. Host-level admission adds provider/capability/session gates on top. See §4.1.3 and §7.1.

### R-004: Claude CLI command name ambiguity [LOW — RESOLVED]

ACP registry lists "Claude Agent" (ACP wrapper), native CLI is "Claude Code" (`claude` binary). Different `provider_id` values prevent collision: `claude` (registry) vs `claude-native` (PATH). See §5.2.

### R-005: Claude CLI args placeholder [LOW — RESOLVED]

Verified: `--print` is the correct non-interactive flag. `ClaudeCliProvider::default_config()` uses `vec!["--print".to_string()]`.

### R-006: Tool risk auto-classification extension [LOW — RESOLVED]

`ToolRiskClassifier` trait created in Wave 1 with `StaticToolRiskClassifier`. `AutoToolRiskClassifier` implemented in V1.19 (D-005) borrowing OpenDesign regex approach. See §7.2.

### R-007: Session persistence reserved [LOW — RESOLVED]

Wave 1 in-memory only. ACP sessions may use existing `SessionManager`. Native sessions have no persistence. `sessions.json` reserved for future use. See §9.3.

### R-008: ACP-vs-native coexistence [LOW — RESOLVED]

Dedupe per `provider_id`, not per agent identity. Both can coexist with distinct IDs. See §5.2.

---

## 15. Version History

| Version | Scope | Key additions |
|---|---|---|
| v1 | Consolidated from V1.18 + V1.19 | Initial SSOT: architecture, provider impl, discovery, policy, streaming, Multica/OpenDesign research, ACP protocol findings, design review decisions |

---

*Created: 2026-05-18. Consolidated from V1.18 compass §3–§5 and V1.19 compass §3–§4. Authoritative SSOT for `nexus-agent-host` subsystem design.*
