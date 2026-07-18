---
module: apps/web + crates/nexus-daemon-runtime
date: 2026-07-18
problem_type: architecture_pattern
category: architecture-patterns
severity: medium
plan_id: "2026-07-18-v1.122-timeline-first-canvas (V1.122 P1; compound of timeline-canvas-architecture.md)"
tags: [canvas, scope, world-vs-work, data-composition, spine-projection, timeline]
applies_when:
  - "Deciding which data sources a Canvas surface should compose"
  - "Reviewing whether a surface should ingest World-scoped vs Work-scoped data"
  - "Adding a new Canvas surface and determining its scope boundary"
---

# World vs Work Scope Discipline for Canvas Surfaces

**Track**: Knowledge (durable guidance, distilled from V1.122 P1 architect-locked decision for the Timeline hero surface).

## Context

The Nexus domain model divides the creative container into two scopes:

| Scope | Core concept | Examples | Surface kinds |
|-------|-------------|----------|---------------|
| **World** | Narrative universe — the spine | KeyBlocks, relationships, events, Forks, timeline | `world-kb`, `timeline` |
| **Work** | Projection of a World into a writing plan | Outline chapters/scenes, manuscript, chapter-relative events | `work-outline-timeline`, `strategy` |

During V1.122, the architect decided whether the Timeline hero surface should compose Work-scoped data (chapter-relative timeline events from `timeline.patch_event`). The answer was **no**, establishing the scope discipline documented here.

## Guidance

### 1. A Canvas Surface Has One Scope

Every canvas surface is scoped to either **World** or **Work**. A surface must not cross-compose data from different scopes.

**World-scoped surfaces** read from World-level endpoints:
- `GET /v1/daemon/worlds/{world_id}/kb/graph` -> `WorldKbGraphResponse`
- `GET /v1/daemon/narrative/worlds/{world_id}` -> `WorldState`

**Work-scoped surfaces** read from Work-level endpoints:
- `GET /v1/daemon/works/{work_id}/outline` -> outline data
- `POST /v1/daemon/works/{work_id}/timeline/patch` -> chapter-relative events

### 2. Do Not Cross-Compose Scoped Data Sources

Composing a Work-scoped data source onto a World-scoped surface introduces:

- **N+1 fetch risk:** each Work bound to the World requires a separate fetch.
- **No merge key:** Work-scoped events are chapter-relative (`realizes_chapter_id`) — they have no World-level merge key.
- **Scope confusion:** the surface's purpose (World building) is diluted by Work projection data.

**Exception:** A surface may reference the other scope's data in **chrome only** (e.g., a header badge showing "Fork of `<parent_world_id>`" from `WorldState`). This is metadata chrome, not data composition.

### 3. The Spine/Projection Model Determines Scope

```
World (spine)                          Work (projection onto a World)
├── Timeline (hero)                    ├── Outline (default for Work entry)
├── World KB (peer)                    ├── Manuscript / Reading
└── Forks (read-only markers)          └── Chapter-relative timeline events
```

- **Spine surfaces** are World-scoped — they show the truth of the narrative universe.
- **Projection surfaces** are Work-scoped — they show authoring plans bound to a World.

### 4. Scope is Additive, Not Migrational

A surface's scope is set at creation and does not change. Do not start a surface as Work-scoped and later "upgrade" it to World-scoped by adding World data sources — this creates a scope-mixed surface that is unclear in purpose.

### 5. Empty State Reflects Scope

A World-scoped surface shows World-level empty-state copy ("This World has no entities yet..."). A Work-scoped surface shows Work-level copy ("This Work has no outline yet..."). Do not cross-scope empty-state copy.

## Why This Matters

- **Performance:** avoiding N+1 fetches per bound Work keeps the surface responsive.
- **Clarity:** authors know which surface serves which purpose (World building vs Work planning). The two entry defaults (World -> Timeline, Work -> Outline) are unambiguous.
- **Maintainability:** a single-scope surface is easier to test, debug, and evolve. Cross-scope composition creates hidden coupling between World and Work data models.
- **Wire contract stability:** single-scope surfaces reuse existing DTOs within that scope, avoiding new cross-scope aggregation endpoints.

## When to Apply

- Designing a new Canvas surface — determine its scope before writing any code.
- Reviewing a surface's data composition plan — verify it does not cross scopes.
- Adding a feature to an existing surface — if the feature requires data from the other scope, consider chrome-only or a new surface.

## Examples

### V1.122: Timeline (World-scoped) — correct

| Decision | Rationale |
|----------|-----------|
| **Scope:** World | Timeline is the "when-axis" of the World's narrative universe |
| **Data source:** `WorldKbGraphResponse` only | Single World-level endpoint |
| **Work-scoped events:** not composed | Chapter-relative, no merge key, N+1 fetches |
| **Fork chrome:** optional `WorldState` sidecar | Header badge, not data composition |
| **Empty-state:** "This World has no entities yet" | World-level copy |

### What Didn't Work

- **Composing Work-scoped timeline events onto World Timeline:** `timeline.patch_event` events are chapter-relative (`realizes_chapter_id`), have no World-level merge key, and would require N+1 fetches per bound Work. Rejected in favor of honest empty-state.
- **Creating a new `GET /v1/daemon/worlds/{world_id}/timeline` route:** deferred — would require daemon Rust changes + a new external route, violating the `wire_contracts_changed: false` constraint. Tracked under `DF-V1122-DEEPER-WB`.