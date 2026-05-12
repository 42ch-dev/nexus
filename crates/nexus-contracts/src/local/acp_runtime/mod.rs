//! ACP runtime local types.
//!
//! These types describe CLI ↔ daemon / CLI ↔ CDN interactions that
//! `nexus-platform` never observes.

pub mod daemon_status_v2;
pub mod registry_manifest;
pub mod trace;

pub use daemon_status_v2::*;
pub use registry_manifest::*;
pub use trace::*;
