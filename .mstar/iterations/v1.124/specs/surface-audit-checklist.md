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

## 7. Deferred to roadmap (P2 Task 4 — Execute 2026-07-19)

Surfaces classified **`web-only wrapper`** (or deferred `studio-local`) that did **not** receive a Studio fixture in V1.124 P2. Priority fixtures landed: Global Timeline list chrome, Layer breadcrumb, conflict-modal shared chrome.

| Surface | Path(s) | Bucket | Why deferred / keep-app-local | Revisit trigger |
|---------|---------|--------|-------------------------------|-----------------|
| Idea input | `canvas/idea-input.tsx` | `web-only wrapper` | Strategy run/steer/resume hooks (`useRunStrategy` etc.) — presentational layer *is* the product-state coupling | Fixture when a pure composer chrome exists without strategy hooks (extract cost ≤ one task) |
| Canvas nav commands | `canvas/canvas-nav-commands.tsx` | `web-only wrapper` | `useRegisterCommand` + `useNavigate` + route params — command registry behavior, not a visual gallery target | Only if a chrome-only palette UI appears independent of the registry |
| Outline alt-view | `outline-canvas/outline-alt-view.tsx` | `web-only wrapper` | Surface-specific list chrome bound to outline projection + sort state; RF-free strip not cheap enough for this plan | Fixture when a shared alt-view toggle/list strip extracts RF-free in ≤ one task |
| Strategy alt-view | `strategy-alt-view.tsx` | `web-only wrapper` | Coupled to strategy canvas view-mode / SM state | Same as Outline alt-view |
| World KB alt-view | `world-kb/world-kb-alt-view.tsx` | `web-only wrapper` | Coupled to entity/relationship table projection | Same as Outline alt-view |
| Timeline alt-view | `timeline-canvas/timeline-alt-view.tsx` | `web-only wrapper` | Orthogonal to P0 node chrome; RF/viewport-preference bound | Same as Outline alt-view |
| Timeline RF node wrappers | `timeline-node-types.tsx`, `work-timeline-node-types.tsx` | `web-only wrapper` | RF `Handle`/`NodeProps` *is* the layer — bodies covered by P0 `timeline-node-chrome` | — |
| Canvas shell / adapters / inspectors | `canvas-shell.tsx`, `*-adapter.tsx`, `**/inspectors/**` | Mostly `web-only wrapper` | Mixed RF + daemon + product routing | Individual extracts when a second gallery need appears |
| World KB entity nodes / graphs | `world-kb/entity-node.tsx`, `world-kb-canvas.tsx`, … | `web-only wrapper` | RF + contracts; partially mirrored via existing Studio WorldKB samples | Promote only if package criteria met (P3) |
| Domain conflict wrappers | `conflict-modal.tsx`, `outline-conflict-modal.tsx`, `world-kb-*-conflict-modal.tsx` | `web-only wrapper` | Product field mapping + i18n adapters over shared chrome (fixture = chrome only) | — |
| Full canvas/** silent-gap residuals | appendix §9 | mixed | Priority set 1–3 fixture-ed first | Next Studio-first gap iteration consumes appendix rows with `action=defer` |

**Product-relevant roadmap note:** Alt-view toggles remain the highest deferred visual candidates after P2; do not force fixtures until a cheap RF-free strip is confirmed. idea-input / nav-commands stay behavior-coupled by default.

---

## 8. Relationship

| Doc | Role |
|-----|------|
| This file | Per-surface inventory + extract locks for P2 |
| `studio-timeline-fixture-boundaries.md` | P0 Timeline nodes (priority sibling, not P2) |
| `studio-fixture-acceptance-criteria.md` | F1–F9 for any fixture landed here |
| P3 plan | Consumes classifications for promotion table |

---

## 9. Appendix — full file-walk (P2 Task 1 Execute)

Additive inventory under `apps/web/src/components/canvas/**` + `global-timeline/**`. **Does not reorder** §3 priority or reopen §4 extract locks. Coupling codes: **RF** = `@xyflow/react`; **D** = daemon/hooks/client; **C** = contracts; **R** = router; **I** = i18n only; **none** = presentational.

| File | Product sentence | Coupling | Bucket | Action |
|------|------------------|----------|--------|--------|
| `global-timeline/global-timeline-view.tsx` | Cross-World Timeline activity page | D+C+R+I | studio-local (via extract) | **Fixture P2** — composes list chrome |
| `global-timeline/presentational/global-timeline-list-chrome.tsx` | List Card + rows chrome | none | studio-local fixture | **Fixture P2** `@web-global-timeline/*` |
| `canvas/layer-breadcrumb.tsx` | Re-export of presentational breadcrumb | none | studio-local | Re-export |
| `canvas/presentational/layer-breadcrumb.tsx` | Layer path zoom-out chrome | none | studio-local fixture | **Fixture P2** `@web-canvas/layer-breadcrumb` |
| `canvas/conflict-modal-base.tsx` | i18n adapter → chrome | I | web-only wrapper | Adapter only |
| `canvas/presentational/conflict-modal-chrome.tsx` | Shared conflict dialog shell | none | studio-local fixture | **Fixture P2** `@web-canvas/conflict-modal-chrome` |
| `canvas/conflict-modal.tsx` | Strategy conflict field mapper | I+product | web-only wrapper | Defer (adapter) |
| `canvas/outline-conflict-modal.tsx` | Outline conflict field mapper | I+product | web-only wrapper | Defer (adapter) |
| `canvas/outline-canvas/conflict-modal.tsx` | Outline dialog host | product | web-only wrapper | Defer |
| `canvas/world-kb/world-kb-conflict-modal.tsx` | World KB entity conflict | I+product | web-only wrapper | Defer (adapter) |
| `canvas/world-kb/world-kb-relationship-conflict-modal.tsx` | World KB relationship conflict | I+product | web-only wrapper | Defer (adapter) |
| `canvas/idea-input.tsx` | Strategy idea composer | D+hooks | web-only wrapper | **Defer** §7 |
| `canvas/canvas-nav-commands.tsx` | Canvas command registration | R+commands | web-only wrapper | **Defer** §7 |
| `canvas/strategy-alt-view.tsx` | Strategy list/alt chrome | surface state | web-only wrapper | **Defer** §7 |
| `canvas/outline-canvas/outline-alt-view.tsx` | Outline chapters/timeline list | surface state | web-only wrapper | **Defer** §7 |
| `canvas/world-kb/world-kb-alt-view.tsx` | World KB table alt | surface state | web-only wrapper | **Defer** §7 |
| `canvas/timeline-canvas/timeline-alt-view.tsx` | Timeline list alt | surface state | web-only wrapper | **Defer** §7 |
| `canvas/presentational/node-chrome-shell.tsx` | Shared node card shell | none | studio-local fixture | P0 / existing |
| `canvas/presentational/timeline-node-chrome.tsx` | Timeline node bodies | none | studio-local fixture | P0 |
| `canvas/timeline-canvas/timeline-node-types.tsx` | RF World Timeline nodes | RF | web-only wrapper | P0 body extract covers chrome |
| `canvas/work-timeline-canvas/work-timeline-node-types.tsx` | RF Work Timeline nodes | RF | web-only wrapper | P0 body extract covers chrome |
| `canvas/canvas-shell.tsx` | Canvas viewport chrome | RF+product | web-only wrapper | Defer |
| `canvas/canvas-surface-adapter.ts` | Surface adapter types/helpers | product | web-only wrapper | Defer (no visual) |
| `canvas/use-canvas-surface.ts` / `use-auto-layout.ts` / `use-canvas-viewport.ts` / `use-semantic-zoom.ts` | Canvas hooks | RF/D | web-only wrapper | Defer (no visual) |
| `canvas/strategy-canvas.tsx` + `strategy-canvas/**` | Strategy surface host + SM + inspectors | RF+D+C | web-only wrapper | Defer; nodes partially in Studio samples |
| `canvas/strategy-nodes.tsx` | Strategy RF nodes | RF | web-only wrapper | Studio samples exist |
| `canvas/outline-canvas.tsx` + `outline-canvas/**` | Outline surface host + nodes + inspectors | RF+D+C | web-only wrapper | Defer; nodes partially in Studio samples |
| `canvas/timeline-canvas/**` (host/adapter/inspectors) | World Timeline surface | RF+D+C | web-only wrapper | Node chrome P0; host stays App |
| `canvas/work-timeline-canvas/**` | Work Timeline surface | RF+D+C | web-only wrapper | Node chrome P0; host stays App |
| `canvas/world-kb/**` (canvas/header/tables/inspectors/nodes) | World KB graph surface | RF+D+C | web-only wrapper | Partial Studio samples; no new P2 fixture |

**Execute confirmation:** Fixture scope matches §3 — Global Timeline → Layer breadcrumb → conflict-modal chrome landed; alt-views deferred per §4.4 / §7.
