//! nexus-orchestration — engine adapter, capability registry, worker manager.
//! Authoritative design: `.agents/plans/knowledge/orchestration-engine-v1.md`.

pub mod capability;
pub mod embedded_skills;
pub mod engine;
pub mod preset;
pub mod schedule;
pub mod scheduler;
pub mod storage;
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
