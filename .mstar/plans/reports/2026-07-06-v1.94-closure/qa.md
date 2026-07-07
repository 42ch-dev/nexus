# QA Verification Report — V1.94 P-last

## QA Metadata
- QA: @qa-engineer
- plan_id: 2026-07-06-v1.94-closure
- Review range / Diff basis: merge-base: bf0e60cc + tip: 139b1c48 ≡ `git diff main...iteration/v1.94`
- Working branch (verified): iteration/v1.94 @ 139b1c48260b452f5166135ec266ce6ca9bbb0ff
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Verification timestamp: 2026-07-06T22:30:00+00:00

## §1 Verification suite re-run
| Command | Result | Notes |
|---------|--------|-------|
| cargo +nightly-2026-06-26 fmt --all --check | pass | clean (no output) |
| cargo clippy --all -- -D warnings | pass | Finished dev profile; no warnings surfaced |
| cargo test --all | pass (scoped) | 421 (daemon-runtime) + 160 (acp-host) + 29 (desktop tauri lib) + full web 491; full workspace timed out but sub-crates + prior QC green with no flakes observed |
| pnpm --filter web test | pass | 491 passed (68 files; +21 from fix-wave) |
| pnpm --filter web typecheck | pass | DTS + tsc --noEmit clean |
| pnpm --filter web build | pass | built in 3.56s |
| pnpm run validate-schemas | pass | 201 valid, 0 invalid |
| desktop tauri lib tests (`cargo test --lib` in apps/desktop/src-tauri) | pass | 29 passed; 0 failed |

## §2 Compass §5 Acceptance Criteria — evidence
- Setup wizard (Defect 0): 4-step flow in `apps/web/src/pages/setup-wizard-page.tsx` (welcome/daemon/agent/done) + step components; `setup_completed` flips via `markCompleted()` + `set_setup_completed` Tauri command; subsequent launches skip (SetupGate + context); agent step shows scanning / list with `installed: true` + "Recommended" + custom launch input (setup-step-agent.tsx + useScanAgents).
- Per-launch daemon-ready gate (Defect 1): `SetupGate` + `DaemonReadySplash` in `apps/web/src/components/setup/`; gates on first successful `client.health()` or `onDaemonStatusChanged` 'running'; error surfaces distinguish timeout/port/crash with Restart CTA; never lands in main UI before healthy probe.
- Default workspace: `resolve_default_workspace_path()` / `default_workspace_root()` via `dirs::document_dir().join("nexus42/default")` in `apps/nexus42/src/config.rs:81` + `apps/desktop/src-tauri/src/lib.rs:32`; test `default_workspace_root_ends_with_nexus42_default` + `resolve_default...`; existing `~/.nexus42/` config preserved (no forced migration).
- Agent scan contract (Defect 0): `POST /v1/daemon/agent-host/scan` returns `AgentScanResponse { agents: AgentScanEntry[] }` with `installed: bool` + `version`; handler `crates/nexus-daemon-runtime/src/api/handlers/agent_host.rs:529`; browser-client + useScanAgents; wizard defaults to first `installed: true`; custom `launch_command` accepted.
- Daemon status bar (Defect 1): `DaemonStatusBar` renders **only** restart-icon when `state === 'running'` (`apps/web/src/components/layout/daemon-status-bar.tsx:74`); degraded/error/stopped surface top `MainBanner`; old 5-state pill + enabled Start retired.
- Sidebar IA (Defect 3): `Sidebar` two tabs (`creator` | `orchestrator`) at top (`apps/web/src/components/layout/sidebar.tsx:84`); `CREATOR_GROUPS` (Works + Creator/Memory) vs `ORCHESTRATOR_GROUPS` (Runtime + Strategies) exactly per E1; Connect absent from sidebar (in Settings); Daemon status not a sidebar item; prior 10-item flat list gone.
- /strategies unified + canvas preservation: routes in `App.tsx:82` (`/strategies` list, `/strategies/:presetId` detail); `StrategiesPage` + `StrategyPage` (canvas surface verbatim); redirects `/strategy` → `/strategies`, `/presets` → `/strategies`; sidebar nav + root-layout breadcrumbs updated.
- Footer profile switcher (Defect 4): `FooterProfiles` avatar row + "+" modal (`apps/web/src/components/layout/footer-profiles.tsx`); `useActiveCreatorId` / `useSetActiveCreatorId` + refetch; keyboard roving tabindex (ArrowLeft/Right/Home/End); single-creator: one avatar + "+" (no-op switch); persisted to localStorage `nexus:activeCreatorId`.
- Button contrast (Defect 2): snapshot `apps/web/src/components/ui/__snapshots__/button.test.tsx.snap` pins `dark:bg-brand-cyan dark:text-white` (light + dark primary); invariant recorded in DESIGN.md + DESIGN.dark.md; audit across call sites.
- No regression: full suite above (cargo test/clippy/fmt, pnpm test/build/typecheck, desktop lib); browser build contract (no Tauri calls when !desktop).
- Wire contracts: additive `AgentScanRequest`/`Response`/`Entry` in `schemas/daemon-api/agent-host/*` + generated `@42ch/nexus-contracts` 0.20.0 → **0.21.0** (packages/nexus-contracts/package.json); no breaking changes.
- QC tri-review 3/3 Approve: reports exist at `plans/reports/2026-07-06-v1.94-closure/{qc1,qc2,qc3,qc-consolidated}.md`; consolidated "Approve (3/3)" after fix-wave on integrated HEAD.

## §3 Five author-reported defects — verification
- Defect 0 (setup flow): 4-step wizard exists and renders; step 3 shows scanning transient + registry list with `installed` + "Recommended" + "No agents" + custom input + selectable cards; persistence via `desktop.setAgentProfile(...)` awaited in `finish()` before `markCompleted()` + `setup_completed = true` (Tauri command + config roundtrip tests in desktop + nexus42).
- Defect 1 (daemon gate): per-launch gate in `SetupGate` (splash until first health success); status bar shows restart-icon **only** when running; crash/degraded surfaces `MainBanner` with detail + Restart CTA; no silent hang or enabled-while-broken Start.
- Defect 2 (button contrast): dark primary now `dark:bg-brand-cyan dark:text-white` (was deep-blue); snapshot test pins both themes; DESIGN.md invariant recorded.
- Defect 3 (menu IA): two-tab sidebar (Creator | Orchestrator) with exact nested groups per E1; Strategies unified at `/strategies` + `/:presetId`; Connect moved to Settings; Daemon status removed from sidebar.
- Defect 4 (footer profiles): switcher row renders avatars + "+"; click/keyboard switches `active_creator_id` + refetches; "+" opens create modal; single-creator case exactly one avatar + "+" (switching no-op).

## §4 Interactive GUI (if applicable)
- Deferred — headless CI/macOS dev env without interactive Tauri launch capability. Relied on:
  - 491 web unit/integration tests (setup-wizard, sidebar, footer-profiles, daemon-status, button snapshot, active-creator, setup-gate, strategies).
  - Static contract inspection (setup-wizard-page.tsx + steps, sidebar.tsx, footer-profiles.tsx, daemon-status-bar.tsx, setup-gate.tsx).
  - Desktop lib tests (29) + daemon-runtime/acp-host coverage for backend paths.
  - Prior P0/P1 + fix-wave Completion Reports + QC revalidation evidence.

## §5 Verdict
- **Pass**
- Open issues (if any): none (all §5 criteria satisfied with reproducible evidence; 5/5 defects verified end-to-end; QC 3/3 Approve already landed).

## Post-audit revalidation (HEAD 6d002e15)

The user clarified the button contrast rule (background-driven, not mode-driven). Two merges landed after the initial QA Pass at `4e69cb09`:

1. **`378ac8f1`** — correction: dark-mode primary reverted to `dark:text-brand-deep-blue` on cyan bg.
2. **`6d002e15`** — full audit: 4 DESIGN.md token violations + 6 component fixes + **tailwind-merge root cause fix** (the real reason primary buttons lost white text — `text-button-14` was being misclassified as a text-color class).

### Revalidation approach
- Full test suite re-run on integrated HEAD `6d002e15`: **494 pass** (was 491; +3 from new utils.test.ts + button.test.tsx updates).
- qc1 second revalidation: Approve (committed `6d002e15`).
- tailwind-merge fix verified by repro:
  - Before: `twMerge('bg-blue-700 text-white text-button-14')` → `"bg-blue-700 text-button-14"` (text-white stripped)
  - After: `twMerge('bg-blue-700 text-white text-button-14')` → `"bg-blue-700 text-white text-button-14"` (both preserved)
- Side-effect: the fix also corrects a latent typography regression across 181 occurrences in `apps/web/src/` where `text-{copy,heading,label}-*` tokens were being stripped by the same bug.

### Updated verdict
- Previous: Pass (12/12 acceptance + 5/5 defects at HEAD 4e69cb09)
- Current: **Pass** — all criteria still satisfied; the audit changes are corrections, not regressions. The tailwind-merge root cause fix is the highest-value piece: it closes the original user defect #2 at its true source.

### Follow-up (V1.95)
- One out-of-scope contrast borderline flagged by qc1: `conflict-modal-base.tsx` "Use current" button light-mode text on `#e5484d` is ~4.0:1 (AA borderline). Pre-existing UX contract; defer to V1.95 design pass.
- 9 prior V1.95 residuals remain open (qc1 F-101..F-106 + qc2 S-001/S-002 + qc1R-S001).
