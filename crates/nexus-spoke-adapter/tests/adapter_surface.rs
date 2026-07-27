//! Parity test: the adapter crate flat-re-exports the spoke 0.4.0 adapter
//! port + orchestration surface. Mirrors spoke-operations' own parity export
//! checklist (`crates/spoke-operations/src/adapter.rs`). Dropping a re-export
//! fails compile at the `use` below; this test additionally exercises the
//! trait + wire-type names so the orchestrator-call monomorphization gap
//! (generic over `P: FullPorts`) is not on this crate's back — spoke's own
//! parity test covers call-path type-checking.

#![allow(dead_code)]
#![allow(unused_imports)]

use nexus_spoke_adapter::{
    // Orchestrators + input (reachability proven by the `use` resolving;
    // call-path monomorphization is covered by spoke-operations' parity test)
    orchestrate_assemble,
    orchestrate_check,
    orchestrate_compute,
    orchestrate_fork_assemble,
    orchestrate_fork_check,
    orchestrate_project,
    orchestrate_promote,
    orchestrate_relate,
    orchestrate_upsert,
    // Existing surface (unchanged — verify still present)
    AssemblePacket,
    AssembleRequest,
    AssembleResponse,
    BaselineAdapter,
    // Composition + aliases
    BaselinePorts,
    CheckRequest,
    CheckResponse,
    CheckRunInput,
    ComputableAdapter,
    ComputablePort,
    ComputablePorts,
    ComputeRequest,
    ComputeResponse,
    ExtensionMap,
    Finding,
    FindingPort,
    ForkAdapter,
    ForkPorts,
    ForkTimelineQueryPort,
    FullAdapter,
    FullPorts,
    // New wire types
    HostCapabilityManifest,
    HostManifestPort,
    KnowledgeEntry,
    // Port traits
    KnowledgeEntryPort,
    ProjectRequest,
    ProjectResponse,
    PromoteRequest,
    PromoteResponse,
    RelateRequest,
    RelateResponse,
    Relation,
    RelationPort,
    Rule,
    RuleQueryPort,
    Scope,
    ScopeQueryPort,
    SpokeReject,
    SpokeRejectCode,
    SpokeResult,
    TimelineEvent,
    UpsertRequest,
    UpsertResponse,
};

#[test]
fn adapter_port_traits_and_wire_types_are_reachable() {
    // Port traits (object-safe surfaces — usable as `&dyn Trait`).
    let _: Option<&dyn KnowledgeEntryPort> = None;
    let _: Option<&dyn RelationPort> = None;
    let _: Option<&dyn ScopeQueryPort> = None;
    let _: Option<&dyn FindingPort> = None;
    let _: Option<&dyn RuleQueryPort> = None;
    let _: Option<&dyn HostManifestPort> = None;
    let _: Option<&dyn ComputablePort> = None;
    let _: Option<&dyn ForkTimelineQueryPort> = None;

    // Composition + adapter aliases.
    let _: Option<&dyn BaselinePorts> = None;
    let _: Option<&dyn ComputablePorts> = None;
    let _: Option<&dyn ForkPorts> = None;
    let _: Option<&dyn FullPorts> = None;
    let _: Option<&dyn BaselineAdapter> = None;
    let _: Option<&dyn ComputableAdapter> = None;
    let _: Option<&dyn ForkAdapter> = None;
    let _: Option<&dyn FullAdapter> = None;

    // Orchestrator input type.
    let _: Option<CheckRunInput> = None;

    // New wire types (typify-generated — not Default; binding proves the
    // re-export path resolves).
    let _: Option<HostCapabilityManifest> = None;
    let _: Option<UpsertRequest> = None;
    let _: Option<UpsertResponse> = None;
    let _: Option<PromoteResponse> = None;
    let _: Option<RelateRequest> = None;
    let _: Option<RelateResponse> = None;
    let _: Option<CheckRequest> = None;
    let _: Option<CheckResponse> = None;
    let _: Option<AssembleRequest> = None;
    let _: Option<AssembleResponse> = None;
    let _: Option<ProjectRequest> = None;
    let _: Option<ProjectResponse> = None;
    let _: Option<ComputeRequest> = None;
    let _: Option<ComputeResponse> = None;
    let _: Option<Rule> = None;
    let _: Option<TimelineEvent> = None;
    let _: Option<Relation> = None;
    let _: Option<Scope> = None;

    // Existing Surface A surface (must remain reachable, unchanged).
    let _: Option<AssemblePacket> = None;
    let _: Option<Finding> = None;
    let _: Option<KnowledgeEntry> = None;
    let _: Option<PromoteRequest> = None;
}
