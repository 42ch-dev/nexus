//! nexus-orchestration — engine adapter, capability registry, graph executor.
//! Authoritative design: `.mstar/specs/orchestration-engine.md`.

pub mod auto_chain;
pub mod auto_chronology;
pub mod capability;
pub mod completion_lock;
pub mod compute_input_builder;
pub mod embedded_rules;
pub mod embedded_skills;
pub mod engine;
pub mod findings_block;
pub mod narrative_index;
pub mod preset;
pub mod preset_gates;
pub mod preset_ids;
pub mod quality_loop;
pub mod resume_rules;
pub mod review_report;
pub mod rules_history;
pub mod rules_layers;
pub mod run_state;
pub mod schedule;
pub mod scheduler;
pub mod skill_link;
pub mod skill_sync;
pub mod stage_gates;
pub mod state_delta;
pub mod storage;
pub mod sync_module;
pub mod system_preset;
pub mod system_preset_dir;
pub mod tasks;
pub mod user_preset_dir;

pub use capability::{
    Capability, CapabilityError, CapabilityRegistry, CapabilityRegistryHolder,
    CapabilityRuntimeDeps, PromptExecutor, PromptPermissionScope, PromptRequest, PromptResult,
    ToolPolicy, WorkspaceExecutor,
};
pub use engine::{
    ChildSessionParams, EngineError, FailedStepWitness, FailurePersistenceDisposition,
    FailureSettlement, FailureSettlementSummary, GraphFlowEngine, OrchestrationEngine, SessionId,
};
pub use preset::resolve_preset;
pub use run_state::{
    durable_cancel_outcome_accomplished, AgentBinding, ChildCheckpoint, OwnedProcessIdentity,
    PresetSourceIdentity, PromptAttempt, PromptPhase, RunCheckpoint, RunDescriptorV1, RunFailure,
    RunRecord, RunStateV1, SettlementResult, TerminalSettlementTarget, WaitKind, WaitRecord,
    WorkflowStateStore,
};
pub use scheduler::{ClockSource, MockClock, Scheduler, SystemClock};
pub use stage_gates::{
    build_preset_input, build_stage_schedule_label, preset_for_stage, WorkFields,
};

use std::path::Path;

/// Ensure all skill links for a preset's roles are set up in the workspace.
///
/// This should be called before starting a preset pipeline to ensure
/// agents can access skill files through `.agents/skills/<slug>/` symlinks.
///
/// Returns the total number of links created/updated across all roles.
#[must_use]
pub fn ensure_preset_skill_links(
    workspace_dir: &Path,
    home_dir: &Path,
    preset: &nexus_contracts::local::orchestration::preset::PresetManifest,
) -> u32 {
    let home_skills_dir = nexus_home_layout::user_skills_dir(home_dir);
    let mut total = 0u32;
    for role in &preset.roles {
        if let Ok(count) = skill_link::ensure_role_skills(
            workspace_dir,
            &home_skills_dir,
            &role.recommended_skills,
        ) {
            total += count;
        }
    }
    total
}
