---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-07-07-v1.97-desktop-first-launch-hardening"
verdict: "Approve"
generated_at: "2026-07-08"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1
- Review Perspective: Security and correctness risk
- Report Timestamp: 2026-07-08

## Scope
- plan_id: 2026-07-07-v1.97-desktop-first-launch-hardening
- Review range / Diff basis: merge-base: 070e26f7ede69bc65d344cdb0bb378beca6b3df1 (main, iteration base) + tip: ab618ee99599f10e138cdd7f0fe09bd22958d649 (feature branch HEAD); equivalent to `git diff 070e26f7...ab618ee9`
- Working branch (verified): feature/v1.97-desktop-first-launch-hardening
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 16 (product changes limited to sidecar.rs, capabilities/main.json, desktop-capabilities.ts + test, setup wizard layout files)
- Commit range: ab618ee9..b3d06b48 (core sidecar + IPC + layout; prototype intake + harness metadata excluded from product review per assignment)
- Tools run: git diff (assigned range), read (sidecar.rs full + targeted), grep (sidecar ownership/owned/child, path literals), bash (rev-parse/branch verification, grep for binaries/nexus42 and /Users/bibi under apps/)

## Findings

### 🔴 Critical
- None.

### 🟡 Warning
- None.

### 🟢 Suggestion
- **S-001** (nit, maintainability): `tauri.conf.json` still documents `externalBin: ["binaries/nexus42"]` (build-time artifact path) while runtime `sidecar("nexus42")` + capability `name` use the bare basename. The distinction is correct per Tauri v2, but a one-line comment in `tauri.conf.json` or the sidecar capability block would prevent future confusion between the two "nexus42" identifiers.
  - File: `apps/desktop/src-tauri/tauri.conf.json:34` and `capabilities/main.json:21`
  - Fix (optional): add `// runtime sidecar() name is bare "nexus42"; externalBin here is the bundle artifact path (see sidecar.rs:249 and Tauri v2 shell docs)` or equivalent.

## Source Trace
- Finding ID: (no blocking findings)
- Source Type: manual-reasoning + git-diff
- Source Reference: `git diff 070e26f7..ab618ee9 -- apps/desktop/src-tauri/src/sidecar.rs apps/desktop/src-tauri/capabilities/main.json apps/web/src/lib/nexus/desktop-capabilities.* apps/web/src/pages/setup-*.tsx*`
- Confidence: High (direct code + test inspection)

## Detailed Review (per assignment focus)

**1. Sidecar ownership/correctness**
- `new()` initializes `owned: false`, `child: None`, `state: Stopped`. Verified.
- Attach path (healthy daemon on port): `probe_health` success → `owned = false`; no child handle fabricated. `stop()` early-returns if `!owned`. Verified in `start_with_budget` (lines 239-244) and `stop` (lines 344-348).
- Spawn path: only after successful `command.spawn()` → `child = Some(...)`; `owned = true`. Kill paths (`stop`, health-fail cleanup, budget-exhaust) only act on owned child. Tests: `new_manager_start_attaches_when_health_ready`, `starting_without_child_does_not_suppress_attach`, `error_state_retries_and_attaches_when_health_ready`.
- `handle_crash` and monitor paths also gate on `owned`. Correct.

**2. Starting-no-child correctness**
- Gate in `start_with_budget`:
  ```rust
  if inner.state == DaemonState::Running
      || (inner.state == DaemonState::Starting && inner.child.is_some())
  {
      return Ok(());
  }
  ```
  Explicit `child.is_some()` check. The `Starting && child.is_none()` case falls through and proceeds to attach or spawn. New regression tests cover the exact scenario.

**3. Capability scope security**
- Capability updated: `"name": "nexus42"` (was `"binaries/nexus42"`).
- `args` validator remains: `{"validator": "^\\d+$"}` for the port argument. No broadening.
- Matches the code change at `sidecar.rs:249` (`shell().sidecar("nexus42")`).
- `externalBin` in `tauri.conf.json` remains `"binaries/nexus42"` — this is the *bundler* artifact locator, not the runtime `sidecar(name)` identifier. Scope for `shell:allow-execute` is now correctly paired with the runtime name. No injection surface widened.

**4. IPC correctness**
- `desktop-capabilities.ts:207`: now sends `{ defaultPath }` (camelCase).
- Regression test `desktop-capabilities.test.ts:113-123` explicitly asserts:
  ```ts
  expect(invoke).toHaveBeenCalledWith('pick_directory', { defaultPath });
  ```
- Matches Rust command expectation. Test would have caught the prior `default_path` regression.

**5. Error handling**
- Stderr tail capture (`drain_stderr`, `stderr_tail`, `format_error_detail`) preserved on spawn and health-fail paths.
- Bounded retry: `MAX_RESTART_ATTEMPTS`, `backoff()` with jitter, explicit budget reset on manual `start_daemon`.
- No panics on attach-without-child or new initial `Stopped` paths (all guarded; tests exercise them).
- Health probe and tcp_reachable use bounded timeouts.

**6. Path hygiene / injection**
- No machine-local paths (`/Users/bibi` etc.) introduced into product code or app tests (confirmed via grep under `apps/`; only node_modules/.bin artifacts).
- Sidecar name is now a literal `"nexus42"` (no interpolation).
- Port is a `u16` formatted into args; capability validator `^\d+$` is defense-in-depth.
- No shell command construction from user strings in the changed paths.

**Tests & verification**
- Added: `new_manager_starts_from_stopped_state`, attach-without-ownership, Starting+no-child does not suppress, Error state retry.
- Web: camelCase regression test.
- Layout changes are containment-only (`overflow-hidden`, `min-w-0`, `flex-shrink-0`); scoped to wizard surface; tests updated.
- No schema, wire-contract, or daemon-API changes (as required).

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 1 |

**Verdict**: Approve

## Completion Report v2

**Agent**: qc-specialist-2
**Task**: Plan-level code review (security + correctness) for 2026-07-07-v1.97-desktop-first-launch-hardening
**Status**: Done
**Scope Delivered**: Full review of product diff per assignment checklist (sidecar ownership/Starting gate, capability scope, IPC casing, error handling, path hygiene). Harness metadata, iteration docs, and smoke evidence excluded from product correctness review per explicit instructions.
**Artifacts**: This report only (`qc2.md`).
**Validation**: Direct inspection of `git diff` range + full reads of sidecar.rs, capabilities, IPC files + tests. Branch/cwd/rev verified. No product code mutations.
**Issues/Risks**: None blocking. One low-impact naming-distinction nit (S-001) for future maintainers.
**Plan Update**: None required (no residuals from this review).
**Handoff**: Ready for consolidated QC / QA gate. Smoke carry-over (clean-state no-creator + headless) is acknowledged as PM-tracked, not a code-correctness blocker here.
**Git**: Report staged only for this file (per QC git discipline).
