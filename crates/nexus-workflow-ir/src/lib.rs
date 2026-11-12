//! Thin Workflow IR contract.
//!
//! `schemas/daemon-api/orchestration/workflow-document.schema.json` is the single source of
//! truth; this crate re-exports the generated document types and adds the Q2 structural checks
//! that schema cannot express. It is deliberately dependency-thin: `nexus-contracts` is the only
//! dependency, so generators, Canvas and validators share the contract without pulling preset
//! loading or orchestration.

pub use nexus_contracts::generated::daemon_api::orchestration::workflow_document::*;

pub mod structure;
pub use structure::{validate, Violation, MAX_NEST_DEPTH};
