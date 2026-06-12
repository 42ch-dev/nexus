---
report_kind: qc-review
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: 2026-06-12-v1.43-cli-first-run-copy
verdict: Approve
generated_at: 2026-06-12T19:52:11+08:00
---

# Code Review Report — P1 (CLI first-run remediation copy)

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: kimi/k2p6
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-06-12T19:34:34+08:00

## Scope
- plan_id: 2026-06-12-v1.43-cli-first-run-copy
- Review range / Diff basis: merge-base: cfdd71d3 + tip: 078d74eb
- Working branch (verified): feature/v1.43-cli-first-run-copy
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.43-p1
- Files reviewed: 8
- Commit range: cfdd71d3..078d74eb
- Tools run:
  - `cargo +nightly fmt --all --check`
  - `cargo clippy -p nexus42 -p nexus-daemon-runtime -p nexus-orchestration -- -D warnings`
  - `cargo test -p nexus42 --lib`
  - `cargo test -p nexus-daemon-runtime --lib`
  - `cargo test -p nexus-orchestration --lib`
  - `rg -n 'novel-writing-quickstart.md §' crates/ | sort`
  - `rg -n 'Work is complete|NOVEL_COMPLETE|gates_failed|preset_gate' .github/`

## Findings

### 🔴 Critical
- None.

### 🟡 Warning
- **F-W01 — Daemon-not-reachable quickstart citation is not wired to user-visible output.**
  `CliError::daemon_not_reachable_quickstart()` (`crates/nexus42/src/errors.rs:247`) was added and tested, but all production call sites in `crates/nexus42/src/api/daemon_client.rs:585,622,659,695` still use the generic `daemon_not_reachable("Start the daemon with `nexus42 daemon start` and retry.")` constructor. A user whose daemon is down therefore receives a suggestion that **does not** cite `docs/novel-writing-quickstart.md §1`, contradicting the `novel-author-experience.md` §3 remediation table (row 1 marked "Shipped") and the `cli-spec.md` §7.1 V1.43 stamp.
  -> Replace the four `daemon_client.rs` call sites with `CliError::daemon_not_reachable_quickstart()` (or make the generic constructor default to the quickstart citation and remove the dead helper).

### 🟢 Suggestion
- **F-S01 — `creator run stage advance` work-completed error embeds a line break.**
  `crates/nexus42/src/commands/creator/run.rs:824` inserts `\n` before the "Hint:" clause, while the `schedules.rs` completion-guard remediation is a single line. For consistency with the "single-line actionable next step" style in `cli-spec.md` §7.1, reformat the run.rs message as one line.
  -> Remove the embedded newline and fold the hint into the same line.

- **F-S02 — Completion-guard unit test duplicates the production string literal.**
  `crates/nexus-daemon-runtime/src/api/handlers/orchestration/schedules.rs:1583` copies the production message verbatim and asserts substring presence. This is brittle and does not exercise the actual guard code path.
  -> Prefer calling the guard logic (or a factored message constructor) and asserting on the returned / emitted message.

- **F-S03 — Daemon-not-reachable test only covers dead code.**
  `crates/nexus42/src/errors.rs:579` tests `daemon_not_reachable_quickstart()` in isolation. Because the helper is not used in production (F-W01), the test gives false confidence that the remediation class is shipped.
  -> After wiring F-W01, add an integration-level assertion on the actual daemon-client error path, or at minimum assert that all `DaemonNotReachable` production sites cite the quickstart.

- **F-S04 — Remediation strings have inconsistent terminal punctuation.**
  Some new strings end with the `§N` anchor and no period (e.g. `errors.rs:251`, `preset_gates.rs:355`), while others end with a period before the anchor or after a parenthetical (e.g. `schedules.rs:183`, `run.rs:824`). This is cosmetic but reduces polish for a first-run UX remediation pass.
  -> Pick one convention (period-terminated sentence with the `§` anchor at the end) and apply it across all five remediation classes.

## Source Trace

- **Finding ID:** F-W01
  - Source Type: git-diff + spec-audit + manual-reasoning
  - Source Reference: `crates/nexus42/src/errors.rs:247-255`; `crates/nexus42/src/api/daemon_client.rs:585,622,659,695`; `.mstar/knowledge/specs/novel-author-experience.md:52`; `.mstar/knowledge/specs/cli-spec.md:583`
  - Confidence: High

- **Finding ID:** F-S01
  - Source Type: git-diff + manual-reasoning
  - Source Reference: `crates/nexus42/src/commands/creator/run.rs:824-828`
  - Confidence: High

- **Finding ID:** F-S02
  - Source Type: test-review + manual-reasoning
  - Source Reference: `crates/nexus-daemon-runtime/src/api/handlers/orchestration/schedules.rs:1577-1593`
  - Confidence: High

- **Finding ID:** F-S03
  - Source Type: test-review + manual-reasoning
  - Source Reference: `crates/nexus42/src/errors.rs:579-594`
  - Confidence: High

- **Finding ID:** F-S04
  - Source Type: git-diff + manual-reasoning
  - Source Reference: `crates/nexus42/src/errors.rs:251`; `crates/nexus-orchestration/src/preset_gates.rs:355,358,361,365,378,392,395`; `crates/nexus-daemon-runtime/src/api/handlers/orchestration/schedules.rs:183,340,521`; `crates/nexus42/src/commands/creator/run.rs:825`; `crates/nexus42/src/commands/creator/works/mod.rs:286,342`
  - Confidence: High

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 4 |

**Verdict**: Request Changes

---

# Appendix — Reviewer-Specific Audits

## Test Robustness Audit (7 tests)

| # | Test function | File | Hermetic | Shared state | Daemon / network | Timing / races | `handler_state()` | Assertion strength | Notes |
|---|---------------|------|----------|--------------|------------------|----------------|-------------------|--------------------|-------|
| 1 | `completion_guard_message_cites_quickstart_section_6` | `nexus-daemon-runtime/src/api/handlers/orchestration/schedules.rs` | Yes | None | None | None | N/A (lib unit test) | Weak | Literal string snapshot; does not exercise guard path. |
| 2 | `remediation_work_field_cites_quickstart` | `nexus-orchestration/src/preset_gates.rs` | Yes | None | None | None | N/A | Medium | Exercises `evaluate_gates` with mock lookup + tempdir. |
| 3 | `remediation_filesystem_scaffold_cites_quickstart_section_2` | `nexus-orchestration/src/preset_gates.rs` | Yes | None | None | None | N/A | Medium | Exercises `evaluate_gates` with mock lookup + tempdir. |
| 4 | `remediation_previous_preset_init_cites_quickstart_section_2` | `nexus-orchestration/src/preset_gates.rs` | Yes | None | None | None | N/A | Medium | Exercises `evaluate_gates` with mock lookup + tempdir. |
| 5 | `remediation_previous_preset_writing_cites_quickstart_section_3` | `nexus-orchestration/src/preset_gates.rs` | Yes | None | None | None | N/A | Medium | Exercises `evaluate_gates` with mock lookup + tempdir. |
| 6 | `reject_produce_when_novel_complete` (updated) | `nexus42/src/commands/creator/run.rs` | Yes | None | None | None | N/A | Medium | Exercises pure function and asserts message content. |
| 7 | `daemon_not_reachable_quickstart_cites_section_1` | `nexus42/src/errors.rs` | Yes | None | None | None | N/A | Weak | Tests dead constructor; does not verify production path. |

All seven tests are hermetic, require no daemon, and make no timing assumptions. Tests #1 and #7 are low-value string-literal checks; #2–#6 are stronger integration-style unit tests.

## String Style Consistency Audit (5 remediation classes)

| Condition | Representative string | Single line | § anchor at end | Period terminated | No jargon | Length | Notes |
|-----------|----------------------|-------------|-----------------|-------------------|-----------|--------|-------|
| Daemon not reachable | "Start the daemon with `nexus42 daemon start`; see docs/novel-writing-quickstart.md §1" | Yes | Yes | **No** | Yes | OK | Dead code path (F-W01). |
| `preset_gates_failed` (work_field) | "Ensure the Work has `work_profile: novel` set. See docs/novel-writing-quickstart.md §2" | Yes | Yes | **No** | Yes | OK | — |
| Missing scaffold / intake incomplete | "Run `creator run start --init-preset novel-project-init` to scaffold `{path}`. See docs/novel-writing-quickstart.md §2" | Yes | Yes | **No** | Yes | OK | — |
| Work completed | "This Work is complete; see docs/novel-writing-quickstart.md §6. To start a new Work, use ... (see docs/novel-writing-quickstart.md §2)" | Yes (schedules.rs) / **No** (run.rs) | Yes | Partial | Yes | OK | run.rs inserts `\n` before hint (F-S01). |
| Open findings blocking progress | "address open findings or run a review pass; see docs/novel-writing-quickstart.md §5" | Yes | Yes | **No** | Yes | OK | — |

Drift observed: inconsistent terminal punctuation (F-S04) and a multi-line error in run.rs (F-S01).

## CI Regression Check

```bash
rg -n 'Work is complete|NOVEL_COMPLETE|gates_failed|preset_gate' .github/
# => no CI grep matches
```
No in-repo CI scripts depend on the changed message fragments. Downstream consumers outside this repo could still parse the old `NOVEL_COMPLETE` tag or the old schedule.rs message, but that risk is unquantifiable from this codebase.

## No-New-Failure-Modes Assessment

- The `NOVEL_COMPLETE` machine tag was removed from the run.rs error. No in-repo consumers break (CI check above).
- The schedules.rs completion-guard message changed from a structured explanatory sentence to a shorter citation-first sentence. No in-repo consumers detected.
- The `daemon_not_reachable_quickstart()` helper is new and unused, so it introduces no runtime failure mode, but it is dead code.
- No new panics, unwraps, or fallible conversions were added.

## Perf Overhead Check

All new strings are constructed on error paths only:
- `errors.rs`: constructed when a `DaemonNotReachable` error is produced (currently only by dead helper).
- `preset_gates.rs`: remediation strings are built when a gate fails.
- `schedules.rs`: message built when the completion guard triggers.
- `run.rs`: message built when `reject_produce_when_novel_complete` returns `Err`.
- `works/mod.rs`: printed during `creator works status` output.

No eager construction in hot loops. No perf overhead concern.

## Spec §3 Compliance Audit

| Row | Condition | Quickstart citation reachable from CLI surface? | Status |
|-----|-----------|------------------------------------------------|--------|
| 1 | Daemon not reachable | **No.** Constructor exists but is unused; production call sites omit the citation. | **Not shipped** |
| 2 | `preset_gates_failed` | Yes. `work_field_remediation` and `previous_preset_remediation` in `preset_gates.rs` append `§2`/`§3`. | Shipped |
| 3 | Missing scaffold / intake incomplete | Yes. `filesystem_remediation` and `intake_status` remediation append `§2`/`§3`. | Shipped |
| 4 | Work completed (auto-chain stopped) | Yes. Cited in `schedules.rs` completion guard, `run.rs` rejection, and `works/mod.rs` status output. | Shipped |
| 5 | Open findings blocking progress | Yes. `works/mod.rs` stale-finding `println!` cites `§5`. | Shipped |

## Spec Stamp Accuracy

- `cli-spec.md` §7.1 V1.43 stamp claims daemon-not-reachable and all remediation classes produce "actionable one-line next steps citing `docs/novel-writing-quickstart.md` §1–§6". This is **partially inaccurate**: the daemon-not-reachable path does not cite the quickstart, and the run.rs work-completed message is two-line.
- `novel-author-experience.md` §3 row 1 checkbox is marked `- [x] Shipped (V1.43 P1)` for "Daemon not reachable". Because the production error does not include the quickstart citation, this checkbox should remain unchecked or be qualified until F-W01 is fixed.

## Help-Text Style Consistency

New `creator run` and `creator works` doc comments in `crates/nexus42/src/commands/creator/mod.rs:443-461` follow the existing clap convention: a short first line plus a detailed paragraph. The text is actionable, uses quickstart vocabulary, and is comparable in length to `Kb` and `Reference` help text. No concerns.

## Risks / Open Questions for PM

1. **F-W01 scope**: Should the fix for the unwired daemon-not-reachable citation be done in this P1 review cycle, or deferred to a follow-up hotfix? If deferred, the `novel-author-experience.md` §3 checkbox and `cli-spec.md` §7.1 stamp should be reverted to avoid claiming shipped behavior.
2. **Downstream message parsers**: Are there any out-of-repo consumers (e.g. platform E2E tests, user scripts) that grep for `NOVEL_COMPLETE` or the old schedule.rs completion message? If so, the message changes need a migration note.
3. **Test strategy**: Two of the seven new/updated tests are literal-string snapshots (#1, #7). Does the team want a follow-up plan slice to convert these into path-covering assertions?

## Revalidation (post-fix wave, fix commit 6f99ae87)

**Re-review mode**: Targeted — qc-specialist-3 only (raised 1 blocking Warning in initial wave)
**Fix range reviewed**: 078d74eb..6f99ae87
**Files in fix wave**: crates/nexus42/src/api/daemon_client.rs (+8/-12), crates/nexus42/src/commands/creator/run.rs (+2/-3), crates/nexus42/src/errors.rs (+0/-1)

### Previously raised findings — re-check
| Finding ID | Summary | Status | Evidence |
|------------|---------|--------|----------|
| qc3-F-W01 (blocking) | Daemon-not-reachable quickstart citation dead code | PASS | `crates/nexus42/src/api/daemon_client.rs:586,622,658,693` all call `CliError::daemon_not_reachable_quickstart()`; `rg -n 'daemon_not_reachable\(' crates/nexus42/src/api/daemon_client.rs` returned 0 hits; `#[allow(dead_code)]` removed in `crates/nexus42/src/errors.rs:245` |
| qc3-F-S01 (was Suggestion) | 2-line work-completed message | PASS | `crates/nexus42/src/commands/creator/run.rs:825` now single-line: `"This Work is complete; see docs/novel-writing-quickstart.md §6. Use `nexus42 creator works status {work_id}` or advance to the 'persist' stage."`; quickstart §6 citation preserved |

### Static checks (re-run on full P1 feature scope cfdd71d3..6f99ae87)
- `cargo +nightly fmt --all --check`: PASS
- `cargo clippy -p nexus42 -p nexus-daemon-runtime -p nexus-orchestration -- -D warnings`: PASS
- Test counts: nexus42 616 passed, daemon-runtime 186 passed, orchestration 559 passed (1 ignored) — 0 failed
- Constructor still `#[allow(dead_code)]`? NO (good)

### Updated verdict
**Verdict**: Approve
**Rationale**: The blocking Warning F-W01 is fully resolved: `daemon_not_reachable_quickstart()` is wired to all four production call sites in `daemon_client.rs`, the old constructor is no longer used there, and the `#[allow(dead_code)]` suppression has been removed. The previously raised Suggestion F-S01 is also fixed in this wave — the work-completed message in `run.rs` is now a single line while preserving the quickstart §6 citation and helpful hint. Static checks (nightly fmt, scoped clippy, scoped lib tests) all pass with no regressions. No new Critical or Warning findings were introduced by the fix commit.
