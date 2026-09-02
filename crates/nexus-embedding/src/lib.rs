//! Embedding readiness contract (roadmap row `harness` RN-OGA-3).
//!
//! Embeddings are a **Nexus Platform-provided advanced feature**. The OSS
//! product ships **no embedding execution**: no provider implementation, no
//! API-key configuration surface, no local model downloads. **Users never
//! research embedding API wiring** — when embeddings arrive, the platform
//! supplies them; the OSS side consumes them through the contract defined
//! here.
//!
//! This crate owns the contract types + protocol logic only — it compiles and
//! tests with zero I/O. No daemon route, no DB migration, no runtime caller
//! this iteration; index *storage* lands with the first platform-era
//! consumer. Normative spec: `.mstar/specs/embedding-readiness.md`.

mod identity;
mod protocol;
mod provider;

pub use identity::{EmbeddingIdentity, IdentityComponent};
pub use protocol::{
    verify_index, verify_vector_dimensions, DerivedIndexState, IdentityMismatch, IndexRecord,
    IndexVerdict, PopulateStamp, RebuildReason,
};
pub use provider::{EmbeddingError, EmbeddingProvider, NoEmbeddings};
