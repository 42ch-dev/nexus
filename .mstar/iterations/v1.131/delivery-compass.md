---
iteration_id: V1.131
start_date: 2026-07-22
status: locked
iteration_base_branch: main
spec_integration_branch: iteration/v1.131
target_branch: main
scale: L
direction_lock_mode: autonomous
plans:
  - 2026-07-22-v1.131-p0-chronos-titlebar
  - 2026-07-22-v1.131-p1-logo-gallery-lockup
  - 2026-07-22-v1.131-p2-shell-ia-finish
  - 2026-07-22-v1.131-p3-desktop-icons-residuals
---

# V1.131 Delivery Compass

## Scope

**Locked direction (autonomous):** Ship Chronos **titlebar-first chrome**, **BG-matched logo placement** / Studio gallery fixes, **all V1.130 dogfood IA gaps** (`DF-V1130-*`), desktop icon verify, and residual slate-clear — **in this iteration**, not as further deferrals.

**Rationale:** User dogfood after PR #167 requires VI chrome/logo fixes **and** insists related incomplete V1.130 work (tracker `DF-V1130-*`) must be **resolved here**. Scale **auto → L** (4 business plans, at cap).

### Priority of pain (user-value rank)

1. **P0 — Light-shell Chronos titlebar** (highest). Full-width ink bar; logo on ink; Settings gear entry into the Settings modal.
2. **P1 — Studio logo gallery + lockup.** Fill previews; BG-match plates; larger wordmark.
3. **P2 — V1.130 shell IA finish (binding).** Close `DF-V1130-MODE-SWITCH-FOOTER`, `SETTINGS-MODAL`, `WORKSPACE-UNDER-ORCH`, `COMPUTE-IN-SETTINGS`; hold `PROFILE-SSOT`. **Must not re-defer these to V1.132.**
4. **P3 — Desktop icons + residual archive.** Compose/generate + slate-clear after P0–P2 land codes.

### Tracker IDs resolved this iteration

| Tracker ID | Plan owner | Outcome required |
|------------|------------|------------------|
| `DF-V1130-SETTINGS-MODAL` | P0 (gear entry) + P2 (modal primary / deep links / sections) | **Ship** — move to shipped archive at close |
| `DF-V1130-MODE-SWITCH-FOOTER` | P2 | **Ship** |
| `DF-V1130-WORKSPACE-UNDER-ORCH` | P2 | **Ship** |
| `DF-V1130-COMPUTE-IN-SETTINGS` | P2 | **Ship** |
| `DF-V1130-PROFILE-SSOT` | P2 (hold / regression) | **Hold** invariant; no open gap left untested |
| `DF-V1131-CHRONOS-TITLEBAR` | P0 | **Ship** — no native-titlebar deferral |
| `DF-V1131-LOGO-GALLERY-LOCKUP` | P1 | **Ship** |
| `DF-V1131-DESKTOP-ICON` | P3 | **Ship** |

## Plans

| Wave | plan_id | Name | Status | blocked_by |
|------|---------|------|--------|------------|
| 1∥ | `2026-07-22-v1.131-p0-chronos-titlebar` | Chronos titlebar chrome | Todo | — |
| 1∥ | `2026-07-22-v1.131-p1-logo-gallery-lockup` | Logo gallery + lockup scale | Todo | — |
| 2 | `2026-07-22-v1.131-p2-shell-ia-finish` | V1.130 shell IA finish | Todo | P0 |
| 3 | `2026-07-22-v1.131-p3-desktop-icons-residuals` | Desktop icons + residual slate | Todo | P0, P1, P2 |

**Scale budget:** L → **4** business plans (cap). Harness process not counted.

**PM order:** Wave 1 = P0 ∥ P1 → Wave 2 = P2 → Wave 3 = P3.

## Milestones

| Milestone | Target date | Status |
|-----------|-------------|--------|
| Spec freeze (Review & Edit lock) | 2026-07-22 | pending |
| Wave 1 (titlebar ∥ gallery) | 2026-07-22 | pending |
| Wave 2 (shell IA finish) | 2026-07-22 | pending |
| Wave 3 (desktop + residuals) | 2026-07-22 | pending |
| Iteration close | 2026-07-22 | pending |

## Acceptance Criteria

### VI / Chronos chrome (P0, P1, P3)

- **AC-1 (P0).** Light shell: full-width deep-blue titlebar (`#0D2B3E`); brand mark on ink, not primary plate alone on light sidebar header.
- **AC-2 (P0).** Titlebar labels white (light) / cyan (dark).
- **AC-3 (P0).** Desktop: Tauri v2 macOS `titleBarStyle: "Overlay"` keeps native traffic lights while the web ink titlebar extends beneath native chrome; controls remain clickable/correctly positioned and only non-interactive titlebar regions are draggable. `decorations: false` is not an accepted delivery path.
- **AC-4 (P1).** Studio `primary` / `whiteBg` cards fill preview width on matching BG (ink / white-paper, not mid-gray).
- **AC-5 (P1).** Dark hero wordmark cap-height ≥ 60% of mark height.
- **AC-6 (P0+P1).** Plate logos only on matching surfaces product-wide.
- **AC-7 (P3).** Desktop `icons:compose` / `icons:generate` Chronos plate; Dock/README cache notes.

### V1.130 IA (P2 — must ship)

- **AC-8.** 创作\|编排 on **功能区 footer** is the only primary mode switch; top sidebar tabs are absent and light/dark fixtures cover the footer state.
- **AC-9.** Settings modal primary = one route-aware `SettingsModalHost` ≥80vw×80vh; `/settings/*` resolves through its section registry over the last safe non-settings route (direct loads use `/works`), and every close vector uses the host dirty guard.
- **AC-10.** The Profile selector labeled **工作区** appears only under **编排 功能区**; global path/connection configuration may remain in Settings but does not duplicate the profile selector.
- **AC-11.** Compute/Modules reuses the existing list/detail body as a registered Settings section, with no section-owned or nested Settings dialog; `/modules` becomes a compatibility entry and 编排 exposes no Compute item after the modal section is green.
- **AC-12.** Profile SSOT list/create still holds (regression guard).

### Residuals (P3)

- **AC-13.** Targeted R-VI-* / R-V1130* either archived (ND-A2) or re-deferred with owner+trigger; `tech_debt_summary` refreshed. **DF-V1130-* feature rows must not remain open** after close unless PROFILE-SSOT is explicitly held as ongoing invariant (no incomplete UI).

## Non-Goals

- Runtime multi-theme switcher (Umbra/Aurora)
- Full Agent Chat product depth
- `DF-70` execution-mode matrix
- `DF-71` menu-bar daemon control
- Marketing/store icon packages beyond app icon pipeline
- Re-deferring `DF-V1130-*` UI gaps to V1.132 (**forbidden**)

## Roadmap Position

- **Current (V1.131):** Chronos titlebar + logo gallery + **finish DF-V1130 shell/Settings IA** + desktop icons + residual slate.
- **Next:** Deeper creator Chat / orchestration polish; DF-70/71 when capacity.
- **Prior:** V1.130 partial ship (#165) + VI logo upgrade (#167).

## Delivery Branch Policy

| Field | Value |
|-------|-------|
| `iteration_base_branch` | `main` |
| `spec_integration_branch` | `iteration/v1.131` |
| `target_branch` | `main` |

## Architecture locks (architect Seat 2)

1. **Tauri v2 titlebar:** keep native window decorations/traffic lights and set the macOS window to `titleBarStyle: "Overlay"` with `hiddenTitle: true`; use `trafficLightPosition` only to align the native controls with the Chronos bar. The web titlebar owns the deep-blue paint and marks only its empty background with `data-tauri-drag-region`. Interactive logo, gear, theme, health, and navigation controls are never drag regions. The desktop smoke gate must prove close/minimize/zoom, double-click behavior, drag, and focus; failure blocks P0 rather than creating another deferred titlebar.
2. **Shell boundary:** `RootLayout` owns a full-width titlebar above the sidebar/content row. Props-driven `ChronosTitlebarChrome` and adjusted sidebar chrome live under `apps/web/src/components/layout/presentational/**` and are mirrored in Studio through `@web-layout/*`; routing, Settings open behavior, theme, and desktop detection stay in app wrappers.
3. **Settings boundary:** exactly one app-level `SettingsModalHost` owns the Radix dialog, section registry, URL resolver, last safe background route, focus restore, and dirty registrations. Section bodies are content-only. `/settings`, known aliases, unknown sections, and `/modules` normalize through one resolver; no full-page Settings shell remains primary.
4. **IA finish:** the existing **功能区 footer** mode switch is hardened rather than reimplemented. `FooterProfiles` becomes the 编排-only **工作区** block. Compute reuses the existing `ModulesPage` query/detail behavior through a Settings modal section adapter; no second query model and no nested dialog are introduced.
5. **Desktop icon pipeline:** `apps/desktop/src-tauri/icons/source/compose-app-icon.mjs` remains the sole composer from canonical `logo-primary.svg`. Run filtered workspace scripts, hash committed source/preview before and after, generate platform files from `source-1024.png`, and verify the built Dock icon after documented cache invalidation.

## Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|------------|--------|------------|
| Tauri overlay/traffic-light alignment | Med | High | Preserve native decorations; use Overlay + explicit safe inset/traffic-light position; mandatory macOS interaction smoke blocks P0 on failure |
| P2 scope larger than expected | Med | Med | L cap already includes P2; do not descope DF-V1130 UI |
| Modal route/dirty-guard regressions | Med | High | One host + one section registry + one URL resolver; section bodies register dirty state and never own dialog chrome |
| Settings route background loss | Med | Med | Keep last non-settings location in the host/router controller; direct `/settings/*` loads use `/works`; close restores the saved safe route |

## Iteration package

| Path | Purpose |
|------|---------|
| `specs/chronos-titlebar-chrome.md` | Titlebar + logo on ink |
| `specs/logo-gallery-lockup.md` | Studio gallery + wordmark |
| `specs/shell-ia-finish.md` | DF-V1130 IA completion |
| `specs/desktop-icons-residuals.md` | Desktop icons + residual archive |

## Autonomous direction lock record

| Field | Value |
|-------|-------|
| Mode | `autonomous` |
| Scale | `auto` → **L** (4 plans) |
| Locked | Chronos VI dogfood **+ resolve all DF-V1130-* UI gaps in-iteration** |
| User clarification | Related tracker problems must be **solved** this iteration, not merely listed |

## Prepare handoff

**Status:** `locked` (PM — Phase 1 Review & Edit complete 2026-07-22)

**Phase 1 lock:** product-manager → architect → writing-specialist complete. DF-V1130-* and VI dogfood items are **must-ship** this iteration (4 plans). Next: Phase 2 Autonomous Execute on `iteration/v1.131`.

Architect Seat 2 locked: Tauri Overlay **Chronos titlebar** with native traffic lights; one route-aware **Settings modal** section registry/dirty guard; **功能区 footer**-only mode switch with 编排-only **工作区**; Compute reuse without nested modal; deterministic desktop icon compose/generate/cache-smoke pipeline.

## Seat 3 — writing-specialist note

Corpus pass complete. Terminology locks applied across compass, plans, and specs:

| Term | Canonical usage |
|------|-----------------|
| Shell layout | 左功能区 + 右内容区 (dual-pane) |
| Mode switch | 创作\|编排 on **功能区 footer** (not sidebar top tabs) |
| Profile/workspace IA | **工作区** under **编排 功能区 only** (not both tabs / not global Settings duplicate) |
| Global config surface | **Settings modal** (≥80vw × 80vh desktop; one `SettingsModalHost`; shared `requestClose` + dirty guard) |
| VI chrome | **Chronos titlebar** — full-width deep-blue ink bar (`#0D2B3E`); logo on ink; Tauri v2 macOS Overlay with native traffic lights |
| Create World / Chat | Out of scope (V1.131 non-goals) |

**Residual copy notes (non-blocking):**

- `SettingsModalHost` names the implementation; author-facing copy uses **Settings modal**.
- "Orchestration" / `orchestrator` remain acceptable in technical prose (plan_ids, crate names, `activeTab`); author-facing copy uses **编排**.
- P0 owns titlebar gear → `SettingsModalHost`; P2 completes modal sections/deep links — do not conflate wire ownership.

No `.mstar/knowledge/` additions (tracker row hygiene only where architect lock conflicted). No architect lock weakening.
