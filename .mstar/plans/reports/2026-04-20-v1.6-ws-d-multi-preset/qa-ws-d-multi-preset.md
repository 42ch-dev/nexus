# QA Report — V1.6 WS-D Multi-Preset Scheduler

**Plan ID**: `2026-04-20-v1.6-ws-d-multi-preset`
**QA Engineer**: @qa-engineer
**Working branch**: `feature/v1.6`
**Review cwd**: `/Users/bibi/workspace/organizations/42ch/nexus`
**Review range**: `git diff ccd6aee..HEAD` (WS-C Done → WS-D HEAD including R1 fix)
**Date**: 2026-04-20

---

## Scope Tested

- Engine startup with `_system/maintenance/` directory: preset registered
- Engine startup with no `_system/` directory: no error
- `nexus42 system preset list` CLI command parsing
- Embedded YAML state machine completion (R1 fix verification)
- `cargo test --workspace` — full workspace test suite
- `cargo clippy --all -- -D warnings` — lint cleanliness
- `cargo +nightly fmt --all -- --check` — format cleanliness

## Acceptance Criteria Checklist

| # | Criterion | Status | Evidence |
|---|-----------|--------|----------|
| 1 | Engine startup with `_system/maintenance/` directory: preset registered | ✅ PASS | 16 unit tests in `nexus-orchestration` pass including `scan_loads_valid_system_presets`, `find_system_preset_by_qualified_id`, `ensure_maintenance_preset_creates_on_first_start`, `ensure_maintenance_preset_does_not_overwrite_existing` |
| 2 | Engine startup with no `_system/` directory: no error | ✅ PASS | `missing_directory_returns_empty` test passes; `scan_skips_corrupted_presets_with_warning` handles graceful degradation |
| 3 | `nexus42 system preset list` shows registered system presets | ✅ PASS | `system_preset_list_parses` and `system_preset_subcommand_required` CLI tests pass; `list_system_preset_ids_returns_qualified_ids` unit test passes |
| 4 | `cargo test --workspace` green; clippy/fmt clean | ⚠️ See findings | Clippy ✅ clean, fmt ✅ clean. 2 pre-existing test failures unrelated to WS-D scope (see below) |
| 5 | Embedded YAML state machine completes (no blocking on `exit_when`) | ✅ PASS | R1 fix verified: `exit_when: { kind: rule }` removed from 3 intermediate states. `system_preset_runs_to_terminal_state` e2e test passes. `embedded_maintenance_yaml_is_valid` passes. |

## Verification Commands & Results

### 1. `cargo test --workspace`

```
nexus42 (lib):   403 passed; 2 failed (pre-existing, see below)
nexus42 (bin):   401 passed; 2 failed (same pre-existing)
nexus-orchestration: 149 passed; 0 failed (all WS-D tests green)
nexus42d:         all passed
All other crates: all passed
```

**2 failures identified — both pre-existing, outside WS-D scope:**

| Test | Root Cause | WS-D Impact |
|------|------------|-------------|
| `auth::tests::get_returns_none_for_unknown_creator` | JSON parse error: "trailing characters" at `auth/mod.rs:240`. Fails at base commit `ccd6aee` and in isolation. `git diff ccd6aee..HEAD -- crates/nexus42/src/auth/` is empty. | None — auth module untouched by WS-D |
| `context::summary::tests::summary_config_from_env_override` | Parallel test env var race: `NEXUS_CONTEXT_MAX_FILE_SIZE` polluted by concurrent test. Passes when run in isolation (`cargo test -p nexus42 --bin nexus42 -- context::summary::tests::summary_config_from_env_override → ok`). Also passes at base commit when run in isolation. | None — test isolation issue, not WS-D regression |

### 2. `cargo clippy --all -- -D warnings`

```
Finished `dev` profile [unoptimized + debuginfo] target(s) in 4.57s
```
**Result: ✅ 0 warnings, 0 errors**

### 3. `cargo +nightly fmt --all -- --check`

```
(no output — exit code 0)
```
**Result: ✅ Clean**

### 4. WS-D Specific Tests

```
nexus-orchestration system_preset tests: 16/16 pass
nexus42 system_preset CLI tests: 2/2 pass
system_preset_e2e::system_preset_runs_to_terminal_state: pass
preset_parse_minimal::system_preset_parses: pass
```

## R1 Fix Verification

QC3 identified Critical R1: `exit_when: { kind: rule }` on intermediate states caused `RuleCheckTask` to return `WaitForInput` when `_rule` context was unset, blocking the state machine after `sync_pull`.

**Fix (commit `d11aa39`)**: Removed `exit_when` from `sync_pull`, `outbox_flush`, and `registry_refresh` states in `EMBEDDED_MAINTENANCE_YAML`. Without `exit_when`, `StateCompositeTask` returns `NextAction::Continue` by default, matching pre-V1.6 `PresetCapabilityTask` behavior.

**Verification**: 
- Diff shows exactly 6 lines removed (3 × `exit_when: { kind: rule }` blocks)
- `system_preset_runs_to_terminal_state` e2e test confirms graph reaches terminal state
- `embedded_maintenance_yaml_is_valid` confirms YAML parses and validates

**Result: ✅ R1 resolved**

## Phase Gate Assessment

| Check | Status | Notes |
|-------|--------|-------|
| Plan exists with tasks | ✅ | T1–T5 all marked `[x]` |
| `clarify` section | ❌ Missing | Plan has no explicit `clarify` section |
| Phase Gate Checklist | ❌ Missing | No formal checklist in plan |
| Residual findings in SSOT | ✅ | No residuals for WS-D in `status.json` |
| Plan status | InReview | Consistent with QA-in-progress |

**Assessment**: Plan lacks formal Phase Gate Checklist (`clarify` + `tasks` sections). This is a documentation gap, not a functional issue. All implementation tasks are complete and verified. PM should decide whether to require a retroactive `clarify` entry or accept as-is.

## Findings

### Pre-existing (outside WS-D scope)

| ID | Severity | Description | Recommendation |
|----|----------|-------------|----------------|
| PF-1 | Medium | `auth::tests::get_returns_none_for_unknown_creator` — JSON parse failure in auth store. Pre-dates WS-D. | Owner: @fullstack-dev. File separate bug. |
| PF-2 | Low | `context::summary::tests::summary_config_from_env_override` — flaky under parallel execution due to env var race. Passes in isolation. | Owner: @fullstack-dev. Consider `#[serial]` attribute or env var cleanup. |

### WS-D Scope

**No findings.** All WS-D acceptance criteria met. R1 fix verified correct.

## Not Tested

- Live daemon startup with actual `_system/maintenance/` directory on disk (no integration test environment configured for this QA session)
- `_system/health-check/` additional preset (plan evidence mentions this but no test explicitly validates multi-preset coexistence in a running daemon)
- `nexus42 schedule start _system.maintenance` end-to-end CLI invocation against a live daemon

## Recommended Owners

- **PF-1 (auth test)**: @fullstack-dev — investigate JSON store corruption in test fixture
- **PF-2 (env var race)**: @fullstack-dev — add `serial_test` or sequentialize env-dependent tests
- **Phase Gate doc gap**: @project-manager — decide whether retroactive `clarify` is required

---

## Sign-off

| Criterion | Verdict |
|-----------|---------|
| WS-D T1–T5 implemented | ✅ Verified |
| System preset directory scanning | ✅ 16 unit tests pass |
| Missing directory graceful handling | ✅ Tested |
| CLI `system preset list` command | ✅ 2 CLI tests pass |
| Embedded YAML state machine (R1 fixed) | ✅ e2e test passes |
| Clippy / fmt clean | ✅ Both clean |
| Pre-existing test failures blocking WS-D | ❌ No — both pre-date WS-D scope |

**Overall verdict: WS-D acceptance criteria PASS** (2 pre-existing test failures outside WS-D scope do not block this plan's sign-off).
