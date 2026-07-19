# V1.123 — Three-Layer Zoom Experience Differentiation Spec (iteration-scoped)

> **Status:** Initial draft (product-manager seat 1) → architect seat 2 review → P4 implementer consume.
>
> **Compass reference:** [`../delivery-compass.md`](../delivery-compass.md) § Three-layer model + AC-V1123-20..24.
>
> **Product IA:** [`three-layer-product-spec.md`](./three-layer-product-spec.md) §5–§6.
>
> **Implements in:** plan `2026-07-18-v1.123-three-layer-zoom-experience` (P4); partial feel seeds in P1 (Brief vs Narrative) and P2 (Moment).

## 1. Purpose

Lock the **per-layer feel contract** so Brief / Narrative / Moment are three perceptibly different instruments — not three labels on one layout. Caller's mandate: **三层不一样的感受**.

Product bar (binary): a screenshot pack with Brief | Narrative | Moment side-by-side must be distinguishable without reading layer chrome labels.

## 2. Per-layer feel contract

### 2.1 Summary table

| Layer | Layout | Density | Visual language | Default zoom | Empty-state copy (i18n keys assigned in P4 catalogs) |
|-------|--------|---------|-----------------|--------------|----------------------------------|
| **Brief** | Horizontal era sweep (L→R); era markers as compact cards; **world-shape summary header** above the axis | **Minimal** — era markers only; no Context clusters; no relationship edge clutter | Era-icon + time-span label + **age accent** (gold/bronze / literary "age" tone) | **Far** — whole-world view fits primary viewport | "No era markers yet — switch to Narrative to see events." + short why-Brief line |
| **Narrative** | Balanced event timeline (L→R); event nodes + relationship edges + Context clusters | **Balanced** — V1.122 baseline density | Event-icon + occurred-at + relationship edges (V1.122 tokens) | **Medium** — event-level reading distance | Reuse V1.122 Timeline empty-states (no entities / no events / partial temporal signal) |
| **Moment** | **Vertical** scene-stack or scene-card (T→B preferred); scene/beat cards + manuscript-anchor badges | **Dense** — scene/beat per chapter region; manuscript-anchored | Scene-icon + beat-pin + **manuscript-link badge** (ink-on-paper accent) | **Close** — scene-level reading distance | "No scene/beat data yet — switch to Narrative to see events." + CTA toward Outline beats |

### 2.2 Brief feel (World hero layer)

**Author emotion:** "I can see the world's shape at a glance."

| Dimension | Spec |
|-----------|------|
| **Layout** | Single horizontal when-axis; era markers as sparse landmarks; optional world-shape summary strip (title + one-line sweep) fixed above canvas or in canvas header |
| **Density** | Lowest of three. Target: a multi-decade World readable without pan for ≤ ~12 era markers; beyond that, gentle horizontal scroll — still far less dense than Narrative |
| **Nodes** | Era / age cards only (carrier-locked by architect). No character/place Context clusters on Brief |
| **Edges** | None or minimal succession edges between eras only — **not** full relationship graph |
| **Chrome** | Layer switcher shows Brief as active; breadcrumb `Brief`; optional "world shape" microcopy |
| **Interaction** | Click era → optional drill into Narrative filtered to that span; double-click / "Open Narrative" affordance |
| **Empty** | Do not fake eras. Fallback Narrative + Brief empty-state in layer chrome |
| **Anti-pattern** | Reusing Narrative event nodes at smaller scale and calling it Brief |

### 2.3 Narrative feel (shared layer — V1.122 baseline preserved)

**Author emotion:** "Events in order, at human pace."

| Dimension | Spec |
|-----------|------|
| **Layout** | V1.122 Timeline layout is the **normative baseline** — balanced L→R event axis + Context clusters off-axis |
| **Density** | Medium. Event nodes + relationships + non-event KeyBlocks as context |
| **Nodes** | `block_type=event` (or Work-scoped event projection) on when-axis; other entities as context |
| **Edges** | Relationship edges when present (read rules per architect / V1.122) |
| **Chrome** | Layer labeled Narrative; on World: `Brief > Narrative` breadcrumb when drilled from era; on Work: `Narrative` peer to Moment |
| **Interaction** | Pan/zoom within layer; select event → optional drill to Moment on Work Timeline (P3 cross-surface may own jump; within Work, Narrative→Moment is layer switch) |
| **Empty** | V1.122 copy family preserved (no silent redirect) |
| **Anti-pattern** | Redesigning Narrative into Brief or Moment aesthetics "for consistency" — Narrative is the familiar baseline |

### 2.4 Moment feel (Work hero layer)

**Author emotion:** "Scene precision — I am inside the beat."

| Dimension | Spec |
|-----------|------|
| **Layout** | Prefer **vertical scene-stack** (chapter → scene → beat cards) or dense scene-card grid with clear reading order. Horizontal when-axis is secondary or omitted if it fights density |
| **Density** | Highest of three. Scenes and beats are first-class; time precision is sub-scene |
| **Nodes** | Scene cards + beat pins; manuscript-anchor badges (chapter/scene link) mandatory when anchor data exists |
| **Edges** | Beat succession within scene; light "realizes event" link when bound to Narrative event — not a full World relationship graph |
| **Chrome** | Layer labeled Moment; breadcrumb `Narrative > Moment` when drilled; ink/paper accent distinguishes from World gold Brief |
| **Interaction** | Click beat → focus card; badge → jump to Outline/manuscript anchor (read path); "zoom out" → Narrative |
| **Empty** | Honest Moment empty-state; do not invent beats from chapter titles alone |
| **Anti-pattern** | Horizontal event timeline with smaller nodes labeled "Moment"; missing manuscript anchors when Outline has Scene/Beat data |

## 3. Semantic zoom contract

### 3.1 Principle

Layer change is a **discrete semantic swap** (projection + feel + default zoom), **not** continuous infinite viewport zoom that slowly morphs layouts.

Within a layer, ordinary canvas pan/zoom remains available.

### 3.2 World Timeline — Brief ↔ Narrative

| Trigger | Product intent |
|---------|----------------|
| Explicit layer control | Header segmented control / tabs: Brief \| Narrative |
| Semantic zoom-in past threshold while on Brief | Swap to Narrative (optionally filtered to focused era) |
| Semantic zoom-out past threshold while on Narrative | Swap to Brief |
| Threshold (product intent) | ~ **0.55–0.70** of "fit-all Brief content" scale on the way in; inverse on the way out. **Architect + frontend verify feasibility** against React Flow viewport APIs; if continuous zoom fights discrete swap, prefer explicit control + optional wheel-at-edge gesture |
| Hysteresis | Require clear overshoot both ways so layers do not flicker at the boundary |

### 3.3 Work Timeline — Narrative ↔ Moment

| Trigger | Product intent |
|---------|----------------|
| Explicit layer control | Header: Narrative \| Moment |
| Semantic zoom-in past threshold on Narrative (with event/scene focus) | Swap to Moment (optionally filtered to selected event's scenes) |
| Semantic zoom-out past threshold on Moment | Swap to Narrative |
| Threshold (product intent) | Similar hysteresis band; Moment default camera is closer — zoom-out returns to Narrative medium |
| Feasibility | Architect/frontend may ship explicit-only in P2 and complete semantic zoom in P4 — document if split |

### 3.4 Breadcrumbs

| Surface | Breadcrumb pattern |
|---------|-------------------|
| World Timeline | `Brief` or `Brief > Narrative` (when Narrative entered via era drill) or `Narrative` |
| Work Timeline | `Narrative` or `Narrative > Moment` or `Moment` |

Breadcrumbs are clickable zoom-out targets (parent layer).

### 3.5 What semantic zoom is not

- Not continuous LOD morphing of the same node set
- Not cross-surface navigation (World ↔ Work)
- Not replacing the layer switcher — zoom is additive convenience

## 4. Animation contract

| Preference | Spec |
|------------|------|
| **Preferred** | Framer Motion `AnimatePresence` (or project-standard motion) for node set exit/enter on layer swap — duration ~200–320ms, ease-out |
| **Fallback** | CSS opacity/transform transitions on layer root |
| **Feel** | Transition should read as **changing instrument**, not camera fly-through of one graph |
| **Reduced motion** | Honor `prefers-reduced-motion`: instant swap, no large transforms |
| **Deferral** | If animation slips, ship discrete swap + register residual with rationale; AC-V1123-21 allows documented deferral |

## 5. Layer-state persistence

| Requirement | Spec |
|-------------|------|
| **Survive surface switch** | World Timeline → World KB → back restores Brief/Narrative choice; Work Timeline → Outline → back restores Narrative/Moment |
| **Preferred encoding** | URL query `?layer=brief\|narrative\|moment` on the Timeline route (shareable, refresh-safe) |
| **Secondary** | React context / session store for in-shell switches without full navigation |
| **Invalid layer** | If URL asks for Moment on World Timeline → ignore, use Brief/Narrative; if Brief on Work Timeline → ignore, use Narrative/Moment |
| **Default when absent** | World: Brief-if-data-else-Narrative; Work Timeline: Moment-if-data-else-Narrative (per product-spec §4.3 preference) |
| **Test** | AC-V1123-23 layer-state-persistence test |

## 6. DESIGN.md / tokens.css impact

### 6.1 Proposed tokens (product intent)

| Token | Role |
|-------|------|
| `--color-canvas-layer-brief-accent` | Era/age accent (gold-bronze literary tone) |
| `--color-canvas-layer-narrative-accent` | May alias existing Timeline/event accent (V1.122) — only add if Narrative needs distinct chrome |
| `--color-canvas-layer-moment-accent` | Ink-on-paper / manuscript scene accent |
| Optional density tokens | `--space-canvas-layer-brief-gap`, `--space-canvas-layer-moment-gap` if spacing diverges sharply |

### 6.2 Registration

- Define in brand/token pipeline per DESIGN.md / `tokens.css` conventions.
- Register utility aliases in `packages/nexus-ui/src/lib/cn.ts` (tailwind-merge / custom groups) when classes land — same class of fix as V1.94 font-size token registration.
- If tokens promote to design-system SSOT, update `DESIGN.md` / `DESIGN.dark.md` (light + dark paired values).

### 6.3 Visual QA

- Design-studio gallery optional but recommended for layer accent swatches.
- P4 screenshot pack is the acceptance evidence (AC-V1123-20), not DESIGN.md alone.

## 7. Honest empty-state copy (product voice)

Final i18n keys live in catalogs; product voice:

| Layer | Empty title (EN intent) | Empty body (EN intent) |
|-------|-------------------------|------------------------|
| Brief | No era markers yet | Brief shows the world's shape across ages. Switch to Narrative to browse events, or add era markers when the Brief carrier is ready. |
| Narrative | (V1.122 family) | Preserve existing empty / no-events / partial-temporal copy |
| Moment | No scene or beat data yet | Moment is scene-precise and manuscript-anchored. Add scenes and beats in Outline, or switch to Narrative for events. |

Tone: literary, honest, non-blaming — aligned with V1.121 "Literary Engine" elevation.

## 8. Implementation phasing

| Plan | Feel obligation |
|------|-----------------|
| **P1** | Brief vs Narrative **seed** differentiation (layout + density + empty-state); full polish may wait for P4 |
| **P2** | Moment **seed** differentiation (vertical/dense + anchors + empty-state) |
| **P3** | Shell/list prominence — not layer feel, but must not flatten layer chrome |
| **P4** | Full contract: tokens, semantic zoom, animation, breadcrumbs, persistence, screenshot pack |

## 9. Open for architect / frontend (do not block seat 1)

1. Exact zoom threshold numbers vs React Flow API feasibility.
2. Whether Moment layout is pure vertical stack vs hybrid with mini when-axis.
3. Token names finalization and dark-theme pair values.
4. Whether P2 ships Moment seed without semantic zoom (explicit switch only until P4).
