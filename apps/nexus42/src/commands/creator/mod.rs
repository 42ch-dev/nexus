//! Creator command module router (v1.189 P1-T3 basic-cli / legacy-cli split).

#[cfg(feature = "legacy-cli")]
#[path = "legacy_impl.rs"]
mod legacy_impl;
#[cfg(feature = "legacy-cli")]
pub use legacy_impl::*;

#[cfg(all(not(feature = "legacy-cli"), not(feature = "basic-cli")))]
compile_error!("Select a CLI cohort: enable `basic-cli` or `legacy-cli` feature");

#[cfg(all(feature = "basic-cli", not(feature = "legacy-cli")))]
mod slim;
#[cfg(all(feature = "basic-cli", not(feature = "legacy-cli")))]
pub mod world;
#[cfg(all(feature = "basic-cli", not(feature = "legacy-cli")))]
pub use slim::*;
