---
iteration_id: V1.123
start_date: 2026-07-18
status: locked
iteration_base_branch: main
target_branch: main
spec_integration_branch: iteration/v1.123
plans:
  - 2026-07-18-v1.123-three-layer-timeline-spec
  - 2026-07-18-v1.123-world-timeline-brief-narrative
  - 2026-07-18-v1.123-work-timeline-narrative-moment
  - 2026-07-18-v1.123-timeline-first-ia-deepening
  - 2026-07-18-v1.123-three-layer-zoom-experience
---

# V1.123 Delivery Compass — Three-Layer Timeline: Brief · Narrative · Moment

> **Direction lock mode: autonomous** (`/iteration-loop`, scale **L+** — caller-explicit scale above the L default cap of 4; this iteration locks **5 business plans** per the explicit `L+` token).
> Locked direction, rationale, and candidate trade-offs are recorded below per `mstar-iteration` §1.2 autonomous + `references/autonomous-direction-lock.md`.
>
> **Phase 1 Review & Edit chain:** product-manager seat 1 → architect seat 2 → writing-specialist seat 3 → PM lock. Direction is **locked** — do not re-question the three-layer Timeline deepening + Timeline-hero strengthening.

## Autonomous direction lock record

**Caller constraint (direction arg):** 本迭代深化 **Timeline(World) + Canvas** 两个领域。Timeline 与对应 Canvas 呈现分**三层**：**Brief 简述级 / Narrative 叙事级 / Moment 时刻级**。对于 **World**（Work 是推进 Timeline 的产物、是落地结果，**World 才是核心**），**Brief + Narrative** 是主要层级（Brief 时间跨度大但它是看清世界全局的核心；Narrative 通常局限在与现实时间相仿的节奏）。对于 **Work**，其 Timeline 以 **Narrative + Moment** 为主（Moment 精确描述一个场景）。需根据此体验设计完善**系统机制与 Canvas 体验**，注意**三层不一样的感受**，且 **Timeline 一定要突出**（产品的核心理念）。

**Scale:** caller token **`L+`** — explicit written constraint above the L default cap (4). `references/autonomous-direction-lock.md` § Scale budget permits >4 business plans when "an explicit written constraint requires more." `L+` is that constraint. **5 business plans** locked (spec refactor + 2 domain Timeline implementations + IA deepening + zoom/UX differentiation). Harness process (Review chain, SDD, QC/QA, compound, close, PR, merge-ready) is not counted and not planned as business plans.

### Candidates evaluated

Research base: V1.122 pivot artifacts (`STRATEGY.md` Three Pillars, `CONCEPTS.md` Timeline-first World building, `specs/canvas-strategy-surface.md` §3.3.2 Timeline peer surface, `iterations/v1.122/specs/timeline-canvas-architecture.md`, `iterations/v1.122/specs/timeline-hero-product-spec.md`), V1.122 P1 shipped code (`apps/web/src/components/canvas/timeline-canvas/`, `apps/web/src/components/canvas/outline-canvas/`, `apps/web/src/components/canvas/canvas-surface-adapter.ts`), `entity-scope-model.md` §1.1 `World > Timeline > Event > Moment` scope hierarchy + §5.1.1 `BlockType` taxonomy, `schemas/domain/timeline-event.schema.json`, `crates/nexus-narrative/src/timeline_event.rs` (`TimelineEvent` aggregate), `crates/nexus-kb/src/key_block.rs`, `iterations/v1.122/delivery-compass.md` § Deferred inventory (`DF-V1122-DEEPER-WB`, `DF-V1122-FORK-UI`, `DF-V1122-COMPUTE-ON-TIMELINE`), `knowledge/deferred-features-cross-version-tracker.md`, `knowledge/architecture-patterns/canvas-surface-extraction-pattern.md`, `knowledge/architecture-patterns/world-vs-work-canvas-scope.md` (V1.122 compound).

| # | Candidate | Trade-off | Verdict |
|---|-----------|-----------|---------|
| A | **Three-Layer Timeline Deepening** — canonize Brief/Narrative/Moment as Timeline's three zoom levels with **domain-differentiated layer use** (World: Brief+Narrative; Work: Narrative+Moment); extend Canvas to render all three layers with **differentiated feel**; **strengthen Timeline-first IA** to honor "Timeline 一定要突出"; reframe spec/contract accordingly | Largest narrative blast radius (touches STRATEGY/CONCEPTS/canvas spec/entity-scope model + Timeline surface code + new Work Timeline peer surface + Canvas IA + zoom UX); but matches caller direction exactly (深化 Timeline + Canvas + 三层 + 系统机制 + Timeline 突出) and unifies V1.122's flat event-timeline into a coherent multi-scale instrument | **LOCKED** — directly implements every clause of caller direction; V1.122 explicitly deferred "deeper World-building" (`DF-V1122-DEEPER-WB`) and Work Timeline composition to this iteration |
| B | World-only three-layer Timeline (Brief+Narrative only, no Work layer changes) | Smaller scope; but contradicts caller's explicit "对于 Work，它的 Timeline 以 Narrative + Moment 为主" — Work Timeline composition is mandatory, not optional | Rejected (partial — misses mandatory Work Timeline + Moment layer) |
| C | Three-layer Canvas zoom without Timeline domain split (one universal Timeline with three zoom levels, no World/Work layer specialization) | Simpler mental model; but loses caller's explicit domain-differentiated layer use ("对于 World... Brief+Narrative", "对于 Work... Narrative+Moment"); caller named the differentiation as a product feature, not a UX accident | Rejected (insufficient — flattens caller's product distinction) |
| D | Spec-only three-layer canonization (rewrite STRATEGY/CONCEPTS/canvas spec for Brief/Narrative/Moment, no code) | Fast, low-risk, sets up future code; but caller said "完善系统机制和 Canvas 体验" — explicitly demands **both** system mechanism **and** Canvas experience. Docs alone are insufficient. | Rejected (insufficient — no Canvas experience change this iteration) |
| E | V1.123 = V1.122 roadmap candidate (V1.121+V1.122 residual cleanup + dogfood) — defer three-layer to V1.124 | Respects V1.122's stated next-iteration candidates; but caller's direction this turn **explicitly overrides** the V1.122 roadmap with a new product bet (three-layer Timeline deepening). Autonomous direction lock must follow caller direction, not prior roadmap prose. | Rejected (wrong direction — caller redirected) |

### Evidence base for A

- **V1.122 Timeline is a single-layer event timeline.** `iterations/v1.122/specs/timeline-canvas-architecture.md` §2.2 locks Timeline `projectGraph` to read **only** `block_type=event` KeyBlocks as the when-axis (Brief or Moment level not modeled); other entity kinds render as "Context clusters" off-axis. There is no concept of a Brief level (multi-decade/era span) or a Moment level (scene-precise beat). Caller's "Brief 时间跨度很大" and "Moment 精确描述一个场景" cannot be satisfied without adding the two missing layers.
- **Work has no Timeline peer surface.** V1.122 P1 explicitly deferred Work-scoped timeline composition (`timeline-canvas-architecture.md` §2.4): "Work outline timeline events are NOT composed onto the World Timeline surface in V1.122." Work entry is Outline-only (`/works/:workId → outline`); there is no `CanvasSurfaceKind = "work-timeline"` peer. Caller's "对于 Work... Narrative + Moment" requires a Work Timeline peer surface that does not exist.
- **`Moment` is already in the scope hierarchy but unused.** `entity-scope-model.md` §1.1: `World > Timeline > Event > Moment` — Moment is defined as "session-start context point" but is **not surfaced as a Timeline zoom level** and has no Canvas representation. Caller's "Moment 精确描述一个场景的情况" elevates Moment from a session-context-only concept to a Timeline layer.
- **`Brief` is not a current concept.** Neither STRATEGY.md, CONCEPTS.md, nor `entity-scope-model.md` names a "Brief" granularity. The closest existing concept is the World Summary / Story Manifesto (high-level World description), but it is not a Timeline projection. Caller's "Brief 简述级 时间跨度很大但它是看清世界全局的核心" introduces a new Timeline granularity.
- **`DF-V1122-DEEPER-WB` already pre-registered the World-scoped TimelineEvent HTTP route as V1.123+ work.** The V1.122 architect explicitly deferred promoting `schemas/domain/timeline-event.schema.json` to `GET /v1/daemon/worlds/{world_id}/timeline` (and the deeper World-building that route enables) to V1.123+ — this iteration inherits that obligation.
- **The entity-scope model already supports the three-layer reframing.** `World > Timeline > Event > Moment` maps cleanly to **Brief (World-summary-on-Timeline) / Narrative (Event-level) / Moment (scene-level)** — caller's three-layer abstraction is a re-projection of an existing hierarchy, not a greenfield invention.
- **Canvas adapter recipe (V1.114) is proven and reusable.** `canvas-strategy-surface.md` §3.3.1 + `knowledge/architecture-patterns/canvas-surface-extraction-pattern.md` (V1.122 compound) document the `CanvasSurfaceAdapter` extraction pattern. Adding layer-aware projection and a Work Timeline peer surface follows the same recipe.
- **V1.122 IA is "Timeline-first World entry" but Timeline itself is not visually prominent.** Today's hero status is route-level (`/worlds/:id → timeline`); the Timeline surface is visually a peer surface from the Canvas shell. Caller's "Timeline 一定要突出" demands deeper IA + visual prominence, not just a route default.
- **`DF-V1122-STATUS-COMPACT` is real but unrelated.** `status.json` is 95KB (>> 20KB threshold); cleanup is harness hygiene (not a business plan) — this iteration triggers it opportunistically as a pre-P-last gate per `.mstar/AGENTS.md`.

### Locked direction (single sentence)

Deepen Nexus's Timeline instrument by canonizing **three zoom layers** — **Brief** (world-global, large time spans), **Narrative** (event-level, reality-paced), **Moment** (scene-precise) — with **domain-differentiated layer use** (World Timeline leads with Brief+Narrative; Work Timeline leads with Narrative+Moment), by (P0) refactoring specs/contracts/`entity-scope-model.md` to canonize the three layers + World/Work layer split + data carrier strategy, (P1) extending World Timeline Canvas to render Brief+Narrative layers with differentiated feel, (P2) adding Work Timeline as a peer Canvas surface rendering Narrative+Moment layers, (P3) deepening Timeline-first IA to make Timeline visually and structurally prominent, and (P4) differentiating the three-layer zoom/switch experience so each layer has its own feel — while all broader work (Fork UI, compute-on-timeline, V1.121+V1.122 residual cleanup, status.json compaction) is recorded into the roadmap.

### Scale budget application

- **P0 (Must)** — Three-layer Timeline spec & contract refactor: STRATEGY/CONCEPTS three-layer canonization + World/Work layer split; `entity-scope-model.md` Brief layer addition + Moment re-projection; `canvas-strategy-surface.md` Draft overlay for three-layer projection contract + Work Timeline peer surface; data carrier lock (Brief-on-KeyBlock vs new entity; Moment-on-TimelineEvent vs new schema); daemon route plan (promote `GET /v1/daemon/worlds/{world_id}/timeline` from `DF-V1122-DEEPER-WB`; extend Work outline read for Moment); `wire_contracts_changed` plan locked. **Business plan** (spec/contract deliverable).
- **P1 (Must)** — World Timeline Brief+Narrative dual-layer Canvas: extend V1.122 `TimelineCanvasAdapter` with Brief layer projection (World-summary-on-Timeline, multi-decade/era clustering) + Narrative layer (current event timeline); Brief↔Narrative layer switch/zoom; Brief-feel vs Narrative-feel differentiation; backend route + DTO changes as locked by P0. **Business plan** (code + schema).
- **P2 (Must)** — Work Timeline Narrative+Moment dual-layer Canvas: new peer `CanvasSurfaceKind = "work-timeline"`; Narrative layer (Work-scoped events from `timeline.patch_event` data + bound World events) + Moment layer (scene/beat-precise, manuscript-anchored); Work entry stays Outline; Work Timeline is a reachable peer from the Work Canvas shell; backend support as locked by P0. **Business plan** (code + schema).
- **P3 (Must)** — Timeline-first IA deepening (Timeline prominence): global Timeline entry (cross-World overview); Canvas shell Timeline visual/structural prominence; Worlds list + Works list surface Timeline activity; in-app navigation gestures that honor "Timeline 一定要突出" as a product thesis, not just a route default. **Business plan** (code, frontend IA).
- **P4 (Must)** — Three-layer zoom experience differentiation: per-layer layout/density/visual language (Brief = compact era markers + global summary; Narrative = balanced event + relationship edges; Moment = scene/beat detail + manuscript anchor); zoom/pan interaction patterns; cross-layer navigation; honest empty-state per layer. **Business plan** (code, frontend UX).
- **Roadmap (deferred, not counted):** Fork creation/merge UI (`DF-V1122-FORK-UI`); compute-on-timeline (`DF-V1122-COMPUTE-ON-TIMELINE`); Harness UI rename (`DF-V1122-HARNESS-RENAME`); Computable pillar UI (`DF-V1122-COMPUTABLE-UI`); V1.121+V1.122 residual cleanup (28 items); `status.json` compaction; deeper multi-timeline / era taxonomy. See **§ Roadmap Position → Deferred inventory**.

### Dependency graph (locked)

```
P0 (spec/contract lock)
   ├── P1 (World Timeline B+N) ──┐
   ├── P2 (Work Timeline N+M) ───┤
   └─────────────────────────────┴── P3 (IA deepening) → P4 (zoom UX)
```

P0 must Prepare + Execute first (locks schema + data carrier + Canvas contract). P1 and P2 may Prepare in parallel after P0 lock but Execute **serially** (P1 → P2) per `mstar-iteration` §2.6 per-plan loop. P3 may Prepare during P1/P2 Execute; P3 Execute after P1+P2 Done. P4 Execute after P3 Done.

## Product story — one sentence + why

### One-sentence product thesis (post-V1.123)

> **A Nexus Timeline is not one line — it is three instruments at three scales: a Brief for the world's whole shape, a Narrative for the events that happen, and Moments for the scenes that breathe — World timelines lead from the Brief, Work timelines lead from the Moments.**

If a reader cannot restate that sentence after reading this compass, the three-layer model has failed.

### Why three layers, why World/Work split (PMF)

V1.122 made Timeline the World-entry hero — but the Timeline it shipped is a **flat event list projected onto a when-axis** (`block_type=event` KeyBlocks only, no zoom, no Brief/Moment distinction). An author opening a World today sees the same granularity whether they want to scan a century or scrutinize a single scene. That collapses three distinct authorial intents into one undifferentiated surface:

| Author intent | Today's V1.122 surface | What they actually need |
|---------------|------------------------|-------------------------|
| "What is the shape of this world's history?" | Flat event timeline; reader must read every event | **Brief** — a few era markers summarizing the world's sweep |
| "What events happened, in order?" | Flat event timeline (today's only mode) | **Narrative** — event-level, reality-paced (today's surface) |
| "What happens in *this exact scene*?" | Not modeled — Moment is a session-context-only concept today | **Moment** — scene/beat precision, manuscript-anchored |

Caller's product lock: **World** authors need Brief + Narrative (world shape + events); **Work** authors need Narrative + Moment (events + scene precision). World's spine is the world shape; Work's spine is the scene being written.

| Layer | Means (author language) | World relevance | Work relevance | Hero change this iteration |
|-------|-------------------------|-----------------|----------------|---------------------------|
| **Brief** | "the shape of the world's history at a glance" — era/decade markers, world-global summaries | **Hero layer** for World Timeline | Out of scope (Work Brief is the Work's outline) | **P0+P1:** canonize as a Timeline layer; implement Brief projection on World Timeline |
| **Narrative** | "events happened, in order, at human pace" — today's event timeline | Peer layer for World Timeline | Peer layer for Work Timeline | **P0+P1+P2:** lock as the shared layer between World and Work; today's V1.122 Timeline surface becomes the Narrative layer |
| **Moment** | "what happens in this exact scene" — scene/beat precision, manuscript-anchored | Out of scope (World Moment is a session-context concept) | **Hero layer** for Work Timeline | **P0+P2:** canonize as a Timeline layer; implement Work Timeline with Moment projection |

**Why Brief is World-hero:** A World is a narrative universe — its "shape" is a multi-decade sweep (kingdoms rise/fall, ages pass). Authors opening a World for the first time need to **see the shape**, not every event. Brief is the World-global layer.

**Why Moment is Work-hero:** A Work is a specific manuscript — its unit is the scene being written. Authors working on a Work need to **scrutinize scenes**, not world history. Moment is the Work-precision layer.

**Why Narrative is shared:** Events at human pace (a battle, a conversation, a journey) belong to both worlds (world history is made of events) and works (chapters realize events). Narrative is the bridge layer.

**PMF signals this iteration must produce (author-feelable, not docs-only):**

| Signal | What the author does | What they feel |
|--------|----------------------|----------------|
| **World Brief visible** | Open a World | Sees Brief layer (era markers, world shape) — not just event list |
| **World Narrative reachable** | Switch to Narrative on World Timeline | Sees today's event timeline, now positioned as one of three layers |
| **Work Timeline exists** | Open a Work + navigate to Work Timeline | Sees Work Timeline (peer surface, not just Outline) |
| **Work Moment visible** | Switch to Moment on Work Timeline | Sees scene/beat precision, manuscript-anchored |
| **Three feels differ** | Switch Brief → Narrative → Moment | Each layer has its own layout/density/visual language |
| **Timeline is prominent** | Use the app's IA | Timeline is structurally prominent — not just a route default |

Without P1, P2, or P3 shipping, V1.123 is a hollow abstraction (Candidate D failure mode). P0 alone is insufficient for PMF.

## Three-layer model (locked product semantics)

### Layer definitions (LOCKED)

| Layer | Granularity | Time span | Author voice | Primary domain | Disambiguation |
|-------|-------------|-----------|---------------|----------------|----------------|
| **Brief** | World-global | Multi-decade / era / age | "In the Age of Stars, the kingdoms rose." | **World** (World Brief = world-history-at-a-glance) | — |
| **Narrative** | Event-level | Human-paced (days/weeks/years) | "On Midsummer's Eve, the treaty was signed." | **Shared** (both World and Work timelines include Narrative) | Timeline layer, **not** prose-craft narrative writing |
| **Moment** | Scene/beat-precise | Sub-scene (minutes/hours within a scene) | "She paused at the door, then spoke the name." | **Work** (Work Moment = scene-precision, manuscript-anchored) | Timeline layer, **not** Moment Context Assembly (session context packing) |

### Layer composition per Timeline (LOCKED)

```
World Timeline (P1):
  ├── Brief layer (hero)     — era markers / world shape / global summary
  ├── Narrative layer (peer) — events at human pace
  └── Moment layer           — out of scope for World Timeline V1.123
                                (Moment-on-World remains a session-context concept;
                                 DF-V1123-WORLD-MOMENT if ever promoted)

Work Timeline (P2):
  ├── Brief layer            — out of scope for Work Timeline V1.123
                                (Work Brief is the Work outline's job today;
                                 DF-V1123-WORK-BRIEF if ever promoted)
  ├── Narrative layer (peer) — events realized in this Work (chapter-relative
                                + bound World events at Work resolution)
  └── Moment layer (hero)    — scene/beat precision, manuscript-anchored
```

**Author mental model (do not invert in code comments or empty-state copy):**

```
World Timeline = Brief (world shape) + Narrative (events)
Work Timeline  = Narrative (events)  + Moment (scenes)
```

### Data carrier strategy (architect seat 2 LOCKED 2026-07-18)

Architect seat 2 LOCKED both carrier strategies in [`specs/three-layer-architecture.md`](specs/three-layer-architecture.md) §2 + §3 after evaluating the trade-off matrices against codebase evidence. Verdicts:

#### Brief carrier — **Brief-on-KeyBlock via new wire `BlockType = "era"`**

Reuses V1.73 `WorldKbGraphResponse` as the sole graph source; reuses V1.73 `kb.patch_entity` write path; reuses V1.73 `WorldKbConflictError` + `WorldKbValidationError` conflict DTOs. Era semantics ride `KeyBlock.body.attributes` (`era_id`, `start_hint`, `end_hint`, `world_summary`) — same freeform-object pattern as `novel_category` / `game_bible_category` / `script_category`. Follows the established `BlockType` extension precedent (V1.54 added 7 values for game-bible; V1.55 added 3 for script). Single additive schema change in `schemas/common/common.schema.json`. **Rejected alternatives:** Brief-on-World (highest churn — new DTO + new route + new conflict DTO + daemon Rust changes); Brief-on-TimelineEvent (requires `TimelineEventType` enum extension AND `GET /v1/daemon/worlds/{world_id}/timeline` route promotion from `DF-V1122-DEEPER-WB` — bigger blast radius; semantic mismatch between "world shape" and "discrete causality event").

#### Moment carrier — **Moment-on-Outline (frontend-only projection)**

Reuses existing V1.72 `GET /v1/daemon/works/{work_id}/outline` route; reuses V1.108 `OutlineSceneNodeData` / `OutlineBeatNodeData` UI types; reuses V1.72 `OutlineConflictError` + `OutlineValidationError` for any future Work-scope writes. Zero wire-contract churn attributable to Moment. **Important caveat:** the V1.72 `WorkOutline` wire does **not** expose scenes/beats today (confirmed by reading `schemas/daemon-api/canvas/outline/work-outline.schema.json`); V1.108 Scene/Beat is fixture-driven UI exploration. So the Moment layer will show honest empty-state in nearly all real Works in V1.123. Future wire extension tracked as `DF-V1123-MOMENT-WIRE` (V1.124+). **Rejected alternative:** Moment-on-TimelineEvent (`event_type=moment`) — requires `TimelineEventType` enum extension AND a Work-scoped timeline-event route (World-scoped `TimelineEvent` row has `world_id` not `work_id`); bigger wire + daemon + storage churn than V1.123 MVP can absorb.

#### Combined `wire_contracts_changed` verdict — **`true`** (single additive enum value attributable to Brief carrier)

P1 owns the single schema change; P2/P3/P4 add zero wire diff. P1 verification per `three-layer-architecture.md` §4.3 8-point gate.

Architect seat 2 LOCKED both carrier strategies in `specs/three-layer-architecture.md` §2 + §3 with trade-off matrices, rationale, and codebase citations.

### Cross-layer navigation (locked)

- Brief → Narrative: "drill into this era" — Narrative filters to events within the era's time span.
- Narrative → Brief: "zoom out" — Brief replaces Narrative as the prominent layer.
- Narrative → Moment: "drill into this scene" — Moment filters to moments realized by the event's chapter.
- Moment → Narrative: "zoom out" — Narrative replaces Moment as the prominent layer.

Cross-layer navigation is **within Timeline** (Brief↔Narrative on World; Narrative↔Moment on Work). It is **not** cross-surface (World Timeline ↔ Work Timeline cross-surface jump is a separate IA concern owned by P3).

## Author IA — three-layer on top of V1.122 (locked product model)

### Mental model (author, post-V1.123)

```
World spine (V1.122 locked, extended in V1.123)
├── World Timeline (V1.122 hero, V1.123 deepened)
│   ├── Brief layer (V1.123 NEW — world shape)
│   ├── Narrative layer (V1.123 — V1.122's existing event timeline reframed)
│   └── [Moment — out of scope for World]
├── World KB (peer, unchanged)
└── Forks (peer, marker-only V1.122; create/merge still deferred)

Work projection (V1.118 locked, V1.123 deepened)
├── Outline (V1.118 default for Work entry, unchanged)
├── Manuscript / Reading (unchanged)
├── Work Timeline (V1.123 NEW — peer surface for Work)
│   ├── [Brief — out of scope for Work]
│   ├── Narrative layer (V1.123 NEW — events realized in this Work)
│   └── Moment layer (V1.123 NEW — scene/beat precision, manuscript-anchored)
└── Strategy (peer, unchanged)
```

### Entry defaults (product lock)

| Context | Route today (V1.122) | Route after V1.123 | Notes |
|---------|----------------------|---------------------|-------|
| **World entry** | `/worlds/:worldId` → Timeline (event list) | `/worlds/:worldId` → **World Timeline Brief layer** (if Brief data exists) or Narrative layer (fallback) | **P1 IA change.** Worlds list pick-target unchanged (Timeline); only the default layer changes. |
| **Work entry** | `/works/:workId` → Outline | **Unchanged** — Outline remains default | Explicit non-goal to flip Work entry to Timeline. |
| **Work Timeline access** | Not available | `/works/:workId/timeline` (peer surface from Work Canvas shell) | **P2 IA change.** Work Timeline is a peer surface, not the Work default. |
| **Canvas shell peers (World)** | Strategy / Outline+Timeline-companion / Timeline / World KB | Strategy / Outline+Timeline-companion / **Timeline (Brief/Narrative switch)** / World KB | **P1 IA change.** Timeline surface gains a layer switcher. |
| **Canvas shell peers (Work)** | Strategy / Outline / World KB (Work inherits) | Strategy / **Outline** / **Work Timeline (NEW)** / World KB | **P2 IA change.** Work gains Timeline as a fourth peer surface. |
| **Cross-World Timeline** (P3) | Not available | Optional global Timeline view (cross-World overview) | **P3 IA change.** Strengthens "Timeline 一定要突出" thesis. |

### Reachability rules (must hold after P3)

1. **Brief is one click away from Narrative** on World Timeline — layer switcher in Timeline canvas header.
2. **Moment is one click away from Narrative** on Work Timeline — layer switcher in Work Timeline canvas header.
3. **Work Timeline is reachable from Work Outline** — Canvas shell nav or equivalent affordance; Work entry stays Outline, but Timeline is not hidden.
4. **World Timeline Brief remains honest** — if a World has no Brief data (no era markers / world summary), the World Timeline falls back to Narrative layer with an empty-state explaining Brief.
5. **Work Timeline Moment remains honest** — if a Work has no Scene/Beat data (no manuscript outline beats), the Work Timeline falls back to Narrative layer with an empty-state explaining Moment.
6. **Outline is always reachable** from Work Timeline — no dead-end hero.
7. **World KB remains a peer** from World Timeline — entity graph not deleted.

### Architect seat 2 LOCKED decisions (locked 2026-07-18 in `specs/three-layer-architecture.md`)

Architect seat 2 LOCKED all five open architectural decisions. Verdicts and rationale live in [`specs/three-layer-architecture.md`](specs/three-layer-architecture.md); the summary below is for compass alignment + PM verification.

| # | Decision | Verdict | Rationale one-liner + cite |
|---|----------|---------|-----------------------------|
| 1 | Brief data carrier | **Brief-on-KeyBlock via new wire `BlockType = "era"`** | Lowest wire-contract churn — single additive enum value; reuses V1.73 `WorldKbGraphResponse` + `kb.patch_entity` + World KB conflict DTOs verbatim; era semantics fit `body.attributes` (arch §2). |
| 2 | Moment data carrier | **Moment-on-Outline (frontend-only projection)** | Zero wire-contract churn for Moment — reuses V1.72 `WorkOutline` route + V1.108 Scene/Beat UI types + V1.72 Outline conflict DTOs; honest empty-state until V1.124+ wire extension (arch §3). |
| 3 | Daemon route plan | **Compose from existing routes; do NOT promote `GET /v1/daemon/worlds/{world_id}/timeline`; do NOT add `GET /v1/daemon/works/{work_id}/timeline`** | Brief + Narrative compose from `kb/graph`; Work Timeline composes from `outline`; `DF-V1122-DEEPER-WB` stays deferred to V1.124+ (arch §5). |
| 4 | `wire_contracts_changed` verdict | **`true`** — single additive enum value (`BlockType = "era"`) attributable to Brief carrier | One file changed (`schemas/common/common.schema.json`); codegen regen limited to `BlockType`; daemon Rust diff empty; minor `@42ch/nexus-contracts` version bump (arch §4). |
| 5 | Conflict policy per layer | **Reuse existing DTOs per layer domain; no new Timeline-specific conflict DTO; Moment is read-only in V1.123** | Brief + Narrative on World reuse V1.73 `WorldKbConflictError` + `WorldKbValidationError`; Narrative on Work reuses V1.72 `OutlineConflictError` + `OutlineValidationError`; Moment edits route through Outline (arch §6). |
| 6 | Work Timeline peer surface adapter contract | **`WorkTimelineLayerAdapter` conforms to V1.114 `CanvasSurfaceAdapter`; `defaultLayer: 'narrative'`** | Mirrors V1.122 `TimelineCanvasAdapter` stable-factory pattern; Work entry still defaults to Outline (V1.118 preserved); architect UX-risk override on default layer authorized by product spec §4.3 (arch §7). |
| 7 | Work Timeline default layer (override of product preference) | **Narrative-default with Moment one click away** (architect UX-risk override) | The V1.72 `WorkOutline` wire has no Scene/Beat data today; Moment-default would surface persistent empty-state in nearly all real Works. Product spec §4.3 explicitly authorized this fallback. Default may flip to Moment when wire extension ships (V1.124+). |

## Scope

本迭代锁定的 spec 点:

- **S0 - Three-layer Timeline abstraction + contract refactor (P0):**
  - `STRATEGY.md` — extend Vision + Three Pillars to name Brief/Narrative/Moment as Timeline's three layers; append V1.123 decision-log entry (three-layer deepening); refresh Canvas pillar description (three layers per Timeline).
  - `CONCEPTS.md` — add `Brief`, `Narrative` (as a Timeline layer, distinct from "narrative writing" prose), and `Moment` (as a Timeline layer, distinct from "Moment Context Assembly" session concept) entries; cross-reference the three layers to `Timeline-first World building` (V1.122); lock World/Work layer split (World: Brief+Narrative; Work: Narrative+Moment).
  - `entity-scope-model.md` — Draft overlay: canonize Brief as a Timeline-granularity concept (World-global layer); re-project Moment from session-context-only to a Timeline layer (Work scope); keep existing `World > Timeline > Event > Moment` scope hierarchy (Moment already in tree, now with Canvas projection).
  - `canvas-strategy-surface.md` — Draft overlay: (a) three-layer projection contract for Timeline (`CanvasSurfaceKind = "timeline"` gains Brief/Narrative layer switcher; new `CanvasSurfaceKind = "work-timeline"` for Work Timeline peer surface with Narrative/Moment layer switcher); (b) layer carrier contract (Brief-on-? / Moment-on-? — architect-locked); (c) cross-layer navigation rules; (d) per-layer empty-state honesty rules.
  - Wire contract + daemon route plan (architect-locked) — `wire_contracts_changed: true|false` verdict per carrier; if `true`, enumerate schema/DTO/route changes; if `false`, document the additive-frontend recipe (unlikely given Brief/Moment likely need carrier).
  - Roadmap re-homing — update `knowledge/deferred-features-cross-version-tracker.md` to record V1.123-deferred items (`DF-V1123-WORLD-MOMENT`, `DF-V1123-WORK-BRIEF`, deeper Fork UI, compute-on-timeline, V1.121+V1.122 residuals) with target/owner/trigger.

- **S1 - World Timeline Brief+Narrative dual-layer Canvas (P1):**
  - Extend V1.122 `TimelineCanvasAdapter` with **Brief layer projection** (world-global era markers / world shape / global summary; data carrier per P0 architect lock).
  - **Narrative layer projection** — V1.122's existing event timeline reframed as the Narrative layer; projection contract unchanged (still `block_type=event` KeyBlocks as when-axis per `iterations/v1.122/specs/timeline-canvas-architecture.md` §2.2).
  - **Brief↔Narrative layer switcher** in Timeline canvas header (switch, zoom, or both).
  - **Brief-feel vs Narrative-feel differentiation** — Brief = compact era markers + world shape; Narrative = balanced event timeline + relationship edges (today's feel).
  - **Honest empty-state per layer** — Brief empty (no era markers) → fallback to Narrative with explanation; Narrative empty (no events) → today's V1.122 empty-state.
  - Backend route + DTO changes as locked by P0 (likely `GET /v1/daemon/worlds/{world_id}/timeline` promotion from `DF-V1122-DEEPER-WB` + Brief carrier DTO).
  - Default World Timeline layer = Brief (if Brief data exists), else Narrative fallback.

- **S2 - Work Timeline Narrative+Moment dual-layer Canvas (P2):**
  - New peer `CanvasSurfaceKind = "work-timeline"` (Work Timeline adapter; conform to V1.114 `CanvasSurfaceAdapter`).
  - **Narrative layer** — Work-scoped events from `timeline.patch_event` data (Work outline events, chapter-relative) + bound World events at Work resolution (per architect data composition lock).
  - **Moment layer** — Scene/Beat precision (reuse V1.108 `OutlineSceneNodeData`/`OutlineBeatNodeData` data, or extend timeline-event with `event_type: "moment"` per architect carrier lock).
  - **Narrative↔Moment layer switcher** in Work Timeline canvas header.
  - **Moment-feel differentiation** — scene/beat-precise layout; manuscript anchor badges; closer time-scale than Narrative.
  - **Honest empty-state per layer** — Moment empty (no Scene/Beat data) → fallback to Narrative with explanation.
  - **Work entry preserved** — `/works/:workId` → Outline remains default (V1.118); Work Timeline is reachable as a peer from Work Canvas shell.
  - Backend route + DTO changes as locked by P0.

- **S3 - Timeline-first IA deepening (P3):**
  - **Global Timeline entry** — cross-World Timeline overview (recent Timeline activity across all Worlds; accessible from primary nav).
  - **Canvas shell Timeline prominence** — visual/structural prominence for Timeline in Canvas shell nav (not just a route default); may include Timeline accent color, Timeline-first ordering, Timeline activity badges.
  - **Worlds list + Works list surface Timeline activity** — last-edited Timeline layer, Brief/Narrative/Moment counts, "active" Timeline indicators.
  - **In-app navigation gestures** that honor "Timeline 一定要突出" — e.g., keyboard shortcut to Timeline from anywhere; Timeline pinned in dock/sidebar.
  - Cross-World Timeline ↔ Work Timeline navigation (e.g., from Work Timeline Moment, jump to bound World Timeline Narrative layer for the same event).

- **S4 - Three-layer zoom experience differentiation (P4):**
  - **Brief-feel layout** — compact era markers; horizontal sweep; minimal node density; world-shape summary header.
  - **Narrative-feel layout** — V1.122's current Timeline layout; balanced density; relationship edges; Context clusters.
  - **Moment-feel layout** — vertical scene-stack or scene-card layout; manuscript-anchor prominence; sub-scene time precision; closer zoom default.
  - **Zoom/pan interaction patterns** — Brief↔Narrative zoom (World), Narrative↔Moment zoom (Work); semantic zoom (not just viewport zoom); layer transition animations.
  - **Cross-layer navigation UX** — "drill into" / "zoom out" affordances; layer breadcrumbs.
  - **Per-layer empty-state copy** — each layer has its own honest empty-state explaining the layer's intent.

## Plans

| plan_id | Name | Status | Notes |
|---------|------|--------|-------|
| `2026-07-18-v1.123-three-layer-timeline-spec` | P0 — Three-layer Timeline spec & contract refactor (STRATEGY + CONCEPTS + entity-scope-model + canvas-strategy-surface + Brief/Moment carrier lock + daemon route plan + roadmap re-homing) | Done | **Must** — without three-layer canonization + carrier lock + route plan, P1/P2 have no normative contract to implement. Architect seat 2 locks Brief/Moment carriers in this plan. |
| `2026-07-18-v1.123-world-timeline-brief-narrative` | P1 — World Timeline Brief+Narrative dual-layer Canvas (Brief layer + Narrative reframing + Brief↔Narrative switcher + Brief-feel differentiation + backend route) | Done | **Must** — World Timeline hero layer (Brief) is the headline PMF signal; depends on P0 spec lock + Brief carrier choice. |
| `2026-07-18-v1.123-work-timeline-narrative-moment` | P2 — Work Timeline Narrative+Moment dual-layer Canvas (new Work Timeline peer surface + Moment layer + Narrative layer + Moment-feel differentiation + Work Outline preserved) | Done | **Must** — Work Timeline peer surface is mandatory per caller direction; depends on P0 spec lock + Moment carrier choice. |
| `2026-07-18-v1.123-timeline-first-ia-deepening` | P3 — Timeline-first IA deepening (global Timeline entry + Canvas shell Timeline prominence + list surfaces Timeline activity + cross-World/Work Timeline navigation) | Todo | **Must** — "Timeline 一定要突出" deepens from route default to structural prominence; depends on P0 spec lock + P1/P2 surface availability. |
| `2026-07-18-v1.123-three-layer-zoom-experience` | P4 — Three-layer zoom experience differentiation (per-layer layout/density/visual language + zoom/pan + cross-layer navigation UX + per-layer empty-state copy) | Todo | **Must** — "三层不一样的感受" is a caller-mandated product feature, not optional polish; depends on P0 + P1/P2 layers existing. |

**Must integrity (no Stretch plans this iteration):** Caller asked for 三层 + World/Work split + Canvas 体验 + 系统机制 + Timeline 突出 + 三层感受. Each plan addresses one mandatory clause; dropping any leaves a hollow iteration. Defer Fork UI, compute-on-timeline, Harness UI rename, Computable UI, V1.121+V1.122 residual cleanup, status.json compaction, deeper multi-timeline, era taxonomy to **§ Roadmap Position → Deferred inventory** — not silent Stretch demotion, not silent drop.

**Dependency:** P0 must Prepare + Execute first. P1 and P2 may Prepare in parallel after P0 lock but Execute serially (P1 → P2). P3 may Prepare during P1/P2 Execute; P3 Execute after P1+P2 Done. P4 Execute after P3 Done.

Status values: `Todo` | `InProgress` | `InReview` | `Done` | `Blocked`

## Milestones

| Milestone | Target date | Status |
|-----------|-------------|--------|
| Spec freeze (Review & Edit chain complete, compass locked) | 2026-07-18 | pending (product-manager seat 1 complete → architect seat 2 complete → writing-specialist seat 3 → PM lock) |
| P0 spec refactor merged to integration | 2026-07-19 | pending |
| P1 World Timeline Brief+Narrative merged | 2026-07-20 | pending |
| P2 Work Timeline Narrative+Moment merged | 2026-07-21 | pending |
| P3 Timeline-first IA deepening merged | 2026-07-22 | pending |
| P4 Three-layer zoom experience merged | 2026-07-23 | pending |
| Iteration close + PR merge-ready | 2026-07-24 | pending |

## Acceptance Criteria

Each AC is binary and evidence-backed (grep, spec citation, build/vitest log, route test, screenshot). "Feels layered" / "looks three-dimensional" is **not** acceptance.

### P0 — Spec & contract (docs evidence)

- **AC-V1123-1** *(P0)* — `STRATEGY.md` Vision + Three Pillars names **Brief / Narrative / Moment** as Timeline's three layers; Canvas pillar description distinguishes World Timeline (Brief+Narrative) from Work Timeline (Narrative+Moment); decision log contains a V1.123 entry (three-layer deepening).
  - *Evidence:* `rg -n "Brief|Narrative|Moment|V1\\.123" STRATEGY.md`; Three Pillars table has layer rows or layer-naming in Canvas pillar description.
- **AC-V1123-2** *(P0)* — `CONCEPTS.md` has entries for `Brief`, `Narrative` (as Timeline layer; cross-referenced to existing prose sense), and `Moment` (as Timeline layer; cross-referenced to existing `Moment Context Assembly`); World/Work layer split stated (World: Brief+Narrative; Work: Narrative+Moment).
  - *Evidence:* `rg -n "^### (Brief|Narrative|Moment)" CONCEPTS.md`; cross-reference links present.
- **AC-V1123-3** *(P0)* — `entity-scope-model.md` has a Draft overlay canonizing Brief as a Timeline-granularity concept (World-global layer) and re-projecting Moment to a Timeline layer (Work scope); existing `World > Timeline > Event > Moment` scope hierarchy preserved.
  - *Evidence:* `rg -n "Brief|Moment layer|Three-layer Timeline" .mstar/specs/entity-scope-model.md`; §1.1 scope tree unchanged.
- **AC-V1123-4** *(P0)* — `canvas-strategy-surface.md` has a **Draft (V1.123) overlay** specifying: (a) `CanvasSurfaceKind = "timeline"` gains Brief/Narrative layer switcher; (b) new `CanvasSurfaceKind = "work-timeline"` peer surface with Narrative/Moment layer switcher; (c) layer carrier contract (architect-locked); (d) cross-layer navigation rules; (e) per-layer empty-state honesty rules. Shipped β text for V1.122 Timeline surface is **not deleted**.
  - *Evidence:* `rg -n 'Draft \\(V1\\.123\\)|work-timeline|Brief.*layer|Moment.*layer' .mstar/specs/canvas-strategy-surface.md`.
- **AC-V1123-5** *(P0)* — `specs/three-layer-architecture.md` (iteration-scoped, architect seat 2) LOCKS: (a) Brief data carrier (one of Brief-on-KeyBlock / Brief-on-World / Brief-on-TimelineEvent); (b) Moment data carrier (one of Moment-on-TimelineEvent / Moment-on-Outline); (c) `wire_contracts_changed: true|false` verdict + enumerated changes if `true`; (d) daemon route plan (promote `GET /v1/daemon/worlds/{world_id}/timeline`? new Work Timeline route?); (e) conflict policy per layer.
  - *Evidence:* file exists; verdicts present with rationale.
- **AC-V1123-6** *(P0)* — `knowledge/deferred-features-cross-version-tracker.md` re-frames `DF-V1122-DEEPER-WB` (closed by V1.123 P0/P1 if World-scoped TimelineEvent HTTP route ships) and adds V1.123-deferred rows for `DF-V1123-WORLD-MOMENT`, `DF-V1123-WORK-BRIEF`, deeper Fork UI inheritance from `DF-V1122-FORK-UI`, compute-on-timeline inheritance from `DF-V1122-COMPUTE-ON-TIMELINE`, V1.121+V1.122 residual cleanup, `status.json` compaction — each with target + owner + trigger.
  - *Evidence:* tracker has V1.123 deferred rows; DF-V1122-DEEPER-WB status updated.

### P1 — World Timeline Brief+Narrative (product + code evidence)

- **AC-V1123-7** *(P1)* — `TimelineCanvasAdapter` projects **Brief layer** (era markers / world shape) **and** Narrative layer (V1.122 event timeline); World Timeline canvas header has a Brief↔Narrative layer switcher; World entry defaults to Brief layer when Brief data exists, else falls back to Narrative with an honest empty-state.
  - *Evidence:* adapter source has Brief projection; layer switcher UI test; route/nav tests assert default-layer fallback logic.
- **AC-V1123-8** *(P1)* — Brief layer writes (if any) and Narrative layer writes follow the P0-locked carrier + write boundary; conflict UX reuses the P0-locked conflict DTOs; `wire_contracts_changed` verdict matches P0 lock (verified by `git diff --stat schemas/` + `@42ch/nexus-contracts` version + `pnpm run codegen` diff per `knowledge/conventions/wire-contracts-frozen-verification.md`).
  - *Evidence:* write-boundary tests; wire-contracts gate evidence on plan completion report.
- **AC-V1123-9** *(P1)* — `pnpm` build + typecheck + vitest green for `apps/web` (and `packages/nexus-ui`/`packages/nexus-contracts` if touched); Rust/daemon changes (if any) pass `cargo clippy -p <crate> -- -D warnings` + `cargo test -p <crate>` per root `AGENTS.md` Development Policy.
  - *Evidence:* build/test logs attached to plan completion report.
- **AC-V1123-10** *(P1)* — From World Timeline Brief layer, **Narrative** is reachable via layer switcher (one click); World KB and Strategy peers remain reachable; Work entry still defaults to Outline (regression).
  - *Evidence:* layer switcher test; nav tests; `app-work-routes` or equivalent still asserts Work → Outline default.

### P2 — Work Timeline Narrative+Moment (product + code evidence)

- **AC-V1123-11** *(P2)* — New `CanvasSurfaceKind = "work-timeline"` peer surface exists; `WorkTimelineCanvasAdapter` projects Narrative layer (Work-scoped events) and Moment layer (scene/beat precision); Work Timeline is reachable from Work Canvas shell (peer with Outline, Strategy, World KB); Work entry still defaults to Outline.
  - *Evidence:* type union includes `"work-timeline"`; adapter source exists; route/nav tests assert peer reachability + Work → Outline default unchanged.
- **AC-V1123-12** *(P2)* — Work Timeline canvas header has a Narrative↔Moment layer switcher; Moment layer (when Scene/Beat data exists) renders scene/beat precision with manuscript-anchor badges; Moment empty (no Scene/Beat) falls back to Narrative with an honest empty-state.
  - *Evidence:* layer switcher UI test; Moment projection test; empty-state copy in source/i18n.
- **AC-V1123-13** *(P2)* — Work Timeline writes follow the P0-locked carrier + write boundary; conflict UX reuses P0-locked DTOs; `wire_contracts_changed` verdict matches P0 lock.
  - *Evidence:* write-boundary tests; wire-contracts gate evidence.
- **AC-V1123-14** *(P2)* — `pnpm` build + typecheck + vitest green; Rust changes (if any) pass clippy + tests.
  - *Evidence:* build/test logs.
- **AC-V1123-15** *(P2)* — From Work Timeline Moment layer, **Narrative** is reachable via layer switcher; Outline reachable from Work Canvas shell; World Timeline unaffected (regression).
  - *Evidence:* layer switcher test; nav tests; World Timeline regression tests still green.

### P3 — Timeline-first IA deepening (product + code evidence)

- **AC-V1123-16** *(P3)* — A **global Timeline entry** exists in primary nav (or documented equivalent affordance); cross-World Timeline overview renders recent Timeline activity across all Worlds; entry is one click from anywhere in the app.
  - *Evidence:* nav tests; global Timeline view test; one-click reachability test.
- **AC-V1123-17** *(P3)* — Canvas shell nav visually distinguishes Timeline (e.g., accent color, ordering, activity badge); Worlds list + Works list surfaces Timeline activity (last-edited layer, Brief/Narrative/Moment counts, or active Timeline indicator).
  - *Evidence:* visual diff in screenshot pack; list-surface tests assert Timeline activity data.
- **AC-V1123-18** *(P3)* — Cross-surface Timeline navigation works: from Work Timeline Moment layer, an author can jump to the bound World Timeline Narrative layer for the same event (if data bound); from World Timeline Narrative, an author can jump to a Work Timeline Moment realizing the event (if a Work realizes it).
  - *Evidence:* cross-surface navigation test; bound-data path documented.
- **AC-V1123-19** *(P3)* — `pnpm` build + typecheck + vitest green; no regression in V1.122 routes/defaults.
  - *Evidence:* build/test logs.

### P4 — Three-layer zoom experience differentiation (product + code evidence)

- **AC-V1123-20** *(P4)* — Brief, Narrative, and Moment layers each have **distinct layout + density + visual language** (Brief = compact era sweep; Narrative = balanced event + edges; Moment = scene-stack/scene-card with manuscript anchors); the three feels are perceptibly different in screenshot pack.
  - *Evidence:* screenshot pack (Brief/Narrative/Moment side-by-side); design-token evidence (different tokens per layer where applicable).
- **AC-V1123-21** *(P4)* — Brief↔Narrative zoom (World Timeline) and Narrative↔Moment zoom (Work Timeline) use **semantic zoom** (not just viewport zoom); layer transitions are animated (or documented why animation is deferred).
  - *Evidence:* zoom interaction test; transition source code or documented deferral.
- **AC-V1123-22** *(P4)* — Each layer has its own **honest empty-state copy** (Brief empty = "no era markers yet"; Narrative empty = today's V1.122 empty-state; Moment empty = "no scene/beat data yet"); empty-state copy present in source/i18n.
  - *Evidence:* i18n strings; empty-state tests.
- **AC-V1123-23** *(P4)* — Cross-layer navigation affordances ("drill into" / "zoom out" / layer breadcrumbs) are present and tested; layer state survives viewport changes (e.g., switching surface and back preserves layer).
  - *Evidence:* navigation tests; layer-state-persistence test.
- **AC-V1123-24** *(P4)* — `pnpm` build + typecheck + vitest green.
  - *Evidence:* build/test logs.

### PMF demo path (required for iteration Done)

**Demo script:** Worlds list → pick World → **Brief layer renders** (era markers / world shape) → switch to Narrative → event timeline → open Work → Outline (unchanged) → Canvas shell nav → **Work Timeline** → Narrative layer → switch to **Moment** → scene/beat precision → global nav → **global Timeline view** → cross-World activity → return. Light + dark screenshots at each step.

Without this demo path shipping, V1.123 is a hollow abstraction (Candidate D failure mode).

## Non-Goals

- **No Fork creation / fork-merge UI this iteration** — Fork markers may continue to render as read projection of existing Fork data; no create-branch or merge workflow (`DF-V1122-FORK-UI` stays deferred).
- **No compute-on-timeline** — Timeline surfaces do not invoke WASM compute modules (`DF-V1122-COMPUTE-ON-TIMELINE` stays deferred).
- **No Harness UI rename this iteration** — "Strategy/Preset" product copy stays; three-layer work is Timeline/Canvas only (`DF-V1122-HARNESS-RENAME` stays deferred).
- **No Computable pillar UI surfacing** — compute module stays backend/WASM (`DF-V1122-COMPUTABLE-UI` stays deferred).
- **No Work-entry default flip** — `/works/:workId` continues to open **Outline** (V1.118). Work Timeline is a peer, not a default.
- **No World-entry default flip away from Timeline** — V1.122's World-entry default (Timeline) is preserved; P1 only changes the **default layer** within Timeline (Brief if data exists, else Narrative).
- **No Outline surface removal** — Outline (Timeline-companion) remains a peer surface on World; Outline remains default for Work entry.
- **No World Moment layer this iteration** — Moment-on-World remains a session-context-only concept (DF-V1123-WORLD-MOMENT if ever promoted).
- **No Work Brief layer this iteration** — Work Brief is the Work outline's job today (DF-V1123-WORK-BRIEF if ever promoted).
- **No V1.121+V1.122 residual cleanup as business scope** — 28 low/nit design/system residuals stay deferred; tracked in `status.json` residuals + roadmap inventory, not absorbed into P0–P4 tasks.
- **No `status.json` compaction as business plan** — harness-hygiene task (DF-V1123-STATUS-COMPACT); triggered opportunistically pre-P-last if `wc -c` ≥ 20KB.
- **No multi-timeline / era taxonomy depth** — Brief layer MVP uses era markers as the granularity; rich era taxonomy (kingdoms, ages, sub-ages) is post-V1.123.
- **No cross-World Timeline merge** — global Timeline view (P3) is read-only overview, not a merged Timeline surface.
- **No new TipTap / whole-document editor surfaces** — V1.75 canvas-pivot invariant preserved; Moment layer is node-granular, not a new rich-text editor.
- **No nexus-platform (private repo) changes.**
- **No Phase 1 knowledge crystallization** — no new `{KNOWLEDGE_DIR}/` docs in the start chain (`mstar-iteration` §1.5.5); tracker re-homing is P0 **implementation**, not Review-chain knowledge authoring.

## Roadmap Position

- **Current iteration（V1.123）**：Three-layer Timeline deepening — canonize Brief/Narrative/Moment as Timeline's three zoom layers; implement World Timeline Brief+Narrative and Work Timeline Narrative+Moment; deepen Timeline-first IA; differentiate three-layer zoom feel. PMF signal = demo path (Worlds → Brief → Narrative → Work Timeline → Moment → global Timeline).
- **Prior expectation override:** V1.122 roadmap said V1.123 would be dogfood + residual cleanup. **User redirected** this iteration to the three-layer Timeline deepening. V1.121+V1.122 residuals remain deferred (not dropped) — see inventory below.
- **Next iteration（V1.124） candidates** (pick after dogfood; owner: product-manager): (a) Fork creation/merge UI (`DF-V1122-FORK-UI`); (b) compute-on-timeline (`DF-V1122-COMPUTE-ON-TIMELINE`); (c) Harness UI rename (`DF-V1122-HARNESS-RENAME`); (d) Computable pillar UI (`DF-V1122-COMPUTABLE-UI`); (e) World Moment layer (`DF-V1123-WORLD-MOMENT`); (f) Work Brief layer (`DF-V1123-WORK-BRIEF`); (g) deeper era taxonomy / multi-timeline; (h) V1.121+V1.122+V1.123 residual cleanup; (i) `status.json` compaction + tech-debt paydown. **Trigger:** V1.123 shipped + dogfood feedback on three-layer Timeline.
- **最终目标**：A Nexus Timeline is three instruments at three scales — Brief for the world's shape, Narrative for the events, Moments for the scenes — World timelines lead from the Brief, Work timelines lead from the Moments, and Timeline is structurally prominent as the product's core idea. V1.123 establishes the three-layer model + ships both World and Work layer compositions + deepens Timeline-first IA; subsequent iterations extend each layer (richer Brief taxonomy, Moment manuscript-binding, cross-World merge) and reactivate deferred pillars (Fork UI, compute-on-timeline, Harness rename, Computable UI).

### Deferred inventory (Durable Roadmap Gate)

Every deferred item has a tracking location. "Later" prose alone is insufficient.

| ID | Item | Pillar | Target | Owner | Trigger | Tracking location |
|----|------|--------|--------|-------|---------|-------------------|
| DF-V1123-WORLD-MOMENT | World Timeline Moment layer (scene-precision within World history) | Canvas | V1.124+ | product-manager | Authors need scene-precision when reading world history, not just when writing | This table + P0 writes into `knowledge/deferred-features-cross-version-tracker.md` |
| DF-V1123-WORK-BRIEF | Work Timeline Brief layer (world-shape projection for a Work) | Canvas | V1.124+ | product-manager | Authors need Work-level world-shape context | Tracker (P0) |
| DF-V1123-ERA-TAXONOMY | Rich era taxonomy for Brief layer (kingdoms, ages, sub-ages; not just era markers) | Canvas | V1.124+ | product-manager | Brief MVP proves the abstraction; richer taxonomy needed | Tracker (P0) |
| DF-V1123-MULTI-TIMELINE | Multiple parallel Timelines per World (alternate-history branches beyond Fork) | Canvas | V1.125+ | architect | Authors need branch comparison beyond Fork semantics | Tracker (P0) |
| DF-V1123-GLOBAL-TIMELINE-MERGE | Cross-World Timeline merge (read-write merged view, not read-only overview) | Canvas | V1.125+ | product-manager | P3 global overview proves valuable; merge needed for cross-World narrative | Tracker (P0) |
| DF-V1123-RESIDUAL-CLEANUP | V1.121 (15) + V1.122 (6) + V1.123 (TBD) low/nit residuals | Cross-cutting | V1.124 polish | frontend-dev | Capacity after three-layer ship | `status.json` `residual_findings` (SSOT); do not mirror detail here |
| DF-V1123-STATUS-COMPACT | `status.json` size hygiene (<20KB) | Cross-cutting | Opportunistic / pre-P-last | project-manager | Before any P-last close when `wc -c` ≥ 20KB | Harness hygiene; not a business plan |
| DF-V1122-FORK-UI (inherited) | Fork creation + fork-merge authoring UI | Canvas | V1.124+ | product-manager | Authors need alternate-history editing, not just markers | Tracker (already V1.122) |
| DF-V1122-COMPUTE-ON-TIMELINE (inherited) | Invoke WASM compute from Timeline surface | Computable + Canvas | V1.124+ | architect | FEAT-WASM-COMPUTE follow-ons + three-layer stable | Tracker (already V1.122) |
| DF-V1122-HARNESS-RENAME (inherited) | Strategy/Preset → Harness product copy | Harness | V1.124+ | product-manager | V1.122 shipped + copy audit | Tracker (already V1.122) |
| DF-V1122-COMPUTABLE-UI (inherited) | Computable pillar UI surfacing | Computable | V1.124+ | product-manager | Dogfood shows authors cannot discover compute | Tracker (already V1.122) |
| DF-70 (inherited) | Settings execution-mode matrix | Harness | V1.105+ still open | product-manager | Settings slice capacity | Tracker §2.3 |
| DF-71 (inherited) | Desktop menu-bar daemon control | Cross-cutting | Any future desktop polish | ops/frontend | Desktop polish slice | Tracker §2.3 |
| DF-46 / DF-47 (inherited) | Capability / host-tool registry completion | Harness | Reduced / narrowed | architect | Capability program revisit | Tracker §2.3 |
| BL-01..09 (inherited) | World merge, shadow read, context DSL, etc. | Canvas / Cross-cutting | Backlog | product-manager | Pillar roadmap prioritizes | Tracker §2.4 |

## Delivery Branch Policy

> Mirror of frontmatter; keep in sync with `{HARNESS_DIR}/status.json` `metadata`.

| Field | Value |
|-------|-------|
| `iteration_base_branch` | `main` |
| `spec_integration_branch` | `iteration/v1.123` |
| `target_branch` | `main` |

Branch resolve evidence (autonomous): `status.json` root metadata (`iteration_base_branch: main`, `target_branch: main`) + V1.118–V1.122 shipped compasses all `main → iteration/vX → main`. `main` is the documented project-policy branch for this repository (`.mstar/AGENTS.md` § Git & PR merge policy: "All landings on the protected branch (`target_branch`, usually `main`) via GitHub PR with squash merge"). Satisfies autonomous branch resolve order step 3 (current git branch is `main` AND `main` is a documented delivery/integration/project-policy branch).

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Three-layer abstraction introduces wire-contract churn beyond V1.122's `wire_contracts_changed: false` baseline | High (realized) | Med (contained) | **Architect seat 2 LOCKED `wire_contracts_changed: true`** with single additive enum value (`BlockType = "era"`) attributable to Brief carrier only — `three-layer-architecture.md` §4. P2/P3/P4 add zero wire diff. 8-point verification gate in §4.3 enforces exactly one `schemas/` file changed, codegen regen limited to `BlockType`, daemon Rust diff empty. |
| Brief carrier choice (Brief-on-KeyBlock vs Brief-on-World vs Brief-on-TimelineEvent) leaks across layers | Med | High | **Architect seat 2 LOCKED Brief-on-KeyBlock** with full trade-off matrix + rejected alternatives in `three-layer-architecture.md` §2. Brief rides existing V1.73 KeyBlock taxonomy — no cross-layer leak (Brief = `block_type=era`, Narrative = `block_type=event`; both partition `entities[]` cleanly in the adapter). |
| Moment carrier (Moment-on-TimelineEvent vs Moment-on-Outline) fragments data ownership | Med | High | **Architect seat 2 LOCKED Moment-on-Outline** with full trade-off matrix + rejected alternatives in `three-layer-architecture.md` §3. Moment is Work-scoped (aligns with locked direction); reuses V1.72 `WorkOutline` as the Work-scoped source of truth — no parallel Work-scoped TimelineEvent table, no scope fragmentation. Honest empty-state until V1.124+ wire extension (`DF-V1123-MOMENT-WIRE`). |
| World-scoped `TimelineEvent` HTTP route promotion (`DF-V1122-DEEPER-WB`) requires daemon Rust changes | High (realized — route NOT promoted) | Low (route not needed) | **Architect seat 2 LOCKED: compose from existing routes; do NOT promote `GET /v1/daemon/worlds/{world_id}/timeline`; do NOT add Work Timeline route** (`three-layer-architecture.md` §5). V1.123 composes Brief + Narrative from existing `kb/graph` and Work Timeline from existing `outline`. `DF-V1122-DEEPER-WB` tracker row updated: stays open with V1.124+ target. |
| Work Timeline peer surface extraction destabilizes Outline+Timeline shipped β (V1.72) | Low | High | **Architect seat 2 LOCKED Work Timeline as a peer `CanvasSurfaceKind = "work-timeline"`** additive to the frontend enum (`canvas-strategy-surface.md` §3.3.3). Outline adapter source is untouched on the P2 branch; P2 regression tests assert `/works/:workId` still redirects to Outline (V1.118). Work Timeline's Moment layer is **read-only** in V1.123 — edits route through Outline (`outline.patch_chapter` / `outline.patch_structure`); no parallel write boundary. |
| Three-layer zoom experience requires heavy animation / virtualization | Med | Med | P4 uses semantic zoom (discrete layer swap), not infinite-viewport zoom; existing `useAutoLayout` + dagre reused per layer. **Architect confirms the 0.55–0.70 semantic-zoom threshold band is feasible** against React Flow `useViewport()` / `onViewportChange` APIs (`canvas-strategy-surface.md` §3.3.3 layer-state persistence + `three-layer-zoom-experience.md` Semantic zoom feasibility). Explicit switcher is the primary affordance; semantic zoom is additive convenience. Performance residual deferred if very large Worlds/Works. |
| Cross-World global Timeline (P3) requires data not exposed today | Med | Med | **Architect LOCKED P3 composes client-side from existing per-World `kb/graph`** (cap N=5–10 most-recent by default). P3 may degrade to "Worlds list + last-edited timestamp" if N+1 fetch cost is prohibitive — honest scope cut documented in completion report (P3 plan Global Constraints → Data composition). |
| Five-plan serial Execute overruns L+ budget | Med | Med | P0 spec lock enables parallel Prepare (P1/P2) during Execute; P3 may Prepare during P1/P2 Execute; serial constraint is per-plan Execute only. Architect LOCK enables P1 to ship the single schema change independently; P2/P3/P4 each have zero wire diff (compose from existing routes). |
| `status.json` size (96KB) crosses 20KB threshold before P-last close | High (already breached) | Low | Trigger `DF-V1123-STATUS-COMPACT` opportunistic compaction pre-P-last; not a business plan; documented in Non-Goals. |
| Accumulated residuals (179 open; 28 from V1.121+V1.122) obscure signal | High | Low | V1.123 does not absorb cleanup as business scope; P0 records tracker entries; compaction + cleanup is V1.124 candidate. |
| Author confusion: Brief vs Narrative vs Moment semantic overlap | Med | Med | Each layer has distinct empty-state copy + in-app layer explanation; PMF demo script is the canonical explainer. **Architect LOCKED distinct carriers + read/write boundaries per layer** reinforce the distinction (Brief = KeyBlock era marker; Narrative = KeyBlock event / WorkOutline event; Moment = Outline Scene/Beat). |
| Timeline-first IA deepening (P3) over-promises and under-delivers | Med | Med | P3 ACs (16–19) are scoped to concrete nav + visual + list-surface changes; "structural prominence" is verified via screenshot + nav tests, not subjective. **Architect LOCKED P3 composition strategy** — client-side composition from existing routes; honest scope cut rule documented if data not exposed. |
| Layer state persistence across surface switches is fragile | Med | Med | P4 AC-V1123-23 tests layer-state persistence via URL query `?layer=brief\|narrative\|moment` (LOCKED in `canvas-strategy-surface.md` §3.3.3). Invalid layer values ignored per layer composition (Moment on World → ignore; Brief on Work → ignore). If fragile, document as residual. |
| Work Timeline Moment-default would surface persistent empty-state (architect-discovered during codebase research) | High (would have realized if product spec §4.3 preference followed) | Med (UX trust erosion) | **Architect UX-risk override:** Work Timeline `defaultLayer: 'narrative'` with Moment one click away (`three-layer-architecture.md` §7.3). Rationale: V1.72 `WorkOutline` wire has no Scene/Beat data today; Moment-default would show empty-state on nearly every Work. Product spec §4.3 explicitly authorized this fallback. Default may flip to Moment when wire extension ships (V1.124+). |
| P1 schema change (`BlockType = "era"`) breaks downstream consumers (`nexus-platform`) | Low (additive enum value) | Low (semver-minor) | Single additive enum value follows V1.54/V1.55 precedent; `nexus-platform` consumes `@42ch/nexus-contracts` via semver lock per repo root `AGENTS.md`. Minor bump is semver-safe for additive enum values; `nexus-platform`'s existing switch/case over `BlockType` continues to work (default arm catches unknown values). 8-point verification gate enforces codegen regen is limited to `BlockType`. |

## Iteration package

> Sibling paths under `{ITERATION_DIR}/v1.123/` — not in `{SPECS_DIR}/` or `{KNOWLEDGE_DIR}/`. Promoted to knowledge at iteration-close via **`mstar-compound`**.

| Path | Purpose |
|------|---------|
| `guides/` | Exploration, process notes |
| `specs/three-layer-architecture.md` | **Architect seat 2 lock:** Brief/Moment carrier choice + `wire_contracts_changed` verdict + daemon route plan + conflict policy + Work Timeline adapter contract |
| `specs/three-layer-product-spec.md` | Author IA + layer semantics + World/Work layer split + demo script + ACs (product-manager seat 1) |
| `specs/layer-feel-differentiation.md` | Per-layer layout/density/visual language spec for P4 (frontend-dev or product-manager seat; architect review) |
| `README.md` | Package document index (recommended; writing-specialist may add) |

## Quality Gate Summary

> Filled at iteration-close. Human summary only; per-plan gate details stay in each main plan, and open residual SSOT stays in `{HARNESS_DIR}/status.json`.

| plan_id | QC decision | QA gate | Residuals | Durable summary |
|---------|-------------|---------|-----------|-----------------|
| P0 three-layer-timeline-spec | _TBD at iteration-close_ | pm-acceptance (docs plan) | _TBD_ | `plans/2026-07-18-v1.123-three-layer-timeline-spec.md#review-gate-summary` |
| P1 world-timeline-brief-narrative | _TBD_ | mandatory | _TBD_ | `plans/2026-07-18-v1.123-world-timeline-brief-narrative.md#review-gate-summary` |
| P2 work-timeline-narrative-moment | _TBD_ | mandatory | _TBD_ | `plans/2026-07-18-v1.123-work-timeline-narrative-moment.md#review-gate-summary` |
| P3 timeline-first-ia-deepening | _TBD_ | mandatory | _TBD_ | `plans/2026-07-18-v1.123-timeline-first-ia-deepening.md#review-gate-summary` |
| P4 three-layer-zoom-experience | _TBD_ | mandatory | _TBD_ | `plans/2026-07-18-v1.123-three-layer-zoom-experience.md#review-gate-summary` |

Notes:

- Raw review bundle: `{SDD_DIR}/review/` (ephemeral; do not rely on it after Done).
- Open residual SSOT: `{HARNESS_DIR}/status.json` root `residual_findings[<plan-id>]`.

## Compound Round Summary

> Filled at iteration-close.

## Iteration Retrospective (minimal)

> Filled at iteration-close.
