---
iteration_id: V1.94
start_date: 2026-07-06
end_date: 2026-07-06
status: completed
iteration_base_branch: main
target_branch: main
plans:
  - 2026-07-06-v1.94-prepare-specs-and-design
  - 2026-07-06-v1.94-backend-setup-runtime
  - 2026-07-06-v1.94-frontend-onboarding-ia
  - 2026-07-06-v1.94-closure
---

# V1.94 — Desktop App Onboarding & IA Pass — Delivery Compass v1

**Status**: completed (Phase 3 §3.1–§3.5 done; PR delivery Phase 4 next). Headline: real-usage
first-impression pass on the Nexus **desktop app**, driven by structured author
feedback after first launch. Wire contracts additive only
(`@42ch/nexus-contracts` 0.20.0 → 0.21.0). The iteration also closed a latent
`tailwind-merge` typography regression discovered during the button-contrast audit.

## 0. Context

V1.92 → V1.93 closed the remote-access story (TLS + connection model + robustness
polish). The desktop shell (`apps/desktop`) was scaffolded in V1.66 (Tauri v2
macOS wrapper around `apps/web`) but had not received a holistic **first-launch
experience / information architecture** pass. The author launched the app for
real usage and reported five interlocking UX defects, all in the
"first-impression / can-I-even-use-this" category:

0. App boots straight into the main view; there is no setup flow that detects
   and designates the local ACP agent client the daemon should talk to.
1. The status bar shows "Daemon starting…" indefinitely; the Start Daemon
   button stays enabled throughout starting (illogical); and the user is in the
   main UI before the daemon is reachable, so other screens appear broken.
2. Buttons with a dark/primary background do not use a light/white text colour
   (most visible in the dark theme `dark:bg-brand-cyan dark:text-brand-deep-blue`
   combination) — needs a unified contrast fix.
3. The sidebar has too many top-level items (10 flat). The author proposes two
   tab categories: **Creator** (Works with nested Chapters/Findings; Creator
   with nested Memory) and **Orchestrator** (Runtime with Sessions/Schedules/
   Daemon status; Strategies unifying Presets + Strategy).
4. The menu needs a footer with a **Profiles** icon list (default: one "Default
   Creator"; plus a "+" icon to add more).

These are coherent: setup-missing → user lands in main UI → sees "starting"
stuck → menu is also cluttered → no profile switcher. This iteration treats
them as one "first-impression / IA" pass rather than five point fixes.

This iteration does **not** pull in the previously-deferred V1.94 candidates
(`R-V192SEC-001` TOFU transport-binding; `R-V193PL-001` `/v1/local/*` path-literal
spec hygiene; `BL-09` maturation dashboard) — they have no code-surface overlap
with the UX work and would dilute focus. They remain open and re-queued.

## 0.1 Terminology conventions

Inherits the V1.90+ "Daemon API" / "Daemon Runtime" vocabulary (do **not**
introduce new "Local API" prose). New V1.94 vocabulary that crosses the
CONCEPTS.md bar:

- **Setup Wizard** — the first-launch 4-step flow (workspace → daemon-ready →
  agent detection → done) gated by a `setup_completed` marker in
  `~/.nexus42/config.toml`. Triggers again if the marker is cleared.
- **ACP Agent Detection** — the combined registry-list + PATH-scan operation
  the Daemon API exposes at `POST /v1/daemon/agent-host/scan`, returning
  candidate agents annotated with local-install availability.
- **Profile Switcher** — the sidebar footer UI that lists Creator avatars and
  switches the active `creator_id` for the SPA's data queries (the daemon
  already supports multi-creator at the API level; this is the missing UI).

Existing terms reused unchanged: **Daemon**, **Daemon API**, **Creator**,
**Workspace**, **ACP**, **AgentProfile**, **Strategy** (= a preset's runtime
visualisation on the canvas), **Preset** (= the YAML manifest a Strategy runs).

## 1. Locked Decisions (grill-me output)

| Decision | Resolution |
|---|---|
| Iteration direction | **A — Desktop App Onboarding & IA Pass.** Sweep all 5 author-reported UX defects as one first-impression story. No new product surface beyond the UX reshape; the only new backend surface is the additive agent-scan endpoint. |
| Setup wizard scope | **A2 + B1.** First-launch 4-step wizard: (1) welcome + workspace detection/selection, (2) daemon starting + readiness probe, (3) ACP agent detection + selection, (4) completion → main UI. Gated by `setup_completed` marker in `~/.nexus42/config.toml`. Reset-able from Settings. |
| Per-launch daemon gate | **(B1 / C1 supplement)** Every app launch — not only first launch — verifies the daemon is running before entering the main UI. First launch shows the full wizard; subsequent launches show a brief "Starting daemon…" splash that resolves to the main UI once the daemon is healthy. |
| Default workspace path | **`~/Documents/nexus42/default/`** (cross-platform: `dirs::document_dir()`). User-visible creative work lives here, distinct from `~/.nexus42/` (config/db/registry cache — not user-browseable). |
| Daemon status bar in main UI | **C1.** Setup-complete / running: status bar shows only a restart-icon button (no pill, no state text). Daemon crashes during a session: a top-of-main-content banner surfaces the failure with error detail and a Restart CTA. The current 5-state pill + always-enabled Start button is retired from the running state. |
| Menu IA form | **D1.** Top-of-sidebar horizontal tabs: **Creator** | **Orchestrator**. Switching tabs swaps the nav items. Footer (Profiles) is always visible regardless of active tab. |
| Menu item placement | **E1.** Creator tab: **Works** (with Chapters + Findings as nested per-work routes), **Creator** (with Memory + SOUL nested). Orchestrator tab: **Runtime** (Sessions + Schedule + Capabilities nested), **Strategies** (unified Presets management + Strategy canvas — list+detail pattern). **Connect** moves out of the sidebar into Settings. **Daemon status** leaves the menu (it lives in the status bar / banner per C1). |
| Footer profiles | **F1.** Sidebar footer is a row of Creator avatar icons (Slack/Chrome profile-switcher pattern). Click → switch active `creator_id`, SPA refetches. "+" → lightweight modal to create a new Creator. Active selection persisted in `localStorage` and reflected in queries. |
| Button contrast scope | **G1.** Fix dark primary to white text semantics; audit every button variant + every primary/dark-bg call site across `apps/web/` for the dark-bg → light-text invariant; record the invariant in `apps/web/DESIGN.md`. |
| ACP agent detection depth | **H1.** Setup wizard step 3 shows the ACP registry list (served from the daemon's local cache) **annotated with PATH-installation status** (scan via daemon). Default recommendation = first PATH-available agent. User may pick another or supply a custom `launch_command`. Daemon API gains an additive `POST /v1/daemon/agent-host/scan` endpoint. |
| Plan structure | **V1.92/V1.93-style dual track.** P-1 Prepare (specs + DESIGN.md amendments; freezes the agent-scan contract) → **P0 Backend** (`@fullstack-dev`, `crates/` + `apps/desktop/src-tauri/`) ‖ **P1 Frontend** (`@frontend-dev`, `apps/web/`) on parallel worktrees → **P-last closure** (QC tri-review + QA + compound + Profile B + PR). P0 ‖ P1 have disjoint file sets → worktree isolation is safe (P1 builds against the P-1-locked contract). |
| Branch policy | `iteration_base_branch=main` (HEAD `bf0e60cc`, post-V1.93 PR #121); `spec_integration_branch=iteration/v1.94`; `target_branch=main`. Matches the documented `.mstar/AGENTS.md` two-tier branch model and the unbroken V1.39–V1.93 history. |
| Contract impact | **Additive only.** New DTOs for `POST /v1/daemon/agent-host/scan` (request with optional filter; response with `AgentScanEntry[]` carrying `name`, `registry_agent_id`, `launch_command`, `installed` flag, `version`). `@42ch/nexus-contracts` 0.20.0 → 0.21.0. No breaking changes; no schema_version bump beyond the additive types. |
| Residual posture | Open V1.94-deferred residuals (`R-V192SEC-001` medium TOFU transport-binding; `R-V193PL-001` low `/v1/local/*` path-literal spec hygiene; `R-V191P1-005` nit FindingsPage memoisation) stay open and re-queued — explicitly **out of scope** for this iteration. Any new V1.94-QC residuals register against V1.95. |

## 2. Scope

This iteration locks four delivery spec points plus closure:

- **SP-1: Prepare — Specs + DESIGN.md (P-1).** Codify four normative additions:
  (a) `specs/desktop-shell.md` gains a **§Setup Wizard** section
  (4-step flow, `setup_completed` marker semantics, per-launch daemon-ready
  gate, default workspace `~/Documents/nexus42/default/` cross-platform rule).
  (b) `specs/desktop-shell.md` gains a **§ACP Agent Detection**
  subsection pointing at the new endpoint contract.
  (c) `specs/web-ui.md` gains a **§Information Architecture (V1.94)**
  section: two-tab sidebar (Creator / Orchestrator), nested nav per E1,
  footer Profiles switcher per F1, simplified daemon status bar per C1.
  (d) `apps/web/DESIGN.md` (+ `DESIGN.dark.md` mirror) records the
  **dark-bg → light/white-text button invariant** under §Component
  Primitives/Button, fixes the `dark:bg-brand-cyan dark:text-brand-deep-blue`
  primary token to `dark:text-white`, and adds footer-profile + setup-wizard
  tokens. Architect-owned spec/design amendments; writing-specialist
  terminology + voice pass. No codegen changes during P-1.
- **SP-2: Backend — Setup Runtime + Agent Scan (P0, backend headline).**
  (a) Daemon-side `POST /v1/daemon/agent-host/scan` endpoint: combines the
  existing `crates/nexus-acp-host/src/registry.rs` cache with a new
  PATH-availability scan (`which`-equivalent on Unix; bounded to known ACP
  agent binary names from the registry). Returns `AgentScanEntry[]` with
  `installed: bool` + `version: Option<String>` (best-effort `--version`
  probe with short timeout). (b) Workspace bootstrap: `apps/desktop/src-tauri`
  + `apps/nexus42` config plumbing to default `workspace_path` to
  `~/Documents/nexus42/default/` (via `dirs::document_dir()`) when none is
  set, creating the directory if absent. (c) `setup_completed: bool` field
  added to `~/.nexus42/config.toml`; the Tauri shell writes it after wizard
  completion and reads it on every launch to decide wizard vs. brief daemon
  splash. (d) Daemon API health-probe plumbing unchanged (already wired);
  the desktop shell gains an ensure-daemon-running gate at launch that
  surfaces a brief splash until first health-probe success (mirrors setup
  wizard step 2 mechanics).
- **SP-3: Frontend — Onboarding + IA + Polish (P1, frontend headline).**
  (a) **Setup wizard**: new `apps/web/src/pages/setup-wizard-page.tsx` with
  4 step components; `/setup` route; `App.tsx` redirect logic
  (`setup_completed === false` → `/setup`; otherwise brief daemon-ready
  splash → main UI). (b) **Sidebar IA rewrite**:
  `apps/web/src/components/layout/sidebar.tsx` restructured into top
  Creator/Orchestrator tabs + nested nav + footer profile switcher; new
  `apps/web/src/components/layout/footer-profiles.tsx`; mobile (`<lg`) nav
  mirrors the two-tab structure as a top dropdown/pill scroller. (c)
  **Daemon status bar simplification**: `daemon-status-bar.tsx` reduced to
  restart-icon-only when running; new `apps/web/src/components/layout/main-banner.tsx`
  for degraded/error states. (d) **Button contrast audit**: `button.tsx`
  dark primary fix + sweep of every button call site in `apps/web/src/`
  for the dark-bg → light-text invariant; DESIGN.md amendment feeds the
  rule. (e) **Strategies unification**: existing `/presets` + `/strategy`
  routes collapse to `/strategies` (list) + `/strategies/:presetId` (canvas
  detail) — preserves V1.70 canvas surface while exposing the CRUD list as
  the entry. (f) **Footer profile switcher**: `useActiveCreatorId` hook
  promoted to client-context-level; avatar list renders from
  `useCreators()`; "+ add" opens a modal consuming the existing creator
  create API.
- **SP-4: Closure.** QC tri-review (qc1 IA/structure lens; qc2 security lens
  on the new agent-scan endpoint + PATH execution; qc3 reliability lens on
  the daemon-ready gate + per-launch splash) + QA + compound + Profile B
  compaction (V1.93 cleanup if not already done at iteration-start) + PR to
  `main`.

## 2.1 Architecture Hierarchy and Ownership

- **P0 (backend) lives in `crates/nexus-daemon-runtime/` + `crates/nexus-acp-host/` + `apps/desktop/src-tauri/` + `apps/nexus42/`**:
  - `crates/nexus-acp-host/src/registry.rs` — add `scan_local_installations`
    helper (PATH probe of registry-known binary names; bounded concurrency;
    short `--version` timeout). No change to the registry fetch/cache.
  - `crates/nexus-daemon-runtime/src/api/handlers/agent_host.rs` — new
    `scan` handler routing to the above; additive `AgentScanRequest` /
    `AgentScanResponse` DTOs (codegen from the new schemas).
  - `crates/nexus-contracts/src/local/...` — generated types for the above
    (no hand-written DTOs per the crate invariant).
  - `apps/desktop/src-tauri/src/lib.rs` — workspace default
    (`dirs::document_dir()`) + `setup_completed` marker plumbing; ensure-daemon-ready
    splash gate on launch (mirrors existing sidecar start logic).
  - `apps/nexus42/src/config.rs` — accept `setup_completed: bool` field;
    expose `config set setup_completed true|false`.
  - `schemas/daemon-api/agent-host/scan-request.schema.json` +
    `scan-response.schema.json` + `agent-scan-entry.schema.json` — additive
    schemas (P-1 freeze; Daemon API path per V1.90 rename).
  - Out of bounds: `apps/web/**` (no contract change beyond what P-1 froze).
- **P1 (frontend) lives in `apps/web/`**:
  - `src/App.tsx` — `/setup` route, `/strategies` route collapse, redirect logic.
  - `src/pages/setup-wizard-page.tsx` + `src/pages/setup-*-step.tsx` (new).
  - `src/components/layout/{sidebar,footer-profiles,daemon-status-bar,main-banner,root-layout}.tsx`.
  - `src/components/ui/button.tsx` — dark-primary token fix.
  - `src/pages/strategies-page.tsx` (replaces `presets-page.tsx` +
    `strategy-page.tsx` entry; canvas detail preserved).
  - `src/lib/client-context.tsx` — `active_creator_id` storage +
    `setup_completed` reader (via Tauri command for desktop; localStorage
    fallback for browser).
  - `DESIGN.md` + `DESIGN.dark.md` — button invariant + new tokens.
  - Out of bounds: `crates/**` (consumes the P-1-locked contract only).
- **P-1 (Prepare) spans `specs/{desktop-shell,web-ui}.md` +
  `apps/web/DESIGN.md` + `apps/web/DESIGN.dark.md` + `schemas/daemon-api/agent-host/`**.
  No code changes during P-1; the agent-scan schemas land in this plan so
  P0 implement + P1 client wiring share a frozen contract. **The strengthened
  §5 Acceptance Criteria (wizard UX states, daemon-gate failure surfaces,
  Strategies verbatim preservation, footer keyboard + single-creator rules,
  browser-build contract) are part of the frozen normative contract that P-1
  must codify into the specs.**
- **Single owner per track, parallel worktrees for P0 ‖ P1.** P-1 must land
  on `iteration/v1.94` before P0/P1 topic branches are cut (the agent-scan
  contract + the IA spec are the implement contracts). P0 and P1 touch
  disjoint trees (`crates/` + `apps/desktop/src-tauri/` + `apps/nexus42/`
  vs `apps/web/`) → no merge conflicts expected.

## 2.2 Product Success Criteria

- **First launch "just works" end-to-end.** A fresh-install user (no
  `~/.nexus42/`) launches the desktop app, walks the 4-step wizard, and
  lands in the main UI with: a workspace at `~/Documents/nexus42/default/`,
  a running daemon, an ACP agent client picked (default = first
  PATH-available), and a "Default Creator" available in the footer
  switcher.
- **Subsequent launches gate on daemon-ready.** A returning user
  (`setup_completed === true`) sees a brief "Starting daemon…" splash (minimal,
  not the main UI shell) and lands in the main UI **only after the first
  successful health probe** — never enters the main UI before the daemon is
  reachable (closes author-reported defect 1). On boot failure the splash
  resolves to a non-hanging error surface (see §5).
- **Daemon status bar shows only a restart icon when running.** No
  always-on pill, no enabled-while-starting Start button (closes defect 1).
  Daemon crashes during a session surface a top-of-main banner with error
  detail + Restart CTA.
- **Sidebar respects the two-tab IA.** Author sees Creator | Orchestrator
  tabs at the top of the sidebar; tab switch swaps nav items; footer
  profiles row always visible. The previous 10-item flat list is gone.
- **Strategies unifies Presets + Strategy.** A single `/strategies` entry
  shows the preset list; clicking a row opens the existing Strategy canvas
  detail (no canvas rewrite; entry-point reshape only).
- **Connect is out of the sidebar.** Connection config lives in Settings
  (the existing `/connect` route is reachable from there).
- **Button contrast invariant holds.** Every button with a dark/primary
  background uses light/white text in both light and dark themes. The
  invariant is recorded in `apps/web/DESIGN.md`.
- **Footer profile switcher switches active Creator.** Clicking an avatar
  updates `active_creator_id` and refetches the queries that depend on it;
  "+" opens a create-Creator modal.
- **No regression** in local / CLI / existing desktop / web-dev flows
  (`cargo test --all`, `pnpm --filter web test`, `pnpm --filter web build`,
  desktop tauri tests, Vite dev proxy).
- `cargo clippy --all -- -D warnings` and `cargo +nightly-2026-06-26 fmt --all --check`
  pass (CI gate).
- QC tri-review consolidated Approve (qc1 IA + structure lens; qc2 security
  lens on the PATH scan + agent-host/scan endpoint; qc3 reliability lens on
  the per-launch daemon gate + crash-banner logic); QA verifies the wizard
  E2E + per-launch gate regression tests.

## 3. Plans

| plan_id | Name | Status | Notes |
|---------|------|--------|-------|
| `2026-07-06-v1.94-prepare-specs-and-design` | Prepare — desktop-shell §Setup Wizard + §ACP Agent Detection + web-ui §Information Architecture (V1.94) + DESIGN.md button invariant + agent-scan schemas | Done | Architect-owned spec/design/schemas amendment; Phase-1 review chain landed spec/DESIGN.md edits; P-1 execute added codegen (commit `3540f928`); merged `41580bb0`. validate-schemas 201; cargo/clippy/fmt/typecheck/build green. QC skipped (prepare/spec; consolidated at P-last). |
| `2026-07-06-v1.94-backend-setup-runtime` | Backend — Setup Runtime + Agent Scan (P0): `POST /v1/daemon/agent-host/scan` + workspace default `~/Documents/nexus42/default/` + `setup_completed` marker + per-launch daemon-ready gate | Done | `@fullstack-dev`, `crates/nexus-daemon-runtime/` + `crates/nexus-acp-host/` + `apps/desktop/src-tauri/` + `apps/nexus42/`. Implemented (`fc488322`); merged `df67e5bb`. cargo test --all green; clippy/fmt green. |
| `2026-07-06-v1.94-frontend-onboarding-ia` | Frontend — Onboarding + IA + Polish (P1): 4-step setup wizard + sidebar tabs + footer profiles + daemon status simplification + button contrast audit + Strategies unification | Done | `@frontend-dev`, `apps/web/`. Implemented (`273863d2`); merged `f7af91bc`. pnpm --filter web test 470 pass; typecheck/build clean. PM committed on behalf of track (subagent left work uncommitted). |
| `2026-07-06-v1.94-closure` | Closure — QC tri-review + QA + compound + Profile B compaction + PR to `main` | Done | `@project-manager`-owned; QC 3/3 Approve (twice — initial + post-audit); QA Pass; compound: 2 new docs + 1 updated; Profile B compaction done. PR Phase 4. Includes mid-Phase-3 user clarification on button contrast rule + full audit that found the tailwind-merge root cause (text-white was being stripped — real cause of original defect #2). |

Status values: `Todo` | `InProgress` | `InReview` | `Done` | `Blocked`

## 4. Milestones

| Milestone | Target date | Status |
|-----------|-------------|--------|
| Compass + plans locked (Phase 1 Review & Edit chain done) | 2026-07-06 | in progress |
| P-1 Prepare complete (specs + DESIGN.md + agent-scan schemas landed) | 2026-07-06 | pending |
| P0 (Backend) + P1 (Frontend) implemented on parallel worktrees | 2026-07-07 | pending |
| QC tri-review Approve | 2026-07-08 | pending |
| QA + iteration-close + PR to `main` | 2026-07-08 | pending |

## 5. Acceptance Criteria

- **Setup wizard (Defect 0)**: The 4-step setup wizard renders end-to-end on first launch (welcome+workspace → daemon-ready → agent detection → done). `setup_completed` flips to `true` on completion. Subsequent launches skip the wizard entirely. Agent detection step (step 3) must show: (a) "Scanning for local ACP agents…" transient state, (b) list of registry entries annotated with `installed: true` + "Recommended" badge for the first PATH-available, (c) explicit "No agents found on PATH" state with a "Use custom launch command" text input + "Continue with custom" primary CTA, (d) selectable cards/rows for discovered agents with name, version (if probed), and install status. Selecting an agent or supplying custom `launch_command` is persisted before "Done".
- **Per-launch daemon-ready gate (Defect 1)**: Every app launch (including after setup) gates entry to the main UI on a healthy daemon probe. A brief "Starting daemon…" splash (or wizard step 2 when first-launch) is visible until the **first** successful `GET /v1/daemon/runtime/health`. Landing in the main UI before the first healthy probe is a regression. On failure (timeout after `HEALTH_START_TIMEOUT`, port conflict, crash): user sees a non-hanging error surface (banner or wizard error step) with: (i) clear copy distinguishing "timed out (15s)" vs "port 8420 already in use" vs "daemon crashed", (ii) actionable next step (Restart CTA or "Kill conflicting process" hint), (iii) never a silent hang or enabled-while-broken Start button.
- **Default workspace**: resolves to `~/Documents/nexus42/default/` (cross-platform via `dirs::document_dir()`); directory created if absent. Existing workspaces under `~/.nexus42/` continue to work — no forced migration.
- **Agent scan contract (Defect 0)**: `POST /v1/daemon/agent-host/scan` returns the registry list annotated with `installed: bool` + best-effort `version`. The wizard's default recommendation is the first `installed: true` entry. Custom `launch_command` path is accepted and stored.
- **Daemon status bar (Defect 1)**: In the running/healthy state the status bar shows **only** a restart-icon button (no pill, no state text, no enabled Start button). Degraded / error / stopped / crash states surface a top-of-main-content banner with error detail + Restart CTA. The old 5-state pill + always-enabled Start button is retired.
- **Sidebar IA (Defect 3)**: Two top-level tabs (Creator | Orchestrator) at the top of the sidebar. Tab switch swaps the visible nav items exactly per compass §1 E1. Connect is reachable only from Settings. Daemon status is not a sidebar item. The previous 10-item flat list is gone. **Strategies unification** (`/strategies` list + `/strategies/:presetId` canvas detail) is an entry-point reshape only; the full V1.70–V1.75 canvas surface, React Flow behaviour, write-boundary, conflict modals, and non-spatial views are preserved verbatim at the detail route (no canvas rewrite).
- **Footer profile switcher (Defect 4)**: Renders a horizontal row of Creator avatar icons (Slack/Chrome pattern). Click/keyboard (Enter/Space) switches active `creator_id`; dependent queries refetch. "+" opens a lightweight create-Creator modal using the existing daemon endpoint. Single-Creator case: exactly one avatar + "+" (switching is a no-op; "+" is the only CTA). Selection is persisted (`localStorage` key `nexus:activeCreatorId` for browser; Tauri store equivalent for desktop) and restored on reload. Keyboard: arrow-left/right, Home/End, Esc closes any transient UI. Avatar fallback (initials or generic icon) when no image.
- **Button contrast (Defect 2)**: Every button (or button-like) with a dark/primary/saturated background uses light/white text in both light and dark themes. The invariant is recorded in `apps/web/DESIGN.md` §Component Primitives/Button and audited across all call sites in `apps/web/src/`.
- **No regression**: local / CLI / desktop / web-dev flows green (`cargo test --all`, `pnpm --filter web test/build/typecheck`, desktop tests). `cargo clippy --all -- -D warnings` + `cargo +nightly-2026-06-26 fmt --all --check` pass.
- **Wire contracts**: additive only (`AgentScanRequest` / `AgentScanResponse`); `@42ch/nexus-contracts` 0.20.0 → 0.21.0; no breaking changes.
- **QC/QA**: QC tri-review consolidated Approve (qc1 IA+structure; qc2 security on PATH scan; qc3 reliability on daemon gate + failure paths). QA Pass on wizard E2E (first + returning + crash recovery), sidebar tabs, footer switch (incl. keyboard + single-creator), button contrast (light+dark), strategies preservation.
- Open V1.94-deferred residuals (`R-V192SEC-001`, `R-V193PL-001`, `R-V191P1-005`) stay open and re-queued — explicitly out of scope.

## 6. Non-Goals

- **TOFU transport-binding (R-V192SEC-001)** — stays deferred to V1.95;
  unrelated to first-impression UX work.
- **`/v1/local/*` path-literal spec hygiene (R-V193PL-001)** — stays
  deferred to a dedicated spec-hygiene pass; unrelated to the desktop UX
  surface.
- **FindingsPage memoisation (R-V191P1-005)** — unchanged, deferred to
  "when list virtualisation lands".
- **Body editor + per-chapter lock** — V1.67 roadmap; canvas pivot (V1.75)
  retired the whole-document editor. Not in scope.
- **Setup Wizard remote-connection step** — the wizard is local-first; the
  existing `/connect` flow (V1.92) remains the separate remote-connection
  surface, reachable from Settings. The wizard does not duplicate it.
- **Strategy canvas rewrite** — the Strategies unification is an
  entry-point reshape only; the canvas surface itself (V1.70 / V1.71 /
  V1.75) is preserved verbatim.
- **Multi-workspace UI / workspace switcher** — out of scope; the wizard
  defaults the workspace and the existing single-workspace model holds.
  Multi-workspace is a separate future iteration.
- **AgentProfile CRUD UI** — the wizard picks a default agent; the
  existing CLI/path config remains the authoritative editor for power
  users. A full AgentProfile management UI is deferred.
- **Mandatory creator bootstrap** — the wizard creates a "Default Creator"
  if none exists, but does not force a creator-creation step on existing
  installs that already have one.
- **Cross-platform desktop builds (Windows / Linux)** — V1.66's macOS-only
  desktop target remains; `dirs::document_dir()` is cross-platform but the
  Tauri build target does not expand in this iteration.
- **Browser-build wizard behaviour** — the wizard and per-launch daemon gate are **desktop-first**. Browser users (who already run their own daemon and hit the UI via Vite dev or static serve) default `setup_completed=true` and skip the wizard. The browser build must not regress (daemon-ready splash is a no-op or instant pass; no Tauri commands assumed).
- **Full multi-creator CRUD / profile management** — the footer switcher only allows selecting an existing Creator or adding one via the existing daemon endpoint. Rename, delete, avatar upload, and advanced profile settings are out of scope (future iteration after the switcher proves the model).
- **Agent detection during non-first-launch** — the scan endpoint exists and is used by the wizard; it is not re-run on every subsequent launch. Agent change after setup is a power-user Settings action (deferred).
- **Daemon crash during wizard step 2** — the wizard step-2 daemon gate inherits the same error surface as the per-launch gate; detailed recovery flows (e.g. "kill port 8420 process") are specified at the gate level but the exact copy is left to P1 visual implementation as long as the three invariants in §5 are met.

## 7. Roadmap Position / Next Iteration Transition

- **Current iteration (V1.94)** — **delivered**: the desktop app's first-impression gap is closed. Authors launching the app for the first time walk a 4-step setup wizard (workspace + daemon-ready + ACP agent detection + done); returning launches gate on daemon readiness before entering the main UI; the sidebar IA is restructured into two tabs (Creator | Orchestrator) with nested nav + footer profile switcher; the daemon status bar shows only a restart-icon when running (crashes surface a top banner); buttons across the app use background-driven contrast (with a `tailwind-merge` fix that also corrected a latent typography regression across 181 call sites); Presets + Strategy are unified at `/strategies`. Additive `POST /v1/daemon/agent-host/scan` endpoint; `@42ch/nexus-contracts` 0.20.0 → 0.21.0. QC 3/3 Approve (twice — initial + post-audit revalidation); QA Pass. 10 residuals open going forward, all V1.95.
- **Next iteration (V1.95) transition criteria**:
  - Trigger: V1.94 merged to `main`; integration branch retired.
  - Selection input: PM reviews post-V1.94 real-usage feedback. Candidates:
    - **R-V192SEC-001 — TOFU transport-binding (medium security)**: the strongest open security item; desktop Tauri reqwest+rustls cert pinning against `pinnedFingerprint`. Likely headline.
    - **R-V193PL-001 — `/v1/local/*` path-literal spec hygiene (low)**: cheap spec sweep; companion.
    - **V1.94-QC residual cluster** (R-V194QC1-S101..S106 + R-V194QC2-S001/S002 + R-V194QC1R-S001): 9 low items; some are quick surgical fixes, others are design-token passes.
    - **Post-V1.94 author feedback** — the author flagged "more feedback coming once the daemon reliably starts". The first wave of that feedback lands once V1.94 ships.
    - **tailwind-merge / design-token-classification compound doc** — capture the V1.94 root-cause lesson as a standalone knowledge doc.
- **Long-term goal**: Nexus gives authors a local-first creative workspace where the first launch "just works" — daemon, workspace, agent, and creator identity come together without CLI prerequisites. V1.94 closes the setup gap + corrects a latent typography regression; V1.95+ can return to depth (security, canvas, BL-09 maturation dashboard) on a foundation that real authors can actually enter. STRATEGY Principle #1 ("local-first privacy") and #4 ("leverage, don't burden") are both reinforced — the wizard detects the user's existing local ACP agents rather than imposing a Nexus-owned runtime.

## 8. Delivery Branch Policy

> Mirror of frontmatter; keep in sync with `{HARNESS_DIR}/status.json` `metadata`.

| Field | Value |
|-------|-------|
| `iteration_base_branch` | `main` |
| `spec_integration_branch` | `iteration/v1.94` |
| `target_branch` | `main` |

Per-plan topic branches:

| Plan | Working branch | Merge target |
|---|---|---|
| P-1 | `feature/v1.94-prepare` | `iteration/v1.94` |
| P0 | `feature/v1.94-backend-setup-runtime` | `iteration/v1.94` |
| P1 | `feature/v1.94-frontend-onboarding-ia` | `iteration/v1.94` |
| P-last | `feature/v1.94-closure` | `iteration/v1.94` |

**Worktree isolation**: required for P0 ‖ P1 (same-repo parallel writers:
`crates/` + `apps/desktop/src-tauri/` + `apps/nexus42/` vs `apps/web/`).
P-1 must land on `iteration/v1.94` before P0/P1 topic branches are cut
(the agent-scan contract + the IA spec are the implement contracts). P-last
runs after both merge.

## 9. Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| PATH scan executes arbitrary binaries from the user's PATH | Medium | Medium | The scan probes `--version` for known registry-listed binary names only (no user-supplied commands during scan); bounded concurrency + short timeout; qc2 reviews the execution boundary. Documented in `desktop-shell.md` §ACP Agent Detection. |
| `setup_completed` marker drift / corruption prevents wizard re-trigger | Low | Medium | The marker is a simple `bool` in `~/.nexus42/config.toml`; Settings exposes a "Re-run setup" action that clears it. Missing marker = first-launch semantics (fail-safe to wizard). |
| Per-launch daemon-ready gate re-introduces the "Daemon starting…" hang if the daemon fails to boot | Medium | High | The launch gate inherits the existing `HEALTH_START_TIMEOUT` (15s) and error/degraded state machine; on boot failure, the user lands in a setup-wizard-like error step with the daemon's port-conflict / crash detail and a Restart CTA — never a silent hang. |
| Dark-primary token change shifts the visual identity | Low | Low | Architect-owned token amendment in DESIGN.md; the brand-cyan stays as the bg, only text shifts to white. qc1 reviews the visual delta. |
| Strategies unification regresses V1.70–V1.75 canvas routes | Low | Medium | The canvas surface itself is preserved; only the entry route changes from `/strategy` to `/strategies/:presetId`. Existing tests stay green via redirect or alias. |
| Sidebar IA rewrite breaks mobile (<lg) nav parity | Medium | Low | Mobile nav keeps the two-tab structure as a top dropdown/pill scroller; WCAG keyboard paths preserved (apps/web/AGENTS.md accessibility floor). |
| Existing V1.66–V1.93 adopters with workspaces under `~/.nexus42/` get force-migrated | Low | Medium | No forced migration; the `~/Documents/nexus42/default/` default applies only when `workspace_path` is unset. Existing config.toml values are preserved verbatim. |
| Profile switcher confuses authors who only have one Creator | Low | Low | Single-Creator case: footer shows one avatar + "+"; switching is a no-op when there's only one. The "+" remains the only call-to-action. |

## Compound Round Summary

- Knowledge docs **updated** (1):
  - `architecture-patterns/nexus-brand-token-hierarchy.md` — V1.94 clarified that the contrast rule is mode-independent (background decides text color, not mode). V1.83's original "cyan fill + deep-blue text" was correct; the V1.94 fix-wave's temporary "cyan fill + white text" was a regression based on a misread of the user's defect report. Final form preserves V1.83 + adds the mode-independence clarification.
- Knowledge docs **new** (2):
  - `architecture-patterns/daemon-ready-gate-pattern.md` — single source of truth (SidecarManager) + multiple observers (setup wizard step 2 + per-launch splash + crash banner) of `onDaemonStatusChanged`; avoid `is_daemon_ready()` commands.
  - `architecture-patterns/local-environment-scan-safety-boundary.md` — five normative constraints for any "scan local environment for installed tools" feature (registry-known names only; bounded concurrency; ≤2s timeout; no shell expansion; no user-supplied commands during scan).
- New CONCEPTS.md entries: **0** (the 3 V1.94 terms — Setup Wizard, ACP Agent Detection, Profile Switcher — were added during the Phase-1 review chain).
- compound-refresh triggered: **no** (the brand-token-hierarchy update extends + corrects; no older doc contradicted or superseded beyond V1.83's now-clarified contrast rule).
- **Highest-value compound candidate not written this round** (deferred to V1.95 or standalone): the `tailwind-merge` custom-token-classification root cause. The fix corrected both the immediate "primary button not white" symptom AND a latent typography regression across 181 occurrences in `apps/web/src/`. Worth a dedicated knowledge doc on "custom Tailwind token registration in tailwind-merge" — but the iteration was already long; the test (`utils.test.ts`) + the qc1 second-revalidation report pin the lesson.

## Iteration Retrospective (minimal)

- **Went well**:
  - Grill-me locked 8 decisions in one pass; the structured 5-defect → 8-decision mapping kept scope honest (no feature creep into security / spec-hygiene / BL-09).
  - P-1 (architect) pre-cooked the spec + DESIGN.md + schemas during Phase-1 review chain, so P0 + P1 ran cleanly on parallel worktrees against a frozen contract.
  - The user's mid-Phase-3 clarification ("background-driven, not mode-driven") triggered a focused audit that found both the conceptual error AND the real tailwind-merge root cause — turning a cosmetic-button-defect iteration into one that also closes a latent typography regression across 181 call sites.
  - Worktree isolation for P0 ‖ P1 was clean (disjoint trees); zero merge conflicts.
- **Could improve**:
  - The V1.94 fix-wave's F-004 (button contrast snapshot) encoded the WRONG rule ("dark mode → white text") because the PM misread the user's original "深色背景（primary）按钮里文字没用白色" as a dark-mode complaint rather than a background-driven rule. The snapshot test pinned the wrong rule. Lesson: when a defect report mentions "dark", disambiguate whether it means "dark mode" or "dark background color" before locking the fix.
  - P1 subagent returned a Completion Report claiming "Done" with files modified but uncommitted in the worktree — PM had to commit on behalf of the track. This is a recurring subagent hygiene issue (V1.92 saw the same with qc3). Worth adding to dispatch-and-assignment rules: "subagent must commit + push before returning Completion Report v2".
  - The tailwind-merge root cause was invisible to the QC tri-review's initial pass — qc1 only caught the symptom (snapshot would have pinned the wrong rule). The real bug needed a runtime repro. Lesson: design-system audits benefit from a "run the actual class merger" step, not just source-text inspection.
- **Next-iteration suggestion**: V1.95 should pick up the deferred residuals — R-V192SEC-001 (TOFU transport-binding, medium security — the strongest open item) as headline, with R-V193PL-001 (path-literal spec hygiene) + R-V194QC1-S* (V1.94 QC suggestions) as cheap companions. Post-V1.94 author feedback (more UX edges once the daemon reliably starts) is also a candidate.
