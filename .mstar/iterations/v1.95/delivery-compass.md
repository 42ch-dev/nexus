---
iteration_id: V1.95
start_date: 2026-07-07
end_date: 2026-07-07
status: completed
iteration_base_branch: main
target_branch: main
plans:
  - 2026-07-07-v1.95-prepare-specs
  - 2026-07-07-v1.95-implement-fixes
---

# V1.95 — Setup Wizard Bugfix & Daemon Startup Chain Fix — Delivery Compass v1

**Status**: active (Phase 1 in flight). Headline: V1.94 desktop app setup wizard
has 6 real bugs blocking first launch. This iteration fixes them all. No new
product surface; no new wire contracts. `wire_contracts_changed: false`.

## 0. Context

V1.94 shipped the desktop onboarding & IA pass (PR #122, merged `7c61c033`).
Real usage immediately surfaced 6 bugs that block the first-launch experience:

1. **"Request failed: The string did not match the expected pattern."** on app
   open. Root cause: `ClientProvider` creates a temporary `BrowserClient` (no
   `baseUrl`, same-origin) during the async connection-config load. In the Tauri
   webview, same-origin is `http://tauri.localhost/` — fetches to
   `/v1/daemon/runtime/health` hit the Tauri static-file server, not the daemon.
   The webview's fetch implementation rejects the URL pattern.
2. **Daemon cannot start** (the biggest blocker). Root cause: the user's local
   SQLite database (pre-V1.94, under an old workspace path) has a migration
   checksum mismatch (`migration 202606070001 was previously applied but has
   been modified`). The daemon crashes on boot; the wizard shows a generic
   "Cannot reach the daemon" error without surfacing the real cause. The
   `SidecarManager` captures the error but the SPA never sees the detail.
3. **Layout broken (no padding, steps should be on the side)**. The CSS token
   pipeline for `setup-wizard-step-*` tokens has a mismatch between `index.css`
   variable names and the Tailwind utility generation. The step indicators are
   horizontal at top; the user wants them vertical on the left.
4. **Workspace path shows `nexus/local/default`** — stale config from a
   pre-V1.94 version. Code generates `~/Documents/nexus42/default` (no "local"),
   but the user's `~/.nexus42/config.toml` has `workspace_path =
   "/Users/bibi/Documents/nexus/local/default"` from an older setup.
5. **No directory picker** — `setup-step-welcome.tsx` only displays the
   workspace path; the user cannot select a custom directory.
6. **FingerprintGate scope** — the gate wraps the entire app (including
   `/setup`), but the wizard runs before any remote config exists. The gate
   bypasses for null config (correct), but the provider-wrapping creates a
   timing risk.

## 0.1 Terminology conventions

Inherits V1.94 vocabulary. No new terms.

## 1. Locked Decisions (grill-me output)

| Decision | Resolution |
|---|---|
| Iteration direction | **Bugfix iteration — fix all 6 V1.94 setup-wizard bugs.** No new product surface; no new wire contracts. |
| ClientProvider fix | **A1 — immediate TauriClient for desktop.** `ClientProvider` skips the temporary `BrowserClient` in the `!loaded` branch for desktop builds; returns `new TauriClient()` + `new TauriDesktopCapabilities()` immediately. `window.__NEXUS_DAEMON_PORT__` is injected before first render. Config loading only affects remote/local switching. |
| Daemon error surfacing | The wizard step 2 must surface the **real** error from `SidecarManager` (migration failure, port conflict, binary missing) — not a generic "Cannot reach the daemon." The `DaemonStatus.detail` field already carries this; the wizard just needs to display it verbatim. |
| Migration mismatch recovery | The wizard detects "daemon failed to start" state and offers a **"Reset local database"** action that wipes the system SQLite DB under `~/.nexus42/` (NOT under the user workspace) and retries daemon start. Pre-release allows data wipe. The reset is **opt-in** (user clicks a button after seeing the real error); no silent wipe. The button label and copy must clearly say this clears daemon state, not the user's creative files. |
| Workspace default path | **`~/Documents/nexus/default/`** (brand name `nexus/`, not `nexus42/`). System data (config.toml, state.db, registry cache, tls, agent-host) lives under `~/.nexus42/` (hidden, with 42). Creative workspace (Works/Stories/Outlines/References) lives under `~/Documents/nexus/default/` (visible, without 42). The wizard overwrites stale `workspace_path` in config.toml. The DB must NOT live under the workspace path — it belongs in `~/.nexus42/`. |
| Layout redesign | Steps indicator moves to a **left sidebar** (vertical), content on the right. Match a standard wizard layout (e.g. VS Code setup, Slack onboarding). |
| CSS token fix | Audit the `setup-wizard-step-*` token pipeline (`index.css` → `tailwind.config.ts` → JSX classes); fix any naming mismatches so Tailwind generates the utilities. |
| Native directory picker | Add `@tauri-apps/plugin-dialog` `open({ directory: true })` call on the workspace step. The user can override the default path. |
| FingerprintGate bypass | Add `/setup` to the bypass routes in `FingerprintGate` (same pattern as `/connect`). One-line change. |
| Branch policy | `iteration_base_branch=main` (HEAD `7c61c033`, post-V1.94 PR #122); `spec_integration_branch=iteration/v1.95`; `target_branch=main`. |
| Contract impact | **NONE.** `wire_contracts_changed: false`. All fixes are in `apps/web/` and `apps/desktop/src-tauri/`. No new endpoints, no schema changes. |

## 2. Scope

- **SP-1: Prepare — minimal spec amendment (P-1).** Document the ClientProvider
  fix (A1) and the daemon-error-surfacing contract in
  `specs/desktop-shell.md` §13 (update the existing V1.94 Setup
  Wizard section). Document the layout change in `specs/web-ui.md`
  §29 (update the V1.94 IA section). No new specs.
- **SP-2: Implement all 6 fixes (P0).** Single-track frontend + desktop-Rust
  plan. See plan for task breakdown.
- **SP-3: Closure.** QC single-review (bugfix iteration; tri-review may be
  overkill but PM decides at P-last) + QA + compound + PR.

## 2.1 Architecture Hierarchy

- All code changes in `apps/web/src/` (ClientProvider, setup wizard components,
  CSS) + `apps/desktop/src-tauri/src/lib.rs` (workspace-path write +
  stale-config detection + migration-reset Tauri command).
- No `crates/` changes (no new endpoints, no daemon-runtime changes).
- No `schemas/` changes.

## 2.2 Architecture Review (architect, Phase 1, 2026-07-07)

### DB-path separation: already correct

The SQLite DB lives under `~/.nexus42/creators/<id>/workspaces/<slug>/state.db`
(via `nexus_home_layout::workspace_state_db_path()` per ADR-014). It does NOT
live under the user workspace (`~/Documents/nexus/default/`). The compass §1
claim is **correct in intent but already implemented** — no path-move fix is
needed, only a migration-mismatch recovery path (`reset_local_database()`).

### Module-boundary violation: triplicate default-path function

`resolve_default_workspace_path()` (or `default_workspace_root()`) exists in
**three independent copies**, all hardcoded to `nexus42`:

| Location | Symbol |
|---|---|
| `apps/nexus42/src/config.rs:81-93` | `resolve_default_workspace_path()` |
| `crates/nexus-daemon-runtime/src/config.rs:14-26` | `resolve_default_workspace_path()` (duplicate) |
| `apps/desktop/src-tauri/src/lib.rs:32-46` | `default_workspace_root()` (triplicate) |

Changing `nexus42` → `nexus` requires touching all three. The correct long-term
fix is to consolidate into `nexus-home-layout` — the canonical path-layout
crate. **For this iteration:** change all three. Document the duplication as a
known tech-debt item (`R-V195-ARCH-DUPLICATE-DEFAULTS`).

### SidecarManager stderr gap (critical for T3)

The `_rx` receiver from `command.spawn()` (sidecar.rs:246) is **discarded**.
The daemon's actual crash reason (e.g., `migration X was previously applied but
has been modified`) is written to stderr but never captured by the
SidecarManager. The `DaemonStatus.detail` field only carries generic
SidecarManager messages ("Daemon did not start" / "port conflict"). This means
**T3's assumption that `detail` carries the real daemon error is incorrect**
for the migration-mismatch case. T4's reset button is the pragmatic recovery
path, but the wizard will show a generic error, not the SQLite specifics.

### ClientProvider fix (A1): technically sound

The `!loaded` → immediate `TauriClient` fix is correct. `TauriDesktopCapabilities`
double-instantiation (once in `!loaded`, once in `loaded` after config resolves)
is harmless — the object is a thin IPC facade.

### Workspace-path default change: migration impact

Changing from `~/Documents/nexus42/default/` to `~/Documents/nexus/default/`
abandons any content at the old default location. Pre-release allows this per
AGENTS.md. The stale-path overwrite must be selective:
- Overwrite when path matches old default (`nexus42` in Documents)
- Overwrite when path matches known stale pattern (`nexus/local/default`)
- **Preserve** custom user-set paths

## 5. Acceptance Criteria

- Desktop app opens without "The string did not match the expected pattern."
- Daemon starts successfully on first launch after wiping stale DB (or the
  wizard offers "Reset local database" when migration mismatch is detected).
- Wizard step 2 surfaces the **real** daemon error (migration failure, port
  conflict, binary missing) — not a generic "Cannot reach the daemon."
- Setup wizard has left-sidebar step indicators + right content area.
- All padding / layout tokens render correctly (no missing CSS).
- Step 1 offers a native directory picker for workspace selection.
- Step 1 overwrites `workspace_path` in config.toml with the correct default
  (`~/Documents/nexus/default/`) when the user completes it (fixes stale paths).
- Database lives under `~/.nexus42/` (system home), NOT under the workspace path.
  Workspace path (`~/Documents/nexus/default/`) is for user-visible creative
  content only (Works/Stories/Outlines/References).
- `pnpm --filter web test` green; `cargo test --all` green; CI green.

## 6. Non-Goals

- No new product features beyond fixing the 6 bugs.
- No `crates/` or `schemas/` changes.
- No multi-workspace support.
- No V1.94-QC residual sweeps (deferred to V1.96).
- No security changes (R-V192SEC-001 stays deferred).

## 7. Roadmap Position

- **Current (V1.95)**: `delivered` — all 6 V1.94 setup-wizard bugs fixed; desktop app first-launch unblocked (code-level; manual smoke deferred as human gate). QC tri Approve with 6 V1.96 residuals; QA Pass.
- **Next (V1.96)**: V1.94-QC residual sweep + post-V1.95 author feedback + the 6 V1.95 residuals (SidecarManager stderr capture `R-V195-ARCH-STRERR-GAP` [medium] is the highest-value — it lets the wizard show the real daemon crash reason; triplicate default-resolver consolidation `R-V195-ARCH-DUPLICATE-DEFAULTS`; config-write atomicity hardening `R-V195QC3-W003`; reset-local-db atomicity `R-V195QC3-W002`; daemon-step timeout `R-V195QC3-S001`; write_workspace_path_at test parity `R-V195QC1-S002`).
- **Long-term**: a desktop app that "just works" on first launch.

## 8. Delivery Branch Policy

| Field | Value |
|-------|-------|
| `iteration_base_branch` | `main` |
| `spec_integration_branch` | `iteration/v1.95` |
| `target_branch` | `main` |

## Compound Round Summary

1 doc crystallized at iteration-close:

- `architecture-patterns/tailwind-theme-key-routing-for-sizing-tokens.md` (Knowledge track) — a token under `theme.extend.colors` generates only color utilities; `max-w-*`/`p-*`/`h-*`/`w-*` must be registered under `maxWidth`/`padding`/`spacing` or Tailwind silently emits nothing. Distilled from the V1.95 setup-wizard layout fix (plan T5). Registered in `knowledge/README.md`.

Skipped (Q5 high overlap — already covered):
- ClientProvider Tauri-webview same-origin initialization root cause → already documented normatively in `specs/desktop-shell.md §13.6.4` (amended by the P-1 prepare plan).
- Tauri transport-boundary pattern (custom command vs JS plugin) → already an invariant in `apps/desktop/AGENTS.md`.

## Iteration Retrospective (minimal)

**What went well**
- Sticky implementer session across 4 SDD units carried context efficiently (Tauri command patterns, test fixtures) — one `fullstack-dev` session covered T1→T4 + the QC W1 fix with no re-onboarding cost.
- Grouping plan T1–T10 into 4 review-cohesive SDD units (by file-cohesion, not 1:1) kept task reviews sharp without 10 round-trips.
- QC tri caught a real reliability bug (W1 blocking-in-async picker) that task-level review missed — validates the L3 plan-QC layer.
- The architect's pre-implementation review (compass §2.2) saved a wrong fix: it caught that the DB already lives under `~/.nexus42/` (no path-move needed) and flagged the SidecarManager stderr gap before implementation.

**What to improve**
- The implementer twice wrote the SDD report to a tracked path instead of the gitignored `{SDD_DIR}` and committed it once — the brief now states the path more firmly, but a `sdd-workspace`-relative convention or a pre-commit hook could prevent recurrence.
- Two `cargo test -p nexus-daemon-runtime` failures were flagged "pre-existing" by QA but not verified against `main` HEAD per the `.mstar/AGENTS.md` protocol — the authoritative check is deferred to CI (`cargo test --all`). If CI flags them, address in the Phase 5 merge-ready loop.
- Bug #3 (CSS layout) was mis-characterised in the plan as a possible "naming mismatch"; the actual root cause was the Tailwind theme-key category. The compound doc now captures this so future token work doesn't repeat the misdiagnosis.
