//! API handler modules

/// Convert between wire-equivalent typify-inlined types via JSON round-trip.
///
/// typify emits a distinct struct copy for every response that references a
/// shared schema (e.g. `world_kb_graph_response::NexusWorldKbEntityProjection`
/// vs the root `WorldKbEntityProjection`). These copies are structurally
/// identical — the wire-drift gate proves equivalence — so the round-trip is
/// semantically a no-op. Used at handler response construction boundaries
/// where a shared helper returns the root type but the response struct expects
/// its own inlined copy. (plan v1.138 P1)
///
/// # Panics
///
/// Panics if serialization or deserialization fails. This is safe because the
/// types are wire-equivalent (drift-gate-proven); a failure indicates a schema
/// regression, not a runtime data issue.
#[allow(clippy::missing_panics_doc)]
pub(crate) fn wire_cast<T, S>(value: S) -> T
where
    T: serde::de::DeserializeOwned,
    S: serde::Serialize,
{
    serde_json::from_value(serde_json::to_value(value).expect("wire_cast: serialize"))
        .expect("wire_cast: deserialize (types are drift-gate-proven equivalent)")
}

/// Derive the RAW user home from the `.nexus42` root.
///
/// `nexus-home-layout` helpers take the raw user home and join `.nexus42`
/// internally (conventions/nexus-home-layout-path-helpers.md); passing the
/// `.nexus42` root double-nests to `<home>/.nexus42/.nexus42/...` (F-QA-001).
/// Same derivation as the V1.170 boot.rs Deviation #1 fix (`boot.rs:287-297`).
pub(crate) fn raw_user_home(
    nexus_home: &std::path::Path,
) -> Result<&std::path::Path, crate::api::errors::NexusApiError> {
    nexus_home
        .parent()
        .ok_or_else(|| crate::api::errors::NexusApiError::Internal {
            code: "NEXUS_HOME_PARENT_MISSING".to_string(),
            message: "nexus_home has no parent directory".to_string(),
        })
}

pub mod acp;
pub mod actor_knowledge;
pub mod agent_host;
pub mod chapters;
pub mod character_memory;
pub mod character_tom;
pub mod characters;
pub mod check;
pub mod compute_modules;
pub mod compute_runs;
pub mod creators;
pub mod directive;
pub mod findings;
pub mod fork;
pub mod host_tool_executor;
pub mod host_tool_handlers;
pub mod inspector;
pub mod kb;
pub mod memory;
pub(crate) mod memory_pipeline;
pub mod monitoring;
pub mod narrative;
pub mod orchestration;
pub mod outline;
pub mod permissions;
pub mod preset_management;
pub mod reading;
pub mod references;
pub mod runtime;
pub mod soul_narrative_synthesizer;
pub mod strategy;
pub mod timeline;
pub mod timeline_events;
pub mod tools;
pub mod works;
pub mod workspace;
pub mod workspaces;
pub mod world_findings;
pub mod world_kb;
pub mod world_kb_guards;
pub mod world_kb_pack;
pub mod world_rules;
