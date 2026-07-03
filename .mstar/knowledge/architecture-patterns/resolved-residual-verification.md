---
module: harness-process
date: 2026-07-03
problem_type: process-convention
category: architecture-patterns
severity: medium
plan_id: 2026-07-03-v1.86-local-api-trust-hardening
tags: [residual, lifecycle, verification, regression-of-resolution, security]
applies_when: treating a prior residual's `lifecycle: resolved` as evidence that a bug class is closed
---

# Resolved Residual Is a Claim, Not a Guarantee

## Context

A `residual_findings` entry with `lifecycle: resolved` records that a specific finding was addressed by a commit. It does **not** prove the underlying bug class is closed on the current codebase. During V1.86 (Local API trust-boundary hardening), the plan initially framed five historical same-class residuals as "open residuals to close." Pre-close verification showed **all five were already `lifecycle: resolved`** (V1.58 / V1.66 / V1.67) — yet the bug class persisted: **two of the five prior resolutions were insufficient**.

- `R-V156P0-M004` ("blocking sync I/O in async fs/* handler") — marked resolved in V1.58 (`764cee49`), but `execute_read_file` still called `std::fs::read_to_string` synchronously on `main`. V1.86 T5 (`42335a16`) actually moved it behind `spawn_blocking`.
- `R-V157P1-W001` ("privileged fs/* host-tool path coverage") — marked resolved in V1.58 (`e78c9307`), but the fs/* host-tool execution surface still lacked sibling-prefix / symlink / worker-IPC tests. V1.86 T4 (`0eb9aa4f`) backfilled them.

The three other same-class resolutions (R-V156P0-M002, R-V166-QC2-TOCTOU, R-V165-QC-SUGG-DEFENSE) genuinely held on `main` and were confirmed no-op.

## Guidance

When relying on a prior `lifecycle: resolved` residual as evidence a class is closed (e.g. when scoping a new iteration against the same class), **verify the resolution against current `main` HEAD**, not against the resolution commit's existence:

1. Read the residual's `resolution.commit` + `resolution.plan_id`.
2. `git show main:<path>` (or checkout the relevant file on `main`) and confirm the fix is actually present and still in effect — not regressed, not half-applied, not in a different code path than the finding described.
3. If the resolution addressed a *coverage* gap (tests), confirm the tests actually exist and assert the security/correctness property, not just a status code.
4. Only then treat the class as closed. If the resolution is insufficient, do the real fix and open a **fresh residual** documenting the regression-of-resolution (do not reopen the historical row — keep the audit trail clean).

This generalizes the `.mstar/AGENTS.md` "Pre-existing claim verification protocol" (which is scoped to test-failure pre-existing claims) to **residual-resolution** claims.

## Why This Matters

`lifecycle: resolved` is set once, at resolution time. Code drifts. A resolution that was correct when landed can be silently insufficient if (a) it was partial, (b) it covered a narrower path than the finding implied, (c) the class re-emerged in a sibling path, or (d) a later refactor reverted it. Trusting the status field without re-verification lets the same bug class survive across iterations — which is exactly how Finding 2 (fs/* bypass) went undetected despite `R-V157P1-W001` being "resolved."

## When to Apply

- Scoping a new iteration against a class that has prior "resolved" residuals.
- A QC/QA reviewer citing a prior residual as "already fixed" to narrow review scope.
- Any claim of the form "this was resolved in Vx.y" used to defer or skip work.

## Examples

- **V1.86 (this doc's source)**: 5 same-class residuals all `resolved`; 2 insufficient → fresh `R-V186-REGRESS-M004` / `R-V186-REGRESS-W001` recorded; 3 confirmed.
- **Anti-pattern**: "We don't need to re-test fs path guards, R-V157P1-W001 closed that" → would have shipped Finding 2.

## Prevention

- Resolutions that fix a *coverage* gap should land the actual tests in the same commit and the residual `description` should name them; a coverage residual resolved without naming the tests is a smell.
- When a security/correctness residual is resolved, the resolving commit's diff should touch the exact code path the finding described — verify on close, not just on resolve.
