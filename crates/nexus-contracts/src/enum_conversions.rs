//! Enum conversion utilities for generated contract types.
//!
//! Provides `Display`, `as_str()`, and `FromStr` implementations for enum types
//! generated from JSON Schema. This file extends generated types without modifying them.

use crate::generated::common::common_types::{
    AccountStatus, AgentProfileStatus, BindingStatus, BlockType, CommandOrigin, CommandStatus,
    CommandType, CreatorStatus, DeliveryState, DeltaOperation, DeltaType, ForkBranchStatus,
    KeyBlockStatus, ManifestType, ManuscriptStorage, MembershipRole, MembershipStatus, MemoryKind,
    MemoryStatus, PairingSource, PairingStatus, ProfileKind, PublishStoryOutcome,
    ReferenceSourceType, RegistrationSource, ScanStatus, SelectionMode, StoryManifestStatus,
    SubscriptionTier, TimelineEventStatus, TimelineEventType, VerificationStatus, WorldStatus,
};
use crate::generated::local_api::works::chapters::chapter_status::ChapterStatus;
use crate::local::domain::runtime_mode::RuntimeMode;
use std::fmt;
use std::str::FromStr;

// ── Display implementations for tracing/logging ──────────────────────────

impl fmt::Display for CreatorStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl fmt::Display for WorldStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl fmt::Display for MembershipStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl fmt::Display for CommandType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl fmt::Display for AccountStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl fmt::Display for SubscriptionTier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl fmt::Display for RuntimeMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ── as_str() implementations ─────────────────────────────────────────────

impl AccountStatus {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Suspended => "suspended",
            Self::Deleted => "deleted",
        }
    }
}

impl SubscriptionTier {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Free => "free",
            Self::Pro => "pro",
            Self::Studio => "studio",
            Self::Enterprise => "enterprise",
        }
    }
}

// V1.54 P1: BlockType as_str — supports novel, game-bible, and script (V1.55 P3) variants.
impl BlockType {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Character => "character",
            Self::Ability => "ability",
            Self::Scene => "scene",
            Self::Organization => "organization",
            Self::Item => "item",
            Self::Conflict => "conflict",
            Self::InfoPoint => "info_point",
            Self::Event => "event",
            Self::Species => "species",
            Self::Faction => "faction",
            Self::MagicSystem => "magic_system",
            Self::Technology => "technology",
            Self::Deity => "deity",
            Self::Level => "level",
            Self::EconomyTier => "economy_tier",
            // V1.55 P3: script taxonomy
            Self::Dialogue => "dialogue",
            Self::Beat => "beat",
            Self::Act => "act",
        }
    }
}

impl fmt::Display for BlockType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl CreatorStatus {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Archived => "archived",
            Self::Locked => "locked",
        }
    }
}

impl RegistrationSource {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Cli => "cli",
            Self::WebAgent => "web_agent",
            Self::Platform => "platform",
        }
    }
}

impl WorldStatus {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Archived => "archived",
        }
    }
}

impl MembershipStatus {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Invited => "invited",
            Self::Suspended => "suspended",
            Self::Removed => "removed",
        }
    }
}

impl MembershipRole {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Owner => "owner",
            Self::Maintainer => "maintainer",
            Self::Collaborator => "collaborator",
            Self::OfficialCreator => "official_creator",
        }
    }
}

impl PairingSource {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::AutoCli => "auto_cli",
            Self::ManualWeb => "manual_web",
            Self::PlatformAuto => "platform_auto",
        }
    }
}

impl PairingStatus {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Revoked => "revoked",
        }
    }
}

impl KeyBlockStatus {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Provisional => "provisional",
            Self::Confirmed => "confirmed",
            Self::Deprecated => "deprecated",
            Self::Merged => "merged",
            Self::Deleted => "deleted",
        }
    }
}

impl TimelineEventType {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::StoryAdvance => "story_advance",
            Self::StateUpdate => "state_update",
            Self::ForkMarker => "fork_marker",
            Self::OfficialProgression => "official_progression",
            Self::PublishMarker => "publish_marker",
        }
    }
}

impl TimelineEventStatus {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Canon => "canon",
            Self::Provisional => "provisional",
            Self::Rejected => "rejected",
        }
    }
}

impl ForkBranchStatus {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Archived => "archived",
        }
    }
}

impl VerificationStatus {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Unverified => "unverified",
            Self::Requested => "requested",
            Self::Verified => "verified",
            Self::Rejected => "rejected",
        }
    }
}

impl MemoryKind {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::StorySummary => "story_summary",
            Self::ResearchMaterial => "research_material",
            Self::ReviewNote => "review_note",
            Self::CharacterNote => "character_note",
            Self::WorldBuilding => "world_building",
            Self::PlotOutline => "plot_outline",
            Self::ThemeAnalysis => "theme_analysis",
            Self::PersonalityCore => "personality_core",
            Self::Custom => "custom",
        }
    }
}

impl MemoryStatus {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Superseded => "superseded",
            Self::Archived => "archived",
        }
    }
}

impl ManifestType {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Chapter => "chapter",
            Self::Arc => "arc",
            Self::Story => "story",
            Self::Excerpt => "excerpt",
        }
    }
}

impl StoryManifestStatus {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::SummaryReady => "summary_ready",
            Self::StagedForPublish => "staged_for_publish",
            Self::Published => "published",
            Self::Archived => "archived",
        }
    }
}

impl PublishStoryOutcome {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Submitted => "submitted",
            Self::Published => "published",
            Self::Rejected => "rejected",
            Self::InvalidState => "invalid_state",
        }
    }
}

impl ManuscriptStorage {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::None => "none",
            Self::LocalWorkspace => "local_workspace",
            Self::PlatformSandbox => "platform_sandbox",
        }
    }
}

impl ReferenceSourceType {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Pdf => "pdf",
            Self::Url => "url",
            Self::Note => "note",
        }
    }
}

impl ScanStatus {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Scanned => "scanned",
            Self::Failed => "failed",
            Self::Ignored => "ignored",
        }
    }
}

impl CommandType {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::AdvanceWorld => "advance_world",
            Self::InjectFutureEvent => "inject_future_event",
            Self::ExtractKb => "extract_kb",
            Self::SyncPush => "sync_push",
            Self::SyncPull => "sync_pull",
            Self::ForkWorld => "fork_world",
            Self::PublishStory => "publish_story",
        }
    }
}

impl CommandOrigin {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::LocalUser => "local_user",
            Self::LocalAgent => "local_agent",
            Self::OfficialCreator => "official_creator",
            Self::System => "system",
        }
    }
}

impl CommandStatus {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
        }
    }
}

impl DeltaType {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::World => "world",
            Self::KeyBlock => "key_block",
            Self::TimelineEvent => "timeline_event",
            Self::ForkBranch => "fork_branch",
            Self::MemoryItem => "memory_item",
            Self::StoryManifest => "story_manifest",
        }
    }
}

impl DeltaOperation {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Update => "update",
            Self::Upsert => "upsert",
            Self::Delete => "delete",
            Self::Append => "append",
        }
    }
}

impl DeliveryState {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Staged => "staged",
            Self::Ready => "ready",
            Self::Sent => "sent",
            Self::Acked => "acked",
            Self::Conflicted => "conflicted",
            Self::Failed => "failed",
        }
    }
}

impl BindingStatus {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Unlinked => "unlinked",
            Self::Stale => "stale",
        }
    }
}

impl ProfileKind {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::LocalAgent => "local_agent",
            Self::PlatformHosted => "platform_hosted",
        }
    }
}

impl SelectionMode {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Registry => "registry",
            Self::ManualCommand => "manual_command",
            Self::ManualRemote => "manual_remote",
        }
    }
}

impl AgentProfileStatus {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Unavailable => "unavailable",
            Self::Deprecated => "deprecated",
        }
    }
}

impl RuntimeMode {
    /// String representation matching JSON Schema enum values.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::LocalOnly => "local_only",
            Self::LocalFirst => "local_first",
            Self::CloudEnhanced => "cloud_enhanced",
        }
    }
}

impl ChapterStatus {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::NotStarted => "not_started",
            Self::Outlined => "outlined",
            Self::Draft => "draft",
            Self::Finalized => "finalized",
            Self::Published => "published",
        }
    }
}

// The generated `ChapterStatus` enum does not derive `Default` (codegen does not
// annotate enums with `#[default]`), so we provide the impl here rather than
// hand-editing generated source.
#[allow(clippy::derivable_impls)]
impl Default for ChapterStatus {
    fn default() -> Self {
        Self::NotStarted
    }
}

impl fmt::Display for ChapterStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ── FromStr implementations ───────────────────────────────────────────────

impl FromStr for CreatorStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "active" => Ok(Self::Active),
            "archived" => Ok(Self::Archived),
            "locked" => Ok(Self::Locked),
            _ => Err(format!("Invalid CreatorStatus: {s}")),
        }
    }
}

impl FromStr for AccountStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "active" => Ok(Self::Active),
            "suspended" => Ok(Self::Suspended),
            "deleted" => Ok(Self::Deleted),
            _ => Err(format!("Invalid AccountStatus: {s}")),
        }
    }
}

impl FromStr for SubscriptionTier {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "free" => Ok(Self::Free),
            "pro" => Ok(Self::Pro),
            "studio" => Ok(Self::Studio),
            "enterprise" => Ok(Self::Enterprise),
            _ => Err(format!("Invalid SubscriptionTier: {s}")),
        }
    }
}

impl FromStr for RegistrationSource {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "cli" => Ok(Self::Cli),
            "web_agent" => Ok(Self::WebAgent),
            "platform" => Ok(Self::Platform),
            _ => Err(format!("Invalid RegistrationSource: {s}")),
        }
    }
}

impl FromStr for WorldStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "active" => Ok(Self::Active),
            "paused" => Ok(Self::Paused),
            "archived" => Ok(Self::Archived),
            _ => Err(format!("Invalid WorldStatus: {s}")),
        }
    }
}

impl FromStr for MembershipStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "active" => Ok(Self::Active),
            "invited" => Ok(Self::Invited),
            "suspended" => Ok(Self::Suspended),
            "removed" => Ok(Self::Removed),
            _ => Err(format!("Invalid MembershipStatus: {s}")),
        }
    }
}

impl FromStr for MembershipRole {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "owner" => Ok(Self::Owner),
            "maintainer" => Ok(Self::Maintainer),
            "collaborator" => Ok(Self::Collaborator),
            "official_creator" => Ok(Self::OfficialCreator),
            _ => Err(format!("Invalid MembershipRole: {s}")),
        }
    }
}

impl FromStr for PairingSource {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "auto_cli" => Ok(Self::AutoCli),
            "manual_web" => Ok(Self::ManualWeb),
            "platform_auto" => Ok(Self::PlatformAuto),
            _ => Err(format!("Invalid PairingSource: {s}")),
        }
    }
}

impl FromStr for PairingStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "active" => Ok(Self::Active),
            "revoked" => Ok(Self::Revoked),
            _ => Err(format!("Invalid PairingStatus: {s}")),
        }
    }
}

impl FromStr for KeyBlockStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "provisional" => Ok(Self::Provisional),
            "confirmed" => Ok(Self::Confirmed),
            "deprecated" => Ok(Self::Deprecated),
            "merged" => Ok(Self::Merged),
            "deleted" => Ok(Self::Deleted),
            _ => Err(format!("Invalid KeyBlockStatus: {s}")),
        }
    }
}

impl FromStr for TimelineEventType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "story_advance" => Ok(Self::StoryAdvance),
            "state_update" => Ok(Self::StateUpdate),
            "fork_marker" => Ok(Self::ForkMarker),
            "official_progression" => Ok(Self::OfficialProgression),
            "publish_marker" => Ok(Self::PublishMarker),
            _ => Err(format!("Invalid TimelineEventType: {s}")),
        }
    }
}

impl FromStr for TimelineEventStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "canon" => Ok(Self::Canon),
            "provisional" => Ok(Self::Provisional),
            "rejected" => Ok(Self::Rejected),
            _ => Err(format!("Invalid TimelineEventStatus: {s}")),
        }
    }
}

impl FromStr for ForkBranchStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "active" => Ok(Self::Active),
            "archived" => Ok(Self::Archived),
            _ => Err(format!("Invalid ForkBranchStatus: {s}")),
        }
    }
}

impl FromStr for VerificationStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "unverified" => Ok(Self::Unverified),
            "requested" => Ok(Self::Requested),
            "verified" => Ok(Self::Verified),
            "rejected" => Ok(Self::Rejected),
            _ => Err(format!("Invalid VerificationStatus: {s}")),
        }
    }
}

impl FromStr for MemoryKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "story_summary" => Ok(Self::StorySummary),
            "research_material" => Ok(Self::ResearchMaterial),
            "review_note" => Ok(Self::ReviewNote),
            "character_note" => Ok(Self::CharacterNote),
            "world_building" => Ok(Self::WorldBuilding),
            "plot_outline" => Ok(Self::PlotOutline),
            "theme_analysis" => Ok(Self::ThemeAnalysis),
            "personality_core" => Ok(Self::PersonalityCore),
            "custom" => Ok(Self::Custom),
            _ => Err(format!("Invalid MemoryKind: {s}")),
        }
    }
}

impl FromStr for MemoryStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "active" => Ok(Self::Active),
            "superseded" => Ok(Self::Superseded),
            "archived" => Ok(Self::Archived),
            _ => Err(format!("Invalid MemoryStatus: {s}")),
        }
    }
}

impl FromStr for ManifestType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "chapter" => Ok(Self::Chapter),
            "arc" => Ok(Self::Arc),
            "story" => Ok(Self::Story),
            "excerpt" => Ok(Self::Excerpt),
            _ => Err(format!("Invalid ManifestType: {s}")),
        }
    }
}

impl FromStr for StoryManifestStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "summary_ready" => Ok(Self::SummaryReady),
            "staged_for_publish" => Ok(Self::StagedForPublish),
            "published" => Ok(Self::Published),
            "archived" => Ok(Self::Archived),
            _ => Err(format!("Invalid StoryManifestStatus: {s}")),
        }
    }
}

impl FromStr for PublishStoryOutcome {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "submitted" => Ok(Self::Submitted),
            "published" => Ok(Self::Published),
            "rejected" => Ok(Self::Rejected),
            "invalid_state" => Ok(Self::InvalidState),
            _ => Err(format!("Invalid PublishStoryOutcome: {s}")),
        }
    }
}

impl FromStr for ManuscriptStorage {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "none" => Ok(Self::None),
            "local_workspace" => Ok(Self::LocalWorkspace),
            "platform_sandbox" => Ok(Self::PlatformSandbox),
            _ => Err(format!("Invalid ManuscriptStorage: {s}")),
        }
    }
}

impl FromStr for ReferenceSourceType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "file" => Ok(Self::File),
            "pdf" => Ok(Self::Pdf),
            "url" => Ok(Self::Url),
            "note" => Ok(Self::Note),
            _ => Err(format!("Invalid ReferenceSourceType: {s}")),
        }
    }
}

impl FromStr for ScanStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "scanned" => Ok(Self::Scanned),
            "failed" => Ok(Self::Failed),
            "ignored" => Ok(Self::Ignored),
            _ => Err(format!("Invalid ScanStatus: {s}")),
        }
    }
}

impl FromStr for CommandType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "advance_world" => Ok(Self::AdvanceWorld),
            "inject_future_event" => Ok(Self::InjectFutureEvent),
            "extract_kb" => Ok(Self::ExtractKb),
            "sync_push" => Ok(Self::SyncPush),
            "sync_pull" => Ok(Self::SyncPull),
            "fork_world" => Ok(Self::ForkWorld),
            "publish_story" => Ok(Self::PublishStory),
            _ => Err(format!("Invalid CommandType: {s}")),
        }
    }
}

impl FromStr for CommandOrigin {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "local_user" => Ok(Self::LocalUser),
            "local_agent" => Ok(Self::LocalAgent),
            "official_creator" => Ok(Self::OfficialCreator),
            "system" => Ok(Self::System),
            _ => Err(format!("Invalid CommandOrigin: {s}")),
        }
    }
}

impl FromStr for CommandStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "running" => Ok(Self::Running),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            _ => Err(format!("Invalid CommandStatus: {s}")),
        }
    }
}

impl FromStr for DeltaType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "world" => Ok(Self::World),
            "key_block" => Ok(Self::KeyBlock),
            "timeline_event" => Ok(Self::TimelineEvent),
            "fork_branch" => Ok(Self::ForkBranch),
            "memory_item" => Ok(Self::MemoryItem),
            "story_manifest" => Ok(Self::StoryManifest),
            _ => Err(format!("Invalid DeltaType: {s}")),
        }
    }
}

impl FromStr for DeltaOperation {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "create" => Ok(Self::Create),
            "update" => Ok(Self::Update),
            "upsert" => Ok(Self::Upsert),
            "delete" => Ok(Self::Delete),
            "append" => Ok(Self::Append),
            _ => Err(format!("Invalid DeltaOperation: {s}")),
        }
    }
}

impl FromStr for DeliveryState {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "staged" => Ok(Self::Staged),
            "ready" => Ok(Self::Ready),
            "sent" => Ok(Self::Sent),
            "acked" => Ok(Self::Acked),
            "conflicted" => Ok(Self::Conflicted),
            "failed" => Ok(Self::Failed),
            _ => Err(format!("Invalid DeliveryState: {s}")),
        }
    }
}

impl FromStr for BindingStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "active" => Ok(Self::Active),
            "unlinked" => Ok(Self::Unlinked),
            "stale" => Ok(Self::Stale),
            _ => Err(format!("Invalid BindingStatus: {s}")),
        }
    }
}

impl FromStr for ProfileKind {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "local_agent" => Ok(Self::LocalAgent),
            "platform_hosted" => Ok(Self::PlatformHosted),
            _ => Err(format!("Invalid ProfileKind: {s}")),
        }
    }
}

impl FromStr for SelectionMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "registry" => Ok(Self::Registry),
            "manual_command" => Ok(Self::ManualCommand),
            "manual_remote" => Ok(Self::ManualRemote),
            _ => Err(format!("Invalid SelectionMode: {s}")),
        }
    }
}

impl FromStr for AgentProfileStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "active" => Ok(Self::Active),
            "unavailable" => Ok(Self::Unavailable),
            "deprecated" => Ok(Self::Deprecated),
            _ => Err(format!("Invalid AgentProfileStatus: {s}")),
        }
    }
}

impl FromStr for RuntimeMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "local_only" => Ok(Self::LocalOnly),
            "local_first" => Ok(Self::LocalFirst),
            "cloud_enhanced" => Ok(Self::CloudEnhanced),
            _ => Err(format!(
                "unknown runtime mode: '{s}'; expected local_only, local_first, or cloud_enhanced"
            )),
        }
    }
}

impl FromStr for ChapterStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "not_started" => Ok(Self::NotStarted),
            "outlined" => Ok(Self::Outlined),
            "draft" => Ok(Self::Draft),
            "finalized" => Ok(Self::Finalized),
            "published" => Ok(Self::Published),
            _ => Err(format!(
                "unknown chapter status: '{s}'; expected not_started, outlined, draft, finalized, or published"
            )),
        }
    }
}
