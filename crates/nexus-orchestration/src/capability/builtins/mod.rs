//! Built-in capability implementations (non-ACP subset).
//!
//! One module per built-in. ACP-touching capabilities (`acp.prompt`,
//! `acp.session_load`, `judge.llm`) are deferred to WS3.

mod creator;
mod judge_rule;
mod outbox;
mod registry;
mod sync;
mod workspace;

pub use creator::{CreatorInjectPrompt, CreatorReadMemory, CreatorWriteMemory};
pub use judge_rule::JudgeRule;
pub use outbox::{OutboxCompact, OutboxFlush};
pub use registry::RegistryRefresh;
pub use sync::{SyncPull, SyncPush};
pub use workspace::{WorkspaceCommit, WorkspaceOpen};
