# Orchestration Engine — Design Specification

**Status**: Shipped (V1.4–V1.34 — orchestration engine SSOT, preset loader, worker IPC, capability registry). Continues to evolve: V1.31 agentic patterns + V1.32 quality gate landed; FL-D (DF-29/31/56) and DF-53 full auto-chain remain in [`deferred-features-cross-version-tracker.md`](deferred-features-cross-version-tracker.md) §3.3.  
**Document class**: Master  
**Author**: @project-manager (brainstorm consolidation) / to be co-authored by @architect before first implement
**Date**: 2026-04-17
**Scope**: daemon runtime (daemon), new `crates/nexus-acp-host`, new `crates/nexus-orchestration`, `nexus42` CLI additions, preset bundle format.
**Supersedes**: — (new topic)
**Coordinates with**:

- [local-cloud-crate-architecture.md](local-cloud-crate-architecture.md) — crate owners for sync/memory capabilities (§5.2 target names; legacy `nexus-sync` / `nexus-domain` until V1.21)
- [acp-client-tech-spec.md](acp-client-tech-spec.md) — §2.3 worker-delegated hosting amendment; §4 Local API additions; §11 `nexus-acp-host` crate spec
- [daemon-lifecycle-api.md](../../archived/knowledge/daemon-lifecycle-api.md) — full 6-state statig HSM closing TD-9
- [architecture-alignment-review.md](../../archived/knowledge/architecture-alignment-review.md) — TD-9 status moves from "gap" to "closed via statig HSM in v2 lifecycle doc"

**Non-goals** (explicit):

- Creator **Schedule** (multi-Schedule queueing, priority, preemption, CRUD by ID, `core_context` derivation and versioning) — **now folded into V1.4 as WS7**, designed separately in [creator-schedule-and-core-context.md](creator-schedule-and-core-context.md). This document defines the engine primitives that WS7 builds on.
- LLM-driven `core_context` summarisation / auto-iteration — V1.4 reserves the data-model variant but does not implement the capability (see [creator-schedule-and-core-context.md](creator-schedule-and-core-context.md) §11); V1.5+.
- Schedule cron / wall-clock triggers — V1.5+ (schema ready in V1.4).
- Preset third-party registry / signing / publish — V1.5+.
- Full `schemas/` vs local-type boundary refactor — **WS5** of V1.4, designed separately in [schemas-boundary.md](../../archived/knowledge/schemas-boundary.md); parallel to WS2 of that compass.

> This document is the **orchestration engine design** from the 2026-04-17 brainstorming session. Scope has since expanded: the `schemas/` boundary refactor is tracked as WS5 ([schemas-boundary.md](../../archived/knowledge/schemas-boundary.md)); the former "B-track" Schedule + core_context work is tracked as WS7 ([creator-schedule-and-core-context.md](creator-schedule-and-core-context.md)). Open questions originally parked in §11 of this document are **now answered** by WS7's spec (see §11 below for the reconciliation table).

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Scope and Responsibility Split](#2-scope-and-responsibility-split)
3. [Architecture Overview](#3-architecture-overview)
4. [Orchestration Engine (graph-flow integration)](#4-orchestration-engine-graph-flow-integration)
5. [Capability Registry](#5-capability-registry)
6. [Worker Model and Daemon ↔ Worker IPC](#6-worker-model-and-daemon--worker-ipc)
7. [Preset Bundle Format](#7-preset-bundle-format)
8. [Preset Loader (YAML → graph-flow Graph)](#8-preset-loader-yaml--graph-flow-graph)
9. [System Schedule vs Creator Schedule](#9-system-schedule-vs-creator-schedule)
10. [Migration Phases and Task Breakdown](#10-migration-phases-and-task-breakdown)
11. [Open Questions (deferred to B-track)](#11-open-questions-deferred-to-b-track)
12. [Coordinated Work Tracks and Knowledge Doc Revisions](#12-coordinated-work-tracks-and-knowledge-doc-revisions)
13. [Risks and Mitigations](#13-risks-and-mitigations)
14. [References](#14-references)

---

## 1. Executive Summary

### 1.1 Problem Statement

Nexus currently drives ACP agents through **one-shot, human-initiated CLI commands** (`nexus42 agent run <ref>`). The daemon (daemon runtime) is a passive HTTP backend that holds workspace state, sync plumbing, and the ACP tool-mediation endpoint — it does **not** drive creators, does **not** run scheduled work, and has **no notion of a strategy** that spans multiple ACP sessions and multiple days.

Users need to express creator workflows as configurable, prompt-driven strategies — e.g. *"collect inspiration → brainstorm → outline → draft"* — that the daemon executes **autonomously** across creator activations, with stable promptable identity and resumable execution across daemon restarts.

### 1.2 Design Pillars

- **Daemon becomes `orchestration engine + capability registry`**: existing HTTP-era capabilities (sync, workspace ops, outbox flush, registry refresh) are **reclassified as first-class capabilities** invokable as graph nodes; HTTP API retreats to a *trigger/query surface* over the same engine.
- **Strategy shape is hierarchical**: an outer **state machine** (long-lived, cross-session) containing inner **DAG graphs** (short, in-memory prompt/tool call chains) — *graph-of-graphs*.
- **ACP remains external, but its *execution locus* moves**: the CLI retains interactive `agent run/list/show/probe`; *orchestration-driven* ACP sessions move to **per-creator long-lived CLI worker subprocesses**. daemon runtime never links the ACP SDK directly.
- **Runtime is `graph-flow` (outer + inner) + custom SQLite `SessionStorage`**: adapted behind a thin trait layer so the upstream `0.2.x` crate is swappable.
- **Daemon lifecycle is `statig` HSM**: 6-state process lifecycle (`Stopped`/`Starting`/`Running`/`Degraded`/`Stopping`/`Failed`) — closes TD-9.
- **Presets are filesystem bundles**: YAML manifest + companion Markdown prompt templates; loaded dynamically by name; decoupled from compiled Rust code.

### 1.3 Deliverables (end-state of §10 Phase 1–4)

1. New crate `crates/nexus-acp-host`: ACP client logic extracted from `crates/nexus42/src/acp/`, linked by worker only.
2. New crate `crates/nexus-orchestration`: graph-flow adapter, capability registry, preset loader, SQLite `SessionStorage` impl.
3. daemon runtime gains: orchestration engine runtime, statig lifecycle HSM, Worker Manager, IPC server.
4. `nexus42` gains: `acp-worker` hidden subcommand (worker entrypoint); `schedule` command group (B-track — not in A's deliverables except a stub that surfaces engine state).
5. First built-in preset: `_system.maintenance` (mandatory) and one user-facing sample `novel-writing`.
6. Knowledge docs revised: [acp-client-tech-spec.md](acp-client-tech-spec.md), [daemon-lifecycle-api.md](../../archived/knowledge/daemon-lifecycle-api.md).

### 1.4 Effort (agent-oriented)

Per [effort-estimation.md](https://github.com/btspoony/mstar-harness/blob/main/docs/agents/effort-estimation.md) conventions (**agent sessions only; no human time**):

| Phase                                                           | Effort     | Approx. agent sessions |
| --------------------------------------------------------------- | ---------- | ---------------------- |
| Phase 1 — `nexus-acp-host` crate extraction                     | M          | 1–2                    |
| Phase 2 — orchestration skeleton (graph-flow + capability + IPC) | L          | 2–3                    |
| Phase 3 — preset loader + `novel-writing` end-to-end             | M          | 1–2                    |
| Phase 4 — statig lifecycle HSM (parallelisable with Phase 2)    | S+         | 1                      |
| Totals (excl. Phase 4 parallel savings; see compass WS5 for schemas refactor effort) | **L → XL** | **6–9** |

---

## 2. Scope and Responsibility Split

### 2.1 In scope (this document is authoritative for)

- Runtime architecture of the orchestration engine and capability registry.
- `nexus-acp-host` crate extraction (crate boundary and linkage matrix).
- Per-creator worker process model and daemon ↔ worker IPC protocol.
- Preset bundle filesystem layout, YAML manifest schema, prompt reference semantics, loader mapping rules.
- Adapter layer over `graph-flow`: trait boundary, SQLite `SessionStorage` impl contract, `Task` impls for the standard node kinds.
- Built-in capabilities catalog (first release).
- How the orchestration engine consumes and is consumed by `statig` daemon lifecycle.
- Migration phases and their ordering constraints.

### 2.2 Out of scope

| Topic                                                                          | Home                                                             |
| ------------------------------------------------------------------------------ | ---------------------------------------------------------------- |
| Multi-preset per-creator scheduling, priority, preemption                      | B-track (`schedule-and-plan-vN.md` — not yet authored)           |
| `nexus42 schedule` CLI command family semantics                                | B-track                                                           |
| Seed-prompt → stable core-context derivation & versioning                      | B-track                                                           |
| Preset distribution / registry / signing                                       | Future (V1.5+)                                                   |
| Wire schemas vs local types boundary refactor                                  | [v1.4-delivery-compass-v1.md](../../iterations/v1.4-delivery-compass-v1.md) §4 WS5 |
| ACP SDK migration (e.g. to `sacp` v1.0)                                        | Governed by [acp-client-tech-spec.md](acp-client-tech-spec.md) §1.2 adapter-layer policy |

### 2.3 Non-goals (explicit)

- **Not an ACP Agent/Server promotion**: daemon runtime remains not-an-ACP-host. See [acp-client-tech-spec.md](acp-client-tech-spec.md) §2.3 (*worker-delegated hosting*).
- **Not a LangChain-style in-memory pipeline**: all engine execution is **durable** and **resumable across daemon restart**; in-memory-only pipelines are explicitly rejected.
- **Not a replacement for interactive `nexus42 agent run`**: that path stays direct stdio CLI-to-agent; orchestration does not route through it.

---

## 3. Architecture Overview

### 3.1 Process topology

```
┌─────────────────────────────────────────────────────────────────┐
│                         daemon runtime                                │
│                                                                 │
│  ┌────────────────┐   ┌──────────────────────┐   ┌───────────┐  │
│  │   statig HSM   │   │  Orchestration       │   │Capability │  │
│  │   (lifecycle)  │   │  Engine              │   │ Registry  │  │
│  │ Stopped/       │◀──┤ (graph-flow +        │◀──┤ sync/     │  │
│  │ Starting/      │   │  SQLite              │   │ outbox/   │  │
│  │ Running/       │   │  SessionStorage)     │   │ workspace │  │
│  │ Degraded/      │   └──────────┬───────────┘   │ /acp/…    │  │
│  │ Stopping/      │              │                └───────────┘  │
│  │ Failed         │              │                              │
│  └────────────────┘              ▼                              │
│                        ┌────────────────────────┐               │
│                        │   Worker Manager       │               │
│                        │ (per-creator, long-    │               │
│                        │  lived subprocess)     │               │
│                        └────────────┬───────────┘               │
│                                     │ stdin/stdout              │
│                                     │ JSON-RPC 2.0              │
└─────────────────────────────────────┼───────────────────────────┘
                                      ▼
                     ┌──────────────────────────────────────┐
                     │ nexus42 acp-worker --creator <id>    │
                     │ links: crates/nexus-acp-host         │
                     │                                      │
                     │ ┌──────────┐     ┌────────────────┐  │
                     │ │ACP Client│◀───▶│ Agent Process  │  │
                     │ │(LocalSet)│     │ (Claude/Codex/ │  │
                     │ └──────────┘     │  …)            │  │
                     │                  └────────────────┘  │
                     └──────────────────────────────────────┘
```

### 3.2 Crate layout (target)

| Crate                                             | New? | Links ACP SDK? | Purpose                                                                                    |
| ------------------------------------------------- | ---- | -------------- | ------------------------------------------------------------------------------------------ |
| `crates/nexus-acp-host`                           | New  | Yes            | All ACP client logic; used by worker + CLI interactive commands                            |
| `crates/nexus-orchestration`                      | New  | No             | graph-flow adapter, capability trait, preset loader, SQLite `SessionStorage`               |
| `crates/nexus-daemon-runtime`                                 | Ext. | No             | Orchestration engine host, statig lifecycle, Worker Manager, HTTP surface (trigger/query)  |
| `crates/nexus42`                                  | Ext. | Yes (via host) | Interactive ACP commands, `acp-worker` subcommand, `schedule` command group (B-track stub) |
| `crates/nexus-contracts`, `nexus-creator`, `nexus-cloud-domain`, … | Ext. | No | Application crates per [local-cloud-crate-architecture.md](local-cloud-crate-architecture.md); legacy monolith `nexus-domain` retired after V1.21 |

### 3.3 Data flow for one strategy tick

1. Engine resolves *which creator Schedule is due* (B-track concern; A-track treats the decision as a **trigger input**).
2. Engine loads the preset bundle (cached after first parse), constructs the outer graph-flow `Graph`.
3. Engine opens or resumes a `Session` for `<creator_id, preset_id, instance_id>` using SQLite `SessionStorage`.
4. `FlowRunner::run(session_id)` executes one *step*:
   - Task `run()` resolves the task kind (capability call / inner-graph launch / ACP prompt / judge / manual-wait).
   - For ACP-related tasks: dispatch to Worker Manager → IPC to the creator's worker.
   - For capability tasks: in-process async call into the registry.
   - For inner-graph: engine spawns a child `Session` keyed `<creator_id, preset_id, instance_id, state_id>` and runs to completion.
5. Task returns `TaskResult { response, next_action }` — engine advances, pauses, or marks done.
6. Engine persists Session context after each step.

### 3.4 Graph-of-graphs model

- **Outer graph** = the state machine. Each `state` in `preset.yaml` ⇒ a `Task` in the outer `Graph`. Transitions (including conditional, LLM-judged, manual) are expressed via graph-flow primitives: `add_edge`, `add_conditional_edge`, `NextAction::GoTo`, `NextAction::WaitForInput`.
- **Inner graph** = the DAG inside a state. Each `state`'s `Task::run()` may (a) synchronously invoke capabilities / ACP prompts, or (b) launch a **child `Session`** over an inner graph and await its completion before returning.
- Both layers use the same `graph-flow` runtime, same `SessionStorage` (namespaced keys), same Task trait surface — no second runtime.

---

## 4. Orchestration Engine (graph-flow integration)

> **Crate selection cross-reference**: `graph-flow = "=0.2.3"` pinning, `sqlx` adoption for the shared pool, and the general dependency conventions are now governed by [`crate-selection-best-practices.md`](crate-selection-best-practices.md) (see §1 conventions + §2.1/§2.2/§2.3 decisions). This section remains the design SSOT for *how* those crates are integrated; it defers crate-identity and versioning policy to the best-practices document.

### 4.1 Library adoption decision

Library: [`graph-flow` v0.2.3](https://github.com/a-agmon/rs-graph-llm) (aka `rs-graph-llm`).

**Why this library** (consolidated rationale from 2026-04-17 brainstorming):

- Core primitives (`Task`, `Context`, `Graph`, `Session`, `SessionStorage`, `FlowRunner`) map one-to-one to our needs.
- First-class pause/resume: `NextAction::WaitForInput`, `ExecutionStatus::Paused`, `GoTo`.
- Pluggable storage trait — we plug SQLite; Postgres / in-memory built-ins remain unused.
- `rig` (LLM backend) is an **optional feature** we do **not** enable — our LLM is remote via ACP, not direct cloud API.
- No recursive session semantics, but acceptable — inner graphs are spawned by outer `Task`s as separate `Session`s and awaited (§3.4).

### 4.2 Adapter trait layer (swap-out insurance)

```rust
// crates/nexus-orchestration/src/engine.rs
pub trait OrchestrationEngine: Send + Sync {
    async fn run_step(&self, session_id: &SessionId) -> Result<StepOutcome>;
    async fn new_session(&self, key: SessionKey, initial_ctx: Context) -> Result<SessionId>;
    async fn get_status(&self, session_id: &SessionId) -> Result<SessionStatus>;
    async fn signal(&self, session_id: &SessionId, signal: EngineSignal) -> Result<()>;
    async fn list_active(&self, filter: SessionFilter) -> Result<Vec<SessionSummary>>;
}
```

First and only impl in Phase 2: `GraphFlowEngine` wraps `graph_flow::FlowRunner` + our `SqliteSessionStorage`. All daemon code depends on the **trait**, not on `graph_flow::*` directly. If the upstream crate ships breaking changes we cannot absorb, we swap the impl — callers are insulated.

### 4.3 SQLite `SessionStorage` implementation

New impl in `crates/nexus-orchestration/src/storage/sqlite.rs`:

```rust
pub struct SqliteSessionStorage {
    pool: sqlx::SqlitePool,           // shares nexus-local-db's SqlitePool (post-WS8)
}

#[async_trait]
impl graph_flow::SessionStorage for SqliteSessionStorage {
    async fn save(&self, session: Session) -> Result<(), graph_flow::Error> { … }
    async fn get(&self, id: &str) -> Result<Option<Session>, graph_flow::Error> { … }
    async fn delete(&self, id: &str) -> Result<(), graph_flow::Error> { … }
}
```

**Pool ownership (post-WS8)**: `nexus-local-db` exposes `Arc<sqlx::SqlitePool>` as the single workspace pool for `state.db` after V1.4 **WS8** unifies the DB engine on `sqlx` ([`2026-04-17-v1.4-ws8-local-db-sqlx-migration.md`](../../plans/2026-04-17-v1.4-ws8-local-db-sqlx-migration.md); decision SSOT: [`crate-selection-best-practices.md`](../crate-selection-best-practices.md) §2.3 + §3.3). `SqliteSessionStorage` takes that `Arc<SqlitePool>` at construction time; no separate connection or separate `.db` file. The `orchestration_sessions` table lands as one more `.sql` migration file under `crates/nexus-local-db/migrations/`, authored in WS2 Task 3 **after** WS8 T1–T2.

Schema (new table in the unified `state.db` owned by `nexus-local-db`; schema migration file added under `crates/nexus-local-db/migrations/`):

```sql
CREATE TABLE IF NOT EXISTS orchestration_sessions (
  session_id    TEXT PRIMARY KEY,
  creator_id    TEXT NOT NULL,
  preset_id     TEXT NOT NULL,
  preset_version INTEGER NOT NULL,
  parent_session_id TEXT,             -- set for inner-graph child sessions
  current_task_id TEXT,
  status        TEXT NOT NULL,        -- running | paused | waiting_for_input | completed | failed
  context_json  BLOB NOT NULL,        -- serialized graph_flow::Context
  chat_history_json BLOB,             -- optional; separate column for readability
  created_at    INTEGER NOT NULL,
  updated_at    INTEGER NOT NULL,
  FOREIGN KEY (parent_session_id) REFERENCES orchestration_sessions(session_id)
);

CREATE INDEX orchestration_sessions_by_creator ON orchestration_sessions(creator_id);
CREATE INDEX orchestration_sessions_by_status  ON orchestration_sessions(status);
```

**Migration path**: additive — new tables do not touch existing domain tables.

### 4.4 Standard `Task` impls (the runtime vocabulary)

Every preset node compiles into one of these Rust `Task` impls:

| Task impl            | Preset node kind         | Behaviour                                                                 |
| -------------------- | ------------------------ | ------------------------------------------------------------------------- |
| `CapabilityTask`     | `capability`             | Resolves to a registered `Capability`, calls its `run(ctx)`, stores output |
| `AcpPromptTask`      | `acp_prompt`             | Dispatches prompt to worker via IPC; streams response back into `Context` |
| `InnerGraphTask`     | `inner_graph`            | Launches a child `Session` over a named inner graph; awaits completion    |
| `JudgeTask`          | `llm_judge` exit_when    | Calls declared `judge_capability` (default `judge.llm`) via worker IPC — **V1.33** fixes stub-only path; see §4.4.1 |
| `ManualWaitTask`     | `manual` exit_when       | Returns `NextAction::WaitForInput`; CLI `advance` resumes                 |
| `RuleCheckTask`      | rule-based exit_when     | Pure function over `Context`; no external calls                           |
| `TimerWaitTask`      | `timer` exit_when (opt.) | Returns `WaitForInput` plus schedules a clock signal (B-track integration) |

All impls live in `crates/nexus-orchestration/src/tasks/`. Task implementations are **pure** over `Context` + well-typed capability handles — no global state.

#### 4.4.1 `llm_judge` runtime contract (V1.33 — Implemented)

**Pre-V1.33 problem (resolved in V1.33 P3):** `StateCompositeTask` used to route `exit_when.kind: llm_judge` through `JudgeTask` that only evaluated stub `_judge_rule` (`always_true` / `always_false`) without calling the declared `judge_capability` or loading `template_file`.

**Required behavior (V1.33+):**

1. Read `judge_capability` from state YAML (default `judge.llm`).
2. Load `template_file` relative to bundle root; pass rendered template + `contextData` to the capability.
3. Require worker IPC for `judge.llm` (same as `acp.prompt`); without worker → `CapabilityError` / `WaitForInput` per preset policy.
4. Parse GO/NOGO from capability output (existing `judge.llm` word-list contract).
5. Map GO → `NextAction::Continue`; NOGO → `NextAction::WaitForInput` (respect `min_interval` if set).
6. **Identity**: use schedule-injected `_creator_id` / `_session_id` only — not raw preset args (aligns with V1.32 `SEC-V131-01`).

**Explicit non-goal:** conditional `next` on NOGO (e.g. return to `gathering`) remains deferred until loader accepts `next.kind: conditional`.

### 4.5 Pausing, cancelling, and signals

- **Pause** (user or Schedule): `engine.signal(session, Pause)` — flips status to `paused`; `FlowRunner` refuses to advance until `Resume`.
- **Manual advance**: `engine.signal(session, Resume)` — returns a `NextAction::Continue` on next `run_step`.
- **Cancel**: `engine.signal(session, Cancel)` — cascades to any child inner-graph sessions; each `Task` impl must be *cancellation-safe* (stop after the current await point; no half-committed writes).
- **Kill** (daemon stop): lifecycle HSM `Stopping` state sends `Cancel` to every active session before shutting the engine down.

---

## 5. Capability Registry

> **Crate selection cross-reference**: Capability implementations MAY depend on third-party crates (e.g. `notify` for file-watch capabilities, `jsonwebtoken` for auth-related capabilities). Any new crate introduced here follows [`crate-selection-best-practices.md`](crate-selection-best-practices.md) §1 (conventions) — in particular §1.5 (PM introduction gate) and §1.3 (feature flag whitelist).

### 5.1 `Capability` trait

```rust
// crates/nexus-orchestration/src/capability.rs
#[async_trait]
pub trait Capability: Send + Sync {
    fn name(&self) -> &'static str;           // e.g. "sync.push"
    fn input_schema(&self) -> &JsonSchema;
    fn output_schema(&self) -> &JsonSchema;
    async fn run(&self, ctx: CapabilityCtx<'_>, input: Value) -> Result<Value, CapabilityError>;
}

pub struct CapabilityCtx<'a> {
    pub creator_id: Option<&'a CreatorId>,
    pub workspace: &'a WorkspaceHandle,
    pub engine: &'a dyn OrchestrationEngine,   // capability may signal engine (rare)
    pub worker: Option<&'a WorkerHandle>,       // present only for ACP-kind capabilities
    pub clock: &'a dyn Clock,
    pub tracing: &'a tracing::Span,
}
```

### 5.2 Built-in capabilities (first release)

All capabilities below are registered at daemon runtime startup. Adding a new capability is a Rust code change (not user-config) for V1.4. User-authored capabilities are **out of scope** (residual for V1.5+).

| Name                        | Purpose                                                        | Owner crate (target)   | Runtime status |
| --------------------------- | -------------------------------------------------------------- | ---------------------- | -------------- |
| `sync.pull`                 | Pull remote deltas (replaces HTTP-era trigger)                 | `nexus-cloud-sync`     | Deferred wiring |
| `sync.push`                 | Push local outbox (replaces HTTP-era trigger)                  | `nexus-cloud-sync`     | Deferred wiring |
| `outbox.flush`              | Flush pending outbox entries                                   | `nexus-cloud-sync`     | Deferred wiring |
| `outbox.compact`            | Compact outbox table                                           | `nexus-local-db`       | Deferred wiring |
| `workspace.open`            | Ensure workspace dir is present and valid                      | `nexus-home-layout`    | Deferred wiring (DF-31) |
| `workspace.commit`          | Commit manuscript diff into working copy                       | `nexus-home-layout`    | Deferred wiring (DF-31) |
| `registry.refresh`          | Refresh ACP registry cache                                     | `nexus-acp-host`       | Deferred network/CDN wiring (DF-29) |
| `creator.read_memory`       | Query persisted creator memory fragments                       | `nexus-creator-memory` | **Real** — SQLite-backed query through `CreatorCapabilityStore` (V1.31 DF-30) |
| `creator.write_memory`      | Persist creator memory fragments and return real `fragment_id` | `nexus-creator-memory` | **Real** — SQLite-backed write through `CreatorCapabilityStore` (V1.31 DF-30) |
| `creator.inject_prompt`     | Queue a prompt to be sent on next `acp.prompt`                 | `nexus-orchestration`  | **Real** — persisted injection queue in `state.db` (V1.31 DF-30) |
| `acp.prompt`                | Send a prompt to this creator's active ACP session             | `nexus-orchestration`  | Real for worker-backed preset execution |
| `acp.session_load`          | Resume a named ACP session id on the creator's worker          | `nexus-orchestration`  | Real for worker-backed preset execution |
| `kb.extract_work`           | Extract KB assets from a work entry into a World               | `nexus-orchestration` (preset-driven via `acp_prompt`) | Real |
| `soul.experience.aggregate` | Aggregate SOUL Experience section from session review items    | `nexus-orchestration` (preset-driven via `acp_prompt`) | Real |
| `judge.llm`                 | Evaluate a go/nogo prompt using a *judge* agent                | `nexus-orchestration`  | **Real** — worker-backed `acp.prompt` with `deny_all`, GO/NOGO parse (V1.31 DF-33/37) |
| `judge.rule`                | Evaluate a pure rule over `contextData`                        | `nexus-orchestration`  | **Real** — boolean literals, field equality/inequality, numeric comparisons (V1.31 DF-32) |
| `context.summarize`         | Summarize context through a worker-backed ACP prompt           | `nexus-orchestration`  | **Real** — returns `{ summary, prompt_hash }` (V1.31 DF-34/37) |
| `timer.wait_until`          | Schedule a wake-up signal (requires B-track clock)             | `nexus-orchestration`  | Deferred clock integration |

> **V1.31 de-stub note:** `creator.*`, `judge.rule`, `judge.llm`, and `context.summarize` are real runtime capabilities as of V1.31. DF-37 reduces worker-backed fallback to explicit standalone/test construction paths; daemon/preset execution injects runtime dependencies through the registry factory.

### 5.3 Capability input/output schemas

Each capability ships its `input_schema` and `output_schema` as constants (JSON Schema draft 2020-12) in Rust. **These schemas are local** (per [schemas-wire-platform-sync-boundary.md](../schemas-wire-platform-sync-boundary.md)) and live under `crates/nexus-contracts/src/local/orchestration/` (or adjacent module), **not** under `schemas/` — they are not wire contracts.

> **Daemon builds:** `sync.*` / `outbox.flush` MUST NOT call `nexus-cloud-sync` on the daemon hot path; see [local-cloud-crate-architecture.md](local-cloud-crate-architecture.md) §7.

### 5.4 Capability errors

`CapabilityError` variants: `InputInvalid`, `TransientExternal`, `PermanentExternal`, `WorkerUnavailable`, `AcpSessionLost`, `Cancelled`, `Internal`. Engine translates these into graph-flow `TaskResult` + Session context `_error` field so presets can fork on error kind.

### 5.5 Runtime dependency injection (V1.31)

Runtime-backed capabilities receive their external handles through `CapabilityRegistry::with_runtime_deps()` or the pool-aware `CapabilityRegistry::with_builtins_and_pool()` factory. The registry owns shared adapters rather than letting individual capabilities discover global state.

- **Creator memory adapter**: `CreatorCapabilityStore` is injected for `creator.read_memory`, `creator.write_memory`, and `creator.inject_prompt`. It uses the orchestration-side SQLite pool and maps capability inputs/session context to creator memory operations.
- **Worker handle provider**: worker-backed capabilities receive `Arc<dyn WorkerHandleProvider>`. `judge.llm` and `context.summarize` call `WorkerHandleProvider::call_acp_prompt(...)`; `judge.llm` uses `tool_policy = deny_all` and parses explicit GO/NOGO responses.
- **Standalone/test mode**: constructors without runtime dependencies may still install deterministic fallbacks for isolated unit tests. Those fallbacks are not the daemon/preset runtime path.

### 5.6 `creator.inject_prompt` queue semantics (V1.31)

`creator.inject_prompt` persists prompt injections in `state.db` in the `creator_prompt_injections` table. Rows are scoped to the creator/session context and are consumed by the next `acp.prompt` on the same creator session.

Operational semantics:

1. `creator.inject_prompt` appends a queued prompt row and returns queue metadata.
2. The next worker-backed `acp.prompt` drains pending injections for that creator session before sending the prompt to the worker.
3. Consumed rows are marked/drained transactionally with the prompt dispatch path so a daemon restart does not lose unconsumed injections.
4. The queue is for orchestration-to-worker prompt augmentation only; it does not bypass ACP tool policy or worker permission handling.

---

## 6. Worker Model and Daemon ↔ Worker IPC

### 6.1 Worker lifecycle

- **Spawn**: Worker Manager starts `nexus42 acp-worker --creator <id>` when the first ACP-kind capability is invoked for that creator (lazy start) or when the statig HSM enters `Running` **and** a creator has an active Schedule requiring one (eager start — B-track decides policy; A-track defaults to **lazy**).
- **Supervise**: Worker Manager monitors exit status and writes to `tracing`; on unexpected exit during an active Session, the corresponding engine signal is `AcpSessionLost` — preset may have a retry path; if none, Session flips to `failed`.
- **Graceful stop**: lifecycle HSM `Stopping` state sends a terminal IPC `shutdown` frame; worker finalises current prompt if any, closes ACP session via `cancel`, exits within 5 s; otherwise `SIGTERM` → `SIGKILL` path per `acp-client-tech-spec.md` §2.3.
- **Crash recovery**: `daemon` restart reads `orchestration_sessions` table, finds sessions in `running` / `waiting_for_input` state that were owned by a now-dead worker, marks them `paused` with reason `worker_crash`, and exposes them to the user for manual resume (B-track may auto-resume on configured strategies).

### 6.2 One worker per creator (MVP)

- One long-lived worker subprocess per **active** creator.
- Worker holds **one** ACP agent subprocess at a time (initial MVP); the choice of *which* ACP agent to run is determined by the preset / Schedule and passed on worker start as `--agent <agent_ref>`.
- Switching agents within a creator requires **worker restart** in MVP (acceptable for V1.4). Multi-agent workers deferred to V1.5+.

### 6.3 IPC transport

Selected transport: **parent↔child stdin/stdout** with **JSON-RPC 2.0** framing.

**Implementation crate selection (closed 2026-04-17)**: `jsonrpsee-core` + proc macros + custom `RpcTransport` trait + newline-delimited framing via `tokio_util::codec::LinesCodec`. Decision SSOT: [`crate-selection-best-practices.md`](crate-selection-best-practices.md) §2.1 + §3.1. The `RpcTransport` trait is the insurance layer if jsonrpsee-core ever needs replacement — callers depend on the trait, not on `jsonrpsee::*` directly.

Rationale:

- Worker is a subprocess of daemon; parent-owned pipes are the most reliable shutdown channel (closing pipe = terminate signal).
- Consistent with ACP's own choice of framing (we can reuse framing code from `nexus-acp-host`).
- No port allocation or Unix-socket path management; cross-platform without extra code.

Trade-off accepted: workers cannot outlive the daemon. This is **correct by design** — if the daemon is down, there is no orchestration authority to drive them.

### 6.4 IPC message catalogue (v1)

Requests (daemon → worker):

| Method                   | Params                                                               | Response                                   |
| ------------------------ | -------------------------------------------------------------------- | ------------------------------------------ |
| `worker/initialize`      | `{ creator_id, agent_ref, workspace_root, acp_session_id? }`         | `{ capabilities, worker_pid }`             |
| `worker/acp_prompt`      | `{ prompt, tool_policy, session_id? }`                               | streaming `worker/acp_prompt_chunk` + final `worker/acp_prompt_complete` |
| `worker/acp_cancel`      | `{ session_id }`                                                     | `{}`                                       |
| `worker/acp_session_load`| `{ session_id }`                                                     | `{ ok, error? }`                           |
| `worker/health`          | `{}`                                                                 | `{ uptime_ms, acp_session_state, last_error? }` |
| `worker/shutdown`        | `{ grace_ms: u32 }`                                                  | `{}` (no further requests accepted)        |

Notifications (worker → daemon, unsolicited):

| Method                         | Params                                                   |
| ------------------------------ | -------------------------------------------------------- |
| `worker/agent_tool_request`    | `{ tool_name, args, request_id }`                        |
| `worker/agent_tool_request_result` (reply shape) | `{ request_id, grant, output? }`       |
| `worker/acp_permission_request`| `{ reason, request_id }`                                 |
| `worker/log`                   | `{ level, message, fields }`                             |
| `worker/unrecoverable_error`   | `{ kind, detail }` — worker will exit after this frame   |

> 详见 [agent-nexus-tool-bridge.md](agent-nexus-tool-bridge.md) §7 — single dispatch table invariant. `worker/agent_tool_request` values under `nexus.*` and the existing `fs/*` baseline must dispatch through the same registry as daemon HTTP tool execute; implementation plan: `2026-06-04-v1.34-agent-tool-implementation`.

### 6.5 Tool policy (connection to permission policy engine)

`tool_policy` is passed per-prompt from engine to worker. V1.4 values:

- `auto_grant_all` (V1.0 behaviour)
- `auto_grant_read_only` (reads allowed, writes require `worker/acp_permission_request` upcall)
- `deny_all`
- `request_policy` (every tool triggers upcall)

The permission *decision engine* is out-of-scope here; V1.4 ships the plumbing and `auto_grant_read_only` default for preset-driven work. ACP-R7 (permission policy engine) becomes **partially addressed** — see [acp-client-tech-spec.md](acp-client-tech-spec.md) Appendix B.

### 6.6 Backpressure and streaming

- Worker streams `acp_prompt_chunk` frames as the agent streams; daemon buffers and feeds into `Context.chat_history` via graph-flow's `add_assistant_message` helper.
- Buffer size per prompt: 256 KB soft cap; exceeding triggers `worker/log` warning and a `Context` flag `_long_response: true` so preset can fork.
- Worker backpressure: stdin write blocks are honoured by daemon (no unbounded in-memory queue).

---

## 7. Preset Bundle Format

### 7.1 Filesystem layout

```
presets/
  <preset-id>/
    preset.yaml             ← manifest (required)
    prompts/                ← prompt template dir (optional; may live elsewhere if YAML references)
      <name>.md
    schemas/                ← optional local input/output schemas for typed nodes
      <name>.schema.json
    README.md               ← optional, for human authors
```

Locations searched (in order):

1. `$XDG_CONFIG_HOME/nexus42/presets/<id>/` (user-installed)
2. `$HOME/.nexus42/presets/<id>/`             (legacy / dev)
3. Preset shipped in the binary (via `include_dir!`) under `nexus-orchestration/embedded-presets/<id>/` (currently `_system.maintenance`, `novel-writing`, `reflection-loop`, `memory-augmented`)

### 7.2 `preset.yaml` schema (v1)

```yaml
# Top-level
preset:
  id: novel-writing                 # [string, /^[a-z][a-z0-9._-]*$/] — must match dir name
  version: 1                        # [int, >=1] — bumped on breaking changes to this preset
  kind: creator                     # [enum: creator | system]
  description: "…"                  # [string, <=240 chars]
  requires_capabilities:            # [string[]] — loader rejects preset if any missing
    - creator.inject_prompt
    - acp.prompt
    - judge.llm
  initial: <state-id>               # [string, must match a states[].id]
  terminal: <state-id>              # [string, must match a states[].id]
  # optional annotations
  author:    "…"
  homepage:  "https://…"
  license:   "MIT"

states:
  - id: gathering
    description: "…"
    enter:
      - kind: capability
        name: creator.inject_prompt
        args:
          prompt_file: prompts/gathering.md     # resolved relative to bundle root
          vars:
            topic: "{{preset.input.topic}}"
    exit_when:
      kind: llm_judge                            # or: rule | graph_complete | manual | timer
      template_file: prompts/gathering-exit.md
      judge_capability: judge.llm                # optional; defaults to judge.llm
      min_interval: "PT6H"                       # ISO-8601 duration; don't re-evaluate sooner
    next: brainstorming                          # or a conditional form, see §7.5

  - id: brainstorming
    enter:
      - kind: inner_graph
        name: brainstorm_graph                   # referenced in inner_graphs below
    exit_when:
      kind: graph_complete
    next: outlining

  - id: outlining
    enter:
      - kind: capability
        name: creator.inject_prompt
        args:
          prompt_file: prompts/outlining.md
    exit_when:
      kind: manual                               # user-driven advance
    next: drafting

  - id: drafting
    enter:
      - kind: inner_graph
        name: drafting_graph
    exit_when:
      kind: graph_complete
    next: done

  - id: done
    terminal: true

inner_graphs:
  brainstorm_graph:
    nodes:
      - id: diverge
        kind: acp_prompt
        template_file: prompts/brainstorm-diverge.md
        tool_policy: auto_grant_read_only
      - id: cluster
        kind: acp_prompt
        depends_on: [diverge]
        template_file: prompts/brainstorm-cluster.md
      - id: select
        kind: acp_prompt
        depends_on: [cluster]
        template_file: prompts/brainstorm-select.md
    output_binding: select.text                  # exported into outer Context as state.brainstorming.output

  drafting_graph:
    nodes: [ … ]
    output_binding: …

signals:                                         # optional: events that can externally push the SM
  - name: user_paused
    on_receive:
      action: pause
  - name: deadline_reached
    on_receive:
      action: force_transition
      target: done
```

### 7.3 Prompt template file (`prompts/*.md`)

Prompt files are Markdown with an optional YAML front-matter header declaring variables.

```markdown
---
vars:
  topic: { type: string, required: true }
  vibe:  { type: string, default: "literary" }
max_tokens: 2000                  # optional model hint (ACP agent may ignore)
---

# Gathering

You are assisting the creator in collecting inspiration for a story about
**{{topic}}** with a **{{vibe}}** vibe.

Suggest ten concrete research directions, each as a bullet with a one-line justification.
```

- Template engine: `handlebars-rust` (simple, safe, no arbitrary code execution). Rejected alternatives: Tera (more features we don't need), MiniJinja (dep hygiene).
- Variable resolution order: node `args.vars` → preset `input` → `Context` exports (e.g. `state.brainstorming.output`) → hard-coded defaults.

### 7.4 `output_binding` and context namespacing

- Each inner-graph node has an output (the Task's response string or structured data).
- `output_binding` in `inner_graphs.<name>` names which node's output becomes the *exported* output of the state.
- Outer `Context` keys follow a fixed namespace:
  - `state.<state-id>.output` — exported from `output_binding`
  - `state.<state-id>.entered_at`, `state.<state-id>.exited_at` — epoch millis (engine-managed)
  - `preset.input.<key>` — read-only; provided by B-track Schedule at start
  - `creator.memory.<key>` — bridged via `creator.read_memory` capability; cached with TTL

### 7.5 Conditional `next` (optional, deferred semantics)

**Long-term SSOT (exploration):** [preset-conditional-routing.md](preset-conditional-routing.md) — Status **Exploration**; not loadable until a future implement compass ships. When normative, this subsection defers to that document for schema and validation rules.

Simple linear `next: <state-id>` covers the first release. Conditional form (future):

```yaml
next:
  kind: conditional
  rules:
    - when: "{{state.brainstorming.output | length > 2000}}"
      to: outlining
    - when: "{{state.brainstorming.output | contains 'unclear'}}"
      to: gathering               # allow re-entry
  default: outlining
```

A-track ships only the linear form; conditional form is a Phase 3 stretch / V1.5 work.

### 7.6 Validation

The loader rejects a preset and returns a structured error listing every problem when:

- YAML does not parse
- Schema fields missing/wrong type
- Unknown `states[].id` references in `next` / `initial` / `terminal`
- Unknown capability names in `enter`, `exit_when.judge_capability`, or `next.rules[].uses`
- `inner_graphs.<name>` contains a cycle or a node with `depends_on` referencing a nonexistent node
- Any `template_file` path escapes the bundle root or does not exist

Error format:

```json
{
  "preset_id": "novel-writing",
  "problems": [
    { "path": "states[1].enter[0].name", "error": "unknown capability: 'foo.bar'" },
    { "path": "inner_graphs.brainstorm_graph", "error": "cycle: diverge → cluster → diverge" }
  ]
}
```

#### 7.6.1 Shared semantic validation facade (V1.32)

V1.32 introduces a **shared semantic validation facade** used by both the CLI/API validate endpoint (`POST /v1/local/presets:validate`) and the orchestration loader. The facade is the single quality gate; there are no parallel weaker checks.

The facade is composed of three layers:

1. **`validate_preset_semantic`** — logical completeness checks:
   - **Reachability**: `initial_state` must reach at least one terminal state via forward edges. Unreachable states from `initial` are errors.
   - **Terminal marker consistency**: every state declared as `terminal` in the YAML must be reachable and must not have a `next` transition.
   - **Bundle id vs directory match**: for user/system bundles (not embedded), the `preset.id` field must match the bundle directory name.
   - **Orphan inner graph detection**: inner graphs defined but not referenced by any state's `enter` produce a **warning** (not an error) — this is an architect-level decision allowing preset authors to draft graphs before wiring them. Inner graphs referenced by states but not defined remain errors.

2. **`validate_assets_in_bundle`** — asset existence checks:
   - `template_file`, `prompt_file`, `system_prompt_file`, and `prompt` references must resolve to existing files within the bundle sandbox.
   - Missing files are errors.

3. **`validate_path_safety`** — filesystem sandbox enforcement:
   - Rejects `..` path traversal, absolute paths, and symlink escapes from the bundle root.
   - All asset paths are canonicalized and verified to remain within the bundle directory.

#### 7.6.2 Capability compatibility checks (V1.32)

The validation facade checks capability references against the registry:

- **Capability existence**: every capability name in `enter`, `exit_when`, and `requires_capabilities` must exist in the `CapabilityRegistry`. The registry provides O(1) lookup by name.
- **Argument drift detection**: capability argument keys in the preset are compared against the capability's declared `input_schema` properties. Unknown or missing keys produce warnings.
- **Schema check skipped fallback**: when a capability does not declare an `input_schema` (or the schema is empty), the argument drift check is skipped gracefully rather than failing. This preserves compatibility with built-in capabilities that predate formal schema declarations.

#### 7.6.3 Embedded presets and normative semantics

Embedded presets under `crates/nexus-orchestration/embedded-presets/` are **runtime assets compiled into the binary**; they are validated through the same shared facade at build/test time. They are **not** normative examples — the normative preset semantics remain in this spec (§7–§8). Preset authors should refer to this spec, not to embedded presets, as the authoritative contract.

### 7.7 Embedded preset index (V1.31+)

The binary includes embedded presets under `crates/nexus-orchestration/embedded-presets/`:

| Preset ID | Pattern / role | State flow (summary) | Primary capabilities |
| --- | --- | --- | --- |
| `novel-writing` | Narrative production (primary user path) | gathering → brainstorming → outlining → drafting → done | `creator.inject_prompt`, `acp.prompt`, `judge.llm` |
| `research` | Reference ingest + synthesis | scanning → extracting → synthesizing → done | `creator.inject_prompt`, `acp.prompt`, `judge.llm` |
| `kb-extract` | Work → World KB extraction | loading → extracting → done | `kb.extract_work`, `acp.prompt` |
| `soul-experience-refresh` | SOUL Experience (deterministic) | aggregate → done | `soul.experience.aggregate` |
| `reflection-loop` | Reflection demonstrator | draft → revise → summarize → done | `acp.prompt`, `judge.llm`, `context.summarize` |
| `memory-augmented` | Memory demonstrator | recall → generate → persist → done | `creator.*`, `judge.rule` |
| `creative-brief-intake` | **V1.33 planned** — grill-me intake | TBD in P2 plan | `acp.prompt` |

All shipped presets use **linear** `next` transitions unless noted; conditional routing remains deferred (§7.5).

### 7.8 Preset `run_intents` (V1.33)

Presets declare **how** they may be started from [work-experience-model.md](work-experience-model.md) and `creator run`:

```yaml
preset:
  id: novel-writing
  # ...
  run_intents:
    - work_init
    - work_continue
```

Closed enum: `work_init` | `work_continue` | `knowledge_ingest` | `work_maintenance` | `system_maintenance`.

Loader rules (V1.33):

- Reject unknown intent strings.
- `_system.*` presets must include `system_maintenance`.
- `creator run start` filters presets where `work_init ∈ run_intents`.
- `creator run continue` filters presets where `work_continue ∈ run_intents`.

Normative classification table: [work-experience-model.md](work-experience-model.md) §5.2.

---

## 8. Preset Loader (YAML → graph-flow Graph)

### 8.1 Loader contract

The loader consumes YAML bundles and produces `LoadedPreset` structs ready for graph-flow execution. As of V1.32, the loader runs the shared semantic validation facade (§7.6.1–§7.6.2) as a mandatory pre-step before graph construction. The CLI/API `validate` endpoint calls the same facade independently, ensuring loader and diagnostic parity.

```rust
// crates/nexus-orchestration/src/loader.rs
pub struct LoadedPreset {
    pub id: String,
    pub version: u32,
    pub outer_graph: Arc<graph_flow::Graph>,
    pub inner_graphs: HashMap<String, Arc<graph_flow::Graph>>,
    pub signals: Vec<SignalBinding>,
    pub source_hash: [u8; 32],             // blake3 over the bundle dir (identity across restarts)
}

pub fn load_preset(
    bundle_root: &Path,
    caps: &CapabilityRegistry,
) -> Result<LoadedPreset, PresetLoadError> { … }
```

The validation facade is exposed separately for diagnostic use:

```rust
// crates/nexus-orchestration/src/validation.rs
pub fn validate_preset_semantic(bundle: &PresetBundle) -> Vec<ValidationProblem> { … }
pub fn validate_assets_in_bundle(bundle_root: &Path, bundle: &PresetBundle) -> Vec<ValidationProblem> { … }
pub fn validate_path_safety(bundle_root: &Path, bundle: &PresetBundle) -> Vec<ValidationProblem> { … }
```

CLI and daemon `POST /v1/local/presets:validate` call these functions directly without constructing a graph, so validation diagnostics are available without full loader overhead.

### 8.2 Mapping rules (YAML → graph-flow)

| YAML fragment                                         | graph-flow construct                                                              |
| ----------------------------------------------------- | --------------------------------------------------------------------------------- |
| `states[].id`                                         | a `Task`'s `id()`                                                                 |
| `states[].enter[*].kind=capability`                   | wrapped in `CapabilityTask`                                                       |
| `states[].enter[*].kind=inner_graph`                  | wrapped in `InnerGraphTask` (holds handle to inner graph by name)                 |
| `states[].exit_when.kind=llm_judge`                   | `JudgeTask` inserted after `enter` tasks; `NextAction::Continue` on go, `WaitForInput` on no-go + retry-after-`min_interval` |
| `states[].exit_when.kind=manual`                      | `ManualWaitTask` → `NextAction::WaitForInput`                                     |
| `states[].exit_when.kind=graph_complete`              | inner-graph's terminal → outer task returns `NextAction::Continue`                |
| `states[].exit_when.kind=rule`                        | `RuleCheckTask`                                                                   |
| `states[].next: <id>`                                 | `add_edge(state_id, next_id)`                                                     |
| `states[].next.kind=conditional` (future)             | `add_conditional_edge`                                                            |
| `terminal: <id>`                                      | that state's task returns `NextAction::End`                                       |
| `inner_graphs.<name>.nodes[].kind=acp_prompt`         | `AcpPromptTask`                                                                   |
| `inner_graphs.<name>.nodes[].depends_on`              | `add_edge(dep, this)` in inner graph                                              |
| `inner_graphs.<name>.output_binding`                  | `InnerGraphTask` post-run: reads `ctx[binding_path]`, writes `state.<x>.output`   |

### 8.3 Caching and reloading

- Loader caches `LoadedPreset` keyed by `source_hash`.
- On `registry.refresh` capability call or the shipped Local API `POST /v1/local/presets/{id}:reload`, loader recomputes hash; if changed, invalidates cache and rebuilds. There is currently no top-level `nexus42 preset reload` CLI.
- Running sessions continue on the previous graph (snapshot semantics); new sessions pick up the new graph.

---

## 9. System Schedule vs Creator Schedule

### 9.1 System Schedule

- ID: `_system.maintenance` (reserved; `_`-prefix cannot be used by user presets).
- Shipped in binary (`embedded-presets`).
- Enters engine when statig lifecycle transitions `Running → entry`.
- Contains periodic states such as: `sync.pull.hourly`, `outbox.flush.on_idle`, `registry.refresh.daily`, `compaction.weekly`. (Exact set: implementation-level, may evolve; what matters architecturally is that they're ordinary preset states using ordinary capabilities.)
- Exits on statig lifecycle transition `Running → Stopping` (cancelled, not terminated).

### 9.2 Creator Schedule

A Creator Schedule is a persistent, user-addressable wrapper around zero or one active engine `Session`. It adds user-facing CRUD (`schedule add/edit/list/inspect/remove`), multi-Schedule per creator, dependency chains, and immutable `core_context` versioning that the engine reads at each state transition.

- **Design SSOT**: [creator-schedule-and-core-context.md](creator-schedule-and-core-context.md).
- **Session relationship**: a Schedule holds `current_session_id: Option<SessionId>` pointing at an active row in `orchestration_sessions` while `Schedule.status == Running`; terminal Schedules retain the Session row for history.
- **Engine primitives consumed** — `new_session`, `run_step`, `signal`, `list_active` are sufficient for the supervisor module defined in WS7; this spec adds no new engine API.
- **Concurrency contract** — multi-Schedule per creator is supported; at most one ACP-calling Schedule may touch the worker at any instant (§6.2 "one worker per creator" constraint). Capability-only Schedules may fully parallel.
- **Supervisor module** — `crates/nexus-orchestration/src/schedule/` (new in WS7) owns the Pending → Running admission logic, dependency resolution, and signal propagation. It is driven by engine session-terminal events.

### 9.3 Relation between the two

Both execute through the same engine using the same `Session` primitive. Differences are administrative, not runtime:

| Aspect                          | System Schedule                          | Creator Schedule                                         |
| ------------------------------- | ---------------------------------------- | -------------------------------------------------------- |
| Identity                        | Fixed ID `_system.maintenance`           | ULID per Schedule                                        |
| Origin                          | Embedded preset in binary                | User or CLI invocation                                   |
| Lifecycle owner                 | statig HSM (Running.entry starts it)     | `ScheduleSupervisor` (Pending → Running admission)       |
| `creator_id` in `orchestration_sessions` row | NULL or reserved "system" value | Real `creator_id` FK                                     |
| Observability row               | `kind: system`                           | `kind: creator`                                          |
| User CRUD                       | Not user-editable                        | Full CRUD per `nexus42 schedule *`                       |
| Holds `core_context`?           | No (the preset does not use per-Schedule core context) | Yes, versioned immutable series                 |

---

## 10. Migration Phases and Task Breakdown

### 10.1 Ordering constraints

```
Phase 1 (nexus-acp-host extraction)
    ↓
Phase 2 (orchestration skeleton) ─── parallel ───► Phase 4 (statig lifecycle / TD-9)
    ↓
Phase 3 (preset loader + novel-writing E2E)
    ↓
Phase 5 (knowledge doc revisions + spec amendments in place)
```

Compass WS5 (`schemas/` boundary refactor) is fully parallel and has no dependencies on this spec's phases — see [v1.4-delivery-compass-v1.md](../../iterations/v1.4-delivery-compass-v1.md) §4 WS5 for detailed scope.

### 10.2 Phase 1 — `nexus-acp-host` crate extraction (M; 1–2 agent sessions)

**Scope**

- Create `crates/nexus-acp-host/` with modules `client`, `transport`, `skills`, `registry`, `error`, `capabilities` (capability ID constants relocated).
- `git mv` existing files from `crates/nexus42/src/acp/*` preserving history where possible.
- Update `crates/nexus42` to `use nexus_acp_host as acp` and re-export for existing call-sites in `commands/agent.rs`.
- Add `nexus42 acp-worker` subcommand as **hidden** (not in `--help`) entrypoint — minimal body for this phase (echo back `worker/initialize` OK). Full worker logic in Phase 2.
- `Cargo.toml` workspace updates; update `rust-toolchain`, CI matrix, and `verify-codegen` (no codegen impact expected).
- Update `acp-client-tech-spec.md` §11 with final crate layout.

**Acceptance**

- [ ] `cargo build --workspace` clean
- [ ] `cargo test --workspace` green (existing ACP tests move with the crate)
- [ ] `cargo +nightly fmt --all -- --check` clean
- [ ] `cargo clippy --all -- -D warnings` clean
- [ ] `nexus42 agent list`, `show`, `probe --registry`, `run` **functionally unchanged** (manual + existing integration tests)
- [ ] `nexus42 acp-worker --creator <id>` starts, prints JSON-RPC initialize reply, exits on SIGTERM

### 10.3 Phase 2 — Orchestration skeleton (L; 2–3 agent sessions)

**Scope**

- New crate `crates/nexus-orchestration/`.
- `OrchestrationEngine` trait + `GraphFlowEngine` impl over `graph_flow = "=0.2.3"`.
- `SqliteSessionStorage` + migration added to `nexus-local-db`.
- `Capability` trait + registry; register **built-ins listed in §5.2 except `acp.*` and `judge.llm`** (those land Phase 3).
- Worker Manager (spawn/supervise/shutdown) + stdin/stdout JSON-RPC IPC codec.
- daemon runtime wires engine at startup (outside any HSM state changes — that's Phase 4).
- New HTTP endpoints (authoritative list added to `acp-client-tech-spec.md` §4.3):
  - `GET  /v1/local/orchestration/sessions`
  - `GET  /v1/local/orchestration/sessions/{session_id}`
  - `POST /v1/local/orchestration/sessions/{session_id}/signal`  (`pause` | `resume` | `cancel` | `advance`)
  - `GET  /v1/local/orchestration/capabilities`
- Register and run `_system.maintenance` hardcoded graph (not yet via file loader).

**Acceptance**

- [ ] Engine can create, step, pause, resume, cancel a hardcoded 3-state test graph
- [ ] SQLite storage roundtrip test: start session → kill daemon → restart → resume (manual signal) → completes
- [ ] Worker Manager test: spawn dummy worker (shell script), send `worker/health`, get reply, `worker/shutdown`, observe graceful exit within 5 s
- [ ] `GET /v1/local/orchestration/sessions` returns at least `_system.maintenance`'s session when daemon is in plain `Running` state (simulated — HSM lands Phase 4)
- [ ] `cargo test --workspace` green; clippy/fmt clean

### 10.4 Phase 3 — Preset loader + `novel-writing` end-to-end (M; 1–2 agent sessions)

**Scope**

- `load_preset()` + validation per §7.6.
- Register `acp.*`, `judge.llm`, `judge.rule`, `creator.*` capabilities.
- Implement `AcpPromptTask` dispatching to Worker Manager.
- Ship embedded `novel-writing` preset with 4 states + 2 inner graphs (minimum demonstrator).
- CLI stub: `nexus42 schedule start <preset-id> --creator <id>` (B-track will deepen this).
- CLI: `nexus42 schedule advance <session-id>` (manual transitions).
- CLI: `nexus42 schedule status <session-id>` pretty printer.

**Acceptance**

- [ ] `nexus42 schedule start novel-writing --creator <id>` returns a session id; `nexus42 schedule status` shows state `gathering`
- [ ] Daemon sends `creator.inject_prompt` output, which reaches the worker via IPC; mocked agent echoes; `judge.llm` advances state → `brainstorming`
- [ ] Inner graph runs all 3 nodes; `output_binding` writes into outer context; state advances to `outlining`
- [ ] `schedule advance` advances past `outlining` → `drafting` → `done`
- [ ] Daemon restart mid-`brainstorming` resumes from the last completed inner-graph node
- [ ] End-to-end integration test in `crates/nexus-orchestration/tests/e2e_novel_writing.rs`

### 10.5 Phase 4 — statig daemon lifecycle (S+; 1 agent session; parallel with Phase 2)

Owned by [daemon-lifecycle-api.md](daemon-lifecycle-api.md); A-track just consumes it. See that doc for state graph, entry/exit actions, event catalogue, and HTTP surface migration (status field now exposes real 6-state values).

**Integration point with engine**: HSM `Running.entry` calls `engine.start()`; `Stopping.entry` calls `engine.shutdown(grace_ms)`; `Degraded` is entered when any of `{sync, acp_registry, worker_manager}` report sustained failures (threshold defined in v2 lifecycle doc).

### 10.6 Phase 5 — Knowledge doc revisions (S; part of each preceding phase)

In the same change window as each phase:

- Phase 1 → commit [acp-client-tech-spec.md](acp-client-tech-spec.md) §11 (crate layout).
- Phase 2 → commit §4.3 (Local API additions) in the same spec.
- Phase 4 → commit [daemon-lifecycle-api.md](daemon-lifecycle-api.md).
- Phase 3 → this document updated: move sections to "Delivered" once implemented.
- **Phase 5b (new)** → WS7 lands [creator-schedule-and-core-context.md](creator-schedule-and-core-context.md) implementation; engine consumes the `ScheduleSupervisor` signal path added in that spec's §4.
- [architecture-alignment-review.md](architecture-alignment-review.md) §2.6 row for TD-9 updated from "Partial" to "Resolved (v2)".

---

## 11. Open Questions — Reconciliation Status (was "deferred to B-track")

The following questions were originally parked as B-track in this document. After the 2026-04-17 scope decision that folds B-track into V1.4 as WS7, status is:

| ID    | Question                                                                                               | V1.4 Resolution                                                                                       |
| ----- | ------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------- |
| OQ-1  | How many concurrent Schedules can one creator have active at once?                                      | **Answered in WS7** — multi-Schedule; concurrency declared per-add; ACP-calling Schedules serialised per-creator via worker mutex. See [creator-schedule-and-core-context.md](creator-schedule-and-core-context.md) §5. |
| OQ-2  | Schedule priority and preemption semantics                                                              | **Answered in WS7** — no priority / no preemption in V1.4; explicit `schedule pause/resume/cancel` only. See §2 decisions in the schedule spec. |
| OQ-3  | What happens when all creator Schedules complete                                                        | **Answered in WS7** — creator returns to idle (no default loop). See §2 decisions.                    |
| OQ-4  | `seed + user_edits + iterated_experience → core_context` derivation + versioning                         | **Partially answered in WS7**; V1.4 implements seed / user_edit / preset_hook derivation kinds and reserves `LlmSummarize` for V1.5. See [creator-schedule-and-core-context.md](creator-schedule-and-core-context.md) §6 + §11. |
| OQ-5  | `nexus42 schedule add/update/remove/inspect` semantics — editing in-flight                              | **Answered in WS7** — full CRUD; in-flight edits accepted but take effect at next state transition ("core_context is stable during execution"). See §3.3 + §6.4. |
| OQ-6  | Timer / clock model for wall-clock triggers                                                             | **Partially answered** — V1.4 on-demand only; `scheduled_at` column reserved; V1.5 adds clock poller zero-migration. See WS7 §2 + §10. |
| OQ-7  | Multi-agent per creator (worker hosts > 1 agent)                                                        | **Still deferred** to V1.5+ (see WS7 §13).                                                            |
| OQ-8  | User-authored capabilities (shell / WASM plugin ABI)                                                    | **Still deferred** to V1.5+ (see WS7 §13).                                                            |

---

## 12. Coordinated Work Tracks and Knowledge Doc Revisions

This document defines the **orchestration engine design itself** — workstream ordering, effort estimation, and program-level coordination with the `schemas/` boundary refactor live in **[v1.4-delivery-compass-v1.md](../../iterations/v1.4-delivery-compass-v1.md)**. Refer to that compass for:

- How WS1–WS4 of this spec map to V1.4 waves and milestones.
- How the `schemas/` boundary refactor (formerly noted here as a "parallel small plan") is formalised as **WS5** of the V1.4 delivery compass.
- Cross-repo dependency rules (`nexus-platform`, ACP registry), minimum regression gate, and risk register.

If you landed on this section looking for the `schemas/` refactor scope, open the compass's **§4 WS5** directly.

### 12.1 Superseded knowledge documents (v1 → v2)

| v1 (preserved, now carries superseded-by pointer) | v2 (new; authoritative)                                         |
| ------------------------------------------------- | --------------------------------------------------------------- |
| [daemon-lifecycle-api-legacy.md](../../archived/knowledge/daemon-lifecycle-api-legacy.md) (archived) | [daemon-lifecycle-api.md](../../archived/knowledge/daemon-lifecycle-api.md)  |
| [acp-client-tech-spec-legacy.md](../../archived/knowledge/acp-client-tech-spec-legacy.md) (archived) | [acp-client-tech-spec.md](acp-client-tech-spec.md)  |

**Archived 2026-04-17** (historical): v1 lifecycle/ACP companion files moved to `.mstar/archived/knowledge/`. This orchestration-engine spec remains **active** under `.mstar/knowledge/specs/` (structure paths in §3–§8 may lag implementation; semantics remain authoritative).

---

## 13. Risks and Mitigations

| Risk                                                                          | Likelihood | Impact | Mitigation                                                                                                           |
| ----------------------------------------------------------------------------- | ---------- | ------ | -------------------------------------------------------------------------------------------------------------------- |
| `graph-flow` breaking change on 0.3.x / 0.4.x before we reach V1.5             | Medium     | Medium | Adapter trait (§4.2); pin `=0.2.3`; isolated in `nexus-orchestration`; swap impl if needed                           |
| `statig` breaking change                                                      | Low        | Low    | statig is 0.3.x; HSM description is small (~200 LOC); trivial to re-implement by hand if library diverges            |
| LocalSet contagion into daemon runtime axum runtime                               | Low        | High   | **Structural**: `nexus-acp-host` never linked from daemon runtime (§3.2 crate matrix); enforced by `Cargo.toml` review   |
| IPC protocol ambiguity → worker hangs                                          | Medium     | High   | Strict JSON-RPC 2.0; request id matching; `worker/health` heartbeat every 5 s; daemon-side timeout 30 s per request   |
| SQLite session table growth (many paused sessions)                            | Medium     | Low    | Capability `session.compact`; configurable retention for `completed`/`failed` sessions (default: keep 30 days)        |
| Preset bundle path traversal                                                  | Low        | High   | Loader validates every `template_file` with `path_clean` + `canonicalize` + "within bundle root" check              |
| User-authored preset with malicious `prompt` injecting tool requests          | Medium     | Medium | Default `tool_policy: auto_grant_read_only` for user presets; `auto_grant_all` only allowed for embedded system preset |
| Worker crash during inner-graph mid-run leaves orphan inner session row       | Medium     | Low    | On worker crash signal, engine scans children of the outer session and flips them to `paused` with reason            |
| ACP registry offline at worker start                                          | Medium     | Medium | Worker Manager retries with backoff; surface `Degraded` to HSM if unresolved for >5 minutes                           |
| `_system.maintenance` infinite-loop bug stalls sync                           | Low        | High   | Embedded preset has mandatory unit-test gate in CI; `statig` observability hooks log transitions                     |

---

## 14. References

Internal:

- [acp-client-tech-spec.md](acp-client-tech-spec.md) — companion spec for ACP host split and worker-delegated hosting
- [daemon-lifecycle-api.md](../../archived/knowledge/daemon-lifecycle-api.md) — companion spec for the 6-state HSM (closes TD-9)
- [architecture-alignment-review.md](../../archived/knowledge/architecture-alignment-review.md) — TD matrix; §2.6 TD-9 row updated to "Resolved via v2" after Phase 4 ships
- [local-db-refactor.md](../../archived/knowledge/local-db-refactor.md) — `nexus-local-db` ownership rules for the new `orchestration_sessions` table. See [local-db-refactor.md §4](../../archived/knowledge/local-db-refactor.md#4-modularization-plan) for pool sharing model.
- [acp-client-tech-spec-legacy.md](../../archived/knowledge/acp-client-tech-spec-legacy.md) — archived; do not rely on directly (see Superseded header)
- [daemon-lifecycle-api-legacy.md](../../archived/knowledge/daemon-lifecycle-api-legacy.md) — archived; do not rely on directly (see Superseded header)

External (stable, public):

- graph-flow (rs-graph-llm): https://github.com/a-agmon/rs-graph-llm — v0.2.3
- statig: https://github.com/mdeloof/statig — v0.3.x (hierarchical state machines)
- ACP Protocol: https://agentclientprotocol.com/
- ACP Registry (public CDN): https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json
- `agent-client-protocol` crate: https://crates.io/crates/agent-client-protocol — `=0.10.4` per `acp-client-tech-spec.md` §1.2

---

*End of specification. The companion knowledge documents ([daemon-lifecycle-api.md](../../archived/knowledge/daemon-lifecycle-api.md), [acp-client-tech-spec.md](acp-client-tech-spec.md), [creator-schedule-and-core-context.md](creator-schedule-and-core-context.md)) fill in details that would otherwise clutter this document; read them together when extending orchestration.*
