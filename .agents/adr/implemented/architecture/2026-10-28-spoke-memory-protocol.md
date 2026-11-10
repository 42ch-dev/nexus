# ADR: SPOKE Memory Protocol

Status: implemented — upstream protocol published (spoke 0.16.0) and the base family plus reflection are wired in nexus; transfer remains explicitly unsupported
Date: 2026-10-28
Scope: The decision to adopt the published `spoke-memory` protocol as the memory family; the wire contract itself is upstream-owned and referenced, not restated here.
Source: migrated from the retired tracked specs corpus

## Problem

**Landing reconciliation (2026-11-01).** Written as a research proposal against SPOKE 0.14.1, this family has since landed upstream: release 0.15.0 (2026-10-30) added it additively (schemas + codegen, `MemoryPort` + validators, Connect dispatch rows, `RemoteMemoryService`, and a fixtures-only reference engine), and 0.15.1 (2026-10-31) is docs/CI only; both crates.io and npm publish 0.15.1. The reference policy engine is `fixtures/memory-engine/rust` (`spoke-fixture-memory-engine`), a path-dependency-only crate that is **never published** — it is implementation, not protocol, so it is not part of the surface a conforming provider must reproduce (§1.1 still binds: the protocol surface is schemas + operations + Connect arbitration). This nexus repository moved its five lockstep pins 0.14.1 →

## Decision

The decision is to adopt the published upstream `spoke-memory` protocol as the memory family, at the 0.16.0 lockstep, rather than define a Nexus-specific wire contract or extend `KnowledgeEntry` / `modules.memory` to carry memory.

Nexus's current consumption is described in [`spoke-memory-provider-binding.md`](../../../../docs/specs/architecture/spoke-memory-provider-binding.md) and [`spoke-memory-conformance-staging.md`](../../../../docs/specs/architecture/spoke-memory-conformance-staging.md): the local provider is the default, remote binding is an opt-in Connect client, the base operations plus reflection are wired through `MemoryPort`, and transfer/import-export remain explicitly unsupported as N4-T. Operation names, wire fields, and `MEM-R###` invariants are owned by the published upstream protocol and are referenced, not restated here.

## Alternatives considered

#### 16.1 Rejected alternatives

- **Reuse `KnowledgeEntry` + `modules.memory`:** rejected. KnowledgeEntry is a narrative KB envelope (canonical_name required, `ke-ownership` disclosure semantics); memory needs candidates, grants, fencing, and receipts that do not exist there. The product-local boundary note is amended instead (§18.2).
- **Expand `BaselinePorts`:** rejected; memory is optional and independent (mirrors `ExtractionPort` standing outside `BaselinePorts`/`FullPorts`).
- **Public storage SPI behind the engine:** rejected; the replacement boundary is `MemoryPort` only. Provider-private storage and transactions stay provider implementation detail; the reference engine is pure algorithm policy.
- **CRDT / offline multi-master / federated recall:** rejected by MEM-R001; conflicts with provider-enforced fencing and atomic review.
- **Webhooks / streaming / callback async:** rejected; one async model (durable receipt + poll, §7.0.3) covers all long-running ops.
- **Inheriting `Scope.viewpoint` authorization:** rejected (MEM-R020); it is a KB disclosure carrier, not a memory grant system.
- **Merge-into-existing review action:** omitted from v1 (current Nexus precedent declares it unimplemented; composable from promote + update). Ablation: no confirmed v1 requirement fails.
- **Unchunked single-message export:** rejected; Connect invoke payloads are bounded and memories must never silently truncate (§6.13).

#### 16.2 Ablation evidence for retained components

- `MemoryGrant` (+`shared_access`) — removal would lose provider-enforced cross-client authorization with unambiguous shared/context coverage; kept for MEM-R006/R017/R018.
- `MemoryCandidate` (+`capture_scope`) — removal would lose never-recall-before-review and the admitted-placement fence for review outcomes; kept for MEM-R004/R009.
- `MemoryReceipt` (content-free) + dedup tombstones — removal would lose uncertain-outcome recovery and erasure-safe audit; kept for MEM-R008/R010/R021/R022.
- `ReadSelection` explicit modes + `ProvenanceFilter` — removal would reintroduce null/empty-array ambiguity and conflate placement with provenance; kept for §6.12/§6.13 semantics.
- `memory.subjects.fence` (context target) / `memory.imports.discard` — removal would lose cross-client per-context fencing and explicit pre-activation rollback; kept for MEM-R024 and §12.4.
- `authority_generation` — removal would make stale post-cutover bindings indistinguishable; kept for MEM-R027.
- Tombstones — removal would lose delete-vs-audit reconciliation; kept for MEM-R014.

## Consequences

The adopted contract fixes these boundaries and non-goals:

#### 1.1 In scope

- A complete wire and service contract for per-subject long-term memory: spaces, subjects, capture, candidate review, retained records, recall, profiles, reflections, grants, write fencing, and versioned export/import.
- The single public replacement boundary `MemoryPort` (in-process) and `RemoteMemoryService` (SPOKE Connect responder seam) through which a product embeds or serves a memory provider.
- A separately reusable reference algorithm engine, delivered as the fixtures-only crate `fixtures/memory-engine/rust` (`spoke-fixture-memory-engine`, internal facade type `MemoryEngine`) containing deterministic, I/O-free algorithm policy. The engine receives already-authorized data as pure function inputs and returns classification, ranking, reflection-assembly, and transfer-transformation results. It holds **no storage abstraction** and enforces no authorization, CAS, idempotency, fencing, or transaction semantics — all constraint enforcement belongs to the provider behind `MemoryPort`, in the provider's own transaction layer. The engine is **not a protocol participant and not part of the published protocol surface** (schemas + operations + Connect arbitration): it is path-dependency-only test tooling and a conformance reference, and each provider — including Nexus — implements its own policy under the behavioral-conformance split (§17).
- Concurrency, revocation, freeze, delete, and provider-switching semantics that hold across multiple clients of one authoritative provider.

#### 1.2 Non-goals

- No distributed exactly-once delivery, no cross-provider or cross-space atomic transactions, no CRDT, no offline multi-master replication, no federated recall, no silent local fallback. One `MemorySpace` has exactly one authoritative provider (MEM-R001).
- No transport definition beyond mapping onto the existing SPOKE Connect envelope (`schemas/connect/*`, verified §18). No HTTP routes, headers, or methods appear in this protocol.
- No mandate of identical ranking, summarization, or reflection algorithms across providers (§9, item 2). Algorithm policy is engine-versioned, not protocol-versioned.
- No space-creation or provider-bootstrap operation: spaces and initial administrator grants are provider-operator provisioned out of band (§7.0.6). No product identity model: `Subject` is a stable opaque ID with optional namespaced kind metadata; the protocol never closes the kind vocabulary to `Creator|Character` or any product enum, and equal subject IDs across systems never auto-link identities (MEM-R002).
- No changes to `BaselinePorts`, the six baseline port families, the core op table (`upsert|promote|relate|check|assemble|project|compute|extract`), or the `port.*` D4 catalogue, except the additive extension points listed in §13 and in the companion implementation plan.

---
