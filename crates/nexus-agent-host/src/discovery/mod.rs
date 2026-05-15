//! Provider discovery: config-based, PATH-scan, ACP-registry.
//!
//! Discovery is deterministic and ordered:
//! 1. Static config from `config.toml` (highest priority).
//! 2. PATH scan for known native commands.
//! 3. ACP registry via `nexus_acp_host::RegistryClient`.
//!
//! Deduplication: config entries suppress matching auto-discovered entries.
//! Native CLI entries use distinct `provider_ids` (R-004, R-008).

pub mod acp_registry;
pub mod catalog;
pub mod config;
pub mod path_scan;

pub use catalog::ProviderCatalog;
