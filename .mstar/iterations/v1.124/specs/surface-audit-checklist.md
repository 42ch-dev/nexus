# Surface Audit Checklist (V1.124 architect contract)

**Status:** Locked pre-classify (iteration-scoped) — architect seat 2; P2 Task 1 Execute may deepen file inventory but **must not reorder** fixture priority without compass edit  
**Document class:** Iteration package working spec  
**Audience:** P2 implementers, QC, P3 classification  
**Authority:** Compass S3 / AC-V1124-4; V1.106 four-bucket rubric; root `AGENTS.md` UI Component Policy  
**Fixture acceptance:** `studio-fixture-acceptance-criteria.md` (F1–F9)

---

## 1. Purpose

1. Publish the **four-bucket classification rubric** so a non-author reader does not need the plan file.
2. **Pre-classify** P2 priority candidates from source read (2026-07-19).
3. Lock **extract paths + aliases** so P2 Tasks 2–3 do not re-open architecture.
4. List defer / web-only rows with revisit triggers.

---

## 2. Four-bucket rubric (canonical)

| Bucket | Means | When to use |
|--------|-------|-------------|
| **`promoted primitive`** | Lives in `@42ch/nexus-ui`; pure presentational; reusable by Web + Studio (+ future) | ≥2 consumers, stable props, RF/daemon/route-free. **Rare for canvas chrome this iteration** — P3 records; P2 does not force package moves. |
| **`studio-local fixture`** | Studio shows it for visual review; code stays app-owned (often via `@web-*` extract) | Has visual surface worth reviewing; presentational layer extractable; not yet (or not ever) package-worthy. **Default for P2 fixtures.** |
| **`web-only wrapper`** | Stays in `apps/web`; **no** Studio mirror required | RF layer *is* the UI, or daemon/route/product-state coupling with no clean presentational shell. Must still appear in the audit with rationale. |
| **`future web product component`** | Not shipped / not ready; track for a later iteration | Genuinely new product UI. None expected in V1.124 priority set. |

### Studio-eligible gate (both must be true)

1. Visual surface worth reviewing (chrome, tokens, layout, state), **and**
2. Presentational layer can be extracted RF-free / daemon-free / contracts-free.

If either fails → `web-only wrapper` with written rationale.

### Decision tree

1. Pure presentational + reusable cross-app → candidate `promoted primitive` (flag for P3; still fixture as `studio-local` this plan unless already promoted).
2. Pure presentational, Studio-useful, App-specific → `studio-local fixture`.
3. RF/daemon-coupled but presentational shell extractable → `studio-local fixture` via `@web-*` extract.
4. RF/daemon-coupled and presentational layer **is** the coupled layer → `web-only wrapper`.
5. Not shipped / not ready → `future web product component`.

---

## 3. Locked product priority (do not reorder without compass edit)

1. **Global Timeline overview**
2. **Layer breadcrumb**
3. **Conflict-modal family**
4. **Alt-view toggles**
5. **Defer-by-default:** `idea-input.tsx`, `canvas-nav-commands.tsx`

---

## 4. Pre-classification — priority set

### 4.1 Global Timeline overview — priority 1

| Field | Value |
|-------|--------|
| Source | `apps/web/src/components/global-timeline/global-timeline-view.tsx` |
| What it renders | Cross-World Timeline activity list: Card chrome + World rows (title, layer/activity summary, last edited) linking to per-World Timeline |
| Imports (coupling) | **Daemon-heavy:** `useNarrativeWorlds`, `useNexusClient`, `useQueries` / query keys, `@42ch/nexus-contracts` (`WorldKbGraphResponse`), `react-router-dom` `Link`, i18n, shared `Card` / Empty/Error/Loading states |
| Presentational extractability | **Yes — list chrome only** |
| Classification | **`studio-local fixture`** via extract |
| Action | **Fixture now** (P2 Task 2) |

#### Extract shape (locked)

| Item | Decision |
|------|----------|
| Extract path | `apps/web/src/components/global-timeline/presentational/global-timeline-list-chrome.tsx` |
| Studio alias | **`@web-global-timeline/*`** → `apps/web/src/components/global-timeline/presentational/*` (**new alias root**) |
| Import example | `@web-global-timeline/global-timeline-list-chrome` |
| What leaves the extract | Props-driven: page title/description strings; `rows: { id, label, activityText, lastEditedText?, layer?: 'brief' \| 'narrative' }[]`; empty-state title/description; optional loading/error **frames as separate fixture states** (do not pull hooks) |
| What stays App-only | Hooks, graph fan-out, `deriveLayer`, router `Link` targets, contracts types, N=5 cap logic |
| App integration | Thin: hooks → map to row props → `<GlobalTimelineListChrome … />` (optional same-PR refactor; if deferred, fixture still uses extract and App adoption is residual — prefer same-PR for F4) |
| Fixture matrix | ≥3 populated rows; empty state; loading frame; error frame; light + dark; product vocabulary (World, Timeline, Brief, Narrative) |
| Guardrail blast | **Yes** — new alias: `vite.config.ts`, `vitest.config.ts`, `tsconfig.json`, `tailwind.config.ts` content path, `apps/design-studio/AGENTS.md` table, `tooling/check-ui-guardrails.sh` if it enumerates allowed `@web-*` prefixes |

**Rejected:** Importing whole `global-timeline-view.tsx` into Studio (daemon + contracts). **Rejected:** Fixture-only mock with no App extract (fails F4).

---

### 4.2 Layer breadcrumb — priority 2

| Field | Value |
|-------|--------|
| Source | `apps/web/src/components/canvas/layer-breadcrumb.tsx` |
| What it renders | Clickable layer path (`Brief › Narrative` / `Narrative › Moment`); parent button zoom-out; active `aria-current="page"` |
| Imports | `react-i18next` only — **no RF, no daemon, no contracts** |
| Presentational extractability | **Yes — already almost pure** |
| Classification | **`studio-local fixture`** (promote-when: third surface reuses chain — header comment) |
| Action | **Fixture now** (P2 Task 3) |

#### Extract shape (locked)

| Item | Decision |
|------|----------|
| Path | Move (or re-export) into `apps/web/src/components/canvas/presentational/layer-breadcrumb.tsx` |
| Studio alias | **Existing** `@web-canvas/layer-breadcrumb` (no new alias root) |
| Props | Keep generic `LayerBreadcrumbProps<L>`; Studio passes **resolved labels** via `defaultValue` / or add optional `label?: string` override — do not require live i18n catalogs for F7 if static English defaults are product vocabulary |
| A11y | Preserve `aria-current="page"`, focus-visible ring on parent button, nav `aria-label` (F8) |
| Fixture matrix | World: Brief only / Brief › Narrative / Narrative only; Work: Narrative only / Narrative › Moment / Moment only; active segment styles |
| Guardrail blast | **No new alias root**; update AGENTS.md `@web-canvas/*` description |

---

### 4.3 Conflict-modal family — priority 3

| Field | Value |
|-------|--------|
| Sources | `conflict-modal-base.tsx` (shared shell); wrappers: `conflict-modal.tsx` (Strategy), `outline-conflict-modal.tsx` + `outline-canvas/conflict-modal.tsx`, `world-kb/…` conflict surfaces |
| What it renders | Overlay + dialog chrome + server/local field diffs + Use current / Reapply / Review / Keep editing actions; focus trap |
| Imports (base) | React hooks, `lucide-react`, `useTranslation` — **no RF, no daemon** |
| Presentational extractability | **Yes — shared shell already extracted as `ConflictModalBase`** |
| Classification | Shell → **`studio-local fixture`**; domain wrappers → **`web-only wrapper`** (product field mapping + i18n) |
| Action | **One shared chrome fixture** (not three parallel modal redraws) |

#### Extract shape (locked) — decision #3

| Item | Decision |
|------|----------|
| Approach | **One shared chrome extract** — relocate/adapt `ConflictModalBase` into `apps/web/src/components/canvas/presentational/conflict-modal-chrome.tsx` |
| Studio alias | `@web-canvas/conflict-modal-chrome` |
| Props | All user-visible strings as props (title, section titles, button labels, field rows). Defaults may use English fallbacks; App wrappers pass `t()` results |
| Fixture matrix | light + dark; open state; with overlapping server/local fields (reapply disabled path if base encodes it); focus-visible on primary actions |
| Per-modal fixtures | **No** separate Strategy/Outline/WorldKB visual shells — wrappers stay product adapters |
| Guardrail blast | **No new alias root** (file under existing `@web-canvas/*`) |

---

### 4.4 Alt-view toggles — priority 4

| Field | Value |
|-------|--------|
| Sources | `outline-canvas/outline-alt-view.tsx`, `strategy-alt-view.tsx`, `world-kb/world-kb-alt-view.tsx`, `timeline-canvas/timeline-alt-view.tsx` |
| Pre-read | Alt-views are surface-specific view-mode chrome; typically coupled to canvas surface state / RF viewport preferences |
| Classification (default) | **`web-only wrapper`** **or** deferred `studio-local` if a thin presentational toggle strip extracts cheaply |
| Action | **Fixture only if** P2 Task 1 Execute confirms a **cheap** RF-free toggle strip (≤ one task). Else **defer** with rationale — do not force |

| Surface | Expected call | Notes |
|---------|---------------|-------|
| Outline alt-view | Defer / web-only unless cheap strip | Prefer keep-app-local if RF viewport-bound |
| Strategy alt-view | Same | |
| World KB alt-view | Same | |
| Timeline alt-view | Same | Timeline **node chrome** already covered by P0 — alt-view is orthogonal |

---

## 5. Pre-classification — defer-by-default / other canvas

| Surface | Path | Coupling | Classification | Action | Revisit trigger |
|---------|------|----------|----------------|--------|-----------------|
| Idea input | `canvas/idea-input.tsx` | Strategy run/steer/resume hooks (`useRunStrategy` etc.) | **`web-only wrapper`** | **Defer** | Fixture when a pure composer chrome exists without strategy hooks |
| Canvas nav commands | `canvas/canvas-nav-commands.tsx` | `useRegisterCommand`, `useNavigate`, route params | **`web-only wrapper`** | **Defer** | Not a visual gallery target — command registry behavior |
| Timeline RF node wrappers | `timeline-node-types.tsx` / `work-timeline-node-types.tsx` | RF | **`web-only wrapper`** | Covered by P0 body extract + Studio fixture | — |
| Timeline body extract | `presentational/timeline-node-chrome.tsx` (P0) | None | **`studio-local fixture`** | P0 | Promote only if P3 criteria met |
| NodeChromeShell | `presentational/node-chrome-shell.tsx` | None | **`studio-local fixture`** (second consumer = Studio) | Already fixture-ed | Promote if ≥2 App surfaces + package criteria (P3) |
| Canvas shell / adapters / inspectors | various under `canvas/**` | Mixed RF/daemon | Mostly **`web-only wrapper`** | No fixture this iteration | Individual extracts when a second gallery need appears |
| World KB entity nodes / graphs | `world-kb/**` | RF + contracts | **`web-only wrapper`** | Already partially mirrored via existing Studio WorldKB samples | — |

P2 Task 1 Execute must still **enumerate every file** under `apps/web/src/components/canvas/**` + `global-timeline/**` into an appendix table (name | one-line product sentence | coupling | bucket | action). Pre-classify above is authoritative for the **priority set**; the full file walk fills silent-gap prevention for AC-V1124-4.

---

## 6. Guardrail blast radius summary (decision #4)

| Work item | New alias root? | Files to touch when implementing |
|-----------|-----------------|----------------------------------|
| P0 Timeline body chrome | **No** | presentational file + AGENTS.md description |
| P2 Layer breadcrumb | **No** (move into `@web-canvas/*`) | move/re-export + AGENTS.md |
| P2 Conflict modal chrome | **No** | move/adapt base → presentational + AGENTS.md |
| P2 Global Timeline list | **Yes — `@web-global-timeline`** | vite, vitest, tsconfig, tailwind content, AGENTS.md, guardrails allowlist if any |

---

## 7. Deferred-to-roadmap section (P2 Task 4 fills triggers)

| Item | Why deferred | Revisit trigger |
|------|--------------|-----------------|
| idea-input | Daemon/strategy-hook coupled | Pure composer chrome extract ≤ one task |
| canvas-nav-commands | Command registry / router | Not a Surfaces gallery target unless chrome-only palette UI appears |
| Alt-view toggles (if not fixture-ed) | RF/surface-state coupling | Cheap presentational strip confirmed |
| Full canvas/** inventory residuals | Priority set first | Next Studio-first gap iteration |

---

## 8. Relationship

| Doc | Role |
|-----|------|
| This file | Per-surface inventory + extract locks for P2 |
| `studio-timeline-fixture-boundaries.md` | P0 Timeline nodes (priority sibling, not P2) |
| `studio-fixture-acceptance-criteria.md` | F1–F9 for any fixture landed here |
| P3 plan | Consumes classifications for promotion table |
