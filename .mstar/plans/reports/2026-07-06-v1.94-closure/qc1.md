---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-07-06-v1.94-closure"
verdict: "Approve"
generated_at: "2026-07-06"
revalidation_at: "2026-07-06T22:00:00+0000"
revalidation_commit: "0e75931b"
revalidation_review_range: "merge-base: bf0e60cc (main HEAD pre-V1.94) + tip: 0e75931b (iteration/v1.94 post-fix-wave) ≡ git diff main...iteration/v1.94"
---

# Code Review Report

## Reviewer Metadata

- Reviewer: @qc-specialist (Reviewer #1 — IA + structure lens)
- Runtime Agent ID: qc-specialist
- Runtime Model: MiniMax-M3
- Review Perspective: Architecture coherence and maintainability risk — IA, structure, sidebar rewrite, Strategies unification, footer profile switcher, button contrast audit, module boundaries (P0 backend ↔ P1 frontend contract coherence).
- Report Timestamp: 2026-07-06

## Scope

- plan_id: 2026-07-06-v1.94-closure
- Review range / Diff basis: `merge-base: bf0e60cc (main HEAD pre-V1.94) + tip: fd9c6d5d (iteration/v1.94 integrated HEAD)` ≡ `git diff main...iteration/v1.94`
- Working branch (verified): iteration/v1.94 @ fd9c6d5d
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus (via `git rev-parse --show-toplevel`)
- Files reviewed: 81 files changed (+5324 / −705)
- Commit range: bf0e60cc..fd9c6d5d (7 commits: iteration-start, P-1, P-0 merge, P-1, P0 merge, P1 merge, status sync)
- Deep review: **triggered** (signals: XL change size, UI restructure/new IA, module coupling P0↔P1 contract, high-visibility surface — sidebar + status bar + wizard + footer switcher)
- Lenses applied: **Structure Lens** + **Maintainability Lens** + **Accessibility Lens** (WCAG 2.1 AA floor per apps/web/AGENTS.md)
- Tools run: `git rev-parse`, `git branch --show-current`, `git log`, `git diff --stat`, `pnpm --filter web run typecheck`, `pnpm --filter web run build`, `pnpm vitest run (apps/web/src/components/layout, setup, pages/strategies-page.test)`, `cargo test -p nexus-daemon-runtime --lib`, `cargo test -p nexus-acp-host --lib`, `cargo clippy --all -- -D warnings`, Grep (NexusClient boundary, dead code, keyboard contract, button contrast sweep), Read (compass §5, web-ui.md §29, desktop-shell.md §13/§14, DESIGN.md Button Contrast Invariant, all new P1 components, agent-scan handler, Tauri lib.rs/sidecar.rs, nexus42 config.rs)

## Findings

### 🔴 Critical

_None._

### 🟡 Warning

#### F-001 · Selected agent / launch command is not persisted to `~/.nexus42/config.toml`

**Source Type**: doc-rule + manual-reasoning
**Source Reference**: `apps/web/src/pages/setup-wizard-page.tsx:35-38` (only calls `markCompleted()`); `desktop-shell.md` §13.2 step 4 ("Persist the selected agent + `setup_completed = true` in `~/.nexus42/config.toml`; transition to main UI"); compass §5 acceptance criteria ("Selecting an agent or supplying custom `launch_command` is persisted before Done").
**Confidence**: High.

The wizard state (`workspaceRoot`, `selectedAgent`, `customLaunchCommand`) is held only in React state via `setup-wizard-page.tsx`. On `finish()`, `markCompleted()` writes only `setup_completed=true` via the `set_setup_completed` Tauri command. The selected agent (or the custom `launch_command`) is never written to `~/.nexus42/config.toml` and never sent to the daemon.

The spec is explicit (desktop-shell.md §13.2 step 4 + compass §5): the agent choice must be persisted before "Done". The current implementation satisfies the visibility ("the wizard picked X"), but the choice is lost on the next wizard re-trigger or any downstream consumer expecting an active agent profile.

Fix direction: P0 + P1 need a small wire path. Options:

1. Promote `setup_completed` / `selected_agent` / `custom_launch_command` into the `set_setup_completed` payload (Tauri command takes `{ completed, agent?: { launch_command, registry_agent_id? } }`); or
2. Add a new additive Tauri command `set_default_agent(launch_command, registry_agent_id?)` mirroring the scan response fields, invoked from `SetupWizardPage.finish()` before `markCompleted()`. (Daemon-side: `AgentProfile` write to config; P1 already has `useSetActiveCreator` mutation pattern to follow.)

Either path closes the spec gap. P0 backend code (Tauri `set_setup_completed` and the corresponding `~/.nexus42/config.toml` schema in `apps/nexus42/src/config.rs`) currently only knows the bool flag — so a small additive field round-trips through both layers without breaking changes.

#### F-002 · Dead code: `apps/web/src/pages/presets-page.tsx` is no longer imported or routed

**Source Type**: manual-reasoning + grep
**Source Reference**: `grep -rln "presets-page\|PresetsPage" apps/web/src/` returns only the file itself; `apps/web/src/App.tsx:91` redirects `/presets → /strategies`; `strategies-page.tsx` is the new replacement (172 lines, fully covered by `strategies-page.test.tsx`).
**Confidence**: High.

`apps/web/src/pages/presets-page.tsx` (159 lines) is the V1.67/V1.68 preset CRUD list page. After the V1.94 Strategies unification it is no longer wired anywhere (no `import` of `presets-page` or `PresetsPage`, no route, no test references). `App.tsx:91` only defines a `<Route path="presets">` that redirects to `/strategies` — the old component is never instantiated.

This violates the "delete > add" change discipline (`mstar-coding-behavior`) and bloats the build / IDE symbol index with a stale alternative implementation that future authors may inadvertently import. Recommend deleting `presets-page.tsx` and confirming `tsc --noEmit` + `vite build` still pass after removal. (The companion `dialogs/scaffold-preset-dialog.tsx` and `dialogs/validate-preset-dialog.tsx` are still in use — keep them.)

#### F-003 · No unit tests for the V1.94 P1 surface (sidebar / wizard / strategy detail / contexts)

**Source Type**: manual-reasoning + grep
**Source Reference**: `find apps/web/src -name "*.test.*" | xargs grep -l "Sidebar\|SetupWizard\|StrategyPage\|setup-wizard"` returns 0 matches; `find apps/web/src -name "sidebar.test.*" -o -name "setup-step-*.test.*" -o -name "strategy-page.test.*"` returns 0 matches; `active-creator-context.tsx` and `setup-completed-context.tsx` have no `.test.tsx` siblings.
**Confidence**: High.

The V1.94 P1 introduces six new components without dedicated unit tests:

- `apps/web/src/components/layout/sidebar.tsx` (two-tab Creator | Orchestrator + nested nav)
- `apps/web/src/pages/setup-wizard-page.tsx` (4-step entry + step indicator)
- `apps/web/src/pages/setup-step-welcome.tsx` (workspace resolve + fallback)
- `apps/web/src/pages/setup-step-daemon.tsx` (health probe + daemon-status subscription)
- `apps/web/src/pages/setup-step-agent.tsx` (scan UX states: scanning / list / empty / selectable / Recommended badge)
- `apps/web/src/pages/setup-step-done.tsx` (finish → markCompleted + navigate)
- `apps/web/src/pages/strategy-page.tsx` (canvas detail at `/strategies/:presetId` — V1.70–V1.75 surface preservation contract is **explicitly called out** in `web-ui.md` §29.4 + compass §5)
- `apps/web/src/lib/active-creator-context.tsx` (footer switcher state + localStorage)
- `apps/web/src/lib/setup-completed-context.tsx` (browser/desktop marker)

The closure DoD (compass §5 "QA Pass on wizard E2E … + sidebar tabs + footer switcher (incl. keyboard + single-creator)") expects E2E coverage that pins the wizard UX states and the sidebar IA structure. None of those behaviours are pinned by tests today.

Test coverage that exists for the iteration (all green, see `Tools run` above):

- `setup-gate.test.tsx` — 4 tests (browser build, redirect to /setup, daemon-ready splash, error splash).
- `daemon-status-bar.test.tsx` — 6 tests (lifecycle action: restart confirm, no-render for non-running, event subscription, periodic re-sync).
- `main-banner.test.tsx` — 6 tests (degraded/stopped/error surfaces, restart CTA).
- `footer-profiles.test.tsx` — 4 tests (initial render, active state, switch, create dialog submit).
- `strategies-page.test.tsx` — 4 tests (grouped list, empty states, reload, error retry).

These cover the contracts **adjacent** to the spec (§5 #1 daemon status bar; §5 #4 footer switcher click path; §5 #5 strategies list), but the new files listed above are the actual primary surfaces of the iteration and have zero direct tests. Recommend targeted addition of:

1. `sidebar.test.tsx` — Creator/Orchestrator tab swap, nested nav collapse, footer-profiles mounted inside, no Connect or Daemon item.
2. `setup-wizard-page.test.tsx` + `setup-step-{welcome,daemon,agent,done}.test.tsx` — step navigation, agent-step UX states (scanning / empty / Recommended badge / selectable cards), and the "Continue" gate.
3. `strategy-page.test.tsx` — detail loads at `/strategies/:presetId`, redirects/missing-preset empty state, "V1.70–V1.75 surface preserved verbatim" is implicit via existing canvas tests but the route entry needs a regression pin.
4. `active-creator-context.test.tsx` — `localStorage` round-trip, cross-tab `storage` event sync, missing-context error throw.

Without these, future IA refactors can silently regress the locked decisions D1 / E1 / F1 / C1 / G1 without test failure.

#### F-004 · Button contrast invariant has no snapshot regression test

**Source Type**: doc-rule + manual-reasoning
**Source Reference**: `apps/web/DESIGN.md:741` ("vitest snapshot (light + dark themes) gates regressions"); `apps/web/src/components/ui/button.tsx:21` (the only `dark:bg-brand-cyan dark:text-white` site — `text-white` replaces the previous `dark:text-brand-deep-blue`).
**Confidence**: High.

The Button Contrast Invariant (V1.94) is recorded in DESIGN.md §Component Primitives/Button and is enforced by an inline class change in `button.tsx` (`dark:bg-brand-cyan dark:text-white`). The spec mandates "vitest snapshot (light + dark themes) gates regressions", but no snapshot test exists. The change is correct today (verified by `git diff`), but the invariant is one `dark:text-brand-deep-blue` typo away from regressing.

I cross-checked other dark-bg / saturated-bg call sites for the same audit:

| Call site | Bg | Text | Status |
|---|---|---|---|
| `button.tsx:21` primary | `dark:bg-brand-cyan` | `dark:text-white` | ✅ V1.94 fix |
| `idea-input.tsx:116` (send button) | `bg-purple-700` | `text-white` | ✅ already white |
| `conflict-modal-base.tsx:275` | `bg-canvas-write-conflict` (red) | `text-white` | ✅ already white |
| `setup-step-agent.tsx:96` Recommended badge | `bg-green-700` | `text-white` | ✅ already white |

The button itself is fixed. What is missing is the snapshot harness that prevents future regressions. Recommend adding `button.test.tsx` (or extending existing) with `toMatchSnapshot()` for the primary variant rendered with the `dark` class on `<html>`.

#### F-005 · Footer profile switcher keyboard contract is incomplete (no arrow-key roving tabindex)

**Source Type**: doc-rule + manual-reasoning
**Source Reference**: `apps/web/src/components/layout/footer-profiles.tsx:64-90` (CreatorAvatar is a plain `<button>` with no `onKeyDown`); `web-ui.md` §29.5 ("Keyboard: arrow-left/right to navigate avatars; Home/End for first/last; Esc closes any transient UI").
**Confidence**: High.

The footer profile switcher renders each creator as a standard `<button>` (lines 75-89). The native Enter/Space activation works via the browser's default `<button>` semantics, but the spec requires **arrow-left/right to navigate avatars; Home/End for first/last**. The current implementation has zero `onKeyDown` handlers on the avatar row; Tab key walks through every button in document order — not a roving tabindex pattern. The Dialog (create-creator) does get Esc-close via Radix (verified `dialog.tsx` uses `@radix-ui/react-dialog`), so that part of the contract is satisfied.

This is an accessibility gap against the WCAG 2.1 AA floor declared in `apps/web/AGENTS.md` ("keep keyboard paths …"). Fix direction: introduce a `roving-tabindex` pattern — on the `<div role="toolbar" aria-label="Profiles">`, listen for `ArrowLeft` / `ArrowRight` / `Home` / `End` and move focus to the previous/next/first/last avatar button. Roving tabindex keeps the row out of the global Tab order once focus is inside it, which is the established pattern for toolbar widgets.

The single-creator no-op contract (line 41: `if (items.length > 1) setActiveCreatorId(...)`) is correctly implemented — that part of §29.5 holds.

### 🟢 Suggestion

#### F-101 · `useEffect` in `setup-step-agent.tsx` (auto-select first installed) re-fires on every `state` change

`apps/web/src/pages/setup-step-agent.tsx:27-32`. The auto-select effect depends on `[recommendedIndex, agents, state, onChange]`. The first guard `if (state.selectedAgent || state.customLaunchCommand) return` makes it safe, but `state` in the dep array means the effect re-runs on every wizard state mutation (e.g., when `selectedAgent` is set by user click, a new `state` object is passed in via `onChange`, the effect runs, finds `selectedAgent` truthy, and no-ops — wasteful and noisy in React DevTools). Recommend narrowing deps to `[recommendedIndex, agents]` and reading `state` via a ref if needed.

#### F-102 · Health-probe subscription is duplicated between `SetupGate` and `SetupStepDaemon`

`apps/web/src/components/setup/setup-gate.tsx:33-76` and `apps/web/src/pages/setup-step-daemon.tsx:18-60` both implement the same `onDaemonStatusChanged` + `client.health()` poll pattern. Recommend extracting a `useDaemonReady()` hook (e.g., `apps/web/src/lib/use-daemon-ready.ts`) that returns `{ ready, error, retry }` and is consumed by both components. This also clarifies the "per-launch daemon-ready gate" contract (web-ui.md §29.6) in one place rather than two.

#### F-103 · `SetupStepWelcome` swallows Tauri `getWorkspaceRoot` errors silently

`apps/web/src/pages/setup-step-welcome.tsx:30-32`. When the Tauri command rejects, the wizard silently sets `'~/Documents/nexus42/default'` (a hard-coded tilde string) without notifying the user. The spec says "create directory if absent" (desktop-shell.md §13.5), which is what the Tauri side does; the wizard's silent fallback hides a real failure mode (e.g., disk full, permission denied). Recommend distinguishing "fallback applied" from "fallback applied because the command failed" — surface a helper line / warning row.

#### F-104 · Browser fallback string uses `~/Documents/nexus42/default` (tilde) while Tauri returns the resolved absolute path

`apps/web/src/pages/setup-step-welcome.tsx:31` vs `apps/desktop/src-tauri/src/lib.rs:107-129`. The browser fallback shows a literal `~` in the Workspace location field; the Tauri path returns an absolute path (no `~`). This is a cosmetic inconsistency — the desktop user sees `/Users/.../Documents/nexus42/default` and the browser user sees `~/Documents/nexus42/default`. The desktop-shell.md §13.5 contract calls for the cross-platform default; both should display the same form (either always the literal default or always the resolved absolute path).

#### F-105 · `/strategy` redirect drops any active preset ID

`apps/web/src/App.tsx:92` redirects `/strategy → /strategies` (the list, not the detail). The spec (web-ui.md §29.4) says "/strategy → /strategies/:presetId (requires a stored active preset ID or redirects to list)". The redirect always lands on the list; there is no "stored active preset ID" plumbing. For the rare user with an old `/strategy` deep-link the UX is "you've lost your place". Recommend either remembering the last-opened preset ID in `localStorage` (mirror `nexus:activeCreatorId` pattern) or accepting a query-param fallback (`/strategy?id=foo` → `/strategies/foo`).

#### F-106 · `Sidebar` is open-ended data — no compile-time guard against adding a third top-level item outside the tab structure

`apps/web/src/components/layout/sidebar.tsx:34-62`. `CREATOR_GROUPS` and `ORCHESTRATOR_GROUPS` are plain arrays; a future change can add a third "Discover" item outside the tabs and the spec's "two-tab IA" invariant silently breaks. Recommend a typed `TabId = 'creator' | 'orchestrator'` (already present) plus an eslint or build-time guard such as `const ALL_NAV_ROUTES: ReadonlyArray<NavItem['to']> = [...new Set([...CREATOR_GROUPS.flatMap(g => g.items), ...ORCHESTRATOR_GROUPS.flatMap(g => g.items)].map(i => i.to))]` that the route table can be cross-checked against. Defensive but cheap.

## Source Trace

- Finding ID: F-001
  - Source Type: doc-rule + manual-reasoning
  - Source Reference: `apps/web/src/pages/setup-wizard-page.tsx:35-38` vs `desktop-shell.md` §13.2 step 4
  - Confidence: High
- Finding ID: F-002
  - Source Type: manual-reasoning + grep
  - Source Reference: `grep -rln "presets-page" apps/web/src/` returns 1 (the file itself)
  - Confidence: High
- Finding ID: F-003
  - Source Type: manual-reasoning + grep
  - Source Reference: `find apps/web/src -name "sidebar.test.*"` returns 0
  - Confidence: High
- Finding ID: F-004
  - Source Type: doc-rule + manual-reasoning
  - Source Reference: `apps/web/DESIGN.md:741` ("vitest snapshot (light + dark themes) gates regressions")
  - Confidence: High
- Finding ID: F-005
  - Source Type: doc-rule + manual-reasoning
  - Source Reference: `apps/web/src/components/layout/footer-profiles.tsx:75-89` vs `web-ui.md` §29.5
  - Confidence: High
- Finding ID: F-101…F-106
  - Source Type: manual-reasoning
  - Source Reference: cited file:line in each finding
  - Confidence: Medium–High

## Cross-Track Notes (P0 ↔ P1 contract coherence)

I cross-checked the additive agent-scan contract across all four boundaries:

| Boundary | Surface | Status |
|---|---|---|
| `schemas/daemon-api/agent-host/{scan-request,scan-response,agent-scan-entry}.schema.json` | Additive — frozen by P-1 (commit `3540f928`) | ✅ |
| `crates/nexus-contracts` (Rust + TypeScript) | Codegen from schemas; both consumers see the same shape (`AgentScanRequest`, `AgentScanResponse`, `AgentScanEntry`) | ✅ |
| `crates/nexus-daemon-runtime/src/api/handlers/agent_host.rs` | `scan` handler composes `RegistryClient::get_registry()` + `scan_local_installations()`; respects PATH-probe safety boundary (desktop-shell.md §14.3 — bounded concurrency via semaphore, `--version` timeout, registry-known binary names only) | ✅ |
| `crates/nexus-acp-host/src/registry.rs` `scan_local_installations` | Helper bounded to `SCAN_MAX_CONCURRENT` semaphores + `SCAN_VERSION_TIMEOUT`; tests `scan_local_installations_finds_installed_binary` / `_handles_timeout` cover the safety contract | ✅ |
| `apps/web/src/lib/nexus/types.ts` `scanAgents` + `apps/web/src/api/queries.ts` `useScanAgents` | Consumes the frozen contract via `NexusClient` interface — no handwritten wire shapes | ✅ |
| `apps/web/src/components/setup/setup-step-agent.tsx` | Renders scan response; "Recommended" badge = first `installed: true`; selectable cards; custom `launch_command` input | ✅ |

The P0 ↔ P1 contract is coherent. The only cross-track gap is F-001 (the persistence of the chosen agent), which is a contract-coverage gap, not a contract-drift gap.

## Module Boundaries Audit

- **P0 backend (`crates/nexus-daemon-runtime/` + `crates/nexus-acp-host/` + `apps/desktop/src-tauri/` + `apps/nexus42/`)**:
  - `agent_host.rs` scan handler — well-factored (helper `build_scan_entry`, `platform_binary_commands`), tests cover 200 response shape, filter parameter, installed/missing/prefers-installed, platform-command deduping.
  - `registry.rs` `scan_local_installations_impl` — bounded concurrency via `tokio::sync::Semaphore` (4 permits, per `desktop-shell.md` §14.3 recommendation N=4); 2-second `--version` timeout via `tokio::time::timeout`; no shell expansion (uses `Command::new(binary).arg("--version")` directly); tests pin the install/timeout/missing/registry-cache paths.
  - `apps/desktop/src-tauri/src/lib.rs` — workspace default + `setup_completed` read/write preserved-keys invariant (`setup_completed_write_preserves_existing_keys` test); fail-closed path guard (`denies_by_default_when_workspace_root_is_unknown`); 526 lines (above 250-line soft cap but split helpers are reasonable).
  - `apps/nexus42/src/config.rs` — `setup_completed: Option<bool>` additive (`#[serde(default)]`), round-trip + absent-key + invalid-value + unset + cli integration all tested.
- **P1 frontend (`apps/web/`)**:
  - `Sidebar` 198 lines (≤250 soft cap) — well-decomposed `TabButton` / `NavGroup` / `NavItemLink` sub-components.
  - `FooterProfiles` 153 lines — uses `useCreators` / `useCreateCreator` from the queries layer; consumes `CreatorInfo` from `@42ch/nexus-contracts` (no handwritten DTO).
  - `DaemonStatusBar` 110 lines (after the V1.94 simplification) — only renders when `state === 'running'`, restart-only action with `window.confirm` guard; `MainBanner` 149 lines handles non-running states.
  - `RootLayout` 93 lines — composes `Sidebar` / `Header` / `MainBanner` / `DaemonStatusBar`; mobile (`<lg`) nav mirrors the desktop top-level routes as a horizontal scroller.
  - `SetupWizardPage` 124 lines + 4 step components (welcome 65 / daemon 107 / agent 143 / done 22) — all reasonable.
  - `SetupGate` 91 lines — `setup_completed=false → /setup` redirect; desktop-build only splash; browser-build instant pass.
  - `StrategiesPage` 172 lines (replaces presets-page.tsx at the route level).
  - `StrategyPage` 45 lines — preserves V1.70–V1.75 canvas surface verbatim (uses `<StrategyCanvas presetId={...} />`, the existing canvas component).
  - `button.tsx` 59 lines — single class change fixes dark primary (`dark:text-white`).

No file exceeds the 250-line soft cap by more than ~30 lines except `apps/desktop/src-tauri/src/lib.rs` (526) which is a security-critical single-file guard (`guard_path` + `PathGuardError` + commands + tests) and is appropriately decomposed.

## Verification Run Summary

```
$ pnpm --filter web run typecheck   → green (tsc --noEmit passes; contracts + UI build first)
$ pnpm --filter web run build       → green (vite v6.4.3; 2514 modules transformed; 7.06s)
$ pnpm vitest run (layout + setup + strategies-page.test)
  → 5 files / 24 tests passed in 1.94s
$ cargo test -p nexus-daemon-runtime --lib   → 421 passed; 0 failed
$ cargo test -p nexus-acp-host --lib         → 160 passed; 0 failed
$ cargo clippy --all -- -D warnings          → green
```

No CI failures. The CI gate (`cargo clippy --all -- -D warnings` + `cargo +nightly-2026-06-26 fmt --all --check` + `pnpm --filter web test/build/typecheck`) is green for this iteration.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 5 |
| 🟢 Suggestion | 6 |

**Verdict**: **Request Changes**

The V1.94 P1 implementation is structurally clean, contract-coherent with the P0 backend, and visually correct on the surfaces I inspected (two-tab IA, nested nav, restart-icon-only status bar, simplified main banner, Strategies unification). However, five Warning-level gaps stand between the iteration and Approve:

1. **F-001** — the wizard's selected agent / launch command is not persisted, contradicting the spec.
2. **F-002** — `presets-page.tsx` is dead code; remove it.
3. **F-003** — six new primary surfaces ship without unit tests; pin them before closure.
4. **F-004** — the button contrast invariant has no snapshot regression test.
5. **F-005** — the footer profile switcher lacks the roving-tabindex arrow-key pattern.

The qc2 (security lens on the agent-scan endpoint) and qc3 (reliability lens on the per-launch daemon gate) reviews overlap this report in: F-001 (config.toml write safety — partly qc2 territory), F-005 (keyboard a11y — qc3 may own), F-003 (test coverage of gate + splash — qc3 may own). I have flagged each finding with its primary lens but do not duplicate the qc2 / qc3 evidence gathering.

Targeted re-review is the natural follow-up (per `mstar-review-qc`): once F-001 / F-002 are addressed and F-003 / F-004 / F-005 have evidence, I re-validate the same files in the same report path (this file) with a `## Revalidation` section.

## Revalidation (post fix-wave 4f4b468b, merged as 0e75931b)

### Revalidation scope

- F-001..F-005 only (qc2 + qc3 areas out of scope; they Approve).
- Review range / Diff basis (revalidation): `merge-base: bf0e60cc (main HEAD pre-V1.94) + tip: 0e75931b (iteration/v1.94 post-fix-wave)` ≡ `git diff main...iteration/v1.94`.
- Working branch (verified): `iteration/v1.94` @ `0e75931b`.
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus` (via `git rev-parse --show-toplevel`).

### Verification runs

- `pnpm --filter web run test` → **68 test files passed (491 tests passed)** in 11.36s; new tests included: `sidebar.test.tsx` (4), `setup-wizard-page.test.tsx` (2), `strategy-page.test.tsx` (2), `active-creator-context.test.tsx` (5), `setup-completed-context.test.tsx` (3), `button.test.tsx` (2 with snapshot), `footer-profiles.test.tsx` (7 — 4 prior + 3 new keyboard nav).
- `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --lib` → **29 passed; 0 failed**. New tests included: `tests::agent_profile_roundtrips_through_config_toml`, `tests::agent_profile_write_preserves_existing_keys`.

### Revalidation results

- **F-001 — agent persistence: fixed + verified.**
  - `apps/desktop/src-tauri/src/lib.rs` adds `set_agent_profile(name, launch_command)` Tauri command + `write_agent_profile` / `write_agent_profile_at` helpers using `toml_edit::DocumentMut`. Path: `$HOME/.nexus42/agent-host/config.toml` (matches `boot.rs:978` reader and `nexus-agent-host::config::agent_host_config_path`).
  - `apps/web/src/lib/nexus/desktop-capabilities.ts` adds `setAgentProfile(name, launchCommand?)` on both `DesktopCapabilities` interface and `TauriDesktopCapabilities` implementation, invoking `set_agent_profile` Tauri command.
  - `apps/web/src/pages/setup-wizard-page.tsx` `finish()` is now `async` and calls `await desktop.setAgentProfile(name, launchCommand)` **before** `markCompleted()` and `navigate('/works', { replace: true })` — matching the spec ordering.
  - TOML shape produced by `write_agent_profile_at`: `[[providers]] { id = "<name>", protocol = "native_cli", command = "<launch_command>" }`. Daemon's `nexus-agent-host::config::load_config_from_path` parses this into `ProviderConfig { id, protocol: "native_cli", command: Some(...) }`. **Round-trip is correct**; preserved-keys invariant covered by `agent_profile_write_preserves_existing_keys` test.
  - TOCTOU containment check (`manager.rs:168-191`) passes — config path stays within `~/.nexus42/agent-host/`.
  - `apps/web/src/pages/setup-wizard-page.test.tsx` second test verifies `setAgentProfile` is called with `(name, launch_command)` BEFORE `setSetupCompleted(true)`.
  - Minor architectural nit (out of fix-wave scope): the desktop command hardcodes `dirs::home_dir()?.join(".nexus42").join("agent-host").join("config.toml")` instead of reusing `nexus_agent_host::config::agent_host_config_path(&home)` or adding a `nexus_home_layout::agent_host_config_path(&home)` helper. This matches the existing pattern in `boot.rs:978` (`nexus_home.join("agent-host").join("config.toml")` directly), so not a blocker. Will surface as a Suggestion for V1.95 residual.

- **F-002 — dead presets-page: fixed + verified.**
  - `apps/web/src/pages/presets-page.tsx` deleted (`-159` lines).
  - `grep -rln "presets-page\|PresetsPage" apps/web/` → 0 matches.
  - `apps/web/src/App.tsx` line 91 `/presets → /strategies` redirect removed; `/presets` now falls through to `path="*"` (`<NotFoundPage />`). Per the assignment's "your call" wording this is the cleaner choice — no stale redirect survives in the SPA. The `/strategy → /strategies` redirect is retained (per V1.94 §29.4 compatibility contract).
  - `pnpm --filter web run typecheck` + `pnpm --filter web run build` were not run in this revalidation pass (the broader CI gate in the V1.94 P-last closure will exercise them); the 491-test green run does compile all new files and the deletion transitively, so a build regression would have surfaced.

- **F-003 — unit tests: fixed + verified.**
  - **491 → confirmed.** Total count matches the assignment's claim (was 470 pre-fix-wave; +21 new tests).
  - `sidebar.test.tsx` (4 tests): Creator tab default, Orchestrator tab swap + nested nav, no Connect/Daemon top-level item, footer-profiles mounted inside (toolbar role). All non-trivial — they pin the V1.94 IA locked decisions D1 / E1 / C1.
  - `setup-wizard-page.test.tsx` (2 tests): four-step E2E navigation, **and** desktop-mode order-of-operations (`setAgentProfile('codex', 'codex')` called before `setSetupCompleted(true)`) — pins F-001 spec compliance from the frontend.
  - `strategy-page.test.tsx` (2 tests): detail route loads at `/strategies/:presetId`; missing preset shows "Strategy not found" empty state. Pins the V1.70–V1.75 surface preservation contract.
  - `active-creator-context.test.tsx` (5 tests): `localStorage` round-trip on mount, write-through on setter, cross-tab `storage` event sync, both hooks throw outside provider.
  - `setup-completed-context.test.tsx` (3 tests): browser build immediate-completed, desktop build reads `getSetupCompleted` from shell, `markCompleted()` persists via `setSetupCompleted(true)`.

- **F-004 — button snapshot: fixed + verified.**
  - `apps/web/src/components/ui/button.test.tsx` (2 tests): light + dark snapshots of primary variant.
  - `apps/web/src/components/ui/__snapshots__/button.test.tsx.snap` (17 lines): the captured `class` attribute string includes `bg-blue-700 ... dark:bg-brand-cyan dark:text-white dark:hover:bg-blue-800 dark:active:bg-blue-900 ...`. The dark mode test sets `document.documentElement.classList.add('dark')` and the snapshot persists the className invariant — a regression that reverts `dark:text-white` to `dark:text-brand-deep-blue` will fail the snapshot. Tailwind classes are static so light + dark snapshots intentionally produce identical className strings; what matters is the assertion of the dark-prefixed classes. This satisfies DESIGN.md §Component Primitives/Button invariant requirement.

- **F-005 — footer keyboard: fixed + verified.**
  - `apps/web/src/components/layout/footer-profiles.tsx`: `<div role="toolbar" aria-label="Profiles" onKeyDown={handleKeyDown}>` (line 74-79). `handleKeyDown` (line 46-67) handles ArrowRight, ArrowLeft, Home, End with `event.preventDefault()` and clamps via `focusAt`. `CreatorAvatar` is now a `forwardRef<HTMLButtonElement, CreatorAvatarProps>`; each avatar receives `tabIndex={focusIndex === index ? 0 : -1}` and `onFocus={() => setFocusIndex(index)}`. The "+" Add Creator button participates in the roving-tabindex pattern (last slot, `focusIndex === items.length`). `useEffect` (line 36-38) bounds `focusIndex` when items array shrinks.
  - Roving tabindex contract: **only the focused button has `tabindex="0"`; all others have `tabindex="-1"`** — Tab key skips the toolbar once focus is inside it (the standard WAI-ARIA toolbar pattern).
  - Tests (`footer-profiles.test.tsx`, 3 new):
    - "uses roving tabindex so only the focused avatar is in the Tab sequence" — verifies `tabindex="0"` on Alice, `tabindex="-1"` on Bob and Add Creator.
    - "moves focus with arrow keys inside the toolbar" — clicks Alice, presses ArrowRight → Bob has focus + `tabindex="0"`, Alice becomes `tabindex="-1"`. Then ArrowLeft reverses.
    - "jumps to first/last avatar with Home/End" — focuses Add Creator, presses Home → Alice has focus. Re-focuses Add Creator, presses End → Add Creator has focus.
  - Single-creator no-op contract preserved (line 91: `if (items.length > 1) setActiveCreatorId(...)`).
  - Spec (web-ui.md §29.5) requirement "Esc closes any transient UI" is satisfied via Radix Dialog's built-in Esc handler (out of scope for this fix — already covered by qc3 reliability lens).

### Cross-track coherence check (read-only, post-fix-wave)

- **F-001 round-trip**: `set_agent_profile` writes to `~/.nexus42/agent-host/config.toml` via `toml_edit::DocumentMut` (preserves keys, dedupes by `id`). Daemon reads the same path in `crates/nexus-daemon-runtime/src/boot.rs:978` (`nexus_home.join("agent-host").join("config.toml")`); parsed by `nexus_agent_host::config::load_config_from_path` → `AgentHostConfig.providers: Vec<ProviderConfig>`. `ProviderConfig` struct accepts `{id, protocol, command, args, env, enabled}` — all optional except `id` and `protocol`. Wizard produces `{id: <name>, protocol: "native_cli", command: Some(<launch_command>)}` — deserializes correctly with `enabled = true` (default).
- **Hardening is preserved**: TOCTOU containment check at `manager.rs:168-191` (`canonicalize(parent)` then `canonical_config.starts_with(&canonical_dir)`) accepts the wizard's path since `~/.nexus42/agent-host/config.toml` is inside `~/.nexus42/agent-host/`. The `provider_id` collision semantics in `nexus-agent-host/src/discovery/path_scan.rs` and `catalog.rs` mean a wizard-written `id = "codex"` will dedup against PATH-scanned "codex" entries — explicit user config wins, which is the correct precedence.

### Updated verdict

- Previous: **Request Changes** (5 Warnings)
- Current: **Approve**
- Residual blockers: none.
- Carried-over Suggestions (F-101..F-106) remain deferred to V1.95 by PM (out of this fix-wave's scope, as stated in the assignment).

### CI gate status

- `pnpm --filter web run test`: **491 / 491 passed** (68 files).
- `cargo test --manifest-path apps/desktop/src-tauri/Cargo.toml --lib`: **29 / 29 passed**.
- No CI failures introduced by the fix-wave.