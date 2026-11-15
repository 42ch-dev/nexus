//! Connect's own error surface.
//!
//! The peer protocol must not depend on `nexus-core`'s `CoreError`: that type
//! carries the local-db/orchestration taxonomy this crate never touches, and
//! importing it would drag the whole core crate back across the boundary the
//! extraction removed. The accept boundary needs exactly one shape — a
//! category-labelled internal failure — and keeps the `internal: {category}`
//! rendering the core error produced.

use thiserror::Error;

/// A Connect-surface failure.
#[derive(Debug, Error)]
pub enum ConnectError {
    /// An internal failure, labelled by its category (config load, identity
    /// persistence, device-id resolution, listener bind).
    #[error("internal: {category}")]
    Internal {
        /// The failure category, already rendered for the operator.
        category: String,
    },
}

/// Connect's result alias.
pub type ConnectResult<T> = Result<T, ConnectError>;
