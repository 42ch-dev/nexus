---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-14-v1.46-author-desk-status-ux"
verdict: "Request Changes"
generated_at: "2026-06-14"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: kimi-for-coding/k2p7
- Review Perspective: Performance and reliability risk — hot-path overhead, resource lifecycle, back-pressure on degraded paths, retry/error semantics, observability, `findings_stale` opt-in perf cost, novel-only gate avoiding wasted work, daemon responsiveness.
- Report Timestamp: 2026-06-14T15:55:00+08:00

## Scope
- plan_id: `2026-06-14-v1.46-author-desk-status-ux`
- Review range / Diff basis: `merge-base: de30a702 → tip: c9fb1abb (5 commits on iteration/v1.46; equivalent to git diff de30a702..c9fb1abb or git show --stat de30a702..c9fb1abb)`
- Working branch (verified): `iteration/v1.46`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 3
- Commit range (if not identical to Review range line, explain): identical to Review range line
- Tools run:
  - `git rev-parse --show-toplevel`
  - `git branch --show-current`
  - `git log -1 --oneline`
  - `git diff de30a702..c9fb1abb --stat`
  - `git show --stat de30a702..c9fb1abb`
  - `cargo clippy --all -- -D warnings`
  - `cargo test -p nexus42 --lib -- 'works::tests'`
  - `cargo test -p nexus42 -- works_status`
  - `cargo +nightly fmt --all --check`

## Findings

### 🔴 Critical
None.

### 🟡 Warning

- **F-001 — Sequential I/O on `works status --json` hot path for novel works**
  - **Triggering condition**: `handle_status` JSON branch calls `fetch_open_findings` and then `/v1/local/findings/stale` sequentially for every `work_profile=novel` status request.
  - **Impact**: Latency stacks. Worst-case wait is ~5 s (findings timeout) + ~30 s (stale default timeout) = ~35 s before the CLI returns, even though the two requests are independent. This is a hot-path regression for authors/agents polling `works status --json`.
  - **Suggested fix**: Parallelize the independent subrequests with `tokio::join!` (or `futures::join!`) inside the `is_novel` block, then pass both results to `enrich_status_json`. Keep the shorter timeout on findings and consider applying the same short timeout to the stale fetch.
  - **Source reference**: `crates/nexus42/src/commands/creator/works/mod.rs:374-392`
  - **Confidence**: High

- **F-002 — Asymmetric timeout on `findings_stale` fetch provides weak back-pressure**
  - **Triggering condition**: The stale fetch uses the default `DaemonClient` request timeout (`DEFAULT_REQUEST_TIMEOUT = 30 s`), while `fetch_open_findings` explicitly caps at `FINDINGS_FETCH_TIMEOUT = 5 s`.
  - **Impact**: A degraded `/v1/local/findings/stale` endpoint can block the status command up to 30 s, six times longer than the findings endpoint. The human path already had this asymmetry, but extending it to the new JSON path amplifies the reliability risk for machine consumers.
  - **Suggested fix**: Use a dedicated short-timeout client for the stale fetch in the JSON branch (consistent with `fetch_open_findings`), or reuse the findings-timeout client. Document the chosen timeout policy in the spec §4.1.
  - **Source reference**: `crates/nexus42/src/commands/creator/works/mod.rs:385-388`; `crates/nexus42/src/api/daemon_client.rs:40-43`
  - **Confidence**: High

- **F-003 — JSON `findings[]` truncation at 50 items is not surfaced to consumers**
  - **Triggering condition**: `fetch_open_findings` applies `FINDINGS_FETCH_LIMIT = 50` and returns the daemon's truncated page verbatim. `enrich_status_json` inserts that array into the JSON output without any `truncated`, `total_count`, or `next_page` metadata.
  - **Impact**: JSON consumers cannot distinguish "exactly 50 open findings" from "50+ open findings". This silently breaks the contract expectation that the array is a complete representation of open findings for the Work.
  - **Suggested fix**: Either (a) add a `findings_truncated: true` flag (or `findings_total_count` field) to the JSON output when `findings.len() == FINDINGS_FETCH_LIMIT`, or (b) document the 50-item cap explicitly in spec §4.1 and expose it. Prefer (a) for machine consumers.
  - **Source reference**: `crates/nexus42/src/commands/creator/works/mod.rs:1073-1112`, `1129-1157`
  - **Confidence**: High

### 🟢 Suggestion

- **S-001 — Plan verification command matches zero tests**
  - **Improvement**: The plan's Verification block lists `cargo test -p nexus42 -- works_status`, which runs 0 tests. Update it to the actual filter used for validation (`cargo test -p nexus42 --lib -- 'works::tests'`) or add an integration test named `works_status`.
  - **Source reference**: `.mstar/plans/2026-06-14-v1.46-author-desk-status-ux.md:69-71`
  - **Confidence**: High

- **S-002 — JSON-path findings degradation lacks observability**
  - **Improvement**: When `fetch_open_findings` returns `Unavailable` or the stale fetch fails, the failures are silently swallowed. Add `tracing::debug!` (or `tracing::warn!`) logging the endpoint, work_id, and elapsed time so operators can diagnose daemon degradation without changing the user-facing contract.
  - **Source reference**: `crates/nexus42/src/commands/creator/works/mod.rs:380-388`
  - **Confidence**: Medium

- **S-003 — Consider caching or skipping stale fetch when findings fetch already failed**
  - **Improvement**: If `fetch_open_findings` reports `Unavailable`, the code still issues a second request to `/v1/local/findings/stale`. Under daemon overload, skipping the stale call when findings are unavailable would further reduce hot-path overhead.
  - **Source reference**: `crates/nexus42/src/commands/creator/works/mod.rs:379-389`
  - **Confidence**: Low

## Source Trace

- **Finding ID**: F-001
  - Source Type: manual-reasoning
  - Source Reference: `crates/nexus42/src/commands/creator/works/mod.rs:374-392`
  - Confidence: High

- **Finding ID**: F-002
  - Source Type: manual-reasoning
  - Source Reference: `crates/nexus42/src/commands/creator/works/mod.rs:385-388`; `crates/nexus42/src/api/daemon_client.rs:40-43`
  - Confidence: High

- **Finding ID**: F-003
  - Source Type: manual-reasoning
  - Source Reference: `crates/nexus42/src/commands/creator/works/mod.rs:1073-1112`, `1129-1157`
  - Confidence: High

- **Finding ID**: S-001
  - Source Type: manual-reasoning
  - Source Reference: `.mstar/plans/2026-06-14-v1.46-author-desk-status-ux.md:69-71`; command `cargo test -p nexus42 -- works_status` (0 tests run)
  - Confidence: High

- **Finding ID**: S-002
  - Source Type: manual-reasoning
  - Source Reference: `crates/nexus42/src/commands/creator/works/mod.rs:380-388`
  - Confidence: Medium

- **Finding ID**: S-003
  - Source Type: manual-reasoning
  - Source Reference: `crates/nexus42/src/commands/creator/works/mod.rs:379-389`
  - Confidence: Low

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 3 |

**Verdict**: Request Changes

**Rationale**: All three Warning findings are directly within the performance/reliability focus of this review seat:
- F-001 adds stacked sequential HTTP latency on a frequently-polled JSON hot path.
- F-002 leaves the new JSON path exposed to a 30-second blocking stale endpoint, inconsistent with the 5-second findings timeout.
- F-003 silently truncates the machine-readable `findings[]` array without metadata, creating a contract reliability hazard.

No Critical findings were identified. The novel-only gate correctly short-circuits before any findings work for non-novel works, satisfying Grill #6. CI gates (`cargo clippy --all -- -D warnings`, `cargo test -p nexus42 --lib -- 'works::tests'`, `cargo +nightly fmt --all --check`) all pass.
