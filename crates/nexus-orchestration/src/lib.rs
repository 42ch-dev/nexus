//! nexus-orchestration — engine adapter, capability registry, worker manager.
//! Authoritative design: `.agents/knowledge/specs/orchestration-engine.md`.

pub mod capability;
pub mod embedded_skills;
pub mod engine;
pub mod preset;
pub mod schedule;
pub mod scheduler;
pub mod skill_link;
pub mod skill_sync;
pub mod storage;
pub mod sync_module;
pub mod system_preset;
pub mod system_preset_dir;
pub mod tasks;
pub mod user_preset_dir;
pub mod worker;

pub use capability::{Capability, CapabilityError, CapabilityRegistry};
pub use engine::{ChildSessionParams, EngineError, GraphFlowEngine, OrchestrationEngine};
pub use preset::resolve_preset;
pub use scheduler::{ClockSource, MockClock, Scheduler, SystemClock};
pub use worker::WorkerManager;

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
