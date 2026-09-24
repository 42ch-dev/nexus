---
module: tooling/check-graph-pins.sh, Cargo feature graph, CI
date: 2026-08-25
problem_type: convention
category: conventions
severity: medium
last_updated: 2026-09-24
tags: 
  - cargo-tree
  - feature-gate
  - graph-pin
  - dependency-graph
  - ci
  - lockstep
applies_when:
  - "Adding or verifying a feature gate that pulls heavy dependencies (connect-client, connect-host, …)"
  - "Claiming a dependency is absent from (or single-version in) the default graph"
  - "Writing CI probes that assert empty/single cargo-tree output"
  - "Bumping a pinned upstream crate that multiple features resolve"
---

# Graph-pin honesty discipline — cargo-tree semantics for feature-gate proofs

## Context

Feature-gated heavy dependencies need CI-pinned graph proofs, and naive pins
produced **two false claims in one iteration** (V1.174):

1. Round-1 claimed the default graph is "free of `spoke-operations`" —
   false: `spoke-operations 0.11.1` is default-graph **prior art** via
   `nexus-spoke-adapter` (since V1.139).
2. Round-1/compass phrased "rmcp behind `connect-client`" as if the crate
   were gated — false at the time: `rmcp 1.8.0` was **already in the default
   graph** through `nexus-daemon-runtime → nexus-acp-host →
   agent-client-protocol =0.11.1 → rmcp (features=["server"])`.

Both were corrected with machine-verified `cargo tree` evidence, and the
probe script `tooling/check-graph-pins.sh` pins the honest form.

**Current state (2026-09 dependency sweep):** ACP core dropped its
rmcp dependency, so the default graph's rmcp obligation flipped from
"exactly one" to **ABSENT** (verified by `cargo tree -i rmcp`);
feature-on graphs (`connect-client`, `embedded-mcp`,
`connect-client,connect-host`) resolve exactly one `rmcp`. ACP itself is
**not** absent from either shipped graph: it rides the unconditional
`nexus-acp-host` normal edge, so nexus42 resolves exactly one
`agent-client-protocol` in every probed combination. graph-flow resolves
exactly one version with an **empty resolved feature set**
(`default-features = false` on every declaration; no `postgres`, no `rig`),
proven by a `-f "{p} feats=[{f}]"` probe, not by lockfile inspection.
Version values are asserted nowhere — see Guidance §5.

## Guidance

### 1. Know the query semantics before asserting absence

- `cargo tree -p <pkg> -i <dep>` answers "which paths bring `<dep>` into
  `<pkg>`'s graph" — an **inverse** query. Prior art legitimately occupies
  the graph: check what sibling crates already depend on before claiming a
  dep is gated.
- **Dev-dependencies of the queried package DO appear in `-p` output.**
  `cargo tree --edges normal` (shipped-graph semantics) is the default edge
  mode and is what the pin script uses, so test-only dev-deps (e.g. an rmcp
  `client` dev-dep, `agent-client-protocol` as a dev-dep) never count — but
  do not assume `-i` queries behave the same; verify the edge mode in the
  actual probe.
- **Graph presence ≠ protocol use.** `libp2p` (the pinned lockstep value — currently whatever `spoke-connect` requires) appears in the
  `connect-client` graph via spoke-connect's **non-optional base dep** even
  though the remote layer never dials libp2p. The pin is single-version
  lockstep, not "no libp2p anywhere". The pin's *value* is the manifest's business and moves on every upstream lockstep (the companion `libp2p` pin moves with `spoke-connect`); do not restate it in this doc or in a probe.

### 2. Formulate pins as honest obligations, not absolute absence

| Obligation form | Example |
|-----------------|---------|
| **No new default-graph package** (delta) | "V1.174 adds no new default-graph package" — instead of "graph free of X" |
| **Exactly-one-version lockstep** in every feature combination | `cargo tree -i rmcp` → ABSENT under default, exactly one `rmcp 3.2.0` under `-F connect-client`, `-F embedded-mcp`, and `-F connect-client,connect-host`; the `=3.2.0` direct pins prevent a second copy when another dep lands |
| **Feature-combination matrix** with expected package sets per combination | default: no `spoke-connect`, no `libp2p`; `-F connect-client`: spoke-connect + spoke-operations appear, and `libp2p` resolves to exactly one version (the pinned lockstep value) |
| **Both graphs `--all-targets`** when lockstep upgrades surface extra literal sites | V1.169 spoke-lockstep practice (feature-gated examples expose literal sites) |

### 3. Probe scripts must not false-green

- **Probe the build shape the obligation is about.** A lib-only `cargo check -p <crate>` can fail while every CI job is green: CI builds tests, and a test build activates the crate's **own dev-dependency**, which unifies a feature the lib-only shape never enables. So "CI is green" and "this check shape compiles" are different facts — name the shape in the obligation (`cargo check -p X`, `cargo check -p X --features Y`, `cargo test -p X`), and when a gate is meant to protect a shape, run that shape rather than a neighbour that happens to pass. When a shape genuinely fails and CI does not, that is a feature-lattice decision (make the shape self-sufficient or retire the cohort), not a probe bug.
- **The `assert_empty` trap:** `cmd 2>&1 || true` followed by a zero-count
  check turns a **failed `cargo tree`** (nonzero exit) into an empty pin —
  a broken probe reads green. Propagate the cargo-tree exit status; only
  treat the output as authoritative when the command itself succeeded.
- Run the probe set in CI and in QA gates (V1.174: 18/18 probes → `graph
  pins OK` in both plan gates).
- When a correction is made, record the prior-art evidence (the exact
  `cargo tree -i` output) in the spec/plan — the pin's job is auditable
  honesty, not a green CI badge.

### 4. Re-verify prior-art claims at each pass

Dependencies of sibling crates land between iterations. A claim verified in
round 1 can be false by round 2 (V1.174's two corrections both came from
re-verification). Re-run the inverse queries whenever the claim is
load-bearing for a feature-gate obligation.

### 5. Assert graph structure, never a version value

A pin asserts *structure* — absence, a single resolved version, a resolved
feature set — and never a version string. A hardcoded `"3.4.0"` in the probe
turns every minor/patch bump into a red CI run and forces a hand edit of the
script merely to restate what the manifest already declares: the manifest is
the single source of truth (workspace members pin exact versions with
`=x.y.z`), and the Dependabot PR updates manifest and lock together. Keep the
single-resolved-version check — it catches genuine version splits — and drop
the value comparison: `assert_exactly_one <crate> <features> <package>`,
`assert_features <crate> <features> <package> <feature-list>`. Version drift
is a manifest-review concern, not a probe concern.

## Why This Matters

Feature gates exist so the default build stays lean; their proof is the graph
pin. A false "absent" claim erodes trust in the gate itself, a probe that
false-greens when `cargo tree` fails is worse than no probe — it certifies a
broken toolchain — and a probe that red-flags a legal dependency bump trains
the team to edit the gate instead of reading it. The honest formulations
(delta obligations, exactly-one lockstep, per-combination matrices,
exit-status propagation, structure-not-value assertions) turn the pin into a
real regression net for the next iteration that touches the graph.

## When to Apply

- Adding a feature-gated dependency to `nexus-daemon-runtime` / `nexus42`
  (extend `check-graph-pins.sh` with the new probes).
- A PR or plan claiming "no new default-graph package" or "dep X gated" —
  demand `cargo tree -i` evidence per package.
- Bumping `spoke-connect`/`agent-client-protocol`/`rmcp` pins — re-run the
  full probe matrix, not just the touched probe.
- Reviewing any CI script that asserts on command output: confirm exit
  status propagates.

## Examples

- `tooling/check-graph-pins.sh` — default graphs free of
  `spoke-connect`/`libp2p`/`rmcp`; exactly one `spoke-operations`, one
  `agent-client-protocol`, and one featureless `graph-flow` in the default
  graphs; exactly one `rmcp` in every feature-on combination;
  `-F connect-client` single-version libp2p. No probe asserts a version
  value (Guidance §5).
- V1.174 corrections: `spoke-operations` prior art (§9.2 of
  `.mstar/iterations/v1.174/specs/v1.174-peer-tools-lock.md`), rmcp default-graph
  reality (same spec, corrections list #1).
- Companion docs: `conventions/wire-contracts-frozen-verification.md`
  (sibling gate discipline — wire contracts, not the dependency graph);
  `engineering/spoke-lockstep-upgrade-procedure.md` (lockstep pins +
  feature-graph evidence); `workflow-patterns/cargo-lockfile-feature-independent-dependabot.md`
  (lockfile presence ≠ compile reachability);
  `architecture-patterns/connect-host-opt-in-feature-gate.md` (N-C0 pattern:
  gate-checkable `cargo tree` for a heavy transport dep).
