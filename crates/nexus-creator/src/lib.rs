//! Nexus Creator — Creator aggregate logic and local state.
//!
//! This crate owns the Creator aggregate and local identity management.
//! Public types come from `nexus-contracts`; domain logic (validation,
//! conversions, local cache records, path helpers) lives here.
//!
//! # Architecture
//!
//! - Creator aggregate with status management and style profiles
//! - Local identity for `local_only` mode (anonymous + persistent)
//! - ID generation and validation helpers
//! - All wire types re-exported from `nexus-contracts`

// Pedantic clippy lints — suppress noisy ones during pre-1.0 development.
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::uninlined_format_args)]

pub mod creator;
pub mod errors;
pub mod local_identity;

// Re-export error types
pub use errors::CreatorError;

// Re-export local identity helpers
pub use local_identity::is_valid_creator_id;

// Re-export common types from nexus-contracts
pub use nexus_contracts::{
    CreatorId, CreatorStatus as ContractCreatorStatus,
    RegistrationSource as ContractRegistrationSource,
};
