//! Per-moment context assembly for ACP sessions.
//!
//! Two assembly strategies:
//!
//! **Stage-0** (`local_only` mode):
//! Assembles context from local sources only: SOUL sections, long-term memories,
//! fragment keywords, and the user prompt.
//!
//! **Two-Stage** (`local_first` / `cloud_enhanced` modes, requires `cloud-stage` feature):
//! Stage-1 calls platform `context/assemble` API; Stage-2 merges with local data.
//!
//! # Features
//!
//! - `cloud-stage` — enables `TwoStageAssembly` and platform API types
//!   (adds dependency on `nexus-cloud-sync`).

pub mod stage0;

// Cloud-stage types gated behind feature flag
#[cfg(feature = "cloud-stage")]
pub mod cloud_stage;

// Re-export primary types
pub use stage0::Stage0Assembly;
#[cfg(feature = "cloud-stage")]
pub use cloud_stage::TwoStageAssembly;
