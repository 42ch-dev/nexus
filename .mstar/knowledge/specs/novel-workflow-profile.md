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
| Status | `draft` \| `active` \| `paused` \| `completed` \| `archived` | Adds **chapter table** in `work-status.md` |
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
| `novel_completion_status` | enum | no | `in_progress` \| `completed` (mirrors Work.status when terminal) |

**Invariant**: `work_ref` is stable for the life of the Work; renaming directory without DB update is unsupported pre-1.0.

---

## 3. Artifact layout

### 3.1 Generic Work root

```text
<workspace>/
  Works/
    <work_ref>/
      README.md
      work-status.md
      Worldbuilding/          # optional; profile-specific subtree
      Outlines/
        volume-outline.md     # optional in V1.36 MVP
        chapters/
          ch<nn>-outline.md
      Stories/                # novel正文 ONLY — sync chapter scan root
        ch<nn>-<slug>.md
      Logs/                   # optional process logs
```

### 3.2 Directory rules

| Path | Sync chapter? | Purpose |
| --- | --- | --- |
| `Works/<work_ref>/README.md` | **No** | Human overview; links to status and outlines |
| `Works/<work_ref>/work-status.md` | **No** | Chapter status table + progress (machine-assist manifest) |
| `Works/<work_ref>/Outlines/**` | **No** | Planning artifacts |
| `Works/<work_ref>/Worldbuilding/**` | **No** | Setting/character bibles |
| `Works/<work_ref>/Stories/*.md` | **Yes** | Chapter正文 (frontmatter `chapter`, `status`) |
| `Works/<work_ref>/Logs/**` | **No** | Brainstorm/write/review logs |

### 3.3 Legacy prohibition

The following are **removed from normative specs** (pre-1.0):

- Workspace-root `Stories/<story_ref>/`
- Prompt variables defaulting to `Stories/{{preset.input.story_ref}}/…` without `Works/<work_ref>/` prefix
- Sync scanners treating any `Works/<work_ref>/*.md` as chapters

Implementations **must not** provide dual-path fallbacks.

### 3.4 Other creative profiles (future)

Non-novel `work_profile` values may use different subtrees under `Works/<work_ref>/` (e.g. `Drafts/`, `Sections/`). They **must not** reuse `Stories/` unless the profile spec says so.

---

## 4. Templates and frontmatter

### 4.1 `work-status.md`

Machine- and human-readable manifest. Minimum sections:

- Frontmatter: `work_id`, `work_ref`, `work_profile: novel`, `status`, `total_planned_chapters`, `current_chapter`, `updated`
- **Chapter status table** with states: `not_started` | `outlined` | `draft` | `finalized` | `published` (published reserved; OSS does not set in V1.36)

### 4.2 Chapter outline (`Outlines/chapters/ch<nn>-outline.md`)

Structured outline before正文. Minimum headings: opening scene, core conflict, turning point, climax, ending hook, character state change, foreshadowing (optional).

### 4.3 Chapter body (`Stories/ch<nn>-<slug>.md`)

```yaml
---
title: string
chapter: integer
status: draft | finalized   # published reserved; not set by OSS core in V1.36
word_count: integer         # optional
---
```

Body text only (title in frontmatter). Status transitions:

| Transition | Actor |
| --- | --- |
| → `draft` | `novel-writing` drafting phase |
| → `finalized` | `novel-writing` review/refine phase or `reflection-loop` stage |

### 4.4 Embedded preset templates

V1.36 implement wave ships template stubs under `crates/nexus-orchestration/embedded-presets/novel-writing/templates/` (or documented equivalent). Prompts reference `Works/{{work_ref}}/…` variables.

---

## 5. Preset responsibilities

| Preset | Role | When |
| --- | --- | --- |
| `novel-project-init` | Interactive grill-me; sets `work_ref`, `total_planned_chapters`, scaffold dirs | Before first `novel-writing` if scaffold missing |
| `creative-brief-intake` | Structured brief on Work | FL-E `intake` / `creator run start` |
| `novel-writing` | Gathering → brainstorm → outline → draft正文 | FL-E `produce` |
| `reflection-loop` | Optional quality pass | FL-E `review` (optional V1.36) |

**Separation rule**: `novel-project-init` is **not** auto-chained inside `novel-writing`. User or `creator run` explicitly schedules it when starting a new novel Work.

---

## 6. Completion semantics

### 6.1 Completion criteria (V1.36 MVP)

A novel Work is **complete** when **all** hold:

1. `current_chapter >= total_planned_chapters` (from `work-status.md` / DB)
2. Every chapter `1..total_planned_chapters` has `status == finalized` in chapter table
3. `intake_status == complete` on Work

### 6.2 Behavior on completion

1. Set `Work.status` → `completed` and `novel_completion_status` → `completed`
2. Update `work-status.md` frontmatter `status: completed`
3. **Stop** enqueueing new `novel-writing` schedules for this Work
4. Emit user-visible message: Work is complete; start a **new** Work via init flow (no automatic switch)

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
| `creator run status <work_id>` | Shows `work_ref`, chapter table summary, completion state |
| `creator run continue <work_id> --note "..."` | Appends inspiration; does not advance chapter index |

First-run path unchanged (≤7 steps per cli-spec §7.1); novel init preset may add optional step for greenfield novels.

---

## 9. Acceptance (spec-level)

1. Layout §3 is stable; no normative `Stories/<story_ref>/` at workspace root.
2. Sync §7 scoped to `Works/<work_ref>/Stories/`.
3. Completion §6 documented and testable without publish.
4. `work_profile: novel` fields §2.1 registered in work-experience-model cross-link.
5. Compass demo path §2 in [v1.36 compass](../../iterations/v1.36-pending-delivery-compass.md) achievable after P1–P3 implement.

---

## 10. Change control

- **Authority**: Active V1.36 compass > this spec > generic Work spec for novel-specific rules.
- **Promotion**: On iteration ship, Status → `Shipped (V1.36)`; merge overlay sections into work-experience-model §profile extension if appropriate.
- **Reference distill**: Internal novels-system patterns informed §3–§6; no external repo paths in normative text.

---

*Draft V1.36 — implement via plans `2026-06-07-v1.36-novel-*`.*
