# Novel Workflow Profile — Normative Specification v1

**Status**: Draft (V1.36 — 2026-06-07)  
**Document class**: Feature line (profile overlay)  
**Created**: 2026-06-07  
**Scope**: `work_profile: novel` on generic **Work** — artifact layout under `Works/<work_ref>/`, templates, chapter status, completion semantics, sync boundaries  
**Coordinates with**:

- [work-experience-model.md](work-experience-model.md) — generic Work entity
- [creator-workflow.md](creator-workflow.md) — FL-E `produce` stage
- [cli-spec.md](cli-spec.md) — workspace layout §13.1
- [novel-writing-sync-contract.md](novel-writing-sync-contract.md) — chapter discovery
- [orchestration-engine.md](orchestration-engine.md) — `novel-writing` preset
- [entity-scope-model.md](entity-scope-model.md) — World entity + World KB (`work_profile: novel` binds Work to World; world content is cross-Work, lives in World KB, NOT in per-Work `Worldbuilding/` subtree)

**Iteration compass**: [v1.36-pending-delivery-compass.md](../../iterations/v1.36-pending-delivery-compass.md)

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
| `world_id` | string (FK) | no | Bind to a World (per [entity-scope-model.md](entity-scope-model.md) §5.4). When set, chapter body may reference World KB items. When NULL, the Work is worldless. `novel-project-init` grill-me asks this question (see §3.5). |
| `novel_completion_status` | enum | no | `in_progress` \| `completed` (mirrors Work.status when terminal) |

**Invariant**: `work_ref` is stable for the life of the Work; renaming directory without DB update is unsupported pre-1.0.

---

## 3. Artifact layout

### 3.1 Generic Work root

```text
<workspace>/
  Works/
    <work_ref>/
      README.md                 # human overview; may include brief world setting notes for worldless Works
      Outlines/
        volume-outline.md       # optional in V1.36 MVP
        chapters/
          ch<nn>-outline.md
        foreshadowing.md        # empty stub V1.36 (F### rows; future V1.37+ scaffold)
        event-index.md          # empty stub V1.36 (E### rows; future V1.37+ scaffold)
      Stories/                  # novel正文 ONLY — sync chapter scan root
        ch<nn>-<slug>.md
      Logs/                     # optional process logs (single-role V1.36; structure OUT)
```

### 3.2 Directory rules

| Path | Sync chapter? | Purpose |
| --- | --- | --- |
| `Works/<work_ref>/README.md` | **No** | Human overview; worldless Works may include world setting notes here |
| `Works/<work_ref>/Outlines/chapters/*` | **No** | Per-chapter outline |
| `Works/<work_ref>/Outlines/foreshadowing.md` | **No** | Cross-chapter foreshadowing index (F### rows) |
| `Works/<work_ref>/Outlines/event-index.md` | **No** | Cross-chapter event index (E### rows) |
| `Works/<work_ref>/Outlines/volume-outline.md` | **No** | Volume-level outline (optional V1.36) |
| `Works/<work_ref>/Stories/*.md` | **Yes** | Chapter正文 (frontmatter `chapter`, `status`) |
| `Works/<work_ref>/Logs/**` | **No** | Brainstorm/write/review logs (structure OUT V1.36) |

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
- **`world_id` is the binding**. A novel Work may be **World-bound** (`work.world_id` set) or **worldless** (`work.world_id == NULL`).
  - **World-bound Work** (`world_id != NULL`): characters, locations, society, rules, events, timelines come from World KB. Chapter body may reference World KB items by id via `world_refs: [char_xxx, loc_yyy]` frontmatter; V1.36 does not validate, V1.37+ may enforce.
  - **Worldless Work** (`world_id == NULL`): no cross-Work continuity. `README.md` may include a brief inline world setting note (1–2 paragraphs) for LLM context. Character names in the body are pure-prose; no KB.
- **`novel-project-init` asks the binding question** (grill-me). Three options: bind to existing `world_id` (user picks from list) / create new World (calls `creator world create --name "..." --kind narrative`, which is a **future** CLI command; V1.36 may prompt for the world metadata inline and pass to a future API) / stay worldless (default).
- **Work → World KB promotion** is the **long-term** path: as chapters finalize, `kb-extract` preset (existing, per [creator-workflow.md](creator-workflow.md) `persist` stage) can extract entities / events / rules from chapter body into World KB items. V1.36 documents this path; enforcement is V1.37+.

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

**V1.37+ extension**: PK becomes `(work_id, volume, chapter)` with `volume` defaulting to 0 for single-volume Works. DDL change in P0 of the V1.37+ iteration; V1.36 ships with `(work_id, chapter)` PK.

#### 4.1.2 Truth model (DB vs frontmatter)

- **`work_chapters` is the queryable SSOT** for `creator run status`, completion evaluation (§6), and sync module (if it needs per-chapter metadata).
- **Chapter .md frontmatter `status` is the human/LLM read-end**: the orchestration engine updates **both** on transition; the frontmatter flip is the visible "I'm now finalized" signal to the next prompt.
- **Reconciliation**: on daemon startup, an optional `creator run reconcile-chapters <work_id>` walks `Works/<work_ref>/Stories/` and rebuilds `work_chapters` rows from filesystem (frontmatter is truth if file is newer; DB row is truth if DB is newer). For V1.36 this is a manual command, not an automatic job.

#### 4.1.3 `README.md` (human overview, no chapter state)

`Works/<work_ref>/README.md` is the only human file. It is **author-edited** and may contain:

- Working title, premise, blurb
- For **worldless** Works: a brief world setting note (1–2 paragraphs) so the LLM has context
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

---

## 5. Preset responsibilities

| Preset | Role | When |
| --- | --- | --- |
| `novel-project-init` | Interactive grill-me; sets `work_ref`, `total_planned_chapters`, `world_id` (bind to existing / create new / worldless), scaffolds `Works/<work_ref>/` dirs, seeds `work_chapters` rows | Before first `novel-writing` if scaffold missing |
| `creative-brief-intake` | Structured brief on Work | FL-E `intake` / `creator run start` |
| `novel-writing` | Outline → draft → **finalize (gated by `llm_judge`)** → `finalized`; per-chapter transitions update both `work_chapters` row + chapter frontmatter | FL-E `produce` |
| `reflection-loop` | Optional deeper quality pass; **not** in V1.36 default flow | FL-E `review` (optional V1.36) |

**Separation rule**: `novel-project-init` is **not** auto-chained inside `novel-writing`. User or `creator run` explicitly schedules it when starting a new novel Work.

### 5.1 V1.36 chapter finalize quality gate

`novel-writing`'s `finalize` state has `exit_when: kind: llm_judge` evaluating a **五问质量检验** prompt (opening three lines / conflict resonance / twist recall / new perspective / ending hook). This prevents the "click and finalize" demo feel where any draft becomes `finalized` without scrutiny.

- **GO** → both `work_chapters.status` AND chapter frontmatter `status` flip to `finalized`; `work_chapters.actual_word_count` is updated from frontmatter `word_count`; `current_chapter` advances.
- **NOGO** → `WaitForInput`; user may `creator run continue <work_id> --note "..."` with additional context, then re-run. `work_chapters.status` and frontmatter `status` both stay `draft`.
- **GO override on NOGO** → user explicit `creator run stage advance --force` with audit-logged reason; both rows flip to `finalized` regardless.

The 五问 template file lives at `embedded-presets/novel-writing/prompts/finalize-exit.md` (P3 deliverable). It references [writing-craft-rules.md §2 五问质量检验](writing-craft-rules.md) when present; otherwise the template embeds the five questions inline.

### 5.2 World-bound Work behavior (in `novel-writing` prompts)

When a Work has `world_id != NULL`:

- Before drafting each chapter, the orchestration engine injects a **World context block** into the prompt: character names + 1-line descriptors, key locations, current timeline position. Sourced from World KB via `creator kb query world <world_id>` (or equivalent capability).
- The LLM is asked to **name characters / locations exactly as in World KB** unless the chapter is introducing a new one (new ones go to `Outlines/event-index.md` for later `kb-extract` promotion).
- `world_refs: [char_xxx, loc_yyy]` frontmatter is filled by the agent based on what the chapter references. V1.36 does not validate; V1.37+ may validate.

For **worldless** Works (`world_id == NULL`): no World context block; LLM uses `README.md` setting notes as the only world context.

---

## 6. Completion semantics

### 6.1 Completion criteria (V1.36 MVP)

A novel Work is **complete** when **all** hold:

1. `works.current_chapter >= works.total_planned_chapters` (from `works` table)
2. Every row in `work_chapters` for `chapter IN 1..total_planned_chapters` has `status == finalized`
3. `works.intake_status == complete`

### 6.2 Behavior on completion

1. Set `works.status` → `completed` and `works.novel_completion_status` → `completed`
2. **Stop** enqueueing new `novel-writing` schedules for this Work
3. Emit user-visible message: Work is complete; start a **new** Work via init flow (no automatic switch)

**Note**: V1.36 single-chapter MVP completion means `ch01` reached `finalized` after the `llm_judge` GO (§5.1). Multi-chapter completion semantics are intentionally V1.37+ scope. The V1.36 "completion" UX is therefore the **chapter-level** finish, not a novel-level finish; the compass calls this the "single-chapter MVP" boundary.

### 6.3 Explicit non-goals

- No automatic creation of next novel Work
- No platform publish on completion
- No "完本后切换" automation (reference-system pattern explicitly rejected for OSS core)

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
| `creator run start --idea "..."` | Default `work_profile: novel` when `--preset novel-writing` or default produce path |
| `creator run start --idea "..." --world-id <world_id>` | Bind the new Work to an existing World (per §3.5); World KB is injected as context in `novel-writing` prompts |
| `creator run start --idea "..." --init-preset novel-project-init` | Run the `novel-project-init` grill-me (scaffold dirs + World binding question + `work_chapters` seed rows) before intake |
| `creator run status <work_id>` | Reads from `work_chapters` table; shows `work_ref`, chapter list with status, completion state |
| `creator run continue <work_id> --note "..."` | Appends inspiration; does not advance chapter index |
| `creator run reconcile-chapters <work_id>` | (V1.36 manual) Rebuilds `work_chapters` rows from `Works/<work_ref>/Stories/` filesystem state; per §4.1.2 reconciliation rules |

First-run path unchanged (≤7 steps per cli-spec §7.1); novel init preset may add optional step for greenfield novels. World-bound Works print a one-line `world: <name> (<world_id>)` in the run summary.

---

## 9. Acceptance (spec-level)

1. Layout §3.1 is stable; no normative `Stories/<story_ref>/` at workspace root; **no per-Work `Worldbuilding/` subtree** (world content lives in World KB per §3.5).
2. Sync §7 scoped to `Works/<work_ref>/Stories/`.
3. Chapter state SSOT is **`work_chapters` table in `state.db`** (§4.1.1); `work-status.md` file is **removed** in V1.36.
4. Completion §6 reads from `work_chapters`; documented and testable without publish.
5. `work_profile: novel` fields §2.1 registered in work-experience-model cross-link; `world_id` is the cross-Work binding (§3.5).
6. `novel-project-init` asks the World binding question (§3.5, §5).
7. Compass demo path §2 in [v1.36 compass](../../iterations/v1.36-pending-delivery-compass.md) achievable after P1–P3 implement.

---

## 10. Change control

- **Authority**: Active V1.36 compass > this spec > generic Work spec for novel-specific rules.
- **Promotion**: On iteration ship, Status → `Shipped (V1.36)`; merge overlay sections into work-experience-model §profile extension if appropriate.
- **Reference distill**: Internal novels-system patterns informed §3.1/§4/§5; **§3.5 World integration** is a **Nexus-architectural choice** (cross-Work content belongs to World, not per-Work), explicitly **rejecting** the reference-system's per-Work `世界设定/` shape. No external repo paths in normative text.
- **V1.36 architecture deltas vs earlier draft** (recorded 2026-06-07):
  - `work_chapters` table replaces `work-status.md` file (§4.1)
  - `Works/<work_ref>/Worldbuilding/` subtree **removed**; world content lives in World KB (§3.5)
  - `world_id` becomes the cross-Work binding (§2.1, §3.5, §5, §8)

---

*Draft V1.36 — implement via plans `2026-06-07-v1.36-novel-*`.*
