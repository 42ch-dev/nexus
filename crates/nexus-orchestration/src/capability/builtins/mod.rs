//! Built-in capability implementations.
//!
//! One module per built-in. Includes both non-ACP capabilities (sync,
//! workspace, etc.) and ACP-touching capabilities (acp.prompt, `acp.session_load`,
//! judge.llm) added in WS3.

mod acp_prompt;
mod acp_session_load;
mod context_summarize;
mod creator;
mod judge_llm;
mod judge_rule;
mod kb_extract_work;
mod novel_chapter_transition;
mod novel_scaffold;
mod novel_scaffold_sanitize;
mod outbox;
pub mod registry;
mod soul_experience_aggregate;
mod sync;
mod workspace;

pub use acp_prompt::AcpPrompt;
pub use acp_session_load::AcpSessionLoad;
pub use context_summarize::ContextSummarize;
pub use creator::{
    CreatorCapabilityStore, CreatorInjectPrompt, CreatorReadMemory, CreatorWriteBrief,
    CreatorWriteMemory,
};
pub use judge_llm::JudgeLlm;
pub use judge_rule::JudgeRule;
pub use kb_extract_work::KbExtractWork;
pub use novel_chapter_transition::NovelChapterTransition;
pub use novel_scaffold::NovelProjectScaffold;
pub use outbox::{OutboxCompact, OutboxFlush};
pub use registry::RegistryRefresh;
pub use soul_experience_aggregate::SoulExperienceAggregate;
pub use sync::{SyncPull, SyncPush};
pub use workspace::{WorkspaceCommit, WorkspaceOpen};
