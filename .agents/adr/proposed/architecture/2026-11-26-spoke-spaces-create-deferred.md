# ADR: Defer `memory.spaces.create` to the MindCore settlement-layer adoption

Status: proposed — Nexus serves no client-named space creation. The local provider advertises `spoke-memory` and answers `memory.spaces.create` with a terminal refusal; the capability is expected with the MindCore settlement-layer adoption, which is itself still proposed.
Date: 2026-11-26
Scope: The disposition of the `memory.spaces.create` operation introduced as a required `MemoryPort` item by SPOKE 0.19.1, and why Nexus neither implements it nor narrows its advertised capability until the MindCore migration.

## Problem

SPOKE 0.19.1 makes `memory.spaces.create` a required method of `spoke_operations::MemoryPort`, gated by the `spoke-memory` capability family. Nexus's only production implementation, `LocalMemoryProvider`, advertises that family (`crates/nexus-memory-provider/src/policy.rs:29` — `CAPABILITIES = ["spoke-memory", "spoke-memory-reflection"]`), so after the 0.19 upgrade (PR #686) the pairing is:

- the capability is advertised, and
- the operation answers `CAPABILITY_PORT_MISSING` with `retry_class: terminal` (`crates/nexus-memory-provider/src/lib.rs:3174`; the conformance fixture does the same at `crates/nexus-memory-conformance/src/server.rs:630`).

That refusal shape is the repository's existing answer for families a build does not serve, and it is honest about the code path — but its name reads as an implementation gap rather than a product decision. Before 0.19.1 the operation did not exist, so advertising the family carried no such expectation.

Implementing the operation is not a mechanical addition. Nexus spaces are operator-provisioned (`LocalMemoryProvider::provision`, spec §7.0.6), and several slices rely on the single-space invariant. A client-named creation path would owe an idempotent receipt, a creator default grant, an admin rule for an existing space, frozen-state handling, and a decision about how operator provisioning and client creation coexist.

Separately, this repository has already recorded that Nexus's memory layer will adopt MindCore's settlement and recall machinery (`2026-11-21-adopt-mindcore-settlement-layer.md`, proposed): MindCore supplies `recall_context`, `absorb`, and `deliberate` behind a Nexus-owned `WorkflowBinding`, and its adapter surface owns the operations Nexus's current provider does not. The owner's direction (2026-11-26) is that the underlying architecture is replaced by MindCore after Monaco, and that these operations are MindCore's by construction.

## Proposal

Keep the terminal refusal for the 0.19 cycle and do not implement client-named space creation in Nexus:

1. `memory.spaces.create` continues to answer `CAPABILITY_PORT_MISSING` / `retry_class: terminal` in both the production provider and the conformance fixture.
2. The advertised `spoke-memory` family is not narrowed: dropping it would also drop reflection, records, receipts, and the rest of the family, which Nexus does serve.
3. Documentation states the product decision plainly (spaces are operator-provisioned; Nexus does not offer client-named creation), so a peer reading `capability_port_missing` understands it as "this product does not offer the operation", not "this build forgot a port".
4. Implementation is deferred to the MindCore adoption, which supplies the operation rather than Nexus emulating it.

## Alternatives considered

1. **Implement `memory.spaces.create` now.** Rejected for this cycle: it would be discarded when the MindCore layer lands (owner direction 2026-11-26), and it would first require product decisions (who may create, quota, interaction with operator provisioning, the single-space invariant) that are not taken.
2. **Narrow the advertised capability so the operation is not expected.** Rejected: the family is negotiated as a unit; withdrawing `spoke-memory` removes reflection and the record/receipt surface Nexus does serve.
3. **Answer with `op_unsupported` instead of `capability_port_missing`.** Rejected for now: it is a different family-level answer and would misrepresent a capability Nexus advertises; the correct wording belongs to the MindCore adoption, where the operation is actually served.
4. **Emulate creation by provisioning lazily.** Rejected: it would create spaces Nexus's provisioning rules did not authorize and break the single-space invariant several slices depend on.

## Consequences

- A 0.19 peer sees the operation in the manifest but receives a terminal refusal. The specification wording (decision point 3) is the mitigation; the refusal code remains a signal a peer may read as a build gap.
- Nexus carries one behaviour difference against the 0.19 surface until the MindCore migration, with no user-visible change for Nexus's own callers (they never create spaces through this operation).
- The MindCore adoption record (`2026-11-21-adopt-mindcore-settlement-layer.md`) gains one more operation it is expected to supply; this record does not itself authorize that adoption, it inherits its timeline and exit conditions.

## Reopen triggers

- The MindCore settlement-layer adoption starts (its `WorkflowBinding` implementation change), at which point `memory.spaces.create` is served there and this record closes as implemented.
- A peer or caller requires client-named space creation before the migration; then the product decisions in Alternative 1 become the work item.
- Upstream SPOKE narrows the `spoke-memory` family so the operation is no longer implied by the advertised capability.
