# Design Studio — Specification v0 (Draft)

**Status**: Draft (V1.98) — product contract; P-1 Prepare complete; P0 implements gallery  
**Document class**: Dev-surface auxiliary app (not author-facing product)  
**Created**: 2026-07-08 (`@product-manager`)  
**Scope**: `apps/design-studio` — read-only gallery for Nexus DESIGN SSOT, brand VI, and `apps/web` UI primitives  
**Iteration compass**: [v1.98-design-studio-and-design-unification-compass-v1.md](../../iterations/v1.98-design-studio-and-design-unification-compass-v1.md)  
**IA guide**: [design-studio-information-architecture.md](../../iterations/v1.98/guides/design-studio-information-architecture.md)  
**Coordinates with**:

- Repo-root [`DESIGN.md`](../../../DESIGN.md) + [`DESIGN.dark.md`](../../../DESIGN.dark.md) — sole normative token SSOT after V1.98 merge
- [`web-ui.md`](web-ui.md) §30 — V1.98 stage note (studio is dev tooling, not Control Room feature)
- [`design-unification.md`](../../iterations/v1.98/specs/design-unification.md) — merge rules (architect-owned, P-1)
- `@42ch/nexus-ui` — brand layer only (logos, marks, `theme.css` swatches)
- `apps/web/src/components/ui/*` — gallery display source for shadcn primitives (no package migration)

---

## 1. Purpose

Contributors and frontend implementers need a **single visual workspace** to validate Nexus tokens, brand VI, component states, voice samples, and representative product chrome — without running the daemon, Tauri, or navigating live product flows.

Design Studio is a **standalone Vite + React SPA** (`apps/design-studio`) that mirrors the unified DESIGN contract. It is a **read-only showcase**: token edits happen in repo-root `DESIGN.md` / `DESIGN.dark.md` on disk; refresh the dev server to see updates. App chrome displays **Read-only · edit `DESIGN.md`** as a persistent helper (see IA guide §2).

**Product outcome (V1.98):** tuning UI/UX/brand no longer requires mental diffing of YAML frontmatter or hunting component usage across `apps/web`. One gallery, one DESIGN SSOT, measurable parity with the shipped Web UI.

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

| Source | Allowed | Notes |
| --- | --- | --- |
| Root `DESIGN.md` / `DESIGN.dark.md` | Yes | SSOT; consumed via `@nexus/design-tokens` CSS pipeline |
| `@nexus/design-tokens` | Yes | Shared `tokens.css` + Tailwind preset with `apps/web` |
| `@42ch/nexus-ui` | Yes | Brand VI gallery only |
| `@web-ui/*` → `apps/web/src/components/ui/*` | Yes (transitional) | Vite/TS alias; see `design-unification.md` §7 |
| `@web-lib/utils` → `apps/web/src/lib/utils.ts` | Yes | `cn()` helper only |
| `apps/web` screens, routing, `NexusClient`, daemon hooks | **No** | Prevents studio becoming a second product shell |
| `apps/web/src/components/layout/**` | **No** | Surfaces slice uses studio-local chrome fixtures |
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

- **Brand layer only** — logos, marks, `brandColors`, `theme.css`
- **Do not** migrate shadcn `components/ui/*` into `nexus-ui` in V1.98 (V1.87 boundary preserved)

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
| Voice & Content | P1 | Labeled specimens per [IA guide §4.4](../../iterations/v1.98/guides/design-studio-information-architecture.md) — strings from DESIGN § Voice & Content |
| Surfaces | P1 | Setup step card + App shell chrome fixtures per [IA guide §4.5](../../iterations/v1.98/guides/design-studio-information-architecture.md) |

Section nav labels and per-component matrix: [IA guide](../../iterations/v1.98/guides/design-studio-information-architecture.md).

---

## 6. Non-goals (V1.98)

- Not shipped inside `nexus42` binary or desktop installer
- No Storybook adoption
- No live token editor, drag-and-drop theme builder, or YAML export/write-back
- No migration of shadcn primitives into `@42ch/nexus-ui`
- No daemon/Tauri integration, schema changes, or `@42ch/nexus-contracts` bump
- Not a replacement for `apps/web` product QA — studio complements, does not gate author flows
- Desktop clean-state / first-launch work → **V1.99** (not studio scope)

---

## 7. Acceptance hooks (product — implementation evidences in P0)

- [ ] Studio starts without daemon on documented dev command
- [ ] Light/dark toggle reflects root DESIGN pair (spot-check ≥3 semantic tokens per theme)
- [ ] Every primitive in `components/ui/*.tsx` (excluding tests) appears in Components gallery with ≥1 interactive state
- [ ] All four `logoVariants` from `@42ch/nexus-ui` render with clear-space guidance visible
- [ ] Voice & Content section shows ≥3 labeled specimens matching IA guide §4.4 fixture strings
- [ ] Surface slices: Setup step card + App shell chrome per IA guide §4.5 — identifiable without live routing
- [ ] `wire_contracts_changed: false`

---

## 8. Resolved (architect P-1)

- **B1 Merge precedence:** `design-unification.md` §3 — apps/web wins neutrals/accent scales; root wins brand VI extended; components union.
- **B2 CSS pipeline:** `@nexus/design-tokens` (`tooling/design-tokens`) — shared preset + `tokens.css`.
- **B3 Import strategy:** `@web-ui/*` Vite alias; `@web-lib/utils` for `cn()` only.
- **B4 Primitive inventory:** 11 modules (§4.3); `tabs` barrel export in P0 T1.
