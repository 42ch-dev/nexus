//! Capability trait + registry.
//!
//! Design: `.mstar/knowledge/specs/orchestration-engine.md` §5.1–5.2.

pub mod builtins;

use async_trait::async_trait;
use serde_json::Value;
use std::sync::Arc;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors produced by capability execution.
#[derive(Error, Debug)]
pub enum CapabilityError {
    #[error("invalid input: {0}")]
    InputInvalid(String),
    #[error("transient external error: {0}")]
    TransientExternal(String),
    #[error("permanent external error: {0}")]
    PermanentExternal(String),
    #[error("worker unavailable")]
    WorkerUnavailable,
    #[error("ACP session lost")]
    AcpSessionLost,
    #[error("cancelled")]
    Cancelled,
    #[error("internal error: {0}")]
    Internal(String),
}

// ---------------------------------------------------------------------------
// WorkerHandleProvider — injected LLM call seam (J0)
// ---------------------------------------------------------------------------

/// Provider trait for capability-layer LLM calls via worker IPC.
///
/// Injected into `JudgeLlm`, `ContextSummarize`, and `AcpPrompt` through
/// the registry factory. When absent, capabilities operate in standalone/
/// test mode with clear error messages.
///
/// Design: architect note C0/J0, `KbExtractWork::with_pool` pattern.
#[async_trait]
pub trait WorkerHandleProvider: Send + Sync {
    /// Call `worker/acp_prompt` via IPC.
    ///
    /// Returns the full JSON-RPC response value on success.
    async fn call_acp_prompt(
        &self,
        creator_id: &str,
        session_id: &str,
        prompt: String,
        tool_policy: &str,
    ) -> Result<Value, CapabilityError>;
}

/// Runtime dependencies injected through `CapabilityRegistry::with_runtime_deps`.
///
/// Groups pool and worker provider so daemon boot can construct a single
/// struct and pass it to the registry factory.
pub struct CapabilityRuntimeDeps {
    /// Pool for pool-backed capabilities (`kb.extract_work`, etc.).
    pub pool: Option<sqlx::SqlitePool>,
    /// Worker handle provider for LLM-backed capabilities (`judge.llm`,
    /// `context.summarize`, `acp.prompt`).
    pub worker_provider: Option<std::sync::Arc<dyn WorkerHandleProvider>>,
}

// ---------------------------------------------------------------------------
// Capability trait
// ---------------------------------------------------------------------------

/// A capability that can be invoked as a graph-flow Task node.
///
/// Per the design spec, every capability ships its own input/output JSON Schema
/// as `&'static str` constants. These are **local** types, not wire contracts.
#[async_trait]
pub trait Capability: Send + Sync {
    /// Dot-separated capability name, e.g. `"sync.pull"`.
    fn name(&self) -> &'static str;

    /// JSON Schema (draft 2020-12) describing valid inputs.
    fn input_schema(&self) -> &'static str;

    /// JSON Schema (draft 2020-12) describing the output shape.
    fn output_schema(&self) -> &'static str;

    /// Execute the capability with the given input.
    ///
    /// Returns a JSON `Value` on success or a [`CapabilityError`].
    async fn run(&self, input: Value) -> Result<Value, CapabilityError>;
}

// ---------------------------------------------------------------------------
// CapabilityRegistry
// ---------------------------------------------------------------------------

/// Registry of available capabilities. Built once at daemon startup.
///
/// Capabilities are stored in a `Vec` for ordered iteration, with a `HashMap`
/// index for O(1) lookup by name (built lazily on first `get()` call).
pub struct CapabilityRegistry {
    capabilities: Vec<Box<dyn Capability>>,
    /// Lazy index: `name` → position in `capabilities`. Built on first lookup.
    index: Option<std::collections::HashMap<&'static str, usize>>,
}

impl CapabilityRegistry {
    /// Create a registry pre-populated with all built-in capabilities.
    ///
    /// Built-ins: `sync.pull`, `sync.push`, `outbox.flush`, `outbox.compact`,
    /// `workspace.open`, `workspace.commit`, `registry.refresh`,
    /// `creator.read_memory`, `creator.write_memory`, `creator.inject_prompt`,
    /// `judge.rule`, `acp.prompt`, `acp.session_load`, `judge.llm`,
    /// `context.summarize`, `kb.extract_work`, `soul.experience.aggregate`.
    ///
    /// `kb.extract_work` is created without a pool (placeholder mode).
    /// Use [`with_builtins_and_pool`] for full e2e support.
    #[must_use]
    pub fn with_builtins() -> Self {
        let caps: Vec<Box<dyn Capability>> = vec![
            Box::new(builtins::SyncPull),
            Box::new(builtins::SyncPush),
            Box::new(builtins::OutboxFlush),
            Box::new(builtins::OutboxCompact),
            Box::new(builtins::WorkspaceOpen),
            Box::new(builtins::WorkspaceCommit),
            Box::new(builtins::RegistryRefresh),
            Box::new(builtins::CreatorReadMemory::new()),
            Box::new(builtins::CreatorWriteMemory::new()),
            Box::new(builtins::CreatorInjectPrompt::new()),
            Box::new(builtins::JudgeRule),
            Box::new(builtins::AcpPrompt::new()),
            Box::new(builtins::AcpSessionLoad),
            Box::new(builtins::JudgeLlm::new()),
            Box::new(builtins::ContextSummarize::new()),
            Box::new(builtins::KbExtractWork::new()),
            Box::new(builtins::SoulExperienceAggregate),
        ];
        let mut reg = Self {
            capabilities: caps,
            index: None,
        };
        reg.build_index();
        reg
    }

    /// Create a registry with built-in capabilities and a pool.
    ///
    /// Same as [`with_builtins`] but `kb.extract_work` receives the pool
    /// for full e2e lifecycle, and all creator capabilities receive a
    /// [`builtins::CreatorCapabilityStore`] for real memory I/O
    /// and prompt injection persistence.
    #[must_use]
    pub fn with_builtins_and_pool(pool: sqlx::SqlitePool) -> Self {
        let creator_store = Arc::new(builtins::CreatorCapabilityStore::new(pool.clone()));
        let caps: Vec<Box<dyn Capability>> = vec![
            Box::new(builtins::SyncPull),
            Box::new(builtins::SyncPush),
            Box::new(builtins::OutboxFlush),
            Box::new(builtins::OutboxCompact),
            Box::new(builtins::WorkspaceOpen),
            Box::new(builtins::WorkspaceCommit),
            Box::new(builtins::RegistryRefresh),
            Box::new(builtins::CreatorReadMemory::with_store(
                creator_store.clone(),
            )),
            Box::new(builtins::CreatorWriteMemory::with_store(
                creator_store.clone(),
            )),
            Box::new(builtins::CreatorInjectPrompt::with_store(creator_store)),
            Box::new(builtins::JudgeRule),
            Box::new(builtins::AcpPrompt::new()),
            Box::new(builtins::AcpSessionLoad),
            Box::new(builtins::JudgeLlm::new()),
            Box::new(builtins::ContextSummarize::new()),
            Box::new(builtins::KbExtractWork::with_pool(pool)),
            Box::new(builtins::SoulExperienceAggregate),
        ];
        let mut reg = Self {
            capabilities: caps,
            index: None,
        };
        reg.build_index();
        reg
    }

    /// Create a registry with runtime dependencies injected.
    ///
    /// Production daemon boot should use this constructor when both a pool
    /// and worker provider are available. Capabilities without runtime deps
    /// are constructed in their default (standalone) form.
    #[must_use]
    pub fn with_runtime_deps(deps: &CapabilityRuntimeDeps) -> Self {
        let kb = deps
            .pool
            .as_ref()
            .map_or_else(builtins::KbExtractWork::new, |pool| {
                builtins::KbExtractWork::with_pool(pool.clone())
            });

        let judge_llm = deps
            .worker_provider
            .as_ref()
            .map_or_else(builtins::JudgeLlm::new, |provider| {
                builtins::JudgeLlm::with_worker_provider(provider.clone())
            });

        let context_summarize = deps
            .worker_provider
            .as_ref()
            .map_or_else(builtins::ContextSummarize::new, |provider| {
                builtins::ContextSummarize::with_worker_provider(provider.clone())
            });

        let acp_prompt = deps
            .worker_provider
            .as_ref()
            .map_or_else(builtins::AcpPrompt::new, |provider| {
                builtins::AcpPrompt::with_worker_provider(provider.clone())
            });

        let creator_store = deps.pool.as_ref().map(|pool| {
            std::sync::Arc::new(builtins::CreatorCapabilityStore::from_arc(
                std::sync::Arc::new(pool.clone()),
            ))
        });

        let creator_read = creator_store
            .as_ref()
            .map_or_else(builtins::CreatorReadMemory::new, |store| {
                builtins::CreatorReadMemory::with_store(store.clone())
            });
        let creator_write = creator_store
            .as_ref()
            .map_or_else(builtins::CreatorWriteMemory::new, |store| {
                builtins::CreatorWriteMemory::with_store(store.clone())
            });
        let creator_inject = creator_store
            .as_ref()
            .map_or_else(builtins::CreatorInjectPrompt::new, |store| {
                builtins::CreatorInjectPrompt::with_store(store.clone())
            });

        let caps: Vec<Box<dyn Capability>> = vec![
            Box::new(builtins::SyncPull),
            Box::new(builtins::SyncPush),
            Box::new(builtins::OutboxFlush),
            Box::new(builtins::OutboxCompact),
            Box::new(builtins::WorkspaceOpen),
            Box::new(builtins::WorkspaceCommit),
            Box::new(builtins::RegistryRefresh),
            Box::new(creator_read),
            Box::new(creator_write),
            Box::new(creator_inject),
            Box::new(builtins::JudgeRule),
            Box::new(acp_prompt),
            Box::new(builtins::AcpSessionLoad),
            Box::new(judge_llm),
            Box::new(context_summarize),
            Box::new(kb),
            Box::new(builtins::SoulExperienceAggregate),
        ];
        let mut reg = Self {
            capabilities: caps,
            index: None,
        };
        reg.build_index();
        reg
    }

    /// Create an empty registry (for testing).
    #[must_use]
    pub fn empty() -> Self {
        let mut reg = Self {
            capabilities: Vec::new(),
            index: None,
        };
        reg.build_index();
        reg
    }

    /// Build the name-to-index `HashMap` for O(1) lookups.
    ///
    /// Called once during construction. Must be called after `capabilities` is populated.
    fn build_index(&mut self) {
        let mut idx = std::collections::HashMap::with_capacity(self.capabilities.len());
        for (i, cap) in self.capabilities.iter().enumerate() {
            idx.insert(cap.name(), i);
        }
        self.index = Some(idx);
    }

    /// Look up a capability by its dot-separated name.
    ///
    /// Uses the pre-built `HashMap` index for O(1) amortized lookups.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&dyn Capability> {
        let idx = self.index.as_ref()?;
        let pos = idx.get(name)?;
        Some(self.capabilities[*pos].as_ref())
    }

    /// Iterate over all registered capabilities.
    pub fn iter(&self) -> impl Iterator<Item = &dyn Capability> {
        self.capabilities.iter().map(std::convert::AsRef::as_ref)
    }

    /// Return the number of registered capabilities.
    #[must_use]
    pub fn len(&self) -> usize {
        self.capabilities.len()
    }

    /// Return whether the registry is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.capabilities.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_sixteen_builtins() {
        let reg = CapabilityRegistry::with_builtins();
        assert_eq!(reg.len(), 17);
    }

    #[test]
    fn registry_lookup_each_builtin() {
        let reg = CapabilityRegistry::with_builtins();
        for name in [
            "sync.pull",
            "sync.push",
            "outbox.flush",
            "outbox.compact",
            "workspace.open",
            "workspace.commit",
            "registry.refresh",
            "creator.read_memory",
            "creator.write_memory",
            "creator.inject_prompt",
            "judge.rule",
            "acp.prompt",
            "acp.session_load",
            "judge.llm",
            "context.summarize",
            "kb.extract_work",
            "soul.experience.aggregate",
        ] {
            assert!(
                reg.get(name).is_some(),
                "expected builtin '{name}' to be registered"
            );
        }
    }

    #[test]
    fn registry_lookup_missing_returns_none() {
        let reg = CapabilityRegistry::with_builtins();
        assert!(reg.get("nonexistent").is_none());
    }

    #[tokio::test]
    async fn registry_iter_returns_all() {
        let reg = CapabilityRegistry::with_builtins();
        let names: Vec<&str> = reg.iter().map(super::Capability::name).collect();
        assert_eq!(names.len(), 17);
        assert!(names.contains(&"sync.pull"));
        assert!(names.contains(&"judge.rule"));
        assert!(names.contains(&"acp.prompt"));
        assert!(names.contains(&"judge.llm"));
        assert!(names.contains(&"context.summarize"));
        assert!(names.contains(&"kb.extract_work"));
        assert!(names.contains(&"soul.experience.aggregate"));
    }
}
