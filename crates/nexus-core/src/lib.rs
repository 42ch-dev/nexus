//! Transport-neutral World KB core service (v1.189 P1-T2).

mod changes;
mod chronology;
mod content;
mod error;
mod findings;
mod knowledge;
mod outline;
mod presets;
mod principal;
mod provider_journal;
mod reading;
mod references;
mod service;
mod worlds;
mod world_kb;
mod timeline;
mod forks;
mod world_pack;
mod world_rules;
mod works;

pub use error::{CoreError, CoreResult};
pub use presets::PresetError;
pub use principal::Principal;
pub use service::{CoreAccess, CoreOpenOptions, CoreService};
pub use worlds::DELETE_WORLD_BLOCKED_BY_BINDINGS;
pub use timeline::{CoreTimelineEventsQuery, CoreTimelineOverviewQuery};
pub use works::{
    WorkDetails, WorkPatchRequest, WorkPoolEntry, WorkInspirationItem,
    SetPoolActiveRequest, ReconcileDryRunQuery, ListPoolQuery, ListPoolResponse,
    PromotePoolRequest, ArchivePoolRequest, AddInspirationRequest, AddInspirationResponse,
    ListInspirationQuery, ListInspirationResponse, PromoteInspirationRequest,
    PromoteInspirationResponse, ArchiveInspirationRequest,
    WorkReconcileReport,
};
pub use chronology::CoreWorkChronology;
pub use content::{CoreChapterContentQuery, resolve_guarded_path, resolve_guarded_path_async};
pub use findings::{
    format_routing_hint, CreateFindingRequest, ListFindingsQuery, ListFindingsResponse,
    PruneFindingsOutcome, StaleFindingEntry, StaleFindingsResponse, UpdateFindingRequest,
};
pub use references::{GetReferenceResponse, ListReferencesResponse, ReferenceInfo};

