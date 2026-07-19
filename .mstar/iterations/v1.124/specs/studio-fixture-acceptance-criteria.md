# Studio Fixture Acceptance Criteria (V1.124 product contract)

**Status:** Locked (iteration-scoped) — product contract for P0/P2 fixtures; architect seat 2 added §8 testability notes  
**Document class:** Iteration package working spec (not `{SPECS_DIR}` Master; promote to knowledge/spec only at compound if still needed)  
**Audience:** Implementers, QC, QA, future contributors landing Studio fixtures  
**Authority:** Root `AGENTS.md` § UI Component Policy (Studio-first); `.mstar/specs/design-studio.md`; compass AC-V1124-1 / AC-V1124-4  
**Boundary companions:** `studio-timeline-fixture-boundaries.md` (F4 extract paths); `surface-audit-checklist.md` (P2 surfaces); `tokens-gallery-audit.md` (P1 gallery, not F1–F9)  
**Out of scope:** Author-facing product behavior; `@42ch/nexus-ui` promotion rules (see P3 + V1.106 workflow)

---

## 1. Why this exists

"Fixture exists" is not acceptance. V1.122/V1.123 proved that shipping visuals only in `apps/web` leaves contributors unable to review chrome without the daemon. This contract defines when a Studio fixture is **accepted** for product purposes.

---

## 2. Definition — fixture accepted in Studio

A fixture is **accepted** only when **all** of the following hold:

| # | Criterion | Observable check |
|---|-----------|------------------|
| F1 | **Daemon-free** | `pnpm --filter design-studio dev` shows the fixture with **no** daemon, Tauri, or `NexusClient`. No network calls required for the fixture frame. |
| F2 | **Both themes** | Light and dark (`.dark`) both render intentional chrome — not inverted garbage, not missing fills. Theme toggle is enough; no separate build. |
| F3 | **All product variants** | Every variant the App exposes for that chrome is visible in the fixture matrix (status, drag, badges, empty/populated, active breadcrumb segment, etc. — as applicable). "Happy path only" is not acceptance. |
| F4 | **Same extract as App** | Fixture composes the **same presentational module** App uses (or a thin props-driven shell of it via `@web-*`). Hand-redrawn parallel CSS that can drift is **reject**. |
| F5 | **No console errors** | Opening the fixture section produces no React/runtime console errors in dev. Smoke tests assert render without throw. |
| F6 | **Token-true** | Colors/borders/fills use `@nexus/design-tokens` CSS variables. No hard-coded hex that bypasses the Timeline/Layer token families under review. |
| F7 | **Voice & vocabulary** | Labels use product terms (Brief, Narrative, Moment, Timeline, Layer, World, Work, KeyBlock) and DESIGN.md Voice & Content tone — clear, non-marketing, no ACP jargon. Static English is required; slang is not. |
| F8 | **A11y baseline preserved** | If the App extract exposes focus rings, `aria-*`, or `aria-current`, the fixture does not strip them "because it's a gallery." |
| F9 | **Discoverable in Studio IA** | Fixture is wired into an existing Studio page section (typically Surfaces) with a human-readable section title a contributor can find without reading the plan. |

**Fail any row → fixture not accepted** for AC-V1124-1 / AC-V1124-4 purposes.

---

## 3. What "visual acceptance moves out of apps/web" means

| Before V1.124 (gap) | After acceptance |
|---------------------|------------------|
| Reviewer starts daemon + opens Control Room Canvas to judge Timeline node chrome | Reviewer opens Studio Surfaces, toggles theme, judges chrome offline |
| Token impact requires grepping `tokens.css` or running App | Token impact visible on Studio Tokens page (P1) **and** on fixture frames that consume those tokens (P0/P2) |
| "Looks fine in my App branch" is the only signal | Studio fixture matrix is the **first** visual gate; App remains integration proof, not the only gallery |

App integration and author runtime behavior remain out of this contract (compass NG-2, NG-10).

---

## 4. Minimum fixture matrix (Timeline node chrome — P0)

| Frame | Must show |
|-------|-----------|
| World — Brief-era | Layer accent `brief`; time-span badge on/off; status variants |
| World — Event (Narrative) | Layer accent `narrative`; source-count on/off; status / drag if App has them |
| World — KeyBlock Context cluster | Distinct from Event; context-cluster chrome |
| Work — Narrative | Layer accent `narrative` |
| Work — Moment | Layer accent `moment`; manuscript-anchor marker variant |

Global Timeline, Layer breadcrumb, conflict-modals: apply the same F1–F9 rows; variant lists live in P2 plan tasks.

---

## 5. Explicit non-acceptance (common false greens)

- Screenshot of `apps/web` attached to PR without Studio section
- Fixture that imports `@xyflow/react` or contracts types
- Fixture that only renders in light theme
- Fixture labels like "Node A" / "Demo" instead of product vocabulary
- Gallery entry for a token (P1) **without** any surface fixture consuming it when the token is surface-specific — token gallery alone does **not** satisfy AC-V1124-1 (node chrome); it satisfies AC-V1124-3

---

## 6. Relationship to other V1.124 docs

| Doc | Role |
|-----|------|
| This file | **Product** acceptance contract for fixtures |
| `studio-timeline-fixture-boundaries.md` | **Architect** extract/alias boundary per node kind |
| `tokens-gallery-audit.md` | Token CSS ↔ gallery delta + recurrence gate |
| `surface-audit-checklist.md` | Per-surface four-bucket inventory + fixture/defer |
| Root `AGENTS.md` UI Component Policy | Durable repo rule this contract operationalizes |

---

## 7. Exit

When P0/P2 claim Done, Completion Reports must cite F1–F9 (or point at tests + short visual note covering themes + variants). "Fixtures added" without F1–F9 is incomplete.

---

## 8. Architectural testability notes (architect seat 2)

Product criteria F1–F9 are **technically testable** as follows. Implementers and QC use these as evidence shapes — not new product requirements.

| # | Criterion | Automated / mechanical | Manual / spot |
|---|-----------|------------------------|---------------|
| F1 | Daemon-free | Studio `package.json` has no daemon dep; fixture imports grepped for `@42ch/nexus-contracts`, `NexusClient`, `@tauri-apps`, `apps/web/src/lib/nexus` — must be empty. `./tooling/check-ui-guardrails.sh` green. | `pnpm --filter design-studio dev` without daemon process |
| F2 | Both themes | Optional: render fixture under `.dark` class in Vitest + assert no throw. | Theme toggle; intentional fills (not inverted garbage) |
| F3 | All variants | Smoke test asserts required `data-testid` frames / product labels per boundary matrix (`studio-timeline-fixture-boundaries.md` §4; P2 checklist variant lists). | Visual matrix completeness |
| F4 | Same extract as App | **Hard:** fixture source imports `@web-canvas/timeline-node-chrome` (P0) / `@web-canvas/layer-breadcrumb` / `@web-canvas/conflict-modal-chrome` / `@web-global-timeline/*` (P2) — not a parallel local JSX clone of badge rows. App RF wrappers import the **same** module. Grep both call sites. | Spot-check chrome parity |
| F5 | No console errors | Vitest + testing-library render; fail on `console.error` spy if project convention supports it; at minimum render-without-throw. | DevTools console on Surfaces section |
| F6 | Token-true | Grep fixture/extract for `#` hex / `rgb(` outside allowed none — prefer class tokens `text-canvas-layer-*` / `var(--color-…)` only. | Swatch vs fixture side-by-side with Tokens page |
| F7 | Voice & vocabulary | Smoke test `getByText` / `toHaveTextContent` for Brief, Narrative, Moment, Timeline, Layer, KeyBlock, World, Work as applicable. | No slang / ACP jargon |
| F8 | A11y baseline | Assert `aria-current` on active breadcrumb; conflict chrome focusable controls present. | Keyboard tab once through interactive fixture chrome |
| F9 | Discoverable IA | Assert Surfaces page section headings / `data-testid` anchors exist (`#timeline`, `#work-timeline`, Global Timeline section, etc.). | Cold open Surfaces — find section without plan |

### F4 concrete extract map (do not re-litigate)

| Fixture family | Required presentational import |
|----------------|--------------------------------|
| World + Work Timeline node chrome | `@web-canvas/node-chrome-shell` **and** `@web-canvas/timeline-node-chrome` |
| Global Timeline list | `@web-global-timeline/global-timeline-list-chrome` |
| Layer breadcrumb | `@web-canvas/layer-breadcrumb` |
| Conflict-modal family | `@web-canvas/conflict-modal-chrome` (one shared shell) |

Hand-redrawn parallel CSS or duplicate badge JSX **outside** those modules is **reject** even if it "looks similar."

### Explicit non-goals for this contract

- Token gallery completeness is **AC-V1124-3 / P1**, not F1–F9 (see §5 false green already).
- Package promotion is **P3**, not fixture acceptance.
