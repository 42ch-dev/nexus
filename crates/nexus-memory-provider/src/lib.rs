//! The in-process `spoke_operations::MemoryPort` provider (v1.245 extraction).
//!
//! This crate owns the local memory provider: the `MemoryPort` boundary and the
//! ops it serves, admission (freeze allowlist, grant coverage, revision CAS,
//! idempotent replay), the provider-private transaction layer inside the core
//! writer guard, the publish journal and its settlement, the legacy
//! projection/read bridge, and the Nexus-owned declared policy.
//!
//! What it deliberately does NOT own: `CoreError`/`CoreResult` and the core
//! taxonomy its refusals project onto. `ProviderError` is this crate's public
//! error, and the projection onto the core taxonomy lives in `nexus-core`
//! (`impl From<ProviderError> for CoreError`), so this crate names no core type.
//! The workspace binding, the service handle, the transports and the daemon stay
//! where they were.
//!
//! Dependency direction: `nexus-core → nexus-memory-provider`, never back.
