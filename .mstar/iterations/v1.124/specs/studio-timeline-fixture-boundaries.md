# Studio Timeline Fixture Boundaries (V1.124 architect contract)

**Status:** Locked (iteration-scoped) — architect seat 2 of Phase 1 Review & Edit chain  
**Document class:** Iteration package working spec (not `{SPECS_DIR}` Master)  
**Audience:** P0 implementers, QC, P3 promotion classification  
**Authority:** Root `AGENTS.md` § UI Component Policy; V1.115 `@web-canvas/node-chrome-shell` precedent; V1.106 Studio-first promotion workflow  
**Consumers:** P0 Tasks 2–4; P2 Layer breadcrumb coordination; P3 classification table  
**Out of scope:** Global Timeline (→ `surface-audit-checklist.md`); token gallery (→ `tokens-gallery-audit.md`); `@42ch/nexus-ui` package promotion (→ P3 / NG-1)

---

## 1. Purpose

Lock, per Timeline node kind:

1. **Where chrome source lives** (App RF wrapper vs presentational extract)
2. **What Studio imports** (`@web-canvas/*` path)
3. **Which layer / surface accent tokens** the fixture must show

So P0 implementers do not re-litigate extract vs hand-mirror, and F4 ("same extract as App") is technically enforceable.

---

## 2. Source inventory (verified 2026-07-19)

| Module | Path | RF? | Daemon/contracts? | Presentational shell today |
|--------|------|-----|-------------------|----------------------------|
| World Timeline nodes | `apps/web/src/components/canvas/timeline-canvas/timeline-node-types.tsx` | **Yes** (`NodeProps`, `Handle`, `Position`) | Indirect (`BLOCK_TYPE_LABELS` from `world-kb/types` → contracts; `useTranslation`) | Composes `NodeChromeShell` only — **body chrome is inline in RF file** |
| Work Timeline nodes | `apps/web/src/components/canvas/work-timeline-canvas/work-timeline-node-types.tsx` | **Yes** | `useTranslation` only (no contracts import) | Same — `NodeChromeShell` + inline body |
| Shared shell | `apps/web/src/components/canvas/presentational/node-chrome-shell.tsx` | **No** | **No** | Already Studio-reachable as `@web-canvas/node-chrome-shell` (V1.115) |

**RF-coupling status (all six node kinds):** the **wrappers** are RF-coupled and stay App-local forever under V1.106. The **visual body** (icon + title + badge row + optional summary) is pure markup + tokens and **is extractable**.

---

## 3. Locked extract path (decision #1)

### Selected approach

| Layer | Decision | Location / import |
|-------|----------|-------------------|
| **Card shell** | **Direct-consume existing extract** | `@web-canvas/node-chrome-shell` (`NodeChromeShell`, `NodeStatus`, status maps) |
| **Timeline body chrome** | **New presentational extract** (single module, multiple exports) | `apps/web/src/components/canvas/presentational/timeline-node-chrome.tsx` → `@web-canvas/timeline-node-chrome` |
| **RF wrappers** | **Stay App-local** | `timeline-node-types.tsx` / `work-timeline-node-types.tsx` become thin: `NodeChromeShell` + `Handle`s + body extract + RF `selected`/`dragging` |

### Rejected alternatives

| Option | Why rejected |
|--------|----------------|
| Hand-mirror bodies only in Studio (V1.121 Outline sample style) | Fails product F4 under Timeline's denser badge/icon chrome — drift risk higher than Volume/Chapter samples |
| New `@web-canvas/timeline-node-chrome` **alias root** | Unnecessary — existing `@web-canvas/*` already resolves `canvas/presentational/*` |
| Promote bodies to `@42ch/nexus-ui` | Premature (NG-1); RF wrappers still App-local; single-consumer until Studio is the second consumer — P3 may later promote **only if** ≥2 pure consumers + stable props (unlikely this iteration) |
| Extend `NodeAccent` with `'timeline' \| 'brief' \| …` | Wrong model. Source comments (V1.123 P4 T2) lock **surface spine** (`worldkb` / `outline`) separate from **intra-surface layer accents** (text/badge token classes). Do not conflate |

### Alias / guardrail blast radius (P0)

| Change | Required in P0? |
|--------|-----------------|
| New file under `canvas/presentational/` | **Yes** |
| New Vite/tsconfig alias root | **No** — `@web-canvas/timeline-node-chrome` works via existing `@web-canvas/*` |
| `tooling/check-ui-guardrails.sh` allowlist | **No** for new alias root |
| `apps/design-studio/AGENTS.md` alias table row text | **Yes (docs)** — extend `@web-canvas/*` description to name `timeline-node-chrome` + `node-chrome-shell` |
| Tailwind content paths | **No** — `presentational/**` already scanned |

---

## 4. Per-node-kind contract

### Legend

- **Classification (V1.106 four-bucket):** fixture path this iteration = `studio-local fixture` via extract; RF wrapper = `web-only wrapper` (not fixture-ed alone).
- **Surface spine** = `NodeChromeShell` `accent` prop (surface identity stripe).
- **Layer accent** = intra-surface token on icon / badge (Brief / Narrative / Moment feel).

### 4.1 World Timeline — Brief-era (`timeline-brief-era`)

| Field | Locked value |
|-------|----------------|
| App RF wrapper | `TimelineBriefEraNode` in `timeline-node-types.tsx` |
| Extract export | `TimelineBriefEraChrome` from `@web-canvas/timeline-node-chrome` |
| Studio import | `NodeChromeShell` + `TimelineBriefEraChrome` |
| Surface spine (`accent`) | `"worldkb"` (Timeline is World-scoped) |
| Layer accent token | `--color-canvas-layer-brief-accent` (icon + time-span badge) |
| Presentational props (minimum) | `title: string`; `blockTypeLabel: string`; `timeSpan: string \| null` (null → temporal-unknown pill); `eraId?: string`; `worldSummary?: string`; `sourceAnchorCount: number`; `version: number` |
| Fixture variants (F3) | selected on/off; dragging on/off; time-span present / start-only / end-only / unknown; with/without `worldSummary`; source count 0 vs N |
| Product vocabulary labels | Brief, Era, Timeline, KeyBlock (block type label), temporal-unknown |

### 4.2 World Timeline — Event (`timeline-event`)

| Field | Locked value |
|-------|----------------|
| App RF wrapper | `TimelineEventNode` |
| Extract export | `TimelineEventChrome` |
| Surface spine | `"worldkb"` |
| Layer accent token | `--color-canvas-layer-narrative-accent` on temporal badge when dated; surface continuity still worldkb spine. (Source today uses `canvas-worldkb-accent` on the occurred-at pill — **extract must migrate badge to `canvas-layer-narrative-accent`** so Brief vs Narrative reads as layer instruments per V1.123 P4 intent; RF wrapper picks up the same extract.) |
| Presentational props | `title`; `blockTypeLabel`; `occurredAtHint: string \| null`; `sourceAnchorCount`; `version` |
| Fixture variants | selected/dragging; dated vs temporal-unknown; source count 0 vs N |

### 4.3 World Timeline — KeyBlock Context cluster (`timeline-key-block`)

| Field | Locked value |
|-------|----------------|
| App RF wrapper | `TimelineKeyBlockNode` |
| Extract export | `TimelineKeyBlockChrome` |
| Surface spine | `"worldkb"` |
| Layer accent | No dedicated layer badge required — distinct by **absence** of temporal/era chrome + block-type pill only (Context cluster, not when-axis event) |
| Presentational props | `title`; `blockTypeLabel`; `sourceAnchorCount`; `version` |
| Fixture variants | selected/dragging; ≥2 block-type labels (e.g. character vs organization) to prove cluster diversity |

### 4.4 Work Timeline — Narrative event (`work-timeline-narrative-event`)

| Field | Locked value |
|-------|----------------|
| App RF wrapper | `WorkTimelineNarrativeEventNode` |
| Extract export | `WorkTimelineNarrativeEventChrome` |
| Surface spine | `"worldkb"` (Timeline-family continuity) |
| Layer accent token | `--color-canvas-layer-narrative-accent` on leading `Flag` icon **and** chapter-anchor badge (source today uses `canvas-worldkb-accent` on icon/badge — **same migration as §4.2** into the extract) |
| Presentational props | `title`; `eventId`; `chapterAnchor: string \| null` (null → "No chapter anchor"); `description?: string` |
| Fixture variants | selected/dragging; with/without chapter anchor; with/without description |

### 4.5 Work Timeline — Moment scene (`work-timeline-moment-scene`)

| Field | Locked value |
|-------|----------------|
| App RF wrapper | `WorkTimelineMomentSceneNode` |
| Extract export | `WorkTimelineMomentSceneChrome` |
| Surface spine | `"outline"` (outline-derived Work surface identity — locked in source header) |
| Layer accent token | `--color-canvas-layer-moment-accent` (icon + manuscript-anchor badge) |
| Presentational props | `title`; `sceneId`; `manuscriptAnchorLabel: string \| null`; `status?: string` |
| Fixture variants | selected/dragging; with/without manuscript anchor; optional status chip |

### 4.6 Work Timeline — Moment beat (`work-timeline-moment-beat`)

| Field | Locked value |
|-------|----------------|
| App RF wrapper | `WorkTimelineMomentBeatNode` |
| Extract export | `WorkTimelineMomentBeatChrome` |
| Surface spine | `"outline"` |
| Layer accent token | `--color-canvas-layer-moment-accent` |
| Presentational props | `title`; `manuscriptAnchorLabel: string \| null`; `status?: string` |
| Fixture variants | selected/dragging; with/without manuscript anchor (mandatory when data exists in App — fixture shows both) |

**Product AC note:** Compass S1 / AC-V1124-1 names "Work Timeline Narrative + Moment". Moment = **scene + beat** frames (both required). Do not ship scene-only.

---

## 5. Extract module shape (Interfaces)

```ts
// apps/web/src/components/canvas/presentational/timeline-node-chrome.tsx
// MUST NOT import: @xyflow/react, @42ch/nexus-contracts, react-router, NexusClient, app providers
// MAY import: react, lucide-react, @/lib/utils (cn), NodeChromeShell types only if needed (prefer not nesting shell)

export interface TimelineBriefEraChromeProps { /* §4.1 */ }
export function TimelineBriefEraChrome(props: TimelineBriefEraChromeProps): JSX.Element;

export interface TimelineEventChromeProps { /* §4.2 */ }
export function TimelineEventChrome(props: TimelineEventChromeProps): JSX.Element;

export interface TimelineKeyBlockChromeProps { /* §4.3 */ }
export function TimelineKeyBlockChrome(props: TimelineKeyBlockChromeProps): JSX.Element;

export interface WorkTimelineNarrativeEventChromeProps { /* §4.4 */ }
export function WorkTimelineNarrativeEventChrome(props: WorkTimelineNarrativeEventChromeProps): JSX.Element;

export interface WorkTimelineMomentSceneChromeProps { /* §4.5 */ }
export function WorkTimelineMomentSceneChrome(props: WorkTimelineMomentSceneChromeProps): JSX.Element;

export interface WorkTimelineMomentBeatChromeProps { /* §4.6 */ }
export function WorkTimelineMomentBeatChrome(props: WorkTimelineMomentBeatChromeProps): JSX.Element;
```

### App RF wrapper pattern (mandatory after extract lands)

```tsx
// Pseudocode — App side
<NodeChromeShell selected={selected} dragging={dragging} accent="worldkb">
  <Handle … />
  <TimelineEventChrome title={…} … />  {/* strings already resolved via t() in wrapper */}
  <Handle … />
</NodeChromeShell>
```

- **i18n stays in the RF wrapper** (or adapter). Extract receives **resolved strings** only — Studio passes static English product vocabulary (NG-7).
- **Handles stay in the RF wrapper** — never in the extract, never in Studio fixtures.
- **`BLOCK_TYPE_LABELS` / contracts stay in the RF wrapper** — fixture passes plain `blockTypeLabel` strings (`"Event"`, `"Era"`, `"Character"`, …).

### Studio fixture pattern (mandatory)

```tsx
// Pseudocode — Studio side
import { NodeChromeShell } from '@web-canvas/node-chrome-shell'; // transitional annotation on specifier line
import { TimelineEventChrome } from '@web-canvas/timeline-node-chrome'; // transitional annotation

<NodeChromeShell selected accent="worldkb">
  <TimelineEventChrome title="The Crossing" blockTypeLabel="Event" … />
</NodeChromeShell>
```

---

## 6. Token mapping summary (implementer cheat sheet)

| Node kind | `NodeChromeShell.accent` | Primary layer / badge token(s) | Also show |
|-----------|--------------------------|--------------------------------|-----------|
| Brief-era | `worldkb` | `canvas-layer-brief-accent` | shared `canvas-node-*` |
| Event | `worldkb` | `canvas-layer-narrative-accent` | `canvas-timeline-accent` optional in section chrome, not required on card |
| KeyBlock | `worldkb` | (none required) | block-type pill only |
| Work Narrative | `worldkb` | `canvas-layer-narrative-accent` | — |
| Work Moment scene | `outline` | `canvas-layer-moment-accent` | — |
| Work Moment beat | `outline` | `canvas-layer-moment-accent` | — |

**Hard rule:** no hard-coded hex in extract or fixtures (F6). No new tokens in P0 (NG-9) — consume existing CSS vars only.

---

## 7. Fidelity / anti-drift rules (architect)

1. **Single body source** — App RF wrappers and Studio fixtures MUST import the same `@web-canvas/timeline-node-chrome` exports. Parallel JSX copies of badge rows are **reject** at QC.
2. **No RF in Studio** — fixture files must not import `@xyflow/react` (F1 / Studio AGENTS.md).
3. **No contracts in extract** — `timeline-node-chrome.tsx` must not import `@42ch/nexus-contracts` or `world-kb/types`.
4. **Layer accent migration is in-scope for the extract** — moving Event/Narrative badge colors from `worldkb-accent` → `layer-narrative-accent` is an intentional V1.123 P4 completion inside the shared extract, not a free visual redesign (NG-2 still forbids density/zoom UX rework).
5. **Layer breadcrumb is out of P0 extract scope** — lives in P2 (`@web-canvas/layer-breadcrumb` after move into `presentational/`). Optional header preview in P0 fixtures is **forbidden** if it pulls breadcrumb without the P2 extract path (avoids half-wired chrome).

---

## 8. Self-verify (three questions per kind)

| Kind | (1) Chrome source | (2) Studio alias | (3) Layer accent token |
|------|-------------------|------------------|------------------------|
| Brief-era | `presentational/timeline-node-chrome.tsx` + shell | `@web-canvas/timeline-node-chrome` + `node-chrome-shell` | `--color-canvas-layer-brief-accent` |
| Event | same | same | `--color-canvas-layer-narrative-accent` |
| KeyBlock | same | same | n/a (spine `worldkb` only) |
| Work Narrative | same | same | `--color-canvas-layer-narrative-accent` |
| Work Moment scene | same | same | `--color-canvas-layer-moment-accent` |
| Work Moment beat | same | same | `--color-canvas-layer-moment-accent` |

---

## 9. Relationship to other V1.124 docs

| Doc | Role |
|-----|------|
| `studio-fixture-acceptance-criteria.md` | Product F1–F9; this doc makes F4 technically concrete for Timeline |
| `surface-audit-checklist.md` | Global Timeline, breadcrumb, conflict-modals, alt-views |
| `tokens-gallery-audit.md` | Gallery registration of the layer/timeline tokens above |
| P0 plan | Execute implements this contract; does not re-open extract vs hand-mirror |

---

## 10. Exit

P0 Done requires: extract file exists, RF wrappers consume it, Studio fixtures import it, smoke tests green, visual matrix covers §4 variants, guardrail script green (no new alias root required).
