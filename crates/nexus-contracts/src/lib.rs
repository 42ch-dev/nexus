//! Nexus Wire Contracts (Generated from JSON Schema)
//!
//! This crate contains type definitions generated from `schemas/` JSON Schema files.
//! All wire types are auto-generated - do not modify manually.
//!
//! Hand-written local types live in `local/` — see `schemas-boundary.md` §3.

pub mod common_types;
pub mod enum_conversions;
pub mod generated;
pub mod local;

// Re-export all generated types at crate root (includes wire types only)
pub use generated::*;

// Re-export SourceAnchor at the crate root for drift-test discoverability
// (`use nexus_contracts::*`). The hand-maintained `common_types` module is
// NOT glob-re-exported here to avoid ambiguity with typify-generated
// inlined copies of the same enums in domain/daemon-api modules.
pub use common_types::SourceAnchor;

// ── Consumer-facing conveniences (plan v1.138 P1) ─────────────────────────
//
// typify inlines shared schema enums per-domain-module with prefixed names
// (e.g. `KeyBlockBlockType`, `PairingPairingSource`) and does NOT emit the
// bare friendly names consumers expect. Below we alias those generated enums
// to their friendly names so that a consumer's `nexus_contracts::BlockType`
// resolves to the SAME type the generated structs use as field types —
// avoiding cross-type conversion at every boundary. Aliases are only added
// where the friendly name is otherwise absent at the crate root (verified
// non-colliding with `generated::*`).
pub use generated::domain::creator::CreatorRegistrationSource as RegistrationSource;
pub use generated::domain::fork_branch::ForkBranchVerificationStatus as VerificationStatus;
pub use generated::domain::key_block::KeyBlockBlockType as BlockType;
pub use generated::domain::memory::MemoryMemoryKind as MemoryKind;
pub use generated::domain::memory::MemoryMemoryType as MemoryType;
pub use generated::domain::pairing::PairingPairingSource as PairingSource;
pub use generated::domain::story_manifest::StoryManifestManifestType as ManifestType;
pub use generated::domain::story_manifest::StoryManifestManuscriptStorage as ManuscriptStorage;
pub use generated::domain::timeline_event::TimelineEventEventType as TimelineEventType;
pub use generated::domain::user::UserAccountStatus as AccountStatus;
pub use generated::domain::user::UserSubscriptionTier as SubscriptionTier;
pub use generated::domain::world::WorldTimePolicy as TimePolicy;
pub use generated::domain::world::WorldVisibility as Visibility;
pub use generated::domain::world_membership::WorldMembershipMembershipStatus as MembershipStatus;
pub use generated::domain::world_membership::WorldMembershipRole as MembershipRole;

// Shared scalar aliases + common_types enums/structs that have NO typify
// counterpart at the crate root (typify either prefixes them or does not
// emit them because the schema is definitions-only). These are the
// canonical, hand-maintained copies from `common_types` (with `as_str()`
// via `enum_conversions` and `FromStr` where consumers need it).
pub use common_types::{
    BundleId, CommandId, CreatorId, DeliveryState, DeltaSequence, KeyBlockId, ManuscriptId,
    ManuscriptPhase, ReferenceSourceType, ScanStatus, SchemaVersion, SourceSummaryRef,
    StoryManifestId, TimelineEventId, Timestamp, UserId, WorkspaceId, WorldId,
};

// Sync / bundle / publish enums: typify mangles these per-schema, so alias the
// generated copies to the friendly names consumers (cloud-sync, orchestration)
// expect. `DeliveryState` has no typify counterpart and is re-exported from
// `common_types` above.
pub use generated::platform::sync::bundle::BundleBundleType as BundleType;
pub use generated::platform::sync::bundle::NexusDeltaDeltaType as DeltaType;
pub use generated::platform::sync::sync_command::SyncCommandCommandType as CommandType;
pub use generated::platform::sync::sync_command::SyncCommandOrigin as CommandOrigin;
pub use generated::platform::sync::sync_command::SyncCommandStatus as CommandStatus;

// `ComputeInput` cannot derive `Default` because its `schema_version` field is
// a `NonZeroU64` (typify emits no `Default` for it). The WASM host's compute
// entry point falls back to a default input when the caller passes empty or
// invalid bytes, so we provide a manual impl with `schema_version = 1` (the
// lowest valid schema version) and otherwise-empty fields. This is a
// behavior-preserving convenience; it does not relax any wire validation.
impl Default for generated::daemon_api::compute::compute_input::ComputeInput {
    fn default() -> Self {
        Self {
            schema_version: std::num::NonZeroU64::MIN,
            // `world_ref` is a generated struct with an all-optional shape; its
            // default is the obvious empty value. The fully-qualified path is
            // long, so rely on field-type inference here.
            #[allow(clippy::default_trait_access)]
            world_ref: Default::default(),
            key_blocks: Vec::new(),
            narrative_state: None,
            invocation: serde_json::Map::new(),
        }
    }
}

// `Display` for the string-newtype IDs that consumers embed directly in
// `format!` / `tracing::%` / comparison contexts. typify only emits `Deref` /
// `From<NewType> for String` for these, not `Display`; deref coercion covers
// `.to_string()` but NOT trait-bound positions like `format!("{}", id)`. Each
// impl delegates to the inner `String` via `Deref`, so the rendered text is
// exactly the wire value — no behavior change.
macro_rules! impl_display_via_deref {
    ($($t:path),* $(,)?) => {
        $(
            impl std::fmt::Display for $t {
                fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    std::fmt::Display::fmt(&**self, f)
                }
            }
        )*
    };
}
impl_display_via_deref! {
    generated::domain::key_block::KeyBlockKeyBlockId,
    generated::domain::key_block::KeyBlockWorldId,
    generated::domain::world::WorldWorldId,
    generated::domain::creator::CreatorCreatorId,
    generated::platform::sync::bundle::BundleBundleId,
    generated::platform::sync::bundle::BundleWorldId,
    generated::platform::sync::bundle::BundleCreatorId,
    generated::platform::sync::bundle::BundleSubmittingCreatorId,
    generated::platform::sync::bundle::BundleWorkspaceId,
    generated::platform::sync::bundle::BundleIdempotencyKey,
    generated::platform::sync::sync_command::SyncCommandCommandId,
    generated::platform::sync::sync_command::SyncCommandWorldId,
    generated::platform::sync::sync_command::SyncCommandCreatorId,
    generated::platform::sync::sync_command::SyncCommandWorkspaceId,
    generated::platform::sync::sync_command::SyncCommandRequestedBy,
    generated::platform::sync::sync_pull_request::SyncPullRequestWorldId,
    generated::platform::http_bff::publish_story_request::PublishStoryRequestWorldId,
    generated::platform::http_bff::world_snapshot_request::WorldSnapshotRequestWorldId,
}
