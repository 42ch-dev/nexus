# Design Studio — Specification v0 (Draft)

**Status**: Draft (V1.98) with V1.99 boundary amendment in progress — product contract
**Document class**: Dev-surface auxiliary app (not author-facing product)  
**Created**: 2026-07-08 (`@product-manager`)  
**Scope**: `apps/design-studio` — read-only gallery and visual proving ground for Nexus DESIGN SSOT, brand VI, shared presentational primitives, and representative surface fixtures
**Iteration compass**: [v1.98/delivery-compass.md](../iterations/v1.98/delivery-compass.md)  
**V1.99 compass**: [v1.99/delivery-compass.md](../iterations/v1.99/delivery-compass.md)
**IA guide**: [design-studio-information-architecture.md](../iterations/v1.98/guides/design-studio-information-architecture.md)  
**Coordinates with**:

- Repo-root [`DESIGN.md`](../../DESIGN.md) + [`DESIGN.dark.md`](../../DESIGN.dark.md) — sole normative token SSOT after V1.98 merge
- [`web-ui.md`](web-ui.md) §30 — V1.98 stage note (studio is dev tooling, not Control Room feature)
- [`design-unification.md`](../iterations/v1.98/specs/design-unification.md) — merge rules (architect-owned, P-1)
- [`component-promotion-boundary.md`](../iterations/v1.99/specs/component-promotion-boundary.md) — V1.99 draft boundary for selected pure presentational primitives in `@42ch/nexus-ui`
- [`studio-first-ui-workflow.md`](../iterations/v1.99/guides/studio-first-ui-workflow.md) — V1.99 validation path from studio fixtures to package promotion to Web integration
- [`studio-first-visual-then-app.md`](../iterations/v1.101/guides/studio-first-visual-then-app.md) — V1.101 process note (Studio visual → App wiring; human smoke separate)
- [`studio-first-visual-then-app.md`](../iterations/v1.102/guides/studio-first-visual-then-app.md) — V1.102 process note (same discipline; Badge tone + Settings chrome + optional Surfaces Stretch)
- [`studio-first-visual-then-app.md`](../iterations/v1.103/guides/studio-first-visual-then-app.md) — V1.103 process note (Settings shell + section fixtures; DESIGN Voice copy tables in section specs)
- [`studio-first-invariant.md`](../iterations/v1.107/guides/studio-first-invariant.md) — **V1.107 locked invariant** (需求 → Studio↔DESIGN.md → App); supersedes V1.106 guide for active iteration
- [`studio-ui-tune.md`](../iterations/v1.107/specs/studio-ui-tune.md) — **V1.107 Must** — Studio Tailwind content, visual FBs, Toast App adoption, shell/Settings presentational SSOT
- `@42ch/nexus-ui` — brand layer plus approved presentational primitives (V1.99 Button/Badge/Card; V1.100 form fields; V1.101 `Select`; V1.102 Badge `tone` soft/solid)
- `apps/web/src/components/ui/*` — transitional gallery source for primitives not yet promoted
- `apps/web/src/components/setup/*` — app-shared setup compositions (e.g. `AgentPicker`); Studio may import via gallery alias — **not** `@42ch/nexus-ui`

---

## 1. Purpose

Contributors and frontend implementers need a **single visual workspace** to validate Nexus tokens, brand VI, component states, voice samples, and representative product chrome — without running the daemon, Tauri, or navigating live product flows.

Design Studio is a **standalone Vite + React SPA** (`apps/design-studio`) that mirrors the unified DESIGN contract. It is a **read-only showcase**: token edits happen in repo-root `DESIGN.md` / `DESIGN.dark.md` on disk; refresh the dev server to see updates. App chrome displays **Read-only · edit `DESIGN.md`** as a persistent helper (see IA guide §2).

**Product outcome (V1.98):** tuning UI/UX/brand no longer requires mental diffing of YAML frontmatter or hunting component usage across `apps/web`. One gallery, one DESIGN SSOT, measurable parity with the shipped Web UI.

**Product outcome (V1.99):** Design Studio becomes the first visual proving ground for reusable View-level UI. Accepted presentational primitives may graduate into `@42ch/nexus-ui`; app behavior, daemon state, routing, and full product shells still graduate only through `apps/web`.

---

## 2. Audiences

| Audience | Job-to-be-done | Studio value |
| --- | --- | --- |
| **Contributors** (design-minded maintainers) | Tune colors, typography, spacing, and component tokens with confidence | Side-by-side token tables + live primitives + light/dark toggle |
| **Frontend developers** | Pick correct variant/state when building screens | Component matrix with interactive states; surface slices as composition reference |
| **Brand / VI reviewers** | Confirm logo usage, clear space, and theme.css alignment | Brand VI section with all four `@42ch/nexus-ui` logo variants + `NexusMark` |
| **Authors** (local Web UI users) | — | **Not in scope** — authors never see design-studio; it is not bundled in `nexus42` |

---

## 3. Placement and boundaries (normative)

### 3.1 Placement: `apps/design-studio`

- pnpm workspace member under `apps/*` (same polyglot product-surface rule as `apps/web`)
- **Consumer**, not producer — no daemon API, no `NexusClient`, no `@42ch/nexus-contracts` wire types
- Runs via `pnpm --filter design-studio dev` (or documented root script alias) **without** daemon or Tauri

### 3.2 What studio may import

**Two-tier model (V1.128):** Studio consumes both `@42ch/nexus-ui` **and** `@web-*` aliases. They are not interchangeable.

| Tier | Pattern | Meaning |
| --- | --- | --- |
| Promoted primitive | `@42ch/nexus-ui` | Publishable package export after Studio visual acceptance |
| App presentational extract | `@web-layout/*`, `@web-canvas/*`, `@web-setup/*`, `@web-settings/*`, `@web-global-timeline/*`, `@web-shell/*`, … | Monorepo-only alias → `apps/web` props-driven chrome |
| Transitional primitive | `@web-ui/*` | Unpromoted `apps/web/src/components/ui/*` mirror |

Surfaces gallery sections display source badges distinguishing extract vs promoted. Mass migration of `@web-*` into `@42ch/nexus-ui` is **out of scope** — clarity over consolidation. Detail: [web-alias-clarity](../iterations/v1.128/specs/web-alias-clarity.md).

| Source | Allowed | Notes |
| --- | --- | --- |
| Root `DESIGN.md` / `DESIGN.dark.md` | Yes | SSOT; consumed via `@nexus/design-tokens` CSS pipeline |
| `@nexus/design-tokens` | Yes | Shared `tokens.css` + Tailwind preset with `apps/web` |
| `@42ch/nexus-ui` | Yes | Brand VI plus V1.99-approved pure presentational primitives, via public package exports only |
| `@web-ui/*` → `apps/web/src/components/ui/*` | Yes (transitional) | Vite/TS alias for not-yet-promoted primitives only; promoted primitives should use `@42ch/nexus-ui` |
| `@web-setup/*` → `apps/web/src/components/setup/*` | Yes | Props-driven setup compositions (AgentPicker, TopStepIndicator, WorkspacePathField); no daemon |
| `@web-layout/*` → `apps/web/src/components/layout/presentational/*` | Yes (V1.107) | Props-driven shell chrome extracts only; no routing or daemon hooks |
| `@web-settings/*` → `apps/web/src/components/settings/presentational/*` | Yes (V1.107) | Props-driven Settings section chrome; no IPC or storage |
| `@web-lib/utils` → `apps/web/src/lib/utils.ts` | Yes | `cn()` helper only |
| `apps/web` screens, routing, `NexusClient`, daemon hooks | **No** | Prevents studio becoming a second product shell |
| `apps/web/src/components/layout/**` (direct) | **No** | Import presentational chrome only via `@web-layout/*` |
| `apps/web/src/components/settings/**` (direct) | **No** | Import presentational chrome only via `@web-settings/*` |
| `apps/web` app providers, route definitions, product hooks, localStorage-backed product state, Tauri helpers | **No** | Studio fixtures stay daemon-independent and behavior-free |
| Live token override, localStorage theme hacks, YAML write-back | **No** | V1.98 read-only invariant |

### 3.3 Toolchain alignment with `apps/web`

| Concern | `apps/web` | `apps/design-studio` |
| --- | --- | --- |
| Bundler | Vite 6 | Vite 6 (same major) |
| React | 18.3 | 18.3 |
| TypeScript | strict, `@/*` alias | strict, `@/*` + `@web-ui/*` aliases |
| Tailwind | v3.4, `class` darkMode | v3.4 — **shared preset** from `@nexus/design-tokens` |
| CSS tokens | `@nexus/design-tokens/tokens.css` | Same import — no second transcription |
| Test runner | Vitest 3 | Vitest 3 |
| Contracts | `@42ch/nexus-contracts` required | **Not used** — dev surface only |
| Dev server port | 5173 | 5174 (document in README; avoid clash) |

### 3.4 Relationship to `apps/web`

- `apps/web` remains the **author-facing** local product UI (daemon-served Control Room + Setup + Authoring)
- Design Studio is **dev/ contributor tooling** — comparable to Storybook in intent, but Nexus-owned and DESIGN-native
- After DESIGN unification, **both** web and studio read the **same root DESIGN pair**; studio must not invent tokens

### 3.5 Relationship to `@42ch/nexus-ui`

- **V1.98 baseline:** brand layer only — logos, marks, `brandColors`, `theme.css`
- **V1.99 amendment:** selected pure presentational primitives may move into `@42ch/nexus-ui` only through the component-promotion boundary draft and follow-up package rule updates.
- **Still forbidden:** app shells, page components, daemon-aware controls, route-aware components, and Web-only behavior must not move into `@42ch/nexus-ui`.
- **Transition rule:** Design Studio may consume both `@42ch/nexus-ui` and `@web-ui/*` while V1.99 proves the first promotion batch. Promoted primitives should stop using `@web-ui/*` in Studio.
- **Fixture rule:** setup steppers, workspace-row mocks, shell chrome, daemon status strips, nav groups, and page-section compositions remain studio-local unless a smaller primitive beneath them is explicitly promoted by the V1.99 boundary.

---

## 4. Dev UX (normative for V1.98)

### 4.1 Commands

| Action | Command |
| --- | --- |
| Start dev server | `pnpm --filter design-studio dev` |
| Build | `pnpm --filter design-studio build` |
| Test | `pnpm --filter design-studio test` |

Exact port and script aliases finalized in P0 `README` / `AGENTS.md`.

### 4.2 Contributor tuning workflow (read-only mirror)

1. **Open studio** — `pnpm --filter design-studio dev`; default to Tokens overview.
2. **Baseline** — toggle light/dark; scan token tables and component matrix for current SSOT.
3. **Edit SSOT** — change values in repo-root `DESIGN.md` and/or `DESIGN.dark.md` (IDE or PR).
4. **Refresh** — reload studio (HMR or manual refresh) until gallery reflects edits.
5. **Validate** — confirm Brand VI, Components, Voice & Content, and Surface slices still look correct in both themes.
6. **Verify product** — run `pnpm --filter web test` and `pnpm --filter web run build` to ensure `apps/web` consumers still resolve tokens (no unintended drift).

**Success signal:** a contributor can complete steps 1–6 without reading `index.css` or tailwind config to understand token impact.

### 4.3 Theme toggle

- Studio exposes an explicit **light / dark** control in app chrome (not author settings)
- Toggle switches CSS variable layer between `DESIGN.md` and `DESIGN.dark.md` values
- Default on load: `prefers-color-scheme`, overridable per session (no persistence required in V1.98)

---

## 5. Gallery scope (summary — detail in IA guide)

| Section | Priority | V1.98 minimum |
| --- | --- | --- |
| Tokens | P0 | Colors, typography, spacing, rounded; elevation/motion if present in SSOT |
| Brand VI | P0 | 4 logo variants + `NexusMark` + `theme.css` swatches + clear-space callout |
| Components | P0 | All `apps/web/src/components/ui/*.tsx` primitives (variant/state matrix) |
| Voice & Content | P1 | Labeled specimens per [IA guide §4.4](../iterations/v1.98/guides/design-studio-information-architecture.md) — strings from DESIGN § Voice & Content |
| Surfaces | P1 | Setup step card + App shell chrome fixtures per [IA guide §4.5](../iterations/v1.98/guides/design-studio-information-architecture.md) |

Section nav labels and per-component matrix: [IA guide](../iterations/v1.98/guides/design-studio-information-architecture.md).

---

## 6. Non-goals (V1.98)

- Not shipped inside `nexus42` binary or desktop installer
- No Storybook adoption
- No live token editor, drag-and-drop theme builder, or YAML export/write-back
- No unbounded migration of shadcn primitives into `@42ch/nexus-ui`; V1.99 allows only approved pure presentational primitives
- No daemon/Tauri integration, schema changes, or `@42ch/nexus-contracts` bump
- Not a replacement for `apps/web` product QA — studio complements, does not gate author flows
- Desktop clean-state / first-launch author onboarding is owned by **V1.105** (setup wizard chrome via Studio fixtures — see [v1.105/delivery-compass.md](../iterations/v1.105/delivery-compass.md)); Design Studio itself does not ship author onboarding.

### V1.106 Surfaces additions (P0 Must — iteration detail)

**Authority:** [`studio-first-pipeline.md`](../iterations/v1.106/specs/studio-first-pipeline.md) §SP-V1106-003.

| Route | Fixture file | Classification |
|-------|--------------|----------------|
| `/surfaces/launch` | `launch-daemon-fixtures.tsx` | `@web-setup/daemon-ready-splash` import |
| `/components` Toast section | `toast-fixtures.tsx` or inline | `@42ch/nexus-ui` + Studio renderer |

Register Launch in `SURFACES_SECTIONS` alongside existing Setup / Shell / AgentPicker / Daemon slices. **Banner** (`/surfaces/banner`, `main-banner-fixtures.tsx`) was a V1.106 composition-only sketch — **removed in V1.128 P0** (closes residual R-V1128P0-001); do not re-add without a new plan entry.

### V1.107 Surfaces and import amendments (P0 Must — iteration detail)

**Authority:** [`studio-ui-tune.md`](../iterations/v1.107/specs/studio-ui-tune.md).

| Topic | Lock |
|-------|------|
| Studio Tailwind `content` | Scan `setup/**`, `layout/presentational/**`, `packages/nexus-ui/src/**` (FB-000) |
| Shell Surfaces | `/surfaces/shell` imports `@web-layout/shell-sidebar-chrome` — replaces inline `AppShellFixture` stub (FB-013) |
| Footer / health | Studio fixtures import `@web-layout/footer-profiles-chrome`, `@web-layout/daemon-health-indicator-chrome` (FB-014) |
| Settings host | `settings-host-fixtures.tsx` imports `@web-settings/*` + `@web-setup/workspace-path-field` (FB-015) |
| Toast | Package primitive in Studio; App adopts via thin `@/lib/use-toast` re-export (FB-012) — closes `R-V1106P0-001` |
| Voice & Content | Workspace field label **Workspace folder**; CTA **Change Folder…** on wizard and Settings (FB-008) |

**Note:** V1.106 promoted Toast to `@42ch/nexus-ui` for Studio fixtures; V1.107 completes App adoption — do not treat “package Toast exists” as “App unified” until FB-012 lands.

---

## 7. Acceptance hooks (product — implementation evidences in P0)

- [ ] Studio starts without daemon on documented dev command
- [ ] Light/dark toggle reflects root DESIGN pair (spot-check ≥3 semantic tokens per theme)
- [ ] Every primitive in `components/ui/*.tsx` (excluding tests) appears in Components gallery with ≥1 interactive state
- [ ] All four `logoVariants` from `@42ch/nexus-ui` render with clear-space guidance visible
- [ ] Voice & Content section shows ≥3 labeled specimens matching IA guide §4.4 fixture strings
- [ ] Surface slices: Setup step card + App shell chrome per IA guide §4.5 — identifiable without live routing
- [ ] `wire_contracts_changed: false`

### V1.99 additional acceptance hooks

- [ ] At least one approved presentational primitive is consumed from `@42ch/nexus-ui` in Studio instead of `@web-ui/*`
- [ ] `/surfaces` functions as a visual proving ground for setup/shell direction without importing Web layout or daemon code
- [ ] Each Studio fixture that influences Web has a recorded decision: promote to package, keep in Web, keep in Studio, or defer
- [ ] Promoted primitives use public `@42ch/nexus-ui` exports; unpromoted primitives are the only allowed remaining `@web-ui/*` usage
- [ ] `/surfaces` fixture sections record whether they are `promoted primitive`, `studio-local fixture`, `web-only wrapper`, or `future web product component`

---

## 8. Resolved (architect P-1)

- **B1 Merge precedence:** `design-unification.md` §3 — apps/web wins neutrals/accent scales; root wins brand VI extended; components union.
- **B2 CSS pipeline:** `@nexus/design-tokens` (`tooling/design-tokens`) — shared preset + `tokens.css`.
- **B3 Import strategy:** `@web-ui/*` Vite alias; `@web-lib/utils` for `cn()` only.
- **B4 Primitive inventory:** 11 modules (§4.3); `tabs` barrel export in P0 T1.
