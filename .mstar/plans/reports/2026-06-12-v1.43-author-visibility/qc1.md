---
report_kind: qc-review
reviewer: qc-specialist
reviewer_index: 1
plan_id: 2026-06-12-v1.43-author-visibility
verdict: Approve with residuals
generated_at: 2026-06-12T17:30:00+08:00
---

# Code Review Report — P2 (author-visible UX)

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: deepseek-v4-pro (volcengine-plan/deepseek-v4-pro)
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-12T17:30:00+08:00

## Scope
- plan_id: 2026-06-12-v1.43-author-visibility
- Review range / Diff basis: merge-base: 04c2490d + tip: 6e6b03bb
- Working branch (verified): feature/v1.43-author-visibility
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.43-p2
- Files reviewed: 3
- Commit range: 04c2490d..6e6b03bb
- Tools run: cargo +nightly fmt --all --check, cargo clippy -p nexus42 -p nexus-daemon-runtime -p nexus-orchestration -- -D warnings, cargo test -p nexus42 --lib (findings + truncate), emoji scan, TODO/FIXME scan, cross-link integrity check

## Findings
### 🔴 Critical
None.

### 🟡 Warning
- **W-1: Silent API failure indistinguishable from "no findings"** → When `fetch_open_findings` fails (daemon unreachable, auth error, network), the user sees `findings: none open` — identical to the genuine "no findings exist" case. The best-effort design is architecturally correct (status command must not fail because findings endpoint is down), but the user cannot distinguish "API error" from "clean state." Consider one of: (a) log a warning to stderr on fetch failure, (b) use a distinct label like `findings: unavailable` on error vs `findings: none open` on success, or (c) add a `--verbose` flag that surfaces the fetch error. **Severity: Warning** — UX ambiguity, not a correctness bug.

### 🟢 Suggestion
- **S-1: Duplicated aggregation logic between CLI and daemon** → The CLI's `FindingsSummary::from_findings_json` reimplements severity counting, highest-severity tracking, and top-N extraction. The daemon already defines `FindingsSummaryDto` (in `crates/nexus-daemon-runtime/src/api/handlers/findings.rs:102-111`) with `open_count`, `by_severity`, and `top_findings` — though no handler currently returns it. If a dedicated `GET /v1/local/works/{work_id}/findings/summary?status=open` endpoint is added later, the CLI should consume it instead of maintaining parallel aggregation. Add a cross-reference comment in `fetch_open_findings` noting the daemon's `FindingsSummaryDto` as the intended future source.

- **S-2: Test output capture duplicates formatting logic** → `capture_findings_output` in the test module reimplements the same formatting as `print_findings_summary` rather than testing through the actual function. The code comment acknowledges the limitation ("We cannot easily redirect println! in unit tests without a global writer"). Consider extracting a `format_findings_summary(&FindingsSummary, work_id: &str) -> String` helper that both `print_findings_summary` and tests can call, eliminating the duplication.

- **S-3: Hardcoded `limit=50` in findings fetch** → If a Work accumulates >50 open findings, the CLI will show incomplete severity counts and miss findings beyond the 50th. This is unlikely in practice (findings are resolved or closed) but the hardcoded limit has no upper-bound guard. Consider either: (a) raising the limit with a comment justifying the ceiling, or (b) using the daemon's summary endpoint (see S-1) which would return aggregated counts regardless of total.

## Source Trace
- Finding ID: W-1
- Source Type: manual-reasoning (error-handling audit)
- Source Reference: crates/nexus42/src/commands/creator/works/mod.rs:762-770 (fetch_open_findings)
- Confidence: High

- Finding ID: S-1
- Source Type: api-contract-check (cross-crate duplication audit)
- Source Reference: crates/nexus42/src/commands/creator/works/mod.rs:788-855 vs crates/nexus-daemon-runtime/src/api/handlers/findings.rs:102-128
- Confidence: High

- Finding ID: S-2
- Source Type: manual-reasoning (test quality audit)
- Source Reference: crates/nexus42/src/commands/creator/works/mod.rs:990-1065 (capture_findings_output)
- Confidence: Medium

- Finding ID: S-3
- Source Type: manual-reasoning (boundary analysis)
- Source Reference: crates/nexus42/src/commands/creator/works/mod.rs:763 (limit=50)
- Confidence: Low

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 3 |

**Verdict**: Approve with residuals

## Verdict Rationale

Zero Critical findings. One Warning (W-1: silent API failure UX ambiguity) — the best-effort design is architecturally sound and the ambiguity is low-risk in practice (findings endpoint is local daemon, not remote). Three Suggestions are forward-looking improvements that do not block merge. Per `mstar-review-qc` gate rules: `Approve with residuals` is appropriate when no Critical remains and residual items are documented for PM tracking.

## Spec §4 Alignment Table

| Row | Question | Status | Evidence |
|-----|----------|--------|----------|
| 1 | Which chapter is active? | ✅ PASS | `current_chapter` field (line 360) + `print_chapter_table` (line 403) in both active and completed paths |
| 2 | Is the Work complete? | ✅ PASS | `COMPLETED` banner with `═══════` framing (lines 340-353), chapter count, quickstart §6 link |
| 3 | Are there open findings? | ✅ PASS | `findings:` line with severity breakdown + highest tag + top-5 list + review hint + quickstart §5 link (lines 347, 400, 877-901) |
| 4 | Is 96h banner active? | ✅ PASS | Pre-existing V1.39 P4 T3 wiring at lines 265-290; stale-count banner with quickstart §5 link; verified present and unchanged in V1.43 P2 |

## API Contract Audit

| Check | Result |
|-------|--------|
| Endpoint exists? | ✅ `GET /v1/local/works/{work_id}/findings` — `list_findings_handler` at findings.rs:180 |
| Query params match? | ✅ `status=open` — `ListFindingsQuery.status: Option<String>` at findings.rs:96 |
| `limit` param supported? | ✅ `ListFindingsQuery.limit: Option<u32>` at findings.rs:98 |
| Response shape matches? | ✅ Returns `Json<Vec<FindingApiDto>>` — array of objects with `severity`, `title`, `routing_hint` fields (findings.rs:32-46) |
| `routing_hint` always present? | ✅ `format_routing_hint` always returns `Some(...)`, so field is always serialized; CLI fallback `unwrap_or("→ none")` is defensive |
| `severity` always present? | ✅ `severity: String` (non-optional); CLI fallback `unwrap_or("info")` is defensive |

**Conclusion**: API contract is fully aligned. No drift between CLI consumer and daemon producer.

## Code Organization Observations

- **Module placement**: `fetch_open_findings()` and `print_findings_summary()` are correctly placed in `crates/nexus42/src/commands/creator/works/mod.rs` — they are CLI display helpers used exclusively by `handle_status` in the same module.
- **Shared helper extraction**: The 8-line `fetch_open_findings` is a thin wrapper around `DaemonClient::get`. Extracting it to a shared crate would add indirection without clear benefit at this scale. The daemon's `FindingsSummaryDto` (S-1) is the more natural extraction point if a summary endpoint is added.
- **Struct extraction**: `FindingsSummary` is well-designed — extracted as a separate struct to enable hermetic unit testing without daemon client dependency. This follows the project's existing pattern (e.g., `ChapterTable` display helpers).
- **No new dependencies**: The change adds no new crate dependencies. All parsing uses `serde_json::Value` — consistent with the rest of `works/mod.rs`.

## Quickstart Sync Quality

| Section | Check | Result |
|---------|-------|--------|
| §5 (findings) | Output format matches code? | ✅ Example shows `findings: 2 open (1 blocker, 1 minor) — highest: blocker` — matches `print_findings_summary` format exactly |
| §5 (findings) | Top findings format? | ✅ `#1 [blocker] "Continuity error..." → write` — matches code output |
| §5 (findings) | Review hint present? | ✅ `Address findings or run: nexus42 creator run stage advance <work_id> --stage review` + quickstart §5 link — both present |
| §6 (completion) | Completed output format? | ✅ Shows `COMPLETED at ...`, chapter count, `findings: none open`, quickstart §6 link, `To start a new Work` hint — all match code |
| §6 (completion) | Banner framing? | ✅ `═══════` framing shown — matches code (introduced in V1.43 P1, consistent across iteration) |
| No drift? | Cross-reference accuracy? | ✅ All command examples and output snippets are accurate representations of actual CLI output |

**Conclusion**: Quickstart sync is accurate and complete. No drift detected between `docs/novel-writing-quickstart.md` and the implemented CLI output.

## Spec Stamps Audit

| Row | Stamp | Accuracy | Notes |
|-----|-------|----------|-------|
| 1 | `— [x] Shipped (V1.43 P2) — current_chapter + chapter table in status output` | ✅ Accurate | `current_chapter` field + `print_chapter_table` both present |
| 2 | `— [x] Shipped (V1.43 P2) — completed banner with COMPLETED marker, quickstart §6 link` | ✅ Accurate | `COMPLETED at ...` + `═══════` framing + §6 link |
| 3 | `— [x] Shipped (V1.43 P2) — findings: line with severity breakdown, top findings, review hint` | ✅ Accurate | Full findings summary with severity counts, highest tag, top-5, review action hint |
| 4 | `— [x] Shipped (V1.39 P4 T3) — already wired in creator works status; verified V1.43 P2` | ✅ Accurate | V1.39 P4 T3 is the correct historical reference (stale-findings watcher + `GET /v1/local/findings/stale` endpoint); the banner is at lines 265-290 and was verified present in V1.43 P2 |

**Conclusion**: All 4 spec stamps are accurate. Row 4 correctly references V1.39 P4 T3 as the original implementation, with V1.43 P2 verification.

## Risks / Open Questions for PM

1. **W-1 (silent failure UX)**: Should the CLI surface fetch failures? The current design is architecturally correct for best-effort status, but the ambiguity between "no findings" and "couldn't check" may confuse authors who expect findings to exist. Recommend tracking as a low-priority residual for P-last or a future UX polish plan.

2. **S-1 (duplicated aggregation)**: If a dedicated findings summary endpoint is added to the daemon in a future plan, the CLI should be updated to consume it. The daemon's `FindingsSummaryDto` is already defined but unused — it's a natural extraction point.

3. **Completion banner style**: The `═══════` framing was introduced in V1.43 P1 and is used consistently in P2. This is a new visual convention for the CLI — worth noting for future CLI consistency work.

## Self-Attestation

Report committed; report frontmatter complete; no invented findings; verdict rationale documented. All findings are traceable to specific code locations with evidence. Static checks (fmt, clippy, emoji, TODO/FIXME) all pass. All 10 new tests pass. API contract verified against daemon handler source.
