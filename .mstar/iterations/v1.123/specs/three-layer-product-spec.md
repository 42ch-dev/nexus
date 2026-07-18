# V1.123 — Three-Layer Timeline Product Spec (iteration-scoped)

> **Status:** Draft (Phase 1 product-manager seat 1). Implements the locked direction: Brief · Narrative · Moment as Timeline's three zoom layers with World/Work domain split.
>
> **Compass reference:** [`../delivery-compass.md`](../delivery-compass.md) § Three-layer model + Author IA + Acceptance Criteria.
>
> **Depends on:** V1.122 Timeline-first Canvas shipped (`iterations/v1.122/specs/timeline-hero-product-spec.md`).
>
> **Implements in:** plan `2026-07-18-v1.123-three-layer-timeline-spec` (P0 spec refactor) → plans P1/P2/P3/P4 (code).
>
> **Peer technical lock:** [`three-layer-architecture.md`](./three-layer-architecture.md) (architect seat 2 — Brief/Moment carrier, routes, wire verdict).
>
> **Peer feel contract:** [`layer-feel-differentiation.md`](./layer-feel-differentiation.md) (P4 layout/density/zoom).

## 1. Purpose

V1.122 made Timeline the World-entry hero — but shipped a **single-granularity event timeline** (`block_type=event` KeyBlocks on a when-axis). Authors who want a century-scale world shape and authors who want scene-level precision land on the same undifferentiated surface.

V1.123 deepens Timeline into **three instruments at three scales**:

| Layer | Author problem it solves |
|-------|--------------------------|
| **Brief** | "What is the shape of this world's history?" — multi-decade / era sweep without reading every event |
| **Narrative** | "What events happened, in order, at human pace?" — today's V1.122 event timeline, reframed as one of three layers |
| **Moment** | "What happens in *this exact scene*?" — scene/beat precision, manuscript-anchored (Work hero) |

**Domain split (locked product semantics — do not invert):**

```
World Timeline = Brief (world shape) + Narrative (events)
Work Timeline  = Narrative (events)  + Moment (scenes)
```

**Why the split:** A World is a narrative universe — its spine is the world shape (Brief). A Work is a specific manuscript — its unit is the scene being written (Moment). Narrative is the shared bridge: events belong to both world history and chapter realization.

**What this iteration must make author-feelable (not docs-only):**

1. Open a World → see **Brief** (or honest Narrative fallback)
2. Switch to **Narrative** on World Timeline
3. Open a Work → still land on **Outline** (unchanged)
4. Reach **Work Timeline** as a peer → see Narrative + **Moment**
5. Feel three distinct layer languages (P4)
6. Feel Timeline as structurally prominent, not just a route default (P3)

Without P1 + P2 + P3 shipping code, V1.123 collapses to Candidate D (spec-only) — a hollow abstraction.

## 2. Layer semantics (author voice)

### 2.1 Definitions

| Layer | Author language | Granularity | Time span | Primary domain | Hero change this iteration |
|-------|-----------------|-------------|-----------|----------------|---------------------------|
| **Brief** | "the shape of the world's history at a glance" — era/decade markers, world-global summaries | World-global | Multi-decade / era / age | **World** (World Brief = world-history-at-a-glance) | **P0+P1:** canonize + implement on World Timeline |
| **Narrative** | "events happened, in order, at human pace" — a battle, a treaty, a journey | Event-level | Human-paced (days/weeks/years) | **Shared** (World and Work) | **P0+P1+P2:** lock as shared layer; V1.122 Timeline becomes Narrative |
| **Moment** | "what happens in this exact scene" — beat precision, manuscript-anchored | Scene/beat-precise | Sub-scene (minutes/hours within a scene) | **Work** (Work Moment = scene-precision) | **P0+P2:** canonize + implement on Work Timeline |

**Author voice examples (copy guidance for empty-states and layer chrome):**

| Layer | Voice sample |
|-------|--------------|
| Brief | "In the Age of Stars, the kingdoms rose." |
| Narrative | "On Midsummer's Eve, the treaty was signed." |
| Moment | "She paused at the door, then spoke the name." |

### 2.2 Cross-references to existing CONCEPTS.md

| This entry | Related existing concept | Disambiguation |
|------------|--------------------------|----------------|
| **Brief** | [Timeline](../../../../CONCEPTS.md#timeline), [Timeline-first World building](../../../../CONCEPTS.md#timeline-first-world-building) | Brief is a **Timeline zoom layer**, not a separate container. Closest pre-V1.123 idea was World Summary / Story Manifesto — those remain prose summaries; Brief is a **when-axis projection**. |
| **Narrative** (Timeline layer) | [Timeline](../../../../CONCEPTS.md#timeline), prose "narrative writing" | **Not** "narrative writing" (prose craft). Narrative here means the **event-level Timeline layer** — human-paced events on the when-axis. |
| **Moment** (Timeline layer) | [Moment Context Assembly](../../../../CONCEPTS.md#moment-context-assembly), scope hierarchy `World > Timeline > Event > Moment` | **Not** Moment Context Assembly (session context packing for agents). This Moment is a **Timeline layer** — scene/beat nodes on Work Timeline. Scope-tree Moment remains valid; V1.123 adds Canvas projection. |

### 2.3 Scope hierarchy alignment

Existing entity-scope model (`entity-scope-model.md` §1.1):

```
World > Timeline > Event > Moment
```

Three-layer re-projection (product intent; architect seat 2 locks carriers):

| Scope level | Timeline layer | Notes |
|-------------|----------------|-------|
| World (global summary on Timeline) | **Brief** | New product surface of world-shape; not a new top-level container |
| Event | **Narrative** | V1.122 baseline (`block_type=event` KeyBlocks) |
| Moment | **Moment** | Elevated from session-context-only to Work Timeline layer |

Hierarchy is **preserved**, not replaced. Architect Draft overlay on `entity-scope-model.md` canonizes Brief + re-projects Moment.

### 2.4 Out of scope per domain (explicit non-goals)

| Layer × domain | V1.123 status | Tracker |
|----------------|---------------|---------|
| World × Moment | Out of scope — Moment-on-World stays session-context | `DF-V1123-WORLD-MOMENT` |
| Work × Brief | Out of scope — Work Brief is Outline's job today | `DF-V1123-WORK-BRIEF` |
| Rich era taxonomy | MVP = era markers only | `DF-V1123-ERA-TAXONOMY` |

## 3. World Timeline IA (Brief + Narrative)

### 3.1 Mental model (World spine, post-V1.123)

```
World spine (V1.122 locked, extended in V1.123)
├── World Timeline (V1.122 hero, V1.123 deepened)
│   ├── Brief layer (V1.123 NEW — world shape)     ← default when Brief data exists
│   ├── Narrative layer (V1.122 event timeline reframed)
│   └── [Moment — out of scope for World]
├── World KB (peer, unchanged)
├── Strategy (peer, unchanged)
├── Outline (Timeline-companion peer, unchanged)
└── Forks (peer, marker-only V1.122; create/merge still deferred)
```

### 3.2 Entry defaults

| Context | Route today (V1.122) | Route after V1.123 | Notes |
|---------|----------------------|---------------------|-------|
| **World entry** | `/worlds/:worldId` → Timeline (event list only) | `/worlds/:worldId` → **World Timeline Brief** if Brief data exists; else **Narrative** + honest Brief empty-state | Worlds list pick-target stays Timeline; only **default layer** changes |
| **Canvas shell peers (World)** | Strategy / Outline+Timeline-companion / Timeline / World KB | Same peers; Timeline gains **Brief↔Narrative layer switcher** | P1 IA change |

### 3.3 Layer switcher (World)

- **Placement:** Timeline canvas header (layer chrome + breadcrumbs).
- **Modes:** explicit switch (Brief | Narrative) and/or semantic zoom (P4) that swaps layers at thresholds.
- **One-click rule:** Brief is one click from Narrative and vice versa.
- **Default:** Brief when era markers / world-shape data exist; else Narrative with empty-state explaining Brief intent.

### 3.4 Reachability (must hold after P1; P3 deepens prominence)

1. From World Timeline Brief → Narrative via layer switcher.
2. From World Timeline Narrative → Brief via layer switcher (when Brief data exists) or Brief empty-state.
3. World KB remains a peer from World Timeline.
4. Strategy remains a peer from World Timeline.
5. Outline (Timeline-companion) remains a peer — not deleted.
6. Work entry still defaults to Outline (regression — non-negotiable).

### 3.5 Honest empty-states (World)

| Condition | Behavior |
|-----------|----------|
| No Brief data (no era markers / world shape) | Default to Narrative; show Brief empty-state in layer chrome: *why Brief exists* + how to add era markers (carrier-dependent CTA — architect locks write path) |
| No Narrative data (no events) | Reuse V1.122 Timeline empty-state (events via World KB / extraction) |
| Brief data exists but sparse | Render available era markers; do not fabricate eras from `updated_at` or entity counts |

### 3.6 Product locks (World Timeline — non-negotiable for P1)

| Lock | Value |
|------|--------|
| Hero context | **World entry** remains Timeline (V1.122) |
| Default layer | **Brief** if data exists, else Narrative |
| Layer pair | Brief + Narrative only |
| Peer surfaces | Strategy, Outline (companion), Timeline, World KB |
| Work entry | Outline unchanged |
| Fork authoring | Still markers/badge only — no create/merge UI |
| Compute | No compute-on-timeline |

## 4. Work Timeline IA (Narrative + Moment)

### 4.1 Mental model (Work projection, post-V1.123)

```
Work projection (V1.118 locked, V1.123 deepened)
├── Outline (V1.118 default for Work entry — UNCHANGED)
├── Manuscript / Reading (unchanged)
├── Work Timeline (V1.123 NEW — peer surface)
│   ├── [Brief — out of scope for Work]
│   ├── Narrative layer (events realized in this Work + bound World events)
│   └── Moment layer (scene/beat precision, manuscript-anchored)  ← Work hero layer
└── Strategy / World KB (peers, unchanged inheritance)
```

### 4.2 Entry defaults

| Context | Route today (V1.122) | Route after V1.123 | Notes |
|---------|----------------------|---------------------|-------|
| **Work entry** | `/works/:workId` → Outline | **Unchanged** → Outline | Explicit non-goal to flip Work entry to Timeline |
| **Work Timeline access** | Not available | `/works/:workId/timeline` (or equivalent peer route) from Work Canvas shell | Peer surface, **not** Work default |
| **Canvas shell peers (Work)** | Strategy / Outline / World KB | Strategy / **Outline** / **Work Timeline (NEW)** / World KB | Fourth peer |

Exact path segment is implementer choice; **product acceptance** is: Work Timeline is reachable as a peer; Work pick still opens Outline.

### 4.3 Layer switcher (Work)

- **Placement:** Work Timeline canvas header.
- **Modes:** Narrative | Moment switch + semantic zoom (P4).
- **One-click rule:** Moment is one click from Narrative and vice versa.
- **Default layer on Work Timeline:** product preference = **Moment** when Scene/Beat data exists (Work-hero layer), else Narrative with Moment empty-state. *(If implementer evidence shows Moment-default confuses empty Works, fall back to Narrative-default with Moment one click away — document in P2 plan; do not flip Work **entry** away from Outline.)*

### 4.4 Reachability (must hold after P2)

1. Work Timeline reachable from Work Outline / Work Canvas shell nav.
2. Outline always reachable from Work Timeline — no dead-end hero.
3. From Work Timeline Moment → Narrative via layer switcher.
4. From Work Timeline Narrative → Moment via layer switcher (or Moment empty-state).
5. World Timeline unaffected (regression).
6. Work → Outline default preserved (regression).

### 4.5 Honest empty-states (Work)

| Condition | Behavior |
|-----------|----------|
| No Moment data (no Scene/Beat outline nodes) | Fall back to Narrative; Moment empty-state explains scene precision + points to Outline beats |
| No Narrative data (no Work-scoped events / bound World events) | Honest empty-state: how events appear on Work Timeline (carrier-dependent) |
| Partial manuscript anchors | Show moments that have anchors; badge missing anchors honestly — do not invent chapter links |

### 4.6 Product locks (Work Timeline — non-negotiable for P2)

| Lock | Value |
|------|--------|
| Work entry default | **Outline** (V1.118) |
| Work Timeline role | **Peer surface**, not default |
| Layer pair | Narrative + Moment only |
| Surface kind | New `CanvasSurfaceKind = "work-timeline"` (architect locks adapter contract) |
| Manuscript editing | No new TipTap / whole-document editor — Moment is node-granular |
| Outline | Not removed; remains Work entry hero |

## 5. Cross-layer navigation

### 5.1 Within-Timeline (layer switch — this section)

Cross-layer navigation is **within one Timeline surface**. It is **not** cross-surface jump (World Timeline ↔ Work Timeline) — that is P3 IA scope.

| Direction | Surface | Author intent | Behavior |
|-----------|---------|---------------|----------|
| Brief → Narrative | World Timeline | "Drill into this era" | Narrative filters (or focuses) events within the era's time span when an era is selected; otherwise full Narrative |
| Narrative → Brief | World Timeline | "Zoom out to world shape" | Brief becomes prominent layer |
| Narrative → Moment | Work Timeline | "Drill into this scene" | Moment filters to moments realized by the selected event/chapter when bound; otherwise full Moment stack |
| Moment → Narrative | Work Timeline | "Zoom out to events" | Narrative becomes prominent layer |

### 5.2 Semantic zoom vs viewport zoom

- Layer change is a **semantic** transition (swap projection + feel), not infinite continuous viewport zoom alone.
- P4 owns thresholds, animation, breadcrumbs, and layer-state persistence.
- Viewport pan/zoom **within** a layer remains available (V1.122 canvas patterns).

### 5.3 Cross-surface (P3 — not layer switcher)

| Direction | Owner | Intent |
|-----------|-------|--------|
| Work Timeline Moment → bound World Timeline Narrative | P3 | Same event in world history |
| World Timeline Narrative → Work Timeline Moment realizing it | P3 | Jump to scene precision in a Work |
| Global Timeline overview ↔ World/Work Timelines | P3 | "Timeline 一定要突出" structural prominence |

Layer switcher UI must **not** pretend to be cross-surface navigation. Cross-surface uses explicit jump affordances (P3).

### 5.4 Layer-state persistence (product intent)

- Active layer survives temporary surface switches (e.g., World Timeline → World KB → back to Timeline restores Brief/Narrative choice).
- Preferred mechanism: URL query (`?layer=brief|narrative|moment`) and/or durable React context — architect/frontend choose; P4 AC-V1123-23 verifies.

## 6. Demo script (PMF)

**Required for iteration Done.** Without this path shipping, V1.123 is a hollow abstraction.

### 6.1 Preconditions

- Daemon healthy; Profile active; at least one World with some Brief-capable data **or** ready to show Brief empty-state.
- At least one Work bound to that World with Outline Scene/Beat data **or** ready to show Moment empty-state.
- Capture **light + dark** screenshots at each marked step.

### 6.2 Steps

| # | Action | Expect |
|---|--------|--------|
| 1 | Launch app | Control Room ready |
| 2 | Creation → **Worlds** list | Worlds list shows Timeline-oriented activity cues when P3 shipped; otherwise list still opens Timeline on pick |
| 3 | Pick a World | **World Timeline** opens |
| 4 | Observe default layer | **Brief** renders (era markers / world shape) **or** Narrative + honest Brief empty-state |
| 5 | Switch to **Narrative** | Event timeline (V1.122 feel, now labeled Narrative); layer chrome shows Brief \| Narrative |
| 6 | Open peer **World KB** | Entity graph still works; return to Timeline preserves layer when P4 persistence ships |
| 7 | Open a **Work** from Works list | Lands on **Outline** (unchanged) |
| 8 | Work Canvas shell → **Work Timeline** | Peer surface opens (not replacing Outline default) |
| 9 | Observe Work Timeline layers | **Narrative** and/or **Moment** per default rule; switcher present |
| 10 | Switch to **Moment** | Scene/beat precision + manuscript-anchor badges **or** Moment empty-state |
| 11 | Switch back to **Narrative** | Events restored |
| 12 | Return to Outline | Always reachable |
| 13 | Global nav → **global Timeline view** (P3) | Cross-World recent Timeline activity (read-only overview) |
| 14 | Return to a World Timeline | Still hero for World entry |

### 6.3 Screenshot pack checklist

- [ ] World Timeline Brief (light + dark)
- [ ] World Timeline Narrative (light + dark)
- [ ] Brief empty-state (if no Brief data fixture)
- [ ] Work Outline entry (regression)
- [ ] Work Timeline Narrative (light + dark)
- [ ] Work Timeline Moment (light + dark)
- [ ] Moment empty-state (if no Scene/Beat fixture)
- [ ] Layer switcher chrome / breadcrumbs
- [ ] Global Timeline entry (P3)
- [ ] Side-by-side Brief | Narrative | Moment feel comparison (P4)

## 7. Acceptance criteria mapping

Each compass AC maps to product behavior. Evidence types stay as in compass; this section states **product intent**.

### P0 — Spec & contract (docs)

| AC | Product intent |
|----|----------------|
| **AC-V1123-1** | Strategy corpus names three layers + World/Work split so external readers can restate the thesis without the compass. |
| **AC-V1123-2** | Domain vocabulary distinguishes Brief / Narrative / Moment as Timeline layers and disambiguates Narrative (prose) and Moment Context Assembly. |
| **AC-V1123-3** | Scope model stays `World > Timeline > Event > Moment` while Brief is canonized and Moment gains Timeline-layer meaning — no hierarchy rewrite. |
| **AC-V1123-4** | Canvas surface contract admits layer switchers on World Timeline and a Work Timeline peer with Narrative/Moment — without deleting V1.122 Timeline β text. |
| **AC-V1123-5** | Architect locks carriers/routes/wire/conflict so P1/P2 implement against a single contract (product does not pick DTOs). |
| **AC-V1123-6** | Deferred inventory is durable (tracker rows); deeper-WB status reflects whether V1.123 promotes the World timeline route. |

### P1 — World Timeline Brief + Narrative

| AC | Product intent |
|----|----------------|
| **AC-V1123-7** | Author opening a World meets Brief-first (or honest fallback) and can switch to Narrative — the headline World PMF signal. |
| **AC-V1123-8** | Writes respect locked carriers; no silent wire drift — trust boundary for structured World edits. |
| **AC-V1123-9** | Quality bar: green builds/tests for touched surfaces. |
| **AC-V1123-10** | Layer switcher + peer reachability; Work→Outline regression holds. |

### P2 — Work Timeline Narrative + Moment

| AC | Product intent |
|----|----------------|
| **AC-V1123-11** | Work gains a real Timeline peer; Outline remains entry — authors can plan chapters and still reach scene-precision Timeline. |
| **AC-V1123-12** | Moment layer delivers scene/beat precision with manuscript anchors; empty Moment is honest. |
| **AC-V1123-13** | Work Timeline writes follow the same locked contract discipline as World. |
| **AC-V1123-14** | Quality bar for P2 surfaces. |
| **AC-V1123-15** | Moment↔Narrative + Outline reachability; World Timeline regression holds. |

### P3 — Timeline-first IA deepening

| AC | Product intent |
|----|----------------|
| **AC-V1123-16** | Timeline is one click from anywhere via global entry — "Timeline 一定要突出" beyond World route default. |
| **AC-V1123-17** | Lists and shell chrome surface Timeline activity so Timeline feels structural, not buried. |
| **AC-V1123-18** | Bound cross-surface jumps connect Work Moments to World Narrative (and reverse) when data binds — layer switcher is not abused for this. |
| **AC-V1123-19** | Quality + V1.122 route regression. |

### P4 — Three-layer zoom feel

| AC | Product intent |
|----|----------------|
| **AC-V1123-20** | Three layers are **perceptibly different** instruments (layout/density/visual language) — not three labels on one layout. |
| **AC-V1123-21** | Layer transitions are semantic (threshold swap + animation or documented deferral), not fake continuous zoom. |
| **AC-V1123-22** | Each layer teaches its own intent via empty-state copy. |
| **AC-V1123-23** | Drill/zoom-out/breadcrumbs work; layer choice persists across surface round-trips. |
| **AC-V1123-24** | Quality bar for P4. |

### PMF demo path (iteration Done)

Compass demo script (§6 above) is mandatory evidence for iteration Done — binary path walk + light/dark pack, not subjective "feels layered."

## 8. Non-goals (product emphasis)

Full list: compass § Non-Goals. Local emphasis for implementers:

- No Work-entry flip to Timeline
- No World-entry flip away from Timeline
- No World Moment layer; no Work Brief layer
- No Fork create/merge; no compute-on-timeline
- No Outline removal; no new whole-document editor
- No cross-World Timeline merge (P3 overview is read-only)
- No Phase 1 new knowledge docs (tracker update only)

## 9. Open for architect (seat 2) — product does not LOCK

1. Brief data carrier (Brief-on-KeyBlock vs Brief-on-World vs Brief-on-TimelineEvent).
2. Moment data carrier (Moment-on-TimelineEvent vs Moment-on-Outline).
3. `wire_contracts_changed` verdict + enumerated schema/route changes.
4. Daemon route plan — promote `GET /v1/daemon/worlds/{world_id}/timeline`? new Work Timeline route vs compose outline + world-kb?
5. Conflict policy reuse vs Timeline-specific DTOs.
6. `WorkTimelineCanvasAdapter` TypeScript contract (V1.114 recipe).
7. Whether Work Timeline **default layer** is Moment-first or Narrative-first when both have data (product preference stated in §4.3; architect may override with UX risk note).

## 10. Open for writing-specialist (seat 3)

- Terminology sweep: Brief / Narrative / Moment consistency across `.mstar/specs/` after architect overlays land.
- Ensure "Narrative" never collides with prose-craft sense without disambiguation on first use in long docs.
- Ensure "Moment" never collides with Moment Context Assembly without disambiguation.
