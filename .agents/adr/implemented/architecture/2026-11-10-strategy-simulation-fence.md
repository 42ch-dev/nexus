# ADR: Strategy Simulation Fence — Simulate Before Approve

Status: implemented — P2 simulator shipped in `v1.237-p2-simulator`; simulation is the standing safety fence before graph approval
Date: 2026-11-10
Scope: Simulate-before-approve for Workflow IR: the trajectory contract, static reachability with stubbed judges, declared fidelity limits, deterministic output, effect rollup, and the requirement that Approve never renders as an enabled no-op.
Source: split from the archived Canvas + Workflow IR redesign and Dynamic Workflow Generation and Orchestration Simulation records

## Problem

Agent-generated or user-composed strategies can contain unreachable states, unintended waits, and write/external effects. Treating generation or validation as sufficient approval evidence would let an unexamined graph reach production. A simulator that called live capabilities would itself mutate Work, World KB, or cloud state, while an unexplained “simulated” badge would overclaim what a static walk proves.

The fence therefore has to be isolated, deterministic, honest about its fidelity, and visibly bound to the document being approved. It must supplement—not replace—the engine's run-time `WaitingForInput` gates.

## Decision

### 1. Simulate-before-approve fence

Validation and simulation precede Approve. Approve is an explicit human or policy-bound action bound to the content hash that passed both. A patch after Validate or Simulate invalidates the earlier result and requires both steps again; results from one graph never approve another. Generate, Compose, Validate, and Simulate do not silently Apply a candidate or open a driven production session.

Approve and Apply are authorized, audited, and CAS-guarded on revision/content hash. Apply persists the approved identity; only then can schedule admission or run start use it. Approve gates graph adoption, while `WaitingForInput` gates continuation of an executing run. Neither replaces the other.

A surface cannot render Approve as an enabled no-op. When approval is offered, the surface presents the current document hash's simulation level in words, stubbed-judge ids/count, effect fence, statically labelled predicted waits, and the disclosure that a passing simulation does not preview output. Missing or stale simulation evidence does not produce an enabled approval action, and a bare “simulated” badge is a defect.

### 2. Trajectory and current contract

Simulation emits a deterministic trajectory describing the reachable nodes, predicted waits, stubbed judges, effect classes, and rolled-up effect summary for the candidate. The walk is static and isolated: no provider is called, no capability is resolved or dispatched, and `write_candidate` / `external` effects reach no live port. Work Artifact writes use the dry-run/stub boundary rather than touching `Works/`.

The shipped simulator reuses the Workflow IR graph semantics without creating a second orchestration runtime. Production remains graph-flow; dry-run differs in capability ports and persistence isolation.

The normative current state—including the trajectory schema, supported level, exact wait derivation, error cases, effect ordering, and disclosure requirements—lives in [docs/specs/orchestration/strategy-simulation.md](../../../../docs/specs/orchestration/strategy-simulation.md). This ADR records why the fence exists and does not duplicate that evolving contract.

### 3. Reachability and stubbed judges

The shipped level performs static reachability from the entry node. An `llm_judge` follows its declared default edge and is disclosed as stubbed; no model chooses a branch. Invalid default-edge cardinality or a dangling default is a deterministic error, not a successful trajectory.

Predicted waits are derived statically and can over-report. They also under-report in one direction that follows from stubbing: a node whose `exit_when` is `llm_judge` contributes only its default edge, so a wait behind any other branch of that node is never visited and never named. Neither direction is a defect to fix at this level, but the under-report is the one a disclosure must state, because a reader cannot infer it from the list. The result does not claim that a signal will fire or that a human will continue. Consumers treat an unknown simulation level as a newer unsupported level rather than silently interpreting it as the shipped level.

### 4. Fidelity boundary

Simulation answers which states are reachable, where the graph can wait statically, which judges were stubbed, the worst effect class that can escape across visited nodes and nests, and whether identical input produces identical trajectory bytes.

It does **not** prove node output, real judge decisions, signal delivery, output quality, graph usefulness, provider cost/latency, or production behavior. It provides no exactly-once external-side-effect guarantee because external effects are never committed. Deeper judge exploration, model-call modes, and signal-conditioned waits are separate tracked upgrades, not implied by P2.

### 5. Determinism and effect rollup

The same document produces identical trajectory bytes. Effect summary uses the total order `pure < read < write_candidate < external`, rolled up over visited nodes and nested children. A deferred nest contributes its declared class without walking its body. The fence reports the worst case even when the effect occurs only on a collapsed branch; nesting cannot hide `write_candidate` or `external`.

Simulation remains side-effect-free with respect to Work trees, World KB settlement, ACP narrative commits, and cloud sync. If a future simulator ever dispatches, it uses a registry fresh per call so isolation remains observable and testable.

## Alternatives considered

- **Approve directly after generation or validation.** Rejected: structural validity alone does not disclose reachable waits or effect exposure.
- **Run capabilities live during simulation.** Rejected: the review step would perform the very Work, narrative, and cloud mutations the fence must prevent.
- **Introduce a separate simulation runtime.** Rejected: simulation and production must share graph semantics; only capability ports and persistence isolation differ.
- **Claim full behavioral preview from static reachability.** Rejected: stubbed judges and static waits cannot predict output, branch choice, or signal delivery.
- **Hide nested effects or report only the happy path.** Rejected: Approve needs the worst-case fence, including effects on collapsed branches and deferred nests.
- **Use Approve as a run continuation or replace `WaitingForInput`.** Rejected: graph adoption and run continuation are complementary gates.
- **Guarantee exactly-once external effects in simulation.** Rejected: simulation stubs or records effects and never commits them.

## Consequences

P2 supplies a deterministic, isolated review artifact that makes reachability, waits, and effect exposure visible before approval. Approval becomes meaningful evidence bound to one document hash rather than an enabled button with no simulation behind it.

The cost is deliberate conservatism: static waits can over-report, judges disclose only their default path, and the result says nothing about output quality or production usefulness. Those limits must be shown wherever Approve is offered. Deterministic errors for invalid judge defaults and fail-closed source identity preserve safety at the cost of refusing ambiguous graphs.

P2 ships validation and simulation only. Canvas compose, pinned pattern packages, the approval freeze and Apply shipped in v1.239 and v1.241 and are now governed by [the composition spec](../../../../docs/specs/orchestration/strategy-composition.md); dual-read loading, IR-primary authoring and retirement of hand-authored YAML remain in [Canvas Composition and Pattern Packages](../../proposed/architecture/2026-11-10-canvas-composition-and-pattern-packages.md), and the document model and validation parity remain in [Workflow IR Document Model](2026-11-10-workflow-ir-document-model.md).
