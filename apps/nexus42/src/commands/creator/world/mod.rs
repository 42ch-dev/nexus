//! `creator world` module router (v1.189 P1-T3 basic-cli / legacy-cli split).

pub mod kb;

#[cfg(feature = "legacy-cli")]
#[path = "legacy_impl.rs"]
mod legacy_impl;
#[cfg(feature = "legacy-cli")]
pub use legacy_impl::{active_creator_id, open_workspace_pool, *};

#[cfg(not(feature = "legacy-cli"))]
mod slim;
#[cfg(not(feature = "legacy-cli"))]
pub use slim::*;
