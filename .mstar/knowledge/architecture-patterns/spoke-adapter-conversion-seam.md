---
module: nexus-knowledge
date: 2026-07-26
problem_type: architecture_pattern
category: architecture-patterns
severity: low
plan_id: 2026-07-26-v1.139-p1-rust-domain-migration
tags: [spoke, adapter, conversion-seam, extensions, knowledge-entry, wire-boundary]
last_updated: 2026-07-26
applies_when: Consuming SPOKE protocol packages in a product that has product-local body content not yet covered by spoke's typed body schema
---

# SPOKE Adapter Conversion-Seam Pattern

## Context

When nexus (or any SPOKE consumer) adopts spoke's published `KnowledgeEntry` wire type, the product has body content (summary, attributes, tags, state, computable) that spoke deliberately keeps product-local. The challenge: use spoke's wire type for identity/status/extensions/ops while preserving product-local body content — without duplicating the spoke type or losing body fidelity.

## Guidance

**Use a conversion seam (Mechanism A):** define a product-domain type (`WorldKbEntry`) that carries the full body content, plus two `From`/`Into` impls that convert to/from `spoke_schemas::KnowledgeEntry` at the wire boundary. The conversion seam is the **sole** extension point when spoke later completes its body schema.

### Why this works

1. **spoke-operations (`validate_promote`, `transition_status`, `build_assemble_packet`, `merge_extensions`) operate on identity/status/revision/extensions — they do NOT consume body content.** So the body only needs to be live in the product domain type; the spoke side never needs it for ops.
2. **The `extensions.<namespace>` bag** carries product-local identity fields (e.g. `extensions.nexus.world_id`) — typed accessors in the adapter crate read/write these without touching body content.
3. **Future spoke body completion:** when spoke declares typed body fields, only the two `From` impls need extending. The validation engine, store, and all downstream consumers: zero change.

### The call-boundary invariant (HARD)

Every `spoke-operations` invocation receives the **converted spoke type only** — never the product domain type. The adapter crate is the sole boundary that constructs spoke objects and delegates. Static enforcement: `rg "spoke_operations::" <product-crate>/` returns zero hits (adapter is the only direct caller).

## When to Apply

- Adopting spoke (or any protocol with a wire type + product-local extensions) where the product has body content the protocol keeps opaque
- The adapter pattern: thin delegation facade (re-export/pass-through, not thick mapping)
- Pre-1.0 product where body schema is still evolving on both sides

## Examples

### nexus V1.139 implementation

```rust
// crates/nexus-knowledge/src/world_kb/knowledge_entry.rs
pub use spoke_schemas::KnowledgeEntry;  // wire boundary type

pub struct WorldKbEntry {
    pub entry_id: ...,
    pub canonical_name: ...,
    pub status: ...,
    pub body: WorldKbBody,  // nexus-local body (summary/attributes/tags/state/computable)
    // ... identity fields that map to extensions.nexus on conversion
}

impl From<WorldKbEntry> for spoke_schemas::KnowledgeEntry {
    // Pack identity → extensions.nexus via adapter accessors
    // Map status → spoke core vocab
    // Body: forward maps state/computable; summary/attributes/tags stay on WorldKbEntry
}

impl From<spoke_schemas::KnowledgeEntry> for WorldKbEntry {
    // Reverse: extract extensions.nexus via adapter accessors
    // Body: reverse maps state/computable; summary/attributes/tags default to None
}
```

```rust
// Lifecycle ops (T3): convert before delegating
fn confirm(&mut self, base_revision: u32) -> Result<()> {
    let spoke_entry: spoke_schemas::KnowledgeEntry = self.clone().into();
    nexus_spoke_adapter::ops::assert_revision(&spoke_entry, base_revision)?;
    nexus_spoke_adapter::ops::transition_status(&spoke_entry, ...)?;
    // ...
}
```

### Version bump considerations

When spoke publishes a new version (e.g. 0.1.1 → 0.2.0) that adds typed body fields:
1. Bump the pin (lockstep all 4 packages)
2. If the new body fields break the conversion seam's destructuring pattern, add `..` to ignore them (defer body alignment to a dedicated iteration)
3. The `From` impls are the only extension point — update them when ready to align body content

## Why This Matters

This pattern minimizes future change: when spoke completes its body schema, the product's validation engine, store, consumers, and lifecycle ops are **unaffected**. Only two `From` impls change. Without the seam, every spoke body change would cascade through the product's entire body-consuming codebase.
