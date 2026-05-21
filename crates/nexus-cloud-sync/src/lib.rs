//! Nexus Cloud Sync Library
//!
//! Provides the sync mechanism for CLI ↔ Platform synchronization using
//! Command, `DeltaBundle`, and Outbox patterns.
//!
//! # Domain integration
//!
//! This crate uses [`nexus_cloud_domain`] for User and Pairing domain
//! invariants (account lifecycle, pairing state transitions, domain ↔ contract
//! conversions). Transport-layer code MUST NOT reimplement those rules; route
//! all User/Pairing validation through `nexus_cloud_domain::user` and
//! `nexus_cloud_domain::pairing`.
//!
//! # Feature Flags
//!
//! - **`legacy-sync`** (default off): Enables the legacy cloud sync pipeline —
//!   outbox, push/pull HTTP client, platform HTTP client, and partial apply.
//!   Daemon consumers should NOT enable this; CLI/cloud-line consumers should.
//!
//! # Architecture
//!
//! - **Command**: User-initiated operations (`advance_world`, `sync_push`, etc.)
//! - **`DeltaBundle`**: Batch of deltas sent to platform in a single bundle envelope
//! - **Outbox**: Local `SQLite` queue of pending operations for offline-first sync
//! - **`SyncClient`**: HTTP client for platform API interactions
//! - **`ConflictResolution`**: Optimistic locking with conflict detection
//! - **`PartialApply`**: Handles Phase A/B partial success semantics
//! - **Precheck**: Local validation before HTTP upload to save round-trips
//!
//! # Modules
//!
//! - [`command`]: Sync command types built on generated `SyncCommand`
//! - [`delta_bundle`]: Bundle builder with metadata fields
//! - [`conflict`]: Conflict resolution strategies
//! - [`precheck`]: Local precheck validation stage
//! - [`errors`]: Sync error types
//!
//! ## Legacy sync modules (require `legacy-sync` feature)
//!
//! - [`outbox`]: SQLite-backed outbox for local operation queue
//! - [`sync_client`]: HTTP client for platform sync API
//! - [`platform_client`]: HTTP client for platform registration/verification
//! - [`pull_apply`]: Apply platform pull responses to local outbox
//! - [`pool`]: `SQLite` connection pool for outbox
//! - [`partial_apply`]: Partial apply semantics (Phase A/B)

// ── Always-available modules ──────────────────────────────────────────
pub mod canonical_hash;
pub mod command;
pub mod conflict;
pub mod delta_bundle;
pub mod device_flow_client;
pub mod device_id;
pub mod errors;
pub mod precheck;

// ── Legacy sync modules (gated behind feature flag) ──────────────────
#[cfg(feature = "legacy-sync")]
pub mod outbox;
#[cfg(feature = "legacy-sync")]
pub mod partial_apply;
#[cfg(feature = "legacy-sync")]
pub mod platform_client;
#[cfg(feature = "legacy-sync")]
pub mod pool;
#[cfg(feature = "legacy-sync")]
pub mod pull_apply;
#[cfg(feature = "legacy-sync")]
pub mod sync_client;

// Re-export common types from nexus-contracts
pub use nexus_contracts::{
    generated::{Bundle, SyncCommand},
    local::domain::OutboxEntry,
    BundleType, CreatorId, ManuscriptPhase, WorldId,
};

// Re-export cloud-domain types for User/Pairing invariants.
// All User/Pairing domain validation MUST route through these types
// rather than being reimplemented in transport code.
pub use nexus_cloud_domain as cloud_domain;

// Re-export sync error types (always available)
pub use errors::{SyncError, SyncResult};

// Re-export legacy sync types (gated)
#[cfg(feature = "legacy-sync")]
pub use platform_client::{classify_platform_error, StagedPlatformError};
#[cfg(feature = "legacy-sync")]
pub use pull_apply::{apply_pull_response_to_outbox, PullApplySummary};
