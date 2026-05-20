//! Nexus Cloud Domain — Platform-bridge logic for User and Pairing.
//!
//! This crate owns **domain logic** for `User` and `Pairing` aggregates.
//! All **types** come from `nexus-contracts` (contracts-first).
//! No HTTP — cloud-sync owns transport.

#![allow(clippy::missing_errors_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::uninlined_format_args)]

pub mod errors;
pub mod pairing;
pub mod user;

pub use errors::CloudDomainError;
