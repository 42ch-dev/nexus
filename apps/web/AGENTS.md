# apps/web — AGENTS.md

The local-first **Control Room + Setup** Web UI. Daemon-served React SPA,
Tauri-ready. Parent rules: [`../../AGENTS.md`](../../AGENTS.md) (repo),
[`../../.mstar/AGENTS.md`](../../.mstar/AGENTS.md) (harness).

## Identity & placement

- This is the **OSS local Web UI** (`apps/web`), a pnpm workspace member under
  `apps/*`. It is **not** the private `nexus-platform` cloud SaaS — do not
  introduce cloud/platform-gated features, platform auth, or platform-only types
  here (spec invariant: `web-ui.md` §2.2).
- Consumes `@42ch/nexus-contracts` via `workspace:*`. **Never** hand-write a
  second source of wire DTO types in this package — all wire types come from the
  generated contracts (web-ui.md §12.6).

## SSOT & authority

- **Design tokens**: Root [`DESIGN.md`](../../DESIGN.md) + [`DESIGN.dark.md`](../../DESIGN.dark.md) are the **sole normative SSOT** (Production completeness). Shared CSS variables + Tailwind preset live in `tooling/design-tokens` (`@nexus/design-tokens`). `src/index.css` + `tailwind.config.ts` *consume* them via `@import '@nexus/design-tokens/tokens.css'` and the shared preset; they do not invent tokens. If a token you need is missing, **report** it to the architect — do not fabricate a value.
- **Product contract**: [`web-ui.md`](../../.mstar/specs/web-ui.md).
- **Transport boundary**: the `NexusClient` interface
  (`src/lib/nexus/types.ts`). Screens must depend only on the interface, never
  on `fetch`/`invoke` directly — that is what keeps the V1.65 Tauri shell a
  one-impl swap (web-ui.md §5, §9). The HTTP path prefix for the daemon
  surface is `/v1/daemon/*`; the SPA reaches the daemon at
  `http://127.0.0.1:<port>/v1/daemon/*`.

## Contracts status (post Wave-1 merge)

This app builds against the **V1.64 hardened contract base** (Track B / plan P0
merged on the integration branch). Cursor pagination (F-P1), the shared
`ErrorResponse` (F-E1), and the findings list endpoint (F-P2) are all available
and consumed by the screens. Remaining gaps the UI adapts around:

| Gap | Adaptation | Target |
| --- | --- | --- |
| List arrays not unified to `items` (F-P3) | `normalizeList` adapter at the query boundary (`src/lib/nexus/adapters.ts`) maps `works`/`sessions`/`schedules`/`capabilities` → `items`. Findings already uses `items`. | V1.66+ structural closure |
| No `sort_by`/`sort_order` (F-F1) | Client-side `sortByDate` for small un-paginated lists; cursor-paginated lists keep server order. | V1.66+ server-side sort |
| `CreateWorkRequest` has no `work_profile` field | **Resolved (V1.67 G1)** — the Create-Work dialog exposes a Work-profile selector (novel/essay/game-bible/script, default `novel`) wired to the existing `work_profile` field. | — |
| Preset get/update/delete (no routes/contracts) | **Resolved (V1.67 G2)** — `getPreset`/`updatePreset`/`deletePreset` promoted onto `NexusClient` (21 → 24); daemon routes + contracts already shipped. A form-based management UI is deferred to the V1.68 canvas. | V1.68 canvas UI |
| Capability admission gates not in list response | Capabilities page shows name + I/O schemas only; admission-gate logic is daemon-side. | Future capability-detail endpoint |

## Build / typecheck contract

- `build` and `typecheck` resolve `@42ch/nexus-contracts` types from its `dist/`.
  The web package runs `pnpm --filter @42ch/nexus-contracts run build` via its
  `prebuild` and `pretypecheck` lifecycle scripts, so `pnpm --filter web run build`
  and `pnpm --filter web run typecheck` are self-contained from a fresh install.
  CI's `web-build` job also builds contracts first, so the explicit CI step and
  the local lifecycle hook stay aligned.
- Workspace + lockfile surfaces touched here (`pnpm-workspace.yaml`, root
  `package.json`, lockfile) are shared with P0's codegen — coordinate at
  integration merge (compass §3 parallelism note).

## Conventions

- **TypeScript strict.** No `any` for wire shapes; prefer generated types.
- **Styling**: Tailwind utilities referencing DESIGN.md theme keys; compose with
  `cn()` (`src/lib/utils.ts`). App-owned component primitives live in
  `src/components/ui/` and read from the DESIGN.md component tables. V1.99-approved
  shared presentational primitives may be consumed from `@42ch/nexus-ui` directly
  or through thin app-local wrappers/re-exports; app behavior, routing, daemon data,
  and product copy stay in `apps/web`.
- **Accessibility (WCAG 2.1 AA floor)**: keep keyboard paths, the global
  focus-visible ring (`src/index.css`), visible labels (no icon-only nav), and
  reduced-motion handling. DESIGN.md dark/light tokens must both pass contrast.
- **Voice & Content**: follow DESIGN.md §Voice & Content — Title Case for titles/
  nav/buttons/headers; sentence case for helpers/errors/toasts; Verb-only for
  buttons/CTAs (single Title Case verb — `Save`, `Create`, `Delete`); name the
  changed object in the dialog title / surrounding copy when screen readers need
  it. Avoid protocol jargon (`ACP`, `cursor token`) in the UI surface.
- **Daemon port**: default HTTP transport `127.0.0.1:8420`
  (`crates/nexus-daemon-runtime/src/boot.rs`); override via `NEXUS_DAEMON_PORT`
  or `VITE_DAEMON_URL` (dev proxy).
- **i18n (V1.112+):** see the [i18n conventions](#i18n) section below.

## i18n

- **Library:** `i18next` + `react-i18next`.
- **Catalog location:** `src/locales/{en,zh-CN}/`.
- **Namespaces (nine):**

  | Namespace | Scope |
  |-----------|-------|
  | `common` | Shared actions, generic loading/error fragments |
  | `shell` | Header, sidebar, mobile nav, route titles, not-found |
  | `settings` | Settings shell helper, section nav labels, Appearance page |
  | `setup` | Setup wizard (P1) |
  | `canvas` | Canvas inspectors, panels, and commands (P1) |
  | `reading` | Reading toolbar and inspectors (P1) |
  | `findings` | Findings list/detail chrome (P1) |
  | `memory` | Memory, soul, and chapters chrome (P1) |
  | `commands` | Command-palette action labels and group headings (P1) |

- **Key convention:** dot-separated within the namespace file, e.g.
  `settings.appearance.language.label` or `shell.nav.works`. English source
  strings follow DESIGN.md §Voice & Content (Title Case for nav/titles/buttons).
- **Usage pattern:** `const { t } = useTranslation('<namespace>')` and render
  `t('path.to.key')`. Do not concatenate translated strings in components; use
  i18next interpolation (`{{name}}`) when a value is dynamic.
- **Rule:** new user-facing strings in `apps/web` must use i18n from day one.
- **Caller-owned copy:** pass `t()` strings into `@42ch/nexus-ui` and shared
  primitives; do not bake product copy into packages.
- **User-facing-only catalogs:** only author-visible product chrome enters the
  locale catalogs. Exclude developer-auxiliary surfaces (`apps/design-studio`),
  test fixtures, and manuscript body text.
- **Normative spec:**
  [`.mstar/iterations/v1.112/specs/i18n-foundation.md`](../../.mstar/iterations/v1.112/specs/i18n-foundation.md).
