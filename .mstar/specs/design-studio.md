# Design Studio — Specification

**Status**: Normative target contract (v1.187) — implementation and visual acceptance are separate  
**Document class**: Feature line  
**Created**: 2026-07-08 (`@product-manager`)  
**Revised**: 2026-09-09 (product, architecture and writing Review & Edit)  
**Scope**: `apps/design-studio` — read-only gallery and visual proving ground for the Nexus DESIGN SSOT, brand VI, shared presentational primitives, and representative surface fixtures  
**Coordinates with**:

- Repo-root [`DESIGN.md`](../../DESIGN.md) + [`DESIGN.dark.md`](../../DESIGN.dark.md) — sole token SSOT
- [`web-ui.md`](web-ui.md) §30 — Studio is contributor tooling, not a Control Room feature
- `@42ch/nexus-ui` — brand layer plus approved presentational primitives (public exports only)
- Root [`AGENTS.md`](../../AGENTS.md) UI Component Policy (Studio-first)
- [`apps/design-studio/AGENTS.md`](../../apps/design-studio/AGENTS.md) — import guardrails

Iteration history that used to live in this file (V1.98–V1.107 task tables, superseded process notes) is **not** current promise. See §12.

---

## 1. Purpose

Contributors and frontend implementers need one visual workspace to validate Nexus tokens, brand VI, component states, voice samples, and representative product chrome without running the daemon, Tauri, or live product flows.

Design Studio is a standalone Vite + React SPA (`apps/design-studio`). It is a **read-only showcase**: token edits happen in repo-root `DESIGN.md` / `DESIGN.dark.md` on disk; refresh the dev server to see updates. App chrome displays **Read-only · edit `DESIGN.md`**.

**Product outcome (v1.187):** Studio is a complete proving ground for a **precision creative tool** visual language. A contributor can find, inspect (both themes and meaningful states), and classify source without scrolling a monolithic gallery blindly, and without a second theme authority.

Studio is **not** the author-facing product. Shared token and primitive changes may restyle `apps/web` naturally. This spec does **not** accept web or desktop page redesign.

---

## 2. Audiences and contributor jobs

| Audience | Job | Studio must |
| --- | --- | --- |
| Contributors | Recalibrate color, type, space, and component tokens with confidence | Filterable indexes, live token specimens, pair-view theme comparison, read-only DESIGN workflow |
| Frontend developers | Pick the correct variant and state when building screens | Component matrices with rest/hover/active/focus/disabled and loading/empty/error where supported; surface slices as composition reference |
| Brand / VI reviewers | Confirm name, timeline-mark geometry, clear space, and theme.css alignment | Brand section with all logo variants + `NexusMark`; geometry preserved; usage rules after palette recalibration |
| Authors (local Web UI users) | — | **Not in scope** — Studio is not bundled in `nexus42` |

### 2.1 Jobs that count as done

A contributor, daemon-free, can:

1. **Find** any inventory item in §7 via the section index (type-to-filter, keyboard, stable hash).
2. **Inspect** that item in **light and dark together** (pair view) or by session theme toggle, including meaningful states in §8.
3. **Read provenance** (promoted / extract / transitional / studio-local).
4. **Edit SSOT only:** change the DESIGN pair, refresh, and see the gallery update.
5. **Keep using** every frozen route and fixture family in §6–§7.

---

## 3. Placement and boundaries (normative)

### 3.1 Placement: `apps/design-studio`

- pnpm workspace member under `apps/*`
- **Consumer**, not producer — no daemon API, no `NexusClient`, no `@42ch/nexus-contracts` wire types
- Runs via `pnpm --filter design-studio dev` on port **5174** without daemon or Tauri

### 3.2 What Studio may import

Four source categories. `@42ch/nexus-ui` and `@web-*` aliases are not interchangeable.

| Category | Pattern | Meaning |
| --- | --- | --- |
| Promoted primitive | `@42ch/nexus-ui` | Publishable package export after Studio visual acceptance |
| App presentational extract | `@web-layout/*`, `@web-canvas/*`, `@web-setup/*`, `@web-settings/*`, `@web-global-timeline/*`, `@web-shell/*`, … | Monorepo-only alias → `apps/web` props-driven chrome |
| Transitional primitive | `@web-ui/*` | Unpromoted `apps/web/src/components/ui/*` mirror |
| Studio-local | `@/fixtures/*`, `@/components/*`, `@/pages/*` | Gallery chrome and fixture composition |

| Source | Allowed | Notes |
| --- | --- | --- |
| Root `DESIGN.md` / `DESIGN.dark.md` | Yes | SSOT; consumed via `@nexus/design-tokens` |
| `@nexus/design-tokens` | Yes | Shared `tokens.css` + Tailwind preset with `apps/web` |
| `@42ch/nexus-ui` | Yes | Brand VI + promoted primitives, public exports only |
| `@web-ui/*` | Yes (transitional) | Unpromoted primitives only; each import annotated `transitional` |
| `@web-setup/*`, `@web-layout/*`, `@web-settings/*`, `@web-canvas/*`, `@web-global-timeline/*`, `@web-shell/*` | Yes | Props-driven extracts; no daemon, routing, or IPC |
| `@web-lib/utils` | Yes | `cn()` only if still required; prefer `@42ch/nexus-ui` `cn` |
| `apps/web` screens, routing, `NexusClient`, daemon hooks, providers, Tauri helpers | **No** | Studio must not become a second product shell |
| Direct `apps/web/src/components/layout/**` or `settings/**` | **No** | Extracts only via `@web-*` |
| Live token override, localStorage theme hacks, YAML write-back | **No** | Read-only invariant |
| New component families or mass `@web-*` → package promotion | **No** | Out of v1.187 scope unless a later plan names the promotion |

Guardrail: `tooling/check-ui-guardrails.sh`.

### 3.3 Toolchain alignment with `apps/web`

| Concern | `apps/web` | `apps/design-studio` |
| --- | --- | --- |
| Bundler | Vite 8 | Vite 8 (same major) |
| React | 19.2 | 19.2 |
| TypeScript | 7.0 strict, `@/*` alias | 7.0 strict, `@/*` + `@web-ui/*` aliases |
| Tailwind | v4.3, `class` darkMode | v4.3 — **shared preset** from `@nexus/design-tokens` |
| CSS tokens | `@nexus/design-tokens/tokens.css` | Same import — no second transcription |
| Test runner | Vitest 5 | Vitest 5 |
| Contracts | `@42ch/nexus-contracts` required | **Not used** — dev surface only |
| Dev server port | 5173 | 5174 (document in README; avoid clash) |

### 3.4 Relationship to `apps/web` and `@42ch/nexus-ui`

- `apps/web` remains the author-facing local product UI.
- Both read the same root DESIGN pair. Studio must not invent tokens.
- v1.187 may restyle existing public primitives. It must **not** break public callable contracts or require `apps/web` callsite edits.
- An API break that would force web edits is a STOP, not a shim.
- Forbidden in the package: app shells, page components, daemon-aware or route-aware controls.

### 3.5 Token projection contract

The DESIGN pair is the authority; checked-in CSS and brand constants are **derived outputs**, not independently editable sources. The original checker only searched strings in handwritten CSS/preset files and could not make a DESIGN edit visible. The target replaces that gap with one shared build-time compiler in `tooling/design-tokens/scripts/project-tokens.mjs`, used by both the generation command and Studio Vite integration. No browser YAML parser, runtime token override, separate Studio palette, or second stylesheet transcription.

**Compiler interface (internal tooling, not a package UI API):**

```ts
type DesignPair = { light: Record<string, unknown>; dark: Record<string, unknown> };
type TokenProjection = { css: string; brandCss: string; brandTokens: string };
loadDesignPair(repoRoot: string): Promise<DesignPair>;
projectDesign(pair: DesignPair): TokenProjection;
```

Use the `yaml` package as an explicit tooling dependency (`^2.6.1`, matching the existing workspace range; current lock resolves 2.9.0). Reject duplicate YAML keys, differing leaf-path sets, unresolved `{path}` references, reference cycles, and non-scalar substitutions into scalar CSS. Resolve references recursively, including references embedded in color-mix, border strings, and typography references. `typography.font-display` and reading metrics are intentional scalar extensions. Existing compound SOUL values `{typography.X} @ {colors.Y}` project their color member to their existing `--color-*` property, not the object or literal `@` text.

**Projection rules (preserve all existing CSS variable and Tailwind utility names):**

| Source | Output rule |
| --- | --- |
| `colors.<name>` | `--color-<name>`; brand families also produce `--nexus-<name>` in package theme.css |
| `typography` structured roles | `--text-<role>` from fontSize; `--text-<role>--line-height`, `--text-<role>--letter-spacing`, `--text-<role>--font-weight`, `--text-<role>--font-family` for the other fields; Tailwind keeps the existing role keys |
| UI / mono / display families | `--font-sans` from heading-16.fontFamily, `--font-mono` from copy-13-mono.fontFamily, `--font-display` from font-display |
| `typography.reading-prose-*` | Identically named `--reading-prose-*` |
| `spacing.space-*`, `rounded.*`, `elevation.*`, `motion.*` | `--space-*`, `--radius-*`, `--shadow-elevation-*` / existing `--shadow-card/popover/modal`, and `--duration-*` / `--ease-*` |
| `components.canvas` flat members | Existing `--color-canvas-*`, including grid-gap/dot-size; node-width members use `--canvas-node-width-<role>` |
| `components.states.<error/success/warning/info>` | `--color-<role>-surface` from backgroundColor and `--color-<role>-surface-border` from borderColor |
| `components.states.disabled.opacity`, `components.listbox.maxHeight` | Preserve `--color-states-disabled-opacity`, `--color-listbox-max-height` |
| `components.data-table.row-protected`, `components.launch-daemon.main-banner.backgroundColor` | `--color-data-table-row-protected`, `--color-main-banner-background` |
| `components.finding-status-pill.<state>` | `--color-finding-status-<state>-<bg/text/border>`; underscores become hyphens; use backgroundColor/textColor/borderColor |
| `components.memory-task-kind-<kind>` | `--color-memory-task-kind-<kind>-<bg/text/border>` for brainstorm/outline/chapter/research/unknown |
| `components.reading-maturation-badge` | world-kb-density-count → `--color-reading-maturation-kb-density-*`; open-findings-count → `--color-reading-maturation-open-findings-*` |
| `components.badge-status-pill.soft.<variant>` | Existing `--color-nexus-ui-badge-soft-<variant>-<bg/text/border>` |
| `components.soul-viz-*`, soul-narrative-prose, soul-growth-curve-stroke | Existing flattened `--color-<component>-<member>`; scalar component has no member suffix; compound label/prose emits the color after `@` |
| `components.reading-annotation-*`, reading-selection-toolbar | Existing flattened `--color-<component>-<member>`; backgroundColor → background, textColor → text, borderColor → border; keep shadow's existing name |
| `components.footer-profile` | Existing `--color-footer-profile-<member>` |
| `components.setup-wizard-step` | Existing `--color-setup-wizard-<member>` (member already contains step where required); step-label-typography emits referenced fontSize only |
| `components.setup-wizard-surface` | Existing `--color-setup-wizard-surface-<member>` for projected members; do not create CSS from asset filenames or behavior recipes |
| `components.dialog`, sheet, sidebar-nav | Preserve `--color-dialog-max-width`, `--dialog-width`, `--dialog-max-height`, `--sheet-width`, `--sidebar-nav-width`, `--sidebar-nav-item-height` |
| `components.reading-chrome-*` | Flatten component + nested member path to existing bare `--reading-chrome-*`; camelCase → kebab-case; game-bible.category-badge.textColor → existing `...-color` |

The existing `tokens.css` declaration inventory is the cutover boundary: every existing property must retain a source mapping, including historical structural names under `--color-*`. Do not infer removal from a naming convention. Extra missing scalar color/brand/type projections are additive and get Tokens specimens. Recipes not directly projected remain compositional contracts, not invented CSS values. A declarative source-path mapping in tooling carries **paths only**, never a copied palette.

`generate-tokens.mjs` writes deterministic `src/tokens.css`, `packages/nexus-ui/theme.css`, and `packages/nexus-ui/src/generated-brand.ts`; `src/tokens.ts` imports/re-exports the generated `brandColors` while preserving every other export and historical asset palette. `brandColors` is a light/default numeric snapshot. `check-tokens.mjs` compares generated output with these checked-in artifacts and checks projection coverage/parity; it must not retain serif-source or exact-source-string tests as a proxy for behavior.

Studio Vite's local plugin uses the **same compiler** to transform the shared tokens.css and package theme.css modules in memory in dev. On either DESIGN file change, invalidate both CSS modules and perform a full reload so computed-value labels also refresh. A manual reload must re-read the pair even if no watch event arrived. Do not write source files during HMR, race two dev servers over generated files, or silently keep last-good tokens on malformed YAML; show the Vite error overlay. Production prebuild runs the same generation command, builds the public primitive package, then builds Studio. No normal app build consumes raw YAML.

### 3.6 API and asset boundaries

All existing public UI exports, props, variants, controlled/uncontrolled behavior, callbacks, and asset filenames remain source-compatible. No new compatibility alias, theme prop on public primitives, promotion, or app edit is required. `CardTitle.voice="content"` stays valid but now selects a larger sans title. `TabsTriggerProps` has no disabled prop; unsupported states are labeled, not fabricated. Keyboard and tab/panel association repairs may be internal.

Frozen logo pigments are installed-identity/geometry references, not live token swatches. Keep all five logo variants, square plates, NexusMark and four historical NexusLogoVariant specimens. The Brand page explicitly separates frozen assets from live CSS/currentColor examples. Do not recolor/regenerate SVG/PNG/desktop assets to force palette equality.

---

## 4. Visual language

Root DESIGN v0.5 locks the production-level contract:

- Silver-neutral light planes, graphite dark planes, cobalt interaction; no warm-paper global default, cyan glow, ornamental gradient, or marketing choreography.
- Offline system sans with named Latin/CJK system fallbacks for interface and content display. Existing display tokens and CardTitle voice prop remain; content voice means title hierarchy, not serif.
- 4px spacing base; existing 24/32/40/48px control sizes; Studio chrome uses small controls and larger narrow/coarse-pointer hit areas. Radius: control 4px, card/popover 8px, fullscreen 12px.
- Primary uses blue-700/800/900 rest/hover/active: white label in light, deep-blue label in dark. Semantic success/running, warning, error, queued, preset and info meanings are preserved.
- 2px background gap + 2px focus band; 120ms state, 160ms popover/enter, 200ms modal, 140ms exit; reduced motion is immediate.
- Keep Nexus name, timeline-mark geometry, asset filenames and desktop icons. Frozen asset pigments are labeled as references, not a second active theme.
- Identical frontmatter token paths in both themes, including dark blue-1100 and Button tiny. Exact values and arithmetic contrast evidence live in the DESIGN pair; runtime acceptance remains separate.

---

## 5. Dev UX (normative)

### 5.1 Commands

| Action | Command |
| --- | --- |
| Start | `pnpm --filter design-studio dev` |
| Build | `pnpm --filter design-studio build` |
| Test | `pnpm --filter design-studio test` |

### 5.2 Read-only tuning workflow

1. Open Studio (`pnpm --filter design-studio dev`).
2. Baseline: session theme from `prefers-color-scheme`; optional pair view.
3. Find the target via the section index (not by scrolling the whole page).
4. Edit `DESIGN.md` and/or `DESIGN.dark.md` on disk.
5. Refresh (HMR or reload) until the gallery reflects the SSOT.
6. Validate Brand, Components, Voice, and Surfaces in both themes.
7. Shared-consumer check: `pnpm --filter web typecheck` and `pnpm --filter web build` **without** editing web source.

**Success signal:** steps 1–7 do not require reading `index.css` or Tailwind config to understand token impact.

### 5.3 Theme toggle and pair view

- Header light/dark control retains `Theme = 'light' | 'dark' | 'system'`, OS default, and the existing `nexus-studio-theme` preference. Only the user preference may persist; pair mode and tokens do not.
- Session theme applies `.dark` and `color-scheme` on the top document's `html`.
- Pair view is page-level, default off, available on the five galleries and all existing nested Surfaces pages. It replaces the single gallery render with **two same-origin iframe documents loading the same Studio entrypoint**, not two React subtrees in the parent.
- The iframe URL is the current allowlisted pathname + `?studio-embed=light` or `?studio-embed=dark` + current hash. This is a Studio display parameter, not a new route. `window.self !== window.top` plus an exact light/dark value enables embedded mode; arbitrary values and top-level uses do not force a theme.
- Embedded boot sets its own document theme **before React renders**, then mounts the existing route/gallery with the forced ThemeProvider value. It does not read/write localStorage, listen to system/storage changes, show global shell/discovery/pair controls, or create nested comparison frames. Normal root behavior remains unchanged.
- Each iframe owns its React root, DOM IDs, SVG definitions, native focus, body portals, and computed-style reads. Dialog/Toaster portals remain in that frame's body. This avoids duplicate label IDs, duplicate SVG IDs, parent-global theme reads, dark-variant ancestor leakage, and modal focus stealing from the other sample. Do not clone fixture markup or monkey-patch createPortal.
- Both frames start with the same existing fixture data/default state and hash; interactions are independent. “Compare” means the same specimen/section in two themes, not synchronized user events or duplicated product behavior. Each frame retains all its section's actual states. A visible Reset comparison action remounts both at the current target.
- Parent navigation, filter and hash selection remain outside the frames; navigation updates both frame URLs. Frame fragment links stay within that frame. The parent hash remains the canonical shareable target. Pair off restores the ordinary gallery at that hash and removes both frames. Session theme changes affect outer chrome only while the pair remains fixed.
- At min-width 1024px use two equal minmax(0,1fr) columns with 16px gap; below 1024px stack light then dark. Each frame is width 100%, height 640px, with its own vertical scroll and visible accessible title (`Light — <gallery>` / `Dark — <gallery>`). No transform scaling, clipping, canvas screenshots, or hidden interactive duplicate in the parent. Surfaces' navigation rail is omitted inside frames; fixture headings/content remain.
- Parent and iframe focus indicators stay visible; Tab enters/exits frames naturally, Dialog Escape closes only its own dialog, and labels/describedby/tab-panel relationships resolve within one document. Reduced-motion applies in each document. Broken frame load is an explicit error with retry/open-current-gallery recovery, never a blank “successful” comparison.
- Embedded App posts `{type: 'nexus-studio-embed-ready', theme, path}` to its same-origin parent after route commit. Parent accepts only its own iframe contentWindow, exact origin and expected theme/path. Reset readiness on pathname/theme/remount changes, not fragment-only navigation within an already-ready document. Native error or no ready message after 10 seconds shows Retry/Open-current-gallery recovery; load alone is neither app-ready proof nor an error. Clear listeners/timers on remount/unmount. This handshake carries readiness only, never tokens, user events or synchronized product state.

### 5.4 Discovery

One Studio-local catalog holds route/anchor/label/keyword/source metadata only, never token values or fixture behavior. `GalleryEntry = { path: string; id: string; label: string; keywords: readonly string[]; importPaths: readonly string[] }`. Freeze all existing explicit IDs; add deterministic IDs to headings without them.

Tokens, Brand, Components and Voice filter their own entries. Surfaces filters all nested route headings plus every Shell/Canvas family in §7.5, including entries on sibling routes. Filtering is trimmed, case-insensitive substring matching across label, id and keywords; it does not hide gallery content. Empty query lists everything in that gallery group.

Render a labeled search input and ordinary links in an index nav, not a new command palette or a misleading listbox. ArrowDown from input focuses the first result, ArrowUp the last; within results arrows move without wrapping. Enter activates the link (or first result from input); Escape clears query and focuses the input. Tab remains natural. A polite result-count/no-results message and Clear action support recovery.

Link activation updates the existing route + stable hash, waits for that route to mount, then focuses the heading (`tabIndex=-1`) and scrolls it below sticky chrome. In pair mode it targets both frame hashes and announces the selected comparison instead of searching the parent for nonexistent fixture IDs. Direct deep links and Back/Forward use the same route/hash path. Home is a job overview with the same five gallery links, not a marketing hero.

---

## 6. Information architecture (frozen)

Changing a slug or a top-level nav label is a **user decision**, not an implementer cleanup.

| Nav label | Route | Role |
| --- | --- | --- |
| (Home wordmark) | `/` | Job overview |
| Tokens | `/tokens` | Scalar scales and canvas token families |
| Brand | `/brand` | Logos, mark, clear space, theme.css |
| Components | `/components` | Primitive and composite matrices |
| Voice | `/voice` | Voice & Content specimens |
| Surfaces | `/surfaces` | Product chrome fixtures |

Surfaces nested routes (sidebar labels frozen):

| Sidebar label | Route |
| --- | --- |
| Overview | `/surfaces` |
| Setup | `/surfaces/setup` |
| Shell | `/surfaces/shell` |
| AgentPicker | `/surfaces/agent-picker` |
| Canvas | `/surfaces/canvas` |
| Daemon | `/surfaces/daemon` |
| Launch | `/surfaces/launch` |
| Selection Submenu | `/surfaces/selection-submenu` |

Do not add `/surfaces/banner`. That sketch was removed; do not restore it without a new plan.

At 1280×800 and 390×844 the same five top-level labels remain reachable (wrap, compact, or overflow menu listing those labels). No document-wide horizontal overflow.

---

## 7. Gallery inventory (must not drop)

This is the baseline at `origin/main` `9db88c10`. Architect may merge or rename **token values**, not silently delete a family or fixture family from Studio. If a DESIGN family is retired, the plan must say so and the gallery must not keep it as the live system.

### 7.1 Tokens (`/tokens`)

| Family | Baseline contents |
| --- | --- |
| Brand | All brand core/extended steps and alphas, plus blue-1100; names retained, values recalibrated |
| Background | `background-100/200/300` |
| Gray solid / alpha | `gray-100`–`gray-1000`, `gray-alpha-100`–`600` |
| Semantic hues | `blue`, `red`, `amber`, `green`, `teal`, `purple`, `pink` (roles preserved) |
| Component surfaces | `data-table-row-protected`, `main-banner-background` |
| Other color families | scrim and all error/success/warning/info surface fills/borders retained and displayed |
| Typography | all display, heading, label, copy (including copy-12), button, mono, and reading-measure roles; bilingual samples |
| Spacing | `space-1` … `space-24` |
| Radius | control, card, popover, fullscreen, pill |
| Elevation | `elevation-0`–`4` plus legacy aliases `shadow-card/popover/modal` |
| Motion | durations, easings, recipes; reduced-motion honesty |
| States | disabled wash |
| Canvas | ambient, node chrome, edges/ports, surface accents, timeline accent, layer accents, outline pins, soul-viz axes, node widths |

Also expose all existing finding-status, memory-task-kind, reading-maturation, badge-soft, reading annotation/chrome, SOUL, footer-profile, setup-wizard and structural sizing projections, grouped by usage. Keep token labels bound to actual computed values, not hardcoded screenshots or literal palette echoes.

Every **changed** family in the locked DESIGN must have a visible specimen in both themes.

### 7.2 Brand (`/brand`)

All `@42ch/nexus-ui` logo variants (primary, whiteBg, white, mono, text), square plate lockups, `NexusMark`, clear-space guidance, `theme.css` swatches, four historical `NexusLogoVariant` specimens, and VI acceptance fixtures. Preserve all asset files and timeline geometry; no desktop icon replacement. Separate **live theme tokens** from **frozen installed assets/historical pigment references** so preserved files are never presented as another active palette.

### 7.3 Components (`/components`)

Promoted (`@42ch/nexus-ui`): Badge, Button, Card, Input, Label, Textarea, Select, Tabs, Toast, TransportErrorBlock, RunFormFields, EntityPickerField, ProposalSections, RunStatusBadge, RunsTable, ComputeResultNodeChrome, ComputeInspectorSections.

Transitional keep-web (`@web-ui/*`): Dialog, States, Table.

Studio sections that must remain: Badge, Button, Card, Dialog, Domain Badges, Input, Label, Select, States, Table, Tabs, Textarea, Form Field, Toast, Transport Error, Run Studio, Compute Timeline, VI acceptance.

No new primitive families in v1.187.

### 7.4 Voice (`/voice`)

Labeled specimens for Title Case, Sentence case, Verb-only, Action + object, error, empty, loading, success — sourced from DESIGN Voice & Content after the overhaul (copy may be recalibrated; patterns stay).

### 7.5 Surfaces fixture families

| Route | Fixture families (do not drop) |
| --- | --- |
| `/surfaces/setup` | Setup wizard chrome |
| `/surfaces/shell` | Chronos titlebar; App shell chrome; Creator Hub dual-pane IA; Creator/Orchestrator functional-region IA; Creator shell; Settings shell chrome; Footer profiles; Header health indicator |
| `/surfaces/agent-picker` | AgentPicker visual states (loading, grid, mixed, empty, error, selected, VI targets) |
| `/surfaces/canvas` | Outline / Strategy / World KB mirrored chrome; Mental Surfacing; NLE Timeline; World Timeline; Work Timeline; Global Timeline; Layer Breadcrumb; Conflict Modals |
| `/surfaces/daemon` | Daemon status strip |
| `/surfaces/launch` | Launch splash (waiting, error, recovery) |
| `/surfaces/selection-submenu` | Six documented variants; delete variant remains deferred unless a later plan names it |

Each Surfaces section shows source badges and the compact legend (extract / promoted / transitional / studio-local). `@42ch/nexus-ui` is promoted, `@web-ui/*` transitional, recognized presentational `@web-*` aliases extract, and Studio composition paths studio-local. Classification is metadata, not permission to import app behavior.

---

## 8. State coverage (normative)

Where a component already supports the state, Studio must show it. Do not fake behavior with non-functional controls when the real primitive can render the state.

| Item | Required states |
| --- | --- |
| Button | rest, hover, active, focus-visible, disabled × existing variants and sizes |
| Badge | all semantic variants × soft/solid |
| Card | existing layout variants; title voice only if the public API still exposes it |
| Input, Label, Textarea, Select | default, hover, focus-visible, disabled, validation/error |
| Tabs | rest, hover, selected, focus-visible, arrow navigation and tab/panel association; disabled explicitly unsupported by current API |
| Toast | existing variants |
| TransportErrorBlock | existing kinds |
| States (keep-web) | loading, empty, error |
| AgentPicker / Global Timeline / Launch / Footer profiles | loading, empty, error (and populated) as the fixture already defines |
| Canvas node chrome | rest, hover, selected (selection is never color-only) |
| RunFormFields / EntityPickerField | populated, empty schema/entries, selected, disabled, invalid where exposed; genuine local controlled changes |
| ProposalSections / RunsTable | populated/empty, event selection, truncated note, long IDs/copy, callback action where provided |
| ComputeResultNodeChrome / ComputeInspectorSections | direct/preset provenance, missing optional report/params/run, affected entries, long content, callback presence/absence |

Before page-level pair view exists, foundation and primitive work must already show its changed tokens/states in the existing Studio galleries and capture separate light/dark evidence. Pair view is an inspection improvement, not permission to defer P0/P1 coverage. Rework existing hard-coded nested theme wrappers into document-theme-following fixtures; retain every distinct behavioral variant, including the six Selection Submenu examples and all VI targets. “Light” labels must not wrap descendants still affected by an ancestor `.dark`.

Focus rings remain visible. Text/control pairings used in the locked language meet WCAG AA.

---

## 9. Non-goals (v1.187)

- Not shipped inside `nexus42` or the desktop installer
- No Storybook adoption
- No live token editor, drag-and-drop theme builder, or YAML write-back
- No unbounded migration of extracts into `@42ch/nexus-ui`
- No daemon/Tauri integration, schema changes, or `@42ch/nexus-contracts` bump
- No `apps/web` or `apps/desktop` source, routing, business-flow, or asset edits
- Not a replacement for `apps/web` product QA — Studio complements it
- No new product routes or renamed top-level nav labels
- No new logo geometry or desktop icon replacement
- No second token authority, per-app override layer, or framework migration

---

## 10. Acceptance (product)

- [ ] Studio starts without daemon on the documented command
- [ ] Header theme toggle reflects the DESIGN pair; pair view shows the same specimen in both themes without a second token system
- [ ] Filterable indexes cover §7 families; no-results is visible; hash deep links work
- [ ] Every remaining/changed token family has a Tokens (or Brand) specimen in both themes
- [ ] Component sections in §7.3 remain reachable; §8 states are present where supported
- [ ] All logo variants and `NexusMark` render; timeline-mark geometry is unchanged
- [ ] Voice section shows labeled specimens matching the locked Voice rules
- [ ] Every Surfaces fixture family in §7.5 renders in both themes with source badges
- [ ] Frozen routes and nav labels in §6 remain
- [ ] 1440×900, 1280×800, and 390×844: usable nav, no document-wide horizontal overflow
- [ ] Keyboard focus visible; reduced-motion recipes collapse
- [ ] Read-only footer / DESIGN edit hint remains
- [ ] `wire_contracts_changed: false`
- [ ] `pnpm --filter web typecheck` and `pnpm --filter web build` pass **without** web source edits
- [ ] No claim of web or desktop redesign acceptance

---

## 11. Resolved (historical, still true)

- CSS pipeline: `@nexus/design-tokens` (`tooling/design-tokens`) — shared preset + `tokens.css`
- Import strategy: public `@42ch/nexus-ui` + annotated `@web-ui/*` + `@web-*` extracts
- Theme mechanism: `.dark` on `html`

The DESIGN pair locks exact v0.5 values and component recipes. This document locks the single compiler, iframe isolation, discovery metadata and source-compatible boundaries. Implementation evidence is required independently; these target contracts do not claim runtime or visual QC verification.

---

## 12. Historical (not current promises)

Studio originated in V1.98 as a read-only DESIGN gallery. Later iterations added `@42ch/nexus-ui` promotions, `@web-*` extracts, Surfaces nested routes, and many fixture families. Those shipped behaviors remain as the inventory in §7.

The following are **archived process**, not v1.187 work to re-execute:

- V1.98 “parity with shipped Web UI” as the primary outcome (this iteration overhauls the shared language; web page QA is deferred)
- V1.99 first-promotion-batch checklists
- V1.106/V1.107 Surfaces addition tables and App Toast-adoption follow-ups
- Earlier IA-guide prose that described five generic home cards and warm-paper Chronos copy as the target look (superseded by §6 and §7)

Do not copy those checklists forward as open tasks.
