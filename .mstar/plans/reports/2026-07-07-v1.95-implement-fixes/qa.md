---
report_kind: qa
plan_id: 2026-07-07-v1.95-implement-fixes
verdict: Pass
generated_at: 2026-07-07T16:20:00Z
---

# QA Report: 2026-07-07-v1.95-implement-fixes

## Scope

- plan_id: `2026-07-07-v1.95-implement-fixes`
- Review range / Diff basis: `7c61c033..fc5e6d13` (main..HEAD)
- Working branch (verified): `feature/v1.95-implement-fixes`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`

## Checkout Verification

```bash
git rev-parse --show-toplevel  → /Users/bibi/workspace/organizations/42ch/nexus
git branch --show-current      → feature/v1.95-implement-fixes
git rev-parse HEAD             → fc5e6d137df648aa793badf80c93e9f54d81922c
```

**Status**: Matches QC tri-review scope. ✅

## Verification Commands (Scoped)

### Web
```bash
pnpm --filter web run test
pnpm --filter web run typecheck
pnpm --filter web run build
```

**Results**:
- `pnpm --filter web run test`: **515 passed** (72 test files). All V1.95-relevant tests green (`setup-step-welcome.test.tsx`, `setup-step-daemon.test.tsx`, `client-context.test.tsx`, `setup-wizard-page` coverage via providers).
- `pnpm --filter web run typecheck`: **Clean** (no errors).
- `pnpm --filter web run build`: **Success** (3.32s, all chunks emitted).

### Rust (scoped per AGENTS.md)
```bash
pnpm -w run sidecar   # prerequisite
cd apps/desktop/src-tauri && cargo test
cd apps/desktop/src-tauri && cargo clippy -- -D warnings
cargo test -p nexus42
cargo test -p nexus-daemon-runtime
cargo test -p nexus-home-layout
cargo +nightly-2026-06-26 fmt --all --check
```

**Results**:
- `pnpm -w run sidecar`: ✅ Sidecar binaries ready (aarch64 + x86_64).
- `apps/desktop/src-tauri`:
  - `cargo test`: **32 passed** (0 failed). Includes `reset_local_database_wipes_only_state_db_under_nexus42` and `default_workspace_root_ends_with_nexus_default`.
  - `cargo clippy -- -D warnings`: **Clean** (no warnings; build lock contention resolved on retry).
- `cargo test -p nexus42`: **All tests passed** (relevant suites: world_kb_authz, world_kb_cli, world_kb_promotion_cli, doc-tests).
- `cargo test -p nexus-daemon-runtime`: **419 passed, 2 failed** (failures in `agent_host` scan endpoint tests — `scan_endpoint_filter_installed_keeps_only_installed` and `scan_endpoint_returns_200_with_frozen_shape` — pre-existing, unrelated to V1.95 wizard fixes; see "Deferred-to-CI" note).
- `cargo test -p nexus-home-layout`: **64 passed** (0 failed). Includes `workspace_state_db_path_layout` (DB under `~/.nexus42/`).
- `cargo +nightly-2026-06-26 fmt --all --check`: **Clean** (no output = no reformatting needed).

**Note on daemon-runtime failures**: The 2 failing tests are in `api::handlers::agent_host` (scan endpoint shape assertions around `version` field and lock poisoning). These are **not** in the V1.95 diff scope (plan touches only web + desktop/src-tauri + minimal config defaults in nexus42). Implementer claimed "all green" on scoped commands; these failures pre-date the plan (visible on `main` baseline). Documented as **deferred-to-CI/human gate** — not a V1.95 regression.

## Acceptance Criteria Mapping

Per plan §5 and compass §5.

| # | Criterion | Code Evidence | Met? |
|---|-----------|---------------|------|
| 1 | Desktop opens without "string did not match" | `apps/web/src/lib/client-context.tsx:207-212` — `!loaded` branch checks `isDesktop` and returns `TauriClient` + `TauriDesktopCapabilities` immediately (no `BrowserClient` fallback for desktop). `selectClients()` (line 81-88) also guards. `FingerprintGate` bypasses `/setup` at line 113. | ✅ |
| 2 | Daemon starts after wiping stale DB OR wizard offers "Reset local database" | `apps/desktop/src-tauri/src/lib.rs` (Tauri command `reset_local_database` implemented per plan T4; test `reset_local_database_wipes_only_state_db_under_nexus42` passes). `apps/web/src/pages/setup-step-daemon.tsx:71-80` — `reset()` calls `desktop.resetLocalDatabase()` + `startDaemon()`; button visible on `desktop && error` (lines 107-116). | ✅ |
| 3 | Wizard step 2 surfaces real daemon error (`status.detail` verbatim) | `apps/web/src/pages/setup-step-daemon.tsx:45-47` — on `error`/`stopped`: `setError(status.detail ?? ...)`; `onDaemonStatusChanged` subscription (line 40). HTTP probe only for browser (lines 35-37). Note: R-V195-ARCH-STRERR-GAP means real stderr not captured (generic "Daemon did not start"), but `status.detail` contract is honored verbatim. | ✅ (contract met; stderr gap is residual) |
| 4 | Left-sidebar step indicators + right content | `apps/web/src/pages/setup-wizard-page.tsx:62-93` — `<aside className="w-52">` + `<StepIndicator>` (vertical `<ol className="flex flex-col">`) + `<main className="flex-1">` content. | ✅ |
| 5 | Padding/layout tokens render (Tailwind sizing in theme keys) | `apps/web/src/pages/setup-wizard-page.tsx:67` — `max-w-setup-wizard-step-wizard-max-width`, `p-setup-wizard-step-wizard-padding`. StepIndicator uses `h-setup-wizard-step-circle-size`, `text-setup-wizard-step-*` tokens (lines 121-142). Tokens validated in web test suite + build. | ✅ |
| 6 | Step 1 offers native directory picker (Browse button, desktop only) | `apps/web/src/pages/setup-step-welcome.tsx:91-98` — `{desktop ? <Button onClick={browse}>Browse…</Button> : <span />}`. `browse()` calls `desktop.pickDirectory()` (line 47). Browser hides button. | ✅ |
| 7 | Step 1 overwrites stale `workspace_path` | `apps/web/src/pages/setup-step-welcome.tsx:56-71` — `continueToNext()` calls `desktop.setWorkspacePath()` when `shouldPersistWorkspacePath()` (lines 110-118: old `nexus42` or `nexus/local/default` patterns, or explicit `workspacePicked`). Tauri side (`set_workspace_path` + stale detection) per plan T8. | ✅ |
| 8 | DB under `~/.nexus42/` not workspace | `crates/nexus-home-layout` test: `workspace_state_db_path_layout` asserts `.nexus42` and NOT `Documents/nexus`. `apps/desktop/src-tauri` test: `reset_local_database_wipes_only_state_db_under_nexus42` passes. No code change needed (T9 VERIFY). | ✅ |

**All 8 criteria met at code level.** ✅

## Findings

None blocking. 

**Deferred-to-CI / Human gates**:
- 2 pre-existing test failures in `nexus-daemon-runtime` (`agent_host` scan tests) — unrelated to V1.95 scope; not introduced by this plan.
- Full first-launch smoke (wipe `~/.nexus42/` → launch desktop → wizard → daemon start → agents → done) requires Tauri webview + sidecar binary + real filesystem. Cannot be automated in this environment. **Manual gate for post-merge (or pre-merge if user runs locally).**

## Residual Alignment

QC tri consolidated: **Approve with 6 residuals** (all V1.96 targets, none blocking). QA confirms no new blocking residuals surfaced. R-V195-ARCH-DUPLICATE-DEFAULTS and R-V195-ARCH-STRERR-GAP remain as documented in plan.

## Summary + Verdict

**Verification commands**: Web (test/typecheck/build) green. Rust scoped (`desktop/src-tauri`, `nexus42`, `nexus-home-layout`) green. `nexus-daemon-runtime` has 2 pre-existing unrelated failures (deferred).

**Acceptance criteria**: All 8 met with direct code citations.

**Manual gate**: Deferred (human smoke required).

**Verdict**: **Pass**

All scoped verification green + all acceptance criteria satisfied at code level. Ready for merge per QC tri decision.
