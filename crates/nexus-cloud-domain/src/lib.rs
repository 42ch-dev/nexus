//! Nexus Cloud Domain — Platform-bridge logic for User and Pairing.
//!
//! This crate owns **domain logic** for `User` and `Pairing` aggregates.
//! All **types** come from `nexus-contracts` (contracts-first).
//! No HTTP — cloud-sync owns transport.

pub mod errors;
pub mod pairing;
pub mod user;

pub use errors::CloudDomainError;
