# Spec: V1.130 shell IA finish

**plan_id:** `2026-07-22-v1.131-p2-shell-ia-finish`  
**Status:** specify+clarify+plan locked (architect Seat 2)

## Problem

V1.130 plans were marked Done and merged (#165), but dogfood left incomplete shell/Settings IA. Tracker rows `DF-V1130-*` must **ship in V1.131**, not remain open-ended deferrals.

## Scope (binding — all Must)

| Tracker ID | Delivery |
|------------|----------|
| `DF-V1130-MODE-SWITCH-FOOTER` | 创作\|编排 switch lives on **功能区 footer**; retire top sidebar tabs as primary mode switch |
| `DF-V1130-SETTINGS-MODAL` | Settings modal **primary** (≥80vw×80vh); `/settings/*` opens modal section; demote full-page Settings |
| `DF-V1130-WORKSPACE-UNDER-ORCH` | Profiles → **工作区** under **编排 功能区 only** (not both tabs / not only global Settings) |
| `DF-V1130-COMPUTE-IN-SETTINGS` | Compute/Modules content inside Settings modal; remove Compute from 编排 only after modal section green |
| `DF-V1130-PROFILE-SSOT` | Hold Profile SSOT invariant (`~/.nexus42/creators/<id>/`); no regression in list/create heal-from-SSOT |

**Ownership split with P0:** P0 delivers Chronos titlebar + gear **entry** into `SettingsModalHost`. This plan completes modal **sections**, deep links, footer mode switch, 工作区 placement, and Compute rehome.

## Non-goals

- Execution-mode matrix (`DF-70`)
- Menu-bar daemon control (`DF-71`)
- Chronos titlebar chrome (P0) / logo gallery (P1)

## Acceptance (dogfood-testable)

- **AC-1:** No top 创作|编排 switch; the existing **功能区 footer** switch is the only primary mode control and is covered in both themes.
- **AC-2:** Gear / Settings entry opens the single **Settings modal** ≥80vw×80vh (desktop); ESC, backdrop, close button, and route-close all call the same dirty-aware `requestClose`.
- **AC-3:** `/settings` and `/settings/:section` resolve through one section registry and open over the last safe non-settings route; direct loads use `/works`, unknown sections use the default, and close restores the saved route. No full-page Settings shell is primary.
- **AC-4:** The Profile selector labeled 工作区 appears only under 编排 功能区. Global workspace-path/daemon configuration may remain a Settings section, but it does not duplicate the Profile selector.
- **AC-5:** Compute/Modules reuses the existing list/detail body inside the shared **Settings modal** with no nested Settings dialog; `/modules` is a compatibility entry and 编排 exposes no Compute item after the section is green.
- **AC-6:** Profile create/list still SSOT-backed (no regression).

## Surfaces

- `apps/web` shell, settings modal host, sidebar/footer, settings routes
- Studio `@web-layout/*` / settings fixtures as needed

## Architecture decision (locked)

### Single Settings host and registry

- Mount exactly one `SettingsModalHost` at the app-shell/router boundary. It owns the Radix `Dialog.Root`, modal sizing, focus trap/restore, scroll lock, section navigation, URL resolution, safe background location, and dirty-source registry.
- Define one typed registry (`SettingsSectionId` + ordered section descriptors) for `agent`, `workspace`, `appearance`, `modules`, and `advanced`. Descriptors provide id, i18n label key, icon, and content renderer. Aliases normalize `/settings`, `/settings/connection`, `/settings/setup`, and `/modules` without adding parallel route trees.
- Existing section modules remain content-only. Replace `SettingsShellLayout`’s nested `<Outlet>` ownership with a modal section frame driven by the registry. Do not place `SettingsModalHost` inside a section and do not let a section render another Settings dialog.
- `SettingsModalHost` exposes typed `openSettings(section, invoker?)`, `selectSection(section)`, `requestClose(reason)`, and `registerDirtySource(key, dirty)` behavior through one context/controller. The exact function names may vary, but ownership may not.

### Route/background contract

- Keep the last non-settings `Location` as the safe background. While the browser URL is `/settings/*`, render the normal route tree against that saved location and render the modal against the real Settings location.
- In-app Settings opens preserve pathname/search/hash of the prior non-settings route. Direct `/settings/*` loads synthesize `/works` as the safe background. Closing uses replace navigation to that saved route so Back does not reopen a phantom full-page Settings screen.
- Unknown Settings sections normalize to the registry default (`agent`). `/settings/connection` → `advanced#connection`, `/settings/setup` → `advanced#setup`, and `/modules` → `modules`.
- Fingerprint recovery remains able to force `advanced#connection`; it targets the same registry/host, not a bypass full page.

### Dirty and dismiss contract

- Every close vector calls `requestClose`: gear toggle, explicit close button, ESC, backdrop, browser navigation away, and section route close.
- Dirty section bodies register with the host. Clean close restores focus to the invoking element. Dirty close remains in the same Settings dialog and shows host-owned discard confirmation; section bodies do not own confirmation/modal chrome.
- Radix supplies focus trap and scroll lock. Tests must cover focus restore, ESC/backdrop clean close, dirty refusal/confirm, and route-driven close.

### 功能区 footer switch and 工作区

- `ShellSidebarChrome` already renders the 创作|编排 tablist on the **功能区 footer**. V1.131 treats that as the baseline: remove any stale top-switch code/tests and add light/dark Studio coverage rather than create a second switch.
- `FooterProfiles` remains the data/behavior wrapper for Profile SSOT operations, but `Sidebar` supplies it only when `activeTab === "orchestrator"`. Its author-visible section label is 工作区. The common mode switch remains visible in both modes.
- “工作区 under 编排 only” refers to the Profile selector. `SettingsWorkspaceSection` may retain path/folder configuration as global Settings content, provided labels distinguish configuration from the 编排 Profile selector.

### Compute rehome

- Reuse the existing `ModulesPage` query/detail implementation as a content body (extract/rename to `SettingsModulesSection` with a thin compatibility adapter if needed). Do not duplicate `useComputeModules`, `useComputeModule`, selection state, or error classification.
- Register that body as `modules` in the Settings registry first; prove list/detail/error/empty behavior inside modal dimensions; then retire the `/modules` product page in favor of a compatibility adapter and keep Compute absent from 编排.

### Profile SSOT guard

- This plan does not change creator persistence. Existing `NexusClient` list/create and active-creator coordination remain the only UI boundary; tests cover list/create and heal-from-SSOT behavior before and after the IA move.

## Validation

- Unit/integration: registry resolution and aliases; last-safe-background restore; unknown-section fallback; dirty close matrix; focus restore; 功能区 footer mode switch; 编排-only 工作区; Modules list/detail inside the Settings modal; Profile SSOT regression.
- Studio: one Settings modal shell with representative clean/dirty/module states and 功能区 footer in light + dark via allowed `@web-layout/*` / `@web-settings/*` extracts.
- Scoped gate: web typecheck/tests plus Design Studio tests/build. No schema/codegen or wire-contract changes.
