# Orchestration Engine — Design Specification

**Status**: Shipped (V1.4–V1.186 — orchestration engine SSOT, preset loader, Host-mediated prompt execution, capability registry). **V1.39 target**: DF-53 on_complete auto-chain + DF-68 boot resume policy. **V1.62 Shipped**: §5.2 `narrative.compute` capability + §8.4 `combat-engine` preset. **V1.179 P2 shipped**: DR-06 bounded joins. **V1.186 shipped**: §15 durable execution completeness and Host cutover.
**Document class**: Master  
**Pillar (V1.122)**: **Harness** — this spec is the control-strategy engine contract for the [Harness](../../STRATEGY.md) pillar (orchestration engine + agent host + capability registry + presets). Harness is the "how an author harnesses AI agents to execute creative work" pillar; the user-visible Strategy/Strategies → **Harness** product rename shipped V1.156 P3; internal identifiers remain `strategy`/`preset` (architect LOCKED).
**Author**: @project-manager (brainstorm consolidation) / to be co-authored by @architect before first implement
**Date**: 2026-04-17; **Last updated**: 2026-09-08 — V1.186 Host execution cutover
**Scope**: daemon runtime, `crates/nexus-acp-host`, `crates/nexus-orchestration`, `nexus42` CLI, and preset bundle format.
**Supersedes**: — (new topic)
**Coordinates with**:

- [local-cloud-crate-architecture.md](local-cloud-crate-architecture.md) — crate owners for sync/memory capabilities (§5.2 target names; legacy `nexus-sync` / `nexus-domain` until V1.21)
- [acp-client-tech-spec.md](acp-client-tech-spec.md) — ACP transport/provider contract and `nexus-acp-host` crate spec
- TD-9 closed: full 6-state HSM lifecycle (status moves from "gap" to "closed")

**Non-goals** (explicit):

- Creator **Schedule** (multi-Schedule queueing, priority, preemption, CRUD by ID, `core_context` derivation and versioning) — **now folded into V1.4 as WS7**, designed separately in [creator-schedule-and-core-context.md](creator-schedule-and-core-context.md). This document defines the engine primitives that WS7 builds on.
- LLM-driven `core_context` summarisation / auto-iteration — V1.4 reserves the data-model variant but does not implement the capability (see [creator-schedule-and-core-context.md](creator-schedule-and-core-context.md) §11); V1.5+.
- Schedule cron / wall-clock triggers — V1.5+ (schema ready in V1.4).
- Preset third-party registry / signing / publish — V1.5+.
- Full `schemas/` vs local-type boundary refactor — **WS5** of V1.4, designed separately in `schemas-boundary.md`; parallel to WS2 of that compass.

> This document is the orchestration engine design from the 2026-04-17 brainstorming session, updated in place for the shipped Host execution plane. Schedule + core_context work is tracked in [creator-schedule-and-core-context.md](creator-schedule-and-core-context.md); §11 retains the reconciliation table.

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Scope and Responsibility Split](#2-scope-and-responsibility-split)
3. [Architecture Overview](#3-architecture-overview)
4. [Orchestration Engine (graph-flow integration)](#4-orchestration-engine-graph-flow-integration)
5. [Capability Registry](#5-capability-registry)
6. [Host-owned ACP execution](#6-host-owned-acp-execution)
7. [Preset Bundle Format](#7-preset-bundle-format)
8. [Preset Loader (YAML → graph-flow Graph)](#8-preset-loader-yaml--graph-flow-graph)
9. [System Schedule vs Creator Schedule](#9-system-schedule-vs-creator-schedule)
10. [Migration Phases and Task Breakdown](#10-migration-phases-and-task-breakdown)
11. [Open Questions (deferred to B-track)](#11-open-questions-deferred-to-b-track)
12. [Coordinated Work Tracks and Knowledge Doc Revisions](#12-coordinated-work-tracks-and-knowledge-doc-revisions)
13. [Risks and Mitigations](#13-risks-and-mitigations)
14. [References](#14-references)
15. [V1.186 execution completeness (product lock)](#15-v1186-execution-completeness-product-lock)

---

## 1. Executive Summary

### 1.1 Problem Statement

Nexus currently drives ACP agents through **one-shot, human-initiated CLI commands** (`nexus42 agent run <ref>`). The daemon (daemon runtime) is a passive HTTP backend that holds workspace state, sync plumbing, and the ACP tool-mediation endpoint — it does **not** drive creators, does **not** run scheduled work, and has **no notion of a strategy** that spans multiple ACP sessions and multiple days.

Users need to express creator workflows as configurable, prompt-driven strategies — e.g. *"collect inspiration → brainstorm → outline → draft"* — that the daemon executes **autonomously** across creator activations, with stable promptable identity and resumable execution across daemon restarts.

### 1.2 Design Pillars

- **Daemon becomes `orchestration engine + capability registry`**: existing HTTP-era capabilities (sync, workspace ops, outbox flush, registry refresh) are **reclassified as first-class capabilities** invokable as graph nodes; HTTP API retreats to a *trigger/query surface* over the same engine.
- **Strategy shape is hierarchical**: an outer **state machine** (long-lived, cross-session) containing inner **DAG graphs** (short, in-memory prompt/tool call chains) — *graph-of-graphs*.
- **ACP remains external behind the Host plane**: CLI keeps interactive `acp run`; orchestration prompts use the daemon's existing `HostFacade` through an injected `PromptExecutor`. `nexus-acp-host` owns transport and process lifecycle; orchestration never owns provider handles.
- **Runtime is `graph-flow` (outer + inner) + custom SQLite `SessionStorage`**: adapted behind a thin trait layer so the upstream `0.2.x` crate is swappable.
- **Daemon lifecycle is `statig` HSM**: 6-state process lifecycle (`Stopped`/`Starting`/`Running`/`Degraded`/`Stopping`/`Failed`) — closes TD-9.
- **Presets are filesystem bundles**: YAML manifest + companion Markdown prompt templates; loaded dynamically by name; decoupled from compiled Rust code.

### 1.3 Deliverables (end-state of §10 Phase 1–4)

1. `crates/nexus-acp-host` owns ACP transport and managed subprocess lifecycle behind the Host provider plane.
2. `crates/nexus-orchestration` owns graph-flow adaptation, capability registration, preset loading, and SQLite `SessionStorage`.
3. daemon runtime owns orchestration, lifecycle HSM, and the existing `HostFacade` integration.
4. `nexus42` retains interactive `acp run` and schedule commands; the obsolete secondary ACP subcommand is removed.
5. First built-in preset: `_system.maintenance` (mandatory) and one user-facing sample `novel-writing`.
6. Knowledge docs revised: [acp-client-tech-spec.md](acp-client-tech-spec.md).

### 1.4 Effort (agent-oriented)

Per [effort-estimation.md](https://github.com/btspoony/mstar-harness/blob/main/docs/agents/effort-estimation.md) conventions (**agent sessions only; no human time**):

| Phase                                                           | Effort     | Approx. agent sessions |
| --------------------------------------------------------------- | ---------- | ---------------------- |
| Phase 1 — `nexus-acp-host` crate extraction                     | M          | 1–2                    |
| Phase 2 — orchestration skeleton (graph-flow + capability + Host seam) | L          | 2–3                    |
| Phase 3 — preset loader + `novel-writing` end-to-end             | M          | 1–2                    |
| Phase 4 — statig lifecycle HSM (parallelisable with Phase 2)    | S+         | 1                      |
| Totals (excl. Phase 4 parallel savings; see compass WS5 for schemas refactor effort) | **L → XL** | **6–9** |

---

## 2. Scope and Responsibility Split

### 2.1 In scope (this document is authoritative for)

- Runtime architecture of the orchestration engine and capability registry.
- `nexus-acp-host` crate extraction (crate boundary and linkage matrix).
- Run-scoped Host session ownership and ACP transport boundary.
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
| Wire schemas vs local types boundary refactor                                  |  §4 WS5 |
| ACP SDK migration (e.g. to `sacp` v1.0)                                        | Governed by [acp-client-tech-spec.md](acp-client-tech-spec.md) §1.2 adapter-layer policy |

### 2.3 Non-goals (explicit)

- **Not an ACP protocol server promotion**: daemon runtime coordinates the existing Host provider plane; it does not expose ACP as a new public server protocol. See [acp-client-tech-spec.md](acp-client-tech-spec.md) §2.3.
- **Not a LangChain-style in-memory pipeline**: all engine execution is **durable** and **resumable across daemon restart**; in-memory-only pipelines are explicitly rejected.
- **Not a replacement for interactive `nexus42 agent run`**: that path stays direct stdio CLI-to-agent; orchestration does not route through it.

---

## 3. Architecture Overview

### 3.1 Process topology

```text
┌─────────────────────────────────────────────────────────────────┐
│ nexus-daemon-runtime                                            │
│  ┌──────────────┐   ┌──────────────────┐   ┌─────────────────┐ │
│  │ lifecycle HSM│   │ orchestration    │   │ capability      │ │
│  │              │◀──┤ engine + SQLite  │◀──┤ registry        │ │
│  └──────────────┘   └────────┬─────────┘   └─────────────────┘ │
│                              │ PromptExecutor                    │
│                              ▼                                   │
│                     ┌──────────────────┐                         │
│                     │ HostFacade       │                         │
│                     │ session manager  │                         │
│                     └────────┬─────────┘                         │
└──────────────────────────────┼───────────────────────────────────┘
                               ▼
                    nexus-agent-host provider
                               │
                               ▼
                    managed ACP agent process
```

### 3.2 Crate layout (target)

| Crate                                             | New? | Links ACP SDK? | Purpose                                                                                    |
| ------------------------------------------------- | ---- | -------------- | ------------------------------------------------------------------------------------------ |
| `crates/nexus-acp-host`                           | New  | Yes            | ACP transport and owned-process lifecycle behind the Host ACP provider                       |
| `crates/nexus-orchestration`                      | New  | No             | graph-flow adapter, capability trait, preset loader, SQLite `SessionStorage`                  |
| `crates/nexus-daemon-runtime`                     | Ext. | No             | Orchestration engine host, lifecycle HSM, `HostFacade` prompt executor, HTTP trigger/query    |
| `apps/nexus42`                                    | Ext. | Yes (via host) | Interactive `acp run` and schedule command groups                                             |
| `crates/nexus-contracts`, `nexus-creator`, `nexus-cloud-domain`, … | Ext. | No | Application crates per [local-cloud-crate-architecture.md](local-cloud-crate-architecture.md); legacy monolith `nexus-domain` retired after V1.21 |

### 3.3 Data flow for one strategy tick

1. Engine resolves *which creator Schedule is due* (B-track concern; A-track treats the decision as a **trigger input**).
2. Engine loads the preset bundle (cached after first parse), constructs the outer graph-flow `Graph`.
3. Engine opens or resumes a `Session` for `<creator_id, preset_id, instance_id>` using SQLite `SessionStorage`.
4. `FlowRunner::run(session_id)` executes one *step*:
   - Task `run()` resolves the task kind (capability call / inner-graph launch / ACP prompt / judge / manual-wait).
   - For ACP-related tasks: call the injected `PromptExecutor`, which owns a run-scoped Host session and dispatches through `HostFacade`.
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

> **Crate selection cross-reference**: `graph-flow = "=0.2.3"` pinning, `sqlx` adoption for the shared pool, and the general dependency conventions are now governed by [`crate-selection-best-practices.md`](../knowledge/crate-selection-best-practices.md) (see §1 conventions + §2.1/§2.2/§2.3 decisions). This section remains the design SSOT for *how* those crates are integrated; it defers crate-identity and versioning policy to the best-practices document.

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

**Pool ownership (post-WS8)**: `nexus-local-db` exposes `Arc<sqlx::SqlitePool>` as the single workspace pool for `state.db` after V1.4 **WS8** unifies the DB engine on `sqlx` (; decision SSOT: [`crate-selection-best-practices.md`](../knowledge/crate-selection-best-practices.md) §2.3 + §3.3). `SqliteSessionStorage` takes that `Arc<SqlitePool>` at construction time; no separate connection or separate `.db` file. The `orchestration_sessions` table lands as one more `.sql` migration file under `crates/nexus-local-db/migrations/`, authored in WS2 Task 3 **after** WS8 T1–T2.

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

**V1.186 product lock:** the `status` column is the operator-authoritative run state for terminal and wait classes, not a write-only `'running'` bookkeeping field. Preserve shipped migration bytes; any column/codegen change MUST land together. See §15.

### 4.4 Standard `Task` impls (the runtime vocabulary)

Every preset node compiles into one of these Rust `Task` impls:

| Task impl            | Preset node kind         | Behaviour                                                                 |
| -------------------- | ------------------------ | ------------------------------------------------------------------------- |
| `CapabilityTask`     | `capability`             | Resolves to a registered `Capability`, calls its `run(ctx)`, stores output |
| `AcpPromptTask`      | `acp_prompt`             | Calls the injected `PromptExecutor`; stores typed Host output in `Context` |
| `InnerGraphTask`     | `inner_graph`            | Launches a child `Session` over a named inner graph; awaits completion    |
| `JudgeTask`          | `llm_judge` exit_when    | Calls the declared judge capability (default `judge.llm`) through the same `PromptExecutor` |
| `ManualWaitTask`     | `manual` exit_when       | Returns `NextAction::WaitForInput`; CLI `advance` resumes                 |
| `RuleCheckTask`      | rule-based exit_when     | Pure function over `Context`; no external calls                           |
| `TimerWaitTask`      | `timer` exit_when (opt.) | Returns `WaitForInput` plus schedules a clock signal                       |

All impls live in `crates/nexus-orchestration/src/tasks/`. Task implementations are **pure** over `Context` + well-typed capability handles — no global state.

#### 4.4.1 `llm_judge` runtime contract (V1.33 — Implemented)

**Pre-V1.33 problem (resolved in V1.33 P3):** `StateCompositeTask` used to route `exit_when.kind: llm_judge` through `JudgeTask` that only evaluated stub `_judge_rule` (`always_true` / `always_false`) without calling the declared `judge_capability` or loading `template_file`.

**Required behavior (V1.33+):**

1. Read `judge_capability` from state YAML (default `judge.llm`).
2. Load `template_file` relative to bundle root; pass rendered template + `contextData` to the capability.
3. Require an injected `PromptExecutor`, trusted run identity, registered cancellation token, and narrowing permission scope; otherwise fail closed and wait per preset policy.
4. Parse GO/NOGO from capability output (existing `judge.llm` word-list contract).
5. Map GO → `NextAction::Continue`; NOGO → `NextAction::WaitForInput` (respect `min_interval` if set).
6. **Identity**: use schedule-injected `_creator_id` / `_session_id` only — not raw preset args (aligns with V1.32 `SEC-V131-01`).

**Explicit non-goal:** conditional `next` on NOGO (e.g. return to `gathering`) remains deferred until loader accepts `next.kind: conditional`.

> **Durable roadmap:** DR-11 (conditional-next routing).

### 4.5 Pausing, cancelling, and signals

- **Pause** (user or Schedule): `engine.signal(session, Pause)` — flips status to `paused`; `FlowRunner` refuses to advance until `Resume`.
- **Manual advance**: `engine.signal(session, Resume)` — returns a `NextAction::Continue` on next `run_step`.
- **Cancel**: `engine.signal(session, Cancel)` — cascades to any child inner-graph sessions; each `Task` impl must be *cancellation-safe* (stop after the current await point; no half-committed writes). **V1.186:** cancel MUST reach active Host/ACP work; it MUST NOT advertise rollback of already committed external effects.
- **Kill** (daemon stop): lifecycle HSM `Stopping` state sends `Cancel` to every active session before shutting the engine down.

---

## 5. Capability Registry

> **Crate selection cross-reference**: Capability implementations MAY depend on third-party crates (e.g. `notify` for file-watch capabilities, `jsonwebtoken` for auth-related capabilities). Any new crate introduced here follows [`crate-selection-best-practices.md`](../knowledge/crate-selection-best-practices.md) §1 (conventions) — in particular §1.5 (PM introduction gate) and §1.3 (feature flag whitelist).

### 5.1 `Capability` trait

```rust
#[async_trait]
pub trait Capability: Send + Sync {
    fn name(&self) -> &'static str;
    fn input_schema(&self) -> &'static str;
    fn output_schema(&self) -> &'static str;
    async fn run(&self, input: Value) -> Result<Value, CapabilityError>;
}

#[async_trait]
pub trait PromptExecutor: Send + Sync {
    async fn execute(&self, request: PromptRequest) -> Result<PromptResult, CapabilityError>;
    async fn finalize_run(&self, run_id: &str) -> Result<(), CapabilityError>;
}
```

Runtime dependencies are injected through `CapabilityRuntimeDeps`; graph context carries serializable values only. Provider, process, and transport handles remain behind the daemon's `HostFacade`.

### 5.2 Built-in capabilities (first release)

All capabilities below are registered at daemon runtime startup. Adding a new capability is a Rust code change (not user-config) for V1.4. User-authored capabilities are **out of scope** (residual for V1.5+).

| Name                        | Purpose                                                        | Owner crate (target)   | Runtime status |
| --------------------------- | -------------------------------------------------------------- | ---------------------- | -------------- |
| `sync.pull`                 | Pull remote deltas (replaces HTTP-era trigger)                 | `nexus-cloud-sync`     | Deferred wiring — tracked DF-46 / PD-05 (§2.3) |
| `sync.push`                 | Push local outbox (replaces HTTP-era trigger)                  | `nexus-cloud-sync`     | Deferred wiring — tracked DF-46 / PD-05 (§2.3) |
| `outbox.flush`              | Flush pending outbox entries                                   | `nexus-orchestration`  | **Shipped (V1.59)** — local drain via `nexus-local-db` pool; see §5.7 |
| `outbox.compact`            | Compact outbox table                                           | `nexus-orchestration`  | **Shipped (V1.59)** — retention-window compaction via `nexus-local-db` pool; see §5.7 |
| `workspace.open`            | Ensure workspace dir is present and valid                      | `nexus-home-layout`    | Deferred wiring (DF-31) |
| `workspace.commit`          | Commit manuscript diff into working copy                       | `nexus-home-layout`    | Deferred wiring (DF-31) |
| `registry.refresh`          | Refresh ACP registry cache                                     | `nexus-acp-host`       | Deferred network/CDN wiring (DF-29) |
| `creator.read_memory`       | Query persisted creator memory fragments                       | `nexus-creator-memory` | **Real** — SQLite-backed query through `CreatorCapabilityStore` (V1.31 DF-30) |
| `creator.write_memory`      | Persist creator memory fragments and return real `fragment_id` | `nexus-creator-memory` | **Real** — SQLite-backed write through `CreatorCapabilityStore` (V1.31 DF-30) |
| `creator.inject_prompt`     | Queue a prompt to be sent on next `acp.prompt`                 | `nexus-orchestration`  | **Real** — persisted injection queue in `state.db` (V1.31 DF-30) |
| `acp.prompt`                | Send a run-scoped prompt through the Host plane                 | `nexus-orchestration`  | **Real** — injected `PromptExecutor`; missing dependencies fail closed |
| `kb.extract_work`           | Extract KB assets from a work entry into a World               | `nexus-orchestration` (preset-driven via `acp_prompt`) | Real |
| `soul.experience.aggregate` | Aggregate SOUL Experience section from session review items    | `nexus-orchestration` (preset-driven via `acp_prompt`) | Real |
| `judge.llm`                 | Evaluate a go/nogo prompt using a judge role                   | `nexus-orchestration`  | **Real** — `PromptExecutor` with `deny_all`, GO/NOGO parse |
| `judge.rule`                | Evaluate a pure rule over `contextData`                        | `nexus-orchestration`  | **Real** — boolean literals, field equality/inequality, numeric comparisons |
| `context.summarize`         | Summarize context through a Host-mediated prompt               | `nexus-orchestration`  | **Real** — returns `{ summary, prompt_hash }` |
| `narrative.compute`         | Invoke WASM compute module; apply state_delta, timeline_events, new_key_blocks, return battle_report | `nexus-orchestration`  | **Real** — calls `nexus-wasm-host::compute()` (V1.61 P3; spec-seal V1.62 P2); see §8.4.1 |
| `timer.wait_until`          | Schedule a wake-up signal (requires B-track clock)             | `nexus-orchestration`  | Deferred clock integration — **Durable roadmap:** DR-12 |

> **V1.186 Host cutover:** all prompt-backed capabilities share the injected `PromptExecutor`; standalone constructors fail closed rather than installing an echo or secondary transport fallback.

### 5.3 Capability input/output schemas

Each capability ships its `input_schema` and `output_schema` as constants (JSON Schema draft 2020-12) in Rust. **These schemas are local** (per [schemas-external-consumer-boundary.md](schemas-external-consumer-boundary.md)) and live under `crates/nexus-contracts/src/local/orchestration/` (or adjacent module), **not** under `schemas/` — they are not wire contracts.

> **Daemon builds:** `sync.*` MUST NOT call `nexus-cloud-sync` on the daemon hot path; see [local-cloud-crate-architecture.md](local-cloud-crate-architecture.md) §7. `outbox.flush` / `outbox.compact` (V1.59) are local-only pool-backed capabilities that operate directly on `outbox_entries` via `nexus-local-db` — they do not depend on `nexus-cloud-sync`.

### 5.4 Capability errors

`CapabilityError` distinguishes invalid input, transient/permanent external failure, provider availability, ACP session loss, cancellation, and internal failure. Engine translates these into graph-flow failure/wait behavior and persisted `_error` context so presets can branch without treating partial output as success.

### 5.5 Runtime dependency injection (V1.31; Host cutover V1.186)

Runtime-backed capabilities receive external dependencies through `CapabilityRegistry::with_runtime_deps()` or the pool-aware `CapabilityRegistry::with_builtins_and_pool()` factory. The registry owns shared adapters rather than letting capabilities discover global state.

- **Creator memory adapter**: `CreatorCapabilityStore` is injected for `creator.read_memory`, `creator.write_memory`, and `creator.inject_prompt`.
- **Prompt executor**: `acp.prompt`, `judge.llm`, `context.summarize`, `nexus.llm.extract`, and graph `acp_prompt` call the injected `PromptExecutor`. The daemon implementation uses its existing `HostFacade`, scopes sessions by run and role, narrows tool policy per operation, and persists typed outcomes. Missing executor, run identity, cancellation token, or workflow permission scope fails closed.
- **Standalone/test mode**: constructors without runtime dependencies return typed unavailability; deterministic behavior belongs in explicit test executors, never a production echo fallback.

### 5.6 `creator.inject_prompt` queue semantics (V1.31)

`creator.inject_prompt` persists prompt injections in `state.db` in the `creator_prompt_injections` table. Rows are scoped to the creator/session context and are consumed by the next `acp.prompt` on the same creator session.

Operational semantics:

1. `creator.inject_prompt` appends a queued prompt row and returns queue metadata.
2. The next run-scoped `acp.prompt` drains pending injections before dispatch through `PromptExecutor`.
3. Consumed rows are marked/drained transactionally with prompt dispatch so restart does not lose unconsumed injections.
4. Injection augments prompt text only; it does not bypass Host admission or the per-operation permission scope.

### 5.7 Outbox consolidation (V1.59)

The dual-outbox architecture identified in TD-8 (`dual-outbox-architecture.md`) was consolidated in V1.59. The unified outbox schema uses `outbox_entries` / `partial_apply_states` (migration `20260420_outbox_tables.sql`) as the single source of truth.

**Single-writer rule**: each outbox event type has exactly one authorized writer subsystem. `nexus-cloud-sync::outbox::Outbox` owns sync push/pull commands. `nexus-orchestration` capabilities (`outbox.flush`, `outbox.compact`) own maintenance operations. The daemon legacy `outbox` queue table had no active Rust-level consumers (confirmed by the V1.59 T3 audit) and was dropped at V1.163 by migration `20260812_drop_legacy_outbox.sql`.

**Flush/compact invocation path**:
- `outbox.flush` (`OutboxFlush`, pool-backed): drains pending (`staged`/`ready`) entries by marking them `acked`. Input: optional `limit`. Output: `{ flushed: N }`.
- `outbox.compact` (`OutboxCompact`, pool-backed): deletes `acked` entries older than a configurable retention window (default 7 days). Input: optional `retentionDays`. Output: `{ removed: N, retained: M }`.
- Both capabilities are local-only (platform paused). Full semantics and test vectors are defined in the Master spec [outbox-consolidation.md](outbox-consolidation.md) (Normative).

**Pool injection**: both capabilities receive `sqlx::SqlitePool` through `with_pool()` constructors, following the same pattern as `kb.extract_work`, `novel.project_scaffold`, and other pool-backed capabilities. The `with_builtins_and_pool()` and `with_runtime_deps()` registry factories inject the pool.

---

## 6. Host-owned ACP execution

### 6.1 Session lifecycle

- The daemon's `HostPromptExecutor` lazily creates one Host session per `(run_id, role)` and never shares a subprocess across runs or Creators.
- `HostFacade::create_session` validates the Creator owner and canonical workspace before the provider launches an ACP child in that workspace.
- Prompt dispatch persists a revision-fenced attempt before the external effect and records the opaque owned-process identity after launch.
- Terminal completion, failure, and cancellation call bounded `HostFacade::shutdown_session`; cache/token eviction occurs only after cleanup is confirmed.
- Cancellation persists intent, fires the run token, drains/cancels the Host operation, confirms process-group teardown, and only then persists terminal `cancelled`. Unconfirmed cleanup remains `interrupted`.
- Restart recovery never blindly replays an ambiguous in-flight ACP/Host effect.

### 6.2 Transport and ownership boundary

ACP stdin/stdout transport and process-group control remain internal to `nexus-acp-host` and the Host ACP provider. Orchestration receives only the narrow `PromptExecutor` result and opaque process identity; provider handles and transport clients never enter graph context or checkpoints.

The retired secondary daemon subprocess/IPC prompt plane is not supported. CLI-only `nexus42 acp run` remains an independent interactive transport entry point.

### 6.3 Tool policy

Workflow prompt policy is carried as a per-operation narrowing scope:

- `auto_grant_all`
- `auto_grant_read_only`
- `deny_all`
- `request_policy` (refused on the non-interactive workflow path until an approval channel exists)

The Host provider intersects this scope with its admitted configuration; it never elevates permissions. Schedule tool execution remains an in-process `HostToolExecutor::dispatch_for_schedule()` lane through the single capability registry. Peer and embedded-MCP admission remain separate retained callers.

### 6.4 Backpressure and streaming

Host providers emit bounded `HostEvent` streams. Only message deltas followed by `FinishReason::EndTurn` constitute prompt success. EOF, refusal, cancellation, timeout, and provider failure are typed non-success outcomes; partial output is never committed as completed work.

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
3. Preset shipped in the binary (via `include_dir!`) under `nexus-orchestration/embedded-presets/<id>/` (currently `essay-writing`, `_system.maintenance`, `novel-writing`, `novel-chapter-review`, `memory-augmented`)

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

### 7.5 Conditional `next` + Converge State Kinds

**Normative SSOT:** [preset-conditional-routing.md](preset-conditional-routing.md) — **Normative** (all overlays promoted V1.158 P3). This subsection is a cross-reference; the full conditional schema, expression grammar, multi-branch routing, and merge semantics live in that document.

**Converge (merge-point) state kind** (V1.56 P2 fix-wave, H-001/W-002): states may declare a `converge` config with a `strategy` field to act as explicit join points for multiple incoming edges:

```yaml
states:
  - id: merged
    converge:
      strategy: wait_for_all   # default
    enter: []
    exit_when: { kind: manual }
    next: done
```

Converge strategies:
  - **`wait_for_all`** (default): all incoming edges must arrive before advancing
  - **`first_completed`**: advance on first arrival
  - **`any`**: idempotent first-arrival advance

Runtime enforcement lives in `StateCompositeTask::run()` via the converge gate. Predecessor tracking is populated at graph build time. Source states record arrivals via `_converge_arrivals_{target_id}` in context.

**Bounded joins** (DR-06, v1.179): join states (carrying `merge:` or
`converge:`) may set additive `timeout_ms` / `on_timeout` fields to bound
the wait — deadline expiry reroutes to the `on_timeout` state or fails
with the typed `converge_timeout:` error naming gate, state, arrivals, and
elapsed time. One field pair serves both gates; the normative field table
and semantics live in
[preset-conditional-routing.md §3.3.3](preset-conditional-routing.md)
("Bounded joins"). The deferred `wait_for_all_timeout_seconds` name is
retired.

**Expression depth limit** (V1.56 P2 fix-wave, W-003): `MAX_EXPR_DEPTH = 32` bounds parsing depth to prevent stack overflow from user-installable presets.

```yaml
next:
  kind: conditional
  rules:
    - when: "{{state.brainstorming.output | length > 2000}}"
      to: outlining
    - when: "{{state.brainstorming.output | contains 'unclear'}}"
      to: gathering
  default: outlining
```

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

V1.32 introduces a **shared semantic validation facade** used by both the CLI/API validate endpoint (`POST /v1/daemon/presets:validate`) and the orchestration loader. The facade is the single quality gate; there are no parallel weaker checks.

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
| `novel-chapter-review` | FL-E `review` stage — novel/work/chapter-aware review producer (findings writer, V1.47) | load_chapter → review → done | `creator.inject_prompt`, `acp.prompt` |
| `memory-augmented` | Memory demonstrator | recall → generate → persist → done | `creator.*`, `judge.rule` |
| `creative-brief-intake` | **V1.33 Shipped** ( P2 plan) — grill-me intake preset | intake → done | `acp.prompt` |
| `essay-writing` | **V1.63 P2 Shipped** — essay production preset with 4-dimension quality rubric (thesis clarity, evidence support, coherence, ending takeaway) | intake → outline → draft → revise → finalize → finalize_commit → done | `creator.inject_prompt`, `acp.prompt`, `judge.llm`, `essay.draft_status.finalize` |

All shipped presets use **linear** `next` transitions unless noted; conditional routing remains deferred (§7.5). **Durable roadmap:** DR-11.

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
- `creator bootstrap` filters intake presets where `work_init ∈ run_intents`.
- `creator run continue` filters presets where `work_continue ∈ run_intents`.

Normative classification table: [work-experience-model.md](work-experience-model.md) §5.2.

### 7.9 Preset `gates` (V1.36 — Implemented)

Presets may declare **precondition gates** that the orchestration engine evaluates **before scheduling** the preset for execution. Gates let a profile require specific Work fields, filesystem state, or prior-preset completion — turning implicit "this should already be true" expectations into enforced preconditions with structured error reporting.

```yaml
preset:
  id: novel-writing
  # ...
  run_intents: [work_init, work_continue]
  gates:
    - kind: work_field
      field: work_profile
      op: equals
      value: novel
    - kind: work_field
      field: work_ref
      op: required
    - kind: work_field
      field: intake_status
      op: equals
      value: complete
    - kind: filesystem
      path: "Works/{{work_ref}}/"
      must_exist: true
    - kind: previous_preset
      preset: novel-project-init
      status: complete
      scope: work            # same work_id
```

#### 7.9.1 Gate kinds (closed set)

| `kind` | `op` / required keys | Semantics |
| --- | --- | --- |
| `work_field` | `field` (string, dot-path), `op` (closed enum), `value` (any, op-dependent) | Query the `works` table for the bound `work_id`; check field against op. `op` enum: `equals` \| `not_equals` \| `required` (non-null) \| `in` (`value: [v1, v2]`) \| `not_in`. |
| `filesystem` | `path` (string, preset-input-var-substituted), `must_exist: true \| false` | Resolve `path` against workspace root (with `{{work_ref}}` and other preset input vars substituted); check existence. Symlink-safe (canonicalize first; see §7.6.1 path-safety rules). |
| `previous_preset` | `preset` (string, id), `status` (closed enum: `complete` \| `any_session`), `scope: work` (only `work` is normative V1.36) | Query the orchestration session log for the named preset, scoped to the same `work_id`; check the named status. `any_session` accepts completed/paused/waiting_for_input; `complete` requires terminal-completion. |

#### 7.9.2 Evaluation timing and contract

1. **Load-time validation** (§7.6 facade): each gate is schema-validated. Unknown `kind` → error. Unknown `op` → error. `field` must be a known column on `works` table (or a known entity extension column). `path` must pass path-safety (no `..` escape, no absolute path).
2. **Enqueue-time evaluation**: the engine evaluates the gate list **immediately before enqueuing** a preset for execution (after preset-input-var binding, before the first state runs). All gates must pass.
3. **Failure behavior**:
   - Return a structured error to the caller (`creator bootstrap` / `creator run <preset_id>` / schedule API):

     ```json
     {
       "error": "preset_gates_failed",
       "preset_id": "novel-writing",
       "work_id": "wrk_abc",
       "failed_gates": [
         { "kind": "filesystem", "path": "Works/cozy-mystery/", "must_exist": true, "actual": "missing",
           "remediation": "Run `creator bootstrap --init-preset novel-project-init` first." },
         { "kind": "work_field", "field": "intake_status", "op": "equals", "expected": "complete", "actual": "pending",
           "remediation": "Complete intake via `creator bootstrap --preset creative-brief-intake`." }
       ]
     }
     ```
   - Preset is **not enqueued**; no state runs.
4. **`--force` override** (audit-logged): `creator bootstrap --force-gates` or `creator run <preset_id> --force-gates` skips gate evaluation. The override records `forced: true`, the user identity, and a free-text reason (when provided) to the audit log. Forced runs still respect the engine's other invariants (capability registry, run_intents, etc.).
5. **Idempotency**: gate evaluation is read-only; it does not mutate Work state. A failed gate check leaves no side effects.

#### 7.9.3 Relationship to other constraints

| Constraint | Scope | Enforced at | Authoritative spec |
| --- | --- | --- | --- |
| `run_intents` (V1.33) | Coarse: which `creator run` subcommand surfaces the preset | CLI dispatch time | §7.8, [work-experience-model.md](work-experience-model.md) §5 |
| `requires_capabilities` (V1.4) | Capability availability at engine startup | Loader | §7.2 |
| `gates` (V1.36) | Per-invocation preconditions (Work fields, filesystem, prior-preset) | Enqueue time | §7.9 |
| `stage` gates (V1.34) | FL-E linear stage ordering (`intake → research → produce → review → persist`) | `creator run <preset_id>` (preset runner validates before enqueue) | [creator-workflow.md](creator-workflow.md) §3.3 |

Gates are **additive** to `run_intents` and `stage` gates; they do not replace either. A preset can declare any combination. Profile-specific gate sets (e.g. novel profile's `Works/<work_ref>/` requirement) live in the profile overlay spec, not in this Master.

#### 7.9.4 Worked example: novel-writing

Full gate set for `novel-writing` is documented in [novel-writing/workflow-profile.md §5.3](./novel-writing/workflow-profile.md). The Master here defines the mechanism; the Draft overlay defines the values.

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

CLI and daemon `POST /v1/daemon/presets:validate` call these functions directly without constructing a graph, so validation diagnostics are available without full loader overhead.

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

> **Durable roadmap:** `next.kind=conditional` loader acceptance (the `(future)` row above) is DR-11.

### 8.3 Caching and reloading

- Loader caches `LoadedPreset` keyed by `source_hash`.
- On `registry.refresh` capability call or the shipped Daemon API `POST /v1/daemon/presets/{id}:reload`, loader recomputes hash; if changed, invalidates cache and rebuilds. There is currently no top-level `nexus42 preset reload` CLI.
- Running sessions continue on the previous graph (snapshot semantics); new sessions pick up the new graph.

### 8.4 `narrative.compute` capability and `combat-engine` preset (V1.62 P2 — Normative)

**Status**: Normative — V1.62 Shipped (deferred from V1.61 P3).

V1.61 introduced the `nexus-wasm-host` crate and the `narrative.compute`
orchestration capability; the capability registration and preset were deferred
to V1.61 P3 and are now documented here as shipped.

#### 8.4.1 `narrative.compute` capability

**Name (registry key):** `narrative.compute`

**Crate:** `nexus-orchestration` (`capability::builtins::NarrativeCompute`).

**Scope:** orchestration-scope capability. Registered in the `CapabilityRegistry`
at daemon boot.

**Input:**

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `world_ref` | object | yes | World and timeline locator for the compute invocation. |
| `module_id` | string | yes | ID of the compute module to invoke (e.g., `"basic-combat"`). |
| `key_block_ids` | array of string | no | Specific KnowledgeEntry IDs to include in `ComputeInput.key_blocks`. When omitted, the capability queries for all computable KnowledgeEntries matching the module's `required_key_block_types`. |
| `invocation` | object | no | Module-declared freeform input parameters. Passed through to `ComputeInput.invocation`. |

**Output:**

| Field | Type | Description |
| --- | --- | --- |
| `state_delta_applied` | array of `StateDelta` | Deltas applied to computable KnowledgeEntry bodies. |
| `timeline_events_appended` | array of `TimelineEvent` | Events appended to the timeline. |
| `new_key_blocks_upserted` | array of `KnowledgeEntry` | New KnowledgeEntries created. |
| `battle_report` | object | Module-declared freeform report. |

**Execution flow:**

```text
1. Resolve module_id → WasmModule from the host's module cache (wasm-host.md §2.2).
2. Query computable KnowledgeEntries from the World KB filtered by module's
   required_key_block_types (see compute-module-abi.md §7.1).
3. Build ComputeInput envelope: world_ref + key_blocks snapshot +
   narrative_state + invocation.
4. Call WasmEngine::compute(module, input) → ComputeOutput.
5. Apply state_delta to computable KnowledgeEntry bodies (atomic; no partial apply).
6. Upsert new_key_blocks into the World KB.
7. Append timeline_events to the timeline.
8. Return battle_report to the caller.
```

**Error handling:** compute failure (any `ComputeError` variant) is surfaced as a
`CapabilityError` and a `TimelineEvent` with `event_type: "compute_error"` is
appended to the timeline. The daemon does not crash on compute failure.

**Related:** [compute-module-abi.md](./compute-module-abi.md) (module ABI contract),
[wasm-host.md](./wasm-host.md) (host runtime), [entity-scope-model.md](./entity-scope-model.md)
§5.5.9 (computable-flag semantics).

#### 8.4.2 `combat-engine` preset

**Preset ID:** `combat-engine`

**Pattern / role:** User-triggered combat resolution (V1.61 Q7).

**State flow:**

```text
load_world → compute → apply_delta → advance_timeline → done
```

| State | Description |
| --- | --- |
| `load_world` | Load world context: resolve combatants from the World KB, validate they are computable (`computable: true`), select the `basic-combat` module. |
| `compute` | Invoke `narrative.compute` capability with `module_id: "basic-combat"` and the selected combatant KnowledgeEntry IDs. |
| `apply_delta` | The `narrative.compute` capability applies the state delta, upserts new KnowledgeEntries, and appends timeline events. This state is a no-op in the preset (the capability already performed the side effects); it exists as an explicit checkpoint for observability. |
| `advance_timeline` | Advance the world timeline past the combat outcome. Append a `story_advance` timeline event summarizing the combat result. |
| `done` | Terminal state. |

**Primary capabilities:** `narrative.compute`

**Prompt templates (per state, under `embedded-presets/combat-engine/prompts/`):**

| State | Template | Purpose |
| --- | --- | --- |
| `load_world` | `prompts/load-world.md` | World context assembly prompt (combatant selection, narrative framing). |
| `compute` | (none — invokes capability directly) | The `compute` state delegates entirely to `narrative.compute`. |
| `apply_delta` | `prompts/apply-delta.md` | Summarize the applied deltas for the user. |
| `advance_timeline` | `prompts/advance-timeline.md` | Narrative framing for the combat outcome timeline event. |

**Registration:** `combat-engine` is registered in `preset_version_for_id` and
included in the preset sync test suite. The preset is **not** embedded in the
binary in V1.62 — it is a filesystem-loaded preset under
`crates/nexus-orchestration/embedded-presets/combat-engine/`.

**Related:** [compute-module-abi.md](./compute-module-abi.md) §7.4 (basic-combat
manifest example), [wasm-host.md](./wasm-host.md) (sandbox limits applied during
`compute`).

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
- **Concurrency contract** — multi-Schedule per creator is supported; prompt operations serialize per run, while distinct admitted runs own separate Host sessions. Capability-only Schedules may fully parallel.
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

Compass WS5 (`schemas/` boundary refactor) is fully parallel and has no dependencies on this spec's phases — see  §4 WS5 for detailed scope.

### 10.2 Phase 1 — `nexus-acp-host` crate extraction (M; 1–2 agent sessions)

**Scope**

- Create `crates/nexus-acp-host/` with modules `client`, `transport`, `skills`, `registry`, `error`, `capabilities` (capability ID constants relocated).
- `git mv` existing files from `apps/nexus42/src/acp/*` preserving history where possible.
- Update `apps/nexus42` to `use nexus_acp_host as acp` and re-export for existing call-sites in `commands/agent.rs`.
- Register the ACP provider recipe with the existing Host plane; no secondary daemon subprocess CLI entry point.
- `Cargo.toml` workspace updates; update `rust-toolchain`, CI matrix, and `verify-codegen` (no codegen impact expected).
- Update `acp-client-tech-spec.md` §11 with final crate layout.

**Acceptance**

- [ ] `cargo build --workspace` clean
- [ ] `cargo test --workspace` green (existing ACP tests move with the crate)
- [ ] `cargo +nightly fmt --all -- --check` clean
- [ ] `cargo clippy --all -- -D warnings` clean
- [ ] `nexus42 agent list`, `show`, `probe --registry`, `run` **functionally unchanged** (manual + existing integration tests)
- [ ] Host ACP provider launches lazily in the admitted Creator workspace and reaps its owned process group on shutdown

### 10.3 Phase 2 — Orchestration skeleton (L; 2–3 agent sessions)

**Scope**

- New crate `crates/nexus-orchestration/`.
- `OrchestrationEngine` trait + `GraphFlowEngine` impl over `graph_flow = "=0.2.3"`.
- `SqliteSessionStorage` + migration added to `nexus-local-db`.
- `Capability` trait + registry; register the initial built-ins.
- Establish the daemon's Host-owned prompt execution seam and durable run-state boundary.
- daemon runtime wires engine at startup (outside any HSM state changes — that's Phase 4).
- New HTTP endpoints (authoritative list added to `acp-client-tech-spec.md` §4.3; current names under the Daemon API namespace):
  - `GET  /v1/daemon/orchestration/sessions`
  - `GET  /v1/daemon/orchestration/sessions/{session_id}`
  - `POST /v1/daemon/orchestration/sessions/{session_id}/signal`  (`pause` | `resume` | `cancel` | `advance`)
  - `GET  /v1/daemon/orchestration/capabilities`
- Register and run `_system.maintenance` hardcoded graph (not yet via file loader).

**Acceptance**

- [ ] Engine can create, step, pause, resume, cancel a hardcoded 3-state test graph
- [ ] SQLite storage roundtrip test: start session → kill daemon → restart → resume (manual signal) → completes
- [ ] Host lifecycle test: launch a fixture ACP session, execute, cancel, and confirm bounded process-group reap
- [ ] `GET /v1/daemon/orchestration/sessions` returns at least `_system.maintenance`'s session when daemon is in plain `Running` state (simulated — HSM lands Phase 4)
- [ ] `cargo test --workspace` green; clippy/fmt clean

### 10.4 Phase 3 — Preset loader + `novel-writing` end-to-end (M; 1–2 agent sessions)

**Scope**

- `load_preset()` + validation per §7.6.
- Register `acp.*`, `judge.llm`, `judge.rule`, and `creator.*` capabilities.
- Implement `AcpPromptTask` through the injected `PromptExecutor` and Host provider plane.
- Ship embedded `novel-writing` preset with 4 states + 2 inner graphs (minimum demonstrator).
- CLI stub: `nexus42 schedule start <preset-id> --creator <id>` (B-track will deepen this).
- CLI: `nexus42 schedule advance <session-id>` (manual transitions).
- CLI: `nexus42 schedule status <session-id>` pretty printer.

**Acceptance**

- [ ] `nexus42 schedule start novel-writing --creator <id>` returns a session id; `nexus42 schedule status` shows state `gathering`
- [ ] `creator.inject_prompt` output reaches the run-scoped Host prompt; a fixture provider returns non-echo output and `judge.llm` advances state
- [ ] Inner graph runs all 3 nodes; `output_binding` writes into outer context; state advances to `outlining`
- [ ] `schedule advance` advances past `outlining` → `drafting` → `done`
- [ ] Daemon restart mid-`brainstorming` follows the durable recovery class without blind replay
- [ ] End-to-end integration test in `crates/nexus-orchestration/tests/e2e_novel_writing.rs`

### 10.5 Phase 4 — statig daemon lifecycle (S+; 1 agent session; parallel with Phase 2)

Owned by the daemon lifecycle state machine (6-state; see [daemon-runtime.md](daemon-runtime.md) §10); A-track just consumes it. Entry/exit actions, event catalogue, and the HTTP surface migration live with the lifecycle owner (status field exposes real 6-state values).

**Integration point with engine**: HSM `Running.entry` calls `engine.start()`; `Stopping.entry` cancels active runs through the shared token/Host finalization path before `engine.shutdown(grace_ms)`. `Degraded` reflects sustained failures of retained subsystems such as sync, ACP registry, and the Agent Host.

### 10.6 Phase 5 — Knowledge doc revisions (S; part of each preceding phase)

In the same change window as each phase:

- Phase 1 → commit [acp-client-tech-spec.md](acp-client-tech-spec.md) §11 (crate layout).
- Phase 2 → commit §4.3 (Daemon API additions) in the same spec.
- Phase 4 → commit the lifecycle doc updates.
- Phase 3 → this document updated: move sections to "Delivered" once implemented.
- **Phase 5b (new)** → WS7 lands [creator-schedule-and-core-context.md](creator-schedule-and-core-context.md) implementation; engine consumes the `ScheduleSupervisor` signal path added in that spec's §4.
- TD-9 tracking row updated from "Partial" to "Resolved (v2)".

---

## 11. Open Questions — Reconciliation Status (was "deferred to B-track")

The following questions were originally parked as B-track in this document. After the 2026-04-17 scope decision that folds B-track into V1.4 as WS7, status is:

| ID    | Question                                                                                               | V1.4 Resolution                                                                                       |
| ----- | ------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------- |
| OQ-1  | How many concurrent Schedules can one creator have active at once?                                      | **Answered** — multi-Schedule; prompt operations serialize per run and each admitted run owns its Host session. See [creator-schedule-and-core-context.md](creator-schedule-and-core-context.md) §5. |
| OQ-2  | Schedule priority and preemption semantics                                                              | **Answered in WS7** — no priority / no preemption in V1.4; explicit `schedule pause/resume/cancel` only. See §2 decisions in the schedule spec. |
| OQ-3  | What happens when all creator Schedules complete                                                        | **Answered in WS7** — creator returns to idle (no default loop). See §2 decisions.                    |
| OQ-4  | `seed + user_edits + iterated_experience → core_context` derivation + versioning                         | **Partially answered in WS7**; V1.4 implements seed / user_edit / preset_hook derivation kinds and reserves `LlmSummarize` for V1.5. See [creator-schedule-and-core-context.md](creator-schedule-and-core-context.md) §6 + §11. |
| OQ-5  | `nexus42 schedule add/update/remove/inspect` semantics — editing in-flight                              | **Answered in WS7** — full CRUD; in-flight edits accepted but take effect at next state transition ("core_context is stable during execution"). See §3.3 + §6.4. |
| OQ-6  | Timer / clock model for wall-clock triggers                                                             | **Partially answered** — V1.4 on-demand only; `scheduled_at` column reserved; V1.5 adds clock poller zero-migration. See WS7 §2 + §10. |
| OQ-7  | Can one run use multiple configured agent roles?                                                      | **Answered in V1.186** — role bindings are frozen in the run descriptor and each `(run, role)` owns a Host session. |
| OQ-8  | User-authored capabilities (shell / WASM plugin ABI)                                                    | **Still deferred** to V1.5+ (see WS7 §13). **Durable roadmap:** DR-10. |

---

## 12. Coordinated Work Tracks and Knowledge Doc Revisions

This document defines the **orchestration engine design itself** — workstream ordering, effort estimation, and program-level coordination with the `schemas/` boundary refactor live in ****. Refer to that compass for:

- How WS1–WS4 of this spec map to V1.4 waves and milestones.
- How the `schemas/` boundary refactor (formerly noted here as a "parallel small plan") is formalised as **WS5** of the V1.4 delivery compass.
- Cross-repo dependency rules (`nexus-platform`, ACP registry), minimum regression gate, and risk register.

If you landed on this section looking for the `schemas/` refactor scope, open the compass's **§4 WS5** directly.

### 12.1 Superseded knowledge documents (v1 → v2)

| v1 (preserved, now carries superseded-by pointer) | v2 (new; authoritative)                                         |
| ------------------------------------------------- | --------------------------------------------------------------- |
| `acp-client-tech-spec-legacy.md` (archived) | [acp-client-tech-spec.md](acp-client-tech-spec.md)  |

**Retired 2026-04-17** (historical): the v1 lifecycle/ACP companion specs were retired. This orchestration-engine spec remains **active** (structure paths in §3–§8 may lag implementation; semantics remain authoritative).

---

## 13. Risks and Mitigations

| Risk                                                                          | Likelihood | Impact | Mitigation                                                                                                           |
| ----------------------------------------------------------------------------- | ---------- | ------ | -------------------------------------------------------------------------------------------------------------------- |
| `graph-flow` breaking change on 0.3.x / 0.4.x before we reach V1.5             | Medium     | Medium | Adapter trait (§4.2); pin `=0.2.3`; isolated in `nexus-orchestration`; swap impl if needed                           |
| `statig` breaking change                                                      | Low        | Low    | statig is 0.3.x; HSM description is small (~200 LOC); trivial to re-implement by hand if library diverges            |
| Host provider lifecycle leaks into orchestration internals                     | Low        | High   | **Structural**: orchestration depends only on `PromptExecutor`; ACP transport/process handles remain behind `HostFacade` |
| Host prompt timeout or cancellation leaves an owned process tree               | Medium     | High   | Persist owned identity; bounded Host session shutdown; birth-validated process-group terminate→kill→reap                |
| SQLite session table growth (many paused sessions)                            | Medium     | Low    | Capability `session.compact`; configurable retention for `completed`/`failed` sessions (default: keep 30 days)        |
| Preset bundle path traversal                                                  | Low        | High   | Loader validates every `template_file` with `path_clean` + `canonicalize` + "within bundle root" check              |
| User-authored preset with malicious `prompt` injecting tool requests          | Medium     | Medium | Default `tool_policy: auto_grant_read_only` for user presets; `auto_grant_all` only allowed for embedded system preset |
| Host failure during inner-graph mid-run leaves ambiguous external work          | Medium     | High   | Persist in-flight attempt and mark interrupted; never blind-replay ambiguous effects                                    |
| ACP launch recipe unavailable                                                   | Medium     | Medium | Provider probe checks recipe availability without spawning; execution fails closed with a typed provider error          |
| `_system.maintenance` infinite-loop bug stalls sync                           | Low        | High   | Embedded preset has mandatory unit-test gate in CI; `statig` observability hooks log transitions                     |

---

## 14. References

Internal:

- [acp-client-tech-spec.md](acp-client-tech-spec.md) — companion ACP transport, provider, and Host ownership specification
- `local-db-refactor.md` — `nexus-local-db` ownership rules for the new `orchestration_sessions` table. See `local-db-refactor.md §4` for pool sharing model.
- `acp-client-tech-spec-legacy.md` — archived; do not rely on directly (see Superseded header)

External (stable, public):

- graph-flow (rs-graph-llm): https://github.com/a-agmon/rs-graph-llm — v0.2.3
- statig: https://github.com/mdeloof/statig — v0.3.x (hierarchical state machines)
- ACP Protocol: https://agentclientprotocol.com/
- ACP Registry (public CDN): https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json
- `agent-client-protocol` crate: https://crates.io/crates/agent-client-protocol — `=0.10.4` per `acp-client-tech-spec.md` §1.2

---

*End of specification. The companion documents ([acp-client-tech-spec.md](acp-client-tech-spec.md), [creator-schedule-and-core-context.md](creator-schedule-and-core-context.md), [daemon-runtime.md](daemon-runtime.md)) fill in details that would otherwise clutter this document; read them together when extending orchestration.*

---

## V1.45 supersession (P-last promotion)

**Superseded by**: [creator-run-preset-entry.md](creator-run-preset-entry.md) (Shipped Master V1.45). The `run_intents` dispatch via generic `creator run <preset_id>` and `--force-gates --reason` semantics are now part of the canonical Master body.



## 15. V1.186 execution completeness (product lock)

**Status:** Prepare product lock; not shipped. Durable overlay for user-selected direction **A** (real workflow execution completeness). Durable terminal/wait truth and real Host/ACP execution are **in this lock**, not deferred kernel work. Implementation lives in iteration v1.186 plans P0–P3; this section is the Master product overlay (no iteration-path links).

1. **Authoritative status.** Reuse `orchestration_sessions.status TEXT` for `running` | `paused` | `waiting_for_input` | `completed` | `failed` | `cancelled` | `interrupted`. Add versioned execution metadata, a revision fence and a frozen run descriptor with a new append-only migration. Transition checkpoint/context/status/metadata atomically; graph position saves MUST NOT overwrite terminal/wait status or a newer revision. Legacy rows without authoritative metadata remain explicitly legacy/unverified unless existing evidence supports conservative reconciliation; do not fabricate historical completion. Public inspect after daemon restart MUST agree, including terminals without a live runner.
2. **One Host plane.** Choose in-process graph/capability execution through an injected Host-independent `PromptExecutor`, implemented in the daemon over existing `HostFacade`. Migrate graph `acp_prompt` and all prompt-backed consumers: `acp.prompt`, `judge.llm`, `context.summarize`, `nexus.llm.extract`; retire every secondary prompt-success branch after callers migrate. Success requires normalized agent output and `HostEvent::OpFinished` with `FinishReason::EndTurn`; refusal, limits, stream EOF and partial output are not successful completion. Host session/process ownership and generic ACP configuration are defined in [agent-host.md](agent-host.md) §4. No second LLM runtime or native RPC adapter.
3. **Public driver.** One daemon coordinator owns single-flight bounded driving for session POST, creator run and schedule admission; the graph-flow engine and existing step loop remain the execution backend. Schedule admission atomically associates a durable session before enqueue; seed/input/preset/provider bindings are frozen before eligibility. Explicit `execution_policy` distinguishes `legacy_inert`, `driven_v1`, and `system_inert`; dates or a Running label are not opt-in. Historical never-started rows MUST NOT execute on boot/tick/cron; only an explicit authorized public start opts that row in. Schedule terminal settlement and existing auto-chain insertion are idempotent by the owned terminal run.
4. **Human wait.** A fresh UUID `wait_id` identifies each manual-wait arrival and persists through restart with the root/child task cursor. Authorized matching continuation consumes it by revision CAS; missing token is `422 invalid_input`, stale/consumed token `409 workflow_wait_conflict`, incompatible state `409 workflow_state_conflict` in the existing error envelope. Other advance/resume/force-transition routes MUST NOT bypass the same human gate. Supported child waits inherit trusted creator/root-preset/parent/graph identity and durable checkpoints; parent-only status without a reconstructible child cursor is insufficient. Never auto-resume WaitingForInput children; multiple waits are exposed one at a time without approving siblings.
5. **Cancel.** Persist a cancel fence before new steps, deliver cancellation out-of-band to the actual Host operation, then bounded session cleanup/reap. Persist cancelled only when owned work has stopped; unconfirmed cleanup is interrupted with an actionable reason, never false successful cancellation. Terminal-v-cancel and continue-v-cancel races are revision-linearized. Cancel MUST NOT claim rollback of effects already committed outside Nexus.
6. **Bounded recovery.** Persist dispatch intent before external work and clear it only with its successful result checkpoint. In-flight/uncertain work is interrupted and MUST NOT be blindly retried, even if old join keys exist. Terminals are never re-driven; human waits wait; shipped converge/merge joins retain bounded resume. Newly admitted presets freeze embedded/directory source identity and a content hash including referenced templates; recovery reattaches existing root/child IDs/cursors from matching source bytes. Missing/changed source refuses rather than loading a current shadow or resetting the run. Checkpoints remain position snapshots, not an effects ledger; arbitrary historical child repair and exactly-once external effects remain non-goals.
7. **Actor / Character and CLI.** Standalone Character Host execution MUST NOT regress. Actor/Viewpoint preset IR is out of scope. `nexus42 acp run` remains a CLI-owned one-shot, not another workflow driver or acceptable substitute for public-workflow QA.
