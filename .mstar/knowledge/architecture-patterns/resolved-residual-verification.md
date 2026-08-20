---
module: harness-process
date: 2026-07-03
last_updated: 2026-07-06
problem_type: process-convention
category: architecture-patterns
severity: medium
tags: [residual, lifecycle, verification, regression-of-resolution, security, deferred-already-satisfied]
applies_when: treating a prior residual's `lifecycle` field (either `resolved` OR `deferred`) as evidence about the current code state
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

## Symmetric case: deferred-but-already-satisfied

The `resolved` direction (above) says "resolved doesn't guarantee closed." The **inverse** is also true: a residual marked `decision: defer` / `target: V1.(N+1)` does **not** guarantee the work is still pending. The prior iteration's fix-wave may have already shipped the production code that satisfies it, leaving only test-coverage + paperwork for the target iteration.

**V1.93 (this section's source)**: of 8 residuals deferred to V1.93 with `target: V1.93`, the V1.93 architect's Prepare-pass code-read found **3 already code-complete on `main`**:
- `R-V192P0-001` (SAN invalidation on re-bind) — `cert_covers_bind_host` + `try_load_existing` fallthrough shipped in V1.92 PR #120 (`feff6f36`); V1.93 only added an IPv6 regression test.
- `R-V192P0-002` (TLS graceful-shutdown grace cap) — `boot.rs` `graceful_shutdown(Some(shutdown_grace))` shipped in V1.92; V1.93 only added a mapping test.
- `R-V192P1-003` (desktop connection-config tests) — 5 tests already existed; V1.93 added 1 gap-fill.

The V1.93 plans were originally scoped as "implement from scratch"; the architect re-scoped them to "verify + add tests + resolve paperwork" after reading the code. Scoping them as new implementation would have duplicated shipped code.

**Guidance** (symmetric to the resolved case): when scoping a **convergence iteration** against a backlog of deferred residuals, **read the current code for each residual before writing the implement plan**. A residual's `target: V1.(N+1)` is a planning claim made at V1.N close; the V1.N fix-wave may have satisfied it incidentally. Plans that "implement" already-shipped code waste effort and risk re-implementing incorrectly; plans that "verify the shipped code + close the test/paperwork gap" are honest and faster.

## Why This Matters

`lifecycle: resolved` is set once, at resolution time. Code drifts. A resolution that was correct when landed can be silently insufficient if (a) it was partial, (b) it covered a narrower path than the finding implied, (c) the class re-emerged in a sibling path, or (d) a later refactor reverted it. Trusting the status field without re-verification lets the same bug class survive across iterations — which is exactly how Finding 2 (fs/* bypass) went undetected despite `R-V157P1-W001` being "resolved."

Symmetrically, `decision: defer` / `target: V1.(N+1)` is set once, at deferral time. The deferring iteration's own fix-wave may satisfy it before the target iteration begins. Trusting the deferral field without a code-read lets a convergence iteration re-implement already-shipped work.

## When to Apply

- Scoping a new iteration against a class that has prior "resolved" residuals.
- Scoping a **convergence iteration** against a backlog of prior "deferred" residuals (the symmetric case — read the code; deferrals may already be satisfied).
- A QC/QA reviewer citing a prior residual as "already fixed" to narrow review scope.
- Any claim of the form "this was resolved in Vx.y" or "this is deferred to Vx.y" used to defer, skip, or re-scope work.

## Examples

- **V1.86 (resolved-is-a-claim)**: 5 same-class residuals all `resolved`; 2 insufficient → fresh `R-V186-REGRESS-M004` / `R-V186-REGRESS-W001` recorded; 3 confirmed.
- **V1.93 (deferred-but-already-satisfied, symmetric)**: 3 of 8 `target: V1.93` residuals already code-complete on `main` from the V1.92 fix-wave; plans re-scoped from "implement" to "verify + test + paperwork."
- **Anti-pattern (resolved)**: "We don't need to re-test fs path guards, R-V157P1-W001 closed that" → would have shipped Finding 2.
- **Anti-pattern (deferred)**: writing a V1.93 plan that says "implement SAN validation" without reading `tls/mod.rs` first → would have re-implemented `cert_covers_bind_host` that already shipped in V1.92.

## Prevention

- Resolutions that fix a *coverage* gap should land the actual tests in the same commit and the residual `description` should name them; a coverage residual resolved without naming the tests is a smell.
- When a security/correctness residual is resolved, the resolving commit's diff should touch the exact code path the finding described — verify on close, not just on resolve.
