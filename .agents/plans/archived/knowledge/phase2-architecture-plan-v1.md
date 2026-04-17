# V1.0-phase2 Architecture Plan — V1.1

**Date**: 2026-04-06
**Author**: @architect
**Based on**: V1.0-phase1 Architecture Review V1, V1.0-phase1 Product Review V1
**Status**: Superseded — merged into `v1.1-overview-v1.md`. Key decisions and residual matrix retained as historical reference.

---

## Executive Summary

V1.0-phase2 targets the V1.1 release and focuses on three objectives: (1) hardening the foundation to eliminate structural debt that would compound with new features, (2) completing the ACP SDK bridge so that `nexus42 agent run` actually communicates with AI agents, and (3) filling the most critical skeleton commands — manuscript operations and daemon auto-spawn — so that users can perform a complete creative writing workflow.

The V1.0-phase1 architecture review identified 3 critical runtime bugs (2 already being fixed in parallel by @fullstack-dev on `fix/v1.0-p0-bugs`), 7 high-severity design issues, and 14 medium-severity concerns. The product review found that only 24% of CLI commands are fully functional, with 43% being skeleton implementations. This plan organizes 38 open residuals and 36 review findings into 5 implementation plans with explicit dependency ordering.

The recommended approach is sequential: **Foundation Hardening → ACP SDK Bridge → Daemon + Manuscript Operations → Auth Flow → Codegen Alignment**. Foundation Hardening (Plan A) is a hard prerequisite for Plans B–D. Plans C and D can proceed in parallel once Plan A lands. Plan E (Codegen) is independent and can run at any point, but should land before Plan C to prevent further schema-contract drift.

---

## 1. V1.0-phase2 Goals & Non-Goals

### Goals

1. **Eliminate all CRITICAL and HIGH architectural blockers** identified in the V1.0-phase1 review (CLI-DAEMON-1/2/3/4/5, DEBT-X1..X4, CC-ERR-1, CODEGEN-1/2, SYNC-ARCH-1)
2. **Make ACP agent interaction functional** — `nexus42 agent run <agent>` must send prompts to agents and receive responses via the ACP protocol
3. **Deliver a complete user workflow**: init workspace → create manuscript → manage phases → run agent assistance → sync to platform
4. **Standardize error propagation** across the CLI–daemon boundary with structured error codes and proper HTTP status codes
5. **Unify SQLite schema management** — single source of truth for table definitions, shared between CLI and daemon
6. **Enforce schema-first contract discipline** — all wire types must originate from JSON Schema codegen, no more hand-written DTOs that bypass the pipeline

### Non-Goals

1. **Platform API integration** — sync push/pull requires the private `nexus-platform` repo; only CLI-side preparation is in scope
2. **GUI frontend** — V1.1 remains CLI-only; any GUI work belongs in V2.0
3. **Multi-agent orchestration** — single-agent interaction is the target; multi-agent workflows are V1.2+
4. **Binary distribution** — Homebrew/npm packaging is out of scope for this architecture plan (separate ops effort)
5. **End-user documentation** — docs task is a separate plan; this plan focuses on technical architecture only
6. **`!Send` futures migration to `sacp`** — staying on `agent-client-protocol` v0.10.4 per the ACP tech spec decision

### Success Criteria

| Criterion | Verification |
|-----------|-------------|
| All CRITICAL and HIGH residuals addressed | `status.json` `metadata.residual_findings` updated; severity audit shows 0 open HIGH+ |
| `nexus42 agent run claude-acp` sends prompt, receives response | Integration test: spawn mock agent, send prompt, assert response |
| Daemon returns proper HTTP 4xx/5xx status codes | Test: POST to init with invalid payload → 400, not 200 |
| SQLite schema defined in exactly 1 location | Grep confirms `CREATE TABLE creators` appears in ≤1 non-generated file |
| Codegen generates all types from `context-assembly-v1.schema.json` | `pnpm run codegen` produces `ContextAssembleRequestV1` in both TS and Rust |
| Workspace validation middleware blocks uninitialized operations | Test: call any daemon handler without init → 4xx with structured error |
| All 445+ existing tests continue to pass | `cargo test --all` passes on `main` after each plan merges |

---

## 2. Plan Breakdown

### 2.1 Plan A: Foundation Hardening

**ID**: `2026-04-06-foundation-hardening`
**Effort**: **M** (~2–4 agent sessions)
**Dependencies**: P0 bug fixes must land first (`fix/v1.0-p0-bugs` branch merge)
**Owner**: @fullstack-dev
**Tags**: `V1.0-phase2`, `foundation`, `error-strategy`, `sqlite`

#### Description

Resolve the 4 DEBT-X cross-cutting items and standardize error handling across the CLI–daemon boundary. This plan is the hard prerequisite for all subsequent V1.0-phase2 plans — new features built on top of inconsistent error propagation and duplicated schemas would accumulate unmanageable debt.

#### Key Technical Decisions

| Decision | Options | Recommendation | Rationale |
|----------|----------|----------------|-----------|
| Connection pool library | `deadpool-sqlite`, `r2d2_sqlite`, hand-rolled | **`deadpool-sqlite`** | Most actively maintained; async-native; works with `tokio::runtime` already in use |
| Error strategy | Unified `thiserror`, hybrid (thiserror + anyhow), custom error codes | **Unified `thiserror` + error code enum** | Domain and sync already use `thiserror`; daemon should migrate. Add `NexusErrorCode` enum for API boundary. |
| Schema dedup location | New `nexus-schema` crate, daemon-only (CLI via API), shared module file | **Shared module in existing crate** | Creating a new crate for 3 table definitions is overkill. Add `crates/nexus-contracts/src/schema.rs` (already published as contract layer) or `crates/nexus42/src/db/schema.rs` with daemon importing it. |
| Workspace validation | Axum middleware layer, per-handler checks, tower layer | **Axum middleware (tower layer)** | Catches all routes uniformly; easy to compose; follows Axum best practices |

#### Task Breakdown

**Task 1: Error Strategy Standardization**
- **Files**:
  - Modify: `crates/nexus42d/src/api/errors.rs` (create — currently none)
  - Modify: `crates/nexus42d/src/api/handlers/*.rs` (all 8 handler files)
  - Modify: `crates/nexus42/src/errors.rs`
  - Modify: `crates/nexus42/src/api/daemon_client.rs`
- **Actions**:
  1. Define `NexusApiError` enum in `nexus42d` with structured variants: `Uninitialized`, `InvalidInput { field, reason }`, `Internal { code, message }`, `AuthRequired`, `NotFound`
  2. Implement `IntoResponse` for `NexusApiError` — maps to proper HTTP status codes (400, 401, 404, 409, 500)
  3. Replace all `Json({ success: false, message })` + HTTP 200 with `Err(NexusApiError::variant)`
  4. Add `From<anyhow::Error>` impl that captures `.source()` chain
  5. Update `DaemonClient` in CLI to parse error responses by status code, not just body
  6. Add `From<NexusApiError>` for `CliError` in CLI

**Task 2: SQLite Connection Pooling (DEBT-X1)**
- **Files**:
  - Modify: `crates/nexus42d/Cargo.toml` (add `deadpool-sqlite`)
  - Modify: `crates/nexus42d/src/workspace/mod.rs`
  - Create: `crates/nexus42d/src/db/pool.rs`
  - Modify: `crates/nexus-sync/src/outbox.rs` (optional — sync crate uses direct connections which is acceptable for CLI single-process)
- **Actions**:
  1. Create `DbPool` wrapper around `deadpool::managed::Pool<SqliteConnManager>`
  2. Initialize pool in daemon startup with configurable pool size (default: 8)
  3. Replace `Mutex<Option<Connection>>` with pool `.get()` in `WorkspaceState`
  4. For CLI (`nexus42`): connection pooling is lower priority (single-process, single-threaded). Replace `Mutex` with `RwLock` to resolve CLI-R14 race condition. Connection-per-command is acceptable for CLI in V1.1.
  5. Integration test: concurrent handler requests all succeed

**Task 3: Schema Deduplication (DEBT-X2)**
- **Files**:
  - Create: `crates/nexus42/src/db/schema.rs` (shared schema definitions)
  - Modify: `crates/nexus42/src/commands/creator.rs` (remove inline CREATE TABLE)
  - Modify: `crates/nexus42/src/commands/research.rs` (remove inline CREATE TABLE)
  - Modify: `crates/nexus42d/src/workspace/mod.rs` (remove inline CREATE TABLE)
  - Modify: `crates/nexus-sync/src/outbox.rs` (verify outbox schema matches)
- **Actions**:
  1. Extract all `CREATE TABLE` and `CREATE INDEX` statements to `crates/nexus42/src/db/schema.rs` as `const` strings or a `Schema` struct with `.init(db: &Connection)` method
  2. CLI and daemon both import from this module
  3. Add schema version column (`schema_version INTEGER NOT NULL DEFAULT 1`) for future migration support (partially resolves SYNC-R10)
  4. Integration test: both CLI and daemon create identical schema

**Task 4: Workspace Validation Middleware (DEBT-X4)**
- **Files**:
  - Create: `crates/nexus42d/src/api/middleware.rs`
  - Modify: `crates/nexus42d/src/api/mod.rs`
  - Modify: `crates/nexus42d/src/main.rs`
- **Actions**:
  1. Create `require_workspace` tower middleware layer that checks `WorkspaceState::is_initialized()` before passing to handler
  2. Returns 409 Conflict with `NexusApiError::Uninitialized` if workspace not initialized
  3. Apply to all daemon routes except `/v1/local/workspace/init` and `/v1/local/runtime/health`
  4. Test: unauthenticated/uninitialized requests get proper rejection

**Task 5: Unsafe Unwrap Cleanup (CLI-R13)**
- **Files**:
  - Modify: all command handler files in `crates/nexus42/src/commands/`
  - Modify: `crates/nexus42d/src/api/handlers/*.rs`
- **Actions**:
  1. Audit all `.unwrap()` calls in production code paths
  2. Replace with `.map_err()?`, `.expect("invariant: ...")`, or proper error propagation
  3. Add `#[deny(clippy::unwrap_used)]` to command modules (opt-in, not workspace-wide yet)

#### Residuals Addressed

| Residual ID | Severity | Resolution |
|-------------|----------|------------|
| DEBT-X1 | HIGH | Task 2 — connection pooling |
| DEBT-X2 | HIGH | Task 3 — schema dedup |
| DEBT-X3 | MEDIUM | Task 1 — error strategy |
| DEBT-X4 | HIGH | Task 4 — middleware |
| CLI-R9 | HIGH | Task 2 (daemon pooling) |
| CLI-R10 | HIGH | Task 3 |
| CLI-R11 | HIGH | Task 1 |
| CLI-R12 | HIGH | Task 4 |
| CLI-R13 | HIGH | Task 5 |
| CLI-R14 | HIGH | Task 2 (RwLock for CLI) |
| CLI-R15 | MEDIUM | Task 4 (middleware adds tracing) |
| CC-ERR-1 | HIGH | Task 1 |
| CC-ERR-2 | MEDIUM | Task 1 |
| SYNC-R4 | HIGH | Partially — pool enables concurrent tests; actual test writing deferred to Plan C |
| SYNC-R10 | MEDIUM | Partially — schema version column added in Task 3 |

**Total: 15 residuals addressed (7 HIGH, 4 MEDIUM resolved; 1 HIGH, 3 MEDIUM partially resolved)**

#### Success Criteria
- [ ] `cargo test --all` passes (445+ tests, no regressions)
- [ ] `cargo clippy --all -- -D warnings` clean
- [ ] Daemon returns HTTP 400 for invalid input, 409 for uninitialized workspace, 500 for internal errors
- [ ] `CREATE TABLE creators` grep returns exactly 1 non-test location
- [ ] No `.unwrap()` in CLI or daemon handler production code paths

---

### 2.2 Plan B: ACP SDK Bridge Implementation

**ID**: `2026-04-06-acp-sdk-bridge`
**Effort**: **M–L** (~3–6 agent sessions)
**Dependencies**: Plan A (Foundation Hardening) — needs error strategy to handle ACP errors properly
**Owner**: @fullstack-dev
**Tags**: `V1.0-phase2`, `acp`, `sdk`, `agent-interaction`
**Spec input**: `.agents/plans/archived/knowledge/acp-client-tech-spec-v1.md` (archived 2026-04-17)

#### Description

Implement the actual ACP SDK method calls in `AcpSdkAdapter`, replacing all stub/placeholder methods with real `LocalSet`-bridged SDK invocations. This is the single most impactful feature gap — without this, `nexus42 agent run` spawns agent subprocesses but cannot communicate with them.

#### Key Technical Decisions

| Decision | Options | Recommendation | Rationale |
|----------|----------|----------------|-----------|
| LocalSet bridge architecture | Dedicated OS thread, `tokio::task::spawn_local`, `spawn_blocking` | **Dedicated `std::thread` with `LocalSet`** | SDK futures are `!Send`; they must run on a thread that doesn't move them. A dedicated thread with `LocalSet` + `mpsc` channels is the standard pattern. |
| Channel type for bridge | `tokio::sync::mpsc`, `std::sync::mpsc`, `flume` | **`tokio::sync::mpsc`** (async side) + **`std::sync::mpsc`** (sync side) | The async task needs `tokio::sync` for `.await`; the `LocalSet` thread needs `std::sync` since it can't `.await` on tokio channels from within `spawn_local`. |
| Error handling for SDK calls | String error wrapping, typed ACP error enum, anyhow propagation | **Typed `AcpError` enum + `From<agent_client_protocol::Error>`** | Matches existing `NexusAcpClient` trait pattern; enables structured error recovery |

#### Task Breakdown

**Task 1: LocalSet Bridge Infrastructure**
- **Files**:
  - Create: `crates/nexus42/src/acp/localset_bridge.rs`
  - Modify: `crates/nexus42/src/acp/client.rs`
  - Modify: `crates/nexus42/src/acp/mod.rs`
- **Actions**:
  1. Create `LocalSetBridge` struct:
     - Spawns a `std::thread` running `LocalSet::new().run_until(async { ... })`
     - Exposes `async fn send_request(request) -> Result<Response>` for the tokio side
     - Uses `tokio::sync::mpsc` for request channel, `std::sync::mpsc` for response channel
  2. Implement `Drop` for graceful shutdown (send shutdown signal, join thread)
  3. Integrate with `AcpSdkAdapter` — adapter methods route through bridge
  4. Unit test: bridge starts, processes request, returns response, shuts down cleanly

**Task 2: AcpSdkAdapter.initialize() Implementation**
- **Files**:
  - Modify: `crates/nexus42/src/acp/client.rs`
  - Modify: `crates/nexus42/src/acp/session.rs`
- **Actions**:
  1. Replace stub with actual `ClientSideConnection::create()` call via bridge
  2. Configure `SimpleClientHandler` with Nexus capability set (6 capabilities already defined in ACP tech spec §7)
  3. Return real `InitializedSession` with session ID and agent metadata
  4. Error handling: map SDK errors to `AcpError` variants (ConnectionFailed, ProtocolError, Timeout)
  5. Integration test: initialize against real ACP agent subprocess

**Task 3: AcpSdkAdapter.prompt() Implementation**
- **Files**:
  - Modify: `crates/nexus42/src/acp/client.rs`
  - Modify: `crates/nexus42/src/commands/agent.rs` (interactive_prompt_loop)
- **Actions**:
  1. Replace stub with `ClientSideConnection.send_request(TextMessage { content })` via bridge
  2. Stream responses: handle both immediate and streaming responses from agent
  3. Update `interactive_prompt_loop` to:
     - Read stdin
     - Send via `adapter.prompt()`
     - Await response
     - Print to stdout
  4. Handle ACP notifications (tool requests, permission requests) — V1.1 auto-grants with warning
  5. Integration test: send prompt to mock agent, verify response received

**Task 4: AcpSdkAdapter.subscribe() and Event Handling**
- **Files**:
  - Modify: `crates/nexus42/src/acp/client.rs`
  - Create: `crates/nexus42/src/acp/events.rs`
- **Actions**:
  1. Implement `subscribe()` to forward ACP notifications (state changes, tool use, errors) to the CLI event loop
  2. Replace immediately-closed broadcast channel with functional event stream
  3. Handle tool permission requests: auto-grant with log warning (per V1.1 policy)
  4. Graceful session shutdown: send `CloseSession` request, wait for ack, then kill subprocess

**Task 5: AcpError Integration with Error Strategy**
- **Files**:
  - Modify: `crates/nexus42/src/acp/errors.rs`
  - Modify: `crates/nexus42/src/errors.rs` (add `From<AcpError> for CliError`)
- **Actions**:
  1. Add `From<AcpError>` for `CliError` — stop losing error information through String conversion (resolves CLI-DAEMON-5)
  2. Map ACP errors to user-friendly messages with error codes
  3. Test: ACP connection failure produces actionable error message, not just "Other(...)"

#### Residuals Addressed

| Residual ID | Severity | Resolution |
|-------------|----------|------------|
| ACP-R3 | LOW | Deferred — terminal kill/wait_for_exit is V1.2+ |
| ACP-R4 | LOW | Partially — slash commands not surfaced in V1.1 |
| ACP-R5 | LOW | Partially — agent plan display not implemented |
| ACP-R6 | MEDIUM | Deferred — session persistence requires V1.2 effort |
| ACP-R7 | MEDIUM | Partially — auto-grant with warning implemented |
| ACP-R8 | MEDIUM | Deferred — daemon-mediated tool access is V1.2+ |
| ACP-ARCH-1 | CRITICAL | Tasks 1–4 — full SDK bridge |
| ACP-ARCH-2 | MEDIUM | Task 1 — `RegistryClient::default()` panic fixed |
| ACP-ARCH-3 | MEDIUM | Task 2 — empty query returns None |
| CLI-DAEMON-5 | HIGH | Task 5 — `From<AcpError> for CliError` |
| CC-TEST-1 | MEDIUM | Tasks 1–5 — integration tests added |

**Total: 10 residuals addressed (1 CRITICAL resolved, 2 HIGH resolved, 4 MEDIUM partially resolved, 3 LOW partially resolved)**

#### Success Criteria
- [ ] `nexus42 agent run claude-acp` sends user prompt to agent subprocess
- [ ] Agent response is received and printed to stdout
- [ ] Session shutdown works cleanly (no zombie processes)
- [ ] `Ctrl+C` during agent run triggers graceful shutdown
- [ ] ACP errors produce structured `CliError`, not `CliError::Other(...)`

---

### 2.3 Plan C: Daemon + Manuscript Operations

**ID**: `2026-04-06-daemon-manuscript-ops`
**Effort**: **L** (~4–8 agent sessions, split into 2 milestones)
**Dependencies**: Plan A (Foundation Hardening)
**Owner**: @fullstack-dev
**Tags**: `V1.0-phase2`, `daemon`, `manuscript`, `auto-spawn`

#### Description

Fill the most critical skeleton commands: daemon auto-spawn, manuscript lifecycle operations (create, edit, export, phase management), and sync status with real data. This plan delivers the first complete user workflow — from workspace init to manuscript export.

#### Milestone 1: Daemon Auto-Spawn and Lifecycle

**Task 1: Daemon Auto-Spawn**
- **Files**:
  - Modify: `crates/nexus42/src/commands/daemon.rs`
  - Create: `crates/nexus42/src/daemon/spawner.rs`
  - Modify: `crates/nexus42/Cargo.toml` (if needed)
- **Actions**:
  1. Implement `spawn_daemon(config: &CliConfig) -> Result<DaemonHandle>`:
     - Locate `nexus42d` binary (same directory as `nexus42`, or via `cargo build -p nexus42d`)
     - Spawn as background process with stdout/stderr logged to `.nexus42/daemon.log`
     - Store PID in `.nexus42/daemon.pid`
     - Wait for health check (up to 5s with polling)
  2. Implement `stop_daemon()`: read PID, send SIGTERM, wait 2s, then SIGKILL
  3. Update `daemon start` command to call auto-spawn instead of printing instructions
  4. Add `--port` and `--foreground` flags
  5. Integration test: spawn daemon, verify health check passes, stop daemon

**Task 2: Daemon Context Assembly Route**
- **Files**:
  - Modify: `crates/nexus42d/src/api/mod.rs`
  - Create: `crates/nexus42d/src/api/context.rs`
- **Actions**:
  1. Register `POST /v1/local/context/assemble` route (resolves CLI-DAEMON-2)
  2. Handler accepts `ContextAssembleRequestV1` (use codegen'd types once Plan E lands, or hand-written DTO temporarily)
  3. Returns context summary from local workspace data
  4. Integration test: CLI context assemble calls daemon, receives 200

**Task 3: Daemon Timeout Configuration**
- **Files**:
  - Modify: `crates/nexus42/src/api/daemon_client.rs`
- **Actions**:
  1. Add configurable timeout to `DaemonClient` (default: 10s connect, 30s request)
  2. Replace `reqwest::Client::new()` with `reqwest::Client::builder().timeout(...).build()`
  3. Test: hung daemon returns timeout error, not infinite hang

#### Milestone 2: Manuscript Operations

**Task 4: Manuscript File Operations**
- **Files**:
  - Modify: `crates/nexus42/src/commands/manuscript.rs`
  - Create: `crates/nexus42/src/manuscript/manager.rs`
  - Create: `crates/nexus42/src/manuscript/export.rs`
- **Actions**:
  1. Implement `manuscript create <title>` — creates `Stories/<title>/` directory with `manuscript.md` and metadata
  2. Implement `manuscript edit <title>` — opens manuscript in `$EDITOR` or prints content
  3. Implement `manuscript export <title> --format=markdown|plain` — exports manuscript with metadata header
  4. Phase management: `manuscript phase <title> <phase>` stores phase in workspace SQLite (using domain `ManuscriptState` aggregate)
  5. `manuscript status` reads from workspace SQLite (resolves the "no workspace initialized" bug — caused by CLI-DAEMON-1 which is being fixed in parallel)
  6. `manuscript promote` — validates phase transitions using domain invariants
  7. `manuscript verify` — validates metadata consistency using domain `consistency.rs` rules

**Task 5: Sync Status with Real Data**
- **Files**:
  - Modify: `crates/nexus42/src/commands/sync.rs`
  - Modify: `crates/nexus42d/src/api/sync.rs` (if exists, create if not)
- **Actions**:
  1. Implement `sync status` using outbox state:
     - Pending bundles count (from outbox)
     - Last successful sync timestamp
     - Unresolved conflicts
  2. Register daemon route `GET /v1/local/sync/status`
  3. CLI calls daemon for status (no longer returns "—" for all fields)
  4. Test: create bundle in outbox, verify `sync status` shows pending count

#### Residuals Addressed

| Residual ID | Severity | Resolution |
|-------------|----------|------------|
| CLI-DAEMON-2 | CRITICAL | Task 2 — register context assembly route |
| CLI-DAEMON-8 | MEDIUM | Task 3 — timeout configuration |
| CLI-R6 | LOW | Task 4 — manuscript promote preconditions via domain invariants |
| CLI-R7 | LOW | Deferred — content hash verification is V1.2+ |
| CLI-R8 | LOW | Deferred — PDF/URL extraction is V1.2+ |
| DM-1 | LOW | Not addressed — MembershipPermissionCheck extraction; V1.2 |
| CTX-R4 | HIGH | Handled via clap validator in Task 4 (world_id format) |
| CTX-R5 | HIGH | Task 4 — UTF-8 safety in content handling |
| SYNC-R7 | HIGH | Not addressed — requires daemon auth context (Plan D dependency) |
| SYNC-R10 | MEDIUM | Partially — addressed via Plan A schema version column |

**Total: 9 residuals addressed (2 CRITICAL, 2 HIGH resolved; 1 HIGH, 1 MEDIUM partially resolved; 3 LOW deferred)**

#### Success Criteria
- [ ] `nexus42 daemon start` spawns daemon in background and returns PID
- [ ] `nexus42 daemon stop` terminates daemon cleanly
- [ ] `nexus42 manuscript create "My Novel"` creates directory structure
- [ ] `nexus42 manuscript phase "My Novel" draft` persists to SQLite
- [ ] `nexus42 manuscript status` displays current phase (not "no workspace")
- [ ] `nexus42 sync status` shows real outbox data

---

### 2.4 Plan D: Auth Flow Completion

**ID**: `2026-04-06-auth-flow`
**Effort**: **M** (~2–3 agent sessions)
**Dependencies**: Plan A (Foundation Hardening) — needs error strategy for auth failures
**Owner**: @fullstack-dev
**Tags**: `V1.0-phase2`, `auth`, `oauth`, `daemon`

#### Description

Complete the device code OAuth flow for user authentication, implement token refresh and lifecycle management, and add auth middleware to the daemon. This enables the full user authentication workflow required for platform-dependent features.

#### Key Technical Decisions

| Decision | Options | Recommendation | Rationale |
|----------|----------|----------------|-----------|
| Auth state storage | Shared crate, CLI-only, daemon-only | **Daemon owns auth state** | Per architecture review recommendation §9.4.3: "Daemon owns all SQLite state." Auth tokens stored in daemon SQLite, CLI reads via API. |
| OAuth device flow | Standard RFC 8628, custom | **RFC 8628** | Standard device authorization grant; platform must implement the matching server side |
| Token refresh strategy | Lazy refresh on 401, proactive refresh timer, hybrid | **Lazy refresh on 401 + proactive refresh if expiry < 5min** | Simple to implement; proactive refresh avoids latency spikes |

#### Task Breakdown

**Task 1: Device Code OAuth Flow**
- **Files**:
  - Modify: `crates/nexus42/src/commands/auth.rs`
  - Modify: `crates/nexus42/src/auth/mod.rs`
  - Modify: `crates/nexus42d/src/auth/mod.rs`
- **Actions**:
  1. Implement `auth login` as RFC 8628 device authorization grant:
     - POST to platform `/oauth/device_authorization` → get device code + verification URI
     - Display verification URI and user code
     - Poll `/oauth/token` with device code until user authorizes
     - Store access_token + refresh_token + expires_at in daemon SQLite
  2. CLI calls daemon API for auth operations (not local SQLite directly)
  3. Test: mock platform endpoints, verify full device flow

**Task 2: Token Lifecycle Management**
- **Files**:
  - Create: `crates/nexus42d/src/auth/token_manager.rs`
  - Modify: `crates/nexus42d/src/auth/mod.rs`
- **Actions**:
  1. `TokenManager::refresh_if_expired()` — checks `expires_at`, calls `/oauth/token` with refresh grant if < 5min remaining
  2. `TokenManager::get_valid_token()` — always returns a valid (non-expired) access token
  3. Store token metadata in SQLite: `auth_tokens` table (user_id, access_token, refresh_token, expires_at, created_at)
  4. Test: expired token triggers refresh, valid token is returned directly

**Task 3: Daemon Auth Middleware**
- **Files**:
  - Create: `crates/nexus42d/src/api/auth_middleware.rs`
  - Modify: `crates/nexus42d/src/api/mod.rs`
- **Actions**:
  1. Create `require_auth` tower middleware layer:
     - Extracts `Authorization: Bearer <token>` header
     - Validates token against stored tokens
     - Injects `AuthenticatedUser` into request extensions
     - Returns 401 if missing/invalid
  2. Apply to all routes except health check and workspace init
  3. Test: unauthenticated request → 401; authenticated request → handler

**Task 4: CLI Auth Client Update**
- **Files**:
  - Modify: `crates/nexus42/src/api/daemon_client.rs`
  - Modify: `crates/nexus42/src/auth/store.rs`
- **Actions**:
  1. CLI `auth status` calls daemon for token state (not local file)
  2. CLI `auth logout` calls daemon to revoke/clear tokens
  3. CLI commands that need auth automatically check auth status and prompt login if needed
  4. Resolve ARCH-1 (no shared auth types): daemon defines `UserSession`, CLI reads via API, no need for shared crate

#### Residuals Addressed

| Residual ID | Severity | Resolution |
|-------------|----------|------------|
| ARCH-1 | MEDIUM | Task 4 — daemon owns auth state, CLI reads via API |
| SYNC-R6 | HIGH | Task 3 — auth middleware enables precheck auth validation |
| SYNC-R9 | MEDIUM | Task 2 — token validation ensures auth token format |

**Total: 3 residuals addressed (1 HIGH resolved, 2 MEDIUM resolved)**

#### Success Criteria
- [ ] `nexus42 auth login` completes device code flow (with mock platform)
- [ ] Token refresh works automatically on expiry
- [ ] Daemon returns 401 for unauthenticated requests to protected routes
- [ ] `nexus42 auth status` shows token expiry time
- [ ] `nexus42 auth logout` clears tokens in daemon

---

### 2.5 Plan E: Codegen & Contract Alignment

**ID**: `2026-04-06-codegen-alignment`
**Effort**: **S** (~1–2 agent sessions)
**Dependencies**: None (can run in parallel with Plans A–D)
**Owner**: @fullstack-dev
**Tags**: `V1.0-phase2`, `codegen`, `contracts`, `schema-first`

#### Description

Fix the codegen pipeline to properly drive CommonTypes generation from schema definitions instead of hard-coded lists, and generate types for `context-assembly-v1.schema.json`. This enforces the schema-first contract discipline and prevents future drift between schemas and generated types.

#### Task Breakdown

**Task 1: Fix CommonTypes Codegen (CODEGEN-1)**
- **Files**:
  - Modify: `tooling/codegen/ts-generator.ts`
  - Modify: `tooling/codegen/rust-generator.ts`
- **Actions**:
  1. Replace hard-coded type lists in `generateCommonTypesFile()` with iteration over `COMMON_DEFINITIONS` map
  2. Both TS and Rust generators: for each entry in `COMMON_DEFINITIONS`, emit the corresponding type definition
  3. Run `pnpm run codegen` and verify output matches expected types
  4. Compare git diff: generated types should be identical (no regression) since the hard-coded list happened to match
  5. Test: add a new type to `common.schema.json`, run codegen, verify it appears in both TS and Rust output

**Task 2: Generate Context Assembly Types (CODEGEN-2)**
- **Files**:
  - Modify: `tooling/codegen/ts-generator.ts`
  - Modify: `tooling/codegen/rust-generator.ts`
  - Modify: `schemas/platform/context-assembly-v1.schema.json` (if needed — verify structure)
- **Actions**:
  1. Fix the `isDefinitionsOnly` check: schemas with only `$defs` / `definitions` but no top-level `properties` should still generate types for each definition
  2. Generate `ContextAssembleRequestV1` and `ContextAssembleResponseV1` in both TypeScript and Rust
  3. Verify generated types match the hand-written versions in `crates/nexus42/src/context/types.rs`
  4. Replace hand-written types in `crates/nexus42/src/context/types.rs` with generated imports
  5. Run `pnpm run codegen && cargo test --all` — no regressions

**Task 3: Registry Manifest Types (CODEGEN-3)**
- **Files**:
  - Modify: `tooling/codegen/rust-generator.ts`
  - Modify: `schemas/acp-runtime/registry-manifest.schema.json`
- **Actions**:
  1. Generate named types for nested definitions in registry manifest (`AgentEntry`, `Distribution`, `NpxDistribution`)
  2. Replace `serde_json::Value` usage in `crates/nexus42/src/acp/registry.rs` with generated types
  3. Run codegen + tests — no regressions

#### Residuals Addressed

| Residual ID | Severity | Resolution |
|-------------|----------|------------|
| CODEGEN-1 | HIGH | Task 1 — CommonTypes from schema |
| CODEGEN-2 | HIGH | Task 2 — context assembly types |
| CODEGEN-3 | MEDIUM | Task 3 — registry manifest types |
| CTX-R6 | MEDIUM | Task 2 — generated MemoryKind from schema |
| DM-R5 | MEDIUM | Task 2 — alignment check exposes MembershipRole enum drift |

**Total: 5 residuals addressed (2 HIGH resolved, 3 MEDIUM resolved)**

#### Success Criteria
- [ ] `pnpm run codegen` produces `ContextAssembleRequestV1` in both TS and Rust
- [ ] Adding a type to `common.schema.json` and running codegen produces it without codegen source changes
- [ ] `crates/nexus42/src/context/types.rs` imports from `nexus-contracts`, not hand-written
- [ ] `cargo test --all` passes after type replacement

---

## 3. Dependency Graph

```
P0 Bug Fixes (fix/v1.0-p0-bugs)
├── CLI-DAEMON-1: workspace init bug
├── CLI-DAEMON-2: missing context route
└── daemon auto-spawn
    │
    ▼
┌─────────────────────────────────────────────────────────┐
│ Plan A: Foundation Hardening                             │
│ (error strategy, SQLite pooling, schema dedup,           │
│  workspace validation, unwrap cleanup)                   │
│ Effort: M (2–4 sessions)                                 │
└────────────┬──────────────┬────────────────┬─────────────┘
             │              │                │
             ▼              ▼                ▼
    ┌─────────────┐ ┌─────────────┐  ┌──────────────┐
    │ Plan B:     │ │ Plan C:     │  │ Plan E:      │
    │ ACP SDK     │ │ Daemon +    │  │ Codegen &    │
    │ Bridge      │ │ Manuscript  │  │ Contract     │
    │             │ │ Ops         │  │ Alignment    │
    │ M–L         │ │ L           │  │ S            │
    │ (3–6 sess)  │ │ (4–8 sess)  │  │ (1–2 sess)   │
    └──────┬──────┘ └──────┬──────┘  └──────────────┘
           │               │
           │               ▼
           │        ┌─────────────┐
           └───────▶│ Plan D:     │
                    │ Auth Flow   │
                    │             │
                    │ M           │
                    │ (2–3 sess)  │
                    └─────────────┘

LEGEND:
──▶  hard dependency (must complete first)
┈┈▶  soft dependency (recommended but not blocking)
```

### Parallelization Opportunities

| Window | Parallel Plans | Notes |
|--------|---------------|-------|
| **During P0 fixes** | Plan E (Codegen) | No dependencies on bug fixes; can start immediately |
| **After Plan A** | Plans B + C | Both depend only on Plan A; can run in parallel |
| **After Plan C milestone 1** | Plan D | Auth flow needs daemon infrastructure from Plan C |
| **Plan E completion** | Unblocks CTX-R6 in Plan C | Generated MemoryKind type can replace hand-written |

### Recommended Execution Order

1. **Week 1**: P0 bug fixes (parallel) + Plan E (Codegen) — ~2–3 sessions
2. **Week 2**: Plan A (Foundation Hardening) — ~2–4 sessions
3. **Week 3–4**: Plans B + C in parallel — ~3–8 sessions each
4. **Week 5**: Plan D (Auth Flow) — ~2–3 sessions
5. **Week 6**: Integration testing and cross-plan validation — ~1–2 sessions

---

## 4. Residual Integration Matrix

### 4.1 All 38 Open Residuals — Coverage by Plan

| # | Residual ID | Severity | Source Plan | V1.0-phase2 Plan | Resolution |
|---|------------|----------|-------------|--------------|------------|
| 1 | DEBT-X1 | HIGH | cross-cutting | A | Resolved — connection pooling |
| 2 | DEBT-X2 | HIGH | cross-cutting | A | Resolved — schema dedup |
| 3 | DEBT-X3 | MEDIUM | cross-cutting | A | Resolved — error strategy |
| 4 | DEBT-X4 | HIGH | cross-cutting | A | Resolved — middleware |
| 5 | DM-R3 | LOW | domain-models | — | **Deferred to V1.2** — edge-case tests |
| 6 | DM-R5 | MEDIUM | domain-models | E | Resolved — enum alignment exposed by codegen |
| 7 | CLI-R5 | LOW | cli-daemon-foundation | — | **Deferred to V1.2** — Unix socket |
| 8 | CLI-R6 | LOW | cli-daemon-foundation | C | Partially — strict mode via domain invariants |
| 9 | CLI-R7 | LOW | cli-daemon-foundation | — | **Deferred to V1.2** — content hash verify |
| 10 | CLI-R8 | LOW | cli-daemon-foundation | — | **Deferred to V1.2** — PDF/URL extraction |
| 11 | CLI-R9 | HIGH | cli-daemon-foundation | A | Resolved — connection pooling |
| 12 | CLI-R10 | HIGH | cli-daemon-foundation | A | Resolved — schema dedup |
| 13 | CLI-R11 | HIGH | cli-daemon-foundation | A | Resolved — error propagation |
| 14 | CLI-R12 | HIGH | cli-daemon-foundation | A | Resolved — workspace validation |
| 15 | CLI-R13 | HIGH | cli-daemon-foundation | A | Resolved — unwrap cleanup |
| 16 | CLI-R14 | HIGH | cli-daemon-foundation | A | Resolved — RwLock replacement |
| 17 | CLI-R15 | MEDIUM | cli-daemon-foundation | A | Resolved — tracing via middleware |
| 18 | SYNC-R4 | HIGH | sync-contract | A | Partially — pool enables tests |
| 19 | SYNC-R5 | HIGH | sync-contract | — | **Deferred to V1.1+ sync activation** — needs platform API |
| 20 | SYNC-R6 | HIGH | sync-contract | D | Resolved — auth middleware enables precheck validation |
| 21 | SYNC-R7 | MEDIUM | sync-contract | A | Partially — schema version column |
| 22 | SYNC-R8 | MEDIUM | sync-contract | — | **Deferred to V1.2** — schema allOf verification |
| 23 | SYNC-R9 | MEDIUM | sync-contract | D | Resolved — token validation |
| 24 | SYNC-R10 | MEDIUM | sync-contract | A | Partially — schema version column |
| 25 | SYNC-R11 | MEDIUM | sync-contract | — | **Deferred to V1.1+ sync activation** — needs platform API |
| 26 | SYNC-R12 | MEDIUM | sync-contract | — | **Deferred to V1.1+ sync activation** |
| 27 | SYNC-R13 | MEDIUM | sync-contract | — | **Deferred to V1.1+ sync activation** |
| 28 | ACP-R3 | LOW | acp-client | — | **Deferred to V1.2** |
| 29 | ACP-R4 | LOW | acp-client | B | Partially — not surfaced in CLI UX |
| 30 | ACP-R5 | LOW | acp-client | — | **Deferred to V1.2** |
| 31 | ACP-R6 | MEDIUM | acp-client | — | **Deferred to V1.2** |
| 32 | ACP-R7 | MEDIUM | acp-client | B | Partially — auto-grant implemented |
| 33 | ACP-R8 | MEDIUM | acp-client | — | **Deferred to V1.2** |
| 34 | ACP-R9 | LOW | acp-client | — | **Deferred to V1.2** |
| 35 | ACP-R10 | LOW | acp-client | — | **Deferred to V1.2** |
| 36 | ACP-R11 | LOW | acp-client | — | **Deferred to V1.2** |
| 37 | CTX-R2 | LOW | context-assembly | — | **Deferred to V1.2** |
| 38 | CTX-R3 | LOW | context-assembly | — | **Accept** — local dirs under user control |
| 39 | CTX-R4 | HIGH | context-assembly | C | Resolved — clap validator |
| 40 | CTX-R5 | HIGH | context-assembly | C | Resolved — UTF-8 safety tests |
| 41 | CTX-R6 | MEDIUM | context-assembly | E | Resolved — generated enum from schema |
| 42 | CTX-R7 | MEDIUM | context-assembly | — | **Deferred to V1.2** |
| 43 | SYNC-R8 | MEDIUM | sync-contract | — | **Deferred to V1.2** — schema allOf |

### 4.2 Summary

| Category | Count | Notes |
|----------|-------|-------|
| **Resolved in V1.0-phase2** | 20 | All CRITICAL (3), most HIGH (9 of 11), several MEDIUM (4) |
| **Partially resolved** | 8 | Addressed component but full resolution needs platform API or V1.2 work |
| **Deferred to V1.2** | 14 | LOW severity, feature gaps requiring V1.2 scope (multi-agent, advanced sync) |
| **Deferred to V1.1+ sync activation** | 4 | SYNC-R5, R11, R12, R13 — need platform API endpoint |
| **Accept as-is** | 1 | CTX-R3 — path traversal not a risk for local user-controlled dirs |

**Net reduction**: 38 open → 14 deferred + 1 accepted = **15 open after V1.0-phase2** (all LOW or MEDIUM with clear deferral rationale)

---

## 5. Architecture Risk Mitigation

### 5.1 ACP SDK Breaking Changes

**Risk**: `agent-client-protocol` v0.10.4 may receive breaking updates or be superseded by `sacp` v1.0.

**Mitigation**:
- Adapter pattern (`NexusAcpClient` trait) isolates SDK usage to `AcpSdkAdapter` — one file to change
- Pin exact version: `agent-client-protocol = "=0.10.4"` in workspace `Cargo.toml`
- When migration is needed: create `SacpSdkAdapter` implementing the same trait; swap at the factory level
- **Acceptance test**: all ACP integration tests run against the pinned SDK version; CI fails on version drift

### 5.2 SQLite Concurrency Under Sync Daemon Loops

**Risk**: Multiple concurrent requests (sync loops, daemon handlers, CLI commands) may cause `SQLITE_BUSY` errors with the current per-request connection pattern.

**Mitigation**:
- Plan A Task 2 (DEBT-X1) resolves this with `deadpool-sqlite` connection pool
- Pool size configurable (default 8); WAL mode enabled for better read concurrency
- Remaining risk: sync daemon's `unchecked_transaction()` bypasses `BEGIN IMMEDIATE` — document as single-writer assumption (resolves SYNC-ARCH-2 documentation concern)
- **Rollback**: If `deadpool-sqlite` proves problematic, fallback to `r2d2_sqlite` (less async-native but battle-tested)

### 5.3 Schema-First Contract Enforcement

**Risk**: Developers continue hand-writing DTOs that bypass codegen, causing drift between schemas and implementation.

**Mitigation**:
- Plan E fixes the codegen pipeline to generate all necessary types
- CI `verify-codegen` job already catches drift (runs `pnpm run codegen` + `git diff`)
- Add `grep -r "serde_json::Value" crates/nexus42/src/ crates/nexus42d/src/` to CI — flag opaque JSON usage as review item
- Document in `CONTRIBUTING.md`: "All wire types MUST come from `schemas/` via codegen. Hand-written DTOs require ADR approval."

### 5.4 Error Strategy Standardization

**Risk**: New features perpetuate the existing inconsistency (anyhow in daemon, thiserror in sync, mixed in CLI).

**Mitigation**:
- Plan A Task 1 establishes the canonical error strategy: `NexusApiError` in daemon, structured `CliError` in CLI
- Add `#[warn(clippy::unwrap_used)]` to daemon crate (progressive enforcement)
- QC checklist: new plans must document error propagation path for each new handler
- **Rollback**: If full migration proves too disruptive for V1.1, scope down to API boundary only (handler return types) and leave internal daemon code on anyhow

### 5.5 Context Assembly Types Violating Schema-First Contract

**Risk**: CLI hand-wrote `ContextAssembleRequestV1` and related types; if Plan E generates different shapes, there will be a migration gap.

**Mitigation**:
- Plan E Task 2 is designed to replace hand-written types with generated imports
- Step 4 of Task 2 explicitly compares generated types against existing hand-written versions
- If shapes differ: update schema to match the hand-written types (which are the "working" contract), then regenerate
- **Rollback**: If schema change is too large, keep hand-written types temporarily with `#[deprecated]` annotation pointing to generated version

### 5.6 Platform API Contract Drift

**Risk**: CLI expects daemon routes that don't exist (proven by CLI-DAEMON-2); similar drift may occur with platform API.

**Mitigation**:
- Plan C Task 2 registers the missing context assembly route
- For platform API: this repo only defines the contract (schemas); platform implementation validates against schemas
- Add API contract tests: for each daemon route, an integration test verifies the route exists and returns expected shape
- **Not in scope**: platform-side validation (private repo)

---

## 6. Effort Summary

| Plan | ID | Complexity | Agent Sessions | Dependencies |
|------|-----|-----------|----------------|--------------|
| **A: Foundation Hardening** | `2026-04-06-foundation-hardening` | **M** | 2–4 | P0 bug fixes |
| **B: ACP SDK Bridge** | `2026-04-06-acp-sdk-bridge` | **M–L** | 3–6 | Plan A |
| **C: Daemon + Manuscript Ops** | `2026-04-06-daemon-manuscript-ops` | **L** | 4–8 (2 milestones) | Plan A |
| **D: Auth Flow** | `2026-04-06-auth-flow` | **M** | 2–3 | Plan A (hard), Plan C (soft) |
| **E: Codegen Alignment** | `2026-04-06-codegen-alignment` | **S** | 1–2 | None |
| **Total (sequential)** | — | — | **12–23 sessions** | — |
| **Total (parallel)** | — | — | **~8–14 sessions** | With parallelization |

### Assumptions
- P0 bug fixes are complete before Plan A starts (in progress on `fix/v1.0-p0-bugs` by @fullstack-dev)
- Plans B and C can run in parallel after Plan A
- Plan E is independent and runs in parallel with everything
- Each "session" is one coherent agent run: read context → implement → run tests → commit
- QC/QA iterations are included in session count (add ~1 session per plan for review cycle)
- No external blockers (platform API not needed for Plans A–E)

---

## Appendix A: Reference Documents

| Document | Path | Relevance |
|----------|------|-----------|
| V1.0-phase1 Architecture Review | `.agents/plans/archived/knowledge/phase1-architecture-review-v1.md` | Primary input — 36 findings |
| V1.0-phase1 Product Review | `.agents/plans/archived/knowledge/phase1-product-review-v1.md` | Feature completeness and UX assessment |
| ACP Client Tech Spec | `.agents/plans/archived/knowledge/acp-client-tech-spec-v1.md` (archived 2026-04-17) | Detailed ACP SDK integration design |
| Plan Status (residuals) | `.agents/plans/status.json` | 38 open residuals, metadata |
| Knowledge Index | `.agents/plans/knowledge/README.md` | Index of all knowledge documents |
| Project AGENTS.md | `AGENTS.md` | Development workflow, constraints, CI |
| Effort Estimation Guide | `~/.config/opencode/docs/agents/effort-estimation.md` | Agent effort sizing methodology |

## Appendix B: P0 Bug Fix Dependencies

The following bugs are being fixed in parallel by @fullstack-dev on `fix/v1.0-p0-bugs`. V1.0-phase2 plans MUST NOT start until these are merged to `main`:

| Bug | Finding ID | Status | Impact on V1.0-phase2 |
|-----|-----------|--------|-------------------|
| Workspace init doesn't set `workspace_path` | CLI-DAEMON-1 [CRITICAL] | In progress | Plan A Task 4 (validation middleware) and Plan C Task 4 (manuscript status) depend on correct init |
| Missing `/v1/local/context/assemble` route | CLI-DAEMON-2 [CRITICAL] | In progress | Plan C Task 2 will register the route — coordinate to avoid merge conflict |
| Daemon auto-spawn | Product Review §4 | In progress | Plan C Task 1 — coordinate implementation approach |

## Appendix C: Architecture Principles for V1.0-phase2 (from Architecture Review §9.4)

These principles, established during the V1.0-phase1 review, govern all V1.0-phase2 design decisions:

1. **Every new type must come from schemas** — no more hand-written DTOs that bypass codegen
2. **Error types must be structured** — no more `anyhow` at API boundaries
3. **Daemon owns all SQLite state** — CLI accesses workspace data through daemon API
4. **HTTP status codes must be meaningful** — proper 4xx/5xx, not always 200
5. **Auth tokens must have lifecycle management** — refresh logic before V1.1 ships
