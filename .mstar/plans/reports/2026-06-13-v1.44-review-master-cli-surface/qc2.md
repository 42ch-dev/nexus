---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-13-v1.44-review-master-cli-surface"
verdict: "Approve"
generated_at: "2026-06-13"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: security and correctness risk (focus: `--finding-id` validation/scoping, `--auto-schedule` opt-in safe-default, daemon route input sanitization, conflict-resolution correctness vs `stage advance --stage review`, creator-scoped access on all paths)
- Report Timestamp: 2026-06-13T12:10:00+08:00

## Scope
- plan_id: 2026-06-13-v1.44-review-master-cli-surface
- Review range / Diff basis: 9d471bdc..c54b1aa6 (verbatim — identical across qc tri-review)
- Working branch (verified): iteration/v1.44
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 7 (run.rs + new hermetic test + quickstart + 3 spec amendments + status.json)
- Commit range: 9d471bdc..c54b1aa6 (a7b70ebf feat(P1-T1) + 031da6ae docs(P1-T4+T5) + 73850ddb test(P1-T6) + c54b1aa6 merge)
- Tools run:
  - `git branch --show-current && git rev-parse --show-toplevel && git log --oneline 9d471bdc..c54b1aa6`
  - `git diff 9d471bdc..c54b1aa6 --stat && git diff 9d471bdc..c54b1aa6 --name-only`
  - `cargo clippy -p nexus42 -- -D warnings` (clean)
  - `cargo test -p nexus42 --test review_master_cli` (5 passed)
  - `cargo +nightly fmt --all -- --check` (clean)
  - Full diff review of `handle_review_master`, daemon findings handlers (list/get/stale), local-db `findings::{list_findings,get_finding}`, preset enqueue paths, quickstart/spec/status changes.
- Out of scope (per assignment): P0 (manuscript-audit), P2 (multi-volume), any daemon handler changes outside the reviewed CLI caller surface.

## Findings

### 🔴 Critical
None.

### 🟡 Warning

- **W1 (finding_id scoping / input validation — correctness of "scoped to one finding" claim)**:  
  `handle_review_master` (when `--finding-id <fid>` is supplied) performs `GET /v1/local/works/{work_id}/findings/{fid}` then unconditionally serializes the returned finding into the `novel-review-master` preset input as `open_findings: [ <full finding dto> ]` and enqueues.  
  The daemon `get_finding_handler` correctly does `works::get_work(creator_id, work_id)` (404 if not owned) + `findings::get_finding(creator_id, finding_id)` (creator-scoped). No path traversal or SQL injection.  
  However, there is **no re-validation** in the CLI (or exposed server filter) that the fetched finding has `target_executor == "master"`. Any finding the creator owns for that work can be passed; the preset will receive it under the "review-master" contract. The spec §3.4 states the flag "Runs or enqueues `novel-review-master` preset **scoped to one finding**" (implying a master-targeted one).  
  Source: run.rs:832–938 (the `--finding-id` block after the initial list), daemon-runtime/src/api/handlers/findings.rs:199–213 (get_finding_handler), nexus-local-db/src/findings.rs:237–270 (creator_id + finding_id WHERE).  
  Impact: low (creator can only affect their own data; preset is expected to be defensive), but a correctness gap for the advertised scoping.  
  → Recommendation: after the GET, assert `finding["target_executor"] == "master"` (or at minimum emit a warning and still proceed with a note); or update spec language to "best-effort single-finding input; preset remains authoritative for master routing".

- **W2 (auto-schedule gating — correctness of opt-in trigger)**:  
  `--auto-schedule` (default `false` — safe explicit opt-in) calls `GET /v1/local/findings/stale` (per-creator, across all Works; returns `stale_count` + list), then if >0, re-uses the client-side `master_findings` captured from the earlier work-specific list call and enqueues `novel-review-master` for **this** `work_id`.  
  The `/stale` handler is intentionally creator-global (used by status banner). The daemon watcher (stale_findings_watcher.rs) uses per-Work `auto_review_master_on_timeout` column for true opt-in auto-enqueue. The CLI path is a coarser "if creator has any stale anywhere, offer to run review-master for this work's current master findings".  
  If the work has 0 master findings at list time but the creator has stale on other works, an enqueue still occurs with `open_findings` possibly empty (or stale relative to the second call).  
  Source: run.rs:941–988 (`--auto-schedule` block), daemon-runtime/src/api/handlers/findings.rs:358–395 (list_stale_findings_handler, only creator_id), nexus42/src/commands/creator/works/mod.rs:268 (prior use of same stale endpoint), stale_findings_watcher.rs:188 (per-work opt-in check).  
  Impact: low (explicit flag, no silent action, only affects the caller's own Works). Still a semantic gap vs the daemon's per-work model.  
  → Recommendation: document the cross-work approximation in quickstart/spec, or (future) add a work-scoped stale query or pass the work_id filter to the stale surface for this CLI path.

- **W3 (daemon route input sanitization / list surface — minor)**:  
  The initial findings list is `GET /v1/local/works/{work_id}/findings?status=open&limit=50` with client-side filter `target_executor == "master"` + manual severity sort. No server-side `target_executor` filter is used (or exposed on the list query for this surface). Hard limit 50 means that for a Work with >50 open findings, some master-targeted rows created earlier could be dropped before client filtering.  
  The list handler itself is correctly creator-scoped (`read_active_creator_id` + `findings::list_findings` WHERE creator_id = ? AND work_id = ?). No injection.  
  Source: run.rs:770–780 (list + filter), daemon-runtime/src/api/handlers/findings.rs:179–196 (list_findings_handler + FindingListFilters), nexus-local-db/src/findings.rs:184–212 (parameterized query).  
  Impact: very low for realistic novel workloads (master findings are sparse).  
  → Suggestion (non-blocking): add optional `target_executor` query param to the list endpoint for precision, or raise the limit for the review-master summary path.

### 🟢 Suggestion

- **S1**: Hermetic tests (5) are strictly surface-only (`assert_cmd` on `--help` text). They correctly assert flag presence, subcommand listing, quickstart reference, distinction from `reflection-loop`/`FL-E`, and non-mention of auto-chain (per plan AC and "must not fork FL-E driver" invariant). True daemon integration (route + enqueue body) is explicitly out-of-scope for this test file (comment points to `nexus-daemon-runtime/tests/`). Good P1 discipline.
- **S2**: Distinction from `stage advance --stage review` is correctly and consistently stamped in help text, quickstart §5 (now primary path + explanatory note), `novel-quality-loop.md` §3.4, `novel-workflow-profile.md` §5.5.2–5.5.3, and `cli-spec.md`. This directly resolves the root cause of R-V143P0-002 (workaround documented the wrong preset).
- **S3**: Status.json residual for R-V143P0-002 is correctly transitioned to `lifecycle: resolved` with `resolution.commit`, `resolution.plan_id`, and note. Quickstart/spec copy updated to "Shipped V1.44". Audit trail is complete for P-last hygiene.
- **S4**: All three enqueue paths (`--finding-id`, `--auto-schedule`, and the implicit "list only" path) guard with `CreatorNotSelected` before calling `/schedules`. Creator context flows through the authenticated `DaemonClient`. No new privileged operations.

## Source Trace
- Finding W1: `git diff 9d471bdc..c54b1aa6 -- crates/nexus42/src/commands/creator/run.rs` (lines 832–938); cross-checked against daemon `get_finding_handler` and local-db `get_finding`.
- Finding W2: same diff (lines 941–988) + `daemon-runtime/src/api/handlers/findings.rs:349` (stale handler) + `stale_findings_watcher.rs:203` (per-work opt-in).
- Finding W3: run.rs:770 + daemon list handler + local-db parameterized list query.
- Distinction / S2: quickstart diff + spec diffs + test assertions in `review_master_cli.rs`.
- Evidence commands: see Scope section (all re-ran in this session on the exact `iteration/v1.44` checkout at `c54b1aa6`).

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 4 |

**Verdict**: Approve

(The three Warnings are low-impact correctness nits on the new CLI surface. Core security properties — creator-scoped access, no injection/traversal, explicit opt-in for `--auto-schedule`, no auto-chain forking — are upheld by pre-existing daemon handlers and the reviewed caller. The distinction from `stage advance --stage review` (the original residual driver) is correctly implemented and documented. No changes required before merge for P1 scope.)
