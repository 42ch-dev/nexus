//! # nexus-spoke-adapter
//! contract — docs/specs/architecture/spoke-adapter-architecture.md §1.2 / §7 / §7.4.
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
//! 3. **The `KnowledgeEntryRecord` ↔ spoke `KnowledgeEntry` conversion seam** — see
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
pub mod constraint;
pub mod conversion;
pub mod extensions;
// `ResolvedExtractionPort` + `extract_candidates` around upstream
// `orchestrate_extract`. No orchestration/core import; no persistence.
pub mod extraction;
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

pub use adapter::NexusAdapter;

pub use adapter::host_manifest_port::ObservedPeerHost;

pub use adapter::knowledge_entry_port::is_world_conflict_reject;

#[cfg(feature = "compute")]
pub use adapter::computable_port::is_module_identity_missing_reject;

#[cfg(feature = "compute")]
pub use adapter::computable_port::is_safe_module_id;

pub use adapter::mca_read::{ScopedKbRead, SpokeBackedKbStore};

pub use spoke_operations::{
    filter_timeline_events_by_moment_scale, order_timeline_events_by_ids,
    order_timeline_events_by_precedes, OrderTimelineEventsByPrecedesOptions,
};
pub use spoke_operations::{ExtensionMap, SpokeReject, SpokeRejectCode, SpokeResult};
pub use spoke_schemas;
// the 4 existing wire types stay listed; the remaining names are the ops
// envelopes + capability types the orchestrators accept/return.
pub use spoke_schemas::{
    AssemblePacket, AssembleRequest, AssembleResponse, CheckRequest, CheckResponse, ComputeRequest,
    ComputeResponse, ExtractRequest, ExtractResponse, Finding, HostCapabilityManifest,
    KnowledgeEntry, ProjectRequest, ProjectResponse, PromoteRequest, PromoteResponse,
    RelateRequest, RelateResponse, Relation, Rule, Scope, SourceAnchor, TimelineEvent,
    UpsertRequest, UpsertResponse,
};

pub use spoke_operations::{
    orchestrate_assemble, orchestrate_check, orchestrate_compute, orchestrate_fork_assemble,
    orchestrate_fork_check, orchestrate_project, orchestrate_promote, orchestrate_relate,
    orchestrate_upsert, BaselineAdapter, BaselinePorts, CheckRunInput, ComputableAdapter,
    ComputablePort, ComputablePorts, FindingPort, ForkAdapter, ForkPorts, ForkTimelineQueryPort,
    FullAdapter, FullPorts, HostManifestPort, KnowledgeEntryPort, KnowledgeQueryPort, RelationPort,
    RuleQueryPort, ScopeQueryPort,
};

pub use spoke_operations::{
    parse_tool_capability_id, validate_manifest_tools, validate_tool_arguments, ToolDescriptor,
};

pub use spoke_operations::{
    orchestrate_extract, ExtractRunInput, ExtractionPort, ExtractionResult,
};

pub use extraction::{
    extract_candidates, ExtractCandidatesOutcome, NativeExtractionOutput, ResolvedExtractionInput,
    ResolvedExtractionPort,
};

pub use adapter::knowledge_entry_port::put_knowledge_entry_in_tx;
pub use adapter::rule_query_port::orchestrate_check_world_scoped;

// `ToolContext::new_simulated` selects it, so a context built through that twin
// records a `nexus.kb_snapshot.write` put and writes no row.
pub use adapter::knowledge_entry_stub::{StubKnowledgeEntryMutation, StubKnowledgeEntryPort};
