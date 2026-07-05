# ACP Client Integration — Technical Specification v2

**Author**: @architect (v1) + @project-manager / @architect (v2 amendments, 2026-04-17)
**Supersedes**: [acp-client-tech-spec-legacy.md](archived/knowledge/acp-client-tech-spec-legacy.md) — archived 2026-04-17
**Status**: Active — authoritative for V1.4 Orchestration track; v1 decisions remain valid unless explicitly amended below.
**Scope**: SDK selection, crate boundary, registry integration, CLI command design, capability IDs, schema definitions, **worker-delegated orchestration hosting (new)**.
**Coordinates with**:

- [orchestration-engine.md](../../knowledge/specs/orchestration-engine.md) — sibling spec for orchestration engine (this doc's §11 describes the crate it extracts; §4 describes the Local API it exposes for engine control)
- [daemon-lifecycle-api.md](daemon-lifecycle-api.md) — worker graceful shutdown timing sourced from lifecycle `Stopping.entry`

---

## How to read this document

- **Sections 1–10**: unchanged from v1 in *intent and decisions* (SDK choice, registry caching, capability IDs, CLI interactive commands). **This document only restates the points that changed** in §2.3, §4, §5, and **Appendix B**. For everything else, treat [acp-client-tech-spec-legacy.md](archived/knowledge/acp-client-tech-spec-legacy.md) (archived) as the authoritative wording and use the "v2 delta" table in §0 to identify what was touched.
- **Section 11 (new)**: defines the `crates/nexus-acp-host` extraction.
- **Appendix B**: residual list updated.

If v1 and v2 disagree on the same topic, **v2 wins** (supersession).

---

## 0. Change Log (v1 → v2)

| Area                          | Kind     | Summary                                                                                                    |
| ----------------------------- | -------- | ---------------------------------------------------------------------------------------------------------- |
| §1 SDK Selection              | unchanged | `agent-client-protocol = "=0.10.4"` + adapter trait stay                                                  |
| §2.1 High-level architecture  | amended  | Diagram adds per-creator ACP worker subprocess owned by daemon; daemon still not linking ACP SDK          |
| §2.2 Module layout            | amended  | ACP modules extracted out of `crates/nexus42` into new `crates/nexus-acp-host` (§11)                      |
| §2.3 Process model            | **revised** | V1.0 rule "CLI spawns agents directly, daemon not involved" still holds for **interactive** `agent run`; orchestration-driven sessions add the **worker-delegated** path |
| §2.4 Connection management    | amended  | `AcpSession` now also exists inside a worker; daemon tracks worker PID + session id for observability      |
| §2.5 Dependency on daemon      | **revised** | V1.0 "no daemon involvement" narrowed: **still none for interactive**; orchestration path has explicit daemon ↔ worker IPC (spec in [orchestration-engine.md](../../knowledge/specs/orchestration-engine.md) §6) |
| §3 Registry                    | unchanged | Registry fetch + cache live in `nexus-acp-host`; still called from CLI + worker                           |
| §4 Local API                   | amended  | Adds `/v1/local/orchestration/...` endpoints for engine control; `/v1/local/acp/...` remain deferred except where orchestration needs them |
| §5 Skills / Capabilities       | amended  | V1.0 frozen capability IDs unchanged; V1.4 **adds** `session.persistence` and `session.modes` as newly declared in worker-delegated sessions (previously ACP-R6/R11 deferred) |
| §6 CLI commands                | unchanged | `agent list / show / probe / run` stay in CLI and continue using `nexus-acp-host` directly                |
| §7 Schemas                     | unchanged | `schemas/acp-runtime/registry-manifest.schema.json` moves crate home but stays wire-level                 |
| §8 ACP-R1/R2                   | unchanged | Both resolved in v1                                                                                       |
| §9 Test strategy               | amended  | Adds IPC-layer integration tests (daemon ↔ worker) to §9.2                                                |
| §10 Task breakdown             | superseded| See [orchestration-engine.md](../../knowledge/specs/orchestration-engine.md) §10 (Phase 1–4 is the new work plan)          |
| §11 `nexus-acp-host` crate     | **new**  | Crate extraction plan: files, re-exports, linkage matrix, compatibility bridge                           |
| Appendix B Residuals           | amended  | ACP-R6 (session persistence), ACP-R7 (permission policy), ACP-R8 (daemon-mediated tool access), ACP-R11 (session.modes) statuses updated |

---

## §2.3 (revised) — Process model with worker-delegated hosting

### 2.3a Two execution paths, one SDK stack

Two code paths now consume `nexus-acp-host`, running in two different processes but **sharing the same crate** and the same adapter pattern:

1. **Interactive path** — `nexus42 agent run <agent-ref>` (unchanged from v1):
   - Spawned as a one-shot CLI subprocess by the user
   - Holds its own `tokio::task::LocalSet`
   - Talks to agent directly over stdin/stdout JSON-RPC
   - Exits when the interactive session ends
   - **Daemon not involved** in the ACP data path

2. **Orchestration path (new)** — `nexus42 acp-worker --creator <id>` spawned by `nexus42d`'s Worker Manager:
   - Long-lived CLI subprocess, one per active creator
   - Also holds its own `tokio::task::LocalSet` (LocalSet contagion **does not cross process boundaries**, so daemon's axum multi-thread runtime is unaffected)
   - Talks to agent directly over stdin/stdout JSON-RPC (same ACP flow as interactive)
   - Receives prompts / cancellations from daemon via a **separate** stdin/stdout JSON-RPC channel (parent pipes to daemon — see [orchestration-engine.md](../../knowledge/specs/orchestration-engine.md) §6.3)
   - Hosts exactly one ACP agent subprocess at a time in MVP; agent switch requires worker restart (§6.2)

### 2.3b Updated architecture diagram (supersedes v1 §2.1)

```
┌──────────────────────────────┐      ┌─────────────────────────────┐
│          nexus42d            │      │         nexus42 CLI         │
│                              │      │ (interactive; user-invoked) │
│ ┌────────────────────────┐   │      │                             │
│ │ Orchestration Engine    │   │      │  agent run/list/show/probe │
│ │ + Worker Manager        │   │      │       │                    │
│ └───────────┬────────────┘   │      │       ▼                    │
│             │ stdin/stdout    │      │  ┌────────────────────┐   │
│             │ JSON-RPC 2.0    │      │  │ nexus-acp-host lib │   │
│             ▼                 │      │  │ (linked here too)  │   │
│     ┌───────────────────┐     │      │  └──────────┬─────────┘   │
│     │ nexus42 acp-worker│     │      │             │ stdio        │
│     │ (long-lived child)│     │      │             ▼              │
│     │  ┌──────────────┐ │     │      │   ┌───────────────────┐   │
│     │  │nexus-acp-host│ │     │      │   │   Agent subprocess│   │
│     │  │   LocalSet   │ │     │      │   │(Claude/Codex/etc.)│   │
│     │  └──────┬───────┘ │     │      │   └───────────────────┘   │
│     └─────────┼─────────┘     │      └─────────────────────────────┘
│               │ stdio          │
│               ▼                │
│       ┌───────────────┐        │
│       │Agent subproc. │        │
│       │(Claude/etc.)  │        │
│       └───────────────┘        │
└──────────────────────────────────┘
```

### 2.3c What explicitly did **not** change

- `nexus42d` **still does not link** `agent-client-protocol` SDK. LocalSet never enters the daemon's axum runtime.
- `nexus42d` is **still not an ACP Agent / ACP Server**. It is the orchestrator that delegates to worker(s).
- Interactive `nexus42 agent run` is unchanged — no new IPC, no daemon roundtrip.

---

## §2.4 (amended) — Connection management

`AcpSession` (defined in `nexus-acp-host`) is now hosted in **three** possible process contexts:

| Context                                | Lifespan                                  | Who owns it                         |
| -------------------------------------- | ----------------------------------------- | ----------------------------------- |
| Interactive CLI (`agent run`)          | One user session                          | `nexus42` process directly          |
| Worker (`acp-worker --creator <id>`)   | Creator's active lifetime (until shutdown) | `nexus42 acp-worker` process        |
| Integration tests (CI)                 | Test run                                  | Test harness with mock agent        |

The daemon's Worker Manager **tracks** workers and the `session_id` they advertise in `worker/initialize` replies — for observability via `GET /v1/local/orchestration/sessions` — but does **not** hold the `AcpSession` Rust value itself.

---

## §2.5 (revised) — Dependency on daemon

| Path                     | Daemon involvement                                                                                                     |
| ------------------------ | ---------------------------------------------------------------------------------------------------------------------- |
| Interactive `agent run`  | **None** (as v1)                                                                                                       |
| Orchestration `acp-worker`| **Explicit**: daemon spawns, supervises, sends prompts, routes tool grant decisions, and terminates the worker         |
| CLI internal (sync etc.) | Unchanged HTTP client (`DaemonClient`) for local RPC                                                                   |

The v1 §2.5 statement "V1.1+ (deferred): The daemon could provide a proxy for agent tool calls / session persistence / permission policy" is **re-scoped** — these are now implemented via the worker-delegated path rather than a separate proxy. See §5 for which V1.1+ deferred items ship in V1.4.

---

## §4 (amended) — Local API Contract

### 4.1 / 4.2 / 4.3 (v1) — unchanged for interactive path

Interactive CLI still uses direct stdio; no Local API additions for that path.

### 4.3 (new in v2) — Orchestration control endpoints

The following endpoints are **added** by the V1.4 orchestration track ([orchestration-engine.md](../../knowledge/specs/orchestration-engine.md) §10.3) and by WS7 ([creator-schedule-and-core-context.md](creator-schedule-and-core-context.md) §9). They live on `nexus42d` alongside existing `/v1/local/workspace`, `/v1/local/daemon/status`, `/v1/local/runtime/*`.

**Engine session endpoints (WS2):**

| Method | Path                                                            | Purpose                                                                  |
| ------ | --------------------------------------------------------------- | ------------------------------------------------------------------------ |
| GET    | `/v1/local/orchestration/sessions`                              | List engine sessions (system + creator); filterable by `?creator_id=`    |
| GET    | `/v1/local/orchestration/sessions/{session_id}`                 | Full session state (current task, context summary, chat history pointer) |
| POST   | `/v1/local/orchestration/sessions/{session_id}/signal`          | Body `{"signal": "pause" \| "resume" \| "cancel" \| "advance"}`          |
| GET    | `/v1/local/orchestration/capabilities`                          | Enumerate registered capabilities + their schemas                        |
| GET    | `/v1/local/orchestration/presets`                               | List loadable preset bundles (embedded + filesystem)                     |
| POST   | `/v1/local/orchestration/presets/{id}:reload`                   | Force loader cache invalidation                                          |

**Creator Schedule endpoints (WS7):**

| Method | Path                                                                         | Purpose                                                             |
| ------ | ---------------------------------------------------------------------------- | ------------------------------------------------------------------- |
| GET    | `/v1/local/orchestration/schedules`                                          | List Schedules; filters `?creator_id=`, `?status=`                  |
| GET    | `/v1/local/orchestration/schedules/{schedule_id}`                            | Schedule detail + current core_context preview                      |
| POST   | `/v1/local/orchestration/schedules`                                          | Add Schedule (`nexus42 schedule add` backs this)                    |
| PATCH  | `/v1/local/orchestration/schedules/{schedule_id}/core-context`               | Apply `EditOp` (user edit of core_context)                          |
| POST   | `/v1/local/orchestration/schedules/{schedule_id}/signal`                     | `{"signal": "start" \| "pause" \| "resume" \| "cancel" \| "advance"}` |
| GET    | `/v1/local/orchestration/schedules/{schedule_id}/core-context`               | Current core_context content                                        |
| GET    | `/v1/local/orchestration/schedules/{schedule_id}/core-context-history`       | Full derivation trace (meta by default, content with flag)          |
| DELETE | `/v1/local/orchestration/schedules/{schedule_id}`                            | Remove Schedule (only if terminal)                                  |

**Wire classification (per [schemas-boundary.md](schemas-boundary.md)):** all endpoints above are **local-only HTTP** (CLI↔daemon, same machine; platform never observes them). Request / response Rust types live as hand-written code under `crates/nexus-contracts/src/local/orchestration/` and `crates/nexus-contracts/src/local/schedule/http.rs` — **not** codegen'd, **not** in `schemas/`.

### 4.4 (deferred, as v1) — ACP tool mediation endpoints

Endpoints `POST /v1/local/acp/tool/grant` / `deny`, `GET /v1/local/acp/sessions`, `DELETE /v1/local/acp/sessions/{id}` from v1 §4.3 remain **deferred** — the orchestration track does not need public endpoints for these; decisions flow over worker IPC instead. Revisit at V1.5+ if external consumers (e.g. platform UI) require them.

---

## §5 (amended) — Skills / Capability Export

### 5.1 / 5.2 (v1) — unchanged for V1.0 frozen IDs

V1.0 frozen capability IDs (`file_system.read`, `file_system.write`, `terminal.create`, `terminal.output`, `terminal.release`) stay exactly as v1 specified.

### 5.2a (new in v2) — V1.4 additions

The following **previously deferred** ACP capability IDs are now declared by the worker during `initialize`:

| Capability ID             | v1 status       | v2 status     | Why it ships now                                                           |
| ------------------------- | --------------- | ------------- | -------------------------------------------------------------------------- |
| `session.persistence`     | Deferred (R6)   | **Declared**   | Worker owns the ACP session across orchestration state transitions; natural fit |
| `session.modes`           | Deferred (R11)  | **Declared**   | Needed by preset `tool_policy` to toggle between ask / act modes                |

Capabilities **still deferred**:

- `terminal.kill`, `terminal.wait_for_exit` — no orchestration dependency; stay R3
- `slash_commands`, `agent_plan` — UI concerns; stay R4/R5
- Full `request_permission` policy engine — partial (worker IPC carries permission requests to daemon; decision logic is still "auto-grant with log" in MVP until V1.5 policy engine)

### 5.3 (amended) — Skills manifest

Still no on-disk `$HOME/.nexus42/skills.json` manifest in V1.4. The orchestration engine's `GET /v1/local/orchestration/capabilities` endpoint (§4.3) serves the same use case for Local clients.

---

## §9 (amended) — Test strategy additions

In addition to v1 §9.1–9.4, add:

### 9.2a Integration tests for worker-delegated path

| Test                                    | Description                                                       | Location                                          |
| --------------------------------------- | ----------------------------------------------------------------- | ------------------------------------------------- |
| Worker initialize roundtrip             | Spawn `nexus42 acp-worker` with mock agent; `worker/initialize` → reply | `crates/nexus-orchestration/tests/worker_ipc.rs`  |
| Prompt streaming over IPC               | Send `worker/acp_prompt`; verify chunks stream back in order      | same                                              |
| Graceful shutdown                       | `worker/shutdown { grace_ms: 500 }`; worker exits within grace    | same                                              |
| Worker crash detection                  | Kill worker PID; engine marks session `paused` with `worker_crash` | `crates/nexus-orchestration/tests/crash_recovery.rs` |
| Tool policy auto_grant_read_only        | Agent requests write tool; worker upcalls daemon; daemon denies  | same                                              |

### 9.4 (amended) — Manual verification additions

Add:

```bash
# Orchestration smoke (after Phase 3)
nexus42 schedule start novel-writing --creator <id>
nexus42 schedule status <session-id>
nexus42 schedule advance <session-id>

# Daemon restart preserves session
systemctl restart nexus42d || pkill nexus42d    # (platform-specific)
nexus42 schedule status <session-id>             # should resume from last task
```

---

## §10 (superseded) — Task breakdown

v1 §10 tasks 1–6 are **either delivered** (most V1.0 scope is done per `plans-done.json`) or **superseded** by the Phase 1–4 breakdown in [orchestration-engine.md](../../knowledge/specs/orchestration-engine.md) §10. This document defers to that doc for sequencing and acceptance criteria; the only ACP-specific Phase-1 task is the **crate extraction** described in §11 below.

---

## §11 (new) — `crates/nexus-acp-host` crate

### 11.1 Purpose

Own all ACP client logic in a single crate so that:

1. It can be linked by both `nexus42` CLI (interactive path) and `nexus42 acp-worker` subcommand (orchestration path) **without code duplication**.
2. It **cannot** be accidentally linked from `nexus42d`, preventing the `!Send` / LocalSet issue from polluting the daemon's axum runtime.
3. SDK upgrades (e.g. when `agent-client-protocol` → `sacp` v1.0 happens) are contained within one crate boundary.

### 11.2 Target layout

```
crates/nexus-acp-host/
├── Cargo.toml
├── src/
│   ├── lib.rs               # public re-exports
│   ├── client.rs            # NexusAcpClient trait + AcpSdkAdapter impl (moved from nexus42)
│   ├── transport.rs         # subprocess spawn + stdio pipe management (moved)
│   ├── skills.rs            # V1.0 + V1.4 capability constants (moved + extended)
│   ├── registry.rs          # registry manifest fetcher + cache (moved)
│   ├── error.rs             # AcpError enum (moved)
│   └── capabilities/        # new module namespace for V1.4 additions
│       ├── session_persistence.rs
│       └── session_modes.rs
└── tests/
    └── smoke.rs             # existing ACP tests relocated + augmented
```

### 11.3 Linkage matrix (enforced)

| Crate                | Links `nexus-acp-host`? | Why                                                                 |
| -------------------- | ----------------------- | ------------------------------------------------------------------- |
| `nexus42` (CLI)      | **Yes**                 | Used by `agent run/list/show/probe` and by `acp-worker` subcommand  |
| `nexus42d` (daemon)  | **No (enforced)**       | LocalSet / `!Send` must not enter axum multi-thread runtime         |
| `nexus-orchestration`| **No**                  | Talks to workers via IPC; does not hold AcpSession values            |
| `nexus-sync`         | No                      | Not ACP-related                                                     |
| `nexus-domain`       | No                      | Pure domain types                                                   |
| `nexus-local-db`     | No                      | Pure storage                                                        |

**Enforcement**: a `cargo-deny` or manual CI job asserts `nexus42d/Cargo.toml` does not list `nexus-acp-host` nor `agent-client-protocol`. Failing this gate blocks merge. Add to `.github/workflows/ci.yml` Phase 1 acceptance.

### 11.4 Migration steps

Ordered for Phase 1 (see [orchestration-engine.md](../../knowledge/specs/orchestration-engine.md) §10.2):

1. `mkdir crates/nexus-acp-host`, initialise `Cargo.toml` (workspace member, dependencies copied from `nexus42` subset relevant to ACP).
2. `git mv crates/nexus42/src/acp/{client,transport,skills,registry,error}.rs crates/nexus-acp-host/src/`.
3. Move ACP tests from `crates/nexus42/tests/acp_*.rs` to `crates/nexus-acp-host/tests/` (preserve content).
4. Update `crates/nexus42/Cargo.toml` to depend on `nexus-acp-host = { path = "../nexus-acp-host" }`; remove direct `agent-client-protocol` dep.
5. Update `crates/nexus42/src/commands/agent.rs` imports from `use crate::acp::*` to `use nexus_acp_host::*`.
6. Add hidden `nexus42 acp-worker` subcommand (minimal initialize-only body; Phase 2 expands).
7. Add CI gate per §11.3.
8. Verify `cargo build --workspace`, `cargo test --workspace`, `cargo +nightly fmt --all -- --check`, `cargo clippy --all -- -D warnings` all clean.
9. Manual: run `nexus42 agent list`, `show claude-acp`, `probe --registry`, `run claude-acp -m "hello"` — must produce identical output to pre-migration.

### 11.5 What stays in `nexus42`

- `crates/nexus42/src/commands/agent.rs` — CLI command dispatch (uses `nexus-acp-host` types but is CLI-layer)
- `crates/nexus42/src/commands/acp_worker.rs` (new) — worker subcommand entry point (imports `nexus-acp-host` for ACP logic + `nexus-orchestration` IPC types for daemon channel)

### 11.6 Compatibility bridge

During Phase 1, keep a short-lived `crates/nexus42/src/acp.rs` shim:

```rust
// crates/nexus42/src/acp.rs  (Phase 1 only; remove in Phase 2)
pub use nexus_acp_host::*;
```

so that any in-repo doc or external consumer still using `crate::acp::…` keeps compiling. Remove in Phase 2 along with any internal cleanup.

---

## Appendix B (amended) — Residual Findings for V1.x+

| ID     | Title                                                | v1 severity  | v1 target | v2 status                                                                           |
| ------ | ---------------------------------------------------- | ------------ | --------- | ----------------------------------------------------------------------------------- |
| ACP-R3 | Terminal kill/wait_for_exit capability               | low          | V1.1      | Unchanged — still deferred; no orchestration need                                   |
| ACP-R4 | Slash commands UI integration                         | low          | V1.1      | Unchanged — UI concern                                                              |
| ACP-R5 | Agent plan display support                            | low          | V1.1      | Unchanged                                                                           |
| ACP-R6 | Session persistence across CLI invocations            | medium       | V1.1      | **Partially addressed in V1.4**: orchestration sessions persist via SQLite; interactive `agent run` still ephemeral |
| ACP-R7 | Permission policy engine (grant/deny UI)              | medium       | V1.1      | **Plumbing only in V1.4**: worker IPC carries permission requests; decision logic remains auto-grant-with-log until V1.5 |
| ACP-R8 | Daemon-mediated agent tool access                     | medium       | V1.1      | **Addressed differently in V1.4**: worker-delegated path replaces the "proxy" design; mark as **resolved (re-scoped)** once Phase 2 lands |
| ACP-R9 | Skills manifest file for multi-agent hosts            | low          | V1.1      | Unchanged — no consumer need yet                                                    |
| ACP-R10| Binary agent auto-update mechanism                    | low          | V1.1      | Unchanged                                                                           |
| ACP-R11| Session modes (ask/act) switching                     | low          | V1.1      | **Resolved in V1.4**: `session.modes` declared; preset `tool_policy` maps to modes  |

---

## References

Internal:

- [orchestration-engine.md](../../knowledge/specs/orchestration-engine.md) — companion spec; primary consumer of this doc's §11 crate and §4.3 endpoints
- [daemon-lifecycle-api.md](daemon-lifecycle-api.md) — worker graceful shutdown timing; lifecycle states that expose ACP-related subsystems in degraded/status reports
- [acp-client-tech-spec-legacy.md](archived/knowledge/acp-client-tech-spec-legacy.md) — archived; do **not** cite directly — cite this v2 instead
- [architecture-alignment-review.md](architecture-alignment-review.md) — TD list; ACP-R8 status change tracked here after Phase 2 lands

External:

- ACP Protocol: https://agentclientprotocol.com/
- `agent-client-protocol` Rust crate: https://crates.io/crates/agent-client-protocol
- Public registry CDN: https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json

---

*End of v2 specification. v1 remains in-repo for historical reachability (see Superseded banner to be added on v1 file).*
