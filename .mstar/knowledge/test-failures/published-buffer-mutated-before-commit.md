---
module: apps/web
date: 2026-10-13
problem_type: test_failure
category: test-failures
severity: low
symptoms:
  - "CI job `Web app build & typecheck` intermittently failed `apps/web/src/api/run-observation.test.tsx` with `AssertionError: expected 'e1:2' to be 'e1:3'` at `:385`, while the frame-list assertion immediately above it (`:384`, expecting `['e1:1','e1:2','gap','e1:3']`) passed"
  - "The same job passed on rerun with no code change, and the failure recurred on unrelated heads (v1.207 PR #367, main 51d74f93, main d2a3c8a4)"
root_cause: "`publish()` in `apps/web/src/api/run-observation.ts` stores the live retained `events` array by reference in published React state, while `retain()` mutates that same array in place the moment a frame arrives. A consumer reading `result.current.events` can therefore observe a frame the still-committed previous snapshot does not describe: the new frame paired with the previous snapshot's scalar `lastEventId`."
resolution_type: test_fix
plan_id: 2026-10-13-web-run-observation-flake
tags:
  - web
  - vitest
  - testing-library
  - flake
  - react-state
  - sse
  - run-observation
---

# Reading a mutation-live published buffer: the `run-observation` wait race

> **Status: fixed** — every `waitFor` in `apps/web/src/api/run-observation.test.tsx` that reads `result.current.events` now also requires the published cursor, through a local `waitForCursor` helper (commit `618deb73c`, plan `2026-10-13-web-run-observation-flake`). The hook's shared-buffer design is unchanged.

## Problem

`useRunObservation` keeps one retained observation per run and re-publishes a React state object per frame. The published object is **not immutable**: its `events` field is the retained buffer itself (`apps/web/src/api/run-observation.ts:456-465`), and `retain()` pushes/splices into that buffer as each frame arrives — deliberately, so no frame costs a whole-history copy (design note at `:84-95`, which tells consumers to memoize on the published `revision`).

`renderHook`'s `result.current` is a *committed* snapshot object, but its `events` field is that live array. Between "frame appended to the buffer" and "React commits the matching publish", the previously committed snapshot reports the new frame while its scalar fields (`lastEventId`, `revision`, `liveFrom`, `phase`) are still the old values.

A `waitFor` whose predicate reads `result.current.events` will resolve on the first poll that lands in that window, and the assertion on the next line then reads the stale scalar off the same object. The outcome depends on how long React's commit takes relative to the 50 ms poll — i.e. on machine load.

## Symptoms

- `expected 'e1:2' to be 'e1:3'` at the `lastEventId` assertion, with the `events` assertion one line above passing: the two halves of one committed state object disagreed.
- Failure rate rose under load (reproduced 2/20 at the branch base with CPU hogs; 0/20 idle in a normal run) and the same job passed on rerun with no code change.
- The failure looked like it "belonged" to whatever PR happened to be the head (v1.207, then two unrelated main commits) — the test file had not changed since #352.

## What Didn't Work

- **Blaming the head commit.** Three occurrences, three different heads, one unchanged test file: the diff of the "suspect" commit (`d2a3c8a4`) touched nothing in the observation path.
- **"Not reproducible locally" as a conclusion.** An idle local run passed repeatedly; only a load generator (8 busy-loop processes on a 10-core host) reproduced the CI signature — 2/20. Timing flakes are reproduced by changing the schedule, not by repeating the same load.
- **`vitest` `retry` / longer timeouts.** Both leave the torn read in place and hide the next one; the assertion is correct, the *wait* was wrong.
- **Copying the buffer on every publish (fixing the hook).** It contradicts the documented no-per-frame-copy design and adds production cost to fix a test-only observation defect.
- **Weakening the assertion.** `events` and `lastEventId` disagreeing is a real invariant violation from the consumer's point of view; loosening it would have discarded the signal.

## Solution

Gate every wait that reads the buffer on a published field, in the same predicate:

```ts
/**
 * A wait that reads `events` must also assert a published field: the retained
 * buffer is mutated in place ahead of React's commit, so a wait on the buffer
 * alone can pass while `result.current` is still the previous snapshot.
 */
async function waitForCursor(
  probe: { result: { current: RunObservationResult } },
  cursor: string,
  buffer: () => void,
): Promise<void> {
  await waitFor(() => {
    expect(probe.result.current.lastEventId).toBe(cursor);
    buffer();
  });
}
```

Sites that assert an exact frame count gate on `revision` (or the cursor that the case's later assertions already depend on). Seven of the file's 46 `waitFor` sites read `result.current.events`; all seven were converted. Nothing else about the tests — or the hook — changed.

## Why This Works

`lastEventId` / `revision` / `phase` / `liveFrom` are scalars copied at publish time, so they only move when a new snapshot commits. A predicate that requires one of them cannot be satisfied by the previous snapshot object, and once it is satisfied the buffer it reads is described by the committed snapshot's own publish — the frame list and the cursor agree by construction. The wait keeps its original meaning ("this frame arrived"), it just stops reading the one field that is not snapshot-scoped.

## Prevention

- **Consumer rule** (already stated at `run-observation.ts:84-95`): memoize on the published `revision`, or read a published scalar — never treat `result.current.events` as a snapshot-scoped value. Any new consumer or test of this hook inherits this rule; the helper's comment carries it in the test file.
- When a `waitFor` predicate reads an array/object that the producer mutates outside React state, ask whether that value is committed or live; if it is live, add a published field to the predicate.
- Reproduce timing flakes by changing the schedule (parallel load, worker count) before declaring a test stable; `N` idle repetitions of a load-dependent race prove nothing.
- Treat "same job passed on rerun, same commit" as flake evidence, not as a fix: it identifies the class, and the class still needs the read/write-order defect removed.
