# Orchestration Engine — Daemon-Host and Delivery History Historical Notes

**Provenance**: frozen from [`specs/active/orchestration/orchestration-engine.md`](../../active/orchestration/orchestration-engine.md) on 2026-10-13 during the **v1.216** de-historicization pass. 
**Evidenced supersession date**: 2026-09-21 — v1.193 P2 deleted the daemon host (and with it the daemon-era hosting, schedule CLI, and delivery narration frozen here); the same pass removed the V1.4/V1.45/V1.186-era delivery framing. Nothing here is current authority.
**Freeze date**: 2026-10-13

---

## Removed-line ledger (verbatim, in original order)

Every line removed from the live document by the v1.216 pass is reproduced below, verbatim and in original order (300 lines). Each block is anchored to the line number the text occupied in the pre-freeze document. Still-true statements were reworded in place in the live spec; their superseded wording appears here.

### at pre-freeze line ~3

```markdown
**Status**: Shipped (V1.4–V1.188 — orchestration engine SSOT, preset loader, Host-mediated prompts, capability registry, recoverable workspace execution and settle-once cancellation). **V1.62**: §5.2 `narrative.compute` + §8.4 `combat-engine`. **V1.179 P2**: DR-06 bounded joins. **V1.186–V1.188**: §15 durable execution completeness and reliability.
```

### at pre-freeze line ~5

```markdown
**Pillar (V1.122)**: **Harness** — this spec is the control-strategy engine contract for the [Harness](../../../../STRATEGY.md) pillar (orchestration engine + agent host + capability registry + presets). Harness is the "how an author harnesses AI agents to execute creative work" pillar; the user-visible Strategy/Strategies → **Harness** product rename shipped V1.156 P3; internal identifiers remain `strategy`/`preset` (architect LOCKED).
**Author**: @project-manager (brainstorm consolidation) / to be co-authored by @architect before first implement
**Date**: 2026-04-17; **Last updated**: 2026-09-29 — scope clarified after v1.193 P2 daemon retirement; V1.188 workspace and cancellation/replay contracts retained
**Scope**: Retained orchestration/preset contracts in `crates/nexus-orchestration` and `crates/nexus-preset`, their `crates/nexus-acp-host` boundary, and preset bundle format under the ordinary direct-core `cli` cohort.
**Historical host scope (v1.193 P2 supersession)**: The original “daemon runtime … `nexus42` CLI” scope and daemon-owned execution/lifecycle/schedule wording below describe the retired integrated host. They do not restore the daemon group or a CLI preset runner. The libraries remain; current CLI entry authority is [cli-spec.md](../../active/cli/cli-spec.md) §6.0B + its delivered v1.193 P2 overlay (`apps/nexus42/src/cli.rs`; retained crate/cohort edges in `apps/nexus42/Cargo.toml`).
```

### at pre-freeze line ~13

```markdown
- TD-9 closed: full 6-state HSM lifecycle (status moves from "gap" to "closed")
```

### at pre-freeze line ~17

```markdown
- Creator **Schedule** (multi-Schedule queueing, priority, preemption, CRUD by ID, `core_context` derivation and versioning) — **now folded into V1.4 as WS7**, designed separately in [creator-schedule-and-core-context.md](../../active/orchestration/creator-schedule-and-core-context.md). This document defines the engine primitives that WS7 builds on.
- LLM-driven `core_context` summarisation / auto-iteration — V1.4 reserves the data-model variant but does not implement the capability (see [creator-schedule-and-core-context.md](../../active/orchestration/creator-schedule-and-core-context.md) §11); V1.5+.
- Schedule cron / wall-clock triggers — V1.5+ (schema ready in V1.4).
- Preset third-party registry / signing / publish — V1.5+.
- Full `schemas/` vs local-type boundary refactor — **WS5** of V1.4, designed separately in `schemas-boundary.md`; parallel to WS2 of that compass.

> This document is the orchestration engine design from the 2026-04-17 brainstorming session, updated in place for the shipped Host execution plane. Schedule + core_context work is tracked in [creator-schedule-and-core-context.md](../../active/orchestration/creator-schedule-and-core-context.md); §11 retains the reconciliation table.
```

### at pre-freeze line ~35

```markdown
10. [Migration Phases and Task Breakdown](#10-migration-phases-and-task-breakdown)
11. [Open Questions (deferred to B-track)](#11-open-questions--reconciliation-status-was-deferred-to-b-track)
12. [Coordinated Work Tracks and Knowledge Doc Revisions](#12-coordinated-work-tracks-and-knowledge-doc-revisions)
```

### at pre-freeze line ~38

```markdown
15. [V1.186 execution completeness (product lock)](#15-v1186-execution-completeness-product-lock)
```

### at pre-freeze line ~46

```markdown
Nexus currently drives ACP agents through **one-shot, human-initiated CLI commands** (`nexus42 agent run <ref>`). The daemon (daemon runtime) is a passive HTTP backend that holds workspace state, sync plumbing, and the ACP tool-mediation endpoint — it does **not** drive creators, does **not** run scheduled work, and has **no notion of a strategy** that spans multiple ACP sessions and multiple days.

Users need to express creator workflows as configurable, prompt-driven strategies — e.g. *"collect inspiration → brainstorm → outline → draft"* — that the daemon executes **autonomously** across creator activations, with stable promptable identity and resumable execution across daemon restarts.
```

### at pre-freeze line ~50

```markdown
- **Daemon becomes `orchestration engine + capability registry`**: existing HTTP-era capabilities (sync, workspace ops, outbox flush, registry refresh) are **reclassified as first-class capabilities** invokable as graph nodes; HTTP API retreats to a *trigger/query surface* over the same engine.
- **Strategy shape is hierarchical**: an outer **state machine** (long-lived, cross-session) containing inner **DAG graphs** (short, in-memory prompt/tool call chains) — *graph-of-graphs*.
- **ACP remains external behind the Host plane**: CLI keeps interactive `acp run`; orchestration prompts use the daemon's existing `HostFacade` through an injected `PromptExecutor`. `nexus-acp-host` owns transport and process lifecycle; orchestration never owns provider handles.
- **Runtime is `graph-flow` (outer + inner, exact `=0.8.0`, default-features off) + custom SQLite `SessionStorage`**: adapted behind a thin trait layer so the upstream crate stays swappable. 0.8 made `Context` synchronous/fallible, graph topology immutable (`GraphBuilder`), and stale-save conflicts honest (`SessionConflict`); the durable checkpoint carries an explicit `graph_version`.
- **Daemon lifecycle is `statig` HSM**: 6-state process lifecycle (`Stopped`/`Starting`/`Running`/`Degraded`/`Stopping`/`Failed`) — closes TD-9.
```

### at pre-freeze line ~55

```markdown
### 1.3 Deliverables (end-state of §10 Phase 1–4)

1. `crates/nexus-acp-host` owns ACP transport and managed subprocess lifecycle behind the Host provider plane.
2. `crates/nexus-orchestration` owns graph-flow adaptation, capability registration, preset loading, and SQLite `SessionStorage`.
3. daemon runtime owns orchestration, lifecycle HSM, and the existing `HostFacade` integration.
4. `nexus42` retains interactive `acp run` and schedule commands; the obsolete secondary ACP subcommand is removed.
5. First built-in preset: `_system.maintenance` (mandatory) and one user-facing sample `novel-writing`.
6. Knowledge docs revised: [acp-client-tech-spec.md](../../active/agents/acp-client-tech-spec.md).

### 1.4 Effort (agent-oriented)

Per [effort-estimation.md](https://github.com/btspoony/mstar-harness/blob/main/docs/agents/effort-estimation.md) conventions (**agent sessions only; no human time**):

| Phase                                                           | Effort     | Approx. agent sessions |
| --------------------------------------------------------------- | ---------- | ---------------------- |
| Phase 1 — `nexus-acp-host` crate extraction                     | M          | 1–2                    |
| Phase 2 — orchestration skeleton (graph-flow + capability + Host seam) | L          | 2–3                    |
| Phase 3 — preset loader + `novel-writing` end-to-end             | M          | 1–2                    |
| Phase 4 — statig lifecycle HSM (parallelisable with Phase 2)    | S+         | 1                      |
| Totals (excl. Phase 4 parallel savings; see compass WS5 for schemas refactor effort) | **L → XL** | **6–9** |

The runtime/preset contracts remain authoritative for the retained crates above. References to the daemon runtime, its lifecycle HSM, and daemon-era `nexus42` schedule/execution entries are historical composition context, not the current CLI execution surface.
```

### at pre-freeze line ~70

```markdown
- Historical host integration: how the orchestration engine consumed and was consumed by the `statig` daemon lifecycle (retired host, not a current CLI requirement).
- Migration phases and their ordering constraints.
```

### at pre-freeze line ~76

```markdown
| Multi-preset per-creator scheduling, priority, preemption                      | B-track (`schedule-and-plan-vN.md` — not yet authored)           |
| `nexus42 schedule` CLI command family semantics                                | B-track                                                           |
| Seed-prompt → stable core-context derivation & versioning                      | B-track                                                           |
| Preset distribution / registry / signing                                       | Future (V1.5+)                                                   |
| Wire schemas vs local types boundary refactor                                  |  §4 WS5 |
```

### at pre-freeze line ~85

```markdown
- **Not an ACP protocol server promotion**: daemon runtime coordinates the existing Host provider plane; it does not expose ACP as a new public server protocol. See [acp-client-tech-spec.md](../../active/agents/acp-client-tech-spec.md) §2.3.
- **Not a LangChain-style in-memory pipeline**: all engine execution is **durable** and **resumable across daemon restart**; in-memory-only pipelines are explicitly rejected.
- **Not a replacement for interactive `nexus42 agent run`**: that path stays direct stdio CLI-to-agent; orchestration does not route through it.
```

### at pre-freeze line ~124

```markdown
| `crates/nexus-daemon-runtime`                     | Ext. | No             | **Deleted in v1.193 P2** — was: orchestration engine host, lifecycle HSM, `HostFacade` prompt executor, HTTP trigger/query. Current owners: engine/supervisor in `crates/nexus-orchestration`, host composition in `crates/nexus-core`, HTTP family in `apps/nexus-service` |
| `apps/nexus42`                                    | Ext. | Yes (via host) | Interactive `acp run`; the schedule command groups were retired in v1.193 P2 (retained scheduling CLI surface: `creator works cron` declaration editing) |
```

### at pre-freeze line ~130

```markdown
1. Engine resolves *which creator Schedule is due* (B-track concern; A-track treats the decision as a **trigger input**).
```

### at pre-freeze line ~157

```markdown
**Why this library** (consolidated rationale from 2026-04-17 brainstorming):
```

### at pre-freeze line ~178

```markdown
First and only impl in Phase 2: `GraphFlowEngine` wraps `graph_flow::FlowRunner` + our `SqliteSessionStorage`. All daemon code depends on the **trait**, not on `graph_flow::*` directly. If the upstream crate ships breaking changes we cannot absorb, we swap the impl — callers are insulated.
```

### at pre-freeze line ~186

```markdown
    pool: sqlx::SqlitePool,           // shares nexus-local-db's SqlitePool (post-WS8)
```

### at pre-freeze line ~197

```markdown
**Pool ownership (post-WS8)**: `nexus-local-db` exposes `Arc<sqlx::SqlitePool>` as the single workspace pool for `state.db` after V1.4 **WS8** unifies the DB engine on `sqlx` (; decision SSOT: [`crate-selection-best-practices.md`](../../../knowledge/crate-selection-best-practices.md) §2.3 + §3.3). `SqliteSessionStorage` takes that `Arc<SqlitePool>` at construction time; no separate connection or separate `.db` file. The `orchestration_sessions` table lands as one more `.sql` migration file under `crates/nexus-local-db/migrations/`, authored in WS2 Task 3 **after** WS8 T1–T2.
```

### at pre-freeze line ~223

```markdown
**V1.186 product lock:** the `status` column is the operator-authoritative run state for terminal and wait classes, not a write-only `'running'` bookkeeping field. Preserve shipped migration bytes; any column/codegen change MUST land together. See §15.
```

### at pre-freeze line ~241

```markdown
#### 4.4.1 `llm_judge` runtime contract (V1.33 — Implemented)
```

### at pre-freeze line ~243

```markdown
**Pre-V1.33 problem (resolved in V1.33 P3):** `StateCompositeTask` used to route `exit_when.kind: llm_judge` through `JudgeTask` that only evaluated stub `_judge_rule` (`always_true` / `always_false`) without calling the declared `judge_capability` or loading `template_file`.

**Required behavior (V1.33+):**
```

### at pre-freeze line ~250

```markdown
6. **Identity**: use schedule-injected `_creator_id` / `_session_id` only — not raw preset args (aligns with V1.32 `SEC-V131-01`).
```

### at pre-freeze line ~253

```markdown
> **Durable roadmap:** DR-11 (conditional-next routing).

- **Cancel**: `engine.signal(session, Cancel)` — cascades to any child inner-graph sessions; each `Task` impl must be *cancellation-safe* (stop after the current await point; no half-committed writes). **V1.186:** cancel MUST reach active Host/ACP work; it MUST NOT advertise rollback of already committed external effects.
- **Kill** (daemon stop): lifecycle HSM `Stopping` state sends `Cancel` to every active session before shutting the engine down.
```

### at pre-freeze line ~285

```markdown
Runtime dependencies are injected through `CapabilityRuntimeDeps`; graph context carries serializable values only. Provider, process, and transport handles remain behind the daemon's `HostFacade`.
```

### at pre-freeze line ~289

```markdown
All capabilities below are registered at daemon runtime startup. Adding a new capability is a Rust code change (not user-config) for V1.4. User-authored capabilities are **out of scope** (residual for V1.5+).
```

### at pre-freeze line ~293

```markdown
| `sync.pull`                 | Pull remote deltas (replaces HTTP-era trigger)                 | `nexus-cloud-sync`     | Deferred wiring — tracked DF-46 / PD-05 (§2.3) |
| `sync.push`                 | Push local outbox (replaces HTTP-era trigger)                  | `nexus-cloud-sync`     | Deferred wiring — tracked DF-46 / PD-05 (§2.3) |
| `outbox.flush`              | Flush pending outbox entries                                   | `nexus-orchestration`  | **Shipped (V1.59)** — local drain via `nexus-local-db` pool; see §5.7 |
| `outbox.compact`            | Compact outbox table                                           | `nexus-orchestration`  | **Shipped (V1.59)** — retention-window compaction via `nexus-local-db` pool; see §5.7 |
```

### at pre-freeze line ~300

```markdown
| `creator.read_memory`       | Query persisted creator memory fragments                       | `nexus-creator-memory` | **Real** — SQLite-backed query through `CreatorCapabilityStore` (V1.31 DF-30) |
| `creator.write_memory`      | Persist creator memory fragments and return real `fragment_id` | `nexus-creator-memory` | **Real** — SQLite-backed write through `CreatorCapabilityStore` (V1.31 DF-30) |
| `creator.inject_prompt`     | Queue a prompt to be sent on next `acp.prompt`                 | `nexus-orchestration`  | **Real** — persisted injection queue in `state.db` (V1.31 DF-30) |
```

### at pre-freeze line ~309

```markdown
| `narrative.compute`         | Invoke WASM compute module; apply state_delta, timeline_events, new_key_blocks, return battle_report | `nexus-orchestration`  | **Real** — calls `nexus-wasm-host::compute()` (V1.61 P3; spec-seal V1.62 P2); see §8.4.1 |
| `timer.wait_until`          | Schedule a wake-up signal (requires B-track clock)             | `nexus-orchestration`  | Deferred clock integration — **Durable roadmap:** DR-12 |
```

### at pre-freeze line ~312

```markdown
> **V1.186 Host cutover:** all prompt-backed capabilities share the injected `PromptExecutor`; standalone constructors fail closed rather than installing an echo or secondary transport fallback.
```

### at pre-freeze line ~318

```markdown
> **Historical (daemon builds; the Rust host was deleted in v1.193 P2):** `sync.*` MUST NOT have crossed into `nexus-cloud-sync` on the daemon hot path — historical edge record: [local-cloud-crate-architecture.md](../architecture/2026-09-21-local-cloud-crate-architecture.md) §7. `outbox.flush` / `outbox.compact` (V1.59) are local-only pool-backed capabilities that operate directly on `outbox_entries` via `nexus-local-db` — they do not depend on `nexus-cloud-sync`.
```

### at pre-freeze line ~324

```markdown
### 5.5 Runtime dependency injection (V1.31; Host cutover V1.186)
```

### at pre-freeze line ~329

```markdown
- **Prompt executor**: `acp.prompt`, `judge.llm`, `context.summarize`, `nexus.llm.extract`, and graph `acp_prompt` call the injected `PromptExecutor`. The daemon implementation uses its existing `HostFacade`, scopes sessions by run and role, narrows tool policy per operation, and persists typed outcomes. Missing executor, run identity, cancellation token, or workflow permission scope fails closed.
```

### at pre-freeze line ~332

```markdown
### 5.6 `creator.inject_prompt` queue semantics (V1.31)
```

### at pre-freeze line ~343

```markdown
### 5.7 Outbox consolidation (V1.59)
```

### at pre-freeze line ~345

```markdown
The dual-outbox architecture identified in TD-8 (`dual-outbox-architecture.md`) was consolidated in V1.59. The unified outbox schema uses `outbox_entries` / `partial_apply_states` (migration `20260420_outbox_tables.sql`) as the single source of truth.
```

### at pre-freeze line ~347

```markdown
**Single-writer rule**: each outbox event type has exactly one authorized writer subsystem. `nexus-cloud-sync::outbox::Outbox` owns sync push/pull commands. `nexus-orchestration` capabilities (`outbox.flush`, `outbox.compact`) own maintenance operations. The daemon legacy `outbox` queue table had no active Rust-level consumers (confirmed by the V1.59 T3 audit) and was dropped at V1.163 by migration `20260812_drop_legacy_outbox.sql`.
```

### at pre-freeze line ~362

```markdown
- The daemon's `HostPromptExecutor` lazily creates one Host session per `(run_id, role)` and never shares a subprocess across runs or Creators.
```

### at pre-freeze line ~540

```markdown
  - `preset.input.<key>` — read-only; provided by B-track Schedule at start
```

### at pre-freeze line ~545

```markdown
**Normative SSOT:** [preset-conditional-routing.md](../../active/orchestration/preset-conditional-routing.md) — **Normative** (all overlays promoted V1.158 P3). This subsection is a cross-reference; the full conditional schema, expression grammar, multi-branch routing, and merge semantics live in that document.
```

### at pre-freeze line ~547

```markdown
**Converge (merge-point) state kind** (V1.56 P2 fix-wave, H-001/W-002): states may declare a `converge` config with a `strategy` field to act as explicit join points for multiple incoming edges:
```

### at pre-freeze line ~566

```markdown
**Bounded joins** (DR-06, v1.179): join states (carrying `merge:` or
```

### at pre-freeze line ~573

```markdown
("Bounded joins"). The deferred `wait_for_all_timeout_seconds` name is
```

### at pre-freeze line ~576

```markdown
**Expression depth limit** (V1.56 P2 fix-wave, W-003): `MAX_EXPR_DEPTH = 32` bounds parsing depth to prevent stack overflow from user-installable presets.
```

### at pre-freeze line ~612

```markdown
#### 7.6.1 Shared semantic validation facade (V1.32)
```

### at pre-freeze line ~614

```markdown
V1.32 introduces a **shared semantic validation facade** used by both the CLI/API validate endpoint (`POST /v1/daemon/presets:validate`) and the orchestration loader. The facade is the single quality gate; there are no parallel weaker checks.
```

### at pre-freeze line ~632

```markdown
#### 7.6.2 Capability compatibility checks (V1.32)
```

### at pre-freeze line ~644

```markdown
### 7.7 Embedded preset index (V1.31+)
```

### at pre-freeze line ~654

```markdown
| `novel-chapter-review` | FL-E `review` stage — novel/work/chapter-aware review producer (findings writer, V1.47) | load_chapter → review → done | `creator.inject_prompt`, `acp.prompt` |
```

### at pre-freeze line ~656

```markdown
| `creative-brief-intake` | **V1.33 Shipped** ( P2 plan) — grill-me intake preset | intake → done | `acp.prompt` |
| `essay-writing` | **V1.63 P2 Shipped** — essay production preset with 4-dimension quality rubric (thesis clarity, evidence support, coherence, ending takeaway) | intake → outline → draft → revise → finalize → finalize_commit → done | `creator.inject_prompt`, `acp.prompt`, `judge.llm`, `essay.draft_status.finalize` |
```

### at pre-freeze line ~659

```markdown
All shipped presets use **linear** `next` transitions unless noted; conditional routing remains deferred (§7.5). **Durable roadmap:** DR-11.
```

### at pre-freeze line ~661

```markdown
### 7.8 Preset `run_intents` (V1.33)
```

### at pre-freeze line ~663

```markdown
Presets declare **how** they may be started from [work-experience-model.md](../../active/creator/work-experience-model.md) and `creator run`:
```

### at pre-freeze line ~676

```markdown
Loader rules (V1.33):
```

### at pre-freeze line ~680

```markdown
- `creator bootstrap` filters intake presets where `work_init ∈ run_intents`.
- `creator run continue` filters presets where `work_continue ∈ run_intents`.
```

### at pre-freeze line ~684

```markdown
### 7.9 Preset `gates` (V1.36 — Implemented)
```

### at pre-freeze line ~720

```markdown
| `previous_preset` | `preset` (string, id), `status` (closed enum: `complete` \| `any_session`), `scope: work` (only `work` is normative V1.36) | Query the orchestration session log for the named preset, scoped to the same `work_id`; check the named status. `any_session` accepts completed/paused/waiting_for_input; `complete` requires terminal-completion. |
```

### at pre-freeze line ~727

```markdown
   - Return a structured error to the caller (`creator bootstrap` / `creator run <preset_id>` / schedule API):
```

### at pre-freeze line ~736

```markdown
           "remediation": "Run `creator bootstrap --init-preset novel-project-init` first." },
```

### at pre-freeze line ~738

```markdown
           "remediation": "Complete intake via `creator bootstrap --preset creative-brief-intake`." }
```

### at pre-freeze line ~743

```markdown
4. **`--force` override** (audit-logged): `creator bootstrap --force-gates` or `creator run <preset_id> --force-gates` skips gate evaluation. The override records `forced: true`, the user identity, and a free-text reason (when provided) to the audit log. Forced runs still respect the engine's other invariants (capability registry, run_intents, etc.).
```

### at pre-freeze line ~750

```markdown
| `run_intents` (V1.33) | Coarse: which `creator run` subcommand surfaces the preset | CLI dispatch time | §7.8, [work-experience-model.md](../../active/creator/work-experience-model.md) §5 |
| `requires_capabilities` (V1.4) | Capability availability at engine startup | Loader | §7.2 |
| `gates` (V1.36) | Per-invocation preconditions (Work fields, filesystem, prior-preset) | Enqueue time | §7.9 |
| `stage` gates (V1.34) | FL-E linear stage ordering (`intake → research → produce → review → persist`) | `creator run <preset_id>` (preset runner validates before enqueue) | [creator-workflow.md](../../active/creator/creator-workflow.md) §3.3 |
```

### at pre-freeze line ~767

```markdown
The loader consumes YAML bundles and produces `LoadedPreset` structs ready for graph-flow execution. As of V1.32, the loader runs the shared semantic validation facade (§7.6.1–§7.6.2) as a mandatory pre-step before graph construction. The CLI/API `validate` endpoint calls the same facade independently, ensuring loader and diagnostic parity.
```

### at pre-freeze line ~809

```markdown
| `states[].next.kind=conditional` (future)             | `add_conditional_edge`                                                            |
```

### at pre-freeze line ~814

```markdown
> **Durable roadmap:** `next.kind=conditional` loader acceptance (the `(future)` row above) is DR-11.

- On `registry.refresh` capability call or the shipped Daemon API `POST /v1/daemon/presets/{id}:reload`, loader recomputes hash; if changed, invalidates cache and rebuilds. There is currently no top-level `nexus42 preset reload` CLI.
```

### at pre-freeze line ~821

```markdown
### 8.4 `narrative.compute` capability and `combat-engine` preset (V1.62 P2 — Normative)

**Status**: Normative — V1.62 Shipped (deferred from V1.61 P3).
```

### at pre-freeze line ~823

```markdown
V1.61 introduced the `nexus-wasm-host` crate and the `narrative.compute`
orchestration capability; the capability registration and preset were deferred
to V1.61 P3 and are now documented here as shipped.
```

### at pre-freeze line ~832

```markdown
at daemon boot.
```

### at pre-freeze line ~869

```markdown
appended to the timeline. The daemon does not crash on compute failure.
```

### at pre-freeze line ~879

```markdown
**Pattern / role:** User-triggered combat resolution (V1.61 Q7).
```

### at pre-freeze line ~907

```markdown
included in the preset sync test suite. The preset is **not** embedded in the
binary in V1.62 — it is a filesystem-loaded preset under
`crates/nexus-orchestration/embedded-presets/combat-engine/`.
```

### at pre-freeze line ~923

```markdown
- Enters engine when statig lifecycle transitions `Running → entry`.
```

### at pre-freeze line ~925

```markdown
- Exits on statig lifecycle transition `Running → Stopping` (cancelled, not terminated).
```

### at pre-freeze line ~929

```markdown
A Creator Schedule is a persistent, user-addressable wrapper around zero or one active engine `Session`. It adds user-facing CRUD (`schedule add/edit/list/inspect/remove`), multi-Schedule per creator, dependency chains, and immutable `core_context` versioning that the engine reads at each state transition.
```

### at pre-freeze line ~933

```markdown
- **Engine primitives consumed** — `new_session`, `run_step`, `signal`, `list_active` are sufficient for the supervisor module defined in WS7; this spec adds no new engine API.
```

### at pre-freeze line ~935

```markdown
- **Supervisor module** — `crates/nexus-orchestration/src/schedule/` (new in WS7) owns the Pending → Running admission logic, dependency resolution, and signal propagation. It is driven by engine session-terminal events.
```

### at pre-freeze line ~944

```markdown
| Origin                          | Embedded preset in binary                | User or CLI invocation                                   |
| Lifecycle owner                 | statig HSM (Running.entry starts it)     | `ScheduleSupervisor` (Pending → Running admission)       |
```

### at pre-freeze line ~948

```markdown
| User CRUD                       | Not user-editable                        | Full CRUD per `nexus42 schedule *`                       |
```

### at pre-freeze line ~953

```markdown
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
- `OrchestrationEngine` trait + `GraphFlowEngine` impl over `graph_flow = "=0.8.0"`.
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

**Historical (daemon-era; the daemon lifecycle was deleted in v1.193 P2):** owned by the daemon lifecycle state machine (6-state; historical record: [daemon-runtime.md](../runtime/2026-09-21-daemon-runtime.md) §10); A-track just consumes it. Entry/exit actions, event catalogue, and the HTTP surface migration live with the lifecycle owner (status field exposes real 6-state values).

**Integration point with engine**: HSM `Running.entry` calls `engine.start()`; `Stopping.entry` cancels active runs through the shared token/Host finalization path before `engine.shutdown(grace_ms)`. `Degraded` reflects sustained failures of retained subsystems such as sync, ACP registry, and the Agent Host.

### 10.6 Phase 5 — Knowledge doc revisions (S; part of each preceding phase)

In the same change window as each phase:

- Phase 1 → commit [acp-client-tech-spec.md](../../active/agents/acp-client-tech-spec.md) §11 (crate layout).
- Phase 2 → commit §4.3 (Daemon API additions) in the same spec.
- Phase 4 → commit the lifecycle doc updates.
- Phase 3 → this document updated: move sections to "Delivered" once implemented.
- **Phase 5b (new)** → WS7 lands [creator-schedule-and-core-context.md](../../active/orchestration/creator-schedule-and-core-context.md) implementation; engine consumes the `ScheduleSupervisor` signal path added in that spec's §4.
- TD-9 tracking row updated from "Partial" to "Resolved (v2)".

```

### at pre-freeze line ~953

```markdown

## 11. Open Questions — Reconciliation Status (was "deferred to B-track")

The following questions were originally parked as B-track in this document. After the 2026-04-17 scope decision that folds B-track into V1.4 as WS7, status is:
```

### at pre-freeze line ~955

```markdown
| ID    | Question                                                                                               | V1.4 Resolution                                                                                       |
| ----- | ------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------- |
| OQ-1  | How many concurrent Schedules can one creator have active at once?                                      | **Answered** — multi-Schedule; prompt operations serialize per run and each admitted run owns its Host session. See [creator-schedule-and-core-context.md](../../active/orchestration/creator-schedule-and-core-context.md) §5. |
| OQ-2  | Schedule priority and preemption semantics                                                              | **Answered in WS7** — no priority / no preemption in V1.4; explicit `schedule pause/resume/cancel` only. See §2 decisions in the schedule spec. |
| OQ-3  | What happens when all creator Schedules complete                                                        | **Answered in WS7** — creator returns to idle (no default loop). See §2 decisions.                    |
| OQ-4  | `seed + user_edits + iterated_experience → core_context` derivation + versioning                         | **Partially answered in WS7**; V1.4 implements seed / user_edit / preset_hook derivation kinds and reserves `LlmSummarize` for V1.5. See [creator-schedule-and-core-context.md](../../active/orchestration/creator-schedule-and-core-context.md) §6 + §11. |
| OQ-5  | `nexus42 schedule add/update/remove/inspect` semantics — editing in-flight                              | **Answered in WS7** — full CRUD; in-flight edits accepted but take effect at next state transition ("core_context is stable during execution"). See §3.3 + §6.4. |
| OQ-6  | Timer / clock model for wall-clock triggers                                                             | **Partially answered** — V1.4 on-demand only; `scheduled_at` column reserved; V1.5 adds clock poller zero-migration. See WS7 §2 + §10. |
| OQ-7  | Can one run use multiple configured agent roles?                                                      | **Answered in V1.186** — role bindings are frozen in the run descriptor and each `(run, role)` owns a Host session. |
| OQ-8  | User-authored capabilities (shell / WASM plugin ABI)                                                    | **Still deferred** to V1.5+ (see WS7 §13). **Durable roadmap:** DR-10. |

```

### at pre-freeze line ~955

```markdown

## 12. Coordinated Work Tracks and Knowledge Doc Revisions

This document defines the **orchestration engine design itself** — workstream ordering, effort estimation, and program-level coordination with the `schemas/` boundary refactor live in ****. Refer to that compass for:

- How WS1–WS4 of this spec map to V1.4 waves and milestones.
- How the `schemas/` boundary refactor (formerly noted here as a "parallel small plan") is formalised as **WS5** of the V1.4 delivery compass.
- Cross-repo dependency rules (`nexus-platform`, ACP registry), minimum regression gate, and risk register.

If you landed on this section looking for the `schemas/` refactor scope, open the compass's **§4 WS5** directly.

### 12.1 Superseded knowledge documents (v1 → v2)

| v1 (preserved, now carries superseded-by pointer) | v2 (new; authoritative)                                         |
| ------------------------------------------------- | --------------------------------------------------------------- |
| `acp-client-tech-spec-legacy.md` (archived) | [acp-client-tech-spec.md](../../active/agents/acp-client-tech-spec.md)  |

**Retired 2026-04-17** (historical): the v1 lifecycle/ACP companion specs were retired. This orchestration-engine spec remains **active** (structure paths in §3–§8 may lag implementation; semantics remain authoritative).
```

### at pre-freeze line ~963

```markdown
| `graph-flow` breaking change on a future 0.x before we reach V1.5              | Medium     | Medium | Adapter trait (§4.2); pin `=0.8.0`; isolated in `nexus-orchestration`; swap impl if needed                           |
| `statig` breaking change                                                      | Low        | Low    | statig is 0.3.x; HSM description is small (~200 LOC); trivial to re-implement by hand if library diverges            |
```

### at pre-freeze line ~971

```markdown
| `_system.maintenance` infinite-loop bug stalls sync                           | Low        | High   | Embedded preset has mandatory unit-test gate in CI; `statig` observability hooks log transitions                     |
```

### at pre-freeze line ~979

```markdown
- `local-db-refactor.md` — `nexus-local-db` ownership rules for the new `orchestration_sessions` table. See `local-db-refactor.md §4` for pool sharing model.
- `acp-client-tech-spec-legacy.md` — archived; do not rely on directly (see Superseded header)
- statig: https://github.com/mdeloof/statig — v0.3.x (hierarchical state machines)
## V1.45 supersession (P-last promotion)

**Superseded by**: [creator-run-preset-entry.md](../creator/2026-09-21-creator-run-preset-entry.md) (Shipped Master V1.45). The `run_intents` dispatch via generic `creator run <preset_id>` and `--force-gates --reason` semantics are now part of the canonical Master body.



## 15. V1.186 execution completeness (product lock)
```

### at pre-freeze line ~996

```markdown
**Status:** Shipped V1.186, amended by verified V1.188 workspace and settlement behavior below. This is the durable Master execution contract; legacy records retain explicit conservative handling rather than fabricated historical success.
```

### at pre-freeze line ~998

```markdown
1. **Authoritative status.** Reuse `orchestration_sessions.status TEXT` for `running` | `paused` | `waiting_for_input` | `completed` | `failed` | `cancelled` | `interrupted`. Add versioned execution metadata, a revision fence and a frozen run descriptor with a new append-only migration. Transition checkpoint/context/status/metadata atomically; graph position saves MUST NOT overwrite terminal/wait status or a newer revision. Legacy rows without authoritative metadata remain explicitly legacy/unverified unless existing evidence supports conservative reconciliation; do not fabricate historical completion. Public inspect after daemon restart MUST agree, including terminals without a live runner.
2. **One Host plane.** Choose in-process graph/capability execution through an injected Host-independent `PromptExecutor`, implemented in the daemon over existing `HostFacade`. Migrate graph `acp_prompt` and all prompt-backed consumers: `acp.prompt`, `judge.llm`, `context.summarize`, `nexus.llm.extract`; retire every secondary prompt-success branch after callers migrate. Success requires normalized agent output and `HostEvent::OpFinished` with `FinishReason::EndTurn`; refusal, limits, stream EOF and partial output are not successful completion. Host session/process ownership and generic ACP configuration are defined in [agent-host.md](../../active/agents/agent-host.md) §4. No second LLM runtime or native RPC adapter.
3. **Public driver.** One daemon coordinator owns single-flight bounded driving for session POST, creator run and schedule admission; the graph-flow engine and existing step loop remain the execution backend. Schedule admission atomically associates a durable session before enqueue; seed/input/preset/provider bindings are frozen before eligibility. Explicit `execution_policy` distinguishes `legacy_inert`, `driven_v1`, and `system_inert`; dates or a Running label are not opt-in. Historical never-started rows MUST NOT execute on boot/tick/cron; only an explicit authorized public start opts that row in. Schedule terminal settlement and existing auto-chain insertion are idempotent by the owned terminal run.
```

### at pre-freeze line ~1006

```markdown
### 15.1 V1.188 workspace execution and settlement
```

### at pre-freeze line ~1008

```markdown
- `workspace.open` and `workspace.commit` consume shared contract DTOs through an injected `WorkspaceExecutor`; the daemon binds the same durable authority as HTTP workspace routes. Missing binding fails explicitly. Target bytes, pre-image OCC, stable revision, retained intent recovery and external visibility limits are normative in [concurrency.md](../../active/runtime/concurrency.md) §9.
```
