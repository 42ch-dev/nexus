//! Transport-neutral World KB core service (v1.189 P1-T2).

mod changes;
mod error;
mod principal;
mod service;
mod world_kb;

pub use error::{CoreError, CoreResult};
pub use principal::Principal;
pub use service::{CoreAccess, CoreAttachedPool, CoreOpenOptions, CoreService};

/// Internal-purpose projection seam for promote/relationship consumers.
pub mod projection {
    pub use crate::world_kb::project_entity;
}
