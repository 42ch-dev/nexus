---
title: Audit fixtures by behaviour, not by helper name
category: testing-patterns
track: knowledge
last_updated: 2026-11-22
created: 2026-11-22
date: 2026-11-22
status: active
problem_type: testing_pattern
module: repo-wide test fixtures
severity: medium
symptoms:
  - "the census says every copy is migrated and a bare call site still leaks"
  - "the same defect is fixed four times in one plan because each round found one more instance"
  - "a helper is deleted as unused, then re-added by a later change for symmetry"
root_cause: "A fixture census built from `rg 'fn <helper>'` only sees named helpers. Call sites that open a pool, forget a tempdir or decode stderr inline have no name to search for, so they survive every round and only surface when a crate-wide gate looks for the effect."
resolution_type: workflow_improvement
---

# Audit fixtures by behaviour, not by helper name

## Context

One plan consolidated test fixtures across six domains. Its census was built the obvious way: `rg -c 'fn fresh_pool'`, `rg -c 'fn fixture_creator_id'`, `rg -c 'fn dto'`. Each round then found more of the same defect that the count said was gone:

- A `stderr` decoder sat in one test file that was never in the "19 pairs" inventory — found by a crate-wide `rg 'fn stderr'`.
- Two `std::mem::forget(dir)` calls lived inside a shared helper that returned only a pool — found by `rg 'mem::forget'`, not by any helper name.
- An `arc_pool_pool_first` wrapper introduced the drop-order defect it was meant to remove, and made 76 call sites wrong at once.
- Three counts in the plan itself were wrong (23 vs 22 copies, 18 vs 19 files, 33/34/36 targets), each discovered only when someone re-derived the number.

## Guidance

**Search for the effect, not the helper.** For each invariant, pick a grep that matches the behaviour wherever it appears:

| Invariant | Search for | Not |
|---|---|---|
| one pool bootstrap | `fn fresh_pool`, `fn admitted_pool`, `init_engine_pool`, `open_pool` | the one helper you know about |
| tempdir outlives the pool | `let (pool, _dir)`, `let (pool, dir)`, `mem::forget(` | the helper that returns the tuple |
| one stderr decoder | `fn stderr`, `from_utf8_lossy(&.*stderr)` | the helper inventory |
| one seed INSERT | `INSERT INTO <table>` | `fn seed_world` |

The second column is the gate. Put each as a one-line `rg` with expected-empty output in the plan's acceptance section, and run them at the end — not per task, where each looks clean.

**Counts drift; re-derive instead of trusting.** A count written once ("23 copies", "19 pairs") becomes false the moment a neighbouring change lands. State the command next to the number, and have the implementer re-run it after every round. Three of this plan's own figures were wrong for exactly this reason.

**State the rule where the wrapper lives.** When a helper exists to enforce an ordering (owner declared last, pool drops first), the doc comment must state the rule and not the tuple shape. A shape that gets cited as its own justification ("this is the order the suite bound them") outlives the reason and misleads the next reader.

## Why This Matters

The failure is quiet: the census is green, the plan is on its last task, and a bare `mem::forget` in an unrelated file leaks a temp directory on every run. Four rounds of "found one more" is the signal that the census method is wrong, not that the reviewers were careless.

## When to Apply

- Any plan whose deliverable is consolidating or deleting test fixtures.
- Reviewing a fixture inventory: ask what `rg` produced it and whether a bare call site would appear in that output.
- After a guard is added: check that the same invariant cannot be re-expressed without a name — a tuple order, an inline decoder, a bare `mem::forget`.
