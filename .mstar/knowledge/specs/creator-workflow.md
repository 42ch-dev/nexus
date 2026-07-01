# Creator Workflow — Normative Specification

**Status**: Shipped (V1.34 — 2026-06-05; V1.35 P4 partial; V1.39 — DF-53 full auto-chain + daemon continuity; **V1.40 Shipped** — DF-63 W5 `kb-extract` persistence via `novel-review-master sync_world_kb`: World-bound Works enqueue extract with `work.world_id`, `source_kind=work_chapter`, `source_locator={{preset.input.body_path}}`, `work_id`; worldless Works (legacy V1.39-and-earlier) skip World promotion; V1.79 additive SOUL visualization contract over memory fragments)  
**Document class**: Feature line  
**Created**: 2026-06-04  
**Last updated**: 2026-07-01 — V1.79 SOUL visualization contract note  
**Scope**: Staged creator journey on **Work** (`intake → research → produce → review → persist`), built on shipped `creator run` + `run_intents`  
**Coordinates with**:

- [work-experience-model.md](work-experience-model.md) — Work entity, intake, run_intents
- [novel-writing/workflow-profile.md](novel-writing/workflow-profile.md) — novel `produce` artifacts and completion (Draft V1.36)
- [cli-spec.md](cli-spec.md) — `creator run <preset_id>` (see §6.2D) and `creator bootstrap`
- [orchestration-engine.md](orchestration-engine.md) — presets, schedules, capabilities
- [agent-nexus-tool-bridge.md](agent-nexus-tool-bridge.md) — Agent-initiated context/tools (parallel channel)

**Iteration compass**: [v1.34-creator-workflow-and-agent-tools-delivery-compass-v1.md](../../iterations/v1.34-creator-workflow-and-agent-tools-delivery-compass-v1.md)

---

## 1. Purpose

The Work loop shipped in V1.33 centered on Creative Brief Intake and `novel-writing`. This spec generalizes the journey to:

```text
intake → research → produce → review → persist
```

without introducing a second scheduler or replacing World/KB SSOT. After V1.35 P4, `creator bootstrap` chains intake → produce by default (`--chain-novel-writing`, default true). **V1.39** ships full-stage `--auto-chain` (default **true**, opt-out `--no-auto-chain`): while the daemon is online, stages advance through `research → produce → review → persist` per chapter without a manual preset dispatch at each boundary. Daemon restart resumes from a Work continuation checkpoint (DF-68).

---

## 2. Relationship to Work model

| Concept | Work model (baseline) | Staged workflow (this spec) |
| --- | --- | --- |
| Work container | Shipped | Extended with `stage`, `stage_status` |
| Entry | `creator bootstrap` | Composite onboarding; intake still `work_init` |
| Continue inspiration | `creator works inspire --note` | Unchanged |
| Stage progression | N/A | **`creator run <preset_id>`** (preset runner applies stage gates before enqueue) |
| Primary produce preset | `novel-writing` | Default for `produce` stage |
| Generic multi-stage workflow | Deferred | **Shipped V1.34** |

**Invariants** (unchanged from work-experience-model):

1. Work ≠ workspace ≠ World ≠ work KB index.
2. `work_continue` presets require `intake_status == complete` unless `--force` (logged).
3. At most one **active** `work_init` intake schedule per Work while intake incomplete.

**New invariant (V1.34)**:

4. At most one **active stage schedule** per Work at a time (no parallel `research` + `novel-writing` stage drivers).

---

## 3. Stage model

### 3.1 Stage identifiers (closed enum)

| `stage` | Meaning | Typical preset(s) | `run_intents` |
| --- | --- | --- | --- |
| `intake` | Creative Brief Intake | `creative-brief-intake` | `work_init` |
| `research` | Reference / KB gathering | `research` | `knowledge_ingest`, `work_continue` |
| `produce` | Primary drafting / generation | `novel-writing` (default) | `work_continue` |
| `review` | Quality loop / revision | `novel-chapter-review` (V1.47: renamed from `reflection-loop`) | `work_continue` |
| `persist` | Memory + KB promotion | `kb-extract`, `creator memory review` CLI | `knowledge_ingest`, `work_continue` |

Stages are **linear** in product order. Skipping a stage requires explicit `--force` (logged) or PM-approved compass exception — not conditional graph edges.

### 3.2 `stage_status` (per Work)

| Value | Meaning |
| --- | --- |
| `pending` | Stage not started |
| `active` | Schedule running or waiting for user |
| `complete` | Stage finished successfully |
| `skipped` | User forced skip (audit) |
| `failed` | Terminal failure; user may retry advance |

**Storage**: columns on `works` table — `current_stage`, `stage_status` (JSON map optional for per-stage status in P1 plan).

### 3.3 Stage gates

Advance from stage `S` to `S+1` requires:

1. `intake_status == complete` before leaving `intake` (unchanged).
2. Current stage `stage_status == complete` for `S`, unless `--force`.
3. No other **active** stage schedule on the Work.

CLI:

```text
creator works status [<work_id>]    # includes current_stage + stage_status (V1.41; default = pool active)
creator run <preset_id> [<work_id>] # e.g. creator run research, creator run novel-writing
```

---

## 4. Preset chain (normative mapping)

| Stage | Preset ID | Notes |
| --- | --- | --- |
| `intake` | `creative-brief-intake` | Shipped V1.33 |
| `research` | `research` | May append references to Work context |
| `produce` | `novel-writing` | Uses `creative_brief` + `inspiration_log`; novel profile writes to `Works/<work_ref>/` per [novel-writing/workflow-profile.md](novel-writing/workflow-profile.md) |
| `review` | `novel-chapter-review` | V1.47 P0: renamed from `reflection-loop` (compass §0.1 #6). Persists ≥1 finding per review pass via the supervisor terminal hook; see [novel-writing/quality-loop.md §8](novel-writing/quality-loop.md#8-reflection-loop-output-contract-v147-draft). |
| `persist` | `kb-extract` (via queue) + CLI memory review | **V1.40 P3**: World-bound novel Works enqueue extract with `work.world_id`; worldless Works skip World promotion. See [novel-writing/workflow-profile.md §3.5.1.5](novel-writing/workflow-profile.md). |

P2 may add wiring presets or seeds only; **no** new conditional `next.kind`.

---

## 5. User journeys

### 5.1 Happy path (explicit stages)

```text
creator workspace init && daemon start && acp agent use
creator bootstrap --idea "..."
  → intake schedule → brief complete
creator run research <work_id>      # preset runner validates stage gates, PATCHes Work stage
creator run novel-writing <work_id>
creator run novel-chapter-review <work_id>
creator run kb-extract <work_id>
creator memory review <id>
creator kb queue-extract --world-id <work.world_id>  # World-bound novel Works (V1.40 P3)
```

For **World-bound** novel Works (`work.world_id != NULL`), the auto-chain `persist` stage MUST enqueue extraction targeting `work.world_id`. For **worldless** Works, persist skips World KB promotion.

### 5.2 Inspiration without stage change

```text
creator run continue <work_id> --note "new angle"
```

Does **not** advance `current_stage`; merges into `inspiration_log` and schedule `core_context` per Work model.

### 5.3 Power user

`daemon schedule` remains valid; schedules created via `creator run <preset>` **must** record `work_id` and stage id in schedule seed/metadata (wire key `fl_e_stage` in V1.34 implementation).

### 5.4 Daemon-attached auto-chain (V1.39 extension)

When `auto_chain_enabled` on a Work (default true for new starts):

1. **Online**: on stage/chapter completion, the engine enqueues the next FL-E driver schedule without a manual `creator run <preset>` dispatch.
2. **Chapter outer loop**: after `persist` for chapter N, auto-enqueue `produce` for chapter N+1 until Work completion (novel profile).
3. **Checkpoint**: persist `stage`, `chapter`, `driver_schedule_id`, and `auto_chain_interrupted` on daemon shutdown or unexpected pause.
4. **Boot resume**: daemon restart auto-resumes only schedules tied to checkpointed auto-chain Works; other schedules remain paused (safe default).

`creator run resume <work_id>` recovers when auto-resume did not run or user disabled auto-chain.

### 5.5 Side-input lane (V1.39 extension)

Insertions during an active auto-chain **must not** fork or cancel the driver schedule:

| Insertion | Behavior |
| --- | --- |
| `creator run continue --note` | Appends `inspiration_log`; visible in `creator works status`; merged into prompt context at **next** preset state transition |
| `nexus.work.patch` (agent) | Same append-only inspiration surface per agent-nexus-tool-bridge |
| Research stage side effects | KB/reference artifacts written during chain; consumed by downstream produce/review via existing assembly (P0.5) |

Invariant: at most one active FL-E stage driver schedule per Work remains enforced.

### 5.6 Author reflection: SOUL visualization (V1.79 extension)

V1.79 adds a read-only reflection surface over the creator's internalized SOUL fragments. The surface consumes rows already stored in the local `memory_fragments` (`n`) table and renders:

- **Keyword clusters** from each fragment's `keywords` JSON array.
- **Temporal drift** from each fragment's `created_at` timestamp, with growth count folded into the timeline.

Wire contract: `schemas/local-api/memory/memory-fragment-info.schema.json` extends the list-fragments item DTO with optional `keywords: string[]` and optional `created_at: string` (RFC 3339 by description). The extension is additive: `fragment_id` and `summary` remain the only required fields, and internal ownership/session fields (`creator_id`, `session_id`, `ttl`) stay out of the response. The visualization is a consumer of the creator-scoped memory list endpoint; it does not create, patch, or delete memory fragments.

---

## 6. Conflicts and non-goals

| Topic | Rule |
| --- | --- |
| Work vs `creator kb --scope work` | Index entries may tag `work_id`; index does not define Work |
| Agent tools vs presets | Agent may read/patch Work via `nexus.work.*`; production presets still run via orchestration |
| Conditional routing | **Not** used for stage selection (DF-56) |
| `--auto-chain` | **V1.39 target (DF-53)**: default true for full FL-E chain + chapter outer loop; `--no-auto-chain` opt-out; manual `creator run <preset>` dispatch still valid for power users |
| Novel project init | Separate preset `novel-project-init` (DF-58); **not** part of `novel-writing` auto-chain |
| Novel completion | Work `status == completed` stops further `novel-writing`; V1.41 extends `mark_work_completed` per [novel-writing/multi-work-lifecycle.md](novel-writing/multi-work-lifecycle.md) (DF-60) |
| Completion-lock | While `.completion-lock.json` exists, auto-chain **must not** tick that Work; after release, `resume --reopen` may resume same `work_id` (V1.41 P0) |
| Runtime lock | `works.runtime_lock_holder` blocks concurrent mutating CLI/API on same Work (V1.41 P0) |
| Multi-Work concurrency | Multiple Works may auto-chain concurrently; pool `active` is CLI default only (DF-60/61) |
| Selection pool | `creator works pool` + `works use` via [novel-writing/work-pool.md](novel-writing/work-pool.md); not a Work profile (DF-61) |
| CLI IA | `creator run` = single-Work actions; `creator works` = list/status/use/pool (V1.41) |
| Platform cloud assemble | Not part of this workflow; see agent-nexus-tool-bridge `policy_blocked` |

---

## 7. Acceptance (spec-level)

1. Stage enum and preset mapping are stable in cli-spec and this document.
2. The preset runner rejects wrong stage order without `--force-gates` (stage gate validation inside `creator run <preset_id>`).
3. Demo path in V1.34 compass §4 is achievable on integration branch.
4. No contradiction with [work-experience-model.md](work-experience-model.md) §3–7.

---

## V1.45 supersession (P-last promotion)

**Superseded by**: [creator-run-preset-entry.md](creator-run-preset-entry.md) (Shipped Master V1.45). FL-E CLI table is now part of the canonical Master body — see §3.3 (`research` / `novel-writing` / `reflection-loop` / `kb-extract` preset ids) and §2 three-plane IA.

---

*Normative staged creator workflow. Shipped V1.34 via `.mstar/plans/2026-06-04-v1.34-*`.*
