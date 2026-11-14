# Strategy simulation — capability boundary

## Scope

What a strategy simulation reports, what it refuses to claim, and where the disclosure must appear.

## Current state

### What simulation answers

| Answers | Does not answer |
|---|---|
| Which states are reachable from the entry node | What any node would output |
| Where the graph can stop and wait (statically) | Whether a wait's signal will actually fire |
| The worst effect class that can escape, rolled up across nests | What a real judge branch would decide |
| Whether two runs of the same document produce identical bytes | Whether the graph is useful |

### Levels

`simulation_level` is an open string. `reachability` is the only level produced today:

- `llm_judge` nodes follow their single declared `default` edge and are listed in `stubbed_judges`.
  A judge with zero, several, or a dangling default edge is a deterministic error, never a successful
  trajectory.
- No provider is called. A trajectory carries no cost, no latency and no output.
- Waits are derived from the reachable set: a `manual_wait` or a reachable `exit_when: manual` yields
  `reason: manual` with the node id; a document-global `signals[].action: pause` yields
  `reason: signal` with the signal name. **A wait is reported when its node is visited, and the walk
  visits only a judge's single `default` edge**, so a wait reachable only through a judge's go or
  no-go branch is never predicted — an under-report in the judge case, by construction, not an
  omission to fix. Within the visited set the list over-reports instead: a wait the run would never
  reach is still named, and a pause signal is named whatever the graph does with it.

Consumers MUST treat an unknown `simulation_level` as a newer, unsupported level rather than assuming
`reachability`.

### Disclosure

Any surface offering an approve decision from a simulation MUST show, for the document hash being
approved: the level in words, the stubbed-judge count with ids, the fence reading, the static label on
predicted waits, and a statement that a passed simulation does not preview output. A bare "simulated"
badge is a defect.

### Upgrade path

Deeper levels are tracked, not promised: judge-branch exploration (I-000416), a budgeted model-call
mode (I-000417), and signal-conditioned waits (I-000418). Each carries its trigger; none is implied by
this document.

## Invariants

- **Nothing executes.** The walk builds no graph and resolves no capability. `write_candidate` and
  `external` reach no port. A simulator that ever needs to dispatch must do it through a registry that
  is fresh per call; a shared registry would make the isolation claim untestable (see
  `.agents/knowledge/architecture-patterns/isolation-tests-need-observable-difference.md`).
- **The effect rollup is a fence, not a forecast.** `effect_summary` is the maximum over visited nodes
  and nested children under the total order `pure < read < write_candidate < external`, taken from the
  generated enum's `Ord`. A deferred (`defer_until_apply`) nest contributes its declared class and its
  body is not walked. An effect that only happens on a collapsed branch is still reported.
- **The wait list is not exhaustive.** Because judges are stubbed to their `default` edge, a wait
  behind any other branch is never predicted; no surface may present the list as complete, and the
  disclosure obligation in §Disclosure is what keeps that visible.

## Where it lives

- Trajectory contract: `schemas/daemon-api/orchestration/simulation-trajectory.schema.json`.
- Walk: `crates/nexus-orchestration/src/simulation/`, behind the `simulate` feature.

## Change obligations

A change to what a level answers, to the effect order, or to the disclosure set updates this page in
the same change and the trajectory schema with it. A new level is added here only once it produces a
trajectory, not when its issue is filed.
