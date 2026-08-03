//! # nexus-spoke-adapter
//!
//! The single boundary that crosses between nexus domain concerns and SPOKE
//! standard objects, and the **spoke capability-aggregation layer** for nexus
//! (tracked spec `spoke-adapter-architecture.md` §1.2 / §7 / §7.4). It owns
//! four surfaces:
//!
//! 1. **Typed accessors** over the `extensions.nexus` namespace on a spoke
//!    [`KnowledgeEntry`] — see the [`extensions`] module.
//! 2. **Surface A delegation** of standard lifecycle invariants to
//!    [`spoke_operations`] — see the [`ops`] module.
//! 3. **The `WorldKbEntry` ↔ spoke `KnowledgeEntry` conversion seam** — see
//!    the [`conversion`] module (V1.145 P1a).
//! 4. **The production `BaselinePorts` implementation** ([`NexusAdapter`]
//!    + 6 port impls) — see the [`adapter`] module (V1.145 P1b).
//!
//! Surfaces 1–3 are **pure delegation / conversion**: where `spoke-operations`
//! already exports a function, this adapter re-exports or thin-wraps it, and it
//! introduces no parallel nexus types where spoke already provides them.
//!
//! Since V1.141 the crate also flat-re-exports the spoke adapter **port
//! traits + orchestration entrypoints** (Surface B, spec §7.3) so consumers
//! implement spoke's ports and call spoke's `orchestrate_*` functions through
//! this single import boundary — still no nexus logic, still pure pass-through.
//!
//! Since V1.142 the crate additionally re-exports spoke 0.5.0's timeline
//! beat-assist pure helpers (Surface A — `filter_timeline_events_by_moment_scale`,
//! `order_timeline_events_by_ids`, `order_timeline_events_by_precedes`, plus
//! the `OrderTimelineEventsByPrecedesOptions` operand) — again pure
//! pass-through, again no nexus logic.
//!
//! Since V1.145 P1b the crate also hosts the **production `BaselinePorts`
//! implementation** (`NexusAdapter` + 6 port impls in the [`adapter`]
//! module, spec §7.4). The adapter consumes `nexus-local-db` storage primitives
//! and bridges spoke's sync port traits to async `SQLite` I/O. This makes
//! `nexus-spoke-adapter` the capability-aggregation layer, while
//! `nexus-local-db` is pure storage (no spoke-adapter dep — spec §8 dep-graph
//! reversal).
//!
//! ## Call-boundary invariant (HARD)
//!
//! Every public function in this crate accepts and returns only spoke
//! standard objects (`KnowledgeEntry`, `Finding`, `Scope`, `PromoteRequest`,
//! `AssemblePacket`, `ExtensionMap`). There are no nexus wrapper types here
//! — the adapter IS the boundary.
//!
//! (The production [`adapter::NexusAdapter`] is the documented
//! exception: it necessarily touches nexus storage rows on the *inside* of
//! its port impls, but its public surface — the spoke port traits — stays
//! spoke-only.)

pub mod adapter;
pub mod conversion;
pub mod extensions;
pub mod ops;

/// The HostCapabilityManifest single builder SSOT (DF-72 N-C0, §4.1).
///
/// [`manifest::build_local_host_manifest`] is shared by
/// `HostManifestPort::get_host_capability_manifest` and the Connect Host's
/// `ConnectConfig.local_manifest`.
pub mod manifest;

/// Narrative Knowledge Pack build/parse helpers.
///
/// Implements the pack dialect defined in the spoke handbook
/// `domain-profile-narrative-knowledge-pack.md` — a portable lore bundle
/// that ships ordered [`KnowledgeEntry`]s, [`Relation`]s, and optional
/// [`SourceAnchor`]s between narrative hosts, with pack-level metadata
/// on the product transport envelope (spoke 0.7.0 pack-catalog demote:
/// catalog is envelope material, never `modules.*` on KnowledgeEntry /
/// AssemblePacket). Nexus keeps it at the pack envelope root under the
/// product-local key `modules.pack`; it is not written into KE atoms.
///
/// See the [module-level documentation](pack) for the full pack shape,
/// validation rules, and round-trip guarantees.
pub mod pack;

// V1.145 P1b — production adapter re-export so consumers can construct
// `NexusAdapter` through the single spoke-adapter import boundary
// (spec §7.4 import path: `nexus_spoke_adapter::NexusAdapter`).
pub use adapter::NexusAdapter;

// V1.145 P2 — `SpokeBackedKbStore` (the `KbStore` impl injected at the MCA
// `assemble_moment` wiring site) + the scoped-read result type. Re-exported so
// the MCA composition root (`apps/nexus42`) constructs it through the single
// spoke-adapter import boundary (spec §7.4 V1.145 P2 amendment).
pub use adapter::mca_read::{ScopedKbRead, SpokeBackedKbStore};

// ── Spoke type re-exports (consumer convenience) ────────────────────────
//
// Consumers depend on `nexus-spoke-adapter` for both the accessors and the
// spoke types that appear in the public API surface, so they do not need a
// direct `spoke-schemas` / `spoke-operations` dependency just to spell the
// operand types. These mirror `spoke_operations`' own `pub use spoke_schemas`.

pub use spoke_operations::{ExtensionMap, SpokeReject, SpokeRejectCode, SpokeResult};
// Spoke 0.5.0 timeline beat-assist pure helpers (Surface A) — pass-through,
// no nexus logic (call-boundary invariant §7 preserved).
pub use spoke_operations::{
    filter_timeline_events_by_moment_scale, order_timeline_events_by_ids,
    order_timeline_events_by_precedes, OrderTimelineEventsByPrecedesOptions,
};
// Surface A operands (existing) + Surface B operands (spoke ≥ 0.3.0):
// the 4 existing wire types stay listed; the remaining names are the ops
// envelopes + capability types the orchestrators accept/return.
pub use spoke_schemas::{
    AssemblePacket, AssembleRequest, AssembleResponse, CheckRequest, CheckResponse, ComputeRequest,
    ComputeResponse, Finding, HostCapabilityManifest, KnowledgeEntry, ProjectRequest,
    ProjectResponse, PromoteRequest, PromoteResponse, RelateRequest, RelateResponse, Relation,
    Rule, Scope, SourceAnchor, TimelineEvent, UpsertRequest, UpsertResponse,
};

// ── Spoke extension-key newtypes (re-export) ─────────────────────────
//
// The typify-generated `*ExtensionsKey` newtypes are the only way to
// look up the `"nexus"` namespace inside a spoke wire type's
// `extensions` map — the newtypes do not implement `Borrow<str>`, so
// `HashMap::get("nexus")` does not compile (see `extensions.rs`'s
// `nexus_key()` helper for the long-form rationale). Re-exporting the
// key types here lets downstream crates (the production adapter home in
// `nexus-local-db`) build the same lookup without a direct
// `spoke-schemas` dependency for relation / finding ports.
pub use spoke_schemas::{
    finding::FindingExtensionsKey, knowledge_entry::KnowledgeEntryExtensionsKey,
    relation::RelationExtensionsKey, ScopeExtensionsKey,
};

// ── Spoke adapter port + orchestration surface (spoke ≥ 0.3.0) ─────────
//
// Surface B in spec §7.3 — consumers implement these port traits and call
// the orchestrate_* entrypoints; the adapter stays the import boundary.
// Re-exports only — no nexus logic, no parallel types (call-boundary
// invariant §7 preserved). Coverage: 8 port traits + 4 composition/adapter
// alias groups + 4 adapter aliases + 9 orchestrate_* entrypoints + the
// `CheckRunInput` orchestrator parameter type.
pub use spoke_operations::{
    orchestrate_assemble, orchestrate_check, orchestrate_compute, orchestrate_fork_assemble,
    orchestrate_fork_check, orchestrate_project, orchestrate_promote, orchestrate_relate,
    orchestrate_upsert, BaselineAdapter, BaselinePorts, CheckRunInput, ComputableAdapter,
    ComputablePort, ComputablePorts, FindingPort, ForkAdapter, ForkPorts, ForkTimelineQueryPort,
    FullAdapter, FullPorts, HostManifestPort, KnowledgeEntryPort, RelationPort, RuleQueryPort,
    ScopeQueryPort,
};
