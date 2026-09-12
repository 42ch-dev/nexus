//! World KB module router (v1.189 P1-T3 basic-cli / legacy-cli split).

pub mod service;

#[cfg(feature = "legacy-cli")]
#[path = "legacy_impl.rs"]
mod legacy_impl;
#[cfg(feature = "legacy-cli")]
pub use legacy_impl::*;

#[cfg(not(feature = "legacy-cli"))]
mod slim;
#[cfg(not(feature = "legacy-cli"))]
pub use slim::*;
