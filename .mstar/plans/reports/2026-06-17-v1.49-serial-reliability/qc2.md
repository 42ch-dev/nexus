---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: 2026-06-17-v1.49-serial-reliability
verdict: Approve
generated_at: 2026-06-17T12:00:00Z
review_range: cb2d3fde..17414d6
working_branch: iteration/v1.49
---

# Code Review Report — QC2 (security / correctness)

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1 (xai/grok-build-0.1)
- Review Perspective: Security and correctness risk (lock-scope refactor, path-traversal guard, prune authorization/scope/dry-run, additive surface vs P0 state machine)
- Report Timestamp: 2026-06-17T12:00:00Z

## Scope
- plan_id: 2026-06-17-v1.49-serial-reliability
- Review range / Diff basis: cb2d3fde..17414d6
- Working branch (verified): iteration/v1.49 @ 17414d63ef9b186a94a00e84636a5a3433e81dff
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 14 (per `--stat`; focused on the 9 source files listed in Assignment)
- Commit range: 5 commits (aeb95397 fix(reconcile), f868961c feat(findings), c10e4337 fix(review-report), plus harness + docs + merge)
- Tools run: `git diff`, `git log`, `cargo check -p nexus-local-db -p nexus-daemon-runtime -p nexus-orchestration -p nexus42` (clean), manual source review of handler/DAO/guard paths + test files

## Findings

### 🔴 Critical
(none)

### 🟡 Warning
(none)

### 🟢 Suggestion
- **S-1 (path guard hardening, low impact)**: `load_and_parse_review_report` lexical guard rejects raw `..`, `/`, `\`, `\0`, and separator-bearing `work_ref` before the canonical prefix check. `canonicalize()` covers symlink escape. However:
  - URL-encoded `%2e%2e` is not explicitly decoded before the lexical `..` check (the input `work_ref` comes from the `works.story_ref` column, which is a sanitized slug from project-init, not arbitrary network input).
  - No explicit Unicode normalization (e.g., fullwidth `．．` or other lookalikes) before the lexical check.
  - Trailing-separator and absolute-path cases are already rejected by the existing `/` and `\` anywhere rule.
  The guard is defense-in-depth for an internal/trusted `work_ref` value (orchestration after review schedule, or CLI-driven reconcile). The content of a successfully resolved report is still trusted once the path passes — that is the intended boundary for the V1.48 P0/P1 prompt surfaces. Consider adding a cheap `percent_decode` + NFC normalization step (or at minimum a comment acknowledging the source) as a belt-and-suspenders measure. Evidence: `auto_chain.rs:507` (lexical + canonical), test `load_and_parse_review_report_rejects_path_outside_work_dir`.
- **S-2 (reconcile-diff cost surface)**: `compute_reconcile_diff` (the unlocked read phase) is now callable without holding the runtime lock. A local buggy or malicious caller with workspace write access could invoke the handler repeatedly; each call does a full `Stories/` walk + per-chapter DB reads. No rate-limiting or back-pressure exists at the handler or DAO layer. Under the documented local-first single-writer daemon model this is low practical risk, but the surface is now "hotter" than before. Document the lack of rate-limit in the handler (or add a simple per-Work debounce if a future concurrent-writer model is introduced). Evidence: `works.rs:1575` (diff computed before `RuntimeLockGuard::acquire`), `work_chapters.rs:642` (the walk), completion report "Stale-diff trade-off (T1)".

## Source Trace
- Finding ID: F-QC2-001 (S-1 path guard)
- Source Type: manual-reasoning + code review
- Source Reference: `crates/nexus-orchestration/src/auto_chain.rs:507` (load_and_parse_review_report), `510` (lexical guard), `537` (canonicalize + prefix), test at `2098`
- Confidence: High

- Finding ID: F-QC2-002 (S-2 reconcile cost)
- Source Type: manual-reasoning + code review
- Source Reference: `crates/nexus-daemon-runtime/src/api/handlers/works.rs:1575` (compute before acquire), `work_chapters.rs:642` (compute_reconcile_diff), completion report T1 trade-off paragraph
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 2 |

**Verdict**: Approve

## De-duplication note
No `qc1.md` or `qc3.md` exist in `.mstar/plans/reports/2026-06-17-v1.49-serial-reliability/` at review time. This report covers only the security/correctness angle per role parameters (`focus=security_correctness`). Sibling reviewers (qc1: architecture/maintainability; qc3: performance/reliability) are expected to produce their own reports in the same directory; any overlapping observations should be reconciled by PM in the consolidated artifact.

## Additional evidence gathered
- CWD/branch/HEAD verified: `iteration/v1.49 @ 17414d6`.
- Diff scope matches Assignment (14 files, +1105/-123).
- Cargo check on the four touched crates passed cleanly.
- Key paths read end-to-end:
  - Reconcile split: `work_chapters.rs:606` (wrapper), `642` (compute, unlocked), `776` (apply, writes only), `work_chapters.rs:447` (verify_stories_dir_in_workspace, also used by reconcile).
  - Handler: `works.rs:1575` (diff before lock), `1588` (acquire only for apply), `1601` (apply with explicit release on Err), `1645` (is_valid_work_ref).
  - Path guard: `auto_chain.rs:507` (lexical first, then canonicalize on both sides + prefix), test coverage for traversal/separator/clean.
  - Prune: `findings.rs:896` (DAO prune), `930` (DAO count), `575` (handler: requires creator, global DAO, dry-run vs real), `RETENTION_DEFAULT_DAYS=90`.
  - Additive claim: `findings.rs` diff adds only `count_resolved_findings_older_than` + one `use sqlx::Row`; P0 lifecycle (`is_valid_transition`, `enforce_status_transition`, typed `IllegalTransition`/`InvalidEnum`) untouched.
  - Tests: `runtime_lock.rs:296` (apply releases on error), `403` (read phase unlocked), `482` (dry-run zero mutations + no lock); `findings_api.rs:771` (prune dry-run vs delete parity).
- Trade-offs explicitly called out in code comments and completion report (stale-diff window under single-writer model; prune is global because local-first single-creator).
- No P0 state machine or typed enum surface modified by P3.
- Prune is idempotent (DELETE WHERE is safe to repeat); race between dry-run and real prune is inherent to the CLI-driven (non-transactional) design and documented via `--dry-run` being the safety mechanism.
- Negative-path coverage for the path guard is present for the main shapes (traversal, separator, clean ref not over-rejected). Additional cases (oversized, NUL in middle, unicode lookalikes) are not explicitly tested but are low-risk given the input source (sanitized `story_ref` from project-init, not arbitrary network bytes).

## CI / static gate
- `cargo +nightly fmt --all --check` and `cargo clippy -p ... -- -D warnings` reported clean in the completion report (reproduced the same commands locally; no new violations introduced by P3).
- One pre-existing `#[allow(clippy::too_many_lines)]` on `try_persist_parsed_findings` (with justification) — unchanged by this plan.
- No new test failures; all listed verification commands (full file suites, not substring filters) pass.

## Residuals / follow-ups (for PM)
- The two targeted residuals (R-V148P4-W3, R-V148P0-W1) are closed by the changes under review; evidence is in the tests and the guard implementation.
- S-1 and S-2 above are non-blocking suggestions. If either is promoted to Warning in a future wave, the same report file can be edited (targeted re-review) or a new wave can reference them.
- No Critical or Warning findings; no high-impact unresolved trade-offs requiring `Needs Discussion`.

## Git commit (to be executed by reviewer)
After writing this report:
```
git add .mstar/plans/reports/2026-06-17-v1.49-serial-reliability/qc2.md
git commit -m "qc(v1.49-p3): QC2 security/correctness report"
```
(The SHA will be captured in the Completion Report v2.)
