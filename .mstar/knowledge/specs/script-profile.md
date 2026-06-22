# Script Profile — Draft Specification V1.60

**Status**: Draft (V1.60 P1)
**Document class**: Feature line
**Created**: 2026-06-22
**Last updated**: 2026-06-23 (V1.60 P1: Depth 3.5 sections — script-writing preset, 五问 rubric, section completion, KB extraction contract)
**Scope**: `work_profile: script` on generic **Work** — artifact layout under `Works/<work_ref>/`, stage chain, preset chain, quality rubric, KB taxonomy, completion semantics
**Coordinates with**:

- [essay-profile.md](essay-profile.md) — first non-novel Feature line pattern (structural template)
- [game-bible-profile.md](game-bible-profile.md) — second non-novel profile (Depth 3.5 Master — template for this spec)
- [novel-writing/workflow-profile.md](novel-writing/workflow-profile.md) — novel baseline (contrast reference)
- [work-experience-model.md](work-experience-model.md) — generic Work entity
- [creator-workflow.md](creator-workflow.md) — FL-E stage model
- [cli-spec.md](cli-spec.md) — creator entry and workspace layout
- [orchestration-engine.md](orchestration-engine.md) — preset execution model
- [entity-scope-model.md](entity-scope-model.md) — World/KB binding and BlockType taxonomy
- [non-novel-profiles-roadmap.md](non-novel-profiles-roadmap.md) — prior Exploration (now promoted)

**Plan**: [2026-06-22-v1.60-script-depth-35.md](../../plans/2026-06-22-v1.60-script-depth-35.md)

---

## 1. Purpose

`work_profile: script` is the third non-novel creative profile in Nexus OSS (following `essay` and `game_bible`). It covers screen/stage/audio script production: acts, beats, dialogue, character directions, and revision passes.

The profile intentionally mirrors `game-bible-profile.md` and `essay-profile.md` structure where useful, but it does not use `work_chapters`, `Stories/`, or `Outlines/` (novel-specific paths). Its durable slice is a script scaffold with template files under `Works/<work_ref>/`.

---

## 2. Relationship to Work

| Concept | Generic Work | Script profile (`work_profile: script`) | Novel contrast |
| --- | --- | --- | --- |
| Identity | `work_id`, `creator_id`, `workspace_slug` | Same | Same |
| Human slug | optional `story_ref` / `work_ref` | **`work_ref`** = directory under `Works/` | Same filesystem root |
| Status | `draft` \| `active` \| `paused` \| `completed` \| `archived` | Work-level status; section status tracked per frontmatter | Novel uses `work_chapters` rows |
| Intake | `creative_brief` | Required before script drafting | Same FL-E intake |
| Produce preset | profile-specific | `script-writing` (V1.60 P1; LLM-driven act/dialogue drafting) | Novel uses `novel-writing` |
| Completion | generic goal met | §8 — all critical script sections accepted | Novel requires all chapters finalized |

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

### 4.1 Section Frontmatter (all script files)

Each scaffolded script file carries YAML frontmatter:

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

**Section lifecycle (V1.60 P1)**:

```
draft → reviewed → accepted
```

- **`draft → reviewed`**: Auto-transition by the `script-writing` preset after a review pass (GO). The `script.section_status.update` capability writes the new frontmatter atomically via temp+rename.
- **`reviewed → accepted`**: Explicit author accept — invoked manually or via preset input flag.
- **No skipping**: `draft → accepted` is rejected.
- **No backwards**: `accepted → reviewed` / `accepted → draft` are rejected.
- **Atomicity**: Frontmatter writes use temp+rename; no half-written file survives a crash. All other frontmatter fields (`section_weight`, etc.) and body content are preserved.

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

## 5. Stage Chain (V1.60 P1 — `script-writing` preset)

```text
intake → outline → draft → revise → finalize
```

| Stage | Artifact | Gate |
| --- | --- | --- |
| `intake` | Work creative brief | Required before script drafting |
| `outline` | Act structure, beat sheet, character setup | Intake complete; `script-writing` preset: outline stage |
| `draft` | Scene + dialogue generation per act | Outline approved (GO on outline review) |
| `revise` | Sections revised per feedback | Review feedback applied; `script-writing` preset: revise stage |
| `finalize` | Polish + KB extraction + 五问 exit judgment | All acts drafted and revised; 五问 all-pass triggers stop |

Script does **not** enforce linear scene-by-scene progression. Stage transitions are profile-defined; the `script-writing` preset drives the stage chain via `outline` → `draft` → `revise` → `finalize`, each gated by an LLM judge exit condition. The preset is registered at `embedded-presets/script-writing/preset.yaml` (V1.60 P1).

### 5.1 `script-writing` Preset Chain (V1.60 P1)

```text
outline → draft → revise → finalize → done
```

| State | Description | Exit condition |
| --- | --- | --- |
| `outline` | Draft act structure, beat sheet, character setup | LLM judge: GO/NOGO on outline quality |
| `draft` | Generate scene + dialogue per act | LLM judge: GO/NOGO on draft quality |
| `revise` | Apply review feedback to the draft | LLM judge: GO/NOGO on revision quality |
| `finalize` | Polish + KB extraction hint + 五问 all-pass judgment | 五问 all-pass → GO; any NO → NOGO |
| `done` | Terminal state | — |

**Exit judge on `finalize`**: The 五问 script rubric (see §5.2) fires as the exit condition. All five dimensions must pass (YES) for GO. Any NO dimension causes NOGO with a specific reason, triggering a revise loop.

**Template variables** (passed via `preset.input`):

| Variable | Source | Purpose |
| --- | --- | --- |
| `{{preset.input.work_ref}}` | Work row `work_ref` | Directory slug under `Works/` |
| `{{preset.input.acts}}` | Beat sheet outline | Act count and structure |
| `{{preset.input.characters}}` | Character directions | Character names and traits |
| `{{preset.input.beats}}` | Beat sheet content | Story beats and scene outline |
| `{{preset.input.scene}}` | Current scene input | Per-scene generation context |

**Gates** (enforced at enqueue time):
- `work_profile: script`
- `work_ref: required`
- `intake_status: complete`
- Filesystem: `Works/<work_ref>/Scripts/` must exist

**Capabilities required**:
- `creator.inject_prompt` — inject prompt into the agent session
- `acp.prompt` — ACP conversation prompt
- `judge.llm` — LLM judge for exit conditions
- `script.section_status.update` — frontmatter auto-transition on review pass

### 5.2 五问 Quality Rubric (Script Domain)

The five quality dimensions for script content (mirroring game-bible's design rubric, adapted for script domain):

| # | Dimension | Question | Criteria |
| --- | --- | --- | --- |
| 1 | **Dialogue Coherence** | Is every line of dialogue character-appropriate and narratively motivated? | Each character speaks in a distinct voice; no line is pure exposition; every exchange either advances plot, reveals character, or builds tension. |
| 2 | **Beat Pacing** | Does each beat land at the right moment and drive the scene forward? | Beats follow a recognizable pattern (setup → conflict → turn); no beat overstays; no beat is skipped; emotional trajectory is coherent. |
| 3 | **Act Structure** | Does the act follow a recognizable structural arc (inciting incident → rising action → climax → resolution)? | Acts have clear beginning/middle/end; act breaks are motivated; the audience understands where they are in the narrative journey. |
| 4 | **Character Voice** | Is each character's voice distinct, consistent, and true to their established traits? | No character sounds like another; vocabulary, sentence length, and register vary by character; voice remains stable across scenes. |
| 5 | **Scene Economy** | Is every scene necessary? Does every scene earn its place in the narrative? | No redundant scenes; each scene either advances plot, deepens character, or enriches world; filler scenes are flagged and cut. |

**Judgment format**: The LLM judge responds with `GO` (all five YES) or `NOGO: <one-line reason>` (any dimension NO).

---

## 6. World Integration

Script Works may optionally bind to a World when the script references a specific fictional setting. Binding is **optional**, not mandatory.

When bound:
- The Work's `world_id` links to the World aggregate.
- KB entries created from script facts use `script_category` (see §7 KB Taxonomy).
- The `script-writing` preset pulls World KB context for character/location consistency on the `draft` stage.

The script profile must not introduce a per-Work `Worldbuilding/` subtree. Cross-Work facts remain in World KB per [entity-scope-model.md](entity-scope-model.md).

---

## 7. KB Taxonomy

### 7.1 BlockType Variants (V1.55 P3)

Three `BlockType` wire enum variants registered in `schemas/common/common.schema.json`:

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

**Profile-aware KB extraction (V1.60 P1)**: `candidate_from_llm_json_for_profile` with `work_profile: "script"` emits `attributes.script_category` via `block_type_to_script_category` mapping, tags `["script", "llm-extracted"]`, and omits `novel_category`/`game_bible_category`.

**Cross-domain reuse mapping** (`block_type_to_script_category`):

| Wire `block_type` | Mapped `script_category` | Rationale |
| --- | --- | --- |
| `dialogue` | `dialogue` | Direct (script BlockType) |
| `beat` | `beat` | Direct (script BlockType) |
| `act` | `act` | Direct (script BlockType) |
| `character` | `dialogue` | Characters express through dialogue |
| `scene` | `act` | Scenes belong to acts |
| `event` | `beat` | Events are beats in narrative |
| `organization` | `act` | Organizations anchor acts |
| `conflict` | `beat` | Conflict is beat-level tension |
| `info_point` | `dialogue` | Info conveyed through dialogue |
| other / unknown | `dialogue` | Safest default (dialogue is most generic script category) |

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
All critical script sections have section_status == accepted
AND works.intake_status == complete
→ works.status = completed
```

| Condition | Check |
| --- | --- |
| Critical sections accepted | `Scripts/script.md` and `Beats/beat-sheet.md` must have `section_status: accepted` |
| Intake complete | `intake_status == complete` |
| Completion gate | `is_work_completed` delegates to `is_script_complete` for `work_profile: script` |

**V1.60 P1**: Section completion detection is active via `is_script_complete` in `nexus-local-db::work_chapters`. The daemon evaluates `section_status` frontmatter across all critical script files when the Work is loaded. `complete_work_if_done` returns `Ok(true)` when both critical sections are `accepted` and intake is `complete`.

**Completion guardian**: `is_work_completed` adds a `work_profile == "script"` guard (mirroring the game-bible guard) that dispatches to `is_script_complete`. The novel chapter-completion logic never applies to script Works.

---

## 9. Profile Registration

Script uses `work_profile = "script"` (TEXT column). Registration:

- `nexus42 creator bootstrap --profile script` auto-selects `--init-preset script-init`
- No auto-chain after init
- Production preset: `script-writing` (V1.60 P1; embedded at `embedded-presets/script-writing/`)

### 9.1 Init Preset: `script-init`

- `run_intents: [work_init]` — one-shot scaffold only
- Scaffolds `Works/<work_ref>/` directory tree with 3 template files + `README.md`
- Creates `Logs/write/` and `Logs/review/` directories
- Sets `work_profile: script` on the Work record
- No ACP conversation turns in V1.55 (scaffold is file-system only)

### 9.2 Production Preset: `script-writing` (V1.60 P1)

- `run_intents: [work_continue]` — LLM-driven per-act drafting + review loop
- Stage chain: `outline` → `draft` → `revise` → `finalize` → `done`
- Gates: `work_profile: script`, `work_ref: required`, `intake_status: complete`, filesystem check on `Works/<work_ref>/Scripts/`
- Exit condition on `finalize`: 五问 judgment (dialogue coherence, beat pacing, act structure, character voice, scene economy)
- `script.section_status.update` capability auto-transitions `draft → reviewed` on GO
- Version: 1 (coordinated with `preset_version_for_id`)

---

## 10. Profile Differences Summary

| Dimension | Novel | Essay | Game-Bible | Script (V1.60) |
| --- | --- | --- | --- | --- |
| Init preset | `novel-project-init` | `essay-init` | `game-bible-init` | `script-init` |
| Auto-chain | Yes | No | No | No |
| Uses `work_chapters`? | Yes | No | No | No |
| Content layout | `Stories/` + `Outlines/` | `Drafts/` + `Outlines/` | `Design/` (12 sections) | `Scripts/` + `Beats/` + `Characters/` |
| World binding | Required | Optional | Optional | Optional |
| KB ValidationMode | `Novel` | `Generic` fallthrough | `GameBible` | `Script` |
| Category field | `novel_category` | None | `game_bible_category` | `script_category` |
| Completion criteria | All chapters finalized | Draft status == finalized | All critical sections accepted | Critical sections accepted |
| Quality gate | 五问 (prose) | Lightweight thesis check | Design rubric (Depth 3.5) | Script rubric (Depth 3.5) |
| Production preset | `novel-writing` | (none) | `design-writing` | `script-writing` |

---

## 11. Acceptance (V1.60 P1)

1. Script layout is distinct from novel (`Stories/`), essay (`Drafts/`), and game-bible (`Design/`) — uses `Scripts/` + `Beats/` + `Characters/`.
2. Stage chain is section-based, not chapter-based.
3. `script-writing` embedded preset exists with stage chain (`outline → draft → revise → finalize`) + gates + template variables.
4. Dialogue/beat/act 五问 quality rubric (5 dimensions: dialogue coherence, beat pacing, act structure, character voice, scene economy) defined and wired into `finalize` exit judge.
5. Section completion detection (`is_script_complete`) triggers stop when both critical sections are `accepted` and intake is `complete`.
6. Profile-aware KB extraction extends `candidate_from_llm_json_for_profile` for `work_profile: script` using `dialogue`/`beat`/`act` BlockType variants.
7. `script-writing` registered in `preset_version_for_id` SSOT + sync test `preset_version_matching_matches_yaml_includes_cron_presets` extended.
8. Three new `BlockType` variants registered in wire schema with `script_category` mapping (V1.55 P3).
9. `ValidationMode::Script` accepts script categories and rejects novel/game-bible categories (V1.55 P3).
10. No novel-specific code paths execute for script Works (profile gate verified by unit tests).
11. `nexus42 creator bootstrap --profile script` creates complete scaffold with 3 templates (V1.55 P3).
12. World binding is optional; no per-Work Worldbuilding subtree.

---

## 12. Future Roadmap (V1.61+)

| Feature | Description | Depends on |
| --- | --- | --- |
| Script auto-chain | Optional stage sequencing for multi-act production | V1.60 preset chain |
| Screenplay export | Formatting normalization + FDX/PDF export | V1.60 scaffold |
| Multi-character voice tracking | Per-character dialogue consistency scoring | V1.60 KB extraction |
| Real-time collaboration | Concurrent script editing with OCC | DF-52 workspace OCC |
| Script-to-storyboard pipeline | Beat → visual scene plan generation | V1.60 beat structure |

---

*Draft V1.60 Feature line. Implementation authority is active per V1.60 P1 plan; P-last T5 promotes to Master.*
