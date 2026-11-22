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

#![allow(clippy::missing_errors_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::uninlined_format_args)]

pub mod creator;
pub mod errors;
pub mod local_identity;

pub use errors::CreatorError;

pub use local_identity::is_valid_creator_id;

pub use nexus_contracts::{
    CreatorId, CreatorStatus as ContractCreatorStatus,
    RegistrationSource as ContractRegistrationSource,
};
