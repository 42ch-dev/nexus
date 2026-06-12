---
report_kind: qc-review
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: 2026-06-12-v1.43-author-visibility
verdict: Approve
generated_at: 2026-06-12T20:45:11+0800
---

# Code Review Report — P2 (author-visible UX)

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: kimi-for-coding/k2p6
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-06-12T20:24:23+08:00

## Scope
- plan_id: 2026-06-12-v1.43-author-visibility
- Review range / Diff basis: merge-base: 04c2490d + tip: 6e6b03bb
- Working branch (verified): feature/v1.43-author-visibility
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.43-p2
- Files reviewed: 3
- Commit range: 04c2490d..6e6b03bb
- Tools run: git diff, cargo +nightly fmt --all --check, cargo clippy -p nexus42 -p nexus-daemon-runtime -p nexus-orchestration -- -D warnings, cargo test -p nexus42 --lib, cargo test -p nexus-daemon-runtime --lib, cargo test -p nexus-orchestration --lib

## Findings
### 🔴 Critical
- None.

### 🟡 Warning
- **W-01: `fetch_open_findings` silently swallows errors and misreports "none open"**
  -> `fetch_open_findings` does `client.get(...).await.ok().and_then(|v| v.as_array().cloned()).unwrap_or_default()`. Any failure (daemon 500, network hiccup, auth edge, findings table lock) yields an empty vec, which `print_findings_summary` renders as `findings: none open`. The user cannot distinguish "zero open findings" from "findings could not be fetched". Fix: on error, print a distinct line such as `findings: unavailable` (or omit the block with a parenthetical) instead of defaulting to the success message.

- **W-02: `limit=50` cap can silently undercount open findings and skew severity breakdown**
  -> The endpoint is called with `?status=open&limit=50`. At exactly 50 returned rows the count is accurate, but if the Work has more than 50 open findings the CLI reports only 50, the severity breakdown reflects only the first page, and there is no `50+` indicator. This violates the spec's "Count + severity summary" accuracy guarantee for high-volume Works. Fix: read a total count if the endpoint exposes one, raise/remove the cap for this status use case, or append a `+` / `(50+ total)` marker when the response is capped.

- **W-03: Additional sequential HTTP subcall adds a 30-second unobservable hang on the status hot path**
  -> `handle_status` already fetched the Work, then calls `fetch_open_findings` with the same `DEFAULT_REQUEST_TIMEOUT` (30 s). If the findings endpoint is slow but the Work endpoint succeeded, `creator works status` can hang for up to 30 s before silently degrading to `findings: none open`. Because this command is used frequently during serial writing, a slow best-effort subcall should not block the whole status line. Fix: wrap the findings subcall in a shorter timeout (e.g., `tokio::time::timeout(Duration::from_secs(5), ...)`) and surface a timeout/unavailable message rather than the misleading "none open".

### 🟢 Suggestion
- **S-01: Completed-Work banner uses heavy box-drawing inconsistent with the rest of the CLI**
  -> The banner uses `═══════════════════════════════════════════════════════` double-line box drawing. Elsewhere the CLI uses simple separators such as `println!("{}", "─".repeat(60))` (daemon/mod.rs, acp/mod.rs). Consider aligning the separator style.

- **S-02: `print_findings_summary` doc comment contradicts the actual output**
  -> The rustdoc says "When `open_findings` is empty, prints 'No open findings.'", but the code prints `findings: none open`. Update the comment or the message so they match.

## Source Trace
- Finding ID: W-01
- Source Type: git-diff
- Source Reference: crates/nexus42/src/commands/creator/works/mod.rs:762-770
- Confidence: High

- Finding ID: W-02
- Source Type: git-diff
- Source Reference: crates/nexus42/src/commands/creator/works/mod.rs:763
- Confidence: High

- Finding ID: W-03
- Source Type: manual-reasoning
- Source Reference: crates/nexus42/src/api/daemon_client.rs:39-43; crates/nexus42/src/commands/creator/works/mod.rs:330-332,762-770
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 2 |

**Verdict**: Request Changes

## Test Robustness Audit (10 new tests)

| # | Test function | File | Hermetic | Shared state | Daemon / network | Timing / races | `handler_state()` | Assertion strength | Notes |
|---|---------------|------|----------|--------------|------------------|----------------|-------------------|--------------------|-------|
| 1 | `findings_summary_empty` | mod.rs:983 | Yes | No | No | No | N/A (pure struct test) | Real: count, option, vec length | Good |
| 2 | `findings_summary_single_finding` | mod.rs:992 | Yes | No | No | No | N/A | Real: count, highest severity, counts vec, title | Good |
| 3 | `findings_summary_mixed_severities` | mod.rs:1003 | Yes | No | No | No | N/A | Real: count, highest severity, priority-sorted counts | Good |
| 4 | `findings_summary_top_five_cap` | mod.rs:1021 | Yes | No | No | No | N/A | Real: open_count=8, top_finding len=5 | Good; note this tests the display cap, not the endpoint `limit=50` |
| 5 | `display_no_open_findings` | mod.rs:1072 | Yes | No | No | No | N/A | Asserts literal output contains expected line; also asserts absence of "highest" | Good |
| 6 | `display_findings_with_severity_summary` | mod.rs:1079 | Yes | No | No | No | N/A | Asserts multiple substrings including count, severities, highest, top finding, work_id, quickstart | Good |
| 7 | `display_findings_completed_work_shows_summary` | mod.rs:1096 | Yes | No | No | No | N/A | Asserts count and severity for info-only finding | Good |
| 8 | `completion_shows_zero_open_findings` | mod.rs:1109 | Yes | No | No | No | N/A | Asserts "findings: none open" | Good |
| 9 | `truncate_with_ellipsis_short` | mod.rs:1116 | Yes | No | No | No | N/A | Exact equality | Good |
| 10 | `truncate_with_ellipsis_long` | mod.rs:1121 | Yes | No | No | No | N/A | Exact equality | Good |

All 10 tests are pure unit tests over `FindingsSummary` / display helpers and are hermetic (no DB, no daemon client, no network, no file system, no timing). `handler_state()` is the daemon-runtime integration pattern for DB-backed handler tests and is not appropriate here. No flakiness sources observed.

## Best-Effort Behavior Audit

- **Silent error swallow**: `fetch_open_findings` does `client.get(...).await.ok()...unwrap_or_default()`. All errors (network, 5xx, auth, deserialization) collapse to an empty vec.
- **Misleading "none open"**: `print_findings_summary` then prints `findings: none open`, which is semantically the success state. A user seeing this while the daemon banner reports stale findings will be confused.
- **Documentation**: The comment on `fetch_open_findings` says "Best-effort — never fails the status command" (line 331), and the function-level doc comment says "returns empty vec on failure". The silent swallow is therefore documented for maintainers, but the user-visible consequence is not made explicit.
- **Timeout / time-to-failure**: `DaemonClient` enforces `DEFAULT_REQUEST_TIMEOUT = 30 s` and `DEFAULT_CONNECT_TIMEOUT = 10 s` (daemon_client.rs:39-43). There is no unbounded wait, but a slow or hung findings endpoint can delay `creator works status` by up to 30 s before the silent fallback.

## String Style Consistency Audit

- **Findings line**: `findings: 2 open (1 blocker, 1 minor) — highest: blocker`. This uses a free-form sentence with an em dash. The surrounding status output uses `key: value` colon style (e.g., `progress: 2 / 5 chapters finalized`, `current_chapter: 0`). It is readable but stylistically different.
- **Completion banner**: Uses `═══════════════════════════════════════════════════════` double-line box drawing. The rest of the CLI uses simple `─` repeat separators (e.g., `commands/daemon/mod.rs:635`, `commands/acp/mod.rs:259`). This is a noticeable stylistic outlier.
- **Truncation**: `truncate_with_ellipsis` uses `…` with a 48-character threshold for finding titles (consistent with the existing 28-character threshold in `print_chapter_table`). Usage is consistent.

## Limit-50 Cap Audit

- **Endpoint call**: `/v1/local/works/{work_id}/findings?status=open&limit=50`.
- **Exactly 50 findings**: If the daemon returns exactly 50 rows, `open_count` and the severity breakdown are accurate for the returned set.
- **More than 50 findings**: The CLI silently reports only 50 and the severity breakdown reflects only those 50. There is no "50+" indicator, total field, or second-page fetch. This can mislead authors about the true backlog size.
- **No test coverage** for the `limit=50` boundary in the new tests; `findings_summary_top_five_cap` only tests the display top-5 cap.

## No-New-Failure-Modes Assessment

- **Network timeout**: Bound by `DEFAULT_REQUEST_TIMEOUT` (30 s). Not infinite, but the status command can hang for 30 s if the findings subcall stalls.
- **Slow daemon**: Same as above; the subcall is sequential and uncached, so a slow daemon directly slows `creator works status`.
- **No network / daemon down**: The main `/v1/local/works/{id}` GET will fail and the command errors out before reaching the findings subcall. If the daemon is up but the findings endpoint returns an error, the command still succeeds but misreports `findings: none open`.
- **No new crash modes** observed: the parsing uses `unwrap_or` defaults for all fields.

## Performance Overhead Check

- `creator works status` now performs a second sequential HTTP GET (`/v1/local/works/{id}/findings`) in addition to the existing Work GET and (for novel works) the stale-findings GET.
- The response is bounded to 50 rows, so parsing is O(50) and allocation is small.
- No caching is implemented; every status invocation re-fetches findings. For a command used repeatedly during serial writing, this is an extra local round-trip per call.
- Given the local daemon and small payload, the overhead is acceptable for now, but a future optimization could cache findings for a short TTL or fold the summary into the main Work response.

## Spec §4 Compliance Audit

| Row | Question | Surface minimum | Code path / output | Verdict |
|-----|----------|-----------------|--------------------|---------|
| 1 | Which chapter is active? | `creator run status` or `creator works status` | `current_chapter: {n}` and `print_chapter_table` in `handle_status` (mod.rs:360,403) | ✅ Reachable |
| 2 | Is the Work complete? | Clear terminal/completed marker per novel-workflow-profile §6 | Completed path prints `COMPLETED at ...`, `{total}/{total} chapters finalized`, and quickstart §6 link (mod.rs:340-353) | ✅ Reachable |
| 3 | Are there open findings? | Count + severity summary; link to review preset name | `fetch_open_findings` + `print_findings_summary` prints `findings: N open (...)` plus top findings and a review hint (mod.rs:330,400,762-902) | ✅ Reachable |
| 4 | Is 96h master-review banner active? | Existing daemon banner; ensure visible in status path | Pre-existing best-effort call to `/v1/local/findings/stale` prints `⏰ N finding(s) stale (>96h) ...` when count > 0 (mod.rs:267-290) | ✅ Reachable |

**Trace example (3 open findings: 1 blocker, 2 minor)**:
A user runs `nexus42 creator works status` on a novel Work. `handle_status` fetches the Work, then `fetch_open_findings(..., limit=50)`. `FindingsSummary::from_findings_json` counts 3, sets `highest_severity = "blocker"`, and sorts counts as `[("blocker", 1), ("minor", 2)]`. `print_findings_summary` prints:

```text
findings: 3 open (1 blocker, 2 minor) — highest: blocker
  #1 [blocker] "..." → write
  ...
  Address findings or run: nexus42 creator run stage advance <work_id> --stage review
  See docs/novel-writing-quickstart.md §5
```

This satisfies spec §4 row 3.

## Spec Stamp Accuracy

The stamps in `.mstar/knowledge/specs/novel-author-experience.md` §4 are:

- Row 1: `Shipped (V1.43 P2)` — accurate; `current_chapter` + chapter table are added/modified in this change.
- Row 2: `Shipped (V1.43 P2)` — accurate; completed banner is introduced in this change.
- Row 3: `Shipped (V1.43 P2)` — accurate; findings summary is introduced in this change.
- Row 4: `Shipped (V1.39 P4 T3)` — accurate; the V1.39 P4 plan `2026-06-09-v1.39-master-decision-timeout` lists T3 as "Status banner formatting", matching the existing stale-finding banner code at mod.rs:267-290.

## Risks / Open Questions for PM

1. **Error-handling policy**: The current "best-effort = silently default to zero" pattern is acceptable for a secondary enrichment, but it creates a user-visible lie. Should the CLI explicitly distinguish "unavailable" from "none open" across all best-effort enrichments?
2. **Findings cap**: Is a 50-finding cap acceptable for the target user, or should the status command fetch all open findings / expose a total count? This affects whether W-02 should be fixed now or accepted as a known limitation.
3. **Status hot-path budget**: How much latency is acceptable for `creator works status`? If authors call it frequently, a 30-second worst-case subcall may warrant a shorter dedicated timeout.
4. **Style guide**: Should the CLI adopt the heavy box-drawing banner as the new standard, or align with existing simple separators? A project-wide decision prevents drift.

## Revalidation (post-fix wave, fix commit 0d6b072f)

**Re-review mode**: Targeted — qc-specialist-3 only (raised 3 blocking Warnings in initial wave)
**Fix range reviewed**: 6e6b03bb..0d6b072f
**Files in fix wave**: crates/nexus42/src/commands/creator/works/mod.rs (+224/-35)

### Previously raised blocking findings — re-check
| Finding ID | Summary | Status | Evidence |
|------------|---------|--------|----------|
| qc3-W-01 | Silent error swallow | PASS | `FindingsResult` enum added with `Fetched` / `Unavailable` variants; `print_findings_summary` prints `findings: unavailable (daemon error)` on `Unavailable`; new unit test `display_unavailable_findings` covers the path. |
| qc3-W-02 | limit=50 silent undercount | PASS | `FindingsSummary::is_truncated` field added; `print_findings_summary` renders `50+ open` when `findings.len() == FINDINGS_FETCH_LIMIT`; new test `display_truncated_findings_shows_plus_indicator` asserts both the presence of `50+ open` and the absence of bare `50 open`. |
| qc3-W-03 | 30s timeout on hot path | PASS (structural) | `FINDINGS_FETCH_TIMEOUT` constant set to 5s; `fetch_open_findings` builds a separate `DaemonClient::with_timeouts(...)` with the short timeout while the main Work fetch keeps the default; runtime timeout behavior is covered by the existing `daemon_client.rs` timeout test suite. |

### Static checks (re-run on full P2 feature scope 04c2490d..0d6b072f)
- `cargo +nightly fmt --all --check`: PASS
- `cargo clippy -p nexus42 -p nexus-daemon-runtime -p nexus-orchestration -- -D warnings`: PASS
- Test counts: nexus42 635 passed, daemon-runtime 186 passed, orchestration 559 passed (1 ignored) — 0 failed

### Updated verdict
**Verdict**: Approve
**Rationale**: All three blocking Warnings raised in the initial wave are resolved in fix commit `0d6b072f`: error states are now distinct from the empty-success state, the 50-finding cap is surfaced with a `+` indicator, and the findings subcall uses a dedicated 5s timeout to avoid blocking the status hot path. Scoped static checks and unit tests pass with no failures. Suggestions S-01/S-02 remain out of scope for this targeted re-review.

## Self-Attestation

- Report committed; report frontmatter complete.
- No invented findings: all findings are grounded in the diff, source code, or observed behavior.
- Verdict rationale documented: all targeted Warnings (qc3-W-01, qc3-W-02, qc3-W-03) are resolved in fix commit `0d6b072f`; updated verdict is `Approve`.
