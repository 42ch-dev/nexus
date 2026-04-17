//! Preset loader module.
//!
//! Loads preset bundles (YAML manifest + optional prompt templates) and
//! validates them per `orchestration-engine-v1.md` §7.6.
//!
//! Types: `nexus-contracts::local::orchestration::preset`.
//! Loader + validation: this module (`loader.rs`).

pub mod loader;
pub mod manifest;

pub use loader::{load_preset, load_preset_from_str, LoadedPreset, PresetLoadError, ValidationProblem};
