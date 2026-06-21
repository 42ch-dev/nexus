# Non-Novel Profiles Roadmap — Exploration v1

**Status**: Exploration (V1.52 — no implementation authority). **Game-bible promoted to Draft in V1.54; V1.55 P2 Depth 3.5 in progress** per [game-bible-profile.md](game-bible-profile.md).  
**Document class**: Exploration  
**Created**: 2026-06-19  
**Last updated**: 2026-06-22 (V1.55 P-1 — game-bible Depth 3.5 and script target activated)  
**Scope**: Future non-novel `work_profile` lines. **V1.55 status**: game-bible (V1.54 scaffold shipped, V1.55 P2 Depth 3.5 in progress) and script (V1.55 P3 scaffold in progress). Pre-V1.55 backlog (V1.36 V2.0+) is preserved in deferred tracker §3.3.  
**Coordinates with**:

- [essay-profile.md](essay-profile.md) — first V1.52 non-novel Feature line
- [novel-writing/workflow-profile.md](novel-writing/workflow-profile.md) — novel profile baseline
- [work-experience-model.md](work-experience-model.md) — generic Work entity
- [orchestration-engine.md](orchestration-engine.md) — preset/stage runtime

**Iteration compass**: [v1.52-author-completion-and-multi-branch-preset-orchestration-delivery-compass-v1.md](../iterations/v1.52-author-completion-and-multi-branch-preset-orchestration-delivery-compass-v1.md)

---

## 1. Game-bible profile

### 1.1 Purpose

`work_profile: game-bible` would support durable design documentation for a game or interactive narrative: pillars, mechanics, characters, levels/areas, economy, progression, and production constraints.

### 1.2 Relationship to Work

Unlike an essay, a game bible is a multi-section reference artifact rather than a single manuscript. Unlike a novel, it does not progress by chapters or volumes. The Work is the container for evolving design truth.

### 1.3 Key differences from novel

| Dimension | Novel | Game bible |
| --- | --- | --- |
| Primary artifact | Chapter正文 | Design sections |
| Progression | chapter/volume | section maturity |
| Completion | all chapters finalized | all required sections accepted |
| World KB | narrative continuity | design facts + mechanics + entities |

### 1.4 Implementation considerations

- Likely layout: `Design/overview.md`, `Design/mechanics.md`, `Design/characters.md`, `Design/levels.md`, `Design/economy.md`.
- Needs section-level status rather than chapter-level status.
- May benefit from World KB, but mechanics/economy facts need a category mapping distinct from novel's seven-category taxonomy.
- Requires a dedicated quality rubric; novel 五问 is not applicable.

### 1.5 Status

**P1 Scaffold Shipped V1.54; V1.55 P2 Depth 3.5 in progress** — game-bible profile scaffold delivered in [plan `2026-06-22-v1.54-game-bible-scaffold`](../../plans/2026-06-22-v1.54-game-bible-scaffold.md). V1.55 P2 targets `design-writing`, design 五问 quality rubric, section completion detection, and KB extraction in [plan `2026-06-22-v1.55-game-bible-depth-35`](../../plans/2026-06-22-v1.55-game-bible-depth-35.md). Feature line spec: [game-bible-profile.md](game-bible-profile.md).

| Deliverable | Status |
| --- | --- |
| `specs/game-bible-profile.md` Draft | ✓ |
| 7 new `BlockType` variants in wire schema | ✓ |
| `game-bible-init` preset | ✓ |
| `nexus42 creator bootstrap --profile game-bible` | ✓ |
| `ValidationMode::GameBible` | ✓ |
| Profile gates (non-novel `work_chapters` + completion) | ✓ |
| KB taxonomy (`game_bible_category` mapping) | ✓ |
| `design-writing` preset (LLM-driven) | V1.55 P2 in progress |
| Design 五问 quality rubric | V1.55 P2 in progress |
| Section completion detection | V1.55 P2 in progress |
| KB extraction for game-bible | V1.55 P2 in progress |

---

## 2. Script profile

### 2.1 Purpose

`work_profile: script` would support screen/stage/audio script production: scenes, dialogue, beats, character directions, and revision passes.

### 2.2 Relationship to Work

A script Work is closer to novel than essay because it has ordered scenes, but its primary units are scenes/beats rather than chapters. Its artifacts need strong formatting conventions and potentially multiple output formats.

### 2.3 Key differences from novel

| Dimension | Novel | Script |
| --- | --- | --- |
| Primary artifact | prose chapter | scene/script document |
| Unit | chapter/volume | scene/beat/act |
| Quality gate | prose 五问 | dialogue/scene/format rubric |
| Layout | `Stories/` + `Outlines/` | likely `Scripts/` + `Beats/` |

### 2.4 Implementation considerations

- Likely layout: `Scripts/script.md`, `Beats/beat-sheet.md`, `Characters/` if not fully World KB-backed.
- Needs formatting normalization (scene headings, dialogue, action lines) before any platform/export path.
- World KB can supply characters/locations, but script-specific scene continuity requires a separate scene index.
- Should not reuse `work_chapters` without a clear migration to generic `work_units` or profile-specific scene rows.

### 2.5 Status

**P3 Scaffold Implemented V1.55** — script profile scaffold delivered in [plan `2026-06-22-v1.55-script-scaffold`](../../plans/2026-06-22-v1.55-script-scaffold.md). Feature line spec: [script-profile.md](script-profile.md) (Draft V1.55).

| Deliverable | Status |
| --- | --- |
| `specs/script-profile.md` Draft | ✓ |
| 3 new `BlockType` variants in wire schema (`dialogue`, `beat`, `act`) | ✓ |
| `script-init` preset | ✓ |
| `nexus42 creator bootstrap --profile script` | ✓ |
| `ValidationMode::Script` | ✓ |
| `script_category` taxonomy mapping | ✓ |
| `ScaffoldTransaction` pattern applied (R-V154P1-W001 closed) | ✓ |
| Full script-writing run-loop (`script-writing` preset) | V1.56+ |
| Script screenplay export / formatting normalization | V1.56+ |
| KB extraction for script | V1.56+ |

---

## 3. Roadmap guardrails

1. Do not add runtime code for `game-bible` or `script` in V1.52.
2. Do not create new `schemas/` wire contracts for these profiles from this Exploration.
3. Promote one profile at a time via a future compass, then create a Feature line spec with layout, stage chain, frontmatter, completion, and acceptance.
4. Preserve `novel-writing/` as the novel-only subtree; future non-novel profile specs stay flat unless an ADR authorizes a new subtree.

---

*Exploration only. Active implementation authority remains with V1.52 `essay-profile.md` and later locked compasses.*
