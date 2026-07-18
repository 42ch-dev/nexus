---
iteration_id: V1.122
start_date: 2026-07-18
status: locked
iteration_base_branch: main
target_branch: main
spec_integration_branch: iteration/v1.122
plans:
  - 2026-07-18-v1.122-three-pillar-spec-refactor
  - 2026-07-18-v1.122-timeline-first-canvas
---

# V1.122 Delivery Compass - Three-Pillar Pivot: Harness · Canvas · Computable

> **Direction lock mode: autonomous** (`/iteration-loop`, default scale **M** - 2 business plans).
> Locked direction, rationale, and candidate trade-offs are recorded below per `mstar-iteration` §1.2 autonomous + `references/autonomous-direction-lock.md`.
>
> **Phase 1 Review & Edit chain:** product-manager seat 1 (this pass) → architect seat 2 → writing-specialist seat 3 → PM lock. Direction is **locked** — do not re-question the three-pillar + Timeline-first pivot.

## Autonomous direction lock record

**Caller constraint (direction arg):** 产品重心放在 Local-first app 来找 PMF。三个核心关键词 **Harness + Canvas + Computable**，分别对应 Nexus OSS 端的控制策略、画布呈现、可计算模块三大特色；**Canvas 应以突出 Timeline 为核心的 World 构建为产品主要 Canvas 卖点**。在此基调下分析并推进：重构、product pivot、spec refactor 等；决定要做但推迟的记入 roadmap。

**Scale:** default **M** (no scale token in args) -> **2 business plans**. Harness process (Review chain, SDD, QC/QA, compound, close, PR, merge-ready) is not counted and not planned as business plans.

### Candidates evaluated

Research base: `STRATEGY.md` (vision + decision log, stale at V1.95), `CONCEPTS.md`, `specs/canvas-strategy-surface.md` (Shipped β V1.74 - 3 surfaces, Timeline bundled in Outline+Timeline), `specs/orchestration-engine.md`, `specs/compute-module-abi.md` + `specs/wasm-host.md` (Shipped V1.62), V1.114 canvas architecture foundation, V1.118 canvas-first work shell, V1.121 design elevation compass, `knowledge/deferred-features-cross-version-tracker.md` (DF-70/71/46/47 + BL-01..09 backlog).

| # | Candidate | Trade-off | Verdict |
|---|-----------|-----------|---------|
| A | **Three-Pillar Pivot** - spec/narrative refactor canonizing Harness/Canvas/Computable + Timeline-first Canvas code refactor (elevate Timeline to peer surface, World-building hero) | Touches canonical docs (STRATEGY/CONCEPTS/canvas spec) + canvas code; largest narrative blast radius but aligns everything with PMF direction and delivers a concrete hero surface | **LOCKED** - matches caller direction exactly (refactor + product pivot + spec refactor); evidence shows Timeline is currently a sub-lane, not a hero |
| B | Timeline-first Canvas implementation only (code-first, no spec/narrative refactor) | Concrete user-visible change; but misses the "product pivot / spec refactor" breadth caller named - the 3-pillar story stays untold | Rejected (partial - misses spec refactor + Harness/Computable framing) |
| C | Spec-only pivot (rewrite STRATEGY/CONCEPTS/canvas spec for 3 pillars + Timeline-first, no code) | Fast, low-risk, sets up future code; but no user-visible product change this iteration -> weak PMF signal | Rejected (insufficient - PMF needs product change, not just docs) |
| D | PMF Discovery Slice - ship one Timeline-first World-building flow end-to-end, light spec refactor | Most PMF-aligned single slice; but under-delivers on the "分析并推进 spec refactor" breadth and leaves Harness/Computable pillars unframed | Rejected (narrow - misses pillar framing the caller explicitly named) |

### Evidence base for A

- **Timeline is not the hero today.** `specs/canvas-strategy-surface.md` §3.3 lists 3 surfaces: Strategy, **Outline+Timeline** (bundled - Timeline is a *lane* within the outline surface, not a peer), World KB. `CanvasSurfaceKind = "strategy" | "work-outline-timeline" | "world-kb"` - no first-class `"timeline"`. The user's "Canvas 应以突出 Timeline 为核心的 World 构建为产品主要 Canvas 卖点" cannot be satisfied without elevating Timeline.
- **World entry today lands on World KB, not Timeline.** `apps/web` Worlds list navigates to `/worlds/:worldId/kb` (World KB canvas). Work entry (V1.118) lands on Outline (`/works/:workId` → `outline`). The hero pivot is a **World-entry** IA change, not a Work-entry change.
- **"Harness" is not a canonized concept.** `CONCEPTS.md` has no "Harness" entry. The closest is the orchestration engine + agent host + capability registry, surfaced in UI as "Strategy/Preset". The user's "Harness = control strategy" pillar needs naming/canonization.
- **"Computable" exists but is not a pillar.** `specs/compute-module-abi.md` + `specs/wasm-host.md` shipped V1.62 (WASM compute + combat-engine preset). V1.114 laid compute module foundation; V1.116 researched canvas-compute readiness. But `STRATEGY.md` Vision mentions "AI-agent-driven" without elevating Compute to a named pillar. The user's "Computable = 可计算模块" pillar needs canonization.
- **STRATEGY.md decision log is stale** - stops at V1.95 (2026-07-07); 26 iterations (V1.96-V1.121) unlogged. A product-pivot iteration must refresh the strategic narrative + decision log.
- **Canvas-first work shell already shipped** (V1.118 - Outline canvas is default enter-**work** UX). The infrastructure to make Timeline the World hero exists; what's missing is the surface extraction + World-building projection + World-entry default change.
- **Roadmap debt is real.** `deferred-features-cross-version-tracker.md` carries DF-70 (execution-mode matrix), DF-71 (desktop menu-bar), DF-46/47 (capability/tool registry), BL-01..09 (world merge, shadow read, context DSL, etc.) - all need re-homing under the 3-pillar framing per caller's "决定要做但推迟的要记入roadmap".

### Locked direction (single sentence)

Reframe Nexus around three pillars - **Harness** (control strategy / orchestration), **Canvas** (with **Timeline-centric World building** as the hero surface), and **Computable** (WASM compute) - by (P0) refactoring the strategic specs (`STRATEGY.md` + `CONCEPTS.md` + `specs/canvas-strategy-surface.md`) to canonize the pivot and (P1) elevating Timeline from an outline sub-lane to a first-class peer canvas surface that projects World-building entities (KeyBlocks/events/Forks) as the primary Canvas selling point, with all broader work (Harness UI rename, Computable pillar surfacing, deeper World-building, residual cleanup) recorded into the roadmap.

### Scale budget application

- **P0 (Must)** - Three-pillar spec & narrative refactor: `STRATEGY.md` decision log refresh + pillar canonization, `CONCEPTS.md` Harness/Computable/Timeline-first entries, `specs/canvas-strategy-surface.md` Timeline-as-peer-surface spec amendment, `specs/` corpus alignment, roadmap re-homing. **Business plan** (docs/spec deliverable).
- **P1 (Must)** - Timeline-first Canvas surface elevation (code refactor): extract Timeline from Outline+Timeline into a peer `CanvasSurfaceKind = "timeline"`, Timeline-centric World-building projection (KeyBlocks/events/Forks on a timeline axis), adapter + node/edge types + write-boundary reuse, Canvas IA so Timeline is the default/hero surface for **World entry** (`/worlds/:worldId`). **Business plan** (code deliverable).
- **Roadmap (deferred, not counted):** Harness UI rename (Strategy->Harness product copy), Computable pillar surfacing in UI, deeper World-building Timeline features (Fork creation UI, compute-on-timeline), V1.121 15 low/nit residuals, DF-70/71/46/47, BL-01..09 re-homing under pillars, `status.json` compaction (91KB > 20KB threshold). See **§ Roadmap Position → Deferred inventory**.

## Product story - one sentence + why

### One-sentence product thesis (post-V1.122)

> **Nexus is the local-first creative-writing tool where a World's Timeline is the central instrument, AI agents are harnessed through Canvas, and Computable modules make worlds react.**

If a reader cannot restate that sentence after reading this compass, the product story has failed.

### Why three pillars, why Timeline-first (PMF)

Nexus is a **local-first creative-writing tool** chasing PMF. After V1.121 the surfaces are functionally rich (Strategy / Outline+Timeline / WorldKB canvas, WASM compute, orchestration, SOUL memory, reading chrome, design system) but **narratively scattered**: the product reads as a feature list, not a thesis. Authors cannot tell what Nexus *is* in one sentence.

The caller's three pillars name the thesis Nexus has been building toward:

| Pillar | Means (author language) | Maps to today | Hero change this iteration |
|--------|-------------------------|---------------|---------------------------|
| **Harness** | How an author **harnesses** AI agents to execute creative work | Orchestration engine + agent host + capability registry + presets (UI still says "Strategy") | **P0:** canonize as a named pillar in STRATEGY/CONCEPTS. **UI rename → roadmap** (V1.123 candidate) |
| **Canvas** | The spatial steering surface where authors see and shape their world | React Flow canvas with 3 surfaces; Timeline is a *lane* inside Outline; World entry lands on World KB | **P0+P1:** elevate Timeline to peer hero surface for **World building** |
| **Computable** | The WASM layer that makes worlds *react* (not just store text) | `compute-module-abi` + `wasm-host` + combat-engine preset (V1.62) | **P0:** canonize as a named pillar. **UI surfacing / compute-on-timeline → roadmap** |

**Why Timeline-first World building as the Canvas hero:**

A creative-writing tool's core artifact is a **World** (`CONCEPTS.md`: "the core creative container - a narrative universe with its own knowledge base, timeline, and structured state"). Today:

1. The World's **Timeline** (`CONCEPTS.md`: "the ordered sequence of events and KeyBlocks in a world - the 'when' axis") is buried as a **lane** inside the Work Outline surface - authors meet chapter structure first, not world history.
2. Picking a World from the Worlds list opens **World KB** (`/worlds/:id/kb`) - a graph of entities, not the "when" axis. That is a valid surface, but it is not the product's selling thesis.

This inverts the product's own domain model: **World + Timeline are the spine; Outline + manuscript are projections of a Work onto that spine.**

Making Timeline the hero Canvas surface means: an author opens a World and **sees its history** - events, KeyBlocks realized on the when-axis, Fork markers - and steers world-building from there. Outline / Strategy / World KB remain peer surfaces; Timeline **leads World entry**. Work entry stays Outline-first (V1.118).

**PMF signals this iteration must produce (author-feelable, not docs-only):**

| Signal | What the author does | What they feel |
|--------|----------------------|----------------|
| **Demo path** | Creation → Worlds → pick a World | Lands on **Timeline** canvas (not World KB) |
| **Spine visible** | Looks at the canvas | Sees World events / KeyBlocks / Fork markers on a **when** axis |
| **Peers reachable** | Uses Canvas shell nav | One click to Outline (Work projection), World KB, Strategy |
| **Work unchanged** | Opens a Work | Still lands on **Outline** (V1.118 canvas-first work shell preserved) |

Without the demo path shipping in P1, V1.122 is a hollow pivot (Candidate C failure mode). P0 alone is insufficient for PMF.

## Author IA - spine vs projection (locked product model)

### Mental model (author)

```
World (spine)
├── Timeline  ← hero surface for World entry (V1.122)
├── World KB  ← peer: entity graph / relationships
└── Forks     ← projected on Timeline; create/merge UI deferred

Work (projection onto a World)
├── Outline   ← default for Work entry (V1.118, unchanged this iteration)
├── Manuscript / Reading
└── Chapter-relative timeline events (may remain on Outline companion; not the World hero)
```

### Entry defaults (product lock)

| Context | Route today | Route after V1.122 | Notes |
|---------|-------------|--------------------|-------|
| **World entry** | `/worlds/:worldId/kb` (World KB) | `/worlds/:worldId` → **Timeline** (e.g. `/timeline` or index redirect) | **This is the P1 IA change.** Worlds list pick-target updates. |
| **Work entry** | `/works/:workId` → `outline` | **Unchanged** - Outline remains default | Explicit non-goal to flip Work entry to Timeline. |
| **Canvas shell peers** | Strategy / Outline+Timeline / World KB | Strategy / Outline (Timeline-companion) / **Timeline** / World KB | Timeline is a fourth peer surface, not a replacement for Outline. |

### Reachability rules (must hold after P1)

1. **Outline is always one click away** from Timeline (Canvas shell nav or equivalent) - no dead-end hero.
2. **World KB remains a peer** - entity graph is not deleted; only loses default World-entry status.
3. **Work Outline is not demoted** - authors writing chapters still open Works → Outline; Timeline is the World-building instrument, not a replacement for chapter planning.
4. **Empty Timeline is honest** - if a World has no events/KeyBlocks/Forks yet, show an empty-state that explains the spine (not a blank canvas or a silent redirect to World KB).

### Product-decision note for architect (data spine)

Today, `timeline.patch_event` is **Work-scoped** (Outline+Timeline β, V1.72), while World KB graph is **World-scoped** (V1.73). P1's "World Timeline" projection must resolve which data sources compose the hero surface without inventing new Daemon routes this iteration. **Product intent:** authors see the World's *when* axis (events + KeyBlocks realized in time + Fork markers). **Architect owns:** exact DTO composition, whether Work-scoped timeline events appear when a World has bound Works, and empty-state honesty when world-level timeline data is sparse. Do **not** expand to new write routes or Fork creation UI to fill the canvas.

#### Architect resolution (seat 2, LOCKED)

- **Read composition:** Timeline `projectGraph` consumes **`GET /v1/daemon/worlds/{world_id}/kb/graph` → `WorldKbGraphResponse`** as the single graph source. KeyBlock entities of `block_type=event` ARE the "when-axis" events per `entity-scope-model.md` §5.1.1. Other entities (character/scene/organization/…) project as **Context** clusters off the axis. Typed `relationships[]` and `source_anchors[]` render as edges and grounding badges.
- **Optional sidecar:** `GET /v1/daemon/narrative/worlds/{world_id}` → `WorldState` may be fetched by the orchestrator (alongside the graph) for a Fork badge in the canvas header (`is_fork`, `parent_world_id`, `forked_from_event_id`). This is **not** a timeline data source; it is World chrome only. The adapter's `projectGraph` accepts `WorldKbGraphResponse` alone.
- **Work-scoped events are NOT composed** in V1.122. `timeline.patch_event` events are chapter-relative outline entries with no World-level merge key; pulling them onto the World when-axis would require a join that does not exist + N+1 fetches per bound Work. They remain on the **Outline (Timeline-companion)** surface for Work entry. Honest empty-state covers sparse Worlds.
- **World-scoped `TimelineEvent` HTTP route is deferred.** The domain `schemas/domain/timeline-event.schema.json` table is currently reachable only via `NarrativeGateway::get_timeline()` (internal) and the `nexus.timeline.recent.get` host-tool capability (ACP/orchestration, not HTTP). Promoting it to `GET /v1/daemon/worlds/{world_id}/timeline` is **out of V1.122 scope** (would touch daemon Rust + add an external route); tracked under `DF-V1122-DEEPER-WB`.
- **Write boundary:** the Timeline surface edits World-scoped KeyBlock entities through **`POST /v1/daemon/worlds/{world_id}/kb/patch-entity`** (V1.73 shipped) only. `timeline.patch_event` (Work-scoped) is NOT invoked from this surface. Relationship edits through `world_kb.patch_relationship` are allowed by the contract but deferred to post-MVP (read-only relationships on the Timeline surface in V1.122).
- **Conflict policy:** reuse `WorldKbConflictError` (HTTP 409, stale `expected_version`) + `WorldKbValidationError` (HTTP 422, domain-rule failure); no Timeline-specific conflict DTO; conflict-modal copy is world-kb-flavored.
- **`wire_contracts_changed: false`:** feasible. V1.122 P1 adds the frontend-only `CanvasSurfaceKind = "timeline"` enum value + a new adapter module; it reuses existing schemas, generated DTOs, daemon routes, and `@42ch/nexus-contracts` version. Verification steps are pinned in P1 Task 6.

Iteration-scoped product detail: [`specs/timeline-hero-product-spec.md`](./specs/timeline-hero-product-spec.md), [`specs/pillar-framing.md`](./specs/pillar-framing.md), [`specs/timeline-canvas-architecture.md`](./specs/timeline-canvas-architecture.md) (architect seat 2).

## Scope

本迭代锁定的 spec 点:

- **S1 - Three-pillar spec & narrative refactor (P0):**
  - `STRATEGY.md` - refresh Vision to name Harness/Canvas/Computable pillars; append V1.96-V1.121 decision-log entries (26 iterations, **grouped**); record V1.122 pivot decision.
  - `CONCEPTS.md` - add `Harness`, `Computable` (as pillar, distinct from existing `Compute (Capability)`), and `Timeline-first World building` entries; clarify Timeline vs Outline relationship (spine vs projection).
  - `specs/canvas-strategy-surface.md` - Draft overlay amendment: introduce `CanvasSurfaceKind = "timeline"` as a peer surface; specify Timeline-centric World-building projection (`WorldKbGraphResponse` entities with `block_type=event` as the when-axis; other entities as Context clusters; typed relationships + source anchors as edges + grounding badges — architect-locked, see `timeline-canvas-architecture.md`); reposition Outline+Timeline as "Outline (Timeline-companion)"; lock Timeline as the default hero surface for **World entry** (not Work entry).
  - `specs/` corpus alignment - update `orchestration-engine.md` (Harness pillar cross-ref), `compute-module-abi.md` + `wasm-host.md` (Computable pillar cross-ref), `web-ui.md` (Canvas IA: Timeline as default **World** surface; Outline remains default **Work** surface).
  - Roadmap re-homing - update `knowledge/deferred-features-cross-version-tracker.md` to re-frame DF-70/71/46/47 + BL-01..09 under the 3 pillars; record V1.122-deferred items (Harness UI rename, Computable UI surfacing, deeper World-building Timeline, V1.121 residuals) with target/owner/trigger. (**P0 implementation owns the tracker edit; Phase 1 review chain must not add new knowledge docs.**)

- **S2 - Timeline-first Canvas surface elevation (P1):**
  - Extract `timeline` as a peer `CanvasSurfaceKind` (split from `work-outline-timeline`); new `CanvasSurfaceAdapter` for Timeline.
  - **Data composition (architect-locked):** Timeline `projectGraph` consumes **only** `GET /v1/daemon/worlds/{world_id}/kb/graph` → `WorldKbGraphResponse` (V1.73 shipped). KeyBlock entities of `block_type=event` are the when-axis events; other entity types render as Context clusters; typed relationships + source anchors render as edges + grounding badges. Work-scoped `timeline.patch_event` events are NOT composed into this surface (they remain on the Outline companion).
  - **Optional sidecar:** `GET /v1/daemon/narrative/worlds/{world_id}` → `WorldState` may be used by the orchestrator for a Fork badge in the canvas header (read-only chrome; not a timeline data source).
  - **Write-boundary reuse:** the Timeline surface edits World-scoped KeyBlock entities through `kb.patch_entity` (V1.73) only. **No `timeline.patch_event` invocation from this surface.** `world_kb.patch_relationship` write is allowed by contract but deferred to post-MVP (V1.122 ships read-only relationships on Timeline).
  - **Conflict UX reuse:** world-kb-flavored `WorldKbConflictError` (409) + `WorldKbValidationError` (422); **no new conflict DTO**.
  - Canvas IA: Timeline becomes the default/hero surface when **entering a World** (`/worlds/:worldId` → Timeline); Worlds list pick-target updates from `/kb` to Timeline; Outline/Strategy/WorldKB remain peer surfaces from the shell.
  - `CanvasShell` + `useCanvasSurface()` reuse (V1.114 P0 adapter recipe); new Timeline node types (event, KeyBlock-on-timeline as Context cluster) + reuse existing `WorldKbEdgeData` for typed relationship edges. Fork marker **nodes** are deferred; Fork badge chrome may render from `WorldState` sidecar.
  - **Honest empty-state:** sparse World timeline (no `block_type=event` entities) shows copy explaining the spine with CTA to peer World KB; dagre layout MUST NOT fabricate event ordering when `body.attributes.occurred_at` is absent; `summarizeGraph` MUST include ordering disclaimer when temporal signal is missing.
  - No wire-contract breakage: additive `CanvasSurfaceKind` value + reuse existing DTOs; `wire_contracts_changed: false` target (architect-locked feasible — see Risk Register).
  - **Work entry preserved:** `/works/:workId` → Outline remains default (V1.118). Outline (`work-outline-timeline`) adapter is **untouched**; P1 regression tests assert Work entry still lands on Outline.

## Plans

| plan_id | Name | Status | Notes |
|---------|------|--------|-------|
| 2026-07-18-v1.122-three-pillar-spec-refactor | P0 - Three-pillar spec & narrative refactor (STRATEGY + CONCEPTS + canvas spec + roadmap re-homing) | Todo | **Must** - without pillar canonization + Timeline-as-peer spec, P1 has no normative contract to implement |
| 2026-07-18-v1.122-timeline-first-canvas | P1 - Timeline-first Canvas surface elevation (peer surface + World-building projection + World-entry IA) | Todo | **Must** - the concrete PMF bet; depends on P0 spec lock (`CanvasSurfaceKind = "timeline"`, projection contract, World-entry rule) |

**Must integrity (no Stretch plans this iteration):** Caller asked for refactor + product pivot + spec refactor under the 3-pillar + Timeline-hero direction. P0 (spec refactor) + P1 (code refactor) are the two legs; dropping either leaves an orphan (spec without code = hollow PMF; code without spec = uncontracted). Defer Harness UI rename, Computable UI surfacing, deeper World-building, residual cleanup to **§ Roadmap Position → Deferred inventory** - not silent Stretch demotion, not silent drop.

**Dependency:** P1 implement starts only after P0 merges the Draft overlay for `CanvasSurfaceKind = "timeline"` + World-entry IA rule into `specs/canvas-strategy-surface.md` (and STRATEGY/CONCEPTS pillar framing). Plans may Prepare in parallel; Execute is serial P0 → P1.

Status values: `Todo` | `InProgress` | `InReview` | `Done` | `Blocked`

## Milestones

| Milestone | Target date | Status |
|-----------|-------------|--------|
| Spec freeze (Review & Edit chain complete, compass locked) | 2026-07-18 | pending (product-manager seat 1 editing) |
| P0 spec refactor merged to integration | 2026-07-19 | pending |
| P1 Timeline-first Canvas merged | 2026-07-20 | pending |
| Iteration close + PR merge-ready | 2026-07-21 | pending |

## Acceptance Criteria

Each AC is binary and evidence-backed (grep, spec citation, build/vitest log, route test, screenshot). "Feels pivoted" / "looks better" is **not** acceptance.

### P0 - Spec & narrative (docs evidence)

- **AC-V1122-1** *(P0)* - `STRATEGY.md` Vision names **Harness / Canvas / Computable** as the three pillars (grep: section or bullet list containing all three names as pillars); decision log contains **grouped** V1.96-V1.121 catch-up entries **and** a V1.122 pivot entry; Vision is not only the pre-pivot "AI-agent-driven creative writing tool" sentence without pillar naming.
  - *Evidence:* `rg -n "Harness|Computable|Three pillars|V1\\.122" STRATEGY.md`; decision-log table includes a V1.122 row.
- **AC-V1122-2** *(P0)* - `CONCEPTS.md` has entries for `Harness`, `Computable` (pillar, **cross-referenced** to existing `Compute (Capability)` as mechanism), and `Timeline-first World building`; Timeline vs Outline relationship stated as **spine vs projection** (Timeline = World's "when" axis + World-entry hero; Outline = Work projection).
  - *Evidence:* `rg -n "^### (Harness|Computable|Timeline-first)" CONCEPTS.md`; Compute vs Computable distinction sentence present.
- **AC-V1122-3** *(P0)* - `specs/canvas-strategy-surface.md` has a **Draft (V1.122) overlay** introducing `CanvasSurfaceKind = "timeline"` as a peer value with: (a) World-building projection contract, (b) node/edge schema, (c) write-boundary reuse plan, (d) **Timeline-as-default-World-entry** IA rule, (e) Outline repositioned as "Outline (Timeline-companion)" without removing the surface. Shipped β text for Strategy/Outline/WorldKB is **not deleted**.
  - *Evidence:* `rg -n 'CanvasSurfaceKind.*"timeline"|Draft \\(V1\\.122\\)|Timeline-companion|default.*World' .mstar/specs/canvas-strategy-surface.md`.
- **AC-V1122-4** *(P0)* - `knowledge/deferred-features-cross-version-tracker.md` re-frames open DF/BL items under the 3 pillars (Harness/Canvas/Computable/Cross-cutting); **V1.122-deferred inventory** rows exist for: Harness UI rename, Computable UI surfacing, deeper World-building (Fork create/merge), compute-on-timeline, V1.121 residual cleanup, `status.json` compaction - each with **target + owner + trigger**.
  - *Evidence:* tracker has Pillar framing + deferred rows; IDs for DF-70/71/46/47 and BL-01..09 still present (no silent delete).

### P1 - Timeline-first Canvas (product + code evidence)

- **AC-V1122-5** *(P1)* - `CanvasSurfaceKind` includes `"timeline"` as a peer value (additive; existing kinds remain); a `TimelineCanvasAdapter` projects World-building entities onto a timeline-axis React Flow canvas; **World entry defaults to Timeline**: Worlds list pick navigates to Timeline (not `/kb`); `/worlds/:worldId` index resolves to Timeline.
  - *Evidence:* type/union includes `"timeline"`; route/nav tests assert pick-target + index redirect; vitest green.
- **AC-V1122-6** *(P1)* - Timeline canvas reuses the structured write boundary **through `kb.patch_entity` only** (no `timeline.patch_event` from this surface); **no raw-file writes** from the webview; `wire_contracts_changed: false` (no schema/codegen churn; `@42ch/nexus-contracts` version unchanged; no new daemon Rust routes).
  - *Evidence:* write-boundary tests asserting `kb.patch_entity` path + asserting `timeline.patch_event` is NOT called from the Timeline adapter; `git diff --stat schemas/` empty for the P1 plan branch; `cat packages/nexus-contracts/package.json | jq '.version'` unchanged; `pnpm run codegen` produces zero generated diff; `git diff --stat crates/nexus-daemon-runtime/src/api/` empty for the P1 plan branch; architect sign-off in P1 plan `## Review Gate Summary`.
- **AC-V1122-7** *(P1)* - `pnpm` build + typecheck + vitest green for `apps/web` (and `packages/nexus-ui` if touched); Rust/daemon untouched.
  - *Evidence:* build/test logs attached to plan completion report.
- **AC-V1122-8** *(P1, author reachability)* - From Timeline canvas, **Outline**, **World KB**, and **Strategy** are reachable as peer surfaces via Canvas shell nav (or equivalent documented affordance). Work entry **still** defaults to Outline (`/works/:workId` → `outline`).
  - *Evidence:* nav/route tests; `app-work-routes` or equivalent still asserts Work → Outline default.
- **AC-V1122-9** *(P1, PMF demo path)* - Dogfood path is demonstrable: Worlds list → pick World → **Timeline canvas renders** (including honest empty-state when no events) in light + dark; screenshot pack attached to P1 plan.
  - *Evidence:* screenshot paths recorded on P1 plan `## Review Gate Summary` or completion report; empty-state copy present in source/i18n.

## Non-Goals

- **No Harness UI rename this iteration** - "Strategy/Preset" product copy stays; pillar canonization is spec-level only. Rename is a dedicated breaking-change plan (roadmap ID: **DF-V1122-HARNESS-RENAME**).
- **No Computable pillar UI surfacing** - compute module stays backend/WASM; no new compute canvas, no compute badges as a pillar marketing surface this iteration (roadmap: **DF-V1122-COMPUTABLE-UI**).
- **No compute-on-timeline** - Timeline canvas does not invoke WASM compute modules (roadmap: **DF-V1122-COMPUTE-ON-TIMELINE**).
- **No wire contract / daemon / Rust changes** - P1 is additive frontend (`CanvasSurfaceKind` enum value + adapter + reuse existing DTOs); `wire_contracts_changed: false` for both plans.
- **No new Daemon API routes** - Timeline canvas reuses existing World KB graph / timeline event / patch routes as mapped by architect; no new endpoints to "fill" the hero surface.
- **No Fork creation / fork-merge UI** - Fork markers may render as read projection of existing Fork data; no create-branch or merge workflow (roadmap: **DF-V1122-FORK-UI**).
- **No Outline surface removal** - Outline (Timeline-companion) remains a peer surface; only Timeline extracts to a peer kind. Outline may retain chapter-relative timeline affordances as companion UX.
- **No Work-entry default flip** - `/works/:workId` continues to open **Outline** (V1.118). Timeline-first applies to **World entry**, not Work entry.
- **No Outline→Timeline silent redirect** - authors who open a Work must not be bounced to World Timeline.
- **No V1.121 residual cleanup as business scope** - 15 low/nit design-elevation residuals stay deferred; tracked in `status.json` residuals + roadmap inventory, not absorbed into P0/P1 tasks.
- **No `status.json` compaction as business plan** - harness-hygiene task (roadmap: **DF-V1122-STATUS-COMPACT**).
- **No nexus-platform (private repo) changes.**
- **No Phase 1 knowledge crystallization** - no new `{KNOWLEDGE_DIR}/` docs in the start chain (`mstar-iteration` §1.5.5); tracker re-homing is P0 **implementation**, not Review-chain knowledge authoring.

## Roadmap Position

- **Current iteration（V1.122）**：Three-pillar pivot - canonize Harness/Canvas/Computable in STRATEGY + CONCEPTS + canvas spec; elevate Timeline to peer Canvas surface with World-building projection as the **World-entry** hero. PMF signal = demo path (Worlds → Timeline).
- **Prior expectation override:** V1.121 roadmap said V1.122 would be post-elevation polish + residual cleanup. **User redirected** this iteration to the three-pillar + Timeline-first pivot. V1.121 residuals remain deferred (not dropped) - see inventory below.
- **Next iteration（V1.123） candidates** (pick after dogfood; owner: product-manager): (a) Harness UI rename; (b) Computable pillar UI / compute-on-timeline; (c) deeper Timeline World-building (Fork create/merge); (d) V1.121 residual cleanup; (e) `status.json` compaction + tech-debt paydown. **Trigger:** V1.122 shipped + dogfood feedback on Timeline-first World entry.
- **最终目标**：Nexus is the local-first creative-writing tool where a World's Timeline is the central instrument, AI agents are harnessed through Canvas, and Computable modules make worlds react. This is the coherent three-pillar product thesis — canonized in STRATEGY, contracted in specs, and verified in the running app. V1.122 establishes the pivot contract and ships the hero surface; subsequent iterations extend each pillar.

### Deferred inventory (Durable Roadmap Gate)

Every deferred item has a tracking location. "Later" prose alone is insufficient.

| ID | Item | Pillar | Target | Owner | Trigger | Tracking location |
|----|------|--------|--------|-------|---------|-------------------|
| DF-V1122-HARNESS-RENAME | Strategy/Preset → Harness product copy (breaking UX rename) | Harness | V1.123+ | product-manager | V1.122 shipped + copy audit | This table + P0 writes into `knowledge/deferred-features-cross-version-tracker.md` |
| DF-V1122-COMPUTABLE-UI | Computable pillar UI surfacing (compute registry/canvas marketing) | Computable | V1.123+ | product-manager | Dogfood shows authors cannot discover compute | Tracker (P0) |
| DF-V1122-COMPUTE-ON-TIMELINE | Invoke WASM compute from Timeline surface | Computable + Canvas | V1.124+ | architect | FEAT-WASM-COMPUTE follow-ons + Timeline hero stable | Tracker (P0); related: FEAT-WASM-COMPUTE V2 backlog |
| DF-V1122-FORK-UI | Fork creation + fork-merge authoring UI | Canvas | V1.123+ | product-manager | Authors need alternate history editing, not just markers | Tracker (P0) |
| DF-V1122-DEEPER-WB | Deeper World-building on Timeline (richer projection, multi-timeline, World-scoped TimelineEvent HTTP route) | Canvas | V1.123+ | product-manager | After Timeline hero dogfood | Tracker (P0) |
| DF-V1122-V1121-RES | V1.121 15 low/nit design residuals | Cross-cutting | V1.123 polish | frontend-dev | Capacity after pivot | `status.json` `residual_findings` (SSOT); do not mirror detail here |
| DF-V1122-STATUS-COMPACT | `status.json` size hygiene (<20KB) | Cross-cutting | Opportunistic / pre-P-last | project-manager | Before any P-last close when `wc -c` ≥ 20KB | Harness hygiene; not a business plan |
| DF-70 | Settings execution-mode matrix (BYOK etc.) | Harness | V1.105+ still open | product-manager | Settings slice capacity | `knowledge/deferred-features-cross-version-tracker.md` §2.3 |
| DF-71 | Desktop menu-bar daemon control | Cross-cutting | Any future desktop polish | ops/frontend | Desktop polish slice | Tracker §2.3 |
| DF-46 / DF-47 | Capability / host-tool registry completion | Harness | Reduced / narrowed remainders | architect | Capability program revisit | Tracker §2.3 |
| BL-01..09 | World merge, shadow read, context DSL, etc. | Canvas / Cross-cutting | Backlog | product-manager | When pillar roadmap prioritizes | Tracker §2.4 |

## Delivery Branch Policy

> Mirror of frontmatter; keep in sync with `{HARNESS_DIR}/status.json` `metadata`.

| Field | Value |
|-------|-------|
| `iteration_base_branch` | `main` |
| `spec_integration_branch` | `iteration/v1.122` |
| `target_branch` | `main` |

Branch resolve evidence (autonomous): `status.json` root metadata (`iteration_base_branch: main`, `target_branch: main`) + V1.118-V1.121 shipped compasses all `main -> iteration/vX -> main`.

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Timeline surface extraction destabilizes Outline+Timeline shipped β | Med | High | P1 reuses V1.114 `CanvasSurfaceAdapter` recipe; Outline surface keeps its adapter unchanged; additive `CanvasSurfaceKind` value, no removal; P1 regression test asserts `/works/:workId` still redirects to Outline |
| World Timeline data is sparse / Work-scoped only | High | High | **RESOLVED (architect seat 2):** Timeline hero composes from **`GET /v1/daemon/worlds/{world_id}/kb/graph`** (V1.73 shipped, `WorldKbGraphResponse`) alone. KeyBlocks of `block_type=event` ARE World-scoped narrative events (`entity-scope-model.md` §5.1.1) — they ARE the "when-axis" content. Work-scoped `timeline.patch_event` events are **NOT composed** into the World Timeline surface (chapter-relative, no World-level merge key); they stay on the Outline companion. No new Daemon route, no new schema, no daemon Rust change. Honest empty-state when `block_type=event` entities are absent. |
| World-scoped `TimelineEvent` (`schemas/domain/timeline-event.schema.json`) not exposed via any HTTP route | High | High | **RESOLVED (architect seat 2):** Confirmed via daemon source review (`crates/nexus-daemon-runtime/src/api/mod.rs`, `handlers/narrative.rs`) — the domain `TimelineEvent` table is reachable only via `NarrativeGateway::get_timeline()` and the `nexus.timeline.recent.get` host-tool capability (orchestration-internal). V1.122 MVP **does not** promote that surface to an HTTP route. Instead the Timeline canvas reads KeyBlock entities (which include `block_type=event`) through the existing World KB graph endpoint. The domain `TimelineEvent` HTTP route is **deferred to a future iteration** (`DF-V1122-DEEPER-WB` or a dedicated `DF-V1122-WORLD-TIMELINE-ROUTE` if prioritized) — tracked, not silently dropped. |
| STRATEGY.md decision-log catch-up (26 iterations) bloats P0 scope | Med | Med | P0 summarizes V1.96-V1.121 in **grouped** entries; detail stays in compasses |
| Timeline-first World entry confuses authors who expected World KB | Med | Med | World KB remains one click away; empty-state explains spine; Work entry still Outline |
| Authors confuse World entry vs Work entry | Med | Med | Compass + product specs lock dual defaults; AC-V1122-8 tests Work → Outline unchanged |
| `wire_contracts_changed` drift if P1 needs new DTOs | Low | High | **Architect verifies feasible:** P1 uses **only** existing DTOs (`WorldKbGraphResponse`, `WorldKbEntityProjection`, `WorldKbPatchEntityRequest`, `WorldKbConflictError`, `WorldKbValidationError`); additive `CanvasSurfaceKind` enum value only; P1 Task 6 verification steps include `git diff --stat schemas/` empty + `@42ch/nexus-contracts` version unchanged + `pnpm run codegen` zero diff |
| Spec refactor + code refactor in one iteration overruns M budget | Med | Med | P0 is docs; P1 scoped to surface extraction + projection + IA; roadmap captures overflow |
| Roadmap re-homing loses DF/BL id traceability | Low | Med | Tracker keeps IDs; only adds pillar framing; archived rows untouched |
| Hollow PMF if P1 slips | Med | High | AC-V1122-9 demo path is non-optional; do not mark iteration success on P0 alone |
| Large World KB graphs slow Timeline `projectGraph` (entity count >> outline) | Med | Med | V1.114 `useCanvasSurface()` already memoizes projection; adapter follows the "Progressive graph rendering" guidance (`canvas-strategy-surface.md` §3.1 — cap visible nodes, lazy-expand subgraphs); V1.122 MVP accepts a soft perf ceiling for very large worlds; dedicated perf residual deferred |
| Timeline adapter's "axis" layout invents a false temporal sequence | Med | Med | Adapter MUST NOT fabricate event ordering when `body.attributes.occurred_at` is absent; entities without a temporal signal cluster as "Context" off-axis with honest copy; dagre `direction: LR` is a visual choice, not a chronology promise. `summarizeGraph` MUST include the disclaimer string when ordering is inferred or absent. |

## Iteration package

> Sibling paths under `{ITERATION_DIR}/v1.122/` - not in `{SPECS_DIR}/` or `{KNOWLEDGE_DIR}/`. Promoted to knowledge at iteration-close via **`mstar-compound`**.

| Path | Purpose |
|------|---------|
| `guides/` | Exploration, process notes |
| `specs/pillar-framing.md` | Product framing for Harness / Canvas / Computable (iteration-scoped) |
| `specs/timeline-hero-product-spec.md` | Author IA, World-entry default, demo path, ACs for Timeline hero; includes architect-seat-2 Architecture section |
| `specs/timeline-canvas-architecture.md` | **Architect seat 2 lock:** data composition, adapter contract, write boundary, conflict policy, `wire_contracts_changed: false` verification contract for P1 |
| `README.md` | Package document index (recommended; writing-specialist may add) |

## Quality Gate Summary

> Filled at iteration-close. Human summary only; per-plan gate details stay in each main plan, and open residual SSOT stays in `{HARNESS_DIR}/status.json`.

| plan_id | QC decision | QA gate | Residuals | Durable summary |
|---------|-------------|---------|-----------|-----------------|
| P0 three-pillar-spec-refactor | TBD | TBD | TBD | `plans/2026-07-18-v1.122-three-pillar-spec-refactor.md#review-gate-summary` |
| P1 timeline-first-canvas | TBD | TBD | TBD | `plans/2026-07-18-v1.122-timeline-first-canvas.md#review-gate-summary` |

Notes:

- Raw review bundle: `{SDD_DIR}/review/` (ephemeral; do not rely on it after Done).
- Open residual SSOT: `{HARNESS_DIR}/status.json` root `residual_findings[<plan-id>]`.

## Compound Round Summary

> Filled at iteration-close.

- 结晶文档数：TBD
- 新增 CONCEPTS.md 条目：TBD（P0 may add Harness/Computable/Timeline-first entries during spec refactor）
- 触发 compound-refresh：TBD

## Iteration Retrospective (minimal)

> Filled at iteration-close.

- 做得好的：
- 可改进的：
- 下迭代建议：
