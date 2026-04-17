//! nexus-orchestration — engine adapter, capability registry, worker manager.
//! Authoritative design: `.agents/plans/knowledge/orchestration-engine-v1.md`.

pub mod capability;
pub mod engine;
pub mod storage;
pub mod system_preset;
pub mod tasks;
pub mod worker;

pub use capability::{Capability, CapabilityRegistry, CapabilityError};
pub use engine::{EngineError, GraphFlowEngine, OrchestrationEngine};
pub use worker::WorkerManager;
