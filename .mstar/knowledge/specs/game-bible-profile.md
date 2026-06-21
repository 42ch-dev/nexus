# Game-Bible Profile — Draft Specification V1.54

**Status**: Draft (V1.55 P2 in progress)  
**Document class**: Feature line  
**Created**: 2026-06-22  
**Scope**: `work_profile: game_bible` on generic **Work** — artifact layout under `Works/<work_ref>/Design/`, section status model, stage chain, KB taxonomy, completion semantics  
**Coordinates with**:

- [essay-profile.md](essay-profile.md) — first non-novel Feature line pattern (structural template)
- [novel-writing/workflow-profile.md](novel-writing/workflow-profile.md) — novel baseline (contrast reference)
- [work-experience-model.md](work-experience-model.md) — generic Work entity
- [creator-workflow.md](creator-workflow.md) — FL-E stage model
- [cli-spec.md](cli-spec.md) — creator entry and workspace layout
- [orchestration-engine.md](orchestration-engine.md) — preset execution model
- [entity-scope-model.md](entity-scope-model.md) — World/KB binding and BlockType taxonomy
- [non-novel-profiles-roadmap.md](non-novel-profiles-roadmap.md) — prior Exploration (now promoted)

**Plan**: [2026-06-22-v1.54-game-bible-scaffold.md](../../plans/2026-06-22-v1.54-game-bible-scaffold.md)

---

## 1. Purpose

`work_profile: game_bible` is the second non-novel creative profile in Nexus OSS (following `essay`). It covers durable design documentation for a game or interactive narrative: design pillars, characters, factions, species, locations, mechanics, magic systems, technology, economy, progression, and lore. The Work is a multi-section reference artifact — not a prose manuscript with chapters or volumes.

The profile intentionally mirrors `essay-profile.md` and `novel-writing/workflow-profile.md` structure where useful, but it does not use `work_chapters`, `Stories/`, or `Outlines/`. Its durable slice is twelve Design template files under `Works/<work_ref>/Design/`.

---

## 2. Relationship to Work

| Concept | Generic Work | Game-Bible profile (`work_profile: game_bible`) | Novel contrast |
| --- | --- | --- | --- |
| Identity | `work_id`, `creator_id`, `workspace_slug` | Same | Same |
| Human slug | optional `story_ref` / `work_ref` | **`work_ref`** = directory under `Works/` | Same filesystem root |
| Status | `draft` \| `active` \| `paused` \| `completed` \| `archived` | Work-level status; section status tracked per Design file frontmatter | Novel uses `work_chapters` rows |
| Intake | `creative_brief` | Required before design production | Same FL-E intake |
| Produce preset | profile-specific | `design-writing` (stub V1.54; full LLM-driven in V1.55+) | Novel uses `novel-writing` |
| Completion | generic goal met | §8 — all critical sections accepted | Novel requires all chapters finalized |

Game-bible Works do not create or read `work_chapters`. Any code path that assumes a chapter table must gate on `work_profile == novel` before applying novel-specific behavior.

---

## 3. Artifact Layout

```text
<workspace>/
  Works/
    <work_ref>/
      README.md                  # Core pillars, genre, tone, target audience
      Design/
        overview.md              # North star section (required for completion)
        pillars.md               # Design pillars, constraints
        characters.md            # Character roles, archetypes
        factions.md              # Factions, politics, alignment
        species.md               # Sapient species, traits
        locations.md             # World geography, levels
        mechanics.md             # Core mechanics / gameplay loops
        magic_system.md          # Magic / superpower system rules
        technology.md            # Tech level, tools, artifacts
        economy.md               # Currency, trade, resources
        progression.md           # Leveling, skill trees, unlocks
        lore.md                  # History, mythology, cosmology
      Logs/
        design/
        review/
```

| Path | Sync manuscript? | Purpose |
| --- | --- | --- |
| `Works/<work_ref>/README.md` | No | Human overview: pillars, genre, tone, target audience |
| `Works/<work_ref>/Design/overview.md` | No | North star: project vision, core loop summary, target experience |
| `Works/<work_ref>/Design/pillars.md` | No | Design pillars, constraints, and non-goals |
| `Works/<work_ref>/Design/characters.md` | No | Character roles, archetypes, relationships |
| `Works/<work_ref>/Design/factions.md` | No | Factions, politics, alignment |
| `Works/<work_ref>/Design/species.md` | No | Sapient species, traits, cultures |
| `Works/<work_ref>/Design/locations.md` | No | World geography, levels, biomes |
| `Works/<work_ref>/Design/mechanics.md` | No | Core mechanics and gameplay loops |
| `Works/<work_ref>/Design/magic_system.md` | No | Magic/superpower system rules and constraints |
| `Works/<work_ref>/Design/technology.md` | No | Tech level, tools, artifacts |
| `Works/<work_ref>/Design/economy.md` | No | Currency, trade, resources, sinks |
| `Works/<work_ref>/Design/progression.md` | No | Leveling, skill trees, unlocks, player growth |
| `Works/<work_ref>/Design/lore.md` | No | History, mythology, cosmology |
| `Works/<work_ref>/Logs/design/` | No | Design iteration logs |
| `Works/<work_ref>/Logs/review/` | No | Review evidence and feedback |

Game-bible profiles **must not** use `Stories/`, `Outlines/`, or `Drafts/` unless a future plan explicitly changes the profile. Those directories remain novel- and essay-specific.

**Not scaffolded in V1.54**: `work_chapters` rows, `volume-outline.md`, `foreshadowing.md`, `event-index.md`, `Stories/`, `Outlines/` — these are novel-specific.

---

## 4. Templates

### 4.1 Section Frontmatter (all Design/*.md files)

Each `Design/*.md` carries YAML frontmatter:

```yaml
---
section_status: draft | reviewed | accepted
section_weight: critical | important | nice_to_have
last_updated: <ISO 8601 datetime>
---
```

| Field | Values | Purpose |
| --- | --- | --- |
| `section_status` | `draft`, `reviewed`, `accepted` | Per-section maturity — analogous to chapter status but for design documents |
| `section_weight` | `critical`, `important`, `nice_to_have` | Priority tier — `critical` sections gate completion |
| `last_updated` | ISO 8601 datetime | Last substantive edit timestamp |

### 4.2 Template Stubs (V1.54 init)

Each Design file is initialized as a minimally-structured stub:

**`Design/overview.md`**:
```yaml
---
section_status: draft
section_weight: critical
---
# Overview
<!-- Project vision, core loop summary, target audience -->
```

**`Design/pillars.md`**:
```yaml
---
section_status: draft
section_weight: critical
---
# Design Pillars
<!-- Core constraints, guiding principles, non-goals -->
```

**`Design/characters.md`**:
```yaml
---
section_status: draft
section_weight: important
---
# Characters
<!-- Character roles, archetypes, relationships -->
```

**`Design/factions.md`**:
```yaml
---
section_status: draft
section_weight: important
---
# Factions
<!-- Factions, politics, alignment, conflicts -->
```

**`Design/species.md`**:
```yaml
---
section_status: draft
section_weight: important
---
# Species
<!-- Sapient species, traits, cultures, biology -->
```

**`Design/locations.md`**:
```yaml
---
section_status: draft
section_weight: important
---
# Locations
<!-- World geography, levels, biomes, maps -->
```

**`Design/mechanics.md`**:
```yaml
---
section_status: draft
section_weight: critical
---
# Mechanics
<!-- Core mechanics, gameplay loops, systems -->
```

**`Design/magic_system.md`**:
```yaml
---
section_status: draft
section_weight: important
---
# Magic System
<!-- Magic/superpower rules, constraints, costs -->
```

**`Design/technology.md`**:
```yaml
---
section_status: draft
section_weight: important
---
# Technology
<!-- Tech level, tools, artifacts, research -->
```

**`Design/economy.md`**:
```yaml
---
section_status: draft
section_weight: important
---
# Economy
<!-- Currency, trade, resources, sinks, balance -->
```

**`Design/progression.md`**:
```yaml
---
section_status: draft
section_weight: important
---
# Progression
<!-- Leveling, skill trees, unlocks, player growth -->
```

**`Design/lore.md`**:
```yaml
---
section_status: draft
section_weight: nice_to_have
---
# Lore
<!-- History, mythology, cosmology, legends -->
```

---

## 5. Stage Chain

```text
intake → design → review → iterate
```

| Stage | Artifact | Gate |
| --- | --- | --- |
| `intake` | Work creative brief | Required before design production |
| `design` | Design sections drafted | Intake complete; `design-writing` preset runs (V1.55+) |
| `review` | Sections reviewed against quality rubric | Author or agent review of critical sections |
| `iterate` | Sections revised | Review feedback applied; cycle back to `design` or `review` |

Game-bible does **not** enforce a linear chapter-by-chapter progression. Stage transitions are profile-defined; the daemon does not auto-chain game-bible stages in V1.54.

---

## 6. World Integration

Game-bible Works may optionally bind to a World when the design documents reference a specific fictional setting. Binding is **optional**, not mandatory.

When bound:
- The Work's `world_id` links to the World aggregate.
- KB entries created from design facts use `game_bible_category` (see §7 KB Taxonomy).
- Future V1.55+ `design-writing` preset may pull World KB context for consistency checks.

The game-bible profile must not introduce a per-Work `Worldbuilding/` subtree. Cross-Work facts remain in World KB per [entity-scope-model.md](entity-scope-model.md).

---

## 7. KB Taxonomy

### 7.1 New BlockType Variants (V1.54)

Seven new `BlockType` wire enum variants are added to `schemas/common/common.schema.json`:

| Wire name (`snake_case`) | UI label | `game_bible_category` | Primary Design section |
| --- | --- | --- | --- |
| `species` | Species | `species` | `species.md` |
| `faction` | Faction | `faction` | `factions.md` |
| `magic_system` | Magic System | `magic_system` | `magic_system.md` |
| `technology` | Technology | `technology` | `technology.md` |
| `deity` | Deity | `deity` | `lore.md` |
| `level` | Level | `level` | `locations.md` |
| `economy_tier` | Economy Tier | `economy_tier` | `economy.md` |

### 7.2 Game-Bible Category Mapping

`nexus-kb::validation` maps `game_bible_category` → `BlockType`:

| `game_bible_category` | Wire `BlockType` | Validation |
| --- | --- | --- |
| `species` | `species` | Required in `body.attributes.game_bible_category` when `ValidationMode::GameBible` |
| `faction` | `faction` | Required |
| `magic_system` | `magic_system` | Required |
| `technology` | `technology` | Required |
| `deity` | `deity` | Required |
| `level` | `level` | Required |
| `economy_tier` | `economy_tier` | Required |

Existing `BlockType` variants (`character`, `ability`, `scene`, `organization`, `item`, `conflict`, `info_point`, `event`) are reused for cross-domain concepts. For example, a game character can use `BlockType::Character` with `game_bible_category: "character"`.

### 7.3 ValidationMode::GameBible

New variant in `ValidationMode` enum (see entity-scope-model.md §5.1.1):

- **Accepts** `game_bible_category` in `body.attributes` (one of the seven valid values).
- **Rejects** `novel_category` in `body.attributes` (validates as absent, not as invalid).
- **Reuses** existing `character`, `organization`, `scene`, `item`, `conflict`, `info_point`, `event` BlockType variants for cross-domain concepts.
- `canonical_name` validation is identical to novel and generic modes.

---

## 8. Completion

A game-bible Work is complete when:

```text
All Design/*.md files with section_weight == critical have section_status == accepted
AND works.intake_status == complete
→ works.status = completed
```

| Condition | Check |
| --- | --- |
| Critical sections accepted | `overview.md`, `pillars.md`, `mechanics.md` must have `section_status: accepted` |
| Intake complete | `intake_status == complete` |
| Completion gate | `complete_work_if_done` returns `Ok(false)` for game-bible in V1.54 (section completion detection deferred to V1.55+) |

**V1.54 scope**: Completion detection is **not** active for game-bible. `complete_work_if_done` returns `Ok(false)` as an explicit gate. Section status model is defined but the daemon does not evaluate it. Completion is manual via `creator works complete` command.

**V1.55+**: Daemon evaluates `section_status` frontmatter across all Design files. `complete_work_if_done` returns `Ok(true)` when all critical sections are `accepted`.

---

## 9. Profile Registration

Game-bible uses `work_profile = "game_bible"` (TEXT column, consistent with `"novel"` and `"essay"`). Registration:

- `nexus42 creator bootstrap --profile game-bible` auto-selects `--init-preset game-bible-init`
- No auto-chain after init (unlike novel's `--chain-novel-writing`)
- No primary production preset in V1.54; `design-writing` stub only

### 9.1 Init Preset: `game-bible-init`

- `run_intents: [work_init]` — one-shot scaffold only
- Scaffolds `Works/<work_ref>/` directory tree with 12 Design template files + `README.md`
- Creates `Logs/design/` and `Logs/review/` directories
- Sets `work_profile: game_bible` on the Work record
- No ACP conversation turns in V1.54 (scaffold is file-system only; V1.55+ adds interactive grill-me)

---

## 10. Profile Differences Summary

| Dimension | Novel | Essay | Game-Bible (V1.54) |
| --- | --- | --- | --- |
| Init preset | `novel-project-init` | `essay-init` | `game-bible-init` |
| Auto-chain | Yes | No | No |
| Uses `work_chapters`? | Yes | No | No |
| Content layout | `Stories/` + `Outlines/` | `Drafts/` + `Outlines/` | `Design/` (12 sections) |
| World binding | Required | Optional | Optional (highly recommended) |
| KB ValidationMode | `Novel` | `Generic` fallthrough | `GameBible` (new) |
| Category field | `novel_category` | None | `game_bible_category` |
| Completion criteria | All chapters finalized | Draft status == finalized | All critical sections accepted |
| Quality gate | 五问 | Lightweight thesis check | Design rubric (V1.55+) |

---

## 11. Acceptance (V1.54 draft)

1. Game-bible layout is distinct from novel (`Stories/`) and essay (`Drafts/`) — uses `Design/` with 12 section templates.
2. Stage chain is section-based, not chapter-based.
3. `complete_work_if_done` returns `Ok(false)` for game-bible (explicit gate until V1.55+ section detection).
4. World binding is optional; no per-Work Worldbuilding subtree.
5. Seven new `BlockType` variants are registered in wire schema with `game_bible_category` mapping.
6. `ValidationMode::GameBible` accepts game-bible categories and rejects novel categories.
7. `nexus42 creator bootstrap --profile game-bible` creates complete `Design/` tree with 12 templates.
8. No novel-specific code paths execute for game-bible Works (profile gate verified by unit tests).

---

## 12. Future Roadmap (V1.55+)

| Feature | Description | Depends on |
| --- | --- | --- |
| `design-writing` preset | LLM-driven per-section drafting with review loop | V1.54 scaffold |
| Section completion detection | Daemon evaluates `section_status` frontmatter across all Design files | V1.54 section model |
| KB extraction for game-bible | LLM extracts design facts into World KB with `game_bible_category` | V1.54 BlockType + ValidationMode |
| Game-bible auto-chain | Optional stage sequencing for multi-section production | V1.54 preset stubs |
| Script profile scaffold | Follow game-bible pattern for script/dialogue Works | V1.54 pattern validated |

---

*Draft V1.54 Feature line. Implementation authority is active while V1.54 compass `v1.54-df46-completion-and-game-bible-foundation-delivery-compass-v1.md` is active; P-last promotes or revises.*
