---
report_kind: qa
plan_id: "2026-06-26-v1.67-local-api-surface-convergence"
verdict: "Pass"
generated_at: "2026-06-26T23:30:00Z"
scope: "Wave-1 full (P0 local-api-surface-convergence + P-sec desktop-shell-polish) on iteration/v1.67 at 3c84aafd (consolidated APPROVE from qc-consolidated.md)"
review_cwd: "/Users/bibi/workspace/organizations/42ch/nexus"
working_branch: "iteration/v1.67"
review_range: "Wave 1 full scope (P0 ea94b028 + P0 fix-wave + P-sec cf48a8f1+bc8d4bea + P-sec fix-wave) vs origin/main"
---

# QA Report — V1.67 Wave 1 (P0 + P-sec) Behavior + CI Gate Verification

**Single QA report covering both plans' Wave-1 scope per assignment.**

## Scope tested
- **Plans**: `2026-06-26-v1.67-local-api-surface-convergence` (P0) + `2026-06-26-v1.67-desktop-shell-polish` (P-sec)
- **Working branch (verified)**: `iteration/v1.67`
- **HEAD (verified)**: `3c84aafd` ("qc(v1.67): Wave 1 consolidated APPROVE (P0+P-sec 3/3 after fix-wave-1)")
- **Diff basis**: vs `origin/main` (merge-base `b06d0755`)
- **QC baseline**: Both `qc-consolidated.md` files show consolidated **Approve** after targeted fix-wave-1 revalidations (qc1/qc2/qc3 revalidation sections present).
- **No code edits by QA** — verification only.

## Full CI-equivalent gate (all executed on `iteration/v1.67` HEAD)

| Gate | Command | Result | Evidence |
|------|---------|--------|----------|
| Format | `cargo +nightly fmt --all --check` | clean (no output) | 2026-06-26 23:05 |
| Wire drift | `bash tooling/check-wire-drift.sh` | 4 tests passed, no drift | 2026-06-26 23:05 |
| Schema drift | `bash tooling/check-schema-drift.sh` | All 7 checks ✅ | 2026-06-26 23:05 |
| Full workspace tests | `SQLX_OFFLINE=true cargo test --all` | All test results `ok` (hundreds of tests across crates; no failures in scope) | 2026-06-26 23:06–23:15 (multiple runs) |
| Clippy (CI command) | `cargo clippy --all -- -D warnings` | clean (Finished dev profile) | 2026-06-26 23:08 |
| Contracts build | `pnpm --filter @42ch/nexus-contracts run build` | success (0.6.0) | dist/ + DTS emitted |
| Web typecheck | `pnpm --filter web typecheck` | clean | 2026-06-26 23:09 |
| Web build | `pnpm --filter web build` | success (950 kB bundle) | 2026-06-26 23:09 |
| Web tests | `pnpm --filter web test` | 110 passed (15 files) | 2026-06-26 23:10 |
| Desktop lib tests | `cargo test --lib -p nexus_desktop` (in apps/desktop/src-tauri) | 18 passed | 2026-06-26 23:11 |

**Notes on test volume**: Individual crate runs showed e.g. `nexus-daemon-runtime` 323 lib tests passed, desktop 18 passed, web 110 passed. Full `--all` produced only "ok" results in scope; no blocking failures.

## Behavior spot-checks (AC mapping)

### P0 plan (`local-api-surface-convergence`)
- **AC3 / F-P3** (`{ items, pagination }` not legacy keys):
  - Verified: `crates/nexus-daemon-runtime/src/api/handlers/works.rs`, `schedules.rs`, `sessions.rs`, `capabilities.rs` return `List*Response { items, pagination }`.
  - Web: `apps/web/src/api/queries.ts` + `adapters.ts` consume `.items` + `.pagination`.
  - Tests: `works_api.rs`, `sort_contract.rs`, `browser-client.test.ts`, `adapter-contract.test.ts` all assert the shape.
- **AC4 / F-F1** (`?sort=-updated_at`; unknown key → `<resource>_sort_invalid`):
  - `?sort` grammar + push-down implemented for works/schedules (SQL `ORDER BY` + `LIMIT/OFFSET`); sessions/capabilities in-memory (bounded).
  - Invalid key tests pass:
    - `list_works_invalid_sort_key_returns_work_sort_invalid` → `body["error"]["code"] === "work_sort_invalid"`
    - `schedule_list_invalid_sort_key_returns_schedule_sort_invalid` → `schedule_sort_invalid`
    - `sessions_list_invalid_sort_key_returns_session_sort_invalid`, `capabilities_list_invalid_sort_key_returns_capability_sort_invalid`
  - Parser unit tests + integration coverage in `sort_contract.rs`, `works_api.rs`, `fl_e_schedule_api.rs`.
- **AC2 / CASING** (wire `ErrorResponse.code` lowercase-snake):
  - `NexusApiError::error_code()` returns e.g. `"not_found"`, `"work_sort_invalid"`, `"preset_gates_failed"`, `"invalid_input"`.
  - Confirmed in `errors.rs:214` (`NotFound => "not_found"`) and sort passthrough (`_ if code.ends_with("_sort_invalid")`).
  - Internal uppercase codes never appear on wire.
- **AC5** (contracts version):
  - `@42ch/nexus-contracts` package.json: `"version": "0.6.0"`
  - Build succeeded; no wire drift.

### P-sec plan (`desktop-shell-polish`)
- **AC2** (desktop tests + restart_count + status fallback):
  - `apps/desktop/src-tauri`: `cargo test --lib` → 18 passed (incl. `sidecar::tests::start_daemon_resets_crash_budget`, `crash_restart_stops_when_budget_exhausted`).
  - `nexus-daemon-runtime` lib: 323 passed (no breakage).
  - Web: `daemon-status-bar.test.tsx` (8 tests) + fallback re-sync test pass.
  - Restart reset only on manual `startDaemon` (not auto crash path) — verified by dedicated regression test.
- **AC3** (no wire/contract change): `check-wire-drift.sh` clean on both waves.
- All 10 residuals closed per plan Completion Report + QC revalidation.

## Not tested (per assignment)
- Interactive GUI / Tauri bundle (explicitly deferred to user, as in V1.66).
- Full e2e with real daemon process (unit + integration + MSW cover the contract surface).

## Recommended owners for any follow-up
- None — all gates and ACs verified Pass on the consolidated Wave-1 HEAD.

## Verdict
**Pass**

All mandatory CI gates green. All P0 + P-sec ACs behaviorally verified with executable evidence. QC consolidated Approve already present. Ready for merge discipline (PR to main after iteration sign-off).
