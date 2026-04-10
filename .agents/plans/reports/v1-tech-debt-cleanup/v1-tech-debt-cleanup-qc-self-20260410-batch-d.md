---
report_kind: qc_self
reviewer: implementer-self-qc
reviewer_index: 0
plan_id: v1-tech-debt-cleanup
verdict: Approve
generated_at: 2026-04-10T23:59:59Z
---

# Code Review Report

## Reviewer Metadata

- Reviewer: implementer-self-qc (qc_self)
- Review Perspective: Batch D closure (TD-7–TD-10) + plan Done bookkeeping under harness `/qc-self`
- Report Timestamp: 2026-04-10T23:59:59Z

## Scope

- plan_id: v1-tech-debt-cleanup
- Review range / Diff basis: `be91600..HEAD` on `main` — Batch D implementation, knowledge docs, `status.json` / archived plan + residuals, qc_self report (single closure commit; verify with `git log -1 --oneline`).
- Working branch (verified): main
- Review cwd (verified): Nexus repository root (`git rev-parse --show-toplevel`; no machine-specific path pasted per AGENTS.md)
- Files reviewed: Rust (nexus-domain contract assertions, nexus42d API/runtime/auth), plans, knowledge, reports
- Commit range (if not identical to Review range line, explain): Same as Review range / Diff basis (`be91600..HEAD`).
- Tools run: `cargo +nightly fmt --all`, `cargo clippy -p nexus-domain -p nexus42d -- -D warnings`, `cargo test -p nexus-domain`, `cargo test -p nexus42d`, `pnpm run typecheck`

## Findings

### 🔴 Critical

- None.

### 🟡 Warning

- **TD-9 scope**: `GET /v1/local/daemon/status` reports `lifecycle_state: "running"` whenever the API is up; this is not a full cli-spec §10.1 state machine. Mitigation: `implementation_scope` field + knowledge doc `daemon-lifecycle-api-v1.md` state the gap explicitly for API consumers.

### 🟢 Suggestion

- Consider versioning or extending the daemon status JSON when the real FSM lands (V1.2+) to avoid silent semantic drift for automation that keys only on `lifecycle_state`.

## Source Trace

- Finding ID: W-TD9-SCOPE
- Source Type: manual-reasoning
- Source Reference: `.agents/plans/knowledge/daemon-lifecycle-api-v1.md`, `crates/nexus42d/src/api/handlers/runtime.rs`
- Confidence: High

## Summary

| Severity   | Count |
| ---------- | ----- |
| 🔴 Critical | 0     |
| 🟡 Warning  | 1     |
| 🟢 Suggestion | 1   |

**Verdict**: Approve

## Plan acceptance

| Criterion / task | Done / Partial / Not done | Evidence |
| ---------------- | ------------------------- | -------- |
| TD-7 ForkBranch alignment | Done | `test_fork_branch_parent_branch_and_event_ids_roundtrip`; `knowledge/fork-branch-contract-alignment-v1.md` |
| TD-8 dual outbox consolidation | Not done (waived) | `knowledge/dual-outbox-architecture-v1.md`; `archived/residuals/v1-tech-debt-cleanup-batch-d.json` |
| TD-9 daemon lifecycle | Partial | `GET /v1/local/daemon/status` + tests; full §10.1 FSM deferred per knowledge doc |
| TD-10 real OAuth | Not done (waived) | `device_flow.rs` module docs; `knowledge/device-flow-oauth-scope-v1.md`; batch-d archive |
| Plan Done + compaction | Done | `status.json` slim row; `archived/plans/v1-tech-debt-cleanup.json`; `notes.json` entry |
| Batch E (TD-11, TD-13) | Not done | Explicitly out of scope until V1.2 (unchanged) |

## Verification

- `cargo +nightly fmt --all` — pass (fmt applied)
- `cargo clippy -p nexus-domain -p nexus42d -- -D warnings` — pass
- `cargo test -p nexus-domain` — pass (181 tests)
- `cargo test -p nexus42d` — pass (unit + integration including `daemon_status_endpoint` / `daemon_status_works_without_init`)
- `pnpm run typecheck` — pass

**Note:** Full `cargo test --all` not used as merge gate for this slice (known offline/registry-dependent cases in other crates per prior plan notes).
