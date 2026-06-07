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

| Preset | Role | When | Gates (see §5.3) |
| --- | --- | --- | --- |
| `novel-project-init` | Interactive grill-me; sets `work_ref`, `total_planned_chapters`, `world_id` (bind to existing / create new / worldless), scaffolds `Works/<work_ref>/` dirs (§5.4), seeds `work_chapters` rows | Before first `novel-writing` if scaffold missing | §5.3.1 |
| `creative-brief-intake` | Structured brief on Work | FL-E `intake` / `creator run start` | (generic; out of novel overlay) |
| `novel-writing` | Outline → draft → **finalize (gated by `llm_judge`)** → `finalized`; per-chapter transitions update both `work_chapters` row + chapter frontmatter | FL-E `produce` | §5.3.2 |
| `reflection-loop` | Optional deeper quality pass; **not** in V1.36 default flow | FL-E `review` (optional V1.36) | §5.3.3 |

**Separation rule**: `novel-project-init` is **not** auto-chained inside `novel-writing`. User or `creator run` explicitly schedules it when starting a new novel Work. The engine enforces this via the `previous_preset: novel-project-init` gate on `novel-writing` (§5.3.2).

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

- **`required`** (default for `novel-writing`): the `world_id` gate is active; World-bound Works must have `world_id` set. For **worldless** Works (`world_id == NULL`), the user must either set `world_id` before scheduling or use `--force` with audit reason.
- **`optional`**: the `world_id` gate is skipped; `novel-writing` runs regardless of `world_id`. The prompt still injects World KB context block when `world_id != NULL` (per §5.2).

For V1.36, `novel-writing` ships with `world_binding: optional` (so worldless users aren't blocked). Future iterations may tighten to `required` if the World ecosystem matures.

#### 5.3.5 Gate failure user experience

When a `novel-writing` gate fails (e.g. user runs `creator run start --idea "..."` without first running `novel-project-init`), the CLI surfaces:

```text
error: preset_gates_failed
  preset: novel-writing
  work_id: wrk_abc123
  failed_gates:
    - filesystem: Works/cozy-mystery/ must exist (actual: missing)
        ↳ Run `creator run start --init-preset novel-project-init` to scaffold the Work.
    - work_field: intake_status must equal "complete" (actual: pending)
        ↳ Complete intake via `creator run stage advance --stage intake`.
  override: pass --force-gates with --reason "<text>" to bypass (audit-logged)
```

This is the **user-visible demo-pain killer**: instead of scheduling `novel-writing` and failing deep in the state graph, the engine rejects at enqueue with a clear remediation.

#### 5.3.6 Implementation note (V1.36)

- Gate evaluation is **read-only** at enqueue time; the engine queries `works` table and filesystem; it does not mutate.
- Gate evaluation is **idempotent**: a failed check leaves no side effects.
- Engine logs gate failures to the structured log with `preset_id`, `work_id`, `failed_gates` array. Failed-gate rate by preset is a future observability dashboard (out of V1.36 scope).

### 5.4 `novel-project-init` scaffold protocol (file enumeration)

`novel-project-init` is the canonical way to bootstrap a Work's `Works/<work_ref>/` tree. The grill-me collects `work_ref`, `total_planned_chapters`, and the World binding question (§3.5); on success, the preset's **scaffold capability** (or handler) creates the full directory tree, copies template stubs, and seeds `work_chapters` rows. This section enumerates every file/dir so the P1 implementer (and any hand-rolled init script) has a checklist.

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

For **World-bound** Works, `README.md` is rendered with a one-line `world_id: <uuid>` header and links to the World KB items the Work will reference most. For **worldless** Works, `README.md` includes a 1–2 paragraph inline world setting note (collected from grill-me, optional).

#### 5.4.3 `work_chapters` row seeding (DB writes)

For `chapter IN 1..total_planned_chapters`, insert one `work_chapters` row per chapter:

| Column | Value |
| --- | --- |
| `work_id` | from grill-me / `creator run start` |
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
| `world_id` | from grill-me (existing / new / NULL) |
| `current_chapter` | `0` (reset on fresh init) |
| `updated_at` | now() |

If the Work already has a `work_ref` and `Works/<work_ref>/` exists (re-init case), the scaffold **does not overwrite** existing files. The PATCH only updates fields the user explicitly changed in this grill-me session.

#### 5.4.5 Idempotency

Re-running `novel-project-init` on a Work that already has the scaffold is **safe**:

- Files that exist are not overwritten (unless user explicitly opts in via grill-me "re-scaffold from templates" answer).
- `work_chapters` rows are not duplicated (PK conflict on `(work_id, chapter)`; existing rows preserved).
- `works` PATCH is a no-op if all fields are unchanged.

The grill-me offers an "overwrite templates" option for users who want to re-render the README/Outlines/ from latest embedded templates (useful after a toolchain update). V1.36 default is **preserve**; "overwrite" is the explicit user opt-in.

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
  - **Preset gates mechanism added** (orchestration-engine.md §7.9 Master + novel-specific gate values in this spec §5.3; new `world_binding: required | optional` toggle in §5.3.4). Replaces the implicit "novel-writing should already have scaffold" assumption with explicit enqueue-time enforcement.
  - **`novel-project-init` scaffold protocol enumerated** (§5.4): explicit file list, template sources, `work_chapters` row seeding, atomicity, idempotency, re-init handling. Replaces the high-level "mkdir scaffold + write template stubs" P1 T2 with a P1-implementer checklist.

---

*Draft V1.36 — implement via plans `2026-06-07-v1.36-novel-*`.*
