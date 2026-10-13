# Novel Workflow Profile — Normative Specification v1

**Status**: Shipped — novel workflow profile overlay: artifact layout (§3), `work_chapters` chapter-state SSOT (§4.1), World binding (§3.5), narrative indexes (§4.6), preset gates (§5.3), completion (§6), cron staggering + auto-chronology (§11)  
**Document class**: Feature line (profile overlay)
**Created**: 2026-06-07  
**Last updated**: 2026-10-13
**Scope**: `work_profile: novel` on generic **Work** — artifact layout under `Works/<work_ref>/`, templates, chapter status, completion semantics, sync boundaries  
**Coordinates with**:

- [work-experience-model.md](../creator/work-experience-model.md) — generic Work entity
- [creator-workflow.md](../creator/creator-workflow.md) — FL-E `produce` stage
- [cli-spec.md](../cli/cli-spec.md) — workspace layout §13.1
- [sync-contract.md](sync-contract.md) — chapter discovery
- [orchestration-engine.md](../orchestration/orchestration-engine.md) — `novel-writing` preset
- [entity-scope-model.md](../architecture/entity-scope-model.md) — World entity + World KB (`work_profile: novel` binds Work to World; world content is cross-Work, lives in World KB, NOT in per-Work `Worldbuilding/` subtree)

---

## 1. Purpose

Nexus models creative efforts as generic **Work** containers. Novel/long-form fiction is one **profile** (`work_profile: novel`) with filesystem artifacts, prompts, and status rules distinct from other creative kinds (essay, script, game bible, etc.).

This spec defines the novel profile up to **正文产出** (scaffold → outline → draft → final for at least one chapter). It explicitly excludes platform **publish** integration.

**Pre-1.0 policy**: Legacy workspace-root `Stories/<story_ref>/` is **not** supported. All novel artifacts live under `Works/<work_ref>/`.

---

## 2. Relationship to Work

| Concept | Generic Work | Novel profile (`work_profile: novel`) |
| --- | --- | --- |
| Identity | `work_id`, `creator_id`, `workspace_slug` | Same |
| Human slug | optional `story_ref` | **`work_ref`** = directory name under `Works/` (may equal `story_ref`) |
| Status | `draft` \| `active` \| `paused` \| `completed` \| `archived` | Adds **`work_chapters` table** in `state.db` (chapter state SSOT; see §4.1) |
| Intake | `creative_brief` via intake preset | Same; init preset may precede intake |
| Produce preset | default `novel-writing` | Same; paths per §3 |
| Completion | generic `long_term_goal` met | §6 criteria — stops `novel-writing` auto-progression |

### 2.1 `work_profile` field

On `works` table / Work API (additive):

| Field | Type | Required when profile=novel | Description |
| --- | --- | --- | --- |
| `work_profile` | enum | yes | `novel` for this spec; future: `essay`, `script`, … |
| `work_ref` | string | yes | Filesystem directory name: `Works/<work_ref>/` |
| `total_planned_chapters` | integer | no | Target chapter count for completion (default TBD in init preset) |
| `current_chapter` | integer | no | Latest chapter number in progress |
| `world_id` | string (FK) | yes for new novel Works | Bind to a World (per [entity-scope-model.md](../architecture/entity-scope-model.md) §5.4). Required for Work creation/init; legacy `NULL` is allowed only when reading legacy worldless Works. `novel-project-init` grill-me asks whether to create a new World or bind an existing one (see §3.5). |
| `novel_completion_status` | enum | no | `in_progress` \| `completed` (mirrors Work.status when terminal) |

**Invariant**: `work_ref` is stable for the life of the Work; renaming directory without DB update is unsupported pre-1.0.

---

## 3. Artifact layout

### 3.1 Generic Work root

```text
<workspace>/
  Works/
    <work_ref>/
      README.md                 # human overview; includes bound world_id for World-bound Works; legacy worldless Works may include brief world setting notes
      Outlines/
        volume-outline.md       # optional (single-volume MVP)
        chapters/
          ch<nn>-outline.md
        foreshadowing.md        # empty stub at scaffold time (F### rows; §4.6 runtime)
        event-index.md          # empty stub at scaffold time (E### rows; §4.6 runtime)
      Stories/                  # novel正文 ONLY — sync chapter scan root
        ch<nn>-<slug>.md
      Logs/                     # optional process logs (see quality-loop.md §5)
```

### 3.2 Directory rules

| Path | Sync chapter? | Purpose |
| --- | --- | --- |
| `Works/<work_ref>/README.md` | **No** | Human overview; World-bound Works include bound `world_id`; legacy worldless Works may include world setting notes here |
| `Works/<work_ref>/Outlines/chapters/*` | **No** | Per-chapter outline |
| `Works/<work_ref>/Outlines/foreshadowing.md` | **No** | Cross-chapter foreshadowing index (F### rows) |
| `Works/<work_ref>/Outlines/event-index.md` | **No** | Cross-chapter event index (E### rows) |
| `Works/<work_ref>/Outlines/volume-outline.md` | **No** | Volume-level outline (optional) |
| `Works/<work_ref>/Stories/*.md` | **Yes** | Chapter正文 (frontmatter `chapter`, `status`) |
| `Works/<work_ref>/Logs/**` | **No** | Process logs ([quality-loop.md](quality-loop.md) §5); excluded from chapter sync |

### 3.3 Legacy prohibition

The following are **removed from normative specs** (pre-1.0):

- Workspace-root `Stories/<story_ref>/`
- Prompt variables defaulting to `Stories/{{preset.input.story_ref}}/…` without `Works/<work_ref>/` prefix
- Sync scanners treating any `Works/<work_ref>/*.md` as chapters

Implementations **must not** provide dual-path fallbacks.

### 3.4 Other creative profiles (future)

Non-novel `work_profile` values may use different subtrees under `Works/<work_ref>/` (e.g. `Drafts/`, `Sections/`). They **must not** reuse `Stories/` unless the profile spec says so.

### 3.5 World integration (cross-Work worldbuilding)

**Key principle**: worldbuilding content is **cross-Work**, not per-Work. A `Works/<work_ref>/` directory is **one event in a World's timeline**, not the canonical home of characters, locations, society, or rules. The canonical home is **World KB** (per [entity-scope-model.md](../architecture/entity-scope-model.md) §5.4 World entity + `nexus-kb` crate).

Therefore:

- **No per-Work `Worldbuilding/` subtree**. The reference-system pattern of `{作品目录}/世界设定/` is intentionally rejected for OSS core; world content is the World's job, not the Work's.
- **`world_id` is the mandatory binding for new novel Works**. New Works must be **World-bound** (`work.world_id` set) at creation/init time.
  - **World-bound Work** (`world_id != NULL`): characters, locations, society, rules, events, timelines come from World KB. Chapter body may reference World KB items by id via `world_refs: [char_xxx, loc_yyy]` frontmatter; validation per §3.5.1.4.
  - **Legacy worldless Work** (`world_id == NULL` only): no cross-Work continuity. Existing data remains readable/operable; creation/init must not produce this state.
- **`novel-project-init` asks the binding question** (grill-me). Two valid options: bind to existing `world_id` (user picks from `nexus42 creator world list`) / create new World (calls `creator world create --title "..."`, narrative kind implicit). There is no "stay worldless" creation option.
- **Work → World KB promotion** is the **long-term** path: as chapters finalize, `kb-extract` preset (existing, per [creator-workflow.md](../creator/creator-workflow.md) `persist` stage) can extract entities / events / rules from chapter body into World KB items. This path is normative per §3.5.1.

### 3.5.1 World KB continuity implement contract

**Scope of this contract**: The `novel-writing` `world_binding` mode is `required` for new Work creation/init. Legacy worldless Works continue to read/operate without a World context block, but no init path may create a worldless Work. Implement slices: P0 world create + validation; P1 taxonomy; P2 prompt context block; P3 kb-extract binding.

#### 3.5.1.1 Mandatory World binding paths for `novel-project-init`

CLI contract:

```text
nexus42 creator world create --title "Neon River" --description "Solarpunk noir city-world"
→ world_id: wld_<uuid>
```

Note: `--kind narrative` is implicit (deferred to P1 taxonomy). `--title` is canonical; `--name` is an alias (see [cli-spec.md §6.2G](../cli/cli-spec.md)).

The init grill-me has exactly two valid binding paths:

1. **Create new World** — call `nexus42 creator world create --title "..."` and bind the returned `world_id` to the Work.
2. **Bind existing World** — pick a visible World from `nexus42 creator world list` and pass/bind its `world_id`.

Both paths compose with P0 `AddScheduleRequest.input` wiring as follows:

1. `novel-project-init` records the user's choice as either `preset.input.create_world = true` plus `world.title` / optional `world.description`, or as `preset.input.world_id = wld_<uuid>` from the existing-World list.
2. For create-new, the daemon invokes a `world create` capability owned by `nexus-narrative`/`nexus-kb`, equivalent to the CLI contract above; for bind-existing, it validates the selected `world_id` against the same store.
3. The resulting `world_id` is bound to the Work and PATCHed via the same atomic scaffold transaction as `work_ref`, `total_planned_chapters`, and `work_chapters` seeding (§5.4.3–§5.4.4).
4. If world creation or existing-World validation fails, the scaffold transaction fails closed: no partial `Works/<work_ref>/` tree, no duplicated `work_chapters`, and no `works.world_id` mutation.

#### 3.5.1.2 `world_id` validation

When a novel Work is created/init-scaffolded, `world_id` MUST reference an existing World visible under the active `creator_id` + `workspace_slug` context. Missing or omitted `world_id` values return a structured `preset_gates_failed`-style error with remediation. Legacy worldless Works with `world_id == NULL` remain readable, but that legacy state is not creatable by init paths.

```text
error: preset_gates_failed
  preset: novel-project-init
  failed_gates:
    - work_field: world_id must reference an existing World (actual: missing or wld_missing)
        ↳ Create the World first via `nexus42 creator world create --title "..."` or pick an existing one with `nexus42 creator world list`.
```

The P0 gate evaluator already supports the `world_id` gate for `novel-writing`; the preset-level toggle is `world_binding: required` (§5.3.4). Implementations may keep the `optional` mode internally for back-compat/legacy reads, but new `novel-project-init` creation must fail closed without a valid `world_id`.

#### 3.5.1.3 Prompt-time World context block

For a World-bound Work (`world_id != NULL`), before each outline and draft prompt the orchestration engine injects a **World context block** sourced from a future `creator kb query world <world_id>` capability or equivalent `nexus-kb` query API. The block is a compact, prompt-safe object; it is not a replacement for full World KB retrieval.

Minimum YAML shape:

```yaml
world_id: wld_123
world_name: "Neon River"
current_timeline: "chapter 3: after the river-market fire"
characters_in_chapter:
  - id: char_lin_xia
    name: "Lin Xia"
    descriptor: "ex-cartographer hiding a forbidden river map"
locations_referenced:
  - id: loc_neon_city
    name: "Neon City"
    descriptor: "tiered canal metropolis"
active_rules:
  - id: rule_magic_cost
    name: "Memory-for-light exchange"
    descriptor: "large spells erase recent autobiographical memory"
```

`characters_in_chapter` and `locations_referenced` are selected from `world_refs` when available, then from outline/body heuristics if needed. `active_rules` includes high-priority `foundation` and `rules` category items that constrain the scene. For legacy worldless Works (`world_id == NULL`), the block is omitted and prompts use only `Works/<work_ref>/README.md` setting notes, preserving read-time compatibility.

#### 3.5.1.4 `world_refs` validation

For World-bound Works, each chapter frontmatter `world_refs` entry MUST be a valid World KB item id under `work.world_id`.

Canonicalization rules:

1. Trim leading/trailing whitespace before validation.
2. Treat ids as **case-sensitive**.
3. Reject duplicates after trimming.
4. Preserve author order for prompt relevance.

Validation timing:

- **Outline time**: invalid ids produce warnings and remediation hints, because outlines may introduce provisional entities that have not yet been promoted.
- **Finalize time** (or transition to `finalized`): invalid ids are errors and block the transition unless the user explicitly overrides with an audit reason.

For World-bound Works, `world_refs` is required by the validation contract: it may be empty only when the chapter truly references no World KB items, and any non-empty entry must validate under `work.world_id`. For legacy worldless Works (`world_id == NULL`), `world_refs` is allowed but unused; if present, implementations should warn but not fail.

#### 3.5.1.5 Chapter → World KB extraction and promotion

The `creator-workflow.md` `persist` stage already maps Work → World KB to `creator kb queue-extract` + `kb-extract`. For novel Works, the extraction target is the Work's `world_id` when set:

- **World-bound Work**: `kb-extract` reads finalized chapter body + outline/event/foreshadowing indexes, extracts entities, events, rules, locations, and relationships, then creates or updates World KB items under `work.world_id` with SourceAnchors back to the chapter path and, where available, the timeline event.
- **Legacy worldless Work**: extraction is skipped or remains local Work scope; it MUST NOT silently create a new World or promote content into an arbitrary World.
- **Explicit promotion**: rows in `Outlines/event-index.md` and `Outlines/foreshadowing.md` may be promoted to World KB items only when the Work is World-bound and the agent/user marks the promotion explicitly (e.g. "promote E012 as background" or "promote F007 as rule").

**Acceptance** (per plan P0–P3): hermetic tests for valid/invalid/missing `world_id`, prompt block presence for new World-bound Works, legacy worldless read compatibility, `world_refs` warning/error timing, and `kb-extract` target selection.

**Anti-patterns** explicitly rejected:

- Per-Work `Worldbuilding/character.md`, `Worldbuilding/location.md` etc. — content is duplicated across Works; defeats the cross-Work reuse.
- Hard-coded character name references in `novel-writing` prompts without KB lookup — produces drift from World state.
- Inheriting the reference-system `{作品目录}/世界设定/` subtree shape into OSS — wrong layer (per-Work is the wrong layer for world content).

DF-63 (World KB cross-Work unification) is closed.

---

## 4. Templates and frontmatter

### 4.1 `work_chapters` table (chapter state SSOT) and `README.md` (human overview)

Chapter state lives in the **DB table** `work_chapters` in `state.db` (owned by `nexus-local-db`). Chapter state is not author-edited as markdown; the orchestration engine owns the truth (CLI reads it via `creator works status`).

#### 4.1.1 `work_chapters` table DDL

```sql
CREATE TABLE work_chapters (
  work_id              TEXT NOT NULL,
  chapter              INTEGER NOT NULL,        -- 章节号; 1..total_planned_chapters
  volume               INTEGER,                  -- nullable; single-volume rows leave NULL; multi-volume uses 1..N
  slug                 TEXT,                     -- filename slug, e.g. "the-third-layer"
  planned_word_count   INTEGER,                  -- 预计字数; set by novel-project-init or user override
  actual_word_count    INTEGER,                  -- 实际字数; auto-derived from chapter frontmatter `word_count` on transition
  status               TEXT NOT NULL,            -- not_started | outlined | draft | finalized | published
  outline_path         TEXT,                     -- relative to workspace root: Works/<work_ref>/Outlines/chapters/ch<nn>-outline.md
  body_path            TEXT,                     -- relative to workspace root: Works/<work_ref>/Stories/ch<nn>-<slug>.md
  created_at           INTEGER NOT NULL,
  updated_at           INTEGER NOT NULL,
  PRIMARY KEY (work_id, chapter),
  FOREIGN KEY (work_id) REFERENCES works(work_id) ON DELETE CASCADE
);
CREATE INDEX work_chapters_by_status ON work_chapters(status);
```

**Naming rationale**: `work_chapters` (vs `work_status`): avoids confusion with `works.status` (Work-level enum already on `works` table); symmetric with `works`; clear scope (per-chapter state, not Work-level state).

#### 4.1.2 Truth model (DB vs frontmatter)

- **`work_chapters` is the queryable SSOT** for `creator works status`, completion evaluation (§6), and sync module (if it needs per-chapter metadata).
- **Chapter .md frontmatter `status` is the human/LLM read-end**: the orchestration engine updates **both** on transition; the frontmatter flip is the visible "I'm now finalized" signal to the next prompt.
- **Reconciliation**: a manual `creator works reconcile-chapters <work_id>` walks `Works/<work_ref>/Stories/` and rebuilds `work_chapters` rows from filesystem for missing rows/files . For status disagreements, the DB row is the source of truth and the next run re-syncs chapter frontmatter via a single transition (§4.5.3). This is a manual command, not an automatic job.

#### 4.1.3 `README.md` (human overview, no chapter state)

`Works/<work_ref>/README.md` is the only human file. It is **author-edited** and may contain:

- Working title, premise, blurb
- For **legacy worldless** Works: a brief world setting note (1–2 paragraphs) so the LLM has context
- For **World-bound** Works (§3.5): a one-liner like `world_id: <uuid>` and links to World KB items the Work uses most
- Optional F### / E### anchors to `Outlines/foreshadowing.md` / `Outlines/event-index.md`

`README.md` is **not** scanned by the sync module (§3.2).

### 4.2 Chapter outline (`Outlines/chapters/ch<nn>-outline.md`)

Structured outline before正文. **Required headings**: opening scene, core conflict, turning point, climax, ending hook, character state change. **Foreshadowing is required**: the outline must list every foreshadowing item touched (buried or paid-off) in this chapter, referencing `Outlines/foreshadowing.md` F### ids. If the foreshadowing file does not yet exist for a single-chapter MVP Work, the outline body may declare new F### items inline and the next outline is responsible for promoting them to the index.

### 4.3 Chapter body (`Stories/ch<nn>-<slug>.md`)

```yaml
---
title: string
chapter: integer
volume: integer             # optional; single-chapter rows leave it empty. Used for multi-volume.
status: draft | finalized   # published reserved; not set by OSS core; mirror of work_chapters.status
word_count: integer         # optional; auto-derived from body length on transition to finalized; mirror of work_chapters.actual_word_count
world_refs: [string]        # optional; list of World KB item ids referenced in this chapter (validation per §3.5.1.4)
---
```

`volume` carries the multi-volume dimension. When present, the chapter must also appear in `Outlines/volume-outline.md` chapter range. Sync module does **not** validate cross-reference (intentional for single-volume Works). `world_refs` is a soft hint; for **World-bound** Works (§3.5) it should list World KB item ids (e.g. `char_lin_xia`, `loc_neon_city`); validation timing per §3.5.1.4. Body text only (title in frontmatter). Status transitions:

| Transition | Actor |
| --- | --- |
| → `draft` | `novel-writing` drafting phase |
| → `finalized` | `novel-writing` finalize phase **only after** `llm_judge` quality gate returns GO (see §5) |
| → `finalized` (manual override) | user explicit advance on NOGO with logged audit reason; escape hatch |

### 4.4 Embedded preset templates

Template stubs live under `crates/nexus-orchestration/embedded-presets/novel-writing/templates/` (or documented equivalent). Prompts reference `Works/{{work_ref}}/…` variables.

### 4.5 Multi-chapter and multi-volume semantics

Single-chapter behavior is a strict subset: when `total_planned_chapters == 1`, chapter 1 is selected, drafted, finalized, and completes the Work exactly as in the single-chapter MVP.

#### 4.5.1 Chapter state machine

The multi-chapter semantics keep the chapter status state machine:

```text
not_started → outlined → draft → finalized
```

`published` remains reserved for platform publish and is not set by OSS core. Each transition updates the `work_chapters` row first and then mirrors the visible status into the corresponding `Stories/ch<NN>-<slug>.md` frontmatter.

#### 4.5.2 Chapter selection algorithm

For a given `work_id`, `novel-writing` must choose a chapter from `work_chapters`, not from filename order alone. The selection contract is:

```sql
next_chapter(work_id):
  SELECT chapter FROM work_chapters
   WHERE work_id = ? AND status = 'not_started'
   ORDER BY chapter ASC LIMIT 1
  if no row: SELECT chapter FROM work_chapters
              WHERE work_id = ? AND status = 'draft'
              ORDER BY chapter ASC LIMIT 1
  if no row: work is at novel-completion (§6.1)
```

**Resume behavior**: if a row exists with `status == 'draft'` and there are no earlier `not_started` rows, `novel-writing` resumes that draft row. It does **not** create a new chapter row and does **not** advance to a later chapter until this draft finalizes or the user explicitly reconciles/edits state.

**Outlined rows**: `outlined` means the outline exists but正文 drafting has not started. The first implementation may either treat `outlined` as the selected `not_started` chapter's pre-draft state or add an explicit outline-resume branch, but it must not skip an `outlined` chapter in favor of a later chapter. If this algorithm is implemented literally with only `not_started` + `draft`, the outline step must transition `outlined` back into the same selected chapter before drafting.

**Work-level invariant**: `works.current_chapter` is updated only on transition to `finalized`. Its value is the chapter number of the latest finalized row, not the chapter currently being outlined or drafted. During an in-progress draft of chapter N, `current_chapter` still points at the latest finalized chapter (often `N - 1`).

#### 4.5.3 DB/frontmatter conflict resolution

`work_chapters` is the queryable source of truth for chapter status. If a `work_chapters` row and filesystem frontmatter in `Stories/ch<NN>-<slug>.md` disagree on `status`, the DB row wins. The next `novel-writing` preset pass or a manual `creator works reconcile-chapters <work_id>` run must re-sync frontmatter through a single status transition so that prompt-visible state catches up without inventing an extra chapter transition.

Missing filesystem hints are still surfaced to the user (see §8.1), but a missing or stale file does not authorize selecting a later DB row. Rebuilds must preserve the DB ordering and status semantics above.

#### 4.5.4 Primary key and volume migration decision

PK and volume migration (grill-me locked):

1. Backfill **implicit `volume = 1`** for all existing `work_chapters` rows (single-volume behavior unchanged).
2. Migrate PK to **`(work_id, volume, chapter)`** (see [local-db-schema.md](../runtime/local-db-schema.md) V1.42 amendment).
3. Preserve row data (`status`, `outline_path`, `body_path`, `actual_word_count`, timestamps) through an idempotent migration.
4. New multi-volume Works declare volume count at init; chapter numbers may repeat across volumes.

#### 4.5.5 Volume outline semantics

Volume-outline scaffold: `novel-project-init` grill-me captures volume count + per-volume chapter totals and seeds `Works/<work_ref>/Outlines/volume-{n}-outline.md` per the YAML structure below. Cross-volume chapter numbers may repeat across volumes (per §4.5.4 PK migration).

When `volume != NULL`, the chapter must appear in `Outlines/volume-outline.md` under that volume's chapter range. The minimum multi-volume outline structure is:

```yaml
---
work_id: <id>
volumes:
  - volume: 1
    title: "First volume title"
    chapter_range: [1, 12]
  - volume: 2
    title: "Second volume title"
    chapter_range: [13, 24]
---
```

The sync module (per [sync-contract.md](sync-contract.md)) does **not** validate `volume-outline.md` cross-references.

World continuity remains World-scoped: when `world_id != NULL` and a chapter references a World KB item, the reference lives in the chapter frontmatter `world_refs` field (§4.3), not in `Outlines/volume-outline.md`. `volume-outline.md` organizes chronology; it is not a World KB index.

#### 4.5.6 Prompt and preset parameterization roadmap

The `novel-writing` preset and templates must not bind or imply `chapter: 1`; chapter selection is a scheduler/gate-evaluator injected value such as `{{work_chapters.next_chapter}}` (or an equivalent preset input variable). The value must be derived from `next_chapter(work_id)` above after the P0 gates have passed (`intake_status == complete`, scaffold exists, `previous_preset: novel-project-init` complete).

This keeps `novel-writing` a single preset that scales from chapter 1 to chapter N rather than creating separate per-chapter presets.

#### 4.5.7 Future acceptance and migration tests

Test #6 (Future multi-volume migration: the `(work_id, chapter)` → `(work_id, volume, chapter)` migration is idempotent and preserves row data) is implemented. Verified by `w01_v142_migration_idempotent` test in `crates/nexus-local-db/tests/v142_migration_fixes.rs`. Index coverage for `next_chapter_volume_aware` query is also verified by `w02_volume_aware_index_coverage` test.
DR-67 (plan `2026-08-27-v1.179-p2-reliability-convergence`): tests #1–#5 are implemented in `crates/nexus-local-db/tests/v179_dr67_acceptance.rs`: `w179_dr67_01_next_chapter_selects_lowest_eligible` (#1, §4.5.2), `w179_dr67_02_current_chapter_advances_only_on_finalize` (#2, §4.5.2 work-level invariant), `w179_dr67_03_completion_requires_all_rows_finalized_count_match_and_intake` (#3, V1.44 row-count contract — see the #3 note below), `w179_dr67_04_resume_reuses_draft_row_without_creating_new` (#4, §4.5.2 resume + §5.4.5), and `w179_dr67_05_reconcile_rebuilds_missing_rows_with_db_status_ssot` (#5, §4.5.3) — mirroring the #6 annotation style above.

The DR-67 acceptance tests pin the following invariants (**Durable roadmap:** DR-67 — all six invariants are implemented; the list below is the normative description of what each pin covers):

1. **Chapter selection**: a 3-chapter Work with rows at varied statuses; assert `next_chapter(work_id)` returns the lowest eligible row per §4.5.2.
2. **`current_chapter` transitions**: `current_chapter` changes only when a row transitions to `finalized`, and it becomes the just-finalized chapter number.
3. **Novel completion**: completion fires only when the row set covers `total_planned_chapters`, every row is `finalized`, and `intake_status == complete` (F-002 volume-aware row-count contract implemented by `is_work_completed`). **Spec-pointer gap:** §6.1's `current_chapter >= total_planned_chapters` conjunct is not enforced by `is_work_completed` — the F-002 row-count contract supersedes it, and the DR-67 tests pin the implemented contract rather than the unenforced pointer.
4. **Resume behavior**: a new run against a Work with one `draft` row resumes that row and does not create a new row.
5. **Reconciliation**: `creator works reconcile-chapters <work_id>` rebuilds missing `work_chapters` rows/files from `Works/<work_ref>/Stories/` while preserving DB-as-status-SSOT conflict resolution (§4.5.3).
6. **Future multi-volume migration**: the `(work_id, chapter)` → `(work_id, volume, chapter)` migration is idempotent and preserves row data.

### 4.6 Narrative indexes — F### / E### runtime

`Outlines/foreshadowing.md` and `Outlines/event-index.md` are scaffolded as empty table stubs; this section defines the **minimum viable index runtime** — file-first SSOT.

#### 4.6.1 SSOT and boundaries

| Path | SSOT | Notes |
| --- | --- | --- |
| `Works/<work_ref>/Outlines/foreshadowing.md` | **File** | F### rows; not scanned as chapter by sync module |
| `Works/<work_ref>/Outlines/event-index.md` | **File** | E### rows; P1 read-only + stub writer |
| `work_chapters` table | DB | Unchanged; no index mirror table |

`sync_module.rs` `SKIP_FILES` must continue to exclude index files from chapter discovery.

#### 4.6.2 F### row schema (5-column — ground truth)

Markdown table columns (aligned with embedded template; per R-V149P1-01 reconciliation):

| Column | Required | Description |
| --- | --- | --- |
| `ID` | yes | `F` + three-digit zero-padded integer (`F001`) |
| `Description` | yes | Short human label |
| `Planted` | yes | Chapter number(s) where first introduced / buried |
| `Paid off` | no | Chapter number(s) where resolved |
| `Status` | yes | `planned` \| `buried` \| `paid_off` |

> **Note**: The 4-column summary in the Draft overlay (`id` / `description` / `status` / `chapters`) is deprecated. The 5-column schema above is ground truth per P1 runtime.

#### 4.6.3 Id allocation

- Next id = `max(existing numeric suffix) + 1` for the Work.
- Inline outline declaration `F###: description` creates row if id absent.
- Duplicate id with conflicting description → fail outline step with remediation citing reconcile or manual edit.

#### 4.6.4 Inline F### declaration format

Canonical format in chapter outline (§4.2 "Foreshadowing Touched" section):

```text
F001: description of the foreshadowing item
```

This is parsed by the outline section parser during `novel-writing`. New F### ids are allocated sequentially when no number is specified.

#### 4.6.5 Promotion path (outline → index)

After `outline_chapter` writes `Outlines/chapters/ch<nn>-outline.md`:

1. Parse "Foreshadowing Touched" section for F### references and inline new items.
2. Upsert rows into `foreshadowing.md` (atomic temp + rename).
3. Inject `foreshadowing_summary` block into subsequent draft prompt via `preset.yaml` → `foreshadowing_summary` template variable.

#### 4.6.6 Read-for-prompt path

The orchestration engine reads `Outlines/foreshadowing.md` before each draft prompt and injects a compact `foreshadowing_summary` block listing active (non-`paid_off`) F### rows with their status and chapter context.

#### 4.6.7 E### read stub

Event-index (`Outlines/event-index.md`): **P1 minimum** — preserve scaffold; read existing rows for prompt if present. **Durable roadmap:** DR-66 (E### event-index write+upsert promotion).

#### 4.6.8 World KB boundary

Explicit promotion from index rows to World KB remains **manual/opt-in** per §3.5.1.5. The index is not auto-synced to World KB.

---

---

## 5. Preset responsibilities

| Preset | Role | When | Gates (see §5.3) |
| --- | --- | --- | --- |
| `novel-project-init` | Interactive grill-me; sets `work_ref`, `total_planned_chapters`, required `world_id` (bind to existing / create new), scaffolds `Works/<work_ref>/` dirs (§5.4), seeds `work_chapters` rows | Before first `novel-writing` if scaffold missing | §5.3.1 |
| `creative-brief-intake` | Structured brief on Work | FL-E `intake` | (generic; out of novel overlay) |
| `novel-writing` | Outline → draft → **finalize (gated by `llm_judge`)** → `finalized`; per-chapter transitions update both `work_chapters` row + chapter frontmatter | FL-E `produce` | §5.3.2 |
| `novel-chapter-review` | FL-E `review` stage — novel/work/chapter-aware review that produces findings | FL-E `review` | §5.3.3 |

**Separation rule**: `novel-project-init` is **not** auto-chained inside `novel-writing`. The author explicitly schedules it when starting a new novel Work — there is no CLI preset runner; preset admission goes through the core execution surface (schedule add via `POST /v1/daemon/orchestration/schedules`, served by `apps/nexus-service`). The engine enforces this via the `previous_preset: novel-project-init` gate on `novel-writing` (§5.3.2).

### 5.1 Chapter finalize quality gate

`novel-writing`'s `finalize` state has `exit_when: kind: llm_judge` evaluating a **五问质量检验** prompt (opening three lines / conflict resonance / twist recall / new perspective / ending hook). This prevents the "click and finalize" demo feel where any draft becomes `finalized` without scrutiny.

- **GO** → both `work_chapters.status` AND chapter frontmatter `status` flip to `finalized`; `work_chapters.actual_word_count` is updated from frontmatter `word_count`; `current_chapter` advances.
- **NOGO** → `WaitForInput`; user may `creator works inspire <work_id> --note "..."` with additional context, then re-run the preset. `work_chapters.status` and frontmatter `status` both stay `draft`.
- **GO override on NOGO** → user explicit advance on NOGO with a logged audit reason only (§4.3); there is no CLI gate-override surface.

The 五问 template file lives at `embedded-presets/novel-writing/prompts/finalize-exit.md` (P3 deliverable). It references writing-craft-rules §2 (五问质量检验) when present; otherwise the template embeds the five questions inline.

### 5.1.1 Outline 五问 gate (Normative)

`novel-writing` gains an `outline_review` state between `outline_chapter` and `draft_chapter`. Its `exit_when: kind: llm_judge` evaluates `prompts/outline-exit.md`, which asks five outline-specific questions:

1. **Structure** — beat-level structure is explicit (headings, bullets, numbered beats).
2. **Arc** — a character or situation arc is present (conflict, stakes, change).
3. **Foreshadow** — at least one future-setup / foreshadowing signal is present.
4. **Pacing** — outline length is within sane bounds (not empty, not a full draft).
5. **Hook** — final line ends with tension, a question, or unresolved action.

The deterministic complement `outline_five_q_check` in `nexus-orchestration::quality_loop` mirrors these five dimensions and is used by tests / fallback paths; the preset gate is the LLM judge.

- **GO** → transition to `draft_chapter`; chapter frontmatter `status` stays `outlined` until drafting begins.
- **NOGO** → `WaitForInput`; the author may revise the outline and `creator works inspire <work_id> --note "..."`.
- The existing `finalize` 五問 gate in §5.1 is unchanged.

### 5.2 World-bound Work behavior (in `novel-writing` prompts)

When a Work has `world_id != NULL`:

- Before drafting each chapter, the orchestration engine injects a **World context block** into the prompt: character names + 1-line descriptors, key locations, current timeline position. Sourced from World KB via `creator kb query world <world_id>` (or equivalent capability).
- The LLM is asked to **name characters / locations exactly as in World KB** unless the chapter is introducing a new one (new ones go to `Outlines/event-index.md` for later `kb-extract` promotion).
- `world_refs: [char_xxx, loc_yyy]` frontmatter is filled by the agent based on what the chapter references; validation timing per §3.5.1.4.

For **legacy worldless** Works (`world_id == NULL`): no World context block; LLM uses `README.md` setting notes as the only world context.

### 5.3 Novel preset gates (overlay on orchestration-engine.md §7.9)

The novel profile declares **gate sets** for each of its three presets. The gate mechanism itself is generic (orchestration-engine.md §7.9 Master); this section defines the **novel-specific values**.

#### 5.3.1 `novel-project-init` gates

```yaml
gates:
  - kind: work_field
    field: work_profile
    op: in
    value: [null, novel]              # fresh Work, or re-init allowed
  - kind: work_field
    field: workspace_slug
    op: required
```

**Rationale**: `novel-project-init` is the bootstrap preset; it should run before `Works/<work_ref>/` exists, so it must NOT gate on filesystem. It also runs before `intake_status` is finalised, so no intake gate.

#### 5.3.2 `novel-writing` gates (most-constrained preset)

```yaml
gates:
  - kind: work_field
    field: work_profile
    op: equals
    value: novel
  - kind: work_field
    field: work_ref
    op: required                      # non-null
  - kind: work_field
    field: intake_status
    op: equals
    value: complete
  - kind: filesystem
    path: "Works/{{work_ref}}/"
    must_exist: true                  # scaffold must exist (from novel-project-init or hand-created)
  - kind: filesystem
    path: "Works/{{work_ref}}/Outlines/"
    must_exist: true
  - kind: filesystem
    path: "Works/{{work_ref}}/Stories/"
    must_exist: true
  - kind: work_field
    field: world_id
    op: required                      # if preset manifest declares world-binding required (see §5.3.4)
  - kind: previous_preset
    preset: novel-project-init
    status: complete
    scope: work
```

**Rationale**: `novel-writing` runs in the FL-E `produce` stage. The gates enforce the **layer cake** from §3.1 (scaffold dirs), the **Work identity** (profile + work_ref), the **intake** requirement, and the **World binding** (if the preset's `run_intents` declares it world-required). The `previous_preset` gate ensures the scaffold was actually created via `novel-project-init` (not hand-edited or copied from another Work).

#### 5.3.3 `novel-chapter-review` gates (review quality pass)

```yaml
gates:
  - kind: work_field
    field: work_profile
    op: equals
    value: novel
  - kind: work_field
    field: work_ref
    op: required
  - kind: filesystem
    path: "Works/{{work_ref}}/Stories/"
    must_exist: true
  - kind: previous_preset
    preset: novel-writing
    status: any_session               # at least one novel-writing session reached a state
    scope: work
```

**Rationale**: `novel-chapter-review` runs after at least one chapter draft. It needs the chapter directory but does not require the chapter to be `finalized` (review may be triggered on `draft` too).

#### 5.3.4 World-binding toggle (preset-level opt-in)

The `world_id` gate in §5.3.2 is conditional. The preset manifest can declare:

```yaml
preset:
  id: novel-writing
  # ...
  world_binding:
    mode: required                   # required | optional
```

- **`required`** (default for `novel-writing`): the `world_id` gate is active; new Works must have `world_id` set before scheduling. Legacy worldless Works (`world_id == NULL`) may continue to run only through explicit back-compat handling or audited override paths; creation/init must not create them.
- **`optional`**: retained only as an internal/back-compat mode for legacy reads or older manifests; `novel-project-init` treats it as a no-op and still requires a valid `world_id` at creation.

Recommendation: keep the toggle in manifests for compatibility with older presets, but default and enforce `required` for novel Work creation/init.

#### 5.3.5 Gate failure user experience

When a `novel-writing` gate fails (e.g. a schedule is added for a Work whose `novel-project-init` scaffold is missing), the engine surfaces at enqueue:

```text
error: preset_gates_failed
  preset: novel-writing
  work_id: wrk_abc123
  failed_gates:
    - filesystem: Works/cozy-mystery/ must exist (actual: missing)
        ↳ Run the `novel-project-init` grill-me (schedule it via the execution surface) to scaffold the Work.
    - work_field: intake_status must equal "complete" (actual: pending)
        ↳ Complete intake via the `creative-brief-intake` preset.
```

This is the **user-visible demo-pain killer**: instead of scheduling `novel-writing` and failing deep in the state graph, the engine rejects at enqueue with a clear remediation.

#### 5.3.6 Implementation note

- Gate evaluation is **read-only** at enqueue time; the engine queries `works` table and filesystem; it does not mutate.
- Gate evaluation is **idempotent**: a failed check leaves no side effects.
- Engine logs gate failures to the structured log with `preset_id`, `work_id`, `failed_gates` array. Failed-gate rate by preset is a future observability dashboard (out of scope).

### 5.4 `novel-project-init` scaffold protocol (file enumeration)

`novel-project-init` is the canonical way to bootstrap a Work's `Works/<work_ref>/` tree. The grill-me collects `work_ref`, `total_planned_chapters`, and the mandatory World binding choice (§3.5); on success, the preset's **scaffold capability** (or handler) creates the full directory tree, copies template stubs, and seeds `work_chapters` rows. This section enumerates every file/dir so the P1 implementer (and any hand-rolled init script) has a checklist.

#### 5.4.1 Directory tree created (all paths relative to workspace root)

```text
Works/
  <work_ref>/
    README.md                              # copy from embedded template; render with {{work_ref}}, {{title}}, {{world_id_or_null}}
    Outlines/
      volume-outline.md                    # copy from embedded template; single-volume: still created (placeholder)
      chapters/                            # mkdir (empty; first outline created on first outline pass)
      foreshadowing.md                     # copy from embedded template (F### table header; §3.1/§3.2)
      event-index.md                       # copy from embedded template (E### table header; §3.1/§3.2)
    Stories/                               # mkdir (empty; first chapter body created on first draft pass)
    Logs/                                  # mkdir (empty; structure OUT per DF-66)
```

**Not created** (intentional):

- `work-status.md` — replaced by `work_chapters` table (§4.1).
- `Worldbuilding/` subtree — content lives in World KB (§3.5).

#### 5.4.2 Template sources

All template files live under `crates/nexus-orchestration/embedded-presets/novel-project-init/templates/` (P1 deliverable). The init preset's scaffold capability:

1. Reads each template from the embedded asset.
2. Substitutes preset input vars (`work_ref`, `title`, `world_id`, etc.) using `handlebars-rust` (per [orchestration-engine.md](../orchestration/orchestration-engine.md) §7.3).
3. Writes to `Works/<work_ref>/...` at the path listed above.

World-bound Works render `README.md` with a one-line `world_id: <uuid>` header and links to the World KB items the Work will reference most. Legacy worldless Works may retain README inline world setting notes, but scaffold rendering does not create new worldless README variants.

#### 5.4.3 `work_chapters` row seeding (DB writes)

For `chapter IN 1..total_planned_chapters`, insert one `work_chapters` row per chapter:

| Column | Value |
| --- | --- |
| `work_id` | from grill-me |
| `chapter` | `i` (1..N) |
| `volume` | `NULL` (single-volume) |
| `slug` | user-provided per chapter from grill-me (or auto-derived from `chNN`) |
| `planned_word_count` | default `4000`; user may override per chapter in grill-me |
| `actual_word_count` | `NULL` (set on first transition to `finalized`) |
| `status` | `'not_started'` |
| `outline_path` | `Works/<work_ref>/Outlines/chapters/ch<NN>-outline.md` (with `NN` zero-padded to 2 digits) |
| `body_path` | `Works/<work_ref>/Stories/ch<NN>-<slug>.md` (slug from grill-me; if absent, use `ch<NN>` as the slug) |
| `created_at` | now() |
| `updated_at` | now() |

**Atomicity**: the entire scaffold (mkdir + template copies + `work_chapters` inserts + `works` PATCH) **must succeed or fail together**. If any step fails, the engine rolls back filesystem deletes and DB inserts in a single transaction. The P1 implementer should use a `creator.workspace.transaction` or equivalent capability.

#### 5.4.4 PATCH on `works` table

After scaffold succeeds, the init preset PATCHes the Work record:

| Field | New value |
| --- | --- |
| `work_profile` | `'novel'` (from `null` or a prior value) |
| `work_ref` | the chosen directory name |
| `total_planned_chapters` | from grill-me |
| `world_id` | required from grill-me (existing / new) |
| `current_chapter` | `0` (reset on fresh init) |
| `updated_at` | now() |

If the Work already has a `work_ref` and `Works/<work_ref>/` exists (re-init case), the scaffold **does not overwrite** existing files. The PATCH only updates fields the user explicitly changed in this grill-me session.

#### 5.4.5 Idempotency

Re-running `novel-project-init` on a Work that already has the scaffold is **safe**:

- Files that exist are not overwritten (unless user explicitly opts in via grill-me "re-scaffold from templates" answer).
- `work_chapters` rows are not duplicated (PK conflict on `(work_id, chapter)`; existing rows preserved).
- `works` PATCH is a no-op if all fields are unchanged.

The grill-me offers an "overwrite templates" option for users who want to re-render the README/Outlines/ from latest embedded templates (useful after a toolchain update). The default is **preserve**; "overwrite" is the explicit user opt-in.

### 5.5 Quality loop (SSOT: quality-loop.md)

**Implement authority** for findings lifecycle, review presets (`novel-chapter-review`, `novel-brainstorm`, `novel-review-master`), rules runtime, `Logs/` discipline, 96h escalation, and producer/consumer behavior: **[quality-loop.md](quality-loop.md)**.

Findings lifecycle (6-state) SSOT: [quality-loop.md §2](quality-loop.md#2-findings-lifecycle).

**Profile-local gate (this file)**: §5.1 `llm_judge` 五问 finalize on `novel-writing` remains the per-chapter craft gate.

**Preset gates only** (quality-loop cross-ref):

| Preset | Gates |
| --- | --- |
| `novel-chapter-review` | §5.3.3 |
| Producer/consumer contract | [quality-loop.md §8–§9](quality-loop.md#8-reflection-loop-output-contract) |

Do not duplicate normative findings detail here.

---

## 6. Completion semantics

### 6.1 Completion criteria

A novel Work is **complete** when **all** hold:

1. `works.current_chapter >= works.total_planned_chapters` (from `works` table)
2. Every row in `work_chapters` for `chapter IN 1..total_planned_chapters` has `status == finalized`
3. `works.intake_status == complete`

For `total_planned_chapters > 1`, `works.current_chapter` is set to the just-finalized chapter number on each transition to `finalized` (§4.5.2). Completion therefore means:

```text
current_chapter >= total_planned_chapters
AND all work_chapters rows for the Work are finalized
AND intake_status == complete
→ works.status = completed
→ works.novel_completion_status = completed
→ stop scheduling novel-writing for this Work
```

The single-chapter case (`total_planned_chapters == 1`) is a strict subset of this rule: chapter 1 finalizes, `current_chapter` becomes `1`, all rows are finalized, and the Work completes.

### 6.2 Behavior on completion

1. Set `works.status` → `completed` and `works.novel_completion_status` → `completed`
2. **Stop** enqueueing new `novel-writing` schedules for this Work
3. Emit user-visible message: Work is complete; start a **new** Work via init flow (no automatic switch)

**Note**: Single-chapter completion means `ch01` reached `finalized` after the `llm_judge` GO (§5.1); the same rule extends across all seeded `work_chapters` rows and does not change behavior for a one-chapter Work.

### 6.3 Explicit non-goals

- No automatic creation of next novel Work **without** explicit switch/pool commands (opt-in ceremony — §6.4)
- No platform publish on completion
- No novels-system 8-step / Redis switch (rejected for OSS core)

### 6.4 Multi-work completion extension

[multi-work-lifecycle.md](multi-work-lifecycle.md) extends completion semantics (PR #53):

1. §6.1–§6.2 completion criteria unchanged.
2. Extend `auto_chain::mark_work_completed`: write `Works/<work_ref>/.completion-lock.json`; stop auto-chain on **that** Work only (lifecycle spec §3).
3. Clear pool `active` when bound pool row completes; no automatic next Work.
4. Next Work via `creator works use` or `creator works pool promote` — not implicit scaffold.
5. **Reopen** same Work: `creator works reopen <work_id> --reason "<text>"` (one core operation that releases the completion lock); distinct from `--from-work` new Work.
6. Pool integration: [work-pool.md](work-pool.md). **OUT:** `creator work switch`.

Multi-volume PK: §4.5.4.

---

## 7. Sync contract overlay

Extends [sync-contract.md](sync-contract.md):

- **Scan root**: `Works/<work_ref>/Stories/` only
- **Exclude**: `outline.md` at Stories root (removed); outlines live under `Outlines/`
- **Chapter ordering**: numeric prefix in filename `ch<nn>-*`
- **Idempotency**: content-hash per chapter file unchanged

Sync **must not** upload full正文 by default (cli-spec §5.3 unchanged).

---

## 8. CLI / UX expectations

| Command | Novel profile behavior |
| --- | --- |
| `creator works status [<work_id>]` | Reads `work_chapters`; shows `work_ref`, chapter list, completion; fields + completion/runtime lock per [cli-spec.md](../cli/cli-spec.md) §6.2H |
| `creator works list` | Lists Works |
| `creator works inspire [<work_id>] --note "..."` | Appends inspiration; does not advance chapter index |
| `creator works reconcile-chapters [<work_id>] [--dry-run] [--yes]` | Rebuilds `work_chapters` rows from `Works/<work_ref>/Stories/` filesystem state; per §4.1.2 reconciliation rules (manual command) |

First-run path adds a mandatory World binding step for novel Work creation: create a new World or pick one from `creator world list`. New Works print a one-line `world: <name> (<world_id>)` in the run summary.

### 8.1 Multi-chapter `creator works status` UX

`creator works status [<work_id>]` is the same command for single- and multi-chapter Works (default `work_id` = pool `active`). The output format scales with the number of `work_chapters` rows and remains sourced from the DB SSOT.

Minimum multi-chapter output:

```text
Work: wrk_abc123 — Cozy Mystery (novel)
work_ref: cozy-mystery
intake: complete
progress: 2 / 3 chapters finalized
current_chapter: 2
total_planned_chapters: 3

Chapters:
  ch01  finalized   words: 4,210   path: Works/cozy-mystery/Stories/ch01-arrival.md
  ch02  finalized   words: 3,980   path: Works/cozy-mystery/Stories/ch02-secret.md
  ch03  not_started words: —       path: Works/cozy-mystery/Stories/ch03-reveal.md

Next action: Chapter 3 is not started; the next `novel-writing` preset pass drafts it (preset admission via the execution surface).
```

Draft resume output:

```text
progress: 1 / 3 chapters finalized
current_chapter: 1

Chapters:
  ch01  finalized   words: 4,210
  ch02  draft       words: —
  ch03  not_started words: —

Next action: Chapter 2 is in draft; the next `novel-writing` preset pass resumes it (preset admission via the execution surface).
```

Completion output:

```text
progress: 3 / 3 chapters finalized
current_chapter: 3
status: completed
novel_completion_status: completed

Next action: All chapters finalized; novel Work is complete.
```

Blocked / missing file hint:

```text
warning: Chapter 3 file is missing on disk.
  expected: Works/cozy-mystery/Stories/ch03-reveal.md
  db_status: not_started
  hint: run `creator works reconcile-chapters wrk_abc123` to rebuild from filesystem.
```

Each chapter row must show `not_started | outlined | draft | finalized` and `actual_word_count` when finalized. For a one-chapter Work, the same output shape may collapse to one row, but command semantics are identical.

---

## 9. Acceptance (spec-level)

1. Layout §3.1 is stable; no normative `Stories/<story_ref>/` at workspace root; **no per-Work `Worldbuilding/` subtree** (world content lives in World KB per §3.5).
2. Sync §7 scoped to `Works/<work_ref>/Stories/`.
3. Chapter state SSOT is **`work_chapters` table in `state.db`** (§4.1.1); no `work-status.md` file is created or read.
4. Completion §6 reads from `work_chapters`; documented and testable without publish.
5. `work_profile: novel` fields §2.1 registered in work-experience-model cross-link; `world_id` is the cross-Work binding (§3.5).
6. `novel-project-init` asks the mandatory World binding question (§3.5, §5) with only create-new / bind-existing paths.

---

## 11. Cron staggering and auto-chronology

### 11.1 Per-Work cron configuration

Every Work that opts in to cron-driven staging has a `works.schedule_json` (TEXT) row and an optional `works.auto_chronology` flag. The schedule JSON shape is normative:

```json
{
  "tz": "Asia/Shanghai",
  "roles": {
    "brainstorm": { "cron": "0 3,9,15,21 * * *", "enabled": true },
    "write":      { "cron": "0 4,10,16,22 * * *", "enabled": true },
    "review":     { "cron": "0,30 * * * *",      "enabled": true }
  }
}
```

Defaults (`apps/nexus42/src/commands/creator/works/cron.rs::DEFAULT_SCHEDULE`):

| Role | Cron expression | Local time (author TZ) |
| --- | --- | --- |
| `brainstorm` | `0 3,9,15,21 * * *` | 03:00 / 09:00 / 15:00 / 21:00 |
| `write` | `0 4,10,16,22 * * *` | 04:00 / 10:00 / 16:00 / 22:00 (offset +1h from brainstorm) |
| `review` | `0,30 * * * *` | :00 / :30 every hour |

CLI surface:

```bash
creator works cron set my-work --brainstorm "0 3,9,15,21 * * *" --tz Asia/Shanghai
creator works cron set my-work --no-review          # disable role
creator works cron show my-work                       # render resolved local + UTC firing times
creator works cron list                              # list across workspace (--limit N)
```

Validation rules:

- At least one role must remain `enabled: true` unless `--all-off` is passed (CLI rejects empty schedules; stable code `E_CRON_ALL_ROLES_DISABLED`).
- TZ must be a valid IANA string (use `chrono-tz::Tz::from_str`).
- All cron expressions parse via the `cron` crate (5-field crontab, normalized internally to 6-field with seconds=0).

### 11.2 Daemon firing semantics

The `cron_supervisor` runs on the 1-min tick supervisor loop and:

1. Reads all Works with `works.schedule_json` non-empty (partial index `works(schedule_json) WHERE schedule_json IS NOT NULL AND schedule_json != ''` covers the query; verified via EXPLAIN QUERY PLAN in the `partial_index_used_in_schedule_json_scan` hermetic test).
2. For each role `enabled: true`, evaluates whether the cron fires in the current minute using the `cron` crate's `Schedule::after(now).next()` (memoised per-Work; content-drift invalidation on `set_schedule_json` writes).
3. Skips if `intake_status != "complete"`, `runtime_lock_holder != NULL`, or `completion_locked != false` (log at `DEBUG`).
4. Skips if a prior same-role same-work schedule is still active (idempotency guard; log at `DEBUG`).
5. Enqueues a `Schedule` with `preset_id = <role-preset>` and `work_ref` from the source Work. The `enqueue_cron_schedule` path is uniform across all three roles (no role-specific branch). The `CRON` prefix + `cron:<role>:<work_id>` label preserves provenance vs SLA-launched schedules.

### 11.3 Out-of-band fire (does not touch `driver_schedule_id`)

Cron-fired schedules are out-of-band: they do NOT touch the FL-E `driver_schedule_id`. This mirrors `enqueue_review_master_schedule`. The existing supervisor `tick()` admits; the existing terminal pipeline handles completion. This avoids disrupting an in-progress FL-E chain.

### 11.4 Review-time KB extraction hook (T-B P1 integration)

When `novel-review-master` cron fires, the schedule reaches the supervisor's terminal pipeline. The T-B P1 hook `quality_loop::extract_kb_candidates_for_review` fires on completion (keyed on `preset_id == NOVEL_REVIEW_MASTER_PRESET_ID`, regardless of trigger origin). The hook writes candidate entities to `kb_extract_jobs` with `promotion_status='pending'`, which the author confirms via `creator world kb adopt`.

### 11.5 Auto-chronology (per-Work opt-in)

Per-Work `auto_chronology: bool` flag (default `false`). When `true`, the daemon `auto_chronology_tick` task (5-min interval; env override `NEXUS_AUTO_CHRONOLOGY_INTERVAL_MIN`) checks finish detection on every tick:

Finish detection (per spec §3.1): current volume all chapters `status='finalized'` AND `works.intake_status == 'complete'` AND `works.completion_locked == false` AND `works.runtime_lock_holder == NULL` AND volume N+1 outline does not already exist. If eligible, the advance executes (§11.5.1).

#### 11.5.1 Atomic advance procedure

1. Open a `state.db` transaction.
2. Commit chapter rows for volume N+1 (`INSERT INTO work_chapters (work_id, volume, chapter, status, word_count, world_refs) VALUES (?, ?, ?, 'not_started', 0, '[]')` for each planned chapter).
3. Bump `works.updated_at`.
4. Commit the transaction.
5. Render `Works/<work_ref>/Outlines/volume-<N+1>-outline.md` from `embedded-presets/novel-writing/templates/volume-outline.md.tmpl` (atomic write: temp + fsync + rename).
6. Append to `Works/<work_ref>/Logs/chronology/<YYYY-MM-DD>-advance-vol<N+1>.md`.

**DB first, FS second**: outline write failures after the tx commits are recoverable on the next tick (manual `creator works chronology advance --force --volume N+1` retries the FS write without re-seeding chapters).

If outline write fails BEFORE the tx commits, the entire advance rolls back and the next tick retries cleanly.

#### 11.5.2 Edge cases

- Volume N is the last planned volume → skip with `INFO` log: "auto_chronology: no further volume planned".
- Volume N+1 outline already exists → skip with `INFO` log: "auto_chronology: outline already exists" (idempotent guard).
- `runtime_lock_holder != NULL` → skip with `DEBUG`.
- Daemon interrupted mid-advance → tx rollback; next tick retries.
- No `total_planned_volumes` column exists; the completion-lock gate subsumes the "last planned volume" edge per spec §3.1 (R-V150P3AUTOCHRONO-01).

#### 11.5.3 CLI surface

```bash
creator works chronology set my-work --auto true
creator works chronology set my-work --auto false
creator works chronology show my-work
creator works chronology advance my-work --volume 3   # manual override
creator works chronology advance my-work --volume 3 --force  # retry after outline-write failure
```

### 11.7 真完本 UX (placeholder)

**Status**: Placeholder (body authored when pulled by T-A P2)  

**Promotes to Normative**: at P-last

Placeholder: define the user-visible "true completion" ceremony after all chapters/volumes are finalized, including status copy, no-auto-switch behavior, and next-Work guidance.

### 11.6 Cross-references

- `entity-scope-model.md` §5.5 — World KB promotion state machine (T-B P1 review-time extraction)
- `embedded-presets/novel-writing/prompts/` — preset prompt templates (consumed by auto-chain; novel-write preset authoring deferred to T-A P2 carry-over per R-V150P1CRONBW-01)
- `nexus-orchestration::auto_chronology` — advance implementation
- `nexus-orchestration::cron_supervisor` — fire evaluator
- `nexus-local-db::works.schedule_json` column — storage

---

