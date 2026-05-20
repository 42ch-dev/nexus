# Nexus42 Single-Binary Daemon Runtime Architecture v1

## Document Metadata

- Scope: `nexus` repository only
- Type: Implementation architecture (dev-process knowledge SSOT)
- **Normative upstream** (architecture boundaries): `nexus-platform` `.agents/designs/v1-spec/local/daemon-runtime-v1.md`, **ADR-026**, **ADR-027**
- Status: Active
- Spec readiness: Ready for implementation handoff
- Decision mode: Break-change allowed, no compatibility migration required
- Related user goals:
  - Unify daemon into `nexus42` CLI entry
  - Prepare clean runtime boundary for future `nexus-agent-host`

## 1. Objective and Constraints

This design defines how to remove `nexus42d` as an independent product boundary and converge to a single executable entrypoint: `nexus42`.

Hard constraints for this version:

1. No migration compatibility layer is required.
2. Existing `nexus42d` external compatibility is not preserved.
3. Local state compatibility is not preserved by requirement.
4. Architecture elegance and long-term maintainability take priority over short-term minimal diff.

## 2. Recommended Architecture

### 2.1 Summary

Use a **single binary + layered runtime** model:

- Keep one user-facing binary: `nexus42`
- Extract daemon internals into a reusable runtime crate
- Invoke runtime from CLI command routing, instead of spawning a separate daemon product binary

### 2.2 Layering

1. `crates/nexus42` (entry and UX layer)
   - CLI parsing
   - command routing
   - user-facing output and process mode dispatch
2. `crates/nexus-daemon-runtime` (new runtime layer)
   - lifecycle state machine
   - subsystem composition (db, sync, worker manager, schedule supervisor, local API)
   - shutdown orchestration and signal handling
3. future `crates/nexus-agent-host` (next topic, not implemented in this scope)
   - agent discovery providers
   - managed agent session hosting
   - host-specific policies and health surfaces

## 3. Crate and Directory Restructure

### 3.1 New crate

Create `crates/nexus-daemon-runtime` and migrate daemon internals from `crates/nexus42d/src/*`:

- `api/`
- `auth/`
- `db/`
- `lifecycle/`
- `workspace/`
- runtime config module (renamed from daemon-specific naming)

This crate should be a library crate. It should not expose a standalone user binary.

### 3.2 CLI integration

In `crates/nexus42`:

- Keep `daemon` command group as the user control surface.
- Replace spawn target from `nexus42d` to runtime invocation paths.
- Add hidden internal command path for background mode, for example:
  - `nexus42 __internal daemon-run ...`

### 3.3 Workspace cleanup

In root `Cargo.toml`:

- Remove `crates/nexus42d` from workspace members.
- Add `crates/nexus-daemon-runtime`.
- Add dependency from `nexus42` to `nexus-daemon-runtime`.

## 4. Process Model

### 4.1 Foreground mode

`nexus42 daemon start --foreground` runs runtime directly in the current process.

### 4.2 Background mode

`nexus42 daemon start` (default) performs:

1. preflight checks
2. self-spawn into hidden internal daemon-run mode
3. parent exits after startup gate

This preserves one-entrypoint operation while keeping daemonized behavior.

### 4.3 Stop/restart/status

- `stop` and `status` continue through local runtime health and process coordination.
- `restart` remains stop -> dead confirmation -> start.
- existing force-stop fallback logic can be preserved during first integration wave.

## 5. API and Transport Strategy

This scope does not remove local API transport.

Keep existing local transport abstractions in runtime:

- HTTP loopback
- Unix socket

`DaemonClient` remains a client to local runtime endpoints, but semantics should no longer imply a separate daemon product binary.

## 6. Lifecycle and Reliability

Carry lifecycle behavior into runtime crate first, then refactor incrementally:

1. Preserve current HSM semantics
2. Preserve signal and panic hooks
3. Preserve graceful shutdown orchestration
4. Preserve subsystem startup ordering

Refinement can happen after behavior parity gate is validated.

## 7. Error and Observability Model

Normalize error ownership by layer:

- CLI layer: command misuse and user-surface errors
- runtime layer: orchestration, subsystem, transport errors
- API handlers: request validation, permission checks, execution failures

Unify log naming around "Nexus daemon runtime" instead of product name `nexus42d`.

## 8. Verification Matrix

Minimum required verification:

1. `nexus42 daemon start --foreground` boots and serves health endpoint
2. default background start returns and runtime stays alive
3. `status` sees running runtime
4. `stop` terminates runtime cleanly
5. `restart` replaces process and health returns
6. ACP-related runtime paths continue to function
7. schedule supervisor boot and shutdown hooks remain valid

## 9. Implementation Batches

### Batch 1: Runtime extraction

- create `nexus-daemon-runtime`
- migrate modules from `nexus42d`
- keep temporary parallel compile path

### Batch 2: Single-binary wiring

- wire `nexus42 daemon` commands to runtime/internal-run mode
- remove hard dependency on spawning `nexus42d`

### Batch 3: Remove old daemon crate

- remove `nexus42d` workspace member and references
- clean build/test matrix

### Batch 4: Naming and hardening

- unify user-facing wording and logs
- finalize reliability and concurrency edge cases

## 10. Sub-spec #2: `nexus-agent-host` Architecture

### 10.1 Confirmed decisions

This sub-spec is locked to the following decisions:

1. Hosting mode: **Managed-only**
2. Protocol model: **Hybrid** (ACP + Native adapters)
3. Target scope: **ACP + common agent CLIs**
   - at minimum includes: Claude CLI, Codex CLI, Gemini CLI, OpenCode CLI, Cursor CLI, Kimi CLI
4. Multica/OpenDesign are architectural references, not runtime dependencies.

### 10.2 Chosen architecture (normative)

`nexus-agent-host` MUST implement a Hybrid host core:

1. Host lifecycle, policy, scheduling, telemetry, and session state are centralized.
2. Provider adapters MAY be ACP-backed or native CLI-backed.
3. Capability execution MUST pass through a normalized operation contract before runtime dispatch.
4. Runtime (`nexus-daemon-runtime`) MUST depend only on host facade traits, never on provider-specific code.

### 10.3 Layered design

Create new crate `crates/nexus-agent-host` with these layers:

1. `core/`
   - session state machine
   - worker lease management
   - host-level policy checks
   - admission control and concurrency limits
2. `capability/`
   - normalized operation model (prompt, tool-call, cancel, stream, health)
   - provider feature negotiation
3. `providers/`
   - `acp_provider`
   - `native_cli_provider` family (claude/codex/gemini/opencode/cursor/kimi adapters)
4. `discovery/`
   - managed-only provider discovery from local environment and host config
5. `telemetry/`
   - structured events, run/session correlation, provider-level metrics

`nexus-daemon-runtime` integrates this crate as a subsystem, not vice versa.

### 10.4 Required crate layout

`crates/nexus-agent-host` should expose:

1. `lib.rs`
   - public facade traits and DTOs used by runtime
2. `core/`
   - session manager and orchestration
3. `capability/`
   - normalized operation definitions and negotiation
4. `providers/`
   - ACP adapter + native CLI adapters
5. `discovery/`
   - provider registration and deterministic discovery
6. `policy/`
   - host-level admission and execution policy checks
7. `telemetry/`
   - event schema and metrics bridge

### 10.5 Managed-only contract

Managed-only means:

1. Host is the process owner for all provider sessions.
2. No attach to external unmanaged sessions.
3. Session lifecycle must be reproducible from host state.
4. Every provider launch must pass through the same policy and audit checkpoints.

This simplifies reliability, cleanup, and security boundaries.

### 10.6 Provider model and adapter contract

Define internal provider trait contract:

- `probe() -> ProviderHealth`
- `launch(spec) -> ManagedSessionHandle`
- `execute(session, op) -> Stream<HostEvent>`
- `cancel(session, op_id)`
- `shutdown(session)`
- `capabilities() -> CapabilityDescriptor`

Adapter responsibilities:

1. Transform host-normalized operations into provider protocol calls
2. Normalize provider events into host event schema
3. Report deterministic error categories
4. Expose declared capability flags

### 10.7 Capability normalization (Hybrid key)

Normalized capability set MUST include:

- text prompt execution
- structured tool invocation
- streaming chunk delivery
- cancellation
- session restore (if supported)
- health and diagnostics

Providers can implement partial capabilities. The host core performs capability negotiation at session start and rejects unsupported operations early.

### 10.8 Discovery design

Discovery MUST be host-owned and deterministic:

1. Static config providers (explicitly declared binaries/commands)
2. Conventional local lookup (known CLI command names)
3. ACP registry-backed providers

Result is a provider catalog with:

- provider id
- launch strategy
- declared protocol kind (`acp` or `native`)
- capability summary
- trust and policy metadata

### 10.9 Policy and security boundary

Policy enforcement is centralized in host core:

1. provider allow/deny
2. capability allow/deny
3. workspace/path constraints
4. network and command guardrails (per provider class)

Provider adapters must not bypass policy checks.

### 10.10 Reliability model

Per-session state machine (host-level):

`Created -> Starting -> Ready -> Busy -> Cancelling -> Ready -> Stopping -> Stopped`

Error side-branch:

`Starting|Busy -> ErrorRecoverable|ErrorTerminal`

Host behavior:

- restart policy only for recoverable failures
- terminal failure requires explicit new session
- always emit deterministic terminal event for observability

### 10.11 Runtime integration points

`nexus-daemon-runtime` should depend on `nexus-agent-host` through a narrow facade:

1. `host_manager.start(config)`
2. `host_manager.create_session(request)`
3. `host_manager.exec(op)`
4. `host_manager.cancel(op_id)`
5. `host_manager.health()`
6. `host_manager.shutdown()`

This keeps daemon runtime independent from provider-specific details.

### 10.12 Host API surface (daemon-facing)

`nexus-daemon-runtime` should expose host operations via local API routes under a single namespace. Recommended endpoints:

1. `POST /v1/local/agent-host/sessions`
   - create managed session
2. `POST /v1/local/agent-host/sessions/{id}/ops`
   - execute normalized operation
3. `POST /v1/local/agent-host/sessions/{id}/cancel`
   - cancel in-flight operation
4. `POST /v1/local/agent-host/sessions/{id}/shutdown`
   - terminate managed session
5. `GET /v1/local/agent-host/sessions`
   - list managed sessions and health
6. `GET /v1/local/agent-host/providers`
   - list discovered providers and capability summaries

Final route names can change, but one dedicated namespace is mandatory.

### 10.13 Config contract

Host config should live under `~/.nexus42/` and include:

1. provider allowlist/denylist
2. provider launch command templates
3. per-provider timeout/retry caps
4. capability policy toggles
5. max concurrent sessions / max concurrent ops

No implicit global mutable defaults outside this config and runtime flags.

### 10.14 Event and tracing contract

Every operation must emit correlated events:

1. `host.session.created`
2. `host.op.started`
3. `host.op.chunk` (for streaming providers)
4. `host.op.finished`
5. `host.op.failed`
6. `host.session.stopped`

Each event must carry:

- `run_id` (if available)
- `session_id`
- `provider_id`
- `protocol_kind` (`acp` or `native`)
- timestamp

### 10.15 Error taxonomy

Host-visible error categories:

1. `ProviderUnavailable`
2. `LaunchFailed`
3. `CapabilityUnsupported`
4. `PolicyDenied`
5. `OperationTimeout`
6. `OperationCancelled`
7. `ProviderProtocolError`
8. `InternalHostError`

Adapters map provider-native failures into this taxonomy before returning to runtime/API layers.

### 10.16 Acceptance criteria for this spec

This sub-spec is considered ready for implementation when all items are true:

1. Single architecture direction is fixed (Hybrid + Managed-only).
2. Provider scope is fixed (ACP + common agent CLIs).
3. Host/runtime layering is fixed and independent.
4. Mandatory host facade methods are defined.
5. Capability normalization contract is defined.
6. Discovery, policy, and reliability contracts are defined.
7. API namespace and config contract are defined.
8. Event and error contracts are defined.

### 10.17 Rollout strategy (design-level only, no impl plan)

Wave recommendation:

1. Build host core + one ACP provider + one native provider
2. Validate normalized capability and telemetry contracts
3. Add remaining native adapters
4. Harden policy and failure matrix
5. Expose stable daemon APIs for host operations

### 10.18 Non-goals for this sub-spec

1. No unmanaged session attach mode
2. No provider-specific UX optimization in first architecture pass
3. No final command UX freeze in this document
4. No compatibility bridge for old agent execution paths

### 10.19 Open risks to track during implementation

1. Native CLI protocol drift across versions
2. Stream backpressure handling consistency
3. Cancellation semantics mismatch between providers
4. Host policy false positives for provider-specific commands
5. Session cleanup under daemon crash or forced termination

## References

- Multica server patterns: [github.com/multica-ai/multica](https://github.com/multica-ai/multica/tree/main/server)
- OpenDesign apps patterns: [github.com/nexu-io/open-design](https://github.com/nexu-io/open-design/tree/main/apps)
