# Work Experience Model — Normative Specification v1

**Status**: Shipped — Work loop + Creative Brief Intake + `creator works` atomic operations + `run_intents`  
**Current authority**: the Work/entity/journey model below is the retained normative product model; preset admission has no CLI entry and goes through the core execution surface (`POST /v1/daemon/orchestration/schedules`, served by `apps/nexus-service`), with `creator works cron` as the retained CLI scheduling declaration surface.  
**Document class**: Feature line  
**Created**: 2026-06-04  
**Scope**: Product-level **Work** container, user journey, Creative Brief Intake, preset run intents, and relationship to workspace / World / schedules  
**Coordinates with**:

- [cli-spec.md](../cli/cli-spec.md) — `creator works` / `preset`
- [orchestration-engine.md](../orchestration/orchestration-engine.md) — presets, schedules, `llm_judge`, run_intents
- [creator-schedule-and-core-context.md](../orchestration/creator-schedule-and-core-context.md) — schedule + core_context
- [entity-scope-model.md](../architecture/entity-scope-model.md) — Creator / World hierarchy
- [novel-writing/workflow-profile.md](../novel-writing/workflow-profile.md) — `work_profile: novel` extension
- [essay-profile.md](essay-profile.md) — `work_profile: essay` extension

---

## 1. Purpose

Nexus OSS needs a **first-class Work** concept so creators can:

1. Start from a **small initial creative direction** (a few words or sentences).
2. Pass through a **structured clarification** phase (grill-me / Creative Brief Intake) before heavy production.
3. **Continue** the same piece of work with additional inspiration and direction tweaks without starting a new schedule from scratch.
4. Run **appropriate presets** only in contexts where they are allowed (init vs continue vs maintenance).

Without Work, users must understand preset IDs, seed strings, and `core_context` — which is correct for power users but not for a complete product cycle.

---

## 2. Definitions

| Term | Meaning | Owning layer |
| --- | --- | --- |
| **Workspace** | Per-creator operational root (`workspace_slug`), `state.db`, filesystem layout | `nexus-home-layout`, CLI `creator workspace` |
| **Work** | A single **creative effort** with stable `work_id`, long-term goal, structured brief, optional World binding, inspiration log, linked runs | Persistence: `nexus-local-db` (`works` table/DAO); operations: `nexus-core` + CLI `creator works` (§3.3) |
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
| `story_ref` | string | no | Legacy preset/manuscript ref; superseded for novel profile by `work_ref` |
| `work_profile` | enum | no | Creative kind: `novel` \| `essay` \| `game_bible` \| `script`. Essay completion: see [essay-profile.md](essay-profile.md) §8. |
| `work_ref` | string | no | Filesystem directory name under `Works/<work_ref>/` when `work_profile` is set |
| `total_planned_chapters` | integer | no | Novel profile: planned chapter count (≥ 1) |
| `current_chapter` | integer | no | Novel profile: last active chapter index |
| `inspiration_log` | array | no | Append-only `{at, note}` supplements |
| `primary_preset_id` | string | default `novel-writing` | Main production preset |
| `schedule_ids` | string[] | no | Linked orchestration schedule ids |
| `created_at` / `updated_at` | timestamp | yes | Audit |

### 3.3 Storage

- **SSOT**: `creators/<creator_id>/workspaces/<workspace_slug>/state.db` table `works`.
- **Not** in `<workspace>/` user-visible tree (runtime metadata only).
- **Not** duplicated in SOUL.md (SOUL remains creator identity; Work is project-scoped).

**Current implementation ownership**:

- **Creator aggregate / local identity:** [`crates/nexus-creator/src/creator.rs`](../../../../crates/nexus-creator/src/creator.rs) defines `Creator`, its identity/status, and user linkage. `nexus-creator` does **not** implement the Work store or Work operations.
- **Work persistence:** each `(creator_id, workspace_slug)` has its `state.db` Work store. `nexus-local-db` owns the `works` tables/migrations (starting with [`20260604_works_table.sql`](../../../../crates/nexus-local-db/migrations/20260604_works_table.sql)) and the [`works.rs` DAO / `WorkRecord`](../../../../crates/nexus-local-db/src/works.rs); chapter persistence is in [`work_chapters.rs`](../../../../crates/nexus-local-db/src/work_chapters.rs). Work is not a nested field of the Creator aggregate.
- **Work operations / CLI:** [`crates/nexus-core/src/works.rs`](../../../../crates/nexus-core/src/works.rs) owns the typed Work service operations used by the retained [`creator works` command families](../../../../apps/nexus42/src/commands/creator/works/mod.rs), including selection/status/pool, inspiration, reopening, completion-lock, and chapter reconciliation.

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

Intake preset must **fail closed**: if brief invalid, Work stays `intake_status=in_progress` and `novel-writing` cannot be admitted to run.

---

## 5. Preset run intents

### 5.1 Manifest extension

In `preset.yaml` (top-level):

```yaml
run_intents:
  - work_init
  - work_continue
```

Allowed values (closed enum):

| Intent | Meaning |
| --- | --- |
| `work_init` | Allowed to create or attach as first run on a new Work |
| `work_continue` | Allowed when Work exists; may append inspiration / resume production |
| `knowledge_ingest` | Reference scanning, extraction, KB pipelines |
| `work_maintenance` | SOUL/experience refresh, non-narrative upkeep |
| `system_maintenance` | `_system.*` only |

Loader **must** reject unknown intent strings.

### 5.2 Embedded preset classification

| Preset | run_intents |
| --- | --- |
| `creative-brief-intake` (new) | `work_init` |
| `novel-writing` | `work_init`, `work_continue` |
| `research` | `knowledge_ingest`, `work_continue` |
| `memory-augmented` | `work_continue` |
| `novel-chapter-review` | `work_continue` |
| `kb-extract` | `knowledge_ingest` |
| `soul-experience-refresh` | `work_maintenance` |
| `novel-chapter-review` | `work_continue` |

Policy: only presets with `work_init` appear in the default bootstrap intake list.

---

## 6. User journey (normative)

### 6.1 Prerequisites

Same as [cli-spec.md](../cli/cli-spec.md) §7: workspace init, active creator, ACP agent use. There is no service-launch step.

### 6.2 Continue Work (`creator works inspire`)

```text
creator works inspire [<work_id>] --note "<supplement>"
```

Behavior:

1. Append `note` to `inspiration_log`.
2. Do **not** create a new Work.

### 6.3 Inspect Work

```text
creator works list [--status active]
creator works status <work_id>
```

Shows: intake status, linked schedules, last session state, world binding.

### 6.4 Power-user scheduling surface

The scheduling surfaces are `creator works cron` declaration editing (CLI) and the `/v1/daemon/orchestration/schedules` add/signal/list/inspect routes served by `apps/nexus-service`. Schedules created for a Work must record `work_id` in schedule metadata.

---

## 7. Creative Brief Intake (grill-me)

### 7.1 Placement

- **Layer**: orchestration preset (ACP multi-turn), not CLI questionnaire.
- **Name**: embedded preset `creative-brief-intake` (or dedicated initial state on `novel-writing` — P2 chooses one; compass prefers **separate preset** for clearer `work_init`).

### 7.2 Behavior

1. Load role with skills for interviewing / clarification.
2. Multiple ACP turns until agent emits structured brief JSON (tool or delimiter contract).
3. Persist brief on Work via Daemon API (P1/P2).
4. Validator checks §4 schema before marking intake complete.

### 7.3 Non-goals

- Not a replacement for product PM grill-me harness (`.mstar` planning only).
- Not implemented as blocking CLI prompts without daemon.

---

## 8. Quality gates (`llm_judge`)

Runtime alignment with [orchestration-engine.md](../orchestration/orchestration-engine.md) requires:

- `exit_when.kind: llm_judge` must invoke declared `judge_capability` (default `judge.llm`) with `template_file`.
- NOGO results in `WaitForInput` on the **same** linear edge; the author appends context via `creator works inspire --note` and the next admission comes from the execution surface.

Presets relying on quality loops: `novel-writing` (gathering exit), `novel-chapter-review` (review/revise).

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

## 10. Generic creator workflow

**Normative SSOT**: [creator-workflow.md](creator-workflow.md) (stages, preset chain, preset runner stage-gate validation).

Work carries the stage fields:

| Field | Type | Description |
| --- | --- | --- |
| `current_stage` | enum | `intake` \| `research` \| `produce` \| `review` \| `persist` |
| `stage_status` | enum or map | Per-stage `pending` \| `active` \| `complete` \| `skipped` \| `failed` |

Stage progression is **explicit** — schedule admission (`add_schedule`, `crates/nexus-core/src/execution/handle_ops.rs:115`) checks the preset's gate policy and inserts a **pending** schedule with no CLI dispatch entry, and it does **not** itself set the Work stage. The stage writers are the auto-chain scheduler (`crates/nexus-orchestration/src/auto_chain.rs` — `update_checkpoint`:869, `mark_work_completed`:904, `set_driver`:1054, `enqueue_auto_chain_schedule_in_tx`:1583, all via the local-db `works::patch_work*`) and the `nexus.manuscript.phase.set` tool (`execute_manuscript_phase_set`, `crates/nexus-core/src/execution/capabilities.rs:2707` → `works::update_work_stage`, `crates/nexus-local-db/src/works.rs:1220`).

---

*Normative product model for Work + FL-E.*
