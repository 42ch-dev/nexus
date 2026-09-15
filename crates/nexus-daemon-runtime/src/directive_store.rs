//! Thin daemon re-export over the core directive composition root
//! (v1.190 P2-T3).
//!
//! Both [`DirectiveStore`] adapters (lifecycle + read-only) and the whole
//! moment-directive resolution/lifecycle logic live in [`nexus_core`]; this
//! module keeps the historical `nexus_daemon_runtime::directive_store` path
//! alive for the CLI composition (P6-T4's shared-writer files) until it
//! consumes `nexus_core` directly.

pub use nexus_core::{LocalDirectiveStore, ReadOnlyDirectiveStore};
