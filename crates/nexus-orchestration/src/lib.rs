//! nexus-orchestration — engine adapter, capability registry, worker manager.
//! Authoritative design: `.agents/plans/knowledge/orchestration-engine-v1.md`.

pub mod capability;
pub mod engine;
pub mod preset;
pub mod schedule;
pub mod storage;
pub mod system_preset;
pub mod tasks;
pub mod worker;

pub use capability::{Capability, CapabilityError, CapabilityRegistry};
pub use engine::{ChildSessionParams, EngineError, GraphFlowEngine, OrchestrationEngine};
pub use worker::WorkerManager;
