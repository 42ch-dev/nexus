---
title: A declared semantic needs a production reader, not just a field or a variant
slug: declared-semantics-need-a-production-reader
kind: lesson
status: active
applies_to: [architecture, data-model, review]
---

# A declared semantic needs a production reader, not just a field or a variant

Three defects in one iteration had the same shape: the data model declared a behaviour, the writer side populated it, and **no production code ever read it**. Each looked implemented from every angle except the one that matters.

| Declaration | What it promised | What production actually did |
|---|---|---|
| `WorkflowNestBindingExpansion::DeferUntilApply` | the nest expands at apply time | nothing expanded it; the only production reader excluded deferred children from a hash |
| `WorkflowNode.context_update` | the engine runs the hook at state exit | projects into `StateDefinition`, ranks the state `write_candidate`, never executes |
| `PeerControlLane::enabled` | a disabled lane stops serving | `state()` reported it; admission checked only the allowlist |

Two of the three shipped with comment blocks asserting the behaviour, which is what made them invisible: a reader who greps the symbol finds a definition, a doc comment and a test, and concludes the feature works. The test proves the *setter*, not the *reader*.

## The check

For any field, variant or flag that claims a behavioural effect, ask: **who reads this on a production path, and what do they do differently because of it?**

- A grep that returns the definition, the constructor and the tests is a **negative** answer.
- `#[cfg(test)]` callers do not count. In one case all three call sites of the hook executor sat inside the module's own `#[cfg(test)] mod tests` — the method was exercised, the feature was not.
- A doc comment is not a reader. Two of the three defects above had comments describing the promised behaviour in the present tense.
- A `write_candidate` effect classification is not a reader either; ranking a state in an effect rollup looks like consumption and is not.

Then check the claim in the spec or ADR against that answer. Either implement the reader or correct the claim — leaving both is the defect.

## Why comments make it worse

A comment beside a field is read as evidence that someone thought about the interaction. When it says "admission is decided by the allowlist, so this is observed state, not a gate", it converts a missing gate into a documented design choice, and a reviewer who doubts the gate is now arguing against documentation rather than against an absence. If the promised behaviour genuinely is not wanted, say so where the *operation's name* is defined, because a control called `disable` that cannot stop anything will be trusted by whoever calls it.
