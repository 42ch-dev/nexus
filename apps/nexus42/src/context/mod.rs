//! Context Assembly — CLI-side module.
//!
//! Provides:
//! - Summary generation from local manuscript files
//! - Request/response types for the platform context assembly contract
//!
//! Note (KCA-002 B2): Context assembly runs CLI in-process via
//! `nexus-moment-context-assembly` (`Stage0Assembly` / `TwoStageAssembly`).
//! The daemon `POST /v1/local/context/assemble` route is retired; no daemon
//! proxy is used.

pub mod summary;
pub mod types;
