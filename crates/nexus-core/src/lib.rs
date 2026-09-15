//! Transport-neutral World KB core service (v1.189 P1-T2).

mod changes;
mod error;
mod principal;
mod provider_journal;
mod service;
mod worlds;
mod world_kb;
mod timeline;
mod forks;
mod world_pack;
mod world_rules;

pub use error::{CoreError, CoreResult};
pub use principal::Principal;
pub use service::{CoreAccess, CoreOpenOptions, CoreService};
pub use worlds::DELETE_WORLD_BLOCKED_BY_BINDINGS;
pub use timeline::{CoreTimelineEventsQuery, CoreTimelineOverviewQuery};

