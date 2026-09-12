//! Transport-neutral World KB core service (v1.189 P1-T2).

pub mod changes;
pub mod error;
pub mod principal;
pub mod service;
pub mod world_kb;

pub use error::{CoreError, CoreResult};
pub use principal::Principal;
pub use service::{CoreAccess, CoreOpenOptions, CoreService};

pub use world_kb::project_entity;
