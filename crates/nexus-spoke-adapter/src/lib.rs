//! # nexus-spoke-adapter
//!
//! The single boundary that crosses between nexus domain concerns and SPOKE
//! standard objects. It does two things and nothing else:
//!
//! 1. **Typed accessors** over the `extensions.nexus` namespace on a spoke
//!    [`KnowledgeEntry`] — see the [`extensions`] module.
//! 2. **Thin delegation** of standard lifecycle invariants to
//!    [`spoke_operations`] — see the [`ops`] module.
//!
//! This crate is a **delegation facade** (tracked spec
//! `spoke-adapter-architecture.md` §1.2 / §7): where `spoke-operations`
//! already exports a function, this adapter re-exports or thin-wraps it. It
//! does NOT reimplement any lifecycle invariant, and it introduces no
//! parallel nexus types where spoke already provides them.
//!
//! Since V1.141 the crate also flat-re-exports the spoke adapter **port
//! traits + orchestration entrypoints** (Surface B, spec §7.3) so consumers
//! implement spoke's ports and call spoke's `orchestrate_*` functions through
//! this single import boundary — still no nexus logic, still pure pass-through.
//!
//! Since V1.142 the crate additionally re-exports spoke 0.4.1's timeline
//! beat-assist pure helpers (Surface A — `filter_timeline_events_by_moment_scale`,
//! `order_timeline_events_by_ids`, `order_timeline_events_by_precedes`, plus
//! the `OrderTimelineEventsByPrecedesOptions` operand) — again pure
//! pass-through, again no nexus logic.
//!
//! ## Call-boundary invariant (HARD)
//!
//! Every public function in this crate accepts and returns only spoke
//! standard objects (`KnowledgeEntry`, `Finding`, `Scope`, `PromoteRequest`,
//! `AssemblePacket`, `ExtensionMap`). There are no nexus wrapper types here
//! — the adapter IS the boundary.

pub mod extensions;
pub mod ops;

// ── Spoke type re-exports (consumer convenience) ────────────────────────
//
// Consumers depend on `nexus-spoke-adapter` for both the accessors and the
// spoke types that appear in the public API surface, so they do not need a
// direct `spoke-schemas` / `spoke-operations` dependency just to spell the
// operand types. These mirror `spoke_operations`' own `pub use spoke_schemas`.

pub use spoke_operations::{ExtensionMap, SpokeReject, SpokeRejectCode, SpokeResult};
// Spoke 0.4.1 timeline beat-assist pure helpers (Surface A) — pass-through,
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
    Rule, Scope, TimelineEvent, UpsertRequest, UpsertResponse,
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
