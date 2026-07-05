---
report_kind: qc-review
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: 2026-06-12-v1.43-cli-first-run-copy
verdict: Approve with residuals
generated_at: 2026-06-12T18:42:00+08:00
---

# Code Review Report — P1 (CLI first-run remediation copy)

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: Security and correctness risk
- Report Timestamp: 2026-06-12T18:42:00+08:00

## Scope
- plan_id: 2026-06-12-v1.43-cli-first-run-copy
- Review range / Diff basis: merge-base: cfdd71d3 + tip: 078d74eb
- Working branch (verified): feature/v1.43-cli-first-run-copy
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.43-p1
- Files reviewed: 8
- Commit range: cfdd71d3..078d74eb
- Tools run: git diff, read (key handlers + specs + quickstart), cargo +nightly fmt --all --check, cargo clippy -p nexus42 -p nexus-daemon-runtime -p nexus-orchestration -- -D warnings, rg (format!/to_string, dangerous cmds, absolute paths, secrets), cargo test -p nexus42 -- creator, cargo test -p nexus-orchestration (preset_gates tests), manual status-code + JSON-shape + injection + dead-code + help-text + invariants audit per assignment.

## Findings
### 🔴 Critical
- (none)

### 🟡 Warning
- W-1: `daemon_not_reachable_quickstart()` constructor in `crates/nexus42/src/errors.rs:246` is marked `#[allow(dead_code)]` + `#[must_use]`. It is only directly constructed inside its own unit test (`errors.rs:573`). No live call site wires the canonical quickstart suggestion string; the existing daemon-not-reachable surface continues to use the older `daemon_not_reachable(suggestion)` helper or direct variant. This is a maintenance smell (new dead-code allowance on a constructor whose only purpose is the remediation copy added in this plan). Per assignment guidance: should be wired into the actual reachability error path or removed. Not a security/correctness blocker for this P1.

### 🟢 Suggestion
- (none material for security/correctness)

## Source Trace
- Finding ID: W-1
- Source Type: code-review (static analysis of new constructor + test-only usage)
- Source Reference: crates/nexus42/src/errors.rs:240-263 (constructor) + 573-588 (test)
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 0 |

**Verdict**: Approve with residuals

## Detailed Audits (per assignment checklist)

### Status Code Preservation (CRITICAL)
- `add_schedule` completion guard (novel completed): returns `(StatusCode::CONFLICT, ...)` before and after — **preserved (409)**.
- `preset_gates_failed` paths (work_id omitted, work not found, gate failures): return `StatusCode::UNPROCESSABLE_ENTITY` (422) with `PresetGatesFailed` JSON body — **preserved**.
- No `StatusCode::` or `.status()` changes in any error return path in the diff.

### JSON Response Shape Preservation
- 422 bodies continue to serialize `PresetGatesFailed { error, preset_id, work_id, failed_gates: Vec<FailedGate { kind, expected, actual, remediation }> }`.
- The only mutation is appending the quickstart citation string to the *existing* `remediation: String` field inside each `FailedGate`.
- No fields removed; no new top-level keys; no change to the outer struct shape or serialization attributes.
- DTOs (`PresetGatesFailed`, `FailedGate`) live in `nexus-orchestration`; this change only mutates the *value* of an already-present string field.

### No Injection in Error Strings
- All V1.43 remediation strings are now **hardcoded literals** ending in `See docs/novel-writing-quickstart.md §N`.
- Completion-guard path: removed the previous `format!(..., body.creator_id)` interpolation; now a pure static string.
- `work_field_remediation` / `filesystem_remediation` / `previous_preset_remediation`: literals or `_ => format!(internal field name)` only (no user PII).
- CLI-side messages (reject_produce, works status prints, new `daemon_not_reachable_quickstart`): static + citations only.
- `rg` on `format!|to_string` in the three touched crates shows the new strings do not interpolate creator_id / work_id / paths into remediation text.

### Dead Code Safety (`daemon_not_reachable_quickstart`)
- Constructor added with `#[allow(dead_code)]` + `#[must_use]`.
- Only caller in the diff is its own unit test.
- Not exposed as a public API variant (CliError is crate-internal).
- Not wired into any production "daemon not reachable" call site in this change.
- Test is direct construction + Display assertion (hermetic).
- **Disposition**: Warning (maintenance smell). Preferred remediation is to wire the quickstart variant into the actual reachability error path (or delete the dead constructor). Not blocking for P1 security/correctness.

### Help-Text Accuracy (T3)
- `creator run`: "Start a new novel project, continue writing, advance chapters, or resume an interrupted work session. For a guided walkthrough, see docs/novel-writing-quickstart.md Part I §1–§3."
  - Matches actual subcommands (start, continue, stage, reconcile-chapters, resume). The summary language is accurate for the group; citation correctly targets first-run (Part I §1-3).
- `creator works`: "List, inspect, and manage your Works and the selection pool. Shows progress, chapter status, open findings, and completion state. See docs/novel-writing-quickstart.md §4–§6 for usage patterns."
  - Matches actual subcommands (list, status, use, completion-lock, pool). §4-6 citation aligns with serial writing / quality / completion sections in the quickstart. Accurate.

### System Invariants (AGENTS.md + plan AC)
- Daemon runtime is never described as ACP Agent/Server in new copy — only `nexus42 daemon start` and `creator run` commands are referenced. OK.
- No new strings mention syncing full manuscript text. OK.
- Wire contracts: 422 shape uses pre-existing `PresetGatesFailed`/`FailedGate` serialization (remediation text lives inside an already-defined string field). No schema change or drift introduced. OK.
- Pre-release warning: no "stable" / "1.0" claims; quickstart itself carries the pre-release note. OK.
- Dangerous commands: `rg` found zero `rm -rf|chmod 777` etc. in remediation strings. Only pre-existing legitimate `--force-gates` / `--force` mentions (gate bypass audit context). OK.

### Test Correctness (7 new/updated tests)
- schedules.rs: 1 new test constructs the exact completion-guard literal and asserts the two quickstart citations. Hermetic (pure string); meaningful (pins the 409 remediation text).
- preset_gates.rs: 4 new tokio tests drive `evaluate_gates` with `MockPreviousLookup` + `tempfile::tempdir` + `make_work`/`make_input` (hermetic, no shared state, no real daemon). Assert on `err.failed_gates[0].remediation.contains(...)` for the exact citations promised by the plan. Meaningful.
- run.rs: updated existing `reject_produce_when_novel_complete` test now asserts the new message ("Work is complete" + §6) instead of the old tag. Still pure function.
- errors.rs: 1 new test directly constructs `daemon_not_reachable_quickstart()` and asserts on Display. Hermetic.
- All 7 tests pass under `cargo test` on the relevant packages; none require a running daemon; none are vacuous substring checks on self-constructed data.

### Static Checks
- `cargo +nightly fmt --all --check`: clean.
- `cargo clippy -p nexus42 -p nexus-daemon-runtime -p nexus-orchestration -- -D warnings`: clean.
- Absolute paths: only in test fixtures / fake-home paths (pre-existing, not in remediation strings).
- Secrets: only in auth-token handling modules (unrelated to this copy change).
- Dangerous commands: none in the remediation/copy diff.

## Verdict Rationale
- 0 Critical.
- 1 Warning (dead-code constructor) that is low-severity maintainability, not security or correctness risk for the P1 deliverable.
- All mandatory audits (status codes, JSON shapes, injection, help text, invariants, tests) pass.
- Runtime behavior preserved (per plan AC); no regression in existing creator CLI tests.
- Gate rule: `Approve with residuals` is allowed when no Critical remain.

**Verdict**: Approve with residuals
