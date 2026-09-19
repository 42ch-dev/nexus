---
module: nexus (workspace)
date: 2026-09-18
problem_type: engineering_convention
category: engineering-conventions
severity: medium
tags: [cargo, lockfile, resolver, frozen-lock, dependency-sweep, idempotence]
last_updated: 2026-09-18
applies_when: Dependency sweeps or any workflow that must keep Cargo.lock byte-stable while re-resolving individual pinned packages; QC/QA seats verifying a frozen lock
---

# `cargo update -p <crate> --precise` is not idempotent — freeze the lock, then verify read-only

## Context

P0's consolidated dependency sweep (v1.191) needed individual lock entries moved to exact
recorded versions (e.g. `clap_complete 4.6.10`) without disturbing the rest of a frozen
`Cargo.lock`. Naive re-runs of `cargo update -p <crate> --precise <same-version>` turned
out to be non-idempotent: a second identical invocation toggled in-range companion
packages (`prost-derive`'s `itertools` 0.14.0 → 0.13.0, `tempfile`'s `getrandom` 0.4.3 →
0.3.4) even though the targeted package itself stayed put.

## Guidance

- **Treat every resolver invocation as a mutation.** Run it once per frozen checkpoint,
  inside the task that owns the checkpoint (the named artifact custodian), never from
  review seats.
- **After the freeze, verification is read-only only:**
  - `cargo metadata --locked` / `cargo check --locked` / `cargo test --locked` (these do
    not rewrite the lock),
  - `cargo tree --locked -i <pkg>@<ver>` for provenance proofs,
  - `bash tooling/check-graph-pins.sh` for exact-pin honesty.
- **Compare the lock, not the exit code**: after any checkpoint, diff the lock against the
  pre-checkpoint baseline and classify every delta as a recorded target, a recorded
  payload move, or an unexplained churn (rollback required).
- **Companion churn pattern:** `--precise` re-resolution can move unrelated in-range
  dependencies that share a solver frontier with the target. When a fix round re-pins one
  package, re-check the neighbours that were changed by the previous resolution and
  restore them with targeted `--precise` calls on their *dependents* if needed — or take
  the whole lock from the last known-good commit and re-apply only the intended moves.

## Why This Matters

Exit code 0 from a resolver says nothing about lock stability: the solver may legally
choose different in-range versions for transitive neighbours on each run. A "green" build
after a stray re-resolve can silently ship dependency drift that no classification table
recorded — exactly what a consolidated sweep is supposed to prevent.

## When to Apply

- Dependency sweeps that land multiple PRs' end states in one lock.
- Fix rounds that touch the lock after QC started reviewing.
- Any workflow where a reviewer/QA seat is tempted to "just re-run cargo update".

## Examples

- v1.191 P0 T1: `cargo update -p clap_complete --precise 4.6.10` (fix for an over-shoot)
  also pulled `itertools`/`getrandom`/`windows-sys`/`socket2` edges to different in-range
  versions; each had to be restored by re-resolving its dependent.
- The differential proof: `git show <base>:Cargo.lock` vs the frozen lock, package-set
  diff, then block-level `prost-derive` comparison to catch edge-only movement that a
  package-set diff misses.
