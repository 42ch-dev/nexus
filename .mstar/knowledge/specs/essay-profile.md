# Essay Profile — Specification v1

**Status**: Shipped (V1.63) — essay profile production-ready: scaffold (ScaffoldTransaction), `essay-writing` preset (7-state chain), 4-dimension quality rubric (blocking with `--force-gates` override), completion detection, optional KB extraction.  
**Document class**: Feature line  
**Created**: 2026-06-19  
**Last updated**: 2026-06-24 (V1.63 Shipped — Draft → Shipped promotion).  
**Scope**: `work_profile: essay` on generic **Work** — artifact layout under `Works/<work_ref>/`, templates, stage chain, completion semantics  
**Coordinates with**:

- [work-experience-model.md](work-experience-model.md) — generic Work entity
- [creator-workflow.md](creator-workflow.md) — FL-E stage model
- [cli-spec.md](cli-spec.md) — creator entry and workspace layout
- [orchestration-engine.md](orchestration-engine.md) — preset execution model
- [entity-scope-model.md](entity-scope-model.md) — optional World/KB binding boundaries
- [novel-writing/workflow-profile.md](novel-writing/workflow-profile.md) — prior `work_profile: novel` Feature line pattern

**Iteration compass**: [v1.52-author-completion-and-multi-branch-preset-orchestration-delivery-compass-v1.md](../iterations/v1.52-author-completion-and-multi-branch-preset-orchestration-delivery-compass-v1.md)

---

## 1. Purpose

`work_profile: essay` is the first non-novel creative profile in Nexus OSS. It covers short-to-medium form nonfiction or reflective prose that needs intake, outline, draft, revise, and finalize stages without chapter state, volume semantics, or serial novel scheduling.

The profile intentionally mirrors the `novel-writing/workflow-profile.md` structure where useful, but it is not a chaptered fiction profile. Its durable slice is one outline artifact and one draft artifact under `Works/<work_ref>/`.

---

## 2. Relationship to Work

| Concept | Generic Work | Essay profile (`work_profile: essay`) | Novel contrast |
| --- | --- | --- | --- |
| Identity | `work_id`, `creator_id`, `workspace_slug` | Same | Same |
| Human slug | optional `story_ref` / `work_ref` | **`work_ref`** = directory under `Works/` | Same filesystem root |
| Status | `draft` \| `active` \| `paused` \| `completed` \| `archived` | Single artifact status: `draft` \| `revised` \| `finalized` | Novel uses `work_chapters` rows |
| Intake | `creative_brief` | Required before outline | Same FL-E intake |
| Produce preset | profile-specific | `essay-writing` or equivalent profile preset | Novel uses `novel-writing` |
| Completion | generic goal met | §8 — draft frontmatter `status == finalized` | Novel requires all chapters finalized |

Essay Works do not create or read `work_chapters`. Any code path that assumes a chapter table must gate on `work_profile == novel` before applying novel-specific behavior.

---

## 3. Artifact layout

```text
<workspace>/
  Works/
    <work_ref>/
      README.md
      Outlines/
        outline.md
      Drafts/
        draft.md
      Logs/
        write/
        review/
```

| Path | Sync manuscript? | Purpose |
| --- | --- | --- |
| `Works/<work_ref>/README.md` | No | Human overview: thesis, audience, constraints |
| `Works/<work_ref>/Outlines/outline.md` | No | Single essay outline; no chapter splits |
| `Works/<work_ref>/Drafts/draft.md` | Yes, if manuscript sync is explicitly enabled | Canonical essay正文 artifact |
| `Works/<work_ref>/Logs/**` | No | Process logs and review evidence |

Essay profiles **must not** use `Stories/` unless a future plan explicitly changes the profile. That directory remains novel-specific.

---

## 4. Template

Minimum `Outlines/outline.md`:

```markdown
---
title: <essay title>
status: outline
---

# Thesis

# Audience

# Structure

1. Opening hook
2. Core argument
3. Supporting evidence
4. Counterpoint / nuance
5. Ending takeaway
```

Minimum `Drafts/draft.md`:

```yaml
---
title: string
status: draft | revised | finalized
word_count: integer
---
```

The markdown body is the single essay manuscript. Frontmatter is the visible human/agent status mirror; Work-level status follows §8.

---

## 5. Stage chain

```text
intake → outline → draft → revise → finalize
```

| Stage | Artifact | Gate |
| --- | --- | --- |
| `intake` | Work creative brief | Required before outline |
| `outline` | `Outlines/outline.md` | Thesis + audience + structure present |
| `draft` | `Drafts/draft.md` status `draft` | Outline exists |
| `revise` | `Drafts/draft.md` status `revised` | Draft exists; review notes optional |
| `finalize` | `Drafts/draft.md` status `finalized` | 4-dimension quality rubric passes or explicit `--force-gates` override (V1.63 P2) |

Essay finalization runs a 4-dimension quality rubric (thesis clarity, evidence support, coherence, ending takeaway) — a **blocking gate** with `--force-gates` override (parity with game-bible/script Depth 3.5). All four dimensions must pass for the `finalize_commit` state to write `status: finalized` to the draft frontmatter.

---

## 6. World integration

Essay Works may optionally bind to a World when the essay analyzes or describes a fictional setting. Binding is **optional**, not mandatory.

Allowed context:

- A single character KB context block when the essay is character-focused.
- A single location KB context block when the essay is setting-focused.
- No automatic World KB promotion by default.

The essay profile must not introduce a per-Work `Worldbuilding/` subtree. Cross-Work facts remain in World KB per [entity-scope-model.md](entity-scope-model.md).

---

## 7. Frontmatter

`Drafts/draft.md` minimum frontmatter:

```yaml
---
title: string
status: draft | revised | finalized
word_count: integer
---
```

Optional fields may include `audience`, `thesis`, `world_refs`, and `source_refs`, but the V1.52 minimum implementation must not require them.

---

## 8. Completion

An essay Work is complete when:

```text
Works/<work_ref>/Drafts/draft.md frontmatter status == finalized
AND works.intake_status == complete
→ works.status = completed
```

Completion does not enqueue a next chapter, next volume, or new Work. Reopening an essay Work follows the generic Work reopen path, not the novel completion-lock path unless a future plan explicitly generalizes that lock.

---

## 9. Acceptance (V1.52 draft)

1. Essay layout is distinct from novel layout (`Drafts/` not `Stories/`).
2. Stage chain is single-artifact and chapter-free.
3. Completion is `status == finalized` on `Drafts/draft.md` plus intake complete.
4. World binding is optional and bounded to compact KB context; no per-Work Worldbuilding subtree.

---

*Draft V1.52 Feature line. Implementation authority is active only while V1.52 compass is active; P-last promotes or revises after T-A P2 evidence.*
