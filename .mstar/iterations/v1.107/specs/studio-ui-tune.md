# Studio UI Tune — Primary Spec (V1.107)

**Status:** Draft — product-complete (§5.1 PM); architecture-locked (§5.2 architect); writing-complete (§5.3)  
**Tier:** Must (P0)  
**Plan:** `2026-07-10-v1.107-studio-ui-tune`  
**Compass:** `../v1.107-delivery-compass.md`

## Product outcome

Authors and contributors can trust Design Studio as the visual SSOT: matrices paint correctly, wizard chrome matches Settings patterns, and App adopts the same package primitives Studio previews.

**User-visible win:** Status pills, buttons, selects, and wizard steps look correct in Studio before App wiring; Toast and shell/Settings Surfaces stop drifting from App.

## Problem

V1.106 completed the studio-first pipeline and claimed Badge/Select/AgentPicker polish, but Design Studio still fails visually for many matrices (Tailwind `content` omits setup + nexus-ui sources), and App has not adopted package Toast (`R-V1106P0-001`). Studio shell and Settings fixtures duplicate or stub App chrome.

## Goals

1. Fix Studio Tailwind content coverage so setup + nexus-ui utilities paint (FB-000).
2. Land visual FBs FB-V1107-001..011 against Studio, then App.
3. Close coverage hygiene FB-V1107-012..016 (Toast, shell, Settings, badges, backlog).

## Non-goals

- DF-70 BYOK / execution-mode
- Package-promoting Dialog / Tabs / Table / States
- Full author-domain Surfaces routes (Reading/SOUL/Canvas/Memory/Findings) — backlog index only in 016

## Studio-first rule

All visual FBs: Studio accept (light+dark, focus-visible) before App “parity” claims.

## Voice & Content (locked)

Follow [IA guide §4.4](../../v1.98/guides/design-studio-information-architecture.md): **Title Case** for headings, labels, and CTAs; **sentence case** for helper text, descriptions, and empty-state body copy.

| Surface | Element | Copy (exact) |
|---------|---------|--------------|
| Workspace path field | Field label | **Workspace folder** |
| Workspace path field | Change action | **Change Folder…** |
| Workspace path field | Loading placeholder | `Resolving…` (sentence case helper) |
| AgentPicker | Empty scan | **No agents found on PATH** (heading) |
| AgentPicker | Loading | *Scanning for local ACP agents…* (sentence case helper) |
| Uninstalled agent card | Badge | **Not installed** |

Wizard workspace step **must** reuse `WORKSPACE_PATH_FIELD_LABEL` / `WORKSPACE_PATH_CHANGE_ACTION` — no **Browse…** on the wizard path (Settings-only historical note in V1.104 specs does not apply after FB-008).

## Wire

Prefer `wire_contracts_changed: false`.

---

## Architecture contracts (§5.2 locked)

Implementers treat this section as the technical SSOT alongside per-FB acceptance below.

### Gallery aliases (Design Studio)

| Alias | Resolves to | Use |
|-------|-------------|-----|
| `@web-setup/*` | `apps/web/src/components/setup/*` | AgentPicker, TopStepIndicator, **WorkspacePathField** (existing alias) |
| `@web-layout/*` | `apps/web/src/components/layout/presentational/*` | Shell sidebar, footer profiles, header/health chrome (**new**) |
| `@web-settings/*` | `apps/web/src/components/settings/presentational/*` | ConnectDaemon + Setup section chrome (**new**) |

Add `@web-layout` and `@web-settings` in `apps/design-studio/vite.config.ts`, `vitest.config.ts`, `tsconfig.json`, and extend `tooling/check-ui-guardrails.sh` allowlist. Do **not** import `apps/web/src/pages/**`, `hooks/**`, or `lib/nexus/**` from Studio.

### Studio Tailwind `content` (FB-000 exact paths)

In `apps/design-studio/tailwind.config.ts`, **add** (keep existing entries):

```text
../web/src/components/setup/**/*.{ts,tsx}
../web/src/components/layout/presentational/**/*.{ts,tsx}
../../packages/nexus-ui/src/**/*.{ts,tsx}
```

### WorkspacePathField (FB-008, FB-015)

**File:** `apps/web/src/components/setup/workspace-path-field.tsx`

**Exports:** `WorkspacePathField`, `WORKSPACE_PATH_FIELD_LABEL` (`'Workspace folder'`), `WORKSPACE_PATH_CHANGE_ACTION` (`'Change Folder…'`), `WorkspacePathFieldProps`.

| Prop | Type | Notes |
|------|------|-------|
| `id` | `string` | Required — `label htmlFor` ↔ readonly `Input` |
| `path` | `string` | Display value |
| `loading` | `boolean` | Optional — placeholder `Resolving…` |
| `changeDisabled` | `boolean` | Optional — disables CTA |
| `onChangeClick` | `() => void` | Optional — omit in Studio fixtures |
| `layout` | `'settings-row' \| 'wizard-stack'` | Default `settings-row`; wizard uses label-above field (no in-row wizard label chrome) |
| `desktopAvailable` | `boolean` | When false, show browser-only helper |
| `browserOnlyHelper` | `string` | Optional override |
| `data-testid` | `string` | Optional root; path input uses caller-chosen id |

**Consumers:** `settings-workspace-section.tsx` (Card wrapper stays in page), `setup-step-workspace.tsx`, Studio `WorkspaceBody` + Settings workspace fixture — import field, do not duplicate markup.

### Shell SSOT — presentational extract + `@web-layout` (FB-013, FB-014)

**Decision:** Extract props-driven chrome into `apps/web/src/components/layout/presentational/`; Studio imports via `@web-layout/*`. **Forbidden:** importing routing-heavy `sidebar.tsx` / `root-layout.tsx` in Studio.

**Blast radius (in scope only):**

- New `layout/presentational/*.tsx` chrome modules
- Thin refactors: `sidebar.tsx`, `footer-profiles.tsx`, `header.tsx`, `daemon-health-indicator.tsx` delegate markup to chrome
- Studio `surfaces.tsx` + fixtures; Design Studio alias + guardrail allowlist; Tailwind content path above

**Out of scope:** `root-layout.tsx` routing, `pages/**`, daemon client wiring, `FooterProfiles` create-creator dialog behavior, mobile nav logic.

| Module | Export | Key props |
|--------|--------|-----------|
| `shell-sidebar-chrome.tsx` | `ShellSidebarChrome` | `activeTab: 'creator' \| 'orchestrator'`, `activeRoute: string`, `settingsActive: boolean`, `navGroups` (Creator/Orchestrator IA data), `onTabChange` (fixture: no-op) |
| `footer-profiles-chrome.tsx` | `FooterProfilesChrome` | `profiles: { id, displayName }[]`, `activeProfileId?: string` — matrix rows 0 / 1 / N |
| `daemon-health-indicator-chrome.tsx` | `DaemonHealthIndicatorChrome` | `kind: 'unknown' \| 'connected' \| 'offline'`, `version?`, `offlineMessage?`, `isRemote?` |
| `shell-header-chrome.tsx` | `ShellHeaderChrome` | `title: string`, `showHealthIndicator: boolean`, `healthProps?`, `showThemeToggle: boolean` (fixture: static) |

App wrappers retain `NavLink`, hooks, and IPC; chrome is markup + classes + `data-testid` SSOT.

### Settings dedup (FB-015)

Shrink `settings-host-fixtures.tsx` by importing:

| Section | Import |
|---------|--------|
| Agent | `@web-setup/agent-picker` (props-driven; existing) |
| Workspace | `@web-setup/workspace-path-field` inside existing Card pattern |
| Connection | `@web-settings/connect-daemon-form-chrome` |
| Setup + confirm | `@web-settings/settings-setup-section-chrome` |

Settings shell nav may remain Studio-local (no React Router); **section body** duplicates must be removed.

### ConnectDaemonFormChrome four-state matrix (FB-015)

**File:** `apps/web/src/components/settings/presentational/connect-daemon-form-chrome.tsx`

Props-driven presentational extract — **no** `useFingerprint`, storage, toast, or daemon client.

**`matrixState` prop (required for fixture rows):**

| Value | V1.92 author-visible state | Visible chrome |
|-------|---------------------------|----------------|
| `firstUse` | First-use TOFU | Default form; fingerprint block hidden until `fingerprintValue` supplied |
| `reconnectMatch` | Pinned fingerprint matches | `data-testid="fingerprint-match-hint"`; primary **Reconnect With These Settings** |
| `fingerprintMismatch` | Certificate changed | `data-testid="fingerprint-mismatch-warning"` + Trust/Cancel pair |
| `loopbackOnly` | Loopback daemon (`fingerprint: ""`) | `data-testid="loopback-info-note"` + **Use Local Daemon** |

**Supporting props:** `savedConfig?: { endpointUrl: string; apiKey: string; label?: string }`, `fingerprintValue?: string`, `fetchStatus?: 'idle' | 'loading' | 'error'`, `fetchErrorMessage?: string`, `showKey?: boolean`, `hasSavedConfig?: boolean`.

Live `ConnectDaemonForm` (`apps/web/src/components/settings/connect-daemon-form.tsx`) keeps behavior; chrome extract is markup/class/testid SSOT for Studio.

### Toast migration (FB-012)

**Decision:** **Thin re-export** — replace `apps/web/src/lib/use-toast.tsx` body with:

```typescript
export {
  ToastProvider,
  useToast,
  Toaster,
  type Toast,
  type ToastVariant,
} from '@42ch/nexus-ui';
```

Preserve `@/lib/use-toast` import path for 40+ call sites. Delete duplicate implementation (~150 LOC). `use-toast.test.tsx` may keep testing through re-export.

**Promotion-boundary (closes `R-V1106P0-001`, `R-V1106P0-002`):** Add `toast` to `.mstar/iterations/v1.99/specs/component-promotion-boundary.md` as **`promote`** (V1.106) with footnote: `lucide-react` is a **package runtime dependency** for Toast variant icons — documented exception; apps retain their own lucide usage elsewhere.

### AgentPicker / wizard (FB-006, FB-007, FB-009)

- `density="compact"` → `grid-cols-1` only (remove `sm:grid-cols-2` on compact path)
- Wizard: `showCustomLaunch={false}` in `setup-step-agent.tsx` + Studio wizard fixture
- `OutboundLink`: Lucide `ArrowUpRight` as child of `<a>`; `gap-3` (or equivalent) between Install and Docs

---

## FB-V1107-000 — Studio Tailwind content coverage

**Problem:** Design Studio `tailwind.config.ts` does not scan `apps/web/src/components/setup/**` or `packages/nexus-ui/src/**`, so many utility classes never generate — Steps circles, Badge solid fills, and Button destructive variants appear invisible.

**User-visible outcome:** Contributors see real chrome in Studio matrices after rebuild, not blank or outline-only controls.

### Acceptance

- [ ] `content` includes `../web/src/components/setup/**/*.{ts,tsx}`.
- [ ] `content` includes `../web/src/components/layout/presentational/**/*.{ts,tsx}`.
- [ ] `content` includes `../../packages/nexus-ui/src/**/*.{ts,tsx}`.
- [ ] Smoke after Studio rebuild: Steps circles, Badge solid semantic fills, Button destructive visible in light and dark.

**SSOT:** `apps/design-studio/tailwind.config.ts`

---

## FB-V1107-001 — Badge Soft/Solid Contrast

**Problem:** Soft Badge variants can read as neutral gray; Solid variants (`queued`, `warning`, `error`, `preset`) may lack visible fills after paint pipeline gaps.

**User-visible outcome:** Authors distinguish running vs queued vs warning vs error at a glance in Control Room and Studio matrices.

### Acceptance

- [ ] All six Soft variants (`neutral`, `running`, `queued`, `warning`, `error`, `preset`) read as distinct hues in light and dark.
- [ ] Solid variants use fills from the same hue family as their Soft counterpart.
- [ ] Solid `queued`, `warning`, `error`, `preset` show visible fills (no outline-only regression).
- [ ] WCAG 2.1 AA contrast floor on text over fills.
- [ ] Studio matrix smoke (light+dark).

**SSOT:** `packages/nexus-ui/src/components/badge.tsx`  
**Token owner:** DESIGN.md `components.badge-status-pill`

---

## FB-V1107-002 — Button Destructive + Disabled

**Problem:** Destructive and disabled Button variants may not paint in Studio; disabled primary can look too similar to enabled.

**User-visible outcome:** Destructive actions (e.g. Settings Re-run Setup confirm) look clearly dangerous; disabled buttons read as inactive.

### Acceptance

- [ ] Destructive variant visible in light and dark (Components matrix + Settings confirm footer).
- [ ] Disabled state clearly muted vs enabled (especially primary).
- [ ] Studio + App smoke.

**SSOT:** `packages/nexus-ui/src/components/button.tsx`

---

## FB-V1107-003 — Select Chevron Inset

**Problem:** Native `<select>` with symmetric padding makes the chevron appear flush against the right border.

**User-visible outcome:** Closed, disabled, and invalid Select controls show clear space between chevron and border.

### Acceptance

- [ ] `appearance-none` + custom chevron with explicit right inset.
- [ ] `disabled` and `invalid` states inherit the same inset.
- [ ] Native `<select>` only — **no Radix Select**.
- [ ] Studio closed/disabled/invalid smoke.

**SSOT:** `packages/nexus-ui/src/components/select.tsx`  
**Token owner:** DESIGN.md `components.select`

---

## FB-V1107-004 — MainBanner Hierarchy

**Problem:** MainBanner description competes visually with title; hierarchy unclear.

**User-visible outcome:** Title dominates; description reads as secondary supporting text.

### Acceptance

- [ ] Title: `text-gray-1000` + semibold.
- [ ] Description: `text-gray-700`.
- [ ] Studio fixture accepted, then App `main-banner.tsx` matches.

**SSOT:** `apps/design-studio/src/fixtures/main-banner-fixtures.tsx` → `apps/web/src/components/layout/main-banner.tsx`

---

## FB-V1107-005 — TopStepIndicator Chrome

**Problem:** Step circles, connectors, and labels may not paint in Studio (depends on FB-000).

**User-visible outcome:** Wizard progress indicator shows active/complete/pending states clearly in Studio and App.

### Acceptance

- [ ] Active, complete, and pending circles visible.
- [ ] Connectors and labels visible.
- [ ] Depends on FB-000; fix tokens/classes only if still broken after content fix.
- [ ] Studio + App smoke.

**SSOT:** `apps/web/src/components/setup/top-step-indicator.tsx` (Studio `@web-setup/top-step-indicator`)

---

## FB-V1107-006 — Wizard AgentPicker Single Column

**Problem:** `density=compact` still renders two-column grid on wizard Agent step, making cards cramped in portrait shell.

**User-visible outcome:** First-launch wizard shows one agent card per row.

### Acceptance

- [ ] `density="compact"` → `grid-cols-1` only (no `sm:grid-cols-2`).
- [ ] Settings `density="default"` unchanged (may keep `sm:grid-cols-2`).
- [ ] Studio wizard chrome + App wizard smoke.

**SSOT:** `apps/web/src/components/setup/agent-picker.tsx`

---

## FB-V1107-007 — Hide Wizard Custom Launch

**Problem:** Wizard Agent step still pitches custom-launch path, adding IA noise during first-launch.

**User-visible outcome:** Wizard focuses on picking an installed agent; Settings retains custom launch for power users.

### Acceptance

- [ ] Wizard path: `showCustomLaunch={false}`.
- [ ] Helper copy updated — no custom-launch pitch on wizard.
- [ ] Continue enabled when agent selected only (no custom-launch requirement).
- [ ] Settings keeps custom launch block.
- [ ] Studio wizard fixture + App wizard smoke.

**SSOT:** `apps/web/src/pages/setup-step-agent.tsx`, Studio `setup-wizard-chrome-fixtures.tsx`

---

## FB-V1107-008 — Workspace Path Field Reuse Settings

**Problem:** Wizard workspace step uses different label/CTA chrome than Settings, causing duplicate copy and inconsistent a11y.

**User-visible outcome:** Wizard and Settings show the same workspace field pattern.

### Acceptance

- [ ] Add `apps/web/src/components/setup/workspace-path-field.tsx` per Architecture contracts.
- [ ] Label **Workspace folder** via `WORKSPACE_PATH_FIELD_LABEL`; CTA **Change Folder…** via `WORKSPACE_PATH_CHANGE_ACTION`.
- [ ] Wizard `layout="wizard-stack"` — label above readonly Input; no in-row wizard label chrome.
- [ ] `label` ↔ `input` a11y association via shared `id`.
- [ ] Studio `WorkspaceBody` + App wizard + Settings smoke.

**SSOT:** `apps/web/src/components/setup/workspace-path-field.tsx` → `@web-setup/workspace-path-field`

### Voice & Content (locked)

| Element | Copy |
|---------|------|
| Field label | **Workspace folder** |
| CTA | **Change Folder…** |

---

## FB-V1107-009 — Outbound Install/Docs Links

**Problem:** Install and Docs links lack visible separation and consistent outbound icon treatment.

**User-visible outcome:** Each outbound link shows clear gap between Install and Docs; icon inside the anchor.

### Acceptance

- [ ] Lucide `ArrowUpRight` inside each outbound `<a>` (not CSS `::after`).
- [ ] Visible gap between Install and Docs links.
- [ ] Studio + App smoke.

**SSOT:** `apps/web/src/components/setup/agent-picker.tsx` (`OutboundLink`)

---

## FB-V1107-010 — Status Dots

**Problem:** Agent selection status dots may be subtle or missing after paint gaps.

**User-visible outcome:** Selected agent shows filled green dot; installed-but-unselected shows hollow green dot at top-right of identity row.

### Acceptance

- [ ] Selected: filled green dot.
- [ ] Unselected installed: hollow green dot.
- [ ] Position: top-right of identity row.
- [ ] Studio + App smoke.

**SSOT:** `apps/web/src/components/setup/agent-picker.tsx` (`StatusDot`)

---

## FB-V1107-011 — Uninstalled AgentCard Chrome

**Problem:** Uninstalled cards show redundant footer “Not installed” text; surface not muted enough.

**User-visible outcome:** Uninstalled agents show gray soft **Not installed** badge near title; card surface muted; no footer duplication.

### Acceptance

- [ ] Remove bottom “Not installed” text.
- [ ] Gray soft Badge near title.
- [ ] Muted card surface.
- [ ] Studio + App smoke.

**SSOT:** `apps/web/src/components/setup/agent-picker.tsx` (`AgentCard` / `AgentCardIdentity`)

---

## FB-V1107-012 — App Adopts Package Toast

**Problem:** App keeps duplicate `apps/web/src/lib/use-toast.tsx` (~40+ call sites) while package Toast exists (`R-V1106P0-001`).

**User-visible outcome:** Toast behavior unchanged for authors; implementation unified under `@42ch/nexus-ui`.

### Acceptance

- [ ] Thin re-export at `apps/web/src/lib/use-toast.tsx` from `@42ch/nexus-ui` (delete duplicate body).
- [ ] `ToastProvider` / `useToast` / `Toaster` resolve to package at runtime.
- [ ] Update `.mstar/iterations/v1.99/specs/component-promotion-boundary.md`: Toast `promote` + lucide runtime dep footnote (`R-V1106P0-002`).
- [ ] Tests green; Studio + App same package.

**SSOT:** `packages/nexus-ui/src/components/toast.tsx`; App `apps/web/src/lib/use-toast.tsx` (re-export shim)

---

## FB-V1107-013 — Sidebar/Shell Surfaces SSOT

**Problem:** Studio `AppShellFixture` is a stub — Creator/Orchestrator IA does not match App.

**User-visible outcome:** Contributors preview real shell chrome in Studio `/surfaces/shell`.

### Acceptance

- [ ] Extract `ShellSidebarChrome` to `layout/presentational/`; App `sidebar.tsx` delegates markup.
- [ ] Add `@web-layout/*` Studio alias; replace inline `AppShellFixture` stub in `surfaces.tsx`.
- [ ] Creator + Orchestrator nav IA match App (group labels, leaf selection bar, Settings footer utility).
- [ ] Studio `/surfaces/shell` light+dark smoke.

**SSOT:** `apps/web/src/components/layout/presentational/shell-sidebar-chrome.tsx`; Studio `@web-layout/shell-sidebar-chrome`

---

## FB-V1107-014 — FooterProfiles + Header/Health Fixtures

**Problem:** Footer profile switcher and header health indicator states lack Studio coverage.

**User-visible outcome:** Contributors preview profile count variants and daemon health states in Studio.

### Acceptance

- [ ] Extract `FooterProfilesChrome` + `DaemonHealthIndicatorChrome` under `layout/presentational/`.
- [ ] Props-driven FooterProfiles fixtures: 0 / 1 / N profiles.
- [ ] Props-driven health matrix: `unknown`, `connected` (local + remote variant), `offline`.
- [ ] Light+dark smoke.

**SSOT:** `apps/web/src/components/layout/presentational/{footer-profiles-chrome,daemon-health-indicator-chrome}.tsx`; Studio fixtures import `@web-layout/*`

---

## FB-V1107-015 — Settings Host Dedup + ConnectDaemonForm

**Problem:** `settings-host-fixtures.tsx` duplicates App presentational modules; ConnectDaemonForm lacks four-state matrix in Studio.

**User-visible outcome:** Settings Surfaces show real Connection form chrome; fixture file substantially smaller.

### Acceptance

- [ ] Add `@web-settings/*` alias; extract `connect-daemon-form-chrome.tsx` + `settings-setup-section-chrome.tsx`.
- [ ] Shrink `settings-host-fixtures.tsx` — import `@web-settings/*`, `@web-setup/workspace-path-field`, `@web-setup/agent-picker`.
- [ ] ConnectDaemonFormChrome matrix rows: `firstUse`, `reconnectMatch`, `fingerprintMismatch`, `loopbackOnly`.
- [ ] Real Connection form chrome in Studio; no IPC in fixtures.
- [ ] Fixture file substantially smaller than current.

**SSOT:** `apps/web/src/components/settings/presentational/*`; `apps/design-studio/src/fixtures/settings-host-fixtures.tsx`

---

## FB-V1107-016 — Domain Badges + Author Surfaces Backlog

**Problem:** Domain-specific badges (Status/Chapter/Finding/TaskKind) lack Studio matrices; author Surfaces domains have no promotion triggers documented.

**User-visible outcome:** Contributors preview domain badge variants; PM has clear triggers for next Surfaces pilots.

### Acceptance

- [ ] Studio `/components` matrices: Status, Chapter, Finding, TaskKind badges.
- [ ] `author-surfaces-backlog.md` indexed with owners, triggers, and suggested first fixtures.
- [ ] Does **not** block Must close on full Surfaces routes.

**SSOT:** Studio `/components`; `.mstar/iterations/v1.107/specs/author-surfaces-backlog.md`
