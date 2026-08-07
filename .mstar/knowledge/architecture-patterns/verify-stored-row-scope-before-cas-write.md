---
module: apps/nexus42, nexus-spoke-adapter, nexus-daemon-runtime
date: 2026-08-07
problem_type: best_practice
category: architecture-patterns
severity: high
plan_id: 2026-08-06-v1.153-p1-connect-inbound-write-ops-n-c1
applies_when: [reviewing or building a scoped or multi-tenant write path, adding OCC/CAS updates on rows keyed by id+revision only, adding create paths for relations or other endpoint-bearing rows, exposing OCC reject details to a peer or client]
tags: [connect, scope-verification, occ, cas, world-scope, security-review, cross-scope-write]
---

# Verify the Stored Row's Scope Before an Optimistic-Concurrency Write

## Context

V1.153 P1 (N-C1 write ops over Connect) added inbound `upsert` / `promote` / `relate` invoke handling with a fail-closed world-scope gate. L2 review found a **cross-world write**: the gate checked only the **payload-claimed** scope (`extensions.nexus.world_id` in the request), while the orchestrators' stored lookup and CAS update were **scope-agnostic** — they matched by id+revision only. A peer scoped to world A could update or promote an entry **stored in world B** by claiming world A and sending the current revision. The OCC reject details leaked the stored revision, so OCC did not stop the write; the CAS then rewrote the world-B row into an inconsistent state. A second instance: relation **create** did not verify endpoint (`from_id` / `to_id`) worlds, enabling cross-world relation edges (graph pollution plus an id-existence oracle via the FK success/failure differential).

This is a security-review pattern for any scoped write path, not a Nexus-specific incident: it was a real defect found in review (L2 Critical + plan QC converge Warning), fixed with regression tests.

## Guidance

### (a) Gate on the STORED row's scope, not the payload's claim

When a request claims a scope (world / tenant / org), verify the **stored** row's scope equals the claimed scope **before** the write. A payload-claimed scope field is attacker-controlled metadata; it proves nothing about where the row actually lives.

```rust
// Before (vulnerable shape — gate reads only the request's claim):
let claimed_world = req.extensions.nexus.world_id;
assert!(peer_scope.allows(claimed_world));            // gate: payload claim only
let stored = adapter.get_knowledge_entry(entry_id);   // scope-agnostic lookup: WHERE key_block_id = ?
adapter.put_knowledge_entry(CasUpdate { entry_id, expected_base_revision: stored.revision, .. }); // scope-agnostic CAS

// After (pattern): read the stored row's scope through the adapter read port
// and deny on mismatch with zero side effects, before any CAS:
let stored = adapter.get_knowledge_entry(entry_id)?;
verify_stored_worlds(claimed_world, stored.world_id)?;  // mismatch => deny, no side effects
let updated = adapter.put_knowledge_entry(CasUpdate { entry_id, expected_base_revision: stored.revision, .. })?;
```

### (b) Verify endpoint rows on create paths, not only the row being written

`relate` creates edges between two endpoints. Verify the **endpoint rows** (`from_id` / `to_id`) worlds on create — the row being written carries no world of its own to check. Without this, cross-world edges pollute the graph and the FK success/failure differential becomes an id-existence oracle.

### (c) OCC is not a scope boundary — reject details leak stored state

An OCC `VersionMismatch` / `StoredRevisionStale` reject necessarily carries the **current stored revision** (e.g. `actualRevision` / `storeRevision`) so the client can retry. That leak means a scope-agnostic CAS gives an attacker: (1) the stored revision of any row whose id they can guess, and (2) an existence oracle via reject-vs-success differential. Concurrency control protects against *stale* writes — it never protects against *wrong-scope* writes. Scope checks must be a separate gate evaluated before (and independently of) the CAS.

### Fix shape used in nexus

A `verify_stored_worlds` gate that reads the stored entry/relation world via the adapter read ports and denies on mismatch with **zero side effects**, placed before the orchestrator CAS in the invoke dispatch path.

## Why This Matters

- **Payload-claimed scope ≠ stored-row scope.** Treating the claim as the gate is the classic cross-tenant write: the authorization check validates input the attacker controls, then the mutation runs against storage matched by id only.
- **Scope-agnostic CAS + revision-leaking OCC = a cross-scope write.** Each piece looks fine in isolation; the composition is the vulnerability. The leak makes the CAS a *confirmation oracle* for rows in other scopes.
- This was a **real shipped-adjacent defect** caught only because the review explicitly asked "what does the stored lookup match on, and what does the CAS match on, vs what does the gate check?" — L2 Critical + plan QC converge Warning, fixed with regression tests (wrong-world and absent-scope denied; no side effects on denial).

## When to Apply

- Reviewing any scoped/multi-tenant write path: Connect invoke dispatch, daemon handlers, future N-C2/N-C3 op sets.
- Adding OCC/CAS updates where the storage query is keyed by id+revision and the scope lives in an extension field or a separate table.
- Adding create paths for relations or any row with endpoint references (`from_id` / `to_id` / parent / child ids).
- Any design where the OCC reject payload will be returned to a peer/client that is not the row's owner.

## Examples

### Cross-world update (the V1.153 P1 defect, before the gate)

```text
Peer scope:  world_scope = [world-A], op_scope = [upsert, promote, relate]
Request:     promote(entry_id = E) with extensions.nexus.world_id = world-A
Stored row:  entry E lives in world-B (revision = 7)

Gate:        peer_scope allows world-A ✓  (payload claim only)
Lookup:      get_knowledge_entry(E) -> revision 7   (WHERE key_block_id = ?, no world predicate)
CAS:         put_knowledge_entry(E, expected_base_revision = 7) ✓  -> world-B row rewritten
OCC leak:    if the attacker sends revision 8 instead, the reject reports actualRevision=7
```

### Denied correctly (after the `verify_stored_worlds` gate)

```text
Gate 1:      peer_scope allows world-A ✓
Gate 2:      verify_stored_worlds(world-A, stored.world_id = world-B) ✗ -> deny, zero side effects
CAS:         never reached — no revision leak, no mutation, no existence oracle
```

## See also

- `architecture-patterns/spoke-adapter-port-orchestration-adoption.md` — Surface B orchestrator adoption, CAS-through-orchestrator verification and the OCC port-extension pattern (shared CAS/OCC vocabulary; this doc adds the scope-verification dimension to those orchestrators).
- `architecture-patterns/connect-host-opt-in-feature-gate.md` — N-C0 host surface; N-C1 builds the write spine this pattern protects.
- Spec `fl-r-e1-connect-write-ops-n-c1.md` — world-scoping fail-closed contract (`world_scope` / `op_scope`, OCC error mapping).
