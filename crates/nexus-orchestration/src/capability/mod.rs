//! Capability trait + registry.
//!
//! Design: `.agents/plans/knowledge/orchestration-engine-v1.md` §5.1–5.2.

pub mod builtins;

use async_trait::async_trait;
use serde_json::Value;
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
pub struct CapabilityRegistry {
    capabilities: Vec<Box<dyn Capability>>,
}

impl CapabilityRegistry {
    /// Create a registry pre-populated with all built-in capabilities.
    ///
    /// Built-ins: `sync.pull`, `sync.push`, `outbox.flush`, `outbox.compact`,
    /// `workspace.open`, `workspace.commit`, `registry.refresh`,
    /// `creator.read_memory`, `creator.write_memory`, `creator.inject_prompt`,
    /// `judge.rule`.
    ///
    /// ACP-touching capabilities (`acp.prompt`, `acp.session_load`, `judge.llm`)
    /// are deferred to WS3.
    pub fn with_builtins() -> Self {
        let caps: Vec<Box<dyn Capability>> = vec![
            Box::new(builtins::SyncPull),
            Box::new(builtins::SyncPush),
            Box::new(builtins::OutboxFlush),
            Box::new(builtins::OutboxCompact),
            Box::new(builtins::WorkspaceOpen),
            Box::new(builtins::WorkspaceCommit),
            Box::new(builtins::RegistryRefresh),
            Box::new(builtins::CreatorReadMemory),
            Box::new(builtins::CreatorWriteMemory),
            Box::new(builtins::CreatorInjectPrompt),
            Box::new(builtins::JudgeRule),
        ];
        Self { capabilities: caps }
    }

    /// Create an empty registry (for testing).
    pub fn empty() -> Self {
        Self {
            capabilities: Vec::new(),
        }
    }

    /// Look up a capability by its dot-separated name.
    pub fn get(&self, name: &str) -> Option<&dyn Capability> {
        self.capabilities
            .iter()
            .find(|c| c.name() == name)
            .map(|c| c.as_ref())
    }

    /// Iterate over all registered capabilities.
    pub fn iter(&self) -> impl Iterator<Item = &dyn Capability> {
        self.capabilities.iter().map(|c| c.as_ref())
    }

    /// Return the number of registered capabilities.
    pub fn len(&self) -> usize {
        self.capabilities.len()
    }

    /// Return whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.capabilities.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_eleven_builtins() {
        let reg = CapabilityRegistry::with_builtins();
        assert_eq!(reg.len(), 11);
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
        assert!(reg.get("acp.prompt").is_none());
        assert!(reg.get("judge.llm").is_none());
        assert!(reg.get("nonexistent").is_none());
    }

    #[tokio::test]
    async fn registry_iter_returns_all() {
        let reg = CapabilityRegistry::with_builtins();
        let names: Vec<&str> = reg.iter().map(|c| c.name()).collect();
        assert_eq!(names.len(), 11);
        assert!(names.contains(&"sync.pull"));
        assert!(names.contains(&"judge.rule"));
    }
}
