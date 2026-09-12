//! Hardcoded `_system.maintenance` graph builder.
//!
//! Returns a [`graph_flow::Graph`] with a linear chain:
//! `sync_pull` → `outbox_flush` → `registry_refresh` → `End`
//!
//! WS3 will replace this with a preset file loader.
//!
//! Design: `.mstar/specs/orchestration-engine.md` §9.1.

use crate::capability::CapabilityRegistry;
use async_trait::async_trait;
use graph_flow::{Graph, NextAction, Task, TaskResult};
use serde_json::Value;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// EndTask — terminal node returning NextAction::End
// ---------------------------------------------------------------------------

/// A terminal task that marks a graph as complete.
///
/// Shared by the system graph and by preset inner graphs (see
/// [`crate::preset::loader`]): graph-flow only reports `Completed` when a task
/// returns `NextAction::End`, so every graph needs an explicit terminal node.
pub(crate) struct EndTask {
    /// Response text recorded for the terminal step.
    message: String,
}

impl EndTask {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[async_trait]
impl Task for EndTask {
    fn id(&self) -> &'static str {
        "end"
    }

    async fn run(
        &self,
        _context: graph_flow::Context,
    ) -> Result<TaskResult, graph_flow::GraphError> {
        Ok(TaskResult::new(Some(self.message.clone()), NextAction::End))
    }
}

// ---------------------------------------------------------------------------
// CapabilityTask — one-shot task wrapping a named capability
// ---------------------------------------------------------------------------

/// A graph-flow Task that invokes a capability by name.
///
/// Unlike the generic `CapabilityTask` in `tasks/mod.rs` (which reads the
/// capability name from Context), this variant has the name baked in at
/// construction time. This is simpler for hardcoded preset graphs.
struct PresetCapabilityTask {
    name: &'static str,
    registry: Arc<CapabilityRegistry>,
    task_id: &'static str,
}

impl PresetCapabilityTask {
    fn new(
        name: &'static str,
        task_id: &'static str,
        registry: Arc<CapabilityRegistry>,
    ) -> Arc<Self> {
        Arc::new(Self {
            name,
            registry,
            task_id,
        })
    }
}

#[async_trait]
impl Task for PresetCapabilityTask {
    fn id(&self) -> &str {
        self.task_id
    }

    async fn run(
        &self,
        context: graph_flow::Context,
    ) -> Result<TaskResult, graph_flow::GraphError> {
        let cap = self.registry.get(self.name).ok_or_else(|| {
            graph_flow::GraphError::TaskExecutionFailed(format!(
                "capability not found: {}",
                self.name
            ))
        })?;

        match cap.run(Value::Null).await {
            Ok(_output) => {
                // Store output in context for debugging.
                context.set(format!("_{}_output", self.name.replace('.', "_")), true)?;
                Ok(TaskResult::new(
                    Some(format!("{} completed", self.name)),
                    NextAction::Continue,
                ))
            }
            Err(e) => Ok(TaskResult::new_with_status(
                Some(format!("{} failed: {}", self.name, e)),
                NextAction::Continue,
                Some(format!("capability '{}' failed: {}", self.name, e)),
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Build the hardcoded `_system.maintenance` graph.
///
/// Accepts a shared [`CapabilityRegistry`] so that the daemon can pass its
/// own instance (avoids duplicate registries with diverging state).
///
/// Returns an `Arc<Graph>` with the linear chain:
/// ```text
/// sync_pull → outbox_flush → registry_refresh → End
/// ```
///
/// Each node is a [`PresetCapabilityTask`] wrapping the corresponding
/// built-in capability. The terminal `End` node returns `NextAction::End`.
/// graph-flow 0.8: assembled through the consuming `GraphBuilder`; the fixed
/// internal topology below is an invariant, so `build()` failure is a bug,
/// not user input — a checked `expect` documents that invariant.
///
/// # Panics
/// Panics when the fixed `_system.maintenance` topology fails to build —
/// an internal invariant violation, never user input.
#[must_use]
pub fn build(registry: Arc<CapabilityRegistry>) -> Arc<Graph> {
    let sync_pull = PresetCapabilityTask::new("sync.pull", "sync_pull", registry.clone());
    let outbox_flush = PresetCapabilityTask::new("outbox.flush", "outbox_flush", registry.clone());
    let registry_refresh =
        PresetCapabilityTask::new("registry.refresh", "registry_refresh", registry);
    let end: Arc<dyn Task> = Arc::new(EndTask::new("_system.maintenance completed"));

    let graph = graph_flow::GraphBuilder::new("_system.maintenance")
        .add_task(sync_pull)
        .add_task(outbox_flush)
        .add_task(registry_refresh)
        .add_task(end)
        // Linear edges.
        .add_edge("sync_pull", "outbox_flush")
        .add_edge("outbox_flush", "registry_refresh")
        .add_edge("registry_refresh", "end")
        // Start task is set automatically (first task added = sync_pull).
        .build()
        .expect("fixed _system.maintenance topology must build");

    Arc::new(graph)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_returns_graph_with_four_nodes() {
        let registry = Arc::new(CapabilityRegistry::with_builtins());
        let graph = build(registry);
        assert_eq!(graph.id, "_system.maintenance");
        // Verify the graph has nodes.
        assert!(graph.get_task("sync_pull").is_some());
        assert!(graph.get_task("outbox_flush").is_some());
        assert!(graph.get_task("registry_refresh").is_some());
        assert!(graph.get_task("end").is_some());
        assert!(graph.get_task("nonexistent").is_none());
    }

    #[test]
    fn start_task_is_sync_pull() {
        let registry = Arc::new(CapabilityRegistry::with_builtins());
        let graph = build(registry);
        assert_eq!(graph.start_task_id(), Some("sync_pull"));
    }

    #[test]
    fn graph_has_four_tasks() {
        let registry = Arc::new(CapabilityRegistry::with_builtins());
        let graph = build(registry);
        // Verify all expected task IDs exist.
        for id in &["sync_pull", "outbox_flush", "registry_refresh", "end"] {
            assert!(graph.get_task(id).is_some(), "expected task '{id}'");
        }
        // Verify a nonexistent task returns None.
        assert!(graph.get_task("nonexistent").is_none());
    }
}
