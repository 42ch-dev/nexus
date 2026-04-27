//! Nexus Sync Library
//!
//! Provides the sync mechanism for CLI ↔ Platform synchronization using
//! Command, DeltaBundle, and Outbox patterns.
//!
//! # Architecture
//!
//! - **Command**: User-initiated operations (advance_world, sync_push, etc.)
//! - **DeltaBundle**: Batch of deltas sent to platform in a single bundle envelope
//! - **Outbox**: Local SQLite queue of pending operations for offline-first sync
//! - **SyncClient**: HTTP client for platform API interactions
//! - **ConflictResolution**: Optimistic locking with conflict detection
//! - **PartialApply**: Handles Phase A/B partial success semantics
//! - **Precheck**: Local validation before HTTP upload to save round-trips
//!
//! # Modules
//!
//! - [`command`]: Sync command types built on generated `SyncCommand`
//! - [`delta_bundle`]: Bundle builder with metadata fields
//! - [`outbox`]: SQLite-backed outbox for local operation queue
//! - [`sync_client`]: HTTP client for platform sync API
//! - [`conflict`]: Conflict resolution strategies
//! - [`partial_apply`]: Partial apply semantics (Phase A/B)
//! - [`precheck`]: Local precheck validation stage
//! - [`errors`]: Sync error types

pub mod canonical_hash;
pub mod command;
pub mod conflict;
pub mod delta_bundle;
pub mod device_id;
pub mod errors;
pub mod outbox;
pub mod partial_apply;
pub mod platform_client;
pub mod pool;
pub mod precheck;
pub mod pull_apply;
pub mod sync_client;

// Re-export common types from nexus-contracts
pub use nexus_contracts::{
    generated::{Bundle, SyncCommand},
    local::domain::OutboxEntry,
    BundleType, CreatorId, ManuscriptPhase, WorldId,
};

// Re-export sync error types
pub use errors::{SyncError, SyncResult};
pub use pull_apply::{apply_pull_response_to_outbox, PullApplySummary};
