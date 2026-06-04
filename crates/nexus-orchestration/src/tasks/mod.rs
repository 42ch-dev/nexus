//! Standard `Task` implementations for the orchestration engine.
//!
//! Design: `.mstar/knowledge/specs/orchestration-engine.md` §4.4.
//!
//! # TODO(V1.17): Run and capability-call trace correlation
//!
//! When the daemon orchestration API is implemented:
//! - Engine/session start paths should read `_run_id` from `graph_flow::Context`
//!   and propagate it to all child tasks.
//! - `CapabilityTask::run` should generate a `capability_call_id` per invocation
//!   and store `_last_capability_call_id` + capability call metadata in context.
//! - `AcpPromptTask::run` should include `run_id` and `capability_call_id` in
//!   `worker/acp_prompt` params when a worker handle exists.
//! - If `_trace_file` is present in context, append start/finish trace events
//!   best-effort using the DTOs from `nexus-contracts::local::acp_runtime::trace`.

use crate::capability::{CapabilityError, CapabilityRegistry};
use crate::engine::OrchestrationEngine;
use crate::preset::manifest::{EnterAction, ExitWhen, StateDefinition};
use async_trait::async_trait;
use graph_flow::{Graph, NextAction, Task, TaskResult};
use serde_json::Value;
use std::sync::Arc;
use thiserror::Error;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors from Task execution.
#[derive(Error, Debug)]
pub enum TaskExecError {
    #[error("capability not found: {0}")]
    CapabilityNotFound(String),
    #[error("capability execution failed: {0}")]
    CapabilityFailed(#[from] CapabilityError),
    #[error("feature not wired: {feature} (since {since})")]
    WsUnwired { feature: String, since: String },
    #[error("invalid input for task: {0}")]
    InvalidInput(String),
}

// ---------------------------------------------------------------------------
// CapabilityTask
// ---------------------------------------------------------------------------

/// Resolves a capability by name, runs it, and stores the result.
///
/// Input (via Context):
/// - `_capability_name` (String): dot-separated capability name
/// - `_capability_input` (Value): input JSON for the capability
pub struct CapabilityTask {
    pub registry: std::sync::Arc<CapabilityRegistry>,
}

#[async_trait]
impl Task for CapabilityTask {
    fn id(&self) -> &'static str {
        "capability_task"
    }

    async fn run(
        &self,
        context: graph_flow::Context,
    ) -> Result<TaskResult, graph_flow::GraphError> {
        let name: String = context.get("_capability_name").await.unwrap_or_default();
        let input: Value = context
            .get("_capability_input")
            .await
            .unwrap_or(Value::Null);

        let cap = self.registry.get(&name).ok_or_else(|| {
            graph_flow::GraphError::TaskExecutionFailed(format!("capability not found: {name}"))
        })?;

        match cap.run(input).await {
            Ok(output) => {
                context.set("_capability_output", output).await;
                Ok(TaskResult::new(
                    Some("capability executed".to_string()),
                    NextAction::Continue,
                ))
            }
            Err(e) => {
                context.set("_capability_error", format!("{e}")).await;
                Ok(TaskResult::new_with_status(
                    Some(format!("capability error: {e}")),
                    NextAction::Continue,
                    Some(format!("capability '{name}' failed: {e}")),
                ))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// RuleCheckTask
// ---------------------------------------------------------------------------

/// Pure function evaluation over Context.
///
/// Reads `_rule` from context, evaluates a simple condition, and returns
/// `NextAction::Continue` if true or `NextAction::WaitForInput` if false.
pub struct RuleCheckTask;

#[async_trait]
impl Task for RuleCheckTask {
    fn id(&self) -> &'static str {
        "rule_check_task"
    }

    async fn run(
        &self,
        context: graph_flow::Context,
    ) -> Result<TaskResult, graph_flow::GraphError> {
        let rule: String = context.get("_rule").await.unwrap_or_default();

        let (passes, reason) = match rule.as_str() {
            "always_true" => (true, "rule: always_true → pass".to_string()),
            "always_false" => (false, "rule: always_false → fail".to_string()),
            other => (false, format!("unsupported rule: '{other}'")),
        };

        context.set("_rule_result", passes).await;
        context.set("_rule_reason", reason).await;

        let next_action = if passes {
            NextAction::Continue
        } else {
            NextAction::WaitForInput
        };

        Ok(TaskResult::new(
            Some(if passes {
                "rule check passed".to_string()
            } else {
                "rule check failed — waiting for input".to_string()
            }),
            next_action,
        ))
    }
}

// ---------------------------------------------------------------------------
// ManualWaitTask
// ---------------------------------------------------------------------------

/// Returns `NextAction::WaitForInput`. CLI `advance` resumes.
pub struct ManualWaitTask;

#[async_trait]
impl Task for ManualWaitTask {
    fn id(&self) -> &'static str {
        "manual_wait_task"
    }

    async fn run(
        &self,
        _context: graph_flow::Context,
    ) -> Result<TaskResult, graph_flow::GraphError> {
        Ok(TaskResult::new(
            Some("waiting for manual input".to_string()),
            NextAction::WaitForInput,
        ))
    }
}

// ---------------------------------------------------------------------------
// InnerGraphTask
// ---------------------------------------------------------------------------

/// Launches a child Session over a named inner graph (§3.4 graph-of-graphs).
///
/// On `run(ctx)`:
/// 1. Inherits `core_context.*` and `preset.input.*` from parent context.
/// 2. Calls `engine.spawn_child_session(parent_session_id, inner_graph, initial_ctx)`.
/// 3. Polls the child session to completion.
/// 4. Reads `output_binding` from child final context.
/// 5. Writes into parent `ctx["state.<parent_state>.output"]`.
/// 6. Returns `NextAction::Continue`.
pub struct InnerGraphTask {
    /// Reference to the orchestration engine for spawning child sessions.
    engine: Arc<dyn OrchestrationEngine>,
    /// The inner graph to execute.
    inner_graph: Arc<Graph>,
    /// The ID of the parent state (for output namespacing).
    parent_state_id: String,
    /// The key in parent context where the parent session ID is stored.
    parent_session_id_key: String,
    /// Output binding (e.g. "select.text") — which node's output to export.
    output_binding: Option<String>,
}

impl InnerGraphTask {
    /// Create a new `InnerGraphTask`.
    ///
    /// `parent_session_id_key` is the context key where the parent session ID
    /// can be found (e.g. `"_session_id"`).
    pub fn new(
        engine: Arc<dyn OrchestrationEngine>,
        inner_graph: Arc<Graph>,
        parent_state_id: impl Into<String>,
        parent_session_id_key: impl Into<String>,
        output_binding: Option<String>,
    ) -> Self {
        Self {
            engine,
            inner_graph,
            parent_state_id: parent_state_id.into(),
            parent_session_id_key: parent_session_id_key.into(),
            output_binding,
        }
    }
}

#[allow(clippy::too_many_lines)]
#[async_trait]
impl Task for InnerGraphTask {
    fn id(&self) -> &str {
        &self.parent_state_id
    }

    async fn run(
        &self,
        context: graph_flow::Context,
    ) -> Result<TaskResult, graph_flow::GraphError> {
        // 1. Read the parent session ID from context.
        let parent_session_id: String = context
            .get(&self.parent_session_id_key)
            .await
            .unwrap_or_default();

        if parent_session_id.is_empty() {
            return Err(graph_flow::GraphError::TaskExecutionFailed(
                "InnerGraphTask: parent session ID not found in context".into(),
            ));
        }

        // 2. Build the initial context for the child session.
        //    Inherit `core_context.*` and `preset.input.*` from parent.
        //    Use namespace "wrap" so inner nodes can't overwrite parent `state.*`.
        let child_ctx = graph_flow::Context::new();

        // Copy core_context.* keys from parent.
        for key_prefix in &["core_context", "preset.input"] {
            // We use a simple approach: copy known keys via serde.
            // Since Context uses Arc<DashMap>, we serialize the parent, extract
            // matching keys, and set them on the child.
            if let Ok(parent_data) = serde_json::to_value(&context) {
                if let Some(obj) = parent_data.as_object() {
                    for (k, v) in obj {
                        if k.starts_with(&format!("{key_prefix}.")) || k == *key_prefix {
                            child_ctx.set(k.as_str(), v.clone()).await;
                        }
                    }
                }
            }
        }

        // 3. Spawn the child session.
        let params = crate::engine::ChildSessionParams {
            parent_session_id: parent_session_id.clone(),
            inner_graph: self.inner_graph.clone(),
            initial_context: child_ctx,
        };

        let child_sid = self.engine.spawn_child_session(params).await.map_err(|e| {
            graph_flow::GraphError::TaskExecutionFailed(format!(
                "InnerGraphTask: failed to spawn child session: {e}"
            ))
        })?;

        // 4. Poll child session to completion.
        let mut last_error = None;
        for _ in 0..256 {
            let outcome = self.engine.run_step(&child_sid).await.map_err(|e| {
                graph_flow::GraphError::TaskExecutionFailed(format!(
                    "InnerGraphTask: run_step failed: {e}"
                ))
            })?;

            match outcome {
                crate::engine::StepOutcome::Completed { .. } => break,
                crate::engine::StepOutcome::Paused {
                    reason,
                    next_task_id,
                } => {
                    // Resume the child if it paused (shouldn't happen for
                    // rule-only inner graphs, but handle gracefully).
                    let _ = self
                        .engine
                        .signal(&child_sid, crate::engine::EngineSignal::Resume)
                        .await;
                    tracing::debug!(
                        child_session = %child_sid.0,
                        %next_task_id,
                        %reason,
                        "InnerGraphTask: child paused, resuming"
                    );
                }
                crate::engine::StepOutcome::WaitingForInput { .. } => {
                    // Inner graphs shouldn't wait for input; resume.
                    let _ = self
                        .engine
                        .signal(&child_sid, crate::engine::EngineSignal::Resume)
                        .await;
                }
                crate::engine::StepOutcome::Error(e) => {
                    last_error = Some(e);
                    break;
                }
            }
        }

        // 5. Read output_binding from child final context.
        let output_value = if let Some(ref binding) = self.output_binding {
            let child_ctx = self.engine.get_context(&child_sid).await.map_err(|e| {
                graph_flow::GraphError::TaskExecutionFailed(format!(
                    "InnerGraphTask: failed to get child context: {e}"
                ))
            })?;

            // Try to read as nodes.<node_id>.text first, then as-is.
            let node_key = format!("nodes.{binding}");
            let direct: Option<String> = child_ctx.get(binding).await;
            let namespaced: Option<String> = child_ctx.get(&node_key).await;
            direct.or(namespaced).unwrap_or_default()
        } else {
            String::new()
        };

        // 6. Write into parent context: state.<parent_state>.output
        let output_key = format!("state.{}.output", self.parent_state_id);
        context.set(&output_key, output_value.clone()).await;

        // Also store the child session ID for debugging.
        context
            .set(
                format!("_inner_child_session_{}", self.parent_state_id),
                child_sid.0,
            )
            .await;

        last_error.map_or_else(
            || {
                Ok(TaskResult::new(
                    Some(format!(
                        "inner graph '{}' completed, output: {}",
                        self.inner_graph.id,
                        if output_value.len() > 80 {
                            format!("{}...", &output_value[..80])
                        } else {
                            output_value.clone()
                        }
                    )),
                    NextAction::Continue,
                ))
            },
            |err| {
                Ok(TaskResult::new_with_status(
                    Some(format!(
                        "inner graph '{}' completed with error: {}",
                        self.inner_graph.id, err
                    )),
                    NextAction::Continue,
                    Some(err),
                ))
            },
        )
    }
}

// ---------------------------------------------------------------------------
// LlmJudgeTask — invokes judge.llm (or judge.rule) via capability registry
// ---------------------------------------------------------------------------

/// Evaluates an LLM judge exit condition by invoking the `judge.llm`
/// capability through the [`CapabilityRegistry`].
///
/// Flow:
/// 1. Render `template_file` content using handlebars against the context.
/// 2. Build capability input: `{ "prompt": <rendered>, _creator_id, _session_id }`.
/// 3. Call `judge_capability` (default `judge.llm`) via the registry.
/// 4. Parse the response `{ result: bool, reason: string }` into Continue/WaitForInput.
///
/// When the capability returns [`CapabilityError::WorkerUnavailable`] (no
/// worker IPC), logs a warning and returns `WaitForInput` so the state
/// machine doesn't silently advance without evaluation.
///
/// Design: `orchestration-engine.md` §4.4.1, compass §2.5.
pub struct LlmJudgeTask {
    /// Path to the judge prompt template (relative to bundle root).
    template: String,
    /// Capability name to invoke (default: `judge.llm`).
    capability_name: String,
    /// Shared capability registry.
    registry: Arc<CapabilityRegistry>,
}

impl LlmJudgeTask {
    /// Create a new `LlmJudgeTask`.
    #[must_use]
    pub const fn new(
        template: String,
        capability_name: String,
        registry: Arc<CapabilityRegistry>,
    ) -> Self {
        Self {
            template,
            capability_name,
            registry,
        }
    }

    /// Render the template and invoke the judge capability.
    async fn evaluate(
        &self,
        context: &graph_flow::Context,
    ) -> Result<(bool, String), graph_flow::GraphError> {
        // 1. Render the prompt template.
        let payload = build_nested_payload(context);
        let prompt = render_core_context_template(&self.template, &payload).unwrap_or_else(|e| {
            tracing::warn!(error = %e, "judge template render failed, using raw template");
            self.template.clone()
        });

        // 2. Build capability input with security-injected identity.
        let creator_id: String = context.get("_creator_id").await.unwrap_or_default();
        let session_id: String = context.get("_session_id").await.unwrap_or_default();

        let mut input = serde_json::json!({
            "prompt": prompt,
        });
        if let Some(obj) = input.as_object_mut() {
            if !creator_id.is_empty() {
                obj.insert("_creator_id".into(), Value::String(creator_id));
            }
            if !session_id.is_empty() {
                obj.insert("_session_id".into(), Value::String(session_id));
            }
        }

        // 3. Resolve the capability from the registry.
        let cap = self.registry.get(&self.capability_name).ok_or_else(|| {
            graph_flow::GraphError::TaskExecutionFailed(format!(
                "judge capability '{}' not found in registry",
                self.capability_name
            ))
        })?;

        // 4. Invoke the capability.
        match cap.run(input).await {
            Ok(output) => {
                let result = output
                    .get("result")
                    .and_then(Value::as_bool)
                    .unwrap_or(false);
                let reason = output
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("judge capability returned no reason")
                    .to_string();
                Ok((result, reason))
            }
            Err(CapabilityError::WorkerUnavailable) => {
                // No worker IPC available — cannot evaluate LLM judge.
                // Log and return NOGO so the state waits rather than advancing
                // without evaluation (safe default).
                //
                // R-V133P3-04 (deferred, documented): This creates a liveness/
                // DoS vector — an attacker who controls worker connectivity can
                // lock states in NOGO. Acceptable for V1.33 local-only single-user
                // daemon where the attacker model is the user themselves. For
                // multi-user or networked deployments, add a circuit-breaker,
                // timeout, or rule-based fallback.
                tracing::warn!(
                    capability = %self.capability_name,
                    "judge capability unavailable (no worker); returning NOGO"
                );
                Ok((
                    false,
                    "judge.llm: worker unavailable — cannot evaluate, waiting".to_string(),
                ))
            }
            Err(e) => Err(graph_flow::GraphError::TaskExecutionFailed(format!(
                "judge capability '{}' failed: {e}",
                self.capability_name
            ))),
        }
    }
}

// ---------------------------------------------------------------------------
// StateCompositeTask (outer graph — per §8.2)
// ---------------------------------------------------------------------------

/// Composite task for an outer-graph state node.
///
/// Encodes the full lifecycle of one state:
/// 1. Run enter actions (capability calls, inner graph launch).
/// 2. Evaluate `exit_when` condition.
/// 3. Return appropriate `NextAction`.
///
/// §8.2 mapping:
/// - `enter[*].kind=capability` → `CapabilityTask` (delegated internally).
/// - `enter[*].kind=inner_graph` → `InnerGraphTask` (spawns child session).
/// - `exit_when.kind=manual` → `ManualWaitTask` (returns `WaitForInput`).
/// - `exit_when.kind=rule` → `RuleCheckTask`.
/// - `exit_when.kind=llm_judge` → `LlmJudgeTask` (invokes judge.llm via registry).
/// - `exit_when.kind=graph_complete` → Continue (inner graph handles it).
/// - `terminal: true` → End.
pub struct StateCompositeTask {
    id: String,
    terminal: bool,
    enter_actions: Vec<EnterAction>,
    exit_when: Option<ExitWhen>,
    /// Orchestration engine reference (for spawning child sessions).
    engine: Option<Arc<dyn OrchestrationEngine>>,
    /// Named inner graphs keyed by name.
    inner_graphs: std::collections::HashMap<String, Arc<Graph>>,
    /// Output bindings for inner graphs: `inner_graph_name` → binding string.
    output_bindings: std::collections::HashMap<String, String>,
    /// Shared capability registry (injected by the engine; falls back to builtins if None).
    registry: Option<std::sync::Arc<CapabilityRegistry>>,
}

impl StateCompositeTask {
    /// Build a composite task from a manifest state definition (basic, no engine).
    ///
    /// Inner graph actions will fail at runtime if no engine is set.
    #[must_use]
    pub fn from_manifest(state: &StateDefinition) -> Self {
        Self {
            id: state.id.clone(),
            terminal: state.terminal,
            enter_actions: state.enter.clone(),
            exit_when: state.exit_when.clone(),
            engine: None,
            inner_graphs: std::collections::HashMap::new(),
            output_bindings: std::collections::HashMap::new(),
            registry: None,
        }
    }

    /// Set the orchestration engine reference.
    #[must_use]
    pub fn with_engine(mut self, engine: Arc<dyn OrchestrationEngine>) -> Self {
        self.engine = Some(engine);
        self
    }

    /// Set the inner graphs map.
    #[must_use]
    pub fn with_inner_graphs(
        mut self,
        graphs: std::collections::HashMap<String, Arc<Graph>>,
    ) -> Self {
        self.inner_graphs = graphs;
        self
    }

    /// Set the output bindings map.
    #[must_use]
    pub fn with_output_bindings(
        mut self,
        bindings: std::collections::HashMap<String, String>,
    ) -> Self {
        self.output_bindings = bindings;
        self
    }

    /// Set the shared capability registry.
    #[must_use]
    pub fn with_registry(mut self, registry: std::sync::Arc<CapabilityRegistry>) -> Self {
        self.registry = Some(registry);
        self
    }

    /// Resolve `template_file` paths in `exit_when: llm_judge` to actual file content.
    ///
    /// For embedded presets, reads the template content from the compiled-in
    /// bundle. If the file doesn't exist (e.g. test fixtures using inline
    /// strings), keeps the original value unchanged.
    ///
    /// # SAFETY
    ///
    /// Path traversal is validated at load time by `assert_template_file_safe`
    /// in the preset loader. Only relative paths without `..` reach this point.
    #[must_use]
    pub fn with_resolved_template(mut self, preset_id: &str) -> Self {
        if let Some(ExitWhen::LlmJudge {
            template_file: Some(ref path),
            ref judge_capability,
            ref min_interval,
        }) = self.exit_when
        {
            if let Some(content) = crate::preset::read_embedded_template(preset_id, path) {
                self.exit_when = Some(ExitWhen::LlmJudge {
                    template_file: Some(content),
                    judge_capability: judge_capability.clone(),
                    min_interval: min_interval.clone(),
                });
            }
        }
        self
    }
}

#[allow(clippy::too_many_lines)]
#[async_trait]
impl Task for StateCompositeTask {
    fn id(&self) -> &str {
        &self.id
    }

    async fn run(
        &self,
        context: graph_flow::Context,
    ) -> Result<TaskResult, graph_flow::GraphError> {
        // Check if this is a re-execution after resume.
        // After ManualWait returns WaitForInput, the engine signals Resume
        // and re-runs this task. On the second run, we should skip the wait
        // and just Continue.
        // Use a state-specific key to avoid leaking across state transitions.
        let resume_key = format!("_state_{}_resumed", self.id);
        let resumed: bool = context.get(&resume_key).await.unwrap_or(false);

        if resumed {
            // Already went through the full lifecycle before the wait.
            // Just continue to the next state.
            let response = Some(format!("state '{}': resumed, continuing", self.id));
            tracing::debug!(state_id = %self.id, terminal = self.terminal, "state resumed");
            return Ok(TaskResult::new(response, NextAction::Continue));
        }

        // 1. Process enter actions.
        for action in &self.enter_actions {
            match action {
                EnterAction::Capability { name, args } => {
                    context.set("_capability_name", name.clone()).await;

                    // Security (SEC-V131-01): inject trusted identity from engine
                    // context into capability args. Capabilities read `_creator_id`
                    // / `_session_id` from their input; the orchestration engine
                    // must set them at the invocation boundary so preset YAML
                    // cannot spoof these values (prevents cross-creator IPC IDOR).
                    let mut cap_input = args.clone().unwrap_or(Value::Null);
                    if cap_input.is_null() {
                        cap_input = Value::Object(serde_json::Map::new());
                    }
                    if let Some(obj) = cap_input.as_object_mut() {
                        // Preset args are untrusted. Strip protected identity
                        // fields first, then inject only trusted context values.
                        obj.remove("_creator_id");
                        obj.remove("_session_id");

                        if let Some(creator_id) = context.get::<String>("_creator_id").await {
                            obj.insert("_creator_id".into(), Value::String(creator_id));
                        }
                        if let Some(session_id) = context.get::<String>("_session_id").await {
                            obj.insert("_session_id".into(), Value::String(session_id));
                        }
                    }
                    context.set("_capability_input", cap_input).await;
                    let registry = self.registry.clone().unwrap_or_else(|| {
                        std::sync::Arc::new(CapabilityRegistry::with_builtins())
                    });
                    let cap_task = CapabilityTask { registry };
                    let cap_result = cap_task.run(context.clone()).await?;
                    // If capability task errored, propagate but still continue
                    // so the state machine doesn't get stuck.
                    if let Some(status_msg) = &cap_result.status_message {
                        context.set("_enter_error", status_msg.clone()).await;
                    }
                }
                EnterAction::InnerGraph { name } => {
                    // Spawn a child session for the inner graph.
                    let inner_graph = self.inner_graphs.get(name.as_str());
                    let output_binding = self.output_bindings.get(name.as_str()).cloned();

                    if let (Some(graph), Some(engine)) = (inner_graph, &self.engine) {
                        let inner_task = InnerGraphTask::new(
                            engine.clone(),
                            graph.clone(),
                            &self.id,
                            "_session_id",
                            output_binding,
                        );
                        inner_task.run(context.clone()).await?;
                    } else if inner_graph.is_none() {
                        // Inner graph not found in the map — error.
                        return Err(graph_flow::GraphError::TaskExecutionFailed(format!(
                            "InnerGraphTask: inner graph '{name}' not found"
                        )));
                    } else {
                        // No engine set — use fallback stub behavior.
                        context.set("_inner_graph_name", name.clone()).await;
                        context
                            .set(
                                format!("_inner_graph_error_{name}"),
                                "no engine reference available",
                            )
                            .await;
                    }
                }
            }
        }

        // 2. Evaluate exit_when.
        let next_action = match &self.exit_when {
            None => {
                // No exit condition — terminal state or just ends.
                if self.terminal {
                    NextAction::End
                } else {
                    NextAction::Continue
                }
            }
            Some(ExitWhen::Manual) => {
                // Mark that enter actions have been processed; next run after
                // resume will skip straight to Continue.
                context.set(resume_key, true).await;
                NextAction::WaitForInput
            }
            Some(ExitWhen::Rule) => {
                // Run rule check inline.
                let rule_task = RuleCheckTask;
                let result = rule_task.run(context.clone()).await?;
                result.next_action
            }
            Some(ExitWhen::LlmJudge {
                ref template_file,
                ref judge_capability,
                ref min_interval,
            }) => {
                // V1.33: invoke judge.llm capability through the registry.
                // Render template_file → build prompt → call capability → GO/NOGO.
                let template = template_file.as_deref().unwrap_or("");
                if template.is_empty() {
                    tracing::warn!(
                        state_id = %self.id,
                        "llm_judge exit_when has no template_file; returning WaitForInput"
                    );
                    context.set("_judge_result", false).await;
                    context
                        .set(
                            "_judge_reason",
                            "llm_judge: no template_file configured".to_string(),
                        )
                        .await;
                    NextAction::WaitForInput
                } else {
                    let cap_name = judge_capability.as_deref().unwrap_or("judge.llm");
                    let registry = self
                        .registry
                        .clone()
                        .unwrap_or_else(|| Arc::new(CapabilityRegistry::with_builtins()));

                    // min_interval throttle: skip evaluation if last
                    // evaluation was too recent.
                    if let Some(ref interval_str) = min_interval {
                        let throttle_key = format!("_judge_last_eval_{}", self.id);
                        let last_eval: Option<String> = context.get(&throttle_key).await;
                        if let Some(last) = last_eval {
                            if let Some(duration) = parse_iso8601_duration(interval_str) {
                                if let Ok(last_time) = last.parse::<chrono::DateTime<chrono::Utc>>()
                                {
                                    let now = chrono::Utc::now();
                                    if now - last_time < duration {
                                        tracing::debug!(
                                            state_id = %self.id,
                                            interval = %interval_str,
                                            "llm_judge: min_interval not elapsed, keeping previous result"
                                        );
                                        // Return the previous judge result.
                                        let prev_result: bool =
                                            context.get("_judge_result").await.unwrap_or(false);
                                        let prev_reason: String = context
                                            .get("_judge_reason")
                                            .await
                                            .unwrap_or_else(|| {
                                                "min_interval throttle: reusing previous result"
                                                    .to_string()
                                            });
                                        return Ok(TaskResult::new(
                                            Some(format!("judge (throttled): {prev_reason}")),
                                            if self.terminal {
                                                NextAction::End
                                            } else if prev_result {
                                                NextAction::Continue
                                            } else {
                                                NextAction::WaitForInput
                                            },
                                        ));
                                    }
                                }
                            }
                        }
                    }

                    let judge_task =
                        LlmJudgeTask::new(template.to_string(), cap_name.to_string(), registry);
                    let (result, reason) = judge_task.evaluate(&context).await?;

                    // Record timestamp for min_interval throttle.
                    if min_interval.is_some() {
                        let throttle_key = format!("_judge_last_eval_{}", self.id);
                        context
                            .set(throttle_key, chrono::Utc::now().to_rfc3339())
                            .await;
                    }

                    context.set("_judge_result", result).await;
                    context.set("_judge_reason", reason.clone()).await;

                    if result {
                        NextAction::Continue
                    } else {
                        NextAction::WaitForInput
                    }
                }
            }
            Some(ExitWhen::GraphComplete) => {
                // Inner graph completion propagates Continue.
                // (InnerGraphTask handles the actual child session; here we just
                // continue since the inner graph ran as part of enter actions.)
                NextAction::Continue
            }
            Some(ExitWhen::Timer { .. }) => {
                // Timer not yet implemented for V1.4; treat as manual wait.
                context.set(resume_key, true).await;
                NextAction::WaitForInput
            }
        };

        // 3. Terminal override — always End regardless of exit_when.
        let final_action = if self.terminal {
            NextAction::End
        } else {
            next_action
        };

        let response = if self.terminal {
            Some(format!("state '{}' completed (terminal)", self.id))
        } else {
            Some(format!("state '{}': {:?}", self.id, final_action))
        };

        Ok(TaskResult::new(response, final_action))
    }
}

// ---------------------------------------------------------------------------
// InnerGraphNodeTask (inner graph nodes — per §8.2, WS-E T5)
// ---------------------------------------------------------------------------

/// A task for a node within an inner graph.
///
/// §8.2 mapping:
/// - `kind=acp_prompt` → `AcpPromptTask` (full in T4; T3 stub that stores a placeholder).
///
/// ## WS-E T5: `session_id` routing
///
/// The task can route prompts to different agent sessions based on:
/// 1. Explicit `session_id` provided at construction (for preset resolution)
/// 2. Node's `agent` field resolved from `session_routes` in context (runtime lookup)
///
/// Backward compatible: if no session routing is configured, uses `"default"`.
pub struct InnerGraphNodeTask {
    id: String,
    /// Worker handle for IPC. `None` for stub mode.
    worker_handle: Option<std::sync::Arc<std::sync::Mutex<Option<crate::worker::WorkerHandle>>>>,
    /// Template file path (resolved relative to preset bundle root).
    template: String,
    /// Tool policy for this node.
    tool_policy: ToolPolicy,
    /// Explicit `session_id` (if preset resolution already determined it).
    session_id: Option<String>,
    /// Agent role reference (if node has `agent` field — will be resolved from `session_routes`).
    agent_ref: Option<String>,
}

impl InnerGraphNodeTask {
    /// Create a new inner graph node task (stub mode, no IPC).
    ///
    /// Used by preset loader for initial graph construction. The real task
    /// is wired at runtime when `worker_handle` and `session_routes` are available.
    #[must_use]
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            worker_handle: None,
            template: String::new(),
            tool_policy: ToolPolicy::AutoGrantReadOnly,
            session_id: None,
            agent_ref: None,
        }
    }

    /// Builder-style `session_id` setter.
    #[must_use]
    pub fn with_session_id(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Builder-style `agent_ref` setter.
    #[must_use]
    pub fn with_agent_ref(mut self, agent_ref: impl Into<String>) -> Self {
        self.agent_ref = Some(agent_ref.into());
        self
    }

    /// Builder-style `worker_handle` setter.
    #[must_use]
    pub fn with_worker_handle(
        mut self,
        handle: Option<std::sync::Arc<std::sync::Mutex<Option<crate::worker::WorkerHandle>>>>,
    ) -> Self {
        self.worker_handle = handle;
        self
    }

    /// Builder-style template setter.
    #[must_use]
    pub fn with_template(mut self, template: impl Into<String>) -> Self {
        self.template = template.into();
        self
    }

    /// Builder-style `tool_policy` setter.
    #[must_use]
    pub const fn with_tool_policy(mut self, tool_policy: ToolPolicy) -> Self {
        self.tool_policy = tool_policy;
        self
    }

    /// Resolve the effective `session_id` for this node (WS-E T5).
    ///
    /// Priority:
    /// 1. Explicit `session_id` (if set at construction)
    /// 2. Lookup from context using `agent_ref` (if set and `session_routes` present)
    /// 3. Default "default"
    async fn resolve_session_id(&self, context: &graph_flow::Context) -> String {
        // Priority 1: explicit session_id
        if let Some(ref sid) = self.session_id {
            return sid.clone();
        }

        // Priority 2: lookup from session_routes via agent_ref
        if let Some(ref agent) = self.agent_ref {
            let routes_key = "_session_routes";
            let routes_json: Option<serde_json::Value> = context.get(routes_key).await;
            if let Some(routes) = routes_json {
                if let Some(obj) = routes.as_object() {
                    if let Some(sid) = obj.get(agent).and_then(|v| v.as_str()) {
                        return sid.to_string();
                    }
                }
            }
        }

        // Priority 3: default
        "default".to_string()
    }
}

#[async_trait]
impl Task for InnerGraphNodeTask {
    fn id(&self) -> &str {
        &self.id
    }

    async fn run(
        &self,
        context: graph_flow::Context,
    ) -> Result<TaskResult, graph_flow::GraphError> {
        // Resolve session_id for this node (WS-E T5).
        let session_id = self.resolve_session_id(&context).await;

        // If we have a worker handle, delegate to AcpPromptTask.
        if let Some(ref handle_arc) = self.worker_handle {
            let acp_task = AcpPromptTask::new(
                Some(handle_arc.clone()),
                &self.id,
                self.template.clone(),
                self.tool_policy.clone(),
                Some(session_id),
            );
            return acp_task.run(context).await;
        }

        // Stub mode: return a placeholder.
        tracing::debug!(
            node_id = %self.id,
            session_id = %session_id,
            agent_ref = ?self.agent_ref,
            "InnerGraphNodeTask running in stub mode"
        );

        let output = format!(
            "inner_node:{}:stub_output [session_id={}]",
            self.id, session_id
        );
        context
            .set(format!("nodes.{}.text", self.id), output.clone())
            .await;
        context
            .set(format!("nodes.{}.output", self.id), output.clone())
            .await;
        context
            .set(format!("nodes.{}.session_id", self.id), session_id)
            .await;

        Ok(TaskResult::new(Some(output), NextAction::Continue))
    }
}

// ---------------------------------------------------------------------------
// AcpPromptTask (dispatches prompt to worker via IPC)
// ---------------------------------------------------------------------------

/// Tool policy for ACP prompt sessions.
///
/// Design: `orchestration-engine.md` §6.5.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolPolicy {
    /// All tools auto-granted (V1.0 behavior).
    AutoGrantAll,
    /// Reads allowed, writes require upcall.
    AutoGrantReadOnly,
    /// No tools allowed.
    DenyAll,
    /// Every tool triggers upcall.
    RequestPolicy,
}

impl std::str::FromStr for ToolPolicy {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "auto_grant_all" => Ok(Self::AutoGrantAll),
            "deny_all" => Ok(Self::DenyAll),
            "request_policy" => Ok(Self::RequestPolicy),
            _ => Ok(Self::AutoGrantReadOnly), // safe default
        }
    }
}

impl ToolPolicy {
    /// Serialize to the string form used in IPC.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::AutoGrantAll => "auto_grant_all",
            Self::AutoGrantReadOnly => "auto_grant_read_only",
            Self::DenyAll => "deny_all",
            Self::RequestPolicy => "request_policy",
        }
    }
}

/// A task that sends a prompt to an ACP agent via the Worker Manager IPC.
///
/// Design: `orchestration-engine.md` §4.4 (`AcpPromptTask` row) + §6.4 (IPC shapes).
///
/// `run(ctx)`:
/// 1. Renders the template with `handlebars` against `ctx` bindings.
/// 2. Calls `worker/acp_prompt { prompt, tool_policy, session_id }` via `WorkerHandle`.
/// 3. Streams `worker/acp_prompt_chunk` notifications into `ctx.chat_history`.
/// 4. On final reply, stores `result.full_text` at `ctx["state.<state_id>.output"]`.
/// 5. Returns `TaskResult { response: Some(full_text), next_action: NextAction::Continue }`.
pub struct AcpPromptTask {
    /// Worker handle for IPC. `None` for test stub mode.
    worker_handle: Option<std::sync::Arc<std::sync::Mutex<Option<crate::worker::WorkerHandle>>>>,
    /// State ID this task belongs to (for context key namespacing).
    state_id: String,
    /// Prompt template (handlebars syntax).
    template: String,
    /// Tool policy for this prompt.
    tool_policy: ToolPolicy,
    /// Session ID for multi-agent routing (WS-E T5).
    /// Routes the prompt to a specific agent slot within the worker.
    /// Default `"default"` for backward compatibility with single-agent workers.
    session_id: String,
}

impl AcpPromptTask {
    /// Create a new `AcpPromptTask`.
    ///
    /// `worker_handle`: the worker handle for IPC. Can be `None` for test mode
    /// where the task operates in stub mode.
    ///
    /// `session_id`: optional session ID for multi-agent routing. If `None`,
    /// defaults to `"default"` for backward compatibility with single-agent workers.
    pub fn new(
        worker_handle: Option<
            std::sync::Arc<std::sync::Mutex<Option<crate::worker::WorkerHandle>>>,
        >,
        state_id: impl Into<String>,
        template: impl Into<String>,
        tool_policy: ToolPolicy,
        session_id: Option<String>,
    ) -> Self {
        Self {
            worker_handle,
            state_id: state_id.into(),
            template: template.into(),
            tool_policy,
            session_id: session_id.unwrap_or_else(|| "default".to_string()),
        }
    }

    /// Test helper: create an `AcpPromptTask` with a worker handle directly.
    pub fn new_for_test(
        handle: crate::worker::WorkerHandle,
        state_id: impl Into<String>,
        template: impl Into<String>,
        tool_policy: ToolPolicy,
    ) -> Self {
        Self {
            worker_handle: Some(std::sync::Arc::new(std::sync::Mutex::new(Some(handle)))),
            state_id: state_id.into(),
            template: template.into(),
            tool_policy,
            session_id: "default".to_string(),
        }
    }

    /// Create an `AcpPromptTask` with explicit `session_id` (WS-E T5).
    ///
    /// Convenience constructor for multi-agent presets where the `session_id`
    /// is known at task creation time.
    pub fn with_session_id(
        worker_handle: Option<
            std::sync::Arc<std::sync::Mutex<Option<crate::worker::WorkerHandle>>>,
        >,
        state_id: impl Into<String>,
        template: impl Into<String>,
        tool_policy: ToolPolicy,
        session_id: impl Into<String>,
    ) -> Self {
        Self {
            worker_handle,
            state_id: state_id.into(),
            template: template.into(),
            tool_policy,
            session_id: session_id.into(),
        }
    }

    /// Render the prompt template using handlebars against a nested JSON payload.
    ///
    /// Renders the prompt template using handlebars against a nested JSON payload.
    ///
    /// Builds a nested JSON payload from flat context keys (e.g.
    /// `core_context.version` → `{"core_context":{"version":"..."}}`) so
    /// that handlebars nested path access (`{{world.title}}`) works.
    ///
    /// Falls back to the raw template if rendering fails (non-fatal for stubs).
    fn render_template(&self, context: &graph_flow::Context) -> String {
        let payload = build_nested_payload(context);
        match render_core_context_template(&self.template, &payload) {
            Ok(rendered) => rendered,
            Err(e) => {
                tracing::warn!(error = %e, "template render failed, using raw template");
                self.template.clone()
            }
        }
    }
}

#[async_trait]
impl Task for AcpPromptTask {
    fn id(&self) -> &str {
        &self.state_id
    }

    async fn run(
        &self,
        context: graph_flow::Context,
    ) -> Result<TaskResult, graph_flow::GraphError> {
        // 1. Render the template.
        let prompt = self.render_template(&context);

        // 2. If we have a worker handle, dispatch via IPC.
        let full_text = if let Some(ref handle_arc) = self.worker_handle {
            // Take the handle out of the Arc<Mutex> to avoid holding
            // the MutexGuard across the await point (which is !Send).
            let handle = {
                let mut guard = handle_arc.lock().map_err(|e| {
                    graph_flow::GraphError::TaskExecutionFailed(format!("worker handle lock: {e}"))
                })?;
                guard.take().ok_or_else(|| {
                    graph_flow::GraphError::TaskExecutionFailed(
                        "worker handle consumed or not available".into(),
                    )
                })?
            };

            // Call worker/acp_prompt via IPC.
            // WS-E T5: include session_id for multi-agent routing.
            let params = serde_json::json!({
                "prompt": prompt,
                "tool_policy": self.tool_policy.as_str(),
                "session_id": self.session_id,
            });

            let ipc_result = handle.call_json_rpc("worker/acp_prompt", params).await;

            // Put the handle back (even if IPC failed, the pipes may still be usable).
            {
                let mut guard = handle_arc.lock().map_err(|e| {
                    graph_flow::GraphError::TaskExecutionFailed(format!("worker handle lock: {e}"))
                })?;
                *guard = Some(handle);
            }

            match ipc_result {
                Ok(result) => {
                    // Extract full_text from the response.
                    result
                        .get("full_text")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string()
                }
                Err(e) => {
                    return Ok(TaskResult::new_with_status(
                        Some(format!("acp_prompt IPC error: {e}")),
                        NextAction::Continue,
                        Some(format!("worker/acp_prompt failed: {e}")),
                    ));
                }
            }
        } else {
            // Stub mode: return a placeholder.
            format!("[acp_prompt stub: {prompt}]")
        };

        // 3. Add to chat history.
        context.add_assistant_message(full_text.clone()).await;

        // 4. Store output at state.<state_id>.output.
        let output_key = format!("state.{}.output", self.state_id);
        context.set(&output_key, full_text.clone()).await;

        // 5. Return TaskResult.
        Ok(TaskResult::new(Some(full_text), NextAction::Continue))
    }
}

// ---------------------------------------------------------------------------
// CoreContext template rendering (DF-11)
// ---------------------------------------------------------------------------

/// Static Handlebars registry — avoids per-call allocation overhead.
///
/// Uses `no_escape` mode to preserve plain-text fidelity in prompts
/// (avoids HTML-encoding `&`, `<`, `>` etc.).
static HANDLEBARS: std::sync::OnceLock<handlebars::Handlebars<'static>> =
    std::sync::OnceLock::new();

/// Return a reference to the shared Handlebars registry.
///
/// The registry is initialized once with `no_escape` mode and reused
/// across all template renders for the process lifetime.
fn handlebars_registry() -> &'static handlebars::Handlebars<'static> {
    HANDLEBARS.get_or_init(|| {
        let mut reg = handlebars::Handlebars::new();
        reg.register_escape_fn(handlebars::no_escape);
        reg
    })
}

/// Render a handlebars template against a JSON payload.
///
/// Used by the orchestration engine to substitute `CoreContext` values into
/// prompt templates. Supports nested path access (e.g. `{{world.title}}`).
///
/// Uses `no_escape` mode to preserve plain-text fidelity in prompts
/// (avoids HTML-encoding `&`, `<`, `>` etc.).
///
/// # Errors
/// Returns an error if the template syntax is invalid or rendering fails.
pub fn render_core_context_template(
    template: &str,
    payload: &serde_json::Value,
) -> anyhow::Result<String> {
    handlebars_registry()
        .render_template(template, payload)
        .map_err(Into::into)
}

/// Build a nested JSON object from flat dot-separated context keys.
///
/// For example, keys like `core_context.version` become
/// `{"core_context": {"version": ...}}`. This allows handlebars templates
/// to use nested path access (`{{core_context.version}}`).
fn build_nested_payload(context: &graph_flow::Context) -> serde_json::Value {
    let Ok(serialized) = serde_json::to_value(context) else {
        return serde_json::json!({});
    };

    // serialized Context is {"data": {...}, "chat_history": {...}} —
    // extract just the data map.
    let data = serialized
        .as_object()
        .and_then(|obj| obj.get("data"))
        .and_then(|d| d.as_object())
        .cloned()
        .unwrap_or_default();

    let mut root = serde_json::Map::new();
    for (key, value) in &data {
        insert_nested(&mut root, key, value.clone());
    }
    serde_json::Value::Object(root)
}

/// Insert a value at a dot-separated path, creating intermediate objects.
fn insert_nested(
    map: &mut serde_json::Map<String, serde_json::Value>,
    key: &str,
    value: serde_json::Value,
) {
    let Some((prefix, leaf)) = key.rsplit_once('.') else {
        map.insert(key.to_string(), value);
        return;
    };

    let mut current = map;
    for segment in prefix.split('.') {
        let entry = current
            .entry(segment.to_string())
            .or_insert_with(|| serde_json::json!({}));
        current = entry
            .as_object_mut()
            .expect("insert_nested: intermediate segment must be an object");
    }
    current.insert(leaf.to_string(), value);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Parse an ISO-8601 duration string (e.g. `"PT6H"`, `"PT1H30M"`) into a
/// `chrono::Duration`.
///
/// Supports days (D), hours (H), minutes (M after T), and seconds (S).
/// Returns `None` for unparseable inputs, logging a warning.
fn parse_iso8601_duration(s: &str) -> Option<chrono::Duration> {
    let s = s.trim();
    if !s.starts_with('P') {
        tracing::warn!(input = %s, "min_interval: missing 'P' prefix");
        return None;
    }
    let body = &s[1..];

    // Parse optional days (before T) and optional time part (after T).
    let mut days: i64 = 0;
    let mut hours: i64 = 0;
    let mut minutes: i64 = 0;
    let mut seconds: i64 = 0;

    if let Some(time_part) = body.strip_prefix('T') {
        // Time-only form: PT6H, PT30M, PT1H30M15S
        if time_part.is_empty() {
            tracing::warn!(input = %s, "min_interval: empty time part after 'T'");
            return None;
        }

        let mut num_buf = String::new();
        for ch in time_part.chars() {
            match ch {
                '0'..='9' => num_buf.push(ch),
                'H' => {
                    hours = num_buf.parse().ok()?;
                    num_buf.clear();
                }
                'M' => {
                    minutes = num_buf.parse().ok()?;
                    num_buf.clear();
                }
                'S' => {
                    seconds = num_buf.parse().ok()?;
                    num_buf.clear();
                }
                _ => {
                    tracing::warn!(
                        input = %s,
                        char = %ch,
                        "min_interval: unsupported unit in time part"
                    );
                    return None;
                }
            }
        }
        if !num_buf.is_empty() {
            tracing::warn!(input = %s, "min_interval: trailing digits in time part");
            return None;
        }
    } else {
        // Date-part only: P1D, P7D (no T separator)
        let mut num_buf = String::new();
        for ch in body.chars() {
            match ch {
                '0'..='9' => num_buf.push(ch),
                'D' => {
                    days = num_buf.parse().ok()?;
                    num_buf.clear();
                }
                _ => {
                    tracing::warn!(
                        input = %s,
                        char = %ch,
                        "min_interval: unsupported unit (only D/H/M/S supported; months/years not supported)"
                    );
                    return None;
                }
            }
        }
        if !num_buf.is_empty() {
            tracing::warn!(input = %s, "min_interval: trailing digits in date part");
            return None;
        }
    }

    let total_seconds = days * 86400 + hours * 3600 + minutes * 60 + seconds;
    if total_seconds == 0 {
        tracing::warn!(input = %s, "min_interval: zero duration");
        return None;
    }

    Some(chrono::Duration::seconds(total_seconds))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn manual_wait_returns_wait_for_input() {
        let task = ManualWaitTask;
        let ctx = graph_flow::Context::new();
        let result = task.run(ctx).await.unwrap();
        assert!(matches!(result.next_action, NextAction::WaitForInput));
    }

    #[tokio::test]
    async fn inner_graph_task_requires_session_id_in_context() {
        let storage = Arc::new(graph_flow::InMemorySessionStorage::new());
        let engine = crate::GraphFlowEngine::new_with_storage(
            storage,
            std::sync::Arc::new(CapabilityRegistry::with_builtins()),
        );
        let inner_graph = graph_flow::Graph::new("test_inner");
        inner_graph.add_task(std::sync::Arc::new(InnerGraphNodeTask::new("n1")));

        let task = InnerGraphTask::new(
            Arc::new(engine),
            Arc::new(inner_graph),
            "A",
            "_session_id",
            Some("n1.text".to_string()),
        );
        let ctx = graph_flow::Context::new();
        // No _session_id set — should fail.
        let result = task.run(ctx).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("parent session ID not found"),
            "error should mention missing session ID: {err}"
        );
    }

    #[tokio::test]
    async fn rule_check_true_continues() {
        let task = RuleCheckTask;
        let ctx = graph_flow::Context::new();
        ctx.set("_rule", "always_true").await;
        let result = task.run(ctx).await.unwrap();
        assert!(matches!(result.next_action, NextAction::Continue));
    }

    #[tokio::test]
    async fn rule_check_false_waits() {
        let task = RuleCheckTask;
        let ctx = graph_flow::Context::new();
        ctx.set("_rule", "always_false").await;
        let result = task.run(ctx).await.unwrap();
        assert!(matches!(result.next_action, NextAction::WaitForInput));
    }

    #[tokio::test]
    async fn llm_judge_task_with_mock_worker_go() {
        // Prove LlmJudgeTask invokes judge.llm capability and maps GO → Continue.
        // Use the registry with a mock worker that returns "GO".
        use crate::capability::CapabilityRuntimeDeps;

        struct MockGoProvider;

        #[async_trait]
        impl crate::capability::WorkerHandleProvider for MockGoProvider {
            async fn call_acp_prompt(
                &self,
                _creator_id: &str,
                _session_id: &str,
                _prompt: String,
                _tool_policy: &str,
            ) -> Result<serde_json::Value, crate::capability::CapabilityError> {
                Ok(serde_json::json!({ "full_text": "GO — evaluation passes." }))
            }
        }

        let deps = CapabilityRuntimeDeps {
            pool: None,
            worker_provider: Some(std::sync::Arc::new(MockGoProvider)),
        };
        let registry = Arc::new(CapabilityRegistry::with_runtime_deps(&deps));

        let judge_task = LlmJudgeTask::new(
            "Is the task done?".to_string(),
            "judge.llm".to_string(),
            registry,
        );

        let ctx = graph_flow::Context::new();
        let (result, reason) = judge_task.evaluate(&ctx).await.unwrap();
        assert!(result, "GO response should give true: {reason}");
        assert!(reason.contains("go"), "reason should mention go: {reason}");
    }

    #[tokio::test]
    async fn llm_judge_task_with_mock_worker_nogo() {
        // Prove LlmJudgeTask maps NOGO → false.
        use crate::capability::CapabilityRuntimeDeps;

        struct MockNogoProvider;

        #[async_trait]
        impl crate::capability::WorkerHandleProvider for MockNogoProvider {
            async fn call_acp_prompt(
                &self,
                _creator_id: &str,
                _session_id: &str,
                _prompt: String,
                _tool_policy: &str,
            ) -> Result<serde_json::Value, crate::capability::CapabilityError> {
                Ok(serde_json::json!({ "full_text": "NO — stop and review." }))
            }
        }

        let deps = CapabilityRuntimeDeps {
            pool: None,
            worker_provider: Some(std::sync::Arc::new(MockNogoProvider)),
        };
        let registry = Arc::new(CapabilityRegistry::with_runtime_deps(&deps));

        let judge_task = LlmJudgeTask::new(
            "Is the task done?".to_string(),
            "judge.llm".to_string(),
            registry,
        );

        let ctx = graph_flow::Context::new();
        let (result, reason) = judge_task.evaluate(&ctx).await.unwrap();
        assert!(!result, "NOGO response should give false: {reason}");
        assert!(
            reason.contains("nogo"),
            "reason should mention nogo: {reason}"
        );
    }

    #[tokio::test]
    async fn llm_judge_task_no_worker_returns_nogo() {
        // Without a worker, judge.llm returns WorkerUnavailable.
        // LlmJudgeTask maps this to NOGO (safe default: wait, don't advance).
        let registry = Arc::new(CapabilityRegistry::with_builtins());
        let judge_task = LlmJudgeTask::new(
            "Is the task done?".to_string(),
            "judge.llm".to_string(),
            registry,
        );

        let ctx = graph_flow::Context::new();
        let (result, reason) = judge_task.evaluate(&ctx).await.unwrap();
        assert!(!result, "no worker → NOGO (safe default)");
        assert!(reason.contains("unavailable"), "reason: {reason}");
    }

    #[tokio::test]
    async fn llm_judge_task_missing_capability_errors() {
        // Unknown capability name → TaskExecutionFailed.
        let registry = Arc::new(CapabilityRegistry::with_builtins());
        let judge_task = LlmJudgeTask::new(
            "test".to_string(),
            "judge.nonexistent".to_string(),
            registry,
        );

        let ctx = graph_flow::Context::new();
        let result = judge_task.evaluate(&ctx).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not found"), "error: {err}");
    }

    // ── T5: StateCompositeTask integration — llm_judge GO/NOGO ────────

    /// Mock worker provider whose response is controlled at runtime.
    struct ControlledMockProvider {
        response: std::sync::Mutex<String>,
    }

    impl ControlledMockProvider {
        fn new(response: &str) -> Self {
            Self {
                response: std::sync::Mutex::new(response.to_string()),
            }
        }
    }

    #[async_trait]
    impl crate::capability::WorkerHandleProvider for ControlledMockProvider {
        async fn call_acp_prompt(
            &self,
            _creator_id: &str,
            _session_id: &str,
            _prompt: String,
            _tool_policy: &str,
        ) -> Result<serde_json::Value, crate::capability::CapabilityError> {
            let resp = self.response.lock().unwrap().clone();
            Ok(serde_json::json!({ "full_text": resp }))
        }
    }

    /// T5: novel-writing gathering exit with GO → Continue.
    #[tokio::test]
    async fn state_composite_llm_judge_go_continues() {
        use crate::capability::CapabilityRuntimeDeps;

        let provider = std::sync::Arc::new(ControlledMockProvider::new(
            "GO — sufficient material gathered.",
        ));
        let deps = CapabilityRuntimeDeps {
            pool: None,
            worker_provider: Some(provider),
        };
        let registry = Arc::new(CapabilityRegistry::with_runtime_deps(&deps));

        let state_def = crate::preset::manifest::StateDefinition {
            id: "gathering".into(),
            description: None,
            enter: vec![],
            exit_when: Some(ExitWhen::LlmJudge {
                template_file: Some("Evaluate: is gathering complete?".to_string()),
                judge_capability: Some("judge.llm".to_string()),
                min_interval: None,
            }),
            next: Some(crate::preset::manifest::NextTarget::Linear(
                "brainstorming".into(),
            )),
            terminal: false,
            context_update: None,
        };

        let task = StateCompositeTask::from_manifest(&state_def).with_registry(registry);

        let ctx = graph_flow::Context::new();
        let result = task.run(ctx.clone()).await.unwrap();
        assert!(
            matches!(result.next_action, NextAction::Continue),
            "GO → Continue, got {:?}",
            result.next_action
        );

        // Verify judge context was stored.
        let judge_result: bool = ctx.get("_judge_result").await.unwrap();
        assert!(judge_result, "judge_result should be true for GO");
    }

    /// T5: novel-writing gathering exit with NOGO → WaitForInput.
    #[tokio::test]
    async fn state_composite_llm_judge_nogo_waits() {
        use crate::capability::CapabilityRuntimeDeps;

        let provider = std::sync::Arc::new(ControlledMockProvider::new(
            "NO — need more research material.",
        ));
        let deps = CapabilityRuntimeDeps {
            pool: None,
            worker_provider: Some(provider),
        };
        let registry = Arc::new(CapabilityRegistry::with_runtime_deps(&deps));

        let state_def = crate::preset::manifest::StateDefinition {
            id: "gathering".into(),
            description: None,
            enter: vec![],
            exit_when: Some(ExitWhen::LlmJudge {
                template_file: Some("Evaluate: is gathering complete?".to_string()),
                judge_capability: Some("judge.llm".to_string()),
                min_interval: None,
            }),
            next: Some(crate::preset::manifest::NextTarget::Linear(
                "brainstorming".into(),
            )),
            terminal: false,
            context_update: None,
        };

        let task = StateCompositeTask::from_manifest(&state_def).with_registry(registry);

        let ctx = graph_flow::Context::new();
        let result = task.run(ctx.clone()).await.unwrap();
        assert!(
            matches!(result.next_action, NextAction::WaitForInput),
            "NOGO → WaitForInput, got {:?}",
            result.next_action
        );

        let judge_result: bool = ctx.get("_judge_result").await.unwrap();
        assert!(!judge_result, "judge_result should be false for NOGO");
    }

    /// T5: llm_judge without worker IPC → WaitForInput (safe fallback).
    #[tokio::test]
    async fn state_composite_llm_judge_no_worker_waits() {
        let registry = Arc::new(CapabilityRegistry::with_builtins());

        let state_def = crate::preset::manifest::StateDefinition {
            id: "gathering".into(),
            description: None,
            enter: vec![],
            exit_when: Some(ExitWhen::LlmJudge {
                template_file: Some("Evaluate: is gathering complete?".to_string()),
                judge_capability: None, // defaults to judge.llm
                min_interval: None,
            }),
            next: Some(crate::preset::manifest::NextTarget::Linear(
                "brainstorming".into(),
            )),
            terminal: false,
            context_update: None,
        };

        let task = StateCompositeTask::from_manifest(&state_def).with_registry(registry);

        let ctx = graph_flow::Context::new();
        let result = task.run(ctx.clone()).await.unwrap();
        assert!(
            matches!(result.next_action, NextAction::WaitForInput),
            "no worker → WaitForInput, got {:?}",
            result.next_action
        );
    }

    /// T5: llm_judge with empty template_file → WaitForInput.
    #[tokio::test]
    async fn state_composite_llm_judge_empty_template_waits() {
        let registry = Arc::new(CapabilityRegistry::with_builtins());

        let state_def = crate::preset::manifest::StateDefinition {
            id: "gathering".into(),
            description: None,
            enter: vec![],
            exit_when: Some(ExitWhen::LlmJudge {
                template_file: None,
                judge_capability: None,
                min_interval: None,
            }),
            next: Some(crate::preset::manifest::NextTarget::Linear(
                "brainstorming".into(),
            )),
            terminal: false,
            context_update: None,
        };

        let task = StateCompositeTask::from_manifest(&state_def).with_registry(registry);

        let ctx = graph_flow::Context::new();
        let result = task.run(ctx.clone()).await.unwrap();
        assert!(
            matches!(result.next_action, NextAction::WaitForInput),
            "empty template → WaitForInput, got {:?}",
            result.next_action
        );
    }

    // ── R-V133P3-02: template_file resolution tests ─────────────────────

    /// Proves that `with_resolved_template` loads actual file content from
    /// the embedded `novel-writing` preset bundle for `prompts/gathering-exit.md`.
    #[test]
    fn with_resolved_template_loads_embedded_file() {
        let state_def = crate::preset::manifest::StateDefinition {
            id: "gathering".into(),
            description: None,
            enter: vec![],
            exit_when: Some(ExitWhen::LlmJudge {
                template_file: Some("prompts/gathering-exit.md".to_string()),
                judge_capability: None,
                min_interval: None,
            }),
            next: None,
            terminal: false,
            context_update: None,
        };

        let task =
            StateCompositeTask::from_manifest(&state_def).with_resolved_template("novel-writing");

        // After resolution, the template_file should contain actual file content
        // (not the path string "prompts/gathering-exit.md").
        if let Some(ExitWhen::LlmJudge {
            ref template_file, ..
        }) = task.exit_when
        {
            let resolved = template_file.as_deref().unwrap_or("");
            assert!(
                !resolved.is_empty(),
                "template_file should be resolved to non-empty content"
            );
            assert!(
                !resolved.contains("prompts/gathering-exit.md"),
                "template_file should contain file content, not the path itself"
            );
            // The actual file should contain some meaningful template content.
            assert!(
                resolved.len() > 50,
                "resolved template should be substantial (got {} bytes)",
                resolved.len()
            );
        } else {
            panic!("expected LlmJudge exit_when after resolution");
        }
    }

    /// Proves that `with_resolved_template` keeps inline strings for unknown
    /// preset IDs (backward compat for tests using inline templates).
    #[test]
    fn with_resolved_template_preserves_inline_for_unknown_preset() {
        let state_def = crate::preset::manifest::StateDefinition {
            id: "test_state".into(),
            description: None,
            enter: vec![],
            exit_when: Some(ExitWhen::LlmJudge {
                template_file: Some("Evaluate: is gathering complete?".to_string()),
                judge_capability: None,
                min_interval: None,
            }),
            next: None,
            terminal: false,
            context_update: None,
        };

        let task = StateCompositeTask::from_manifest(&state_def)
            .with_resolved_template("nonexistent-preset");

        if let Some(ExitWhen::LlmJudge {
            ref template_file, ..
        }) = task.exit_when
        {
            // Should keep the original inline string.
            assert_eq!(
                template_file.as_deref(),
                Some("Evaluate: is gathering complete?")
            );
        } else {
            panic!("expected LlmJudge exit_when");
        }
    }

    // ── parse_iso8601_duration tests ──────────────────────────────────

    #[test]
    fn parse_duration_hours() {
        let dur = parse_iso8601_duration("PT6H").unwrap();
        assert_eq!(dur.num_hours(), 6);
    }

    #[test]
    fn parse_duration_minutes() {
        let dur = parse_iso8601_duration("PT30M").unwrap();
        assert_eq!(dur.num_minutes(), 30);
    }

    #[test]
    fn parse_duration_hours_minutes_seconds() {
        let dur = parse_iso8601_duration("PT1H30M15S").unwrap();
        assert_eq!(dur.num_seconds(), 3600 + 1800 + 15);
    }

    #[test]
    fn parse_duration_seconds() {
        let dur = parse_iso8601_duration("PT45S").unwrap();
        assert_eq!(dur.num_seconds(), 45);
    }

    /// R-V133P3-03: P1D (1 day) support.
    #[test]
    fn parse_duration_days() {
        let dur = parse_iso8601_duration("P1D").unwrap();
        assert_eq!(dur.num_hours(), 24);
    }

    /// R-V133P3-03: P7D (7 days) support.
    #[test]
    fn parse_duration_seven_days() {
        let dur = parse_iso8601_duration("P7D").unwrap();
        assert_eq!(dur.num_days(), 7);
    }

    /// R-V133P3-03: months/years are unsupported with warn.
    #[test]
    fn parse_duration_rejects_months() {
        assert!(parse_iso8601_duration("P1M").is_none());
    }

    #[test]
    fn parse_duration_invalid_returns_none() {
        assert!(parse_iso8601_duration("6H").is_none());
        assert!(parse_iso8601_duration("P6H").is_none());
        assert!(parse_iso8601_duration("").is_none());
        assert!(parse_iso8601_duration("PT").is_none());
    }

    #[tokio::test]
    async fn capability_task_runs_workspace_open() {
        let reg = Arc::new(CapabilityRegistry::with_builtins());
        let task = CapabilityTask { registry: reg };
        let ctx = graph_flow::Context::new();
        ctx.set("_capability_name", "workspace.open").await;
        ctx.set("_capability_input", serde_json::json!({})).await;
        let result = task.run(ctx).await.unwrap();
        assert!(matches!(result.next_action, NextAction::Continue));
    }

    #[tokio::test]
    async fn capability_task_returns_error_for_missing() {
        let reg = Arc::new(CapabilityRegistry::with_builtins());
        let task = CapabilityTask { registry: reg };
        let ctx = graph_flow::Context::new();
        ctx.set("_capability_name", "nonexistent.capability").await;
        let result = task.run(ctx).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn acp_prompt_task_stub_mode() {
        let task = AcpPromptTask::new(
            None, // no worker handle — stub mode
            "test-state",
            "Hello {{core_context.version}}",
            ToolPolicy::DenyAll,
            None, // default session_id
        );
        let ctx = graph_flow::Context::new();
        ctx.set("core_context.version", "42").await;
        let result = task.run(ctx).await.unwrap();
        assert!(matches!(result.next_action, NextAction::Continue));
        let response = result.response.unwrap();
        assert!(response.contains("Hello 42"), "response: {response}");
    }

    #[tokio::test]
    async fn acp_prompt_task_nested_handlebars_rendering() {
        // QC2-W-001: prove nested path rendering ({{world.title}}) works
        // through the real AcpPromptTask execution path.
        let task = AcpPromptTask::new(
            None,
            "test-state",
            "World: {{world.title}}, Chapter: {{world.chapter}}",
            ToolPolicy::DenyAll,
            None,
        );
        let ctx = graph_flow::Context::new();
        ctx.set("world.title", "Nexus").await;
        ctx.set("world.chapter", "1").await;
        let result = task.run(ctx).await.unwrap();
        let response = result.response.unwrap();
        assert!(
            response.contains("World: Nexus"),
            "nested world.title should render: {response}"
        );
        assert!(
            response.contains("Chapter: 1"),
            "nested world.chapter should render: {response}"
        );
    }

    #[tokio::test]
    async fn acp_prompt_task_no_escape_preserves_special_chars() {
        // QC2-S-001: handlebars must NOT HTML-escape prompt values.
        let task = AcpPromptTask::new(
            None,
            "test-state",
            "Text: {{content}}",
            ToolPolicy::DenyAll,
            None,
        );
        let ctx = graph_flow::Context::new();
        ctx.set("content", "foo & bar < baz > qux").await;
        let result = task.run(ctx).await.unwrap();
        let response = result.response.unwrap();
        assert!(
            response.contains("foo & bar < baz > qux"),
            "special chars must not be HTML-escaped: {response}"
        );
    }

    #[tokio::test]
    async fn acp_prompt_task_stores_output_in_context() {
        let task = AcpPromptTask::new(
            None,
            "state-1",
            "test prompt",
            ToolPolicy::AutoGrantReadOnly,
            None, // default session_id
        );
        let ctx = graph_flow::Context::new();
        let result = task.run(ctx.clone()).await.unwrap();
        let stored: String = ctx.get("state.state-1.output").await.unwrap();
        assert!(stored.contains("test prompt"), "stored: {stored}");
        assert_eq!(result.response.as_deref(), Some(stored.as_str()));
    }

    #[tokio::test]
    async fn acp_prompt_task_with_explicit_session_id() {
        // WS-E T5: test session_id field
        let task = AcpPromptTask::new(
            None,
            "state-1",
            "test prompt",
            ToolPolicy::AutoGrantReadOnly,
            Some("writer_session".to_string()), // explicit session_id
        );
        let ctx = graph_flow::Context::new();
        let _result = task.run(ctx.clone()).await.unwrap();
        // Stub mode should still work with explicit session_id
        let stored: String = ctx.get("state.state-1.output").await.unwrap();
        assert!(stored.contains("test prompt"), "stored: {stored}");
    }

    #[tokio::test]
    async fn acp_prompt_task_with_session_id_method() {
        // WS-E T5: test with_session_id constructor
        let task = AcpPromptTask::with_session_id(
            None,
            "state-1",
            "test prompt",
            ToolPolicy::AutoGrantReadOnly,
            "reviewer_session",
        );
        let ctx = graph_flow::Context::new();
        let result = task.run(ctx.clone()).await.unwrap();
        assert!(matches!(result.next_action, NextAction::Continue));
    }

    #[tokio::test]
    async fn inner_graph_node_task_stub_mode_with_session_id() {
        // WS-E T5: InnerGraphNodeTask should track session_id even in stub mode
        let task = InnerGraphNodeTask::new("n1").with_session_id("writer_session");
        let ctx = graph_flow::Context::new();
        let _result = task.run(ctx.clone()).await.unwrap();

        // Check output includes session_id
        let stored: String = ctx.get("nodes.n1.text").await.unwrap();
        assert!(
            stored.contains("writer_session"),
            "stored should contain session_id: {stored}"
        );

        // Check session_id stored in context
        let sid: String = ctx.get("nodes.n1.session_id").await.unwrap();
        assert_eq!(sid, "writer_session");
    }

    #[tokio::test]
    async fn inner_graph_node_task_resolves_session_id_from_routes() {
        // WS-E T5: InnerGraphNodeTask should lookup session_id from session_routes
        let task = InnerGraphNodeTask::new("n1").with_agent_ref("writer"); // agent role reference

        let ctx = graph_flow::Context::new();
        // Set session_routes: writer → writer_session
        ctx.set(
            "_session_routes",
            serde_json::json!({
                "writer": "writer_session",
                "reviewer": "reviewer_session",
            }),
        )
        .await;

        let _result = task.run(ctx.clone()).await.unwrap();

        // Check session_id was resolved correctly
        let sid: String = ctx.get("nodes.n1.session_id").await.unwrap();
        assert_eq!(sid, "writer_session");

        // Check output includes resolved session_id
        let stored: String = ctx.get("nodes.n1.text").await.unwrap();
        assert!(stored.contains("writer_session"), "stored: {stored}");
    }

    #[tokio::test]
    async fn inner_graph_node_task_falls_back_to_default() {
        // WS-E T5: No session_id, no agent_ref, no routes → default
        let task = InnerGraphNodeTask::new("n1");
        let ctx = graph_flow::Context::new();
        let _result = task.run(ctx.clone()).await.unwrap();

        let sid: String = ctx.get("nodes.n1.session_id").await.unwrap();
        assert_eq!(sid, "default");
    }

    #[tokio::test]
    async fn inner_graph_node_task_agent_ref_missing_in_routes() {
        // WS-E T5: agent_ref set but not in routes → default
        let task = InnerGraphNodeTask::new("n1").with_agent_ref("unknown_role");

        let ctx = graph_flow::Context::new();
        ctx.set(
            "_session_routes",
            serde_json::json!({
                "writer": "writer_session",
            }),
        )
        .await;

        let _result = task.run(ctx.clone()).await.unwrap();

        let sid: String = ctx.get("nodes.n1.session_id").await.unwrap();
        assert_eq!(sid, "default");
    }

    #[tokio::test]
    async fn inner_graph_node_task_explicit_session_id_overrides_routes() {
        // WS-E T5: explicit session_id should win over routes lookup
        let task = InnerGraphNodeTask::new("n1")
            .with_session_id("explicit_session")
            .with_agent_ref("writer"); // this should be ignored

        let ctx = graph_flow::Context::new();
        ctx.set(
            "_session_routes",
            serde_json::json!({
                "writer": "writer_session",
            }),
        )
        .await;

        let _result = task.run(ctx.clone()).await.unwrap();

        let sid: String = ctx.get("nodes.n1.session_id").await.unwrap();
        assert_eq!(sid, "explicit_session");
    }

    #[tokio::test]
    async fn tool_policy_from_str() {
        use std::str::FromStr;
        assert_eq!(
            ToolPolicy::from_str("auto_grant_all").unwrap(),
            ToolPolicy::AutoGrantAll
        );
        assert_eq!(
            ToolPolicy::from_str("auto_grant_read_only").unwrap(),
            ToolPolicy::AutoGrantReadOnly
        );
        assert_eq!(
            ToolPolicy::from_str("deny_all").unwrap(),
            ToolPolicy::DenyAll
        );
        assert_eq!(
            ToolPolicy::from_str("request_policy").unwrap(),
            ToolPolicy::RequestPolicy
        );
        assert_eq!(
            ToolPolicy::from_str("unknown").unwrap(),
            ToolPolicy::AutoGrantReadOnly
        );
    }

    // ── R-V113-003: OnceLock determinism regression test ──────────

    #[test]
    fn core_context_template_repeated_renders_are_deterministic() {
        let payload = serde_json::json!({ "world": { "title": "Nexus" } });

        let first = render_core_context_template("World: {{world.title}}", &payload)
            .expect("first render should succeed");
        let second = render_core_context_template("World: {{world.title}}", &payload)
            .expect("second render should succeed");

        assert_eq!(first, "World: Nexus");
        assert_eq!(second, first);
    }

    // ── SEC-V131-01: Caller-boundary identity injection regression ────
    //
    // Proves that when the orchestration engine invokes a capability via
    // StateCompositeTask, the trusted `_creator_id` / `_session_id` from
    // the engine context are injected into the capability's input args.
    // Without this fix, capabilities receive "default" for both fields.

    #[tokio::test]
    async fn sec_v131_01_state_composite_injects_trusted_identity_into_capability() {
        use crate::preset::manifest::EnterAction;

        // Build a StateCompositeTask with one enter action: acp.prompt
        // (standalone mode — no worker IPC needed for this regression).
        let state_def = crate::preset::manifest::StateDefinition {
            id: "gathering".into(),
            description: None,
            enter: vec![EnterAction::Capability {
                name: "acp.prompt".into(),
                args: Some(serde_json::json!({
                    "prompt": "Hello from orchestration engine"
                })),
            }],
            exit_when: None,
            next: None,
            terminal: true,
            context_update: None,
        };

        let task = StateCompositeTask::from_manifest(&state_def)
            .with_registry(Arc::new(CapabilityRegistry::with_builtins()));

        // Simulate the engine setting identity in context (as start_session does).
        let ctx = graph_flow::Context::new();
        ctx.set("_creator_id", "creator_alice").await;
        ctx.set("_session_id", "sess_42ch_001").await;

        let result = task.run(ctx.clone()).await.unwrap();
        assert!(
            matches!(result.next_action, NextAction::End),
            "terminal state should End"
        );

        // Verify the capability received the injected identity.
        // acp.prompt in standalone mode echoes session_id in its output.
        let output: serde_json::Value = ctx.get("_capability_output").await.unwrap_or(Value::Null);
        let full_text = output
            .get("full_text")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(
            full_text.contains("sess_42ch_001"),
            "capability should receive injected session_id, got: {full_text}"
        );
        assert!(
            !full_text.contains("default"),
            "capability should NOT receive 'default' session_id, got: {full_text}"
        );
    }

    #[tokio::test]
    async fn sec_v131_01_engine_overwrites_spoofed_identity_in_preset_args() {
        use crate::preset::manifest::EnterAction;

        // Preset YAML tries to spoof _creator_id / _session_id in args.
        // The engine MUST overwrite these with trusted values from context.
        let state_def = crate::preset::manifest::StateDefinition {
            id: "spoof_test".into(),
            description: None,
            enter: vec![EnterAction::Capability {
                name: "acp.prompt".into(),
                args: Some(serde_json::json!({
                    "prompt": "test",
                    "_creator_id": "spoofed_creator",
                    "_session_id": "spoofed_session"
                })),
            }],
            exit_when: None,
            next: None,
            terminal: true,
            context_update: None,
        };

        let task = StateCompositeTask::from_manifest(&state_def)
            .with_registry(Arc::new(CapabilityRegistry::with_builtins()));

        let ctx = graph_flow::Context::new();
        ctx.set("_creator_id", "real_creator").await;
        ctx.set("_session_id", "real_session").await;

        let result = task.run(ctx.clone()).await.unwrap();
        assert!(matches!(result.next_action, NextAction::End));

        let output: serde_json::Value = ctx.get("_capability_output").await.unwrap_or(Value::Null);
        let full_text = output
            .get("full_text")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        assert!(
            full_text.contains("real_session"),
            "engine must overwrite spoofed session_id with real value, got: {full_text}"
        );
        assert!(
            !full_text.contains("spoofed_session"),
            "spoofed session_id must not reach capability, got: {full_text}"
        );
    }

    #[tokio::test]
    async fn sec_v131_01_strips_spoofed_identity_when_context_missing() {
        use crate::preset::manifest::EnterAction;

        let state_def = crate::preset::manifest::StateDefinition {
            id: "spoof_without_context".into(),
            description: None,
            enter: vec![EnterAction::Capability {
                name: "acp.prompt".into(),
                args: Some(serde_json::json!({
                    "prompt": "test",
                    "_creator_id": "victim_creator",
                    "_session_id": "victim_session"
                })),
            }],
            exit_when: None,
            next: None,
            terminal: true,
            context_update: None,
        };

        let task = StateCompositeTask::from_manifest(&state_def)
            .with_registry(Arc::new(CapabilityRegistry::with_builtins()));

        let ctx = graph_flow::Context::new();
        let result = task.run(ctx.clone()).await.unwrap();
        assert!(matches!(result.next_action, NextAction::End));

        let input: serde_json::Value = ctx.get("_capability_input").await.unwrap_or(Value::Null);
        assert!(
            input.get("_creator_id").is_none(),
            "untrusted _creator_id must be stripped when trusted context is absent: {input}"
        );
        assert!(
            input.get("_session_id").is_none(),
            "untrusted _session_id must be stripped when trusted context is absent: {input}"
        );
    }
}
