# Script Profile — Draft Specification V1.55

**Status**: Draft (V1.55 P3)
**Document class**: Feature line
**Created**: 2026-06-22
**Scope**: `work_profile: script` on generic **Work** — artifact layout under `Works/<work_ref>/Scripts/`, templates, stage chain, KB taxonomy, completion semantics
**Coordinates with**:

- [essay-profile.md](essay-profile.md) — first non-novel Feature line pattern (structural template)
- [game-bible-profile.md](game-bible-profile.md) — second non-novel profile (V1.54 scaffold + V1.55 Depth 3.5)
- [novel-writing/workflow-profile.md](novel-writing/workflow-profile.md) — novel baseline (contrast reference)
- [work-experience-model.md](work-experience-model.md) — generic Work entity
- [creator-workflow.md](creator-workflow.md) — FL-E stage model
- [cli-spec.md](cli-spec.md) — creator entry and workspace layout
- [orchestration-engine.md](orchestration-engine.md) — preset execution model
- [entity-scope-model.md](entity-scope-model.md) — World/KB binding and BlockType taxonomy
- [non-novel-profiles-roadmap.md](non-novel-profiles-roadmap.md) — prior Exploration (now promoted)

**Plan**: [2026-06-22-v1.55-script-scaffold.md](../../plans/2026-06-22-v1.55-script-scaffold.md)

---

## 1. Purpose

`work_profile: script` is the third non-novel creative profile in Nexus OSS (following `essay` and `game_bible`). It covers screen/stage/audio script production: scenes, dialogue, beats, character directions, and revision passes.

The profile intentionally mirrors `game-bible-profile.md` and `essay-profile.md` structure where useful, but it does not use `work_chapters`, `Stories/`, or `Outlines/` (novel-specific paths). Its durable slice is a script scaffold with template files under `Works/<work_ref>/`.

---

## 2. Relationship to Work

| Concept | Generic Work | Script profile (`work_profile: script`) | Novel contrast |
| --- | --- | --- | --- |
| Identity | `work_id`, `creator_id`, `workspace_slug` | Same | Same |
| Human slug | optional `story_ref` / `work_ref` | **`work_ref`** = directory under `Works/` | Same filesystem root |
| Status | `draft` \| `active` \| `paused` \| `completed` \| `archived` | Work-level status; scene/beat status tracked per frontmatter | Novel uses `work_chapters` rows |
| Intake | `creative_brief` | Required before script drafting | Same FL-E intake |
| Produce preset | profile-specific | `script-writing` (future; V1.55 scaffold only) | Novel uses `novel-writing` |
| Completion | generic goal met | §8 — all required script elements accepted | Novel requires all chapters finalized |

Script Works do not create or read `work_chapters`. Any code path that assumes a chapter table must gate on `work_profile == novel` before applying novel-specific behavior.

---

## 3. Artifact Layout

```text
<workspace>/
  Works/
    <work_ref>/
      README.md                  # Core concept, format, genre, target medium
      Scripts/
        script.md                # Main script body (scenes, dialogue)
      Beats/
        beat-sheet.md            # Beat sheet / scene outline
      Characters/
        characters.md            # Character directions, casting notes
      Logs/
        write/
        review/
```

| Path | Sync manuscript? | Purpose |
| --- | --- | --- |
| `Works/<work_ref>/README.md` | No | Human overview: concept, format, genre, target medium |
| `Works/<work_ref>/Scripts/script.md` | No | Main script body — scenes, dialogue, action lines |
| `Works/<work_ref>/Beats/beat-sheet.md` | No | Beat sheet — story beats, scene outline, act structure |
| `Works/<work_ref>/Characters/characters.md` | No | Character directions, casting notes, arc tracking |
| `Works/<work_ref>/Logs/write/` | No | Writing iteration logs |
| `Works/<work_ref>/Logs/review/` | No | Review evidence and feedback |

Script profiles **must not** use `Stories/`, `Outlines/`, `Drafts/`, or `Design/` unless a future plan explicitly changes the profile. Those directories remain specific to other profiles.

**Not scaffolded in V1.55 P3**: full script writing run-loop, `work_chapters` rows, platform screenplay export, formatting normalization.

---

## 4. Templates

### 4.1 Section Frontmatter

Each scaffolded template carries YAML frontmatter:

```yaml
---
section_status: draft | reviewed | accepted
section_weight: critical | important | nice_to_have
last_updated: <ISO 8601 datetime>
---
```

| Field | Values | Purpose |
| --- | --- | --- |
| `section_status` | `draft`, `reviewed`, `accepted` | Per-section maturity |
| `section_weight` | `critical`, `important`, `nice_to_have` | Priority tier — `critical` sections gate completion |
| `last_updated` | ISO 8601 datetime | Last substantive edit timestamp |

### 4.2 Template Stubs (V1.55 init)

**`Scripts/script.md`**:
```yaml
---
section_status: draft
section_weight: critical
---
# Script
<!-- Scene headings, dialogue, action lines, parentheticals -->
```

**`Beats/beat-sheet.md`**:
```yaml
---
section_status: draft
section_weight: critical
---
# Beat Sheet
<!-- Story beats, scene outline, act structure -->
```

**`Characters/characters.md`**:
```yaml
---
section_status: draft
section_weight: important
---
# Characters
<!-- Character directions, casting notes, arc tracking -->
```

---

## 5. Stage Chain

```text
intake → draft → review → revise
```

| Stage | Artifact | Gate |
| --- | --- | --- |
| `intake` | Work creative brief | Required before script drafting |
| `draft` | `Scripts/script.md` + `Beats/beat-sheet.md` | Intake complete |
| `review` | Sections reviewed against script rubric | Author or agent review |
| `revise` | Sections revised | Review feedback applied |

Script does **not** enforce linear scene-by-scene progression. Stage transitions are profile-defined; the daemon does not auto-chain script stages in V1.55. Full script-writing production preset (`script-writing`) is deferred to V1.56+.

---

## 6. World Integration

Script Works may optionally bind to a World when the script references a specific fictional setting. Binding is **optional**, not mandatory.

When bound:
- The Work's `world_id` links to the World aggregate.
- KB entries created from script facts use `script_category` (see §7 KB Taxonomy).
- Future script-writing preset may pull World KB context for character/location consistency.

The script profile must not introduce a per-Work `Worldbuilding/` subtree. Cross-Work facts remain in World KB per [entity-scope-model.md](entity-scope-model.md).

---

## 7. KB Taxonomy

### 7.1 New BlockType Variants (V1.55 P3)

Three new `BlockType` wire enum variants are added to `schemas/common/common.schema.json`:

| Wire name (`snake_case`) | UI label | `script_category` | Primary section |
| --- | --- | --- | --- |
| `dialogue` | Dialogue | `dialogue` | `Scripts/script.md` |
| `beat` | Beat | `beat` | `Beats/beat-sheet.md` |
| `act` | Act | `act` | `Beats/beat-sheet.md` |

### 7.2 Script Category Mapping

`nexus-kb::validation` maps `script_category` → `BlockType`:

| `script_category` | Wire `BlockType` | Validation |
| --- | --- | --- |
| `dialogue` | `dialogue` | Required in `body.attributes.script_category` when `ValidationMode::Script` |
| `beat` | `beat` | Required |
| `act` | `act` | Required |

### 7.3 ValidationMode::Script

New variant in `ValidationMode` enum:

- **Accepts** `script_category` in `body.attributes` (one of three valid values).
- **Rejects** `novel_category` and `game_bible_category` in `body.attributes` (validates as absent).
- **Reuses** existing `character`, `scene`, `organization`, `event` BlockType variants for cross-domain concepts.
- `canonical_name` validation is identical to novel and generic modes.

---

## 8. Completion

A script Work is complete when:

```text
Scripts/script.md section_status == accepted
AND Beats/beat-sheet.md section_status == accepted
AND works.intake_status == complete
→ works.status = completed
```

| Condition | Check |
| --- | --- |
| Critical sections accepted | `script.md`, `beat-sheet.md` must have `section_status: accepted` |
| Intake complete | `intake_status == complete` |
| Completion gate | `complete_work_if_done` returns `Ok(false)` for script in V1.55 (completion detection deferred to V1.56+) |

**V1.55 scope**: Completion detection is **not** active for script. `complete_work_if_done` returns `Ok(false)` as an explicit gate.

---

## 9. Profile Registration

Script uses `work_profile = "script"` (TEXT column). Registration:

- `nexus42 creator bootstrap --profile script` auto-selects `--init-preset script-init`
- No auto-chain after init
- No primary production preset in V1.55; `script-writing` is V1.56+

### 9.1 Init Preset: `script-init`

- `run_intents: [work_init]` — one-shot scaffold only
- Scaffolds `Works/<work_ref>/` directory tree with 3 template files + `README.md`
- Creates `Logs/write/` and `Logs/review/` directories
- Sets `work_profile: script` on the Work record
- No ACP conversation turns in V1.55 (scaffold is file-system only)

---

## 10. Profile Differences Summary

| Dimension | Novel | Essay | Game-Bible | Script (V1.55) |
| --- | --- | --- | --- | --- |
| Init preset | `novel-project-init` | `essay-init` | `game-bible-init` | `script-init` |
| Auto-chain | Yes | No | No | No |
| Uses `work_chapters`? | Yes | No | No | No |
| Content layout | `Stories/` + `Outlines/` | `Drafts/` + `Outlines/` | `Design/` (12 sections) | `Scripts/` + `Beats/` + `Characters/` |
| World binding | Required | Optional | Optional | Optional |
| KB ValidationMode | `Novel` | `Generic` fallthrough | `GameBible` | `Script` (new) |
| Category field | `novel_category` | None | `game_bible_category` | `script_category` |
| Completion criteria | All chapters finalized | Draft status == finalized | All critical sections accepted | Critical sections accepted |
| Quality gate | 五问 | Lightweight thesis check | Design rubric (V1.55+) | Script rubric (V1.56+) |

---

## 11. Acceptance (V1.55 draft)

1. Script layout is distinct from novel (`Stories/`), essay (`Drafts/`), and game-bible (`Design/`) — uses `Scripts/` + `Beats/` + `Characters/`.
2. Stage chain is section-based, not chapter-based.
3. `complete_work_if_done` returns `Ok(false)` for script (explicit gate until V1.56+).
4. World binding is optional; no per-Work Worldbuilding subtree.
5. Three new `BlockType` variants are registered in wire schema with `script_category` mapping.
6. `ValidationMode::Script` accepts script categories and rejects novel/game-bible categories.
7. `nexus42 creator bootstrap --profile script` creates complete scaffold with 3 templates.
8. No novel-specific code paths execute for script Works (profile gate verified by unit tests).

---

## 12. Future Roadmap (V1.56+)

| Feature | Description | Depends on |
| --- | --- | --- |
| `script-writing` preset | LLM-driven scene/dialogue drafting with review loop | V1.55 scaffold |
| Section completion detection | Daemon evaluates `section_status` frontmatter | V1.55 section model |
| KB extraction for script | LLM extracts script facts into World KB with `script_category` | V1.55 BlockType + ValidationMode |
| Script auto-chain | Optional stage sequencing for multi-scene production | V1.55 preset stubs |
| Screenplay export | Formatting normalization + FDX/PDF export | V1.55 scaffold |

---

*Draft V1.55 Feature line. Implementation authority is active while V1.55 compass is active; P-last may promote to Master.*

