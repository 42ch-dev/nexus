# UI Shell SSOT + AgentPicker + Canvas Surfaces — Primary Spec (V1.108 P1)

**Status:** Draft — product-complete (§5.1 PM); architecture locked (§5.2 architect); writing-complete (§5.3)  
**Tier:** Must (P1)  
**Plan:** `2026-07-10-v1.108-ui-shell-ssot`  
**Compass:** `../v1.108-delivery-compass.md`  
**Invariant:** Studio-first (see `../guides/studio-first-invariant.md`)

## Product outcome

Authors and contributors see **one shell chrome story** across App and Design Studio: Settings fixtures match `ShellSidebarChrome`, AgentPicker selection semantics are trustworthy, Work detail exposes canvas entry points, and World KB copy is honest.

**User-visible win:** No stale underline-tab Settings fixture; unselected agents do not look "valid"; Work detail offers **Open Outline** and **Open Strategy**; Studio previews canvas chrome at `/surfaces/canvas`; World KB empty state does not promise a missing command palette.

## Problem

1. Settings Studio host fixture still shows **stale shell** (underline Creator/Orchestrator tabs, plain text nav, profile name under avatars) while App/`ShellSidebarChrome` uses segmented pill + sectioned icon nav + icon-only profiles (V1.107 extract).
2. AgentPicker: unselected hollow **green** StatusDot implies validity (V1.107 shipped hollow green — V1.108 corrects to gray); hover only paints inner button; custom launch has no Verify path.
3. Work detail only links **Open World KB** — Outline and Strategy canvases are hard to discover from Work context.
4. Design Studio has **no** `/surfaces/canvas` fixtures (V1.107 backlog promote trigger when Canvas IA starts).
5. World KB header copy references a command palette that does not exist in the product UI.

## Goals

1. Land FB-UI-001..010 under studio-first (Studio accept light+dark before App parity claims for visual FBs).
2. Settings host fixtures consume `ShellSidebarChrome` / `@web-layout/*` SSOT — delete or shrink stale dual track.
3. AgentPicker StatusDot, whole-card hover, and custom-launch Verify polish.
4. Work detail canvas CTAs for Outline and Strategy (`primary_preset_id` gated).
5. Studio `/surfaces/canvas` first fixtures (shell + context menu chrome).
6. World KB empty/header copy honest about real authoring paths.

## Non-goals

- Implementing full canvas command palette or shell dirty-route guard beyond honest copy.
- Sidebar nesting for Outline/KB; Worlds list page; DF-70 BYOK.
- Package-promoting Dialog/Tabs/Table to `@42ch/nexus-ui`.
- Outline spatial graph behavior — **P0** owns App Outline; P1 fixtures are presentational preview.

## Studio-first rule

All visual FBs (001..008, 004 canvas fixtures): **Studio accept** (light+dark, focus-visible, Voice & Content) before App wiring claims. FB-UI-009..010 are App copy/IA — still verify in App smoke.

## Voice & Content (locked)

Follow DESIGN.md §Voice & Content: **Title Case** for headings, labels, and CTAs; **sentence case** for helper text; **Verb + Noun** for actions.

| Surface | Element | Copy (exact) |
|---------|---------|--------------|
| Work detail | Outline CTA | **Open Outline** |
| Work detail | Strategy CTA | **Open Strategy** (visible only when `primary_preset_id` is set) |
| Work detail | World KB CTA | **Open World KB** (existing — unchanged) |
| World KB header | Empty helper | *No entries to show yet. Refresh after adding world content, or continue from a linked Work.* |
| World KB header | Non-empty helper | *Browse entities and promotion candidates. Edits are guarded by per-row version checks.* (existing — unchanged) |
| Custom launch | Field label | **Use custom launch command** (existing) |
| Custom launch | Verify action | **Verify Agent** |
| Custom launch | Success helper | *Agent responded successfully.* |
| Custom launch | Failure helper | *Could not reach this agent. Check the command and try again.* |
| Profiles section | Section label | **PROFILES** (existing) |

**Forbidden in World KB empty copy:** "command palette", `kb adopt/snapshot`, or any UI path that does not exist in App. See `../guides/studio-first-invariant.md` § Voice & Content.

## Wire

**Default:** `wire_contracts_changed: false`.

| Feature | Mechanism | Wire risk |
|---------|-----------|-----------|
| Shell / AgentPicker visual FBs | Presentational components + Studio fixtures | None |
| Work CTAs | React Router links | None |
| World KB copy | String change in header | None |
| **Verify Agent** | App host calls `NexusClient.scanAgents()` (`POST /v1/daemon/agent-host/scan`) with custom launch command in request body | **Low** — reuse existing scan endpoint; `ScanRequest` may accept optional `custom_launch_command` if probe matching requires it (additive only) |

**Escape hatch (Verify only):** If scan cannot validate a custom command without a new request field or route, add **additive** schema field + handler support. Document residual with evidence before expanding scope.

---

## Architecture Locks (§5.2)

### Shell SSOT

| Layer | SSOT path | Consumption |
|-------|-----------|-------------|
| Sidebar chrome | `apps/web/src/components/layout/presentational/shell-sidebar-chrome.tsx` | App `sidebar.tsx` wrapper; Studio via `@web-layout/shell-sidebar-chrome` |
| Profile row | `layout/presentational/footer-profiles-chrome.tsx` | App `footer-profiles.tsx`; Studio fixtures pass props |
| Settings fixture | `apps/design-studio/src/fixtures/settings-host-fixtures.tsx` | **Deprecate** `SettingsShellChromeFixture` inline dual track (underline tabs, plain nav, profile names) — replace with `ShellSidebarChrome` + `FooterProfilesChrome` import pattern from `surfaces.tsx` `ShellSidebarFixture` |

Settings host fixture (`settings-host-fixture-shell`) **must** shrink — same component tree as App shell, not parallel markup.

### AgentPicker ownership

| Piece | Owner | Path |
|-------|-------|------|
| Presentational grid | P1 | `apps/web/src/components/setup/agent-picker.tsx` |
| `StatusDot` | P1 | same file — unselected installed: hollow **gray** (`border-gray-500`); selected: filled **green** |
| `AgentCard` hover | P1 | Outer `div` receives hover bg; inner select `button` keeps focus ring |
| `CustomLaunchField` + **Verify Agent** | P1 presentational; App hosts wire | Add `onVerify?: () => void`, `verifyStatus?: 'idle' \| 'loading' \| 'success' \| 'error'` props — **no** `NexusClient` inside picker |
| Verify probe | App hosts | `settings-agent-section.tsx`, `setup-step-agent.tsx` → `scanAgents({ custom_launch_command })` or match scan result by command string |
| Studio matrix | P1 | `agent-picker-fixtures.tsx` — static verify states; no IPC |

### Work CTA routing contracts

| CTA | Route | Visibility |
|-----|-------|------------|
| **Open Outline** | `/works/:workId/outline` | Always (Work exists) |
| **Open Strategy** | `/strategies/:presetId` using `work.primary_preset_id` | Visible when `primary_preset_id` set; hidden or disabled with honest helper when absent |
| **Open World KB** | `/worlds/:worldId/kb` | When `world_id` present (existing — unchanged) |

Implement in `apps/web/src/pages/work-detail-page.tsx` action row beside existing **Open World KB** button pattern (`Button asChild` + `Link`).

### Studio `/surfaces/canvas` boundaries

| Rule | Lock |
|------|------|
| Fixture location | `apps/design-studio/src/fixtures/canvas-surfaces-fixtures.tsx` (**new**) |
| Route registration | `apps/design-studio/src/pages/surfaces.tsx` — add `Canvas` to `SURFACES_SECTIONS` + child route `/surfaces/canvas` |
| Content | Presentational canvas shell chrome + context-menu chrome matrices (light+dark) |
| Forbidden | `NexusClient`, `@42ch/nexus-contracts`, daemon hooks, live outline data |
| P0 coordination | Stub outline node/edge chrome until P0 lands; swap to imported presentational extracts when available — **no** blocking dependency on P0 merge for shell/context-menu fixtures |

### File-disjoint guidance (parallel with P0)

**P1 owns (write):**

- `apps/web/src/components/layout/presentational/**` (extend only if props gap)
- `apps/web/src/components/setup/agent-picker.tsx` (+ tests)
- `apps/web/src/pages/work-detail-page.tsx`
- `apps/web/src/components/canvas/world-kb/world-kb-canvas-header.tsx` (copy only)
- `apps/web/src/pages/settings/settings-agent-section.tsx`, `setup-step-agent.tsx` (Verify wiring)
- `apps/design-studio/src/**` (fixtures, surfaces routes)

**P1 must not touch (P0 blast radius):**

- `apps/web/src/components/canvas/outline-canvas/**`
- `outline-canvas.tsx`

**Shared read-only:** `canvas-shell.tsx` — P1 fixtures may **mirror** chrome visually; do not edit for P1 unless PM coordinates.

---

## FB-UI-001 — Icon-Only Profiles

**Problem:** Settings fixture shows profile display name under avatars; App `FooterProfilesChrome` is icon +「+」only.

**User-visible outcome:** Profile switcher shows avatars and a「+」add control only — no name text under icons.

### Acceptance

- [ ] Settings host fixture profiles row: avatar icons +「+」only.
- [ ] Section label **PROFILES** retained.
- [ ] Matches App `FooterProfilesChrome` / shell SSOT in light+dark Studio smoke.
- [ ] App parity where profiles render in Settings context.

**SSOT:** `ShellSidebarChrome` / `FooterProfilesChrome`; Settings host fixtures

---

## FB-UI-002 — Segmented Pill Creator/Orchestrator Tabs

**Problem:** Settings fixture uses underline text tabs; App uses segmented pill control.

**User-visible outcome:** Creator and Orchestrator mode switch is a segmented pill everywhere shell chrome appears.

### Acceptance

- [ ] Creator/Orchestrator control renders as segmented pill (not underline tabs) in Settings fixture.
- [ ] Active tab state visually distinct in light+dark.
- [ ] Matches App `ShellSidebarChrome` pill pattern.

**SSOT:** `shell-sidebar-chrome.tsx`; Settings host fixtures

---

## FB-UI-003 — Sectioned Icon Navigation

**Problem:** Settings fixture uses plain text nav; App uses section headers + icon+label rows + active rail.

**User-visible outcome:** Settings and shell previews show grouped nav with section headers, icons, labels, and active selection rail.

### Acceptance

- [ ] Nav groups render section headers (Creator/Orchestrator IA).
- [ ] Leaf items show icon + label; active item shows selection rail.
- [ ] Studio `/surfaces/shell` and Settings host fixture match App IA.

**SSOT:** `ShellSidebarChrome`; `settings-host-fixtures.tsx`

---

## FB-UI-004 — Studio `/surfaces/canvas` Fixtures

**Problem:** Contributors cannot preview canvas shell/context-menu chrome in Design Studio.

**User-visible outcome:** Design Studio exposes a Canvas Surfaces section with shell + context menu chrome fixtures (light+dark).

### Acceptance

- [ ] `surfaces.tsx` includes `/surfaces/canvas` route or section entry.
- [ ] Fixtures show canvas shell chrome and context-menu chrome (presentational; no daemon data).
- [ ] Light+dark matrices accepted in Studio before claiming complete.
- [ ] Aligns with P0 outline node chrome when available; stub presentational acceptable until P0 lands.

**SSOT:** `apps/design-studio/src/pages/surfaces.tsx`; new canvas fixture module(s)

---

## FB-UI-005 — Settings Host Fixture Consumes Shell SSOT

**Problem:** `settings-host-fixture-shell` duplicates stale markup instead of importing `ShellSidebarChrome`.

**User-visible outcome:** Settings Surfaces shell preview is the same component tree as App shell chrome — not a parallel stub.

### Acceptance

- [ ] `settings-host-fixtures.tsx` imports `ShellSidebarChrome` via `@web-layout/*` (or thin wrapper).
- [ ] Stale underline/plain-nav dual track removed or reduced to props-only wrapper.
- [ ] Fixture file size shrinks vs pre-V1.108 duplicate markup.

**SSOT:** `settings-host-fixtures.tsx`; `@web-layout/shell-sidebar-chrome`

---

## FB-UI-006 — StatusDot Selection Semantics

**Problem:** Unselected installed agents show hollow **green** dot (V1.107), implying validity before selection.

**User-visible outcome:** Only the **selected** agent shows a filled green dot; unselected agents show hollow **gray** dot.

### Acceptance

- [ ] Selected agent: filled green StatusDot.
- [ ] Unselected installed agent: hollow gray dot (not green).
- [ ] Position unchanged: top-right of identity row.
- [ ] Studio matrix accept then App AgentPicker smoke (light+dark).
- [ ] **Supersedes** V1.107 FB-V1107-010 hollow-green-unselected semantics for AgentPicker.

**SSOT:** `apps/web/src/components/setup/agent-picker.tsx` (`StatusDot`)

---

## FB-UI-007 — Whole-Card Agent Hover

**Problem:** Hover background applies only to inner button — card affordance feels broken.

**User-visible outcome:** Hovering anywhere on an AgentCard paints the entire card background.

### Acceptance

- [ ] `AgentCard` outer surface receives hover background (not inner button only).
- [ ] Focus-visible ring still meets a11y floor on keyboard path.
- [ ] Studio + App smoke for default and compact densities.

**SSOT:** `agent-picker.tsx` (`AgentCard`)

---

## FB-UI-008 — Custom Launch Verify

**Problem:** Custom launch field has no way to test the command before continuing.

**User-visible outcome:** Authors click **Verify Agent** and see inline success/failure feedback without leaving Setup.

### Acceptance

- [ ] **Verify Agent** button adjacent to custom launch input.
- [ ] Success shows helper *Agent responded successfully.*; failure shows *Could not reach this agent. Check the command and try again.*
- [ ] Probe reuses existing agent scan / version check where possible.
- [ ] Studio matrix documents idle/loading/success/failure states.
- [ ] Additive wire only if probe blocked — otherwise residual with evidence.

**SSOT:** `CustomLaunchField` in `agent-picker.tsx`; daemon agent-host scan client

---

## FB-UI-009 — Work Detail Canvas CTAs

**Problem:** Work detail exposes World KB but not Outline or Strategy — authors must guess routes.

**User-visible outcome:** Work detail action row includes canvas entry points for Outline and Strategy (when preset bound).

### Acceptance

- [ ] **Open Outline** link/button navigates to `/works/:workId/outline`.
- [ ] **Open Strategy** link/button navigates to Strategy canvas for the Work when `primary_preset_id` is present; hidden or disabled with honest helper when absent.
- [ ] **Open World KB** retained when `world_id` present (existing behavior).
- [ ] CTAs use Verb + Noun Title Case; icons optional but consistent with **Open World KB** pattern.

**SSOT:** `apps/web/src/pages/work-detail-page.tsx`

---

## FB-UI-010 — World KB Empty-State Honesty

**Problem:** Empty World KB copy promises `command palette (kb adopt/snapshot)` — no such UI exists.

**User-visible outcome:** Empty World KB describes real next steps without fake palette promises.

### Acceptance

- [ ] Empty helper copy: *No entries to show yet. Refresh after adding world content, or continue from a linked Work.*
- [ ] No mention of "command palette", `kb adopt`, or `snapshot` in author-visible empty copy.
- [ ] Non-empty helper locked (§5.3 writing-complete) — unchanged from existing copy.
- [ ] App smoke on empty and non-empty states.

**SSOT:** `apps/web/src/components/canvas/world-kb/world-kb-canvas-header.tsx`
