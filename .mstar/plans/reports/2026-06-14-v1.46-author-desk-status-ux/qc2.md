---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-14-v1.46-author-desk-status-ux"
verdict: "Approve"
generated_at: "2026-06-14"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1 (xai/grok-build-0.1)
- Review Perspective: Security and correctness risk
- Report Timestamp: 2026-06-14

## Scope
- plan_id: 2026-06-14-v1.46-author-desk-status-ux
- Review range / Diff basis: merge-base: de30a702 → tip: c9fb1abb (5 commits on iteration/v1.46; equivalent to git diff de30a702..c9fb1abb or git show --stat de30a702..c9fb1abb)
- Working branch (verified): iteration/v1.46
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 3 (primary: crates/nexus42/src/commands/creator/works/mod.rs; spec and plan docs)
- Commit range: de30a702..c9fb1abb (5 commits)
- Tools run: cargo clippy --all -- -D warnings (clean), cargo test -p nexus42 --lib -- 'works::tests' (40 passed)

## Findings
### 🔴 Critical
(none)

### 🟡 Warning
(none)

### 🟢 Suggestion
- **S-1 (maintainability, minor correctness documentation)**: The `enrich_status_json` tests use a minimal `finding_json` helper that only populates `finding_id`, `severity`, `title`, `routing_hint`, and `status`. While the runtime implementation correctly does a verbatim passthrough (`arr.to_vec()`) of whatever the daemon `/v1/local/works/{id}/findings` returns, the test does not assert on the full element shape documented in spec §4.1 ("Same element shape as findings list API"). Adding one or more fields known to exist in the real list response (e.g., `created_at`, `updated_at`, or any remediation metadata) to the test helper and an assertion would make the "verbatim fidelity" claim stronger and catch future drift earlier. This is not a security or behavioral correctness risk because the production path is a direct copy; it is a test-coverage hygiene item.

## Source Trace
- Finding ID: F-001 (S-1)
- Source Type: manual-reasoning + test inspection
- Source Reference: crates/nexus42/src/commands/creator/works/mod.rs:1638–1752 (enrich_* tests) + 1411 (finding_json helper) + 1141–1145 (passthrough insert)
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 1 |

**Verdict**: Approve

## Detailed Security & Correctness Analysis (qc-specialist-2 focus)

**Novel-only gate (Grill #6 / spec §4)**:
- Implemented with defense-in-depth: (1) explicit `is_novel` check in `handle_status` json branch before any findings fetch (line ~377–392), (2) identical `is_novel` guard inside `enrich_status_json` that early-returns the original daemon payload for non-novel (line ~1134–1136).
- Generic work resp + findings passed to enrich → findings key is absent (test `enrich_generic_work_omits_findings_gate`).
- Missing `work_profile` or non-"novel" value → treated as non-novel (test `enrich_missing_work_profile_omits_findings`).
- No path in the diff allows findings or findings_stale to appear for generic works.

**JSON contract fidelity (spec §4.1)**:
- `findings` is a root-level array whose elements are exactly the objects returned by the daemon findings list endpoint (passthrough via `to_vec()`).
- `findings_stale` is inserted only when the `/v1/local/findings/stale` response has `stale_count > 0` (opt-in, human-parity).
- When findings fetch fails (`FindingsResult::Unavailable`), the key is **omitted** rather than fabricated as `[]`. This allows JSON consumers to distinguish "genuinely no open findings" from "transient daemon fault" (explicitly documented in the new function comment and tested).
- Daemon GET work fields (work_id, title, work_profile, current_chapter, chapters, etc.) are preserved; only additive insertion for novel works (tests `enrich_preserves_daemon_work_fields`, `enrich_novel_with_findings_inserts_array`).

**Error / best-effort paths**:
- Both findings fetch (5s timeout, shorter than default) and stale fetch use `.ok()` / `FindingsResult::Unavailable` so the `status --json` command never fails due to ancillary endpoints. This matches the pre-existing human banner contract (V1.39 P4 baseline) and is explicitly called out in the plan ("best-effort degradation").
- No new unbounded waits or resource leaks introduced.

**Data leakage / injection surfaces**:
- `routing_hint` values in the JSON output come verbatim from the daemon findings response (trusted server data for this client). No user-controlled string from the status command is interpolated into executable context in the json path.
- The human remediation change (empty findings case) now emits `nexus42 creator run novel-review-master <sanitized_work_id>` using `sanitize_for_terminal`. The sanitize function (pre-existing but exercised by new code) strips ANSI CSI, ASCII controls except \n/\t, and DEL; it is only used for terminal display of a copy-paste hint, not for any `Command::new` / `sh -c` construction.
- No new file-path construction, SQL, or privileged operation driven by unsanitized input in the diff.
- Work ID resolution for omitted arg re-uses the existing pool-active query (outside the new json enrichment logic).

**State / race considerations**:
- The json enrichment performs two independent best-effort GETs (findings per-work, global stale) after the authoritative work GET. There is no cross-endpoint transaction. This is identical in spirit to the human path (which also fetches stale separately). No new TOCTOU or inconsistent snapshot risk beyond the baseline already shipped in V1.43 P2.
- `findings_stale` insertion is purely opt-in on `stale_count > 0`; zero-count responses are discarded. Matches the human stale banner visibility rule.

**CI / test evidence (within scope)**:
- `cargo clippy --all -- -D warnings`: clean (finished dev profile with no warnings emitted).
- `cargo test -p nexus42 --lib -- 'works::tests'`: 40 tests passed, including the full suite of new `enrich_*` contract tests and the updated `display_*` / `capture_findings_output` tests that verify removal of the old blanket `reflection-loop` + quickstart footer and addition of the per-finding `routing_hint` + empty → `novel-review-master` behavior.

**Scope discipline note**:
- The human stale banner (non-json path) still contains a reference to the retired `docs/novel-writing-quickstart.md §5`. This text was **not modified** in the `de30a702..c9fb1abb` range (the diff hunks only touch the json branch and the `print_findings_summary` remediation block). Per explicit instruction, pre-existing code outside the review diff is not flagged. The P1 plan (`2026-06-14-v1.46-spec-cli-hygiene`) is chartered to perform the normative remediation sweep and quickstart deletion.

**Overall**:
No security, correctness, or contract violations were introduced by the five commits under review. The novel-only gate, JSON shape passthrough, graceful degradation, `findings_stale` opt-in, and daemon-field preservation are all implemented and unit-tested correctly. The change is narrowly scoped, defense-in-depth guarded, and follows the established best-effort patterns of the status command.

## Verification Evidence (for Completion Report)
- Checkout: `git rev-parse --show-toplevel` → `/Users/bibi/workspace/organizations/42ch/nexus`; `git branch --show-current` → `iteration/v1.46`; `git log -1 --oneline` → `c9fb1abb ...`
- Diff: `git diff de30a702..c9fb1abb --stat` confirmed 3 files, 238 insertions (core delta in works/mod.rs).
- Clippy: clean.
- Tests: 40/40 passed for the works module (including all new V1.46 P0 enrich + display tests).
- Report will be committed as the only staged path.

## Revalidation (N/A — initial wave)
(N/A for initial QC wave)
