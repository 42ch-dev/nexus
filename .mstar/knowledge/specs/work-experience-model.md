# Work Experience Model — Normative Specification v1

**Status**: Shipped (V1.33 — Work loop + Creative Brief Intake + `creator run` + `run_intents`)  
**Document class**: Feature line  
**Created**: 2026-06-04  
**Shipped**: 2026-06-04 (compass [v1.33-work-experience-loop-delivery-compass-v1.md](../../iterations/v1.33-work-experience-loop-delivery-compass-v1.md) — 5 plans P1–P5 all Done)  
**Scope**: Product-level **Work** container, user journey, Creative Brief Intake, preset run intents, and relationship to workspace / World / schedules  
**Coordinates with**:

- [cli-spec.md](cli-spec.md) — `creator run`, `system preset`
- [orchestration-engine.md](orchestration-engine.md) — presets, schedules, `llm_judge`, run_intents
- [creator-schedule-and-core-context.md](creator-schedule-and-core-context.md) — schedule + core_context
- [entity-scope-model.md](entity-scope-model.md) — Creator / World hierarchy

---

## 1. Purpose

Nexus OSS needs a **first-class Work** concept so creators can:

1. Start from a **small initial creative direction** (a few words or sentences).
2. Pass through a **structured clarification** phase (grill-me / Creative Brief Intake) before heavy production.
3. **Continue** the same piece of work with additional inspiration and direction tweaks without starting a new schedule from scratch.
4. Run **appropriate presets** only in contexts where they are allowed (init vs continue vs maintenance).

Without Work, users must understand `daemon schedule`, preset IDs, seed strings, and `core_context` — which is correct for power users but not for a complete product cycle.

---

## 2. Definitions

| Term | Meaning | Owning layer |
| --- | --- | --- |
| **Workspace** | Per-creator operational root (`workspace_slug`), `state.db`, filesystem layout | `nexus-home-layout`, CLI `creator workspace` |
| **Work** | A single **creative effort** with stable `work_id`, long-term goal, structured brief, optional World binding, inspiration log, linked runs | **V1.33** — `state.db` table + CLI `creator run` |
| **Work index** | CLI `creator kb --scope work` local file index | Daemon `/v1/local/kb/entries` — **not** Work |
| **World** | Narrative universe (`world_id`) with timeline and World KB | `nexus-narrative`, `nexus-kb` |
| **Creative brief** | Structured output of intake; required fields before `novel-writing` production states | Work.creative_brief (JSON) |
| **Run** | One execution of a preset bound to a Work (via schedule/session) | Schedule + orchestration session |
| **Preset run intent** | Declared capability of a preset in manifest metadata | `preset.yaml` → `run_intents[]` |

---

## 3. Work entity (normative)

### 3.1 Identity and scope

- `work_id`: opaque stable id (e.g. `wrk_<uuid>`), unique per `(creator_id, workspace_slug)`.
- `creator_id`: owning creator (required).
- `workspace_slug`: active workspace when Work was created (required).
- `status`: `draft` | `active` | `paused` | `completed` | `archived`.

### 3.2 Core fields

| Field | Type | Required | Description |
| --- | --- | --- | --- |
| `title` | string | yes | Human label (may start as truncated idea) |
| `long_term_goal` | string | yes | What “done” means for this Work |
| `initial_idea` | string | yes | Raw user input at start |
| `creative_brief` | object | after intake | Structured brief (schema §4) |
| `intake_status` | enum | yes | `pending` \| `in_progress` \| `complete` \| `skipped` |
| `world_id` | string | no | Bound narrative world when known |
| `story_ref` | string | no | Preset/manuscript ref when allocated |
| `inspiration_log` | array | no | Append-only `{at, note}` supplements |
| `primary_preset_id` | string | default `novel-writing` | Main production preset |
| `schedule_ids` | string[] | no | Linked orchestration schedule ids |
| `created_at` / `updated_at` | timestamp | yes | Audit |

### 3.3 Storage (V1.33 target)

- **SSOT**: `creators/<creator_id>/workspaces/<workspace_slug>/state.db` table `works` (exact DDL in P1 plan).
- **Not** in `<workspace>/` user-visible tree (runtime metadata only).
- **Not** duplicated in SOUL.md (SOUL remains creator identity; Work is project-scoped).

### 3.4 Invariants

1. A Work is **never** a workspace and **never** a World.
2. `creator kb --scope work` entries may **reference** a `work_id` tag in metadata but do not define Work.
3. At most one **active** `work_init` intake schedule per Work while `intake_status != complete` (unless user explicitly restarts intake).
4. `work_continue` presets require `intake_status == complete` OR explicit `--force` (logged).

---

## 4. Creative brief schema (post-intake)

Minimum required keys after Creative Brief Intake:

```json
{
  "genre": "string",
  "tone": "string",
  "audience": "string",
  "constraints": ["string"],
  "themes": ["string"],
  "non_goals": ["string"],
  "protagonist_hook": "string",
  "setting_hook": "string",
  "open_questions_resolved": ["string"]
}
```

Validation rules:

- All string fields non-empty after trim.
- `constraints` and `themes` at least one entry each (may be `["none"]` if explicitly confirmed in intake).
- Brief version field `brief_schema_version: 1` for forward compatibility.

Intake preset must **fail closed**: if brief invalid, Work stays `intake_status=in_progress` and `novel-writing` cannot start via `creator run`.

---

## 5. Preset run intents

### 5.1 Manifest extension

In `preset.yaml` (top-level):

```yaml
run_intents:
  - work_init
  - work_continue
```

Allowed values (closed enum for V1.33):

| Intent | Meaning |
| --- | --- |
| `work_init` | Allowed to create or attach as first run on a new Work |
| `work_continue` | Allowed when Work exists; may append inspiration / resume production |
| `knowledge_ingest` | Reference scanning, extraction, KB pipelines |
| `work_maintenance` | SOUL/experience refresh, non-narrative upkeep |
| `system_maintenance` | `_system.*` only |

Loader **must** reject unknown intent strings. Embedded presets updated in V1.33 plans.

### 5.2 Embedded preset classification (V1.33 target)

| Preset | run_intents |
| --- | --- |
| `creative-brief-intake` (new) | `work_init` |
| `novel-writing` | `work_init`, `work_continue` |
| `research` | `knowledge_ingest`, `work_continue` |
| `memory-augmented` | `work_continue` |
| `reflection-loop` | `work_continue` |
| `kb-extract` | `knowledge_ingest` |
| `soul-experience-refresh` | `work_maintenance` |
| `reflection-loop` | `work_continue` |

Policy: only presets with `work_init` appear in `creator run start --preset` default list.

---

## 6. User journey (normative)

### 6.1 Prerequisites

Same as [cli-spec.md](cli-spec.md) §7: workspace init, daemon start, ACP agent use, active creator.

### 6.2 Start Work (`creator run start`)

```text
creator run start --idea "<text>" [--preset novel-writing] [--world-id <id>]
```

Behavior:

1. Create Work (`draft` → `active`, `intake_status=pending`).
2. If intake required: enqueue `creative-brief-intake` schedule (`work_init`).
3. On intake complete: set `creative_brief`, `intake_status=complete`.
4. Enqueue primary preset (default `novel-writing`) with seed derived from brief + `initial_idea`.
5. Print `work_id`, schedule ids, next-step hints.

### 6.3 Continue Work (`creator run continue`)

```text
creator run continue <work_id> [--note "<supplement>"] [--preset <id>]
```

Behavior:

1. Append `note` to `inspiration_log`.
2. Merge note into schedule `core_context` (via existing `daemon schedule edit --append` semantics).
3. Optionally start/resume preset with `work_continue` intent.
4. Do **not** create a new Work.

### 6.4 Inspect Work

```text
creator run list [--status active]
creator run status <work_id>
```

Shows: intake status, linked schedules, last session state, world binding.

### 6.5 Power-user escape hatch

`daemon schedule ...` remains valid; schedules created via `creator run` must record `work_id` in schedule metadata (P1).

---

## 7. Creative Brief Intake (grill-me)

### 7.1 Placement

- **Layer**: orchestration preset (ACP multi-turn), not CLI questionnaire.
- **Name**: embedded preset `creative-brief-intake` (or dedicated initial state on `novel-writing` — P2 chooses one; compass prefers **separate preset** for clearer `work_init`).

### 7.2 Behavior

1. Load role with skills for interviewing / clarification.
2. Multiple ACP turns until agent emits structured brief JSON (tool or delimiter contract).
3. Persist brief on Work via Local API (P1/P2).
4. Validator checks §4 schema before marking intake complete.

### 7.3 Non-goals

- Not a replacement for product PM grill-me harness (`.mstar` planning only).
- Not implemented as blocking CLI prompts without daemon.

---

## 8. Quality gates (`llm_judge`)

V1.33 requires runtime alignment with [orchestration-engine.md](orchestration-engine.md):

- `exit_when.kind: llm_judge` must invoke declared `judge_capability` (default `judge.llm`) with `template_file`.
- Until **conditional routing** ships, NOGO results in `WaitForInput` on the **same** linear edge (user may `daemon schedule advance` or append context via `creator run continue`).

Presets relying on quality loops: `novel-writing` (gathering exit), `reflection-loop` (draft/revise).

---

## 9. Memory and KB in the Work loop

| Phase | Action |
| --- | --- |
| During preset | `creator.read_memory` / `write_memory` → `memory_fragments` |
| After ACP session | `session_capture` → pending review queue |
| User promotion | `creator memory review` → LTM files |
| Work → World KB | `creator kb queue-extract` + `kb-extract` preset |
| Verification | `platform context assemble-moment` |

Closed loop requires daemon routes for review + fragments (P4).

---

## 10. Generic creator workflow — V1.34

**Normative SSOT**: [creator-workflow.md](creator-workflow.md) (stages, preset chain, stage advance).

V1.34 extends Work with:

| Field | Type | Description |
| --- | --- | --- |
| `current_stage` | enum | `intake` \| `research` \| `produce` \| `review` \| `persist` |
| `stage_status` | enum or map | Per-stage `pending` \| `active` \| `complete` \| `skipped` \| `failed` |

Stage progression is **explicit** via `creator run stage advance` (not default auto-chain — see DF-53).

FL-E reuses V1.33 `creator run start/continue/list/status` and does not replace `daemon schedule`.

---

## 11. Resolved / deferred questions

| Topic | Status |
| --- | --- |
| `works` DDL | Shipped V1.33 P1; stage columns V1.34 P1 |
| `creative-brief-intake` standalone preset | Shipped V1.33 P2 |
| Brief JSON from orchestration | Shipped V1.33 P2 |
| Auto-start `novel-writing` after intake | V1.33 `--chain-novel-writing`; V1.34 uses **stage advance** for produce |

---

*Normative product model for Work + FL-E. V1.33 plans: `.mstar/plans/2026-06-04-v1.33-*`. V1.34: `.mstar/plans/2026-06-04-v1.34-*`.*
