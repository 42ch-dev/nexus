//! Context Assembly — CLI-side module.
//!
//! Provides:
//! - Summary generation from local manuscript files
//! - Local API client for POST /v1/local/context/assemble
//! - Request/response types

pub mod client;
pub mod summary;
pub mod types;
