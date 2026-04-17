//! Domain local types.
//!
//! Domain aggregates and value objects that are local-only — not carried
//! in sync bundles or platform HTTP payloads.

pub mod agent_profile;
pub mod local_identity;
pub mod manuscript_state;
pub mod outbox_entry;
pub mod reference_source;
pub mod runtime_mode;
pub mod workspace_binding;

pub use agent_profile::*;
pub use local_identity::*;
pub use manuscript_state::*;
pub use outbox_entry::*;
pub use reference_source::*;
pub use runtime_mode::*;
pub use workspace_binding::*;
