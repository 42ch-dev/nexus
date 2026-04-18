//! Built-in capability implementations.
//!
//! One module per built-in. Includes both non-ACP capabilities (sync,
//! workspace, etc.) and ACP-touching capabilities (acp.prompt, acp.session_load,
//! judge.llm) added in WS3.

mod acp_prompt;
mod acp_session_load;
mod context_summarize;
mod creator;
mod judge_llm;
mod judge_rule;
mod outbox;
pub mod registry;
mod sync;
mod workspace;

pub use acp_prompt::AcpPrompt;
pub use acp_session_load::AcpSessionLoad;
pub use context_summarize::ContextSummarize;
pub use creator::{CreatorInjectPrompt, CreatorReadMemory, CreatorWriteMemory};
pub use judge_llm::JudgeLlm;
pub use judge_rule::JudgeRule;
pub use outbox::{OutboxCompact, OutboxFlush};
pub use registry::RegistryRefresh;
pub use sync::{SyncPull, SyncPush};
pub use workspace::{WorkspaceCommit, WorkspaceOpen};
