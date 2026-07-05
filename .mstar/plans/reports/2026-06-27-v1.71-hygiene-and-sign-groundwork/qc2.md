---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-27-v1.71-hygiene-and-sign-groundwork"
verdict: "Approve"
generated_at: "2026-06-28"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1
- Review Perspective: Security and correctness risk
- Report Timestamp: 2026-06-28

## Scope
- plan_id: 2026-06-27-v1.71-hygiene-and-sign-groundwork
- Review range / Diff basis: P1 Hygiene + Sign groundwork changes merged in commit 63e52fa3 on iteration/v1.71 (tip 707aaac4 relative to pre-P1 state 39493026); full landed diff covers daemon spawn/tracing, served-UI smoke script + CI job, admission-gate UI note + test, chapter can_edit_outline path-guard probe + 422 field-error for title, shared sort comparator + --sort CLI, desktop bundle --sign-identity env-gated plumbing + SIGNING.md + CI secret guard, plus 13 residual closures in status.json.
- Working branch (verified): iteration/v1.71
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 32 (per merge 63e52fa3 stat)
- Commit range: 707aaac4 (P1 tip) .. 39493026 (pre-P1 integration)
- Tools run: git diff / log inspection of spawn paths, smoke script, sign plumbing, chapter handler, sort.rs, capabilities handler; cross-reference to plan scope (B1–B3), compass §Track B, and AGENTS.md rules; verification that no schemas/codegen/DTO touched (`wire_contracts_changed: FALSE`); spot-check of residual closure evidence vs landed commits.

## Findings

### 🔴 Critical
None.

### 🟡 Warning
None (no blocking security or correctness defects under the P1 scope).

### 🟢 Suggestion
- **S1 — Smoke script python3 dependency for ephemeral port**: `scripts/served-ui-smoke.sh` uses `python3 -c 'import socket; ...'` when `NEXUS_DAEMON_PORT` is unset. This is acceptable in CI (macos-14 runner has python3) and the script already falls back to the env var, but a pure-bash/netcat or ss fallback would remove the implicit dependency. Add a short comment or `command -v python3` guard with a clear error.
- **S2 — lsof usage in stop_daemon is best-effort and platform-specific**: The unix `lsof -ti :${port}` path in `stop_daemon` and restart is already behind `#[cfg(unix)]` and treated as last-resort after PID file. Consider documenting that on systems without lsof the fallback is a no-op (user must kill manually) and that output parsing assumes one PID per line. Not a correctness bug for the current macOS/Linux CI + dev use.
- **S3 — Sign plumbing delegates actual signing to Tauri**: The `nexus42 desktop bundle` command only forwards `APPLE_SIGNING_IDENTITY` into the pnpm env when supplied (CLI arg or env). The real `codesign` step lives inside `pnpm --filter desktop tauri build`. This is the correct "groundwork" shape, but future readers should know the trust boundary is Tauri's bundler + the macOS keychain. The CI guard (secret presence) + SIGNING.md already make this clear; consider adding a one-line note in `apps/nexus42/src/commands/desktop/mod.rs` rustdoc.
- **S4 — Residual closure evidence is commit-linked but not uniformly cross-referenced**: Most of the 13 closures have direct code changes (chapter guard probe + test, schedule --sort + shared comparator, daemon tracing try_init, smoke script, admission UI note, sign plumbing, etc.). A few (e.g. error-envelope e2e, AGENTS.md rule, backoff log) are small and well-scoped. For auditability, the plan's T17 step (status.json updates) is complete; future hygiene plans could add a one-line "evidence commit" per residual in the plan body.

## Source Trace
- Daemon spawn/tracing safety: `apps/nexus42/src/commands/daemon/mod.rs:205` (Command::new(exe) + args only, Stdio::null, process_group), `crates/nexus-daemon-runtime/src/boot.rs:112` (try_init).
- Served-UI smoke: `scripts/served-ui-smoke.sh` (mktemp HOME, explicit config seeding, trap cleanup, --foreground spawn, curl to localhost only, no secrets).
- CI wiring: `.github/workflows/desktop-build.yml` (new served-ui-smoke job after desktop-build; APPLE_SIGNING_IDENTITY only from secrets).
- Sign groundwork: `apps/nexus42/src/commands/desktop/mod.rs:50` (or_else + filter non-empty), `74` (env only when set), `SIGNING.md`, desktop-build.yml + desktop-release.yml conditional.
- Chapter can_edit_outline + save-error: `crates/nexus-daemon-runtime/src/api/handlers/chapters.rs:167` (to_detail now takes workspace_root + resolve_guarded_path), `181` (can_edit_outline = guard check), `564` (title PATCH → PresetGatesFailed with field_errors), tests for escape path and 422 field error.
- Schedule --sort: `crates/nexus-daemon-runtime/src/api/sort.rs` (parse_sort_terms + new compare_by_terms), handler usage, CLI flag + test update.
- Admission-gate surface: `apps/web/src/pages/capabilities-page.tsx` + `.test.tsx` (notice text only; enforcement is pre-existing daemon logic).
- No shell injection, no credential leakage, no hardcoded secrets, no bypass of workspace path guard.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 4 |

**Verdict**: Approve

**Rationale**: P1 is surgical hygiene + infrastructure groundwork. All security- and correctness-sensitive paths (daemon self-spawn with no shell and proper reaping, tracing try_init for re-entrant starts, throwaway HOME + trap cleanup in smoke script, env-gated signing with zero hard-coded identity, real workspace path-guard probe for `can_edit_outline`, structured 422 field_errors, robust sort parser + shared comparator) are implemented correctly and match the plan/compass acceptance criteria. No new attack surface, no credential exposure, no accidental signing path, and the 13 residual closures are evidenced by the landed commits. The four suggestions are minor polish items (port selection fallback, lsof documentation, sign delegation note, evidence cross-ref) that do not block merge or release.

## Residual Findings (proposed for SSOT)
No new Critical or Warning findings requiring registration. The four Suggestions above can be tracked as low-severity tech-debt if desired (owner @ops-engineer or @fullstack-dev, target V1.72 hygiene pass).

All 13 residuals listed in the plan have been marked `lifecycle: resolved` in `status.json` with matching code changes; spot-check of key items (chapter guard, schedule sort, daemon spawn, smoke, sign, admission UI) confirms the fixes are present and exercised.
