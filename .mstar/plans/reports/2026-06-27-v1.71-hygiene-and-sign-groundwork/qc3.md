---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-27-v1.71-hygiene-and-sign-groundwork"
verdict: "Approve"
generated_at: "2026-06-28T08:55:00Z"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: volcengine-plan/ark-code-latest
- Review Perspective: Performance, reliability, resource lifecycle, CI cost/flakiness
- Report Timestamp: 2026-06-28T08:55:00Z

## Scope
- plan_id: `2026-06-27-v1.71-hygiene-and-sign-groundwork`
- Review range / Diff basis: `394930269d733855dc26e46ac6153b4a23020591..63e52fa322960392c95a4dab8227526be4743cbf` (`63e52fa3^1..63e52fa3`)
- Working branch (verified): `iteration/v1.71`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 32 changed files, with focused review of smoke script, daemon lifecycle, CI jobs, sort paths, shell/file I/O, and UI test waits
- Commit range: `394930269d733855dc26e46ac6153b4a23020591..63e52fa322960392c95a4dab8227526be4743cbf`
- Tools run:
  - `bash -n scripts/served-ui-smoke.sh`
  - `cargo test -p nexus-daemon-runtime api::sort --quiet`
  - `cargo test -p nexus-daemon-runtime --test error_envelope --quiet`
  - `cargo test -p nexus42 commands::daemon::schedule::tests::list_command_parses --quiet`
  - `pnpm --filter web test -- src/pages/capabilities-page.test.tsx src/pages/chapter-page.test.tsx`

## Findings

### 🔴 Critical
_None._

### 🟡 Warning
_None._

### 🟢 Suggestion

- **S-001 — Served-UI smoke cleanup kills any process on the selected port before proving ownership**
  - **Scope:** `scripts/served-ui-smoke.sh:21-26`
  - **Evidence:** Before starting its own daemon, the script runs `lsof -ti tcp:"${PORT}"` and sends `kill` to every returned PID. This is low-risk in CI because the script normally chooses an ephemeral port and then owns the child daemon via `DAEMON_PID`, but it can terminate an unrelated local process when `NEXUS_DAEMON_PORT` is manually set, or in the small race where another process binds the ephemeral port after selection.
  - **Fix:** Prefer failing fast when the chosen port is already occupied, or only kill a process proven to be a previous Nexus smoke daemon owned by this script/test home. Keep the existing child-PID trap cleanup for the daemon started by this run.
  - **Risk if deferred:** Low reliability/operability risk for local runs; not a CI blocker.

- **S-002 — Smoke readiness loop is bounded but slower than necessary on fast failures**
  - **Scope:** `scripts/served-ui-smoke.sh:109-115`
  - **Evidence:** The loop waits up to 30 seconds with one-second sleeps, then the next health `curl` reports failure. This is deterministic and not flaky, but if the daemon exits immediately the script still waits the full readiness budget before surfacing `daemon.log` only through manual inspection.
  - **Fix:** On each retry, check whether `DAEMON_PID` is still alive and, if it has exited, print `daemon.log` and fail immediately. A shorter sub-second poll is also acceptable if paired with an unchanged total deadline.
  - **Risk if deferred:** Low CI ergonomics cost; no correctness impact.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| S-001 | manual-reasoning | `scripts/served-ui-smoke.sh:21-26`; `NEXUS_DAEMON_PORT` override path | High |
| S-002 | manual-reasoning | `scripts/served-ui-smoke.sh:89-115`; daemon PID + readiness loop | High |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 2 |

**Verdict**: Approve

P1 is acceptable from a performance and reliability perspective. The desktop sidecar lifecycle has bounded health probes, bounded restart backoff, a graceful-stop timeout, and process cleanup; daemon spawn avoids shell execution and the tracing change prevents re-entrant subscriber panics; CI adds a macOS served-UI smoke job with a bounded timeout and no secret exposure; schedule sorting is pushed to SQL for schedules and remains small-list in memory for sessions/capabilities; UI tests use deterministic async waits rather than fixed sleeps. The two smoke-script findings are non-blocking local-operability hardening items, with S-001 registered as a low-severity residual because it can affect unrelated local processes when the port is overridden.

## Verification evidence

- Shell syntax: `bash -n scripts/served-ui-smoke.sh` — passed
- Sort tests: `cargo test -p nexus-daemon-runtime api::sort --quiet` — passed
- Error envelope test: `cargo test -p nexus-daemon-runtime --test error_envelope --quiet` — passed; existing unrelated test-target warnings observed
- CLI schedule parse test: `cargo test -p nexus42 commands::daemon::schedule::tests::list_command_parses --quiet` — passed
- Targeted web tests: `pnpm --filter web test -- src/pages/capabilities-page.test.tsx src/pages/chapter-page.test.tsx` — passed

## Residual Findings proposed for SSOT

- `R-V171P1-QC3-001` — Served-UI smoke cleanup kills any process on the selected port before proving ownership; severity `low`; owner `@ops-engineer`; target `V1.72 or V1.71 P-last cleanup`.
