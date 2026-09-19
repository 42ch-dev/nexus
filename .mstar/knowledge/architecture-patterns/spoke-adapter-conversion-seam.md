---
module: nexus-knowledge
date: 2026-07-26
problem_type: architecture_pattern
category: architecture-patterns
severity: low
tags: [spoke, adapter, conversion-seam, extensions, knowledge-entry, wire-boundary, surface-a]
last_updated: 2026-09-18
applies_when: Consuming SPOKE protocol packages in a product whose domain type must round-trip body content plus product-local identity through the spoke wire type; using SPOKE pure-helper delegates (Surface A) where the caller manages its own storage
---

# SPOKE Adapter Conversion-Seam Pattern (Surface A)

> **Companion:** [`spoke-adapter-port-orchestration-adoption.md`](spoke-adapter-port-orchestration-adoption.md) covers **Surface B** (port traits + injection orchestration, spoke ≥ 0.3.0, adopted V1.141). This doc covers **Surface A** (pure-helper delegates, V1.139 baseline). Both surfaces coexist on the same adapter boundary — read both.

## Context

When nexus (or any SPOKE consumer) adopts spoke's published `KnowledgeEntry` wire type, the product has body content (summary, attributes, tags, state, computable) that spoke deliberately keeps product-local. The challenge: use spoke's wire type for identity/status/extensions/ops while preserving product-local body content — without duplicating the spoke type or losing body fidelity.

> **Update (2026-09-18):** spoke's closed body now carries those five fields (since spoke 0.4.0), and the conversion maps them in both directions; the seam remains load-bearing for identity/owner/extensions round-trip and for the shape conversions nexus `computable: bool` ↔ spoke map and `attributes` object ↔ `Vec<BodyAttribute>`. The pattern statement below is the V1.139-era rationale, retained for provenance.

## Guidance

**Use a conversion seam (Mechanism A):** define a product-domain type (`KnowledgeEntryRecord`, formerly `WorldKbEntry`) that carries the full body content, plus adapter-owned free functions (`knowledge_record_to_spoke` / `spoke_to_knowledge_record`) that convert to/from `spoke_schemas::KnowledgeEntry` at the wire boundary. `From`/`Into` impls are not available here: both types are foreign to the adapter crate (orphan rule E0117), which is why the seam is free functions. The conversion seam is the **sole** extension point when spoke later completes its body schema.

### Why this works

1. **spoke-operations (`validate_promote`, `transition_status`, `build_assemble_packet`, `merge_extensions`) operate on identity/status/revision/extensions — they do NOT consume body content.** So the body only needs to be live in the product domain type; the spoke side never needs it for ops.
2. **The `extensions.<namespace>` bag** carries product-local identity fields (e.g. `extensions.nexus.world_id`) — typed accessors in the adapter crate read/write these without touching body content.
3. **Future spoke body completion:** when spoke declares typed body fields, only the two conversion functions need extending. The validation engine, store, and all downstream consumers: zero change.

### The call-boundary invariant (HARD)

Every `spoke-operations` invocation receives the **converted spoke type only** — never the product domain type. The adapter crate is the sole boundary that constructs spoke objects and delegates. Static enforcement: `rg "spoke_operations::" <product-crate>/` returns zero hits (adapter is the only direct caller).

## When to Apply

- Adopting spoke (or any protocol with a wire type + product-local extensions) where the product has body content the protocol keeps opaque
- The adapter pattern: thin delegation facade (re-export/pass-through, not thick mapping)
- Pre-1.0 product where body schema is still evolving on both sides

## Examples

### nexus implementation (V1.139 seam; names as of v1.184)

```rust
// crates/nexus-knowledge/src/world_kb/knowledge_entry.rs
pub struct KnowledgeEntryRecord {   // v1.184: owner-aware aggregate (formerly WorldKbEntry)
    pub entry_id: ...,
    pub canonical_name: ...,
    pub status: ...,
    pub body: KnowledgeEntryBody,  // nexus-local body (summary/attributes/tags/state/computable)
    // ... identity fields that map to extensions.nexus on conversion
}

// crates/nexus-spoke-adapter/src/conversion/knowledge_entry.rs
// Free functions, not `From` impls — both types are foreign to this crate
// (orphan rule E0117; the compiler's own suggestion is a free function).
pub fn knowledge_record_to_spoke(entry: &KnowledgeEntryRecord) -> SpokeKnowledgeEntry {
    // Pack identity/owner → extensions.nexus via adapter accessors
    // Map status → spoke core vocab
    // Body: map all five typed body fields
}

pub fn spoke_to_knowledge_record(
    entry: SpokeKnowledgeEntry,
) -> Result<KnowledgeEntryRecord, KbError> {
    // Reverse: extract extensions.nexus via adapter accessors; owner claim
    // fails closed on absent/ambiguous/malformed input
    // Body: restore the lossless nexus body carrier first, else the typed fields
}
```

```rust
// Lifecycle ops: delegate through the local extension trait (which converts
// to the spoke type internally before calling `spoke_operations`)
use nexus_spoke_adapter::conversion::KnowledgeEntryRecordSpokeExt;

record.confirm(&membership, base_revision, &conflict_check, &visible_manifests)?;
```

### Version bump considerations

When spoke publishes a new version (e.g. 0.11.1 → 0.13.0) that adds typed body fields:
1. Bump the pin (lockstep the `spoke-schemas` / `spoke-operations` (and `spoke-connect`) dependencies plus the root npm packages and `tooling/check-wire-drift.sh::SPOKE_PIN`)
2. If the new body fields break the conversion functions, adapt them (`..` in a destructuring pattern, or an explicit mapping) — never skip the seam
3. The two conversion functions are the only extension point — update them when ready to align body content

## Why This Matters

This pattern minimizes future change: when spoke extends its body schema, the product's validation engine, store, consumers, and lifecycle ops are **unaffected**. Only the two conversion functions change. Without the seam, every spoke body change would cascade through the product's entire body-consuming codebase.
