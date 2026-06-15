# Novel Workflow Profile — Normative Specification v1

**Status**: Shipped (V1.36 — 2026-06-07); V1.37 extensions; V1.39 shipped — auto-chain + quality loop; **V1.40 Shipped** — World KB implement contract (§3.5.1); **V1.41 Shipped** — multi-work completion + switch cross-refs (§6.4) via [novel-multi-work-lifecycle.md](novel-multi-work-lifecycle.md); **V1.42 Shipped** — multi-volume PK migration (§4.5.4), volume outline scaffold (§4.5.5), migration tests (§4.5.7) via plan [2026-06-11-v1.42-multi-volume](../../plans/2026-06-11-v1.42-multi-volume.md).
**Document class**: Feature line (profile overlay)  
**Created**: 2026-06-07  
**Last updated**: 2026-06-12 (V1.43 P-last — V1.42 Shipped stamps)
**Scope**: `work_profile: novel` on generic **Work** — artifact layout under `Works/<work_ref>/`, templates, chapter status, completion semantics, sync boundaries  
**Coordinates with**:

- [work-experience-model.md](work-experience-model.md) — generic Work entity
- [creator-workflow.md](creator-workflow.md) — FL-E `produce` stage
- [cli-spec.md](cli-spec.md) — workspace layout §13.1
- [novel-writing-sync-contract.md](novel-writing-sync-contract.md) — chapter discovery
- [orchestration-engine.md](orchestration-engine.md) — `novel-writing` preset
- [entity-scope-model.md](entity-scope-model.md) — World entity + World KB (`work_profile: novel` binds Work to World; world content is cross-Work, lives in World KB, NOT in per-Work `Worldbuilding/` subtree)

**Iteration compass**: [v1.37-novel-writing-foundation-delivery-compass-v1.md](../../iterations/v1.37-novel-writing-foundation-delivery-compass-v1.md) extends the shipped V1.36 baseline without changing the single-chapter behavior.

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

### 2.1 `work_profile` field (V1.36 extension)

On `works` table / Work API (additive):

| Field | Type | Required when profile=novel | Description |
| --- | --- | --- | --- |
| `work_profile` | enum | yes | `novel` for this spec; future: `essay`, `script`, … |
| `work_ref` | string | yes | Filesystem directory name: `Works/<work_ref>/` |
| `total_planned_chapters` | integer | no | Target chapter count for completion (default TBD in init preset) |
| `current_chapter` | integer | no | Latest chapter number in progress |
| `world_id` | string (FK) | yes for new V1.40 novel Works | Bind to a World (per [entity-scope-model.md](entity-scope-model.md) §5.4). Required for V1.40 Work creation/init; legacy `NULL` is allowed only when reading V1.39-and-earlier worldless Works. `novel-project-init` grill-me asks whether to create a new World or bind an existing one (see §3.5). |
| `novel_completion_status` | enum | no | `in_progress` \| `completed` (mirrors Work.status when terminal) |

**Invariant**: `work_ref` is stable for the life of the Work; renaming directory without DB update is unsupported pre-1.0.

---

## 3. Artifact layout

### 3.1 Generic Work root

```text
<workspace>/
  Works/
    <work_ref>/
      README.md                 # human overview; includes bound world_id for V1.40 Works; legacy worldless Works may include brief world setting notes
      Outlines/
        volume-outline.md       # optional in V1.36 MVP
        chapters/
          ch<nn>-outline.md
        foreshadowing.md        # empty stub V1.36 (F### rows; future V1.37+ scaffold)
        event-index.md          # empty stub V1.36 (E### rows; future V1.37+ scaffold)
      Stories/                  # novel正文 ONLY — sync chapter scan root
        ch<nn>-<slug>.md
      Logs/                     # optional process logs (single-role V1.36; V1.37+ roadmap structure §5.5.5)
```

### 3.2 Directory rules

| Path | Sync chapter? | Purpose |
| --- | --- | --- |
| `Works/<work_ref>/README.md` | **No** | Human overview; V1.40 Works include bound `world_id`; legacy worldless Works may include world setting notes here |
| `Works/<work_ref>/Outlines/chapters/*` | **No** | Per-chapter outline |
| `Works/<work_ref>/Outlines/foreshadowing.md` | **No** | Cross-chapter foreshadowing index (F### rows) |
| `Works/<work_ref>/Outlines/event-index.md` | **No** | Cross-chapter event index (E### rows) |
| `Works/<work_ref>/Outlines/volume-outline.md` | **No** | Volume-level outline (optional V1.36) |
| `Works/<work_ref>/Stories/*.md` | **Yes** | Chapter正文 (frontmatter `chapter`, `status`) |
| `Works/<work_ref>/Logs/**` | **No** | Brainstorm/write/review/publish process logs (V1.37+ roadmap §5.5.5); excluded from chapter sync |

### 3.3 Legacy prohibition

The following are **removed from normative specs** (pre-1.0):

- Workspace-root `Stories/<story_ref>/`
- Prompt variables defaulting to `Stories/{{preset.input.story_ref}}/…` without `Works/<work_ref>/` prefix
- Sync scanners treating any `Works/<work_ref>/*.md` as chapters

Implementations **must not** provide dual-path fallbacks.

### 3.4 Other creative profiles (future)

Non-novel `work_profile` values may use different subtrees under `Works/<work_ref>/` (e.g. `Drafts/`, `Sections/`). They **must not** reuse `Stories/` unless the profile spec says so.

### 3.5 World integration (cross-Work worldbuilding)

**Key principle**: worldbuilding content is **cross-Work**, not per-Work. A `Works/<work_ref>/` directory is **one event in a World's timeline**, not the canonical home of characters, locations, society, or rules. The canonical home is **World KB** (per [entity-scope-model.md](entity-scope-model.md) §5.4 World entity + `nexus-kb` crate).

Therefore:

- **No per-Work `Worldbuilding/` subtree** in V1.36. The reference-system pattern of `{作品目录}/世界设定/` is intentionally rejected for OSS core; world content is the World's job, not the Work's.
- **`world_id` is the mandatory binding for new V1.40 novel Works**. New Works must be **World-bound** (`work.world_id` set) at creation/init time.
  - **World-bound Work** (`world_id != NULL`): characters, locations, society, rules, events, timelines come from World KB. Chapter body may reference World KB items by id via `world_refs: [char_xxx, loc_yyy]` frontmatter; V1.40 validates per §3.5.1.4.
  - **Legacy worldless Work** (`world_id == NULL`, V1.39 and earlier only): no cross-Work continuity. Existing data remains readable/operable; V1.40 creation/init must not produce this state.
- **`novel-project-init` asks the binding question** (grill-me). Two valid V1.40 options: bind to existing `world_id` (user picks from `nexus42 creator world list`) / create new World (calls `creator world create --title "..."`, narrative kind implicit). There is no V1.40 "stay worldless" creation option.
- **Work → World KB promotion** is the **long-term** path: as chapters finalize, `kb-extract` preset (existing, per [creator-workflow.md](creator-workflow.md) `persist` stage) can extract entities / events / rules from chapter body into World KB items. V1.36 documents this path; enforcement is V1.37+.

### 3.5.1 World KB continuity implement contract (V1.37 P2 roadmap → V1.40 implement)

**Scope of this extension**: V1.37 P2 locked the roadmap; **V1.40** implements it across plans P0–P3 ([v1.40-novel-world-kb-delivery-compass-v1.md](../../iterations/v1.40-novel-world-kb-delivery-compass-v1.md)). The `novel-writing` `world_binding` mode is `required` for V1.40 new Work creation/init. Legacy worldless Works from V1.39 and earlier continue to read/operate without a World context block, but no V1.40 init path may create a worldless Work. Implement slices: P0 world create + validation; P1 taxonomy; P2 prompt context block; P3 kb-extract binding.

#### 3.5.1.1 Mandatory World binding paths for `novel-project-init`

**V1.40 implement** — CLI contract:

```text
nexus42 creator world create --title "Neon River" --description "Solarpunk noir city-world"
→ world_id: wld_<uuid>
```

Note: `--kind narrative` is implicit (deferred to P1 taxonomy). `--title` is canonical; `--name` is an alias (see [cli-spec.md §6.2G](cli-spec.md)).

The init grill-me has exactly two valid V1.40 binding paths:

1. **Create new World** — call `nexus42 creator world create --title "..."` and bind the returned `world_id` to the Work.
2. **Bind existing World** — pick a visible World from `nexus42 creator world list` and pass/bind its `world_id`.

Both paths compose with P0 `AddScheduleRequest.input` wiring as follows:

1. `novel-project-init` records the user's choice as either `preset.input.create_world = true` plus `world.title` / optional `world.description`, or as `preset.input.world_id = wld_<uuid>` from the existing-World list.
2. For create-new, the daemon invokes a `world create` capability owned by `nexus-narrative`/`nexus-kb`, equivalent to the CLI contract above; for bind-existing, it validates the selected `world_id` against the same store.
3. The resulting `world_id` is bound to the Work and PATCHed via the same atomic scaffold transaction as `work_ref`, `total_planned_chapters`, and `work_chapters` seeding (§5.4.3–§5.4.4).
4. If world creation or existing-World validation fails, the scaffold transaction fails closed: no partial `Works/<work_ref>/` tree, no duplicated `work_chapters`, and no `works.world_id` mutation.

**V1.40 P0** ships this path; until then behavior remains stubbed.

#### 3.5.1.2 `world_id` validation

When a novel Work is created/init-scaffolded in V1.40, `world_id` MUST reference an existing World visible under the active `creator_id` + `workspace_slug` context. Missing or omitted `world_id` values return a structured `preset_gates_failed`-style error with remediation. Legacy V1.39-and-earlier Works with `world_id == NULL` remain readable, but that legacy state is not creatable by V1.40 init paths.

```text
error: preset_gates_failed
  preset: novel-project-init
  failed_gates:
    - work_field: world_id must reference an existing World (actual: missing or wld_missing)
        ↳ Create the World first via `nexus42 creator world create --title "..."` or pick an existing one with `nexus42 creator world list`.
```

The P0 gate evaluator already supports the `world_id` gate for `novel-writing`; the preset-level toggle is `world_binding: required` for V1.40 (§5.3.4). Implementations may keep the `optional` mode internally for back-compat/legacy reads, but new `novel-project-init` creation must fail closed without a valid `world_id`.

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

`characters_in_chapter` and `locations_referenced` are selected from `world_refs` when available, then from outline/body heuristics if needed. `active_rules` includes high-priority `foundation` and `rules` category items that constrain the scene. For legacy worldless Works (`world_id == NULL`, V1.39 and earlier), the block is omitted and prompts use only `Works/<work_ref>/README.md` setting notes, preserving read-time compatibility.

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

For V1.40 World-bound Works, `world_refs` is required by the validation contract: it may be empty only when the chapter truly references no World KB items, and any non-empty entry must validate under `work.world_id`. For legacy worldless Works (`world_id == NULL`, V1.39 and earlier), `world_refs` is allowed but unused; if present, implementations should warn but not fail.

#### 3.5.1.5 Chapter → World KB extraction and promotion

The `creator-workflow.md` `persist` stage already maps Work → World KB to `creator kb queue-extract` + `kb-extract`. For novel Works, the extraction target is the Work's `world_id` when set:

- **World-bound Work**: `kb-extract` reads finalized chapter body + outline/event/foreshadowing indexes, extracts entities, events, rules, locations, and relationships, then creates or updates World KB items under `work.world_id` with SourceAnchors back to the chapter path and, where available, the timeline event.
- **Legacy worldless Work** (V1.39 and earlier): extraction is skipped or remains local Work scope; it MUST NOT silently create a new World or promote content into an arbitrary World.
- **Explicit promotion**: rows in `Outlines/event-index.md` and `Outlines/foreshadowing.md` may be promoted to World KB items only when the Work is World-bound and the agent/user marks the promotion explicitly (e.g. "promote E012 as background" or "promote F007 as rule").

**V1.40 acceptance** (per plan P0–P3): hermetic tests for valid/invalid/missing `world_id`, prompt block presence for new World-bound Works, legacy worldless read compatibility, `world_refs` warning/error timing, and `kb-extract` target selection. On ship, close DF-63 in [deferred-features-cross-version-tracker.md](../deferred-features-cross-version-tracker.md).

**Anti-patterns** explicitly rejected:

- Per-Work `Worldbuilding/character.md`, `Worldbuilding/location.md` etc. — content is duplicated across Works; defeats the cross-Work reuse.
- Hard-coded character name references in `novel-writing` prompts without KB lookup — produces drift from World state.
- Inheriting the reference-system `{作品目录}/世界设定/` subtree shape into OSS — wrong layer (per-Work is the wrong layer for world content).

See DF-63 in [deferred-features-cross-version-tracker.md](../deferred-features-cross-version-tracker.md) for the full cross-Work roadmap (World KB extraction path, KB item schema, World KB ↔ Work binding protocol).

---

## 4. Templates and frontmatter

### 4.1 `work_chapters` table (chapter state SSOT) and `README.md` (human overview)

V1.36 chapter state lives in a **new DB table** `work_chapters` in `state.db` (owned by `nexus-local-db`). The legacy `Works/<work_ref>/work-status.md` file is **removed** — chapter state is no longer author-edited as markdown; the orchestration engine + `creator run` commands own the truth.

#### 4.1.1 `work_chapters` table DDL (V1.36)

```sql
CREATE TABLE work_chapters (
  work_id              TEXT NOT NULL,
  chapter              INTEGER NOT NULL,        -- 章节号; 1..total_planned_chapters
  volume               INTEGER,                  -- nullable; V1.36 single-chapter MVP leaves NULL; V1.37+ multi-volume uses 1..N
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

**V1.37 P1 roadmap decision**: V1.37 does **not** migrate this primary key. The PK stays `(work_id, chapter)` for V1.37 single-volume Works; `volume` remains nullable and defaults to `NULL` for V1.36/V1.37 single-volume rows. A future V1.37+ multi-volume implementation may drop this PK and add a unique index on `(work_id, volume, chapter)` after it ships volume-aware migrations and backfill rules (§4.5.4).

#### 4.1.2 Truth model (DB vs frontmatter)

- **`work_chapters` is the queryable SSOT** for `creator run status`, completion evaluation (§6), and sync module (if it needs per-chapter metadata).
- **Chapter .md frontmatter `status` is the human/LLM read-end**: the orchestration engine updates **both** on transition; the frontmatter flip is the visible "I'm now finalized" signal to the next prompt.
- **Reconciliation**: on daemon startup, an optional `creator run reconcile-chapters <work_id>` walks `Works/<work_ref>/Stories/` and rebuilds `work_chapters` rows from filesystem for missing rows/files. For status disagreements, V1.37 narrows the rule: the DB row is the source of truth and the next run re-syncs chapter frontmatter via a single transition (§4.5.3). For V1.36 this is a manual command, not an automatic job.

#### 4.1.3 `README.md` (human overview, no chapter state)

`Works/<work_ref>/README.md` is the only human file. It is **author-edited** and may contain:

- Working title, premise, blurb
- For **legacy worldless** Works (V1.39 and earlier): a brief world setting note (1–2 paragraphs) so the LLM has context
- For **World-bound** Works (§3.5): a one-liner like `world_id: <uuid>` and links to World KB items the Work uses most
- Optional F### / E### anchors to `Outlines/foreshadowing.md` / `Outlines/event-index.md`

`README.md` is **not** scanned by the sync module (§3.2).

### 4.2 Chapter outline (`Outlines/chapters/ch<nn>-outline.md`)

Structured outline before正文. **Required headings**: opening scene, core conflict, turning point, climax, ending hook, character state change. **Foreshadowing is required**: the outline must list every foreshadowing item touched (buried or paid-off) in this chapter, referencing `Outlines/foreshadowing.md` F### ids. If the foreshadowing file does not yet exist for a V1.36 single-chapter MVP, the outline body may declare new F### items inline and the next outline is responsible for promoting them to the index.

### 4.3 Chapter body (`Stories/ch<nn>-<slug>.md`)

```yaml
---
title: string
chapter: integer
volume: integer             # optional; V1.36 single-chapter MVP leaves it empty. Reserved for V1.37+ multi-volume.
status: draft | finalized   # published reserved; not set by OSS core in V1.36; mirror of work_chapters.status
word_count: integer         # optional; auto-derived from body length on transition to finalized; mirror of work_chapters.actual_word_count
world_refs: [string]        # optional; list of World KB item ids referenced in this chapter (V1.36 advisory, not validated)
---
```

`volume` is **forward-compatible** for V1.37+ multi-volume expansion. When present, the chapter must also appear in `Outlines/volume-outline.md` chapter range. Sync module does **not** validate cross-reference in V1.36 (intentional — V1.36 single-volume). `world_refs` is a soft hint; for **World-bound** Works (§3.5) it should list World KB item ids (e.g. `char_lin_xia`, `loc_neon_city`). V1.36 does not enforce; V1.37+ may validate. Body text only (title in frontmatter). Status transitions:

| Transition | Actor |
| --- | --- |
| → `draft` | `novel-writing` drafting phase |
| → `finalized` | `novel-writing` finalize phase **only after** `llm_judge` quality gate returns GO (see §5) |
| → `finalized` (manual override) | user explicit advance on NOGO with logged audit reason; V1.36 escape hatch only |

### 4.4 Embedded preset templates

V1.36 implement wave ships template stubs under `crates/nexus-orchestration/embedded-presets/novel-writing/templates/` (or documented equivalent). Prompts reference `Works/{{work_ref}}/…` variables.

### 4.5 Multi-chapter and multi-volume semantics (V1.37 extension)

**Scope of this extension**: V1.37 P1 is **roadmap-only**. It locks the semantics future implementers must follow, but it does not claim a code, schema, preset, or migration implementation in this plan. V1.36 single-chapter behavior is a strict subset: when `total_planned_chapters == 1`, chapter 1 is selected, drafted, finalized, and completes the Work exactly as in the shipped V1.36 MVP.

#### 4.5.1 Chapter state machine

The V1.37 multi-chapter roadmap keeps the V1.36 chapter status state machine:

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

`work_chapters` is the queryable source of truth for chapter status. If a `work_chapters` row and filesystem frontmatter in `Stories/ch<NN>-<slug>.md` disagree on `status`, the DB row wins. The next `novel-writing` or `creator run reconcile-chapters <work_id>` pass must re-sync frontmatter through a single status transition so that prompt-visible state catches up without inventing an extra chapter transition.

Missing filesystem hints are still surfaced to the user (see §8.1), but a missing or stale file does not authorize selecting a later DB row. Rebuilds must preserve the DB ordering and status semantics above.

#### 4.5.4 Primary key and volume migration decision

**V1.37–V1.41 (shipped):** PK `(work_id, chapter)`; `volume` nullable; single-volume Works use unique chapter numbers across the Work.

**V1.42 P1 (implement — grill-me locked):**

1. Backfill **implicit `volume = 1`** for all existing `work_chapters` rows (single-volume behavior unchanged).
2. Migrate PK to **`(work_id, volume, chapter)`** (see [local-db-schema.md](local-db-schema.md) V1.42 amendment).
3. Preserve row data (`status`, `outline_path`, `body_path`, `actual_word_count`, timestamps) through an idempotent migration.
4. New multi-volume Works declare volume count at init; chapter numbers may repeat across volumes.

Plan: [2026-06-11-v1.42-multi-volume.md](../../plans/2026-06-11-v1.42-multi-volume.md).

#### 4.5.5 Volume outline semantics

**V1.42 P1 (Implemented):** Volume-outline scaffold delivered. `novel-project-init` grill-me captures volume count + per-volume chapter totals and seeds `Works/<work_ref>/Outlines/volume-{n}-outline.md` per the YAML structure below. Cross-volume chapter numbers may repeat across volumes (per §4.5.4 PK migration).

When `volume != NULL`, the chapter must appear in `Outlines/volume-outline.md` under that volume's chapter range. The minimum V1.37+ multi-volume outline structure is:

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

The sync module (per [novel-writing-sync-contract.md](novel-writing-sync-contract.md)) does **not** validate `volume-outline.md` cross-references in V1.37. A post-roadmap V1.37+ implementation may enforce that every non-NULL `work_chapters.volume` row falls inside exactly one declared volume range.

World continuity remains World-scoped: when `world_id != NULL` and a chapter references a World KB item, the reference lives in the chapter frontmatter `world_refs` field (§4.3), not in `Outlines/volume-outline.md`. `volume-outline.md` organizes chronology; it is not a World KB index.

#### 4.5.6 Prompt and preset parameterization roadmap

The V1.36 `novel-writing` preset and templates may still bind or imply `chapter: 1` in places. Future implementation must replace hard-coded chapter-1 bindings with a scheduler/gate-evaluator injected value such as `{{work_chapters.next_chapter}}` (or an equivalent preset input variable). The value must be derived from `next_chapter(work_id)` above after the P0 gates have passed (`intake_status == complete`, scaffold exists, `previous_preset: novel-project-init` complete).

This keeps `novel-writing` a single preset that scales from chapter 1 to chapter N rather than creating separate per-chapter presets.

#### 4.5.7 Future acceptance and migration tests

**V1.42 P1 (Implemented subset):** Test #6 (Future multi-volume migration: the `(work_id, chapter)` → `(work_id, volume, chapter)` migration is idempotent and preserves row data) is implemented. Verified by `w01_v142_migration_idempotent` test in `crates/nexus-local-db/tests/v142_migration_fixes.rs`. Index coverage for `next_chapter_volume_aware` query is also verified by `w02_volume_aware_index_coverage` test. Other tests (#1–#5) remain future per the existing roadmap below.

A future implementation plan for this roadmap must include at least these tests:

1. **Chapter selection**: a 3-chapter Work with rows at varied statuses; assert `next_chapter(work_id)` returns the lowest eligible row per §4.5.2.
2. **`current_chapter` transitions**: `current_chapter` changes only when a row transitions to `finalized`, and it becomes the just-finalized chapter number.
3. **Novel completion**: completion fires only when every row is `finalized`, `current_chapter >= total_planned_chapters`, and `intake_status == complete` (§6.1).
4. **Resume behavior**: a new run against a Work with one `draft` row resumes that row and does not create a new row.
5. **Reconciliation**: `creator run reconcile-chapters <work_id>` rebuilds missing `work_chapters` rows/files from `Works/<work_ref>/Stories/` while preserving DB-as-status-SSOT conflict resolution (§4.5.3).
6. **Future multi-volume migration**: the `(work_id, chapter)` → `(work_id, volume, chapter)` migration is idempotent and preserves row data.

---

## 5. Preset responsibilities

| Preset | Role | When | Gates (see §5.3) |
| --- | --- | --- | --- |
| `novel-project-init` | Interactive grill-me; sets `work_ref`, `total_planned_chapters`, required `world_id` (bind to existing / create new), scaffolds `Works/<work_ref>/` dirs (§5.4), seeds `work_chapters` rows | Before first `novel-writing` if scaffold missing | §5.3.1 |
| `creative-brief-intake` | Structured brief on Work | FL-E `intake` / `creator bootstrap` | (generic; out of novel overlay) |
| `novel-writing` | Outline → draft → **finalize (gated by `llm_judge`)** → `finalized`; per-chapter transitions update both `work_chapters` row + chapter frontmatter | FL-E `produce` | §5.3.2 |
| `reflection-loop` | Optional deeper quality pass; **not** in V1.36 default flow | FL-E `review` (optional V1.36) | §5.3.3 |

**Separation rule**: `novel-project-init` is **not** auto-chained inside `novel-writing`. User or `creator run` explicitly schedules it when starting a new novel Work. The engine enforces this via the `previous_preset: novel-project-init` gate on `novel-writing` (§5.3.2).

### 5.1 V1.36 chapter finalize quality gate

`novel-writing`'s `finalize` state has `exit_when: kind: llm_judge` evaluating a **五问质量检验** prompt (opening three lines / conflict resonance / twist recall / new perspective / ending hook). This prevents the "click and finalize" demo feel where any draft becomes `finalized` without scrutiny.

- **GO** → both `work_chapters.status` AND chapter frontmatter `status` flip to `finalized`; `work_chapters.actual_word_count` is updated from frontmatter `word_count`; `current_chapter` advances.
- **NOGO** → `WaitForInput`; user may `creator run continue <work_id> --note "..."` with additional context, then re-run. `work_chapters.status` and frontmatter `status` both stay `draft`.
- **GO override on NOGO** → user explicit `creator run novel-writing <work_id> --force-gates --reason "<text>"` with audit-logged reason; both rows flip to `finalized` regardless.

The 五问 template file lives at `embedded-presets/novel-writing/prompts/finalize-exit.md` (P3 deliverable). It references [writing-craft-rules.md §2 五问质量检验](writing-craft-rules.md) when present; otherwise the template embeds the five questions inline.

### 5.2 World-bound Work behavior (in `novel-writing` prompts)

When a Work has `world_id != NULL`:

- Before drafting each chapter, the orchestration engine injects a **World context block** into the prompt: character names + 1-line descriptors, key locations, current timeline position. Sourced from World KB via `creator kb query world <world_id>` (or equivalent capability).
- The LLM is asked to **name characters / locations exactly as in World KB** unless the chapter is introducing a new one (new ones go to `Outlines/event-index.md` for later `kb-extract` promotion).
- `world_refs: [char_xxx, loc_yyy]` frontmatter is filled by the agent based on what the chapter references. V1.36 does not validate; V1.37+ may validate.

For **legacy worldless** Works (`world_id == NULL`, V1.39 and earlier): no World context block; LLM uses `README.md` setting notes as the only world context.

### 5.3 V1.36 novel preset gates (Draft overlay on orchestration-engine.md §7.9)

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

#### 5.3.3 `reflection-loop` gates (optional quality pass)

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

**Rationale**: `reflection-loop` is optional and runs after at least one chapter draft. It needs the chapter directory but does not require the chapter to be `finalized` (reflection may be triggered on `draft` too).

#### 5.3.4 World-binding toggle (preset-level opt-in)

The `world_id` gate in §5.3.2 is conditional. The preset manifest can declare:

```yaml
preset:
  id: novel-writing
  # ...
  world_binding:
    mode: required                   # required | optional
```

- **`required`** (V1.40 default for `novel-writing`): the `world_id` gate is active; new V1.40 Works must have `world_id` set before scheduling. Legacy worldless Works (`world_id == NULL`, V1.39 and earlier) may continue to run only through explicit back-compat handling or audited override paths; V1.40 creation/init must not create them.
- **`optional`**: retained only as an internal/back-compat mode for legacy reads or older manifests; new V1.40 `novel-project-init` behavior treats it as a no-op and still requires a valid `world_id` at creation.

Recommendation: keep the toggle in manifests for compatibility with older presets, but default and enforce `required` for V1.40 novel Work creation/init.

#### 5.3.5 Gate failure user experience

When a `novel-writing` gate fails (e.g. user runs `creator bootstrap --idea "..."` without first running `novel-project-init`), the CLI surfaces:

```text
error: preset_gates_failed
  preset: novel-writing
  work_id: wrk_abc123
  failed_gates:
    - filesystem: Works/cozy-mystery/ must exist (actual: missing)
        ↳ Run `creator bootstrap --init-preset novel-project-init` to scaffold the Work.
    - work_field: intake_status must equal "complete" (actual: pending)
        ↳ Complete intake via `creator bootstrap --preset creative-brief-intake`.
  override: pass --force-gates with --reason "<text>" to bypass (audit-logged)
```

This is the **user-visible demo-pain killer**: instead of scheduling `novel-writing` and failing deep in the state graph, the engine rejects at enqueue with a clear remediation.

#### 5.3.6 Implementation note (V1.36)

- Gate evaluation is **read-only** at enqueue time; the engine queries `works` table and filesystem; it does not mutate.
- Gate evaluation is **idempotent**: a failed check leaves no side effects.
- Engine logs gate failures to the structured log with `preset_id`, `work_id`, `failed_gates` array. Failed-gate rate by preset is a future observability dashboard (out of V1.36 scope).

### 5.4 `novel-project-init` scaffold protocol (file enumeration)

`novel-project-init` is the canonical way to bootstrap a Work's `Works/<work_ref>/` tree. The grill-me collects `work_ref`, `total_planned_chapters`, and the mandatory World binding choice (§3.5); on success, the preset's **scaffold capability** (or handler) creates the full directory tree, copies template stubs, and seeds `work_chapters` rows. This section enumerates every file/dir so the P1 implementer (and any hand-rolled init script) has a checklist.

#### 5.4.1 Directory tree created (all paths relative to workspace root)

```text
Works/
  <work_ref>/
    README.md                              # copy from embedded template; render with {{work_ref}}, {{title}}, {{world_id_or_null}}
    Outlines/
      volume-outline.md                    # copy from embedded template; V1.36 single-volume: still created (placeholder)
      chapters/                            # mkdir (empty; first outline created on first outline pass)
      foreshadowing.md                     # copy from embedded template (F### table header; §3.1/§3.2)
      event-index.md                       # copy from embedded template (E### table header; §3.1/§3.2)
    Stories/                               # mkdir (empty; first chapter body created on first draft pass)
    Logs/                                  # mkdir (empty; structure OUT V1.36 per DF-66)
```

**Not created** (intentional):

- `work-status.md` — replaced by `work_chapters` table (§4.1).
- `Worldbuilding/` subtree — content lives in World KB (§3.5).

#### 5.4.2 Template sources

All template files live under `crates/nexus-orchestration/embedded-presets/novel-project-init/templates/` (P1 deliverable). The init preset's scaffold capability:

1. Reads each template from the embedded asset.
2. Substitutes preset input vars (`work_ref`, `title`, `world_id`, etc.) using `handlebars-rust` (per [orchestration-engine.md](orchestration-engine.md) §7.3).
3. Writes to `Works/<work_ref>/...` at the path listed above.

For V1.40 Works, `README.md` is rendered with a one-line `world_id: <uuid>` header and links to the World KB items the Work will reference most. Legacy worldless Works from V1.39 and earlier may retain README inline world setting notes, but V1.40 scaffold rendering does not create new worldless README variants.

#### 5.4.3 `work_chapters` row seeding (DB writes)

For `chapter IN 1..total_planned_chapters`, insert one `work_chapters` row per chapter:

| Column | Value |
| --- | --- |
| `work_id` | from grill-me / `creator bootstrap` |
| `chapter` | `i` (1..N) |
| `volume` | `NULL` (V1.36 single-volume) |
| `slug` | user-provided per chapter from grill-me (or auto-derived from `chNN` for V1.36 MVP) |
| `planned_word_count` | default `4000` (single V1.36 value; user may override per chapter in grill-me) |
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
| `work_profile` | `'novel'` (was `null` or previously set) |
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

The grill-me offers an "overwrite templates" option for users who want to re-render the README/Outlines/ from latest embedded templates (useful after a toolchain update). V1.36 default is **preserve**; "overwrite" is the explicit user opt-in.

### 5.5 Quality loop roadmap (V1.37 P3 extension)

**Scope of this extension**: V1.37 P3 is **roadmap-only**. It records future contracts for findings, target executor mapping, master-decision escalation, rules, logs, and `reflection-loop` integration. It does **not** add a `findings` migration, new presets, daemon scheduled task, CLI subcommands, prompt templates, or file writers in V1.37 P3. The V1.36 `novel-writing` path and its `llm_judge` 五问 finalize gate (§5.1) remain the active quality gate.

#### 5.5.1 Findings lifecycle and local DB sketch

Future quality-loop implementation should store review outcomes in local `state.db`, not Redis. The initial lifecycle is intentionally small:

```text
open → resolved | wont_fix
```

The richer workflow `open → triaged → in_review → resolved | wont_fix | duplicate` remains a possible later extension, but V1.37 P3 selects the three-state model to keep the first migration and CLI surface narrow.

Finding severities are author-facing and map to Morning Star machine residual severities as follows:

| Finding severity | Meaning | `status.json` residual severity mapping |
| --- | --- | --- |
| `info` | Context, note, or non-actionable observation | `low` (or omit from residual tracking if no follow-up is needed) |
| `minor` | Small craft/continuity issue; not blocking drafting | `medium` |
| `major` | High-impact narrative, continuity, or user-visible quality problem | `high` |
| `blocker` | Must resolve or explicitly waive before approval/publish | `critical` |

Schema sketch for a future `nexus-local-db` migration:

```sql
CREATE TABLE findings (
  finding_id TEXT PRIMARY KEY,
  work_id TEXT,
  chapter INTEGER,
  kind TEXT NOT NULL,            -- e.g. 'continuity', 'craft', 'plot_hole', 'world_inconsistency'
  severity TEXT NOT NULL,        -- 'info' | 'minor' | 'major' | 'blocker'
  status TEXT NOT NULL,          -- 'open' | 'resolved' | 'wont_fix'
  title TEXT NOT NULL,
  body TEXT NOT NULL,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL,
  FOREIGN KEY (work_id) REFERENCES works(work_id) ON DELETE CASCADE
);
CREATE INDEX findings_by_work ON findings(work_id, status);
CREATE INDEX findings_by_chapter ON findings(work_id, chapter, status);
```

`kind` is open vocabulary at the first roadmap level. Suggested minimum values are `continuity`, `craft`, `plot_hole`, and `world_inconsistency`. World-related findings reference World KB ids through the body or a future structured column, but the canonical world content remains World KB (§3.5; entity-scope-model.md §5.1).

#### 5.5.2 Target executor mapping to Nexus presets and CLI surfaces

The reference-system executor concepts map to Nexus presets and surfaces as follows:

| Reference executor concept | Nexus preset / stage | CLI status surface | V1.37 P3 disposition |
| --- | --- | --- | --- |
| `write` | Existing `novel-writing` (`produce` stage) | `creator run status <work_id>` chapter progress and active schedule summary | Existing path remains; findings may later enrich prompt context, but no behavior change in P3 |
| `brainstorm` | `novel-brainstorm` preset (Shipped V1.39) | `creator run novel-brainstorm <work_id>`; status shows open findings driving brainstorm prompts | Shipped V1.39 |
| `none` | No preset; user or reviewer acknowledges, resolves, or marks `wont_fix` | `creator run status <work_id>` lists the finding with `next_action: none` / manual decision | Shipped V1.39 |
| `master` | `novel-review-master` preset (Shipped V1.39); CLI `creator run review-master <work_id>` (Shipped V1.44) | `creator run review-master <work_id>`; status shows master-decision banner and open findings requiring approval | Shipped V1.39 (preset) / V1.44 (CLI) |

`novel-brainstorm` should turn open findings into ideation prompts without writing chapter正文 directly. `novel-review-master` should surface findings, relevant rules, and proposed actions to the user for final approval. Neither preset replaces `novel-writing`; they are auxiliary quality-loop surfaces layered around the single-role V1.36 path.

#### 5.5.3 Master-decision timeout (96h) in local-first OSS terms

The reference-system pattern escalates a finding if it remains `open` for 96 hours. Nexus OSS maps this to local persistence and daemon lifecycle instead of Redis, cron, or platform workers:

1. A `nexus-daemon-runtime` lifecycle/scheduler task runs every 24 hours while the daemon is healthy.
2. It queries `findings` for rows where `status = 'open'` and `created_at < now - 96h`.
3. It emits a structured log entry with `work_id`, `chapter`, `finding_id`, `age_hours`, and suggested command.
4. `creator run status <work_id>` shows a banner such as:

   ```text
   Findings: 2 open, 1 older than 96h
   Next action: run `nexus42 creator run review-master <work_id>` to make a master decision.
   ```

5. Automatic escalation is **user opt-in** through `--auto-schedule` CLI flag or Work-level `auto_review_master_on_timeout` setting. By default, the daemon performs no auto-action; the user explicitly runs `creator run review-master <work_id>`.

**Shipped V1.39** (daemon watcher + preset); **Shipped V1.44** (CLI `review-master` subcommand).

This keeps the OSS path local-first: local DB + daemon scheduled task + CLI status banner. It explicitly does not reintroduce Redis, external cron, platform queues, or platform workers.

#### 5.5.4 Three-layer rules architecture

Future quality-loop work uses three rules layers:

| Layer | Location | Mutability | Purpose |
| --- | --- | --- | --- |
| Layer 1 — Shared writing craft rules | User override: `~/.nexus42/rules/writing-craft.md`; in-repo default: `crates/nexus-orchestration/embedded-rules/writing-craft.md` | Immutable per version; user override may pin/replace a version | Cross-Work craft guidance read by all `novel-writing` runs; includes prose quality, pacing, scene craft, and the 五问 gate rationale |
| Layer 2 — Per-work novel rules | `Works/<work_ref>/Rules/novel-rules.md` | User-editable; reset/replaced by future `creator run rules reset <work_id>` | Per-Work style preferences: POV, tense, chapter length, allowed tone, banned motifs, target audience, stylistic constraints |
| Layer 3 — Append-only rules history | `Works/<work_ref>/Rules/novel-rules-history.md` | Append-only; never deleted | Audit trail for every Layer 2 change with timestamp, reason, actor/source, and previous/new summary |

Rules responsibilities are distinct from adjacent knowledge surfaces:

- **SOUL**: creator identity, voice, experience, and memory. SOUL is not a rule file and should not receive per-Work style churn.
- **World KB**: characters, locations, society, magic/physics/technology rules, timeline facts, and source anchors (§3.5; entity-scope-model.md §5.1). World KB describes fictional reality, not prose craft instructions.
- **Shared writing craft rules**: how to write prose across Works (scene quality, pacing, clarity, consistency, 五问).
- **Per-work novel rules**: how this Work should be written (POV, tense, chapter size, style preferences).

If no `writing-craft-rules.md` exists, `novel-writing` continues to embed the 五问 prompt inline (§5.1). P3 found no existing `writing-craft-rules.md`; creating the embedded/user rule files is future implementation.

#### 5.5.5 `Logs/` structure and write discipline

V1.36 creates `Works/<work_ref>/Logs/` as an empty optional root (§5.4.1). V1.37+ quality-loop work may add the following subdirectories:

```text
Works/<work_ref>/Logs/
  brainstorm/    # brainstorm session outputs: chats, sketches, alternatives
  write/         # chapter drafting process logs and prompt/response summaries
  review/        # review outputs, findings notes, master-decision context
  publish/       # future publish process notes; out until platform publish ships
```

Write discipline:

1. Logs are process evidence, not canonical chapter正文, not World KB, and not SOUL.
2. Logs may be summarized into findings, rules, or KB items, but the summary/promotion must be explicit.
3. `Logs/publish/` remains reserved until platform publish (DF-59) ships.
4. Per §3.2 and §7, `Works/<work_ref>/Logs/**` is **not** scanned by the chapter sync module. Chapter sync remains scoped to `Works/<work_ref>/Stories/*.md` only.

#### 5.5.6 `reflection-loop` feeding findings and rules updates

`reflection-loop` is already the FL-E `review` stage preset and remains optional in V1.36 (§5.3.3; creator-workflow.md §3.1 and §4). Future integration should work as follows:

1. The user runs `reflection-loop` on a draft or finalized chapter after `novel-writing` has produced content.
2. The preset inspects chapter body, outline context, `llm_judge` output, relevant World KB context for World-bound Works, and active rules layers.
3. It writes one or more rows to `findings` with `kind = 'craft'`, `kind = 'continuity'`, or another supported kind.
4. `creator run status <work_id>` surfaces a **Findings** section summarizing open findings by severity and chapter.
5. The user can resolve, mark `wont_fix`, or flag a finding as a **rule suggestion**.
6. If accepted as a rule suggestion, the daemon updates `Works/<work_ref>/Rules/novel-rules.md` and appends an audit entry to `novel-rules-history.md` with timestamp + reason.

This integration depends on the `findings` table and rules files existing, so it is future implementation scope.

#### 5.5.7 V1.37 P3 scope decision (superseded for implement by V1.39)

V1.37 P3 was roadmap-only. **V1.39** reopens implementation via [novel-quality-loop.md](novel-quality-loop.md) and plans P1–P4. The five coupled work items are now in scope:

1. `findings` table migration and DAO/API surface (`nexus-local-db`, daemon handlers, CLI status rendering).
2. Net-new `novel-brainstorm` and `novel-review-master` presets plus prompt templates.
3. 96h master-decision daemon scheduled task and opt-in setting.
4. Rules file readers/writers plus append-only history discipline.
5. `Logs/` subdirectory write discipline and `reflection-loop` integration that depends on findings.

Implement authority: V1.39 compass [v1.39-novel-auto-chain-and-quality-loop-delivery-compass-v1.md](../../iterations/v1.39-novel-auto-chain-and-quality-loop-delivery-compass-v1.md). Must preserve local-first path; no Redis, external cron, platform workers, or platform publish.

---

## 6. Completion semantics

### 6.1 Completion criteria (V1.36 MVP; V1.37 multi-chapter extension)

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

The V1.36 single-chapter case (`total_planned_chapters == 1`) is a strict subset of this rule: chapter 1 finalizes, `current_chapter` becomes `1`, all rows are finalized, and the Work completes.

### 6.2 Behavior on completion

1. Set `works.status` → `completed` and `works.novel_completion_status` → `completed`
2. **Stop** enqueueing new `novel-writing` schedules for this Work
3. Emit user-visible message: Work is complete; start a **new** Work via init flow (no automatic switch)

**Note**: V1.36 single-chapter MVP completion means `ch01` reached `finalized` after the `llm_judge` GO (§5.1). V1.37 supersedes the chapter-1-only interpretation by extending the same rule across all seeded `work_chapters` rows; it does not change behavior for a one-chapter Work.

### 6.3 Explicit non-goals (through V1.40)

- No automatic creation of next novel Work **without** explicit switch/pool commands (V1.41 adds opt-in ceremony — §6.4)
- No platform publish on completion
- No novels-system 8-step / Redis switch (rejected for OSS core)

### 6.4 Multi-work completion extension (Shipped V1.41)

[novel-multi-work-lifecycle.md](novel-multi-work-lifecycle.md) shipped V1.41 (PR #53):

1. §6.1–§6.2 completion criteria unchanged.
2. Extend `auto_chain::mark_work_completed`: write `Works/<work_ref>/.completion-lock.json`; stop auto-chain on **that** Work only (lifecycle spec §3).
3. Clear pool `active` when bound pool row completes; no automatic next Work.
4. Next Work via `creator works use`, `creator works pool promote`, or `creator bootstrap --from-work` — not implicit scaffold.
5. **Reopen** same Work: `completion-lock release` then `creator run resume --reopen --reason` (grill-me B); distinct from `--from-work` new Work.
6. Pool integration: [novel-work-pool.md](novel-work-pool.md). **OUT:** `creator work switch`.

**V1.42 P1**: multi-volume PK (§4.5.4) — [v1.42-multi-volume-serial-writing-delivery-compass-v1.md](../../iterations/v1.42-multi-volume-serial-writing-delivery-compass-v1.md).

---

## 7. Sync contract overlay

Extends [novel-writing-sync-contract.md](novel-writing-sync-contract.md):

- **Scan root**: `Works/<work_ref>/Stories/` only
- **Exclude**: `outline.md` at Stories root (removed); outlines live under `Outlines/`
- **Chapter ordering**: numeric prefix in filename `ch<nn>-*`
- **Idempotency**: content-hash per chapter file unchanged

Sync **must not** upload full正文 by default (cli-spec §5.3 unchanged).

---

## 8. CLI / UX expectations

| Command | Novel profile behavior |
| --- | --- |
| `creator bootstrap --idea "..."` | Default `work_profile: novel` when `--preset novel-writing` or default produce path; V1.40 creation/init must obtain a `world_id` via create-new or bind-existing before scaffold completes |
| `creator bootstrap --idea "..." --world-id <world_id>` | Bind the new Work to an existing World (per §3.5); World KB is injected as context in `novel-writing` prompts |
| `creator bootstrap --idea "..." --init-preset novel-project-init` | Run the `novel-project-init` grill-me (scaffold dirs + mandatory World binding question + `work_chapters` seed rows) before intake |
| `creator works status [<work_id>]` | **V1.41** — migrated from `creator run status`; reads `work_chapters`; shows `work_ref`, chapter list, completion; **V1.39** fields + completion/runtime lock per [cli-spec.md](cli-spec.md) §6.2H |
| `creator works list` | **V1.41** — migrated from `creator run list` |
| `creator run resume <work_id>` | **V1.39** — resume checkpointed auto-chain after daemon restart |
| `creator run continue <work_id> --note "..."` | Appends inspiration; does not advance chapter index |
| `creator run reconcile-chapters <work_id>` | (V1.36 manual) Rebuilds `work_chapters` rows from `Works/<work_ref>/Stories/` filesystem state; per §4.1.2 reconciliation rules |

First-run path adds a mandatory World binding step for novel Work creation: create a new World or pick one from `creator world list`. New V1.40 Works print a one-line `world: <name> (<world_id>)` in the run summary.

### 8.1 Multi-chapter `creator works status` UX (V1.37 extension; V1.41 command path)

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

Next action: Chapter 3 is not started; run `creator run continue wrk_abc123` to begin.
```

Draft resume output:

```text
progress: 1 / 3 chapters finalized
current_chapter: 1

Chapters:
  ch01  finalized   words: 4,210
  ch02  draft       words: —
  ch03  not_started words: —

Next action: Chapter 2 is in draft; run `creator run continue wrk_abc123` to resume.
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
  hint: run `creator run reconcile-chapters wrk_abc123` to rebuild from filesystem.
```

Each chapter row must show `not_started | outlined | draft | finalized` and `actual_word_count` when finalized. For a one-chapter Work, the same output shape may collapse to one row, but command semantics are identical.

---

## 9. Acceptance (spec-level)

1. Layout §3.1 is stable; no normative `Stories/<story_ref>/` at workspace root; **no per-Work `Worldbuilding/` subtree** (world content lives in World KB per §3.5).
2. Sync §7 scoped to `Works/<work_ref>/Stories/`.
3. Chapter state SSOT is **`work_chapters` table in `state.db`** (§4.1.1); `work-status.md` file is **removed** in V1.36.
4. Completion §6 reads from `work_chapters`; documented and testable without publish.
5. `work_profile: novel` fields §2.1 registered in work-experience-model cross-link; `world_id` is the cross-Work binding (§3.5).
6. `novel-project-init` asks the mandatory World binding question (§3.5, §5) with only create-new / bind-existing V1.40 paths.
7. Compass demo path §2 in [v1.36 compass](../../iterations/v1.36-pending-delivery-compass.md) achievable after P1–P3 implement.

---

## 10. Change control

- **Authority**: Active V1.37 compass > V1.37 extension sections in this spec > shipped V1.36 baseline in this spec > generic Work spec for novel-specific rules. V1.37 delivery batching does not retroactively claim implementation until a plan ships.
- **Promotion**: On iteration ship, Status → `Shipped (V1.36)`; merge overlay sections into work-experience-model §profile extension if appropriate.
- **Reference distill**: Internal novels-system patterns informed §3.1/§4/§5; **§3.5 World integration** is a **Nexus-architectural choice** (cross-Work content belongs to World, not per-Work), explicitly **rejecting** the reference-system's per-Work `世界设定/` shape. No external repo paths in normative text.
- **V1.36 architecture deltas vs earlier draft** (recorded 2026-06-07):
  - `work_chapters` table replaces `work-status.md` file (§4.1)
  - `Works/<work_ref>/Worldbuilding/` subtree **removed**; world content lives in World KB (§3.5)
  - `world_id` becomes the cross-Work binding (§2.1, §3.5, §5, §8)
  - **Preset gates mechanism added** (orchestration-engine.md §7.9 Master + novel-specific gate values in this spec §5.3; new `world_binding: required | optional` toggle in §5.3.4). Replaces the implicit "novel-writing should already have scaffold" assumption with explicit enqueue-time enforcement.
  - **`novel-project-init` scaffold protocol enumerated** (§5.4): explicit file list, template sources, `work_chapters` row seeding, atomicity, idempotency, re-init handling. Replaces the high-level "mkdir scaffold + write template stubs" P1 T2 with a P1-implementer checklist.
- **V1.37 P1 multi-chapter roadmap deltas** (recorded 2026-06-08):
  - **Roadmap-only** decision recorded (§4.5): no code/schema/preset implementation claimed in P1.
  - `next_chapter(work_id)` algorithm defined (§4.5.2): lowest `not_started`, then lowest `draft` resume, otherwise novel-completion.
  - `works.current_chapter` clarified as latest finalized chapter only (§4.5.2, §6.1).
  - `work_chapters` PK migration deferred: V1.37 keeps `(work_id, chapter)` and reserves `(work_id, volume, chapter)` unique index for post-V1.37 multi-volume support (§4.5.4).
  - `Outlines/volume-outline.md` minimum structure and `world_refs` placement documented (§4.5.5).
  - Multi-chapter `creator run status` output made testable (§8.1).
- **V1.37 P2 World KB roadmap deltas** (recorded 2026-06-08):
  - **Roadmap-only** decision recorded (§3.5.1): no CLI/API/schema/prompt runtime/validator/`kb-extract` implementation claimed in P2.
  - Future `creator world create --title ... --description ...` contract defined for the init "create new World" path (§3.5.1.1); `--name` remains an alias and narrative kind is implicit.
  - `world_id` existence validation, legacy `world_binding: optional` V1.37 posture, and `preset_gates_failed` remediation documented (§3.5.1.2).
  - Prompt-time World context block shape documented for World-bound Works, with legacy worldless Works preserving README-only context (§3.5.1.3).
  - **V1.40 amendment** (recorded 2026-06-10): new novel Work creation/init requires `world_id`; legacy V1.39-and-earlier worldless Works remain readable but cannot be newly created (§2.1, §3.5, §3.5.1, §5.3.4, §5.4).
  - `world_refs` canonicalization and warning/error timing documented (§3.5.1.4).
  - Chapter → World KB extraction and explicit event/foreshadowing promotion path documented (§3.5.1.5).
- **V1.37 P3 quality-loop roadmap deltas** (recorded 2026-06-08):
  - **Roadmap-only** decision recorded (§5.5): no findings migration, new presets, daemon scheduled task, CLI subcommands, prompt templates, or file writers claimed in P3.
  - Findings lifecycle, severity mapping, and future `findings` table sketch documented (§5.5.1).
  - Reference executor concepts mapped to Nexus presets / CLI surfaces (`novel-writing`, future `novel-brainstorm`, future `novel-review-master`) (§5.5.2).
  - 96h master-decision timeout mapped to local DB + daemon scheduled task + `creator run status` banner with opt-in escalation (§5.5.3).
  - Three-layer rules architecture and SOUL / World KB boundaries documented (§5.5.4).
  - `Logs/brainstorm|write|review|publish` roadmap structure documented while reaffirming `Logs/**` sync exclusion (§5.5.5).
  - `reflection-loop` → findings / rule-suggestion integration documented as future implementation (§5.5.6).

---

*Shipped V1.36 baseline with V1.37 P1/P2/P3 roadmap extensions. Implement V1.37 multi-chapter, World KB continuity, or quality-loop behavior only via a future locked implementation plan.*
