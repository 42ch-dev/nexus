//! Independent durable SPOKE memory-provider fixture for the N3 conformance suite.
//!
//! The crate owns its SQLite file, schema, storage and bootstrap policy so the
//! conformance suite has a durable provider that shares nothing with
//! `nexus-core`'s local provider. It is a test fixture, never a production
//! server: the private operation slices grow the provider, and the full
//! `MemoryPort` implementation and loopback service are assembled only when
//! every base operation has a real implementation.

// Exercised by its embedded tests until the assembly task implements
// `MemoryPort` over these slices; the allowance disappears with that consumer.
#[allow(dead_code)]
mod candidates;
mod provider;
// Exercised by its embedded tests until the assembly task implements
// `MemoryPort` over these slices; the allowance disappears with that consumer.
#[allow(dead_code)]
mod spaces;
mod storage;

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
