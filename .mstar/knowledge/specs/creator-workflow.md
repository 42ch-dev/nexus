# Creator Workflow — Normative Specification

**Status**: Shipped (V1.34 — 2026-06-05)  
**Document class**: Feature line  
**Created**: 2026-06-04  
**Scope**: Staged creator journey on **Work** (`intake → research → produce → review → persist`), built on shipped `creator run` + `run_intents`  
**Coordinates with**:

- [work-experience-model.md](work-experience-model.md) — Work entity, intake, run_intents
- [cli-spec.md](cli-spec.md) — `creator run` and `creator run stage`
- [orchestration-engine.md](orchestration-engine.md) — presets, schedules, capabilities
- [agent-nexus-tool-bridge.md](agent-nexus-tool-bridge.md) — Agent-initiated context/tools (parallel channel)

**Iteration compass**: [v1.34-creator-workflow-and-agent-tools-delivery-compass-v1.md](../../iterations/v1.34-creator-workflow-and-agent-tools-delivery-compass-v1.md)

---

## 1. Purpose

The Work loop shipped in V1.33 centered on Creative Brief Intake and `novel-writing`. This spec generalizes the journey to:

```text
intake → research → produce → review → persist
```

without introducing a second scheduler or replacing World/KB SSOT. Stages are **explicit** (user or script advances); default auto-chaining remains deferred (DF-53).

---

## 2. Relationship to Work model

| Concept | Work model (V1.33) | Staged workflow (this spec) |
| --- | --- | --- |
| Work container | Shipped | Extended with `stage`, `stage_status` |
| Entry | `creator run start` | Unchanged; intake still `work_init` |
| Continue inspiration | `creator run continue --note` | Unchanged |
| Stage progression | N/A | **`creator run stage advance --stage <id>`** |
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
| `review` | Quality loop / revision | `reflection-loop` | `work_continue` |
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
creator run stage list <work_id>
creator run stage advance <work_id> --stage <stage_id> [--force]
creator run status <work_id>    # includes current_stage + stage_status
```

---

## 4. Preset chain (normative mapping)

| Stage | Preset ID | Notes |
| --- | --- | --- |
| `intake` | `creative-brief-intake` | Shipped V1.33 |
| `research` | `research` | May append references to Work context |
| `produce` | `novel-writing` | Uses `creative_brief` + `inspiration_log` |
| `review` | `reflection-loop` | `llm_judge` gates per orchestration-engine |
| `persist` | `kb-extract` (via queue) + CLI memory review | No dedicated persist-only preset required |

P2 may add wiring presets or seeds only; **no** new conditional `next.kind`.

---

## 5. User journeys

### 5.1 Happy path (explicit stages)

```text
creator workspace init && daemon start && acp agent use
creator run start --idea "..."
  → intake schedule → brief complete
creator run stage advance --stage research
creator run stage advance --stage produce
creator run stage advance --stage review
creator run stage advance --stage persist
creator memory review <id>
creator kb queue-extract  # when applicable
```

### 5.2 Inspiration without stage change

```text
creator run continue <work_id> --note "new angle"
```

Does **not** advance `current_stage`; merges into `inspiration_log` and schedule `core_context` per Work model.

### 5.3 Power user

`daemon schedule` remains valid; schedules created via `creator run` / stage advance **must** record `work_id` and stage id in schedule seed/metadata (wire key `fl_e_stage` in V1.34 implementation).

---

## 6. Conflicts and non-goals

| Topic | Rule |
| --- | --- |
| Work vs `creator kb --scope work` | Index entries may tag `work_id`; index does not define Work |
| Agent tools vs presets | Agent may read/patch Work via `nexus.work.*`; production presets still run via orchestration |
| Conditional routing | **Not** used for stage selection (DF-56) |
| `--auto-chain` | Deferred DF-53; explicit `stage advance` required |
| Platform cloud assemble | Not part of this workflow; see agent-nexus-tool-bridge `policy_blocked` |

---

## 7. Acceptance (spec-level)

1. Stage enum and preset mapping are stable in cli-spec and this document.
2. `creator run stage advance` rejects wrong stage order without `--force`.
3. Demo path in V1.34 compass §4 is achievable on integration branch.
4. No contradiction with [work-experience-model.md](work-experience-model.md) §3–7.

---

*Normative staged creator workflow. Shipped V1.34 via `.mstar/plans/2026-06-04-v1.34-*`.*
