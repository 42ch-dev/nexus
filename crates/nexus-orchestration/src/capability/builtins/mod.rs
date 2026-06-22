//! Built-in capability implementations.
//!
//! One module per built-in. Includes both non-ACP capabilities (sync,
//! workspace, etc.) and ACP-touching capabilities (acp.prompt, `acp.session_load`,
//! judge.llm) added in WS3.

mod acp_prompt;
mod acp_session_load;
mod context_summarize;
mod creator;
mod essay_scaffold;
mod game_bible_scaffold;
mod game_bible_section_status;
mod judge_llm;
mod judge_rule;
mod kb_extract_work;
mod llm_extract;
mod novel_chapter_transition;
mod novel_scaffold;
mod novel_scaffold_sanitize;
mod outbox;
mod reference_refresh;
pub mod registry;
mod script_scaffold;
mod soul_experience_aggregate;
mod sync;
mod workspace;
mod world_refs_validate;

pub use acp_prompt::AcpPrompt;
pub use acp_session_load::AcpSessionLoad;
pub use context_summarize::ContextSummarize;
pub use creator::{
    CreatorCapabilityStore, CreatorInjectPrompt, CreatorReadMemory, CreatorWriteBrief,
    CreatorWriteMemory,
};
pub use essay_scaffold::EssayProjectScaffold;
pub use game_bible_scaffold::GameBibleProjectScaffold;
pub use game_bible_section_status::GameBibleSectionStatusUpdate;
pub use judge_llm::JudgeLlm;
pub use judge_rule::JudgeRule;
pub use kb_extract_work::KbExtractWork;
pub use llm_extract::LlmExtract;
pub use novel_chapter_transition::NovelChapterTransition;
pub use novel_scaffold::NovelProjectScaffold;
pub use outbox::{OutboxCompact, OutboxFlush};
pub use reference_refresh::ReferenceRefresh;
pub use registry::{
    embedded_snapshot_capabilities, embedded_snapshot_version, validate_cdn_url_static, CdnConfig,
    CdnError, RegistryRefresh,
};
pub use script_scaffold::ScriptProjectScaffold;
pub use soul_experience_aggregate::SoulExperienceAggregate;
pub use sync::{SyncPull, SyncPush};
pub use workspace::{WorkspaceCommit, WorkspaceOpen};
// WAIVER: pre-1.0 local-first; see V1.41 P-last residual R-V140P3-S5
// — world_refs_validate is exposed as a library function but not registered
// as a Capability in the builtins registry; it is called directly by daemon
// handlers. Acceptable until capability registration is unified.
pub use world_refs_validate::{
    validate_world_refs, ValidationStage, WorldRefFinding, WorldRefSeverity,
    WorldRefsValidationParams, WorldRefsValidationResult,
};
