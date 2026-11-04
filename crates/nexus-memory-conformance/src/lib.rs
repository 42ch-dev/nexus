//! Independent durable SPOKE memory-provider fixture for the N3 conformance
//! suite.
//!
//! The crate owns its SQLite file, schema, storage and bootstrap policy so the
//! conformance suite has a durable provider that shares nothing with
//! `nexus-core`'s local provider. It is a test fixture, never a production
//! server: the private operation slices implement the protocol duties, and
//! [`server`] assembles the full `MemoryPort` and the signed Connect memory
//! service over them.

// The assembled `MemoryPort` in `server` consumes the request-scope bridge
// (`Admitted::from_scope`), while this slice's own direct-admission entry
// points and its pending/failed receipt states stay exercised by the embedded
// tests and the §7.0.3 async owners.
#[allow(dead_code)]
mod candidates;
// Operator/test-only fault points and the controllable clock; consumed by the
// assembled mutation path.
mod faults;
// The published provider policy and the authenticated peer→principal table.
mod policy;
mod provider;
// The D15 raw-request digest scope and the wire error-envelope normalization.
mod raw_invoke;
// As in `spaces`: the outer convenience wrappers stay the slices' own tested
// entry points, while the assembled port commits through the caller-owned
// `*_in_tx` siblings so effect, receipt and dedup share one transaction.
#[allow(dead_code)]
mod records;
#[allow(dead_code)]
mod recall;
// The assembled `MemoryPort`, its admission wrapper and the signed service.
mod server;
// The same split as `records`: the assembly needs this slice's authorization,
// grant-loading and caller-owned seams, and its outer wrappers remain its own
// embedded-test entry points.
#[allow(dead_code)]
mod spaces;
mod storage;

pub use candidates::{
    RetentionSweep, DEDUP_RETENTION_SECONDS, IDEMPOTENCY_RETENTION_SECONDS,
    RECEIPT_RETENTION_SECONDS,
};
pub use faults::{FaultHold, FaultHoldFailure, FaultHoldLost, FaultPoint, FixtureFaults};
pub use policy::PeerDirectory;
pub use provider::{FixtureIdentity, FixtureMemoryProvider};

/// Failure to open the fixture store or provision its durable bootstrap state.
#[derive(Debug, thiserror::Error)]
pub enum FixtureError {
    /// SQLite connection or pool failure.
    #[error("fixture storage failure: {0}")]
    Storage(#[from] sqlx::Error),
    /// The embedded fixture migration failed to apply.
    #[error("fixture migration failure: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),
    /// The database holds a bootstrap state this provider cannot serve.
    #[error("fixture bootstrap invariant violated: {0}")]
    Bootstrap(String),
}
