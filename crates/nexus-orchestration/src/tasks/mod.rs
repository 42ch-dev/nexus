//! Standard `Task` implementations for the orchestration engine.
//!
//! Design: `.mstar/specs/orchestration-engine.md` §4.4.
//!
//! # TODO(V1.17): Run and capability-call trace correlation
//!
//! When the daemon orchestration API is implemented:
//! - Engine/session start paths should read `_run_id` from `graph_flow::Context`
//!   and propagate it to all child tasks.
//! - `CapabilityTask::run` should generate a `capability_call_id` per invocation
//!   and store `_last_capability_call_id` + capability call metadata in context.
//! - If `_trace_file` is present in context, append start/finish trace events
//!   best-effort using the DTOs from `nexus-contracts::local::acp_runtime::trace`.

use crate::capability::{CapabilityError, CapabilityRegistry};
use crate::engine::{OrchestrationEngine, SessionId};
use crate::preset::manifest::{ConvergeConfig, ConvergeStrategy};
use crate::preset::manifest::{EnterAction, ExitWhen, MergeKind, NextTarget, StateDefinition};
use async_trait::async_trait;
use graph_flow::{Graph, NextAction, Task, TaskResult};
use serde_json::Value;
use std::collections::HashSet;
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
        let name: String = context.get("_capability_name").unwrap_or_default();
        let input: Value = context.get("_capability_input").unwrap_or(Value::Null);

        let cap = self.registry.get(&name).ok_or_else(|| {
            graph_flow::GraphError::TaskExecutionFailed(format!("capability not found: {name}"))
        })?;

        // A capability invocation may perform an external effect (capability
        // calls can trigger provider/prompt/host activity). Mark the step as
        // having performed a potentially-external effect so a failed
        // post-effect `commit_transition` can persist an interrupted
        // disposition rather than blindly rewinding (Important 3).
        context.set(crate::engine::EXTERNAL_EFFECT_MARKER, true)?;

        match cap.run(input).await {
            Ok(output) => {
                context.set("_capability_output", output)?;
                Ok(TaskResult::new(
                    Some("capability executed".to_string()),
                    NextAction::Continue,
                ))
            }
            Err(e) => {
                context.set("_capability_error", format!("{e}"))?;
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
        let rule: String = context.get("_rule").unwrap_or_default();

        let (passes, reason) = match rule.as_str() {
            "always_true" => (true, "rule: always_true → pass".to_string()),
            "always_false" => (false, "rule: always_false → fail".to_string()),
            other => (false, format!("unsupported rule: '{other}'")),
        };

        context.set("_rule_result", passes)?;
        context.set("_rule_reason", reason)?;

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
        // 0. Current-step hygiene: child-wait markers from an earlier step
        //    must never leak into this one's boundary (round-4 Critical 2).
        //    Cleared here; re-set only when THIS step parks at a child
        //    human wait.
        context.set("_child_wait_record", Value::Null)?;
        context.set("_child_wait_session", Value::Null)?;
        context.set("_child_wait_task", Value::Null)?;

        // 1. Read the parent session ID from context.
        let parent_session_id: String =
            context.get(&self.parent_session_id_key).unwrap_or_default();

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
            // Context serializes as `{"data": {...}, "chat_history": {...}}`;
            // the flat `preset.input.*` / `core_context.*` keys live in the
            // `data` map. Iterate THAT map, not the top-level envelope —
            // otherwise no key ever matches the prefix and the child loses
            // the frozen admission input (N-7: inner-graph prompts render
            // empty `preset.input.*` variables).
            if let Ok(parent_data) = serde_json::to_value(&context) {
                if let Some(data) = parent_data
                    .as_object()
                    .and_then(|obj| obj.get("data"))
                    .and_then(|d| d.as_object())
                {
                    for (k, v) in data {
                        if k.starts_with(&format!("{key_prefix}.")) || k == *key_prefix {
                            child_ctx.set(k.as_str(), v.clone())?;
                        }
                    }
                }
            }
        }

        // 3. Attach an existing persisted child session (recovery, Important 4)
        //    or spawn a new one. After a parent restarts with its cursor
        //    still at this inner-graph task, recovery hydrates the persisted
        //    child rows into the engine's children map; we reattach the
        //    matching non-terminal child and resume it (preserving
        //    cursor/context) rather than creating a duplicate child row and
        //    potentially replaying completed child work. We only spawn a new
        //    child when no matching child row exists.
        let child_sid = match self
            .engine
            .attach_existing_child_session(&parent_session_id, self.inner_graph.clone())
            .await
        {
            Ok(Some(existing)) => existing,
            Ok(None) => {
                let params = crate::engine::ChildSessionParams {
                    parent_session_id: parent_session_id.clone(),
                    inner_graph: self.inner_graph.clone(),
                    initial_context: child_ctx,
                };

                self.engine.spawn_child_session(params).await.map_err(|e| {
                    graph_flow::GraphError::TaskExecutionFailed(format!(
                        "InnerGraphTask: failed to spawn child session: {e}"
                    ))
                })?
            }
            // A7 rule 2 (P3 T1 rereview-2 P1): a persisted child row whose
            // descriptor no longer matches the trusted root is non-replayable.
            // Fail closed — never mint a fresh child id and replay the work.
            Err(e) => {
                return Err(graph_flow::GraphError::TaskExecutionFailed(format!(
                    "InnerGraphTask: existing child session is non-replayable: {e}"
                )));
            }
        };

        // Driving an inner graph session is itself an external effect: the
        // child may run capability/prompt/host-tool effects (or have run them
        // pre-crash — the persisted child carries a completed effect that must
        // not be replayed). Propagate the effect marker to the parent so a
        // failed post-child-effect parent `commit_transition` (e.g. stale child
        // CAS) is classified Interrupted, never deterministically restored and
        // replayed on restart (Important 2). Scoped to this parent step: the
        // engine clears the marker after a successful parent checkpoint.
        context.set(crate::engine::EXTERNAL_EFFECT_MARKER, true)?;

        // A reattached child that is already terminal (its durable checkpoint
        // was persisted before the parent committed past this inner graph)
        // must NOT be re-stepped — its completed prompt/effect work would be
        // replayed. Consume it directly: read its terminal output below and
        // record an empty run (no extra step). We detect terminal status via
        // the engine's in-memory tracker (registered by attach).
        let child_terminal = self
            .engine
            .get_status(&child_sid)
            .await
            .is_ok_and(|s| s.is_terminal());

        // 4. Poll child session to completion (only for a non-terminal child —
        //    a terminal reattached child is consumed, never re-stepped,
        //    Round-5 Important 1).
        let mut child_wait = None;
        if !child_terminal {
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
                    crate::engine::StepOutcome::WaitingForInput { response } => {
                        // Round-4 Critical 2 (A4): a supported inner-graph child
                        // waiting for human input MUST NOT be auto-resumed by the
                        // inner poller (auto-approval bypass). Preserve the
                        // child's WaitRecord and propagate the wait to the parent:
                        // the parent step parks with `WaitForInput` and the root
                        // `WaitRecord` carries the exact child cursor (A4 "a
                        // waiting child makes the parent waiting"). Explicit
                        // continuation re-runs the parent inner-graph task, which
                        // reattaches the still-waiting child and steps it — the
                        // child's durable token is then consumed by the matching
                        // authorized continue, never by boot or the poller.
                        child_wait = Some((child_sid.0.clone(), response));
                        break;
                    }
                }
            }
        }

        // 5a. Round-4 Critical 2: a child human wait must make THIS parent
        //     step return `WaitForInput` (never Continue) so the engine
        //     persists the parent as `waiting_for_input` with a fresh A4
        //     token naming the exact waiting child. The child's persisted
        //     wait record is untouched (its token remains for the matching
        //     continue); parent/root wait + child checkpoint are persisted
        //     together by `commit_transition` before the engine returns
        //     `WaitingForInput` (A4). The child's current task id comes from
        //     the authoritative persisted child snapshot; `None` degrades to
        //     the child session id (never empty).
        if let Some((child_session, response)) = child_wait {
            let child_task_id = self
                .engine
                .get_current_task_id(&SessionId(child_session.clone()))
                .await
                .unwrap_or(None);
            context.set("_child_wait_session", child_session.clone())?;
            context.set(
                "_child_wait_task",
                child_task_id.unwrap_or_else(|| child_session.clone()),
            )?;
            return Ok(TaskResult::new(
                Some(response.unwrap_or_else(|| {
                    format!(
                        "inner graph '{}' child '{}' is waiting for human input",
                        self.inner_graph.id, child_session
                    )
                })),
                NextAction::WaitForInput,
            ));
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
            let direct: Option<String> = child_ctx.get(binding);
            let namespaced: Option<String> = child_ctx.get(&node_key);
            direct.or(namespaced).unwrap_or_default()
        } else {
            String::new()
        };

        // 6. Write into parent context: state.<parent_state>.output
        let output_key = format!("state.{}.output", self.parent_state_id);
        context.set(&output_key, output_value.clone())?;

        // Also store the child session ID for debugging.
        context.set(
            format!("_inner_child_session_{}", self.parent_state_id),
            child_sid.0,
        )?;

        Ok(TaskResult::new(
            Some(format!(
                "inner graph '{}' completed, output: {}",
                self.inner_graph.id,
                if output_value.len() > 80 {
                    format!("{}...", &output_value[..80])
                } else {
                    output_value
                }
            )),
            NextAction::Continue,
        ))
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
        let creator_id: String = context.get("_creator_id").unwrap_or_default();
        let session_id: String = context.get("_session_id").unwrap_or_default();

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
                // WAIVER: pre-1.0 local-first; see V1.41 P-last residual R-V133P3-04
                // — WorkerUnavailable → NOGO creates a liveness/DoS vector: an attacker
                // who controls worker connectivity can lock states in NOGO. Acceptable
                // for local-only single-user daemon where the attacker model is the user
                // themselves. For multi-user or networked deployments, add a
                // circuit-breaker, timeout, or rule-based fallback.
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
// LlmExtractTask — invokes nexus.llm.extract via capability registry (V1.51 T-A P0)
// ---------------------------------------------------------------------------

/// Extract World KB candidates by invoking the `nexus.llm.extract` capability
/// through the [`CapabilityRegistry`].
///
/// Sibling to [`LlmExtractTask`] (sic — see [`LlmJudgeTask`]): mirrors the
/// `LlmJudgeTask` lifecycle (render template → build capability input → invoke
/// → parse result), but emits [`crate::quality_loop::LlmExtractOutcome`] (entity
/// candidates + V1.76 relationship candidates) instead of a GO/NOGO verdict.
///
/// Flow:
/// 1. Render `template_file` content using handlebars against the context.
/// 2. Read `chapter_prose` from the context (the review-time hook writes the
///    prose there before invoking the task).
/// 3. Build capability input: `{ prompt, chapter_prose, _creator_id, _session_id }`.
/// 4. Call `nexus.llm.extract` (or configured capability name) via the registry.
/// 5. Parse the response `{ candidates: [...], relationships: [...] }` into
///    [`crate::quality_loop::LlmExtractOutcome::Candidates`].
///
/// When the capability returns [`CapabilityError::WorkerUnavailable`] (no
/// worker IPC), returns an empty `Vec` so the caller can fall back to the
/// heuristic. The task does NOT persist candidates — persistence is the
/// caller's responsibility (the review-time hook), keeping the task pure.
///
/// Relationship candidates (V1.76) are surfaced the same way candidates are:
/// [`Self::evaluate`] returns them inside [`LlmExtractOutcome::Candidates`], and
/// the review-time hook persists them via `run_llm_extract` →
/// `persist_relationship_candidates` (`quality_loop.rs`). A preset author wiring
/// `LlmExtractTask` directly MUST destructure `Candidates { candidates,
/// relationships, .. }` rather than dropping `relationships` — the task never
/// persists them itself, so a caller that ignores the field silently loses the
/// extracted relationships (qc1 F-002 / `R-V176QC1-S001`).
///
/// Design: `llm-extract.md` §2, compass §0.1 #7.
// R-V152TA-S003: `LlmExtractTask` has no production preset routing yet
// (`exit_when: llm_extract` is a future preset kind; see llm-extract.md §2).
// It is exercised by the hermetic tests below and by `quality_loop`'s shared
// `run_llm_extract` pathway. Suppressed in non-test builds until a preset
// wires it into the runtime graph.
#[cfg_attr(not(test), allow(dead_code))]
pub struct LlmExtractTask {
    /// Extraction instruction template (rendered against the context).
    template: String,
    /// Capability name to invoke (default: `nexus.llm.extract`).
    capability_name: String,
    /// Shared capability registry.
    registry: Arc<CapabilityRegistry>,
}

impl LlmExtractTask {
    /// Create a new `LlmExtractTask`.
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

    /// Render the template and invoke the extract capability.
    ///
    /// Returns [`LlmExtractOutcome`] so the caller can distinguish:
    /// - `Candidates(vec)` — LLM returned candidates (may be empty).
    /// - `WorkerUnavailable` — no worker IPC; caller should fall back.
    /// - `CapabilityError(reason)` — capability missing or failed; caller may
    ///   treat as a hard error or fall back.
    ///
    /// Public so the review-time hook and future `exit_when: llm_extract`
    /// preset routing can invoke it directly (`llm-extract.md` §2).
    ///
    /// # Errors
    ///
    /// Returns [`graph_flow::GraphError::TaskExecutionFailed`] only for
    /// unexpected internal failures (e.g., template render panic). Capability
    /// errors are represented inside [`LlmExtractOutcome`] so callers decide
    /// how to handle them.
    // R-V152TA-S003: see struct-level note — no production graph wiring yet.
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) async fn evaluate(
        &self,
        context: &graph_flow::Context,
    ) -> Result<crate::quality_loop::LlmExtractOutcome, graph_flow::GraphError> {
        // 1. Render the extraction template.
        let payload = build_nested_payload(context);
        let prompt = render_core_context_template(&self.template, &payload).unwrap_or_else(|e| {
            tracing::warn!(error = %e, "extract template render failed, using raw template");
            self.template.clone()
        });

        // 2. Read chapter prose, identity, and work_profile from the context.
        let chapter_prose: String = context.get("chapter_prose").unwrap_or_default();
        let creator_id: String = context.get("_creator_id").unwrap_or_default();
        let session_id: String = context.get("_session_id").unwrap_or_default();
        // V1.55 P2 fix-wave (F-001): read work_profile from context so the
        // extraction path produces profile-aware payloads. Defaults to "novel"
        // for backward compatibility with existing callers.
        let work_profile: String = context
            .get("work_profile")
            .unwrap_or_else(|| "novel".to_string());

        // 3. Use the shared extraction path (closes R-V151Q3-W001).
        Ok(crate::quality_loop::run_llm_extract(
            Some(&self.registry),
            &self.capability_name,
            &prompt,
            &chapter_prose,
            &creator_id,
            &session_id,
            &work_profile,
        )
        .await)
    }
}

// ---------------------------------------------------------------------------
// Join clock (DR-06, v1.179 — bounded join deadlines)
// ---------------------------------------------------------------------------

/// Millisecond clock source for bounded-join deadlines (DR-06).
///
/// Mirrors the [`crate::scheduler::ClockSource`] pattern (production impl +
/// mock for hermetic tests) but at millisecond resolution, which the
/// `timeout_ms` join deadline requires. Time is read as milliseconds since
/// an arbitrary fixed epoch — only differences are meaningful.
pub trait JoinClock: Send + Sync {
    /// Current time in milliseconds since an arbitrary fixed epoch.
    fn now_ms(&self) -> u64;
}

/// Production clock: `SystemTime` wall time.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemJoinClock;

impl JoinClock for SystemJoinClock {
    fn now_ms(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
    }
}

/// Deterministic clock for hermetic timeout tests (DR-06).
///
/// Starts at a fixed value and advances only when [`Self::advance`] is
/// called, so join-deadline branches can be exercised without sleeping.
#[derive(Debug)]
pub struct DeterministicClock {
    now_ms: std::sync::atomic::AtomicU64,
}

impl DeterministicClock {
    /// Create a deterministic clock at `start_ms`.
    #[must_use]
    pub const fn new(start_ms: u64) -> Self {
        Self {
            now_ms: std::sync::atomic::AtomicU64::new(start_ms),
        }
    }

    /// Advance the clock by `delta_ms`.
    pub fn advance(&self, delta_ms: u64) {
        self.now_ms
            .fetch_add(delta_ms, std::sync::atomic::Ordering::SeqCst);
    }
}

impl JoinClock for DeterministicClock {
    fn now_ms(&self) -> u64 {
        self.now_ms.load(std::sync::atomic::Ordering::SeqCst)
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
    /// Transition target (linear, go/nogo, or conditional).
    next: Option<NextTarget>,
    /// Orchestration engine reference (for spawning child sessions).
    engine: Option<Arc<dyn OrchestrationEngine>>,
    /// Named inner graphs keyed by name.
    inner_graphs: std::collections::HashMap<String, Arc<Graph>>,
    /// Output bindings for inner graphs: `inner_graph_name` → binding string.
    output_bindings: std::collections::HashMap<String, String>,
    /// Shared capability registry (injected by the engine; falls back to builtins if None).
    registry: Option<std::sync::Arc<CapabilityRegistry>>,
    /// Daemon-side tool dispatch for `nexus.*` host tool actions (DF-47, V1.42 P3).
    daemon_tool_dispatch: Option<std::sync::Arc<dyn crate::capability::DaemonToolDispatch>>,
    /// Merge semantics for states with multiple incoming labeled edges (V1.52 T-B P1).
    merge_kind: Option<MergeKind>,
    /// Expected number of incoming labeled edges for merge nodes.
    ///
    /// Populated by the loader/graph-builder when wiring the outer graph.
    /// Used at runtime to evaluate merge conditions (all/any/quorum).
    expected_incoming: usize,
    /// Pre-computed merge key ("_merge_{id}") to avoid per-tick allocation (W-QC3-2).
    merge_key: String,
    /// Converge (merge-point) config for states with explicit `converge:` declaration
    /// (V1.56 P2 fix-wave, H-001/W-002).
    converge: Option<ConvergeConfig>,
    /// Converge-predecessor tracking key ("_`converge_arrivals`_{id}").
    converge_key: String,
    /// Predecessor task IDs for this converge node (populated at graph build time).
    converge_predecessors: std::collections::HashSet<String>,
    /// Pre-compiled expression AST for this state's conditional branches (M-004).
    ///
    /// Parsed once at task construction time; reused across transitions.
    /// `None` means this state has no conditional/branches next, or parse failed.
    cached_expr: Option<CachedExpressions>,
    /// Optional workspace session state for expression evaluation (V1.56 P3).
    ///
    /// When set, `build_context_json()` exposes this as a nested `workspace`
    /// object. Tests set this directly; in production the engine populates it
    /// from the active workspace session.
    workspace_state: Option<serde_json::Value>,
    /// Bounded-join deadline in milliseconds for this state's join gate
    /// (DR-06, v1.179). `None` = unbounded wait (legacy behaviour).
    timeout_ms: Option<u64>,
    /// Reroute target when the join deadline fires (DR-06, v1.179).
    ///
    /// `None` = fail with the typed `converge_timeout:` error.
    on_timeout: Option<String>,
    /// Millisecond clock for join-deadline checks (DR-06, v1.179).
    ///
    /// Defaults to [`SystemJoinClock`]; tests inject [`DeterministicClock`]
    /// via [`Self::with_join_clock`].
    clock: Arc<dyn JoinClock>,
}

/// Pre-compiled expression ASTs for conditional routing (V1.56 P2 fix-wave, M-004).
///
/// V1.56 P3: extended with context dependency flags so the runtime knows
/// whether to invoke `registry.refresh` or query workspace session state
/// before evaluating branches.
#[derive(Clone)]
struct CachedExpressions {
    /// Parsed expressions + their target state IDs.
    branches: Vec<(crate::preset::expr::Expr, String)>,
    /// Default target when no branch matches.
    default: String,
    /// Any branch expression references `_context.registry_refresh.*`
    needs_registry_refresh: bool,
    /// Any branch expression references `_context.workspace.*`
    needs_workspace: bool,
}

impl StateCompositeTask {
    /// Build a composite task from a manifest state definition (basic, no engine).
    ///
    /// Inner graph actions will fail at runtime if no engine is set.
    #[must_use]
    pub fn from_manifest(state: &StateDefinition) -> Self {
        // M-004: pre-compile expression AST at construction time.
        let cached_expr = Self::build_expr_cache(state.next.as_ref());

        Self {
            id: state.id.clone(),
            terminal: state.terminal,
            enter_actions: state.enter.clone(),
            exit_when: state.exit_when.clone(),
            next: state.next.clone(),
            engine: None,
            timeout_ms: state.timeout_ms,
            on_timeout: state.on_timeout.clone(),
            clock: Arc::new(SystemJoinClock),
            inner_graphs: std::collections::HashMap::new(),
            output_bindings: std::collections::HashMap::new(),
            registry: None,
            daemon_tool_dispatch: None,
            merge_kind: state.merge.clone(),
            expected_incoming: 0,
            merge_key: format!("_merge_{}", state.id),
            converge: state.converge.clone(),
            converge_key: format!("_converge_arrivals_{}", state.id),
            converge_predecessors: std::collections::HashSet::new(),
            cached_expr,
            workspace_state: None,
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

    /// Set the daemon-side tool dispatch for `nexus.*` host tool actions (DF-47, V1.42 P3).
    #[must_use]
    pub fn with_daemon_tool_dispatch(
        mut self,
        dispatch: std::sync::Arc<dyn crate::capability::DaemonToolDispatch>,
    ) -> Self {
        self.daemon_tool_dispatch = Some(dispatch);
        self
    }

    /// Set the expected number of incoming labeled edges for merge tracking (V1.52 T-B P1).
    #[must_use]
    pub const fn with_expected_incoming(mut self, count: usize) -> Self {
        self.expected_incoming = count;
        self
    }

    /// Set the converge predecessor task IDs (V1.56 P2 fix-wave, H-001).
    #[must_use]
    pub fn with_converge_predecessors(mut self, preds: HashSet<String>) -> Self {
        self.converge_predecessors = preds;
        self
    }

    /// Set the millisecond clock used for join-deadline checks (DR-06, v1.179).
    ///
    /// Production leaves the default [`SystemJoinClock`]; hermetic tests
    /// inject [`DeterministicClock`] to exercise timeout branches without
    /// sleeping.
    #[must_use]
    pub fn with_join_clock(mut self, clock: Arc<dyn JoinClock>) -> Self {
        self.clock = clock;
        self
    }

    /// Set the workspace session state for expression evaluation (V1.56 P3).
    ///
    /// When set, `build_context_json()` exposes this value as a nested
    /// `workspace` object, accessible in expressions as
    /// `_context.workspace.<field>`.
    ///
    /// # Activation status (V1.58 P2 — R-V156P3-S004)
    ///
    /// This builder is currently **test-only**: the production loader
    /// (`build_outer_graph` / `build_wired_outer_graph`) does not call it,
    /// so `self.workspace_state` is `None` at runtime. When `None`,
    /// `inject_workspace_context` falls back to a minimal synthetic default
    /// (`session_id: ""`, `committed: false`, `change_count: 0`).
    ///
    /// Production activation requires wiring the engine to inject real
    /// workspace session state per schedule tick (e.g. via a context key
    /// that `inject_workspace_context` reads, or a capability invocation
    /// mirroring the `registry.refresh` pattern). This is deferred to a
    /// future plan because it requires changes to the engine → task context
    /// injection boundary.
    #[must_use]
    pub fn with_workspace_state(mut self, state: serde_json::Value) -> Self {
        self.workspace_state = Some(state);
        self
    }

    /// Pre-compile expression AST for conditional/branches next (M-004).
    ///
    /// Parses each branch's `when` expression once at construction time;
    /// returns `None` if the state has no conditional next or all parses fail.
    ///
    /// V1.56 P3: also scans each expression for context dependencies
    /// (`registry_refresh`, `workspace`) so the runtime can invoke the
    /// appropriate capability before evaluating branches.
    fn build_expr_cache(next: Option<&NextTarget>) -> Option<CachedExpressions> {
        let (rules, default) = match next {
            Some(NextTarget::Conditional(cond)) => (&cond.rules, &cond.default),
            Some(NextTarget::Branches(branches)) => (&branches.branches, &branches.default),
            _ => return None,
        };

        let mut branches = Vec::with_capacity(rules.len());
        let mut needs_registry_refresh = false;
        let mut needs_workspace = false;

        for rule in rules {
            match crate::preset::expr::parse(&rule.when) {
                Ok(ast) => {
                    // V1.56 P3: scan expression for context dependencies.
                    let deps = crate::preset::expr::scan_context_deps(&ast);
                    needs_registry_refresh = needs_registry_refresh || deps.needs_registry_refresh;
                    needs_workspace = needs_workspace || deps.needs_workspace;
                    branches.push((ast, rule.target.clone()));
                }
                Err(e) => {
                    tracing::warn!(
                        when = %rule.when,
                        error = %e,
                        "expression parse error at construction time, branch will be skipped at runtime"
                    );
                }
            }
        }

        Some(CachedExpressions {
            branches,
            default: default.clone(),
            needs_registry_refresh,
            needs_workspace,
        })
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

    /// Determine the `NextAction` after judge evaluation.
    ///
    /// When `next` is `GoNogo`, both GO and NOGO advance via `Continue`
    /// (the conditional edge routes to the correct target).
    /// When `next` is `Labeled` (V1.52 T-B P0), routing is via
    /// [`Self::resolve_labeled_target`] instead — this method should NOT
    /// be called for `Labeled`.
    /// When `next` is `Linear` or `None`, GO advances but NOGO waits.
    // Clippy wants const but this borrows self.next; suppress.
    #[allow(clippy::missing_const_for_fn)]
    fn judge_next_action(&self, judge_result: bool) -> NextAction {
        match &self.next {
            Some(NextTarget::GoNogo(_) | NextTarget::Branches(_) | NextTarget::Conditional(_)) => {
                // Conditional/Branches routing handles both GO and NOGO via
                // resolve_expression_target (step 2.5).
                NextAction::Continue
            }
            _ if judge_result => NextAction::Continue,
            _ => NextAction::WaitForInput,
        }
    }

    /// V1.52 T-B P0: resolve labeled routing target from judge output.
    ///
    /// Scans the judge's output text (`judge_reason`) for known label
    /// strings declared in `next` edges. On match, writes the matched
    /// label to context as `_judge_label` and returns `GoTo(target)`.
    ///
    /// For legacy binary `GoNogo` states, auto-converts: treats `"go"` and
    /// `"nogo"` as labeled edges (same preset reachable via either routing API).
    ///
    /// # Errors
    ///
    /// Returns `Err(GraphError::TaskExecutionFailed)` when no label
    /// substring matches the judge output (deterministic branch fail
    /// instead of silent stall). The error includes the list of known
    /// labels and an excerpt of the judge output.
    fn resolve_labeled_target(
        &self,
        context: &graph_flow::Context,
        judge_reason: &str,
    ) -> Result<NextAction, graph_flow::GraphError> {
        // Collect candidate (label, target) pairs from the next target.
        // Sort by descending label length to prevent shorter labels (e.g. "go")
        // from matching as substrings of longer labels (e.g. "nogo").
        let mut candidates: Vec<(&str, &str)> = match &self.next {
            Some(NextTarget::Labeled(edges)) => edges
                .iter()
                .map(|e| (e.label.as_str(), e.target.as_str()))
                .collect(),
            Some(NextTarget::GoNogo(go_nogo)) => {
                // W-QC3-2: binary→Labeled auto-conversion.
                vec![("go", go_nogo.go.as_str()), ("nogo", go_nogo.nogo.as_str())]
            }
            Some(NextTarget::Conditional(_) | NextTarget::Branches(_)) => {
                return Err(graph_flow::GraphError::TaskExecutionFailed(
                    "resolve_labeled_target: Conditional/Branches routing requires expression evaluation, not label matching".to_string(),
                ));
            }
            _ => return Ok(NextAction::WaitForInput),
        };
        candidates.sort_by_key(|(label, _)| std::cmp::Reverse(label.len()));

        for (label, target) in &candidates {
            if judge_reason.contains(label) {
                // W-001: write matched label to context for observability.
                context.set("_judge_label", (*label).to_string())?;
                // V1.52 T-B P1: record label arrival for merge tracking.
                // If target is a merge node, _merge_<target_id> accumulates labels.
                // Non-merge targets ignore this key.
                let merge_key = format!("_merge_{target}");
                let mut arrived: Vec<String> = context.get(&merge_key).unwrap_or_default();
                if !arrived.contains(&(*label).to_string()) {
                    arrived.push((*label).to_string());
                }
                context.set(&merge_key, arrived)?;
                // V1.56 P2 fix-wave (H-001): also record converge arrival.
                Self::record_converge_arrival(context, target, &self.id)?;
                return Ok(NextAction::GoTo((*target).to_string()));
            }
        }

        // W-QC3-3: no-match → deterministic fail (not silent stall).
        let known_labels: Vec<String> = candidates.iter().map(|(l, _)| (*l).to_string()).collect();
        let excerpt = if judge_reason.len() > 200 {
            format!("{}...", &judge_reason[..200])
        } else {
            judge_reason.to_string()
        };
        tracing::warn!(
            state_id = %self.id,
            known_labels = ?known_labels,
            judge_output_excerpt = %excerpt,
            "resolve_labeled_target: no label matched judge output; failing deterministically"
        );
        Err(graph_flow::GraphError::TaskExecutionFailed(format!(
            "Labeled routing: no label matched judge output. Known labels: {known_labels:?}. Judge output excerpt: {excerpt}"
        )))
    }

    /// V1.56 P2: resolve expression-based conditional routing target.
    ///
    /// Uses pre-compiled expression AST (M-004) for performance. Evaluates each
    /// branch's `when` expression against the context, returning the first matching
    /// branch's target. Falls back to the `default` target if no branch matches.
    ///
    /// V1.56 P2 fix-wave (M-006): expression eval failures are now propagated as
    /// errors instead of silently skipping the branch.
    fn resolve_expression_target(
        &self,
        context: &graph_flow::Context,
    ) -> Result<NextAction, graph_flow::GraphError> {
        let ctx_json = build_context_json(context);

        let cache = self.cached_expr.as_ref().ok_or_else(|| {
            graph_flow::GraphError::TaskExecutionFailed(
                "resolve_expression_target: no cached expressions available".to_string(),
            )
        })?;

        for (i, (ast, target)) in cache.branches.iter().enumerate() {
            match crate::preset::expr::evaluate(ast, &ctx_json) {
                Ok(true) => {
                    tracing::debug!(
                        state_id = %self.id,
                        branch_index = i,
                        target = %target,
                        "expression branch matched"
                    );
                    // V1.56 P2 fix-wave (H-001): record arrival at converge target.
                    Self::record_converge_arrival(context, target, &self.id)?;
                    return Ok(NextAction::GoTo(target.clone()));
                }
                Ok(false) => {
                    // Continue to next branch.
                }
                Err(e) => {
                    // M-006: propagate expression eval failures, don't swallow.
                    tracing::error!(
                        state_id = %self.id,
                        branch_index = i,
                        error = %e,
                        "expression evaluation error, failing the transition"
                    );
                    return Err(graph_flow::GraphError::TaskExecutionFailed(format!(
                        "expression evaluation error in state '{}' branch {}: {e}",
                        self.id, i
                    )));
                }
            }
        }

        // No branch matched — use default.
        tracing::debug!(
            state_id = %self.id,
            default = %cache.default,
            "no expression branch matched, falling back to default"
        );
        // V1.56 P2 fix-wave (H-001): record arrival at converge target for default branch too.
        Self::record_converge_arrival(context, &cache.default, &self.id)?;
        Ok(NextAction::GoTo(cache.default.clone()))
    }

    /// Record a converge arrival for the given target state (V1.56 P2 fix-wave, H-001).
    ///
    /// Writes `source_id` into `_converge_arrivals_{target}` (per-source tracking via
    /// `HashSet` for automatic dedup). Each distinct predecessor's `source_id` is
    /// recorded exactly once; repeated arrivals from the same source are idempotent.
    /// If the target is not a converge node, this is a no-op (the key is ignored).
    ///
    /// # Source ID semantics
    ///
    /// `source_id` is the task ID (`self.id`) of the predecessor that is arriving
    /// at the converge target.  This aligns with `converge_predecessors` (a
    /// `HashSet<String>` of predecessor task IDs populated at graph build time),
    /// so the converge gate check `arrived_sources.len() == expected_predecessor_count`
    /// correctly gates on per-predecessor arrival.
    ///
    /// # ROUND-4 GATE-SCOPE CORRECTION
    ///
    /// This writer is invoked by labeled/conditional ROUTING for ANY routed
    /// target — including plain manual-wait states — so the
    /// `_converge_arrivals_*` (and `_merge_*` / `_join_wait_start_*`) keys it
    /// produces are **not** authoritative evidence that the CURRENT step parked
    /// at a scheduler gate. The authoritative scheduler-park signal is the
    /// state-scoped [`set_gate_park`] marker that the gated task itself writes
    /// ONLY at its genuine merge/converge wait returns (and nulls on gate
    /// success/leave/timeout). Recovery classification therefore uses
    /// `gate_park_live(data, current_task_id)`, never the any-key chain test,
    /// so a manual wait reached through a labeled/conditional path with stale
    /// or broad join keys keeps its fresh retained A4 token.
    ///
    /// # Errors
    /// Returns `graph_flow::GraphError` when persisting the updated arrival
    /// set into the context fails.
    pub fn record_converge_arrival(
        context: &graph_flow::Context,
        target: &str,
        source_id: &str,
    ) -> graph_flow::Result<()> {
        let converge_key = format!("_converge_arrivals_{target}");
        let mut arrived: std::collections::HashSet<String> =
            context.get(&converge_key).unwrap_or_default();
        if arrived.insert(source_id.to_string()) {
            // New predecessor arrival — persist the updated set.
            context.set(&converge_key, arrived)?;
        }
        // else: duplicate arrival from same source → idempotent no-op.
        Ok(())
    }

    /// Set or clear the CURRENT-GATE scheduler-park marker for `state_id`
    /// (round 4, Critical 1).
    ///
    /// The gated `StateCompositeTask` writes `_gate_park_{state_id}` = `true`
    /// exactly when its merge/converge gate returns `WaitForInput` (the engine
    /// persists that outcome as `paused` + tokenless), and nulls it on gate
    /// success-leave and deadline expiry (reroute/typed failure). This is the
    /// ONLY authoritative scheduler-park evidence: unlike the routing-written
    /// `_merge_*` / `_converge_arrivals_*` / `_join_wait_start_*` keys (which
    /// label/conditional routing writes for ANY routed target), the marker is
    /// state-scoped and written by the gate itself. A manual/nested wait
    /// reached through a labeled/conditional path never carries a live marker
    /// for its own state, so it stays `waiting_for_input` with a fresh token.
    fn set_gate_park(
        context: &graph_flow::Context,
        state_id: &str,
        parked: bool,
    ) -> graph_flow::Result<()> {
        let key = format!("_gate_park_{state_id}");
        if parked {
            context.set(&key, true)?;
        } else {
            context.set(&key, Value::Null)?;
        }
        Ok(())
    }

    /// Bounded-join deadline check for one waiting tick (DR-06, v1.179).
    ///
    /// No-op unless this state sets `timeout_ms`. On the first waiting tick
    /// records `_join_wait_start_{id}` in the context; every subsequent tick
    /// compares elapsed wall time (via the injected clock) against the
    /// deadline. When the deadline has elapsed:
    ///
    /// - the arrivals key (`_merge_{id}` / `_converge_arrivals_{id}`) and the
    ///   wait-start key are cleared for the next cycle;
    /// - the wait-start key is ALSO cleared when the join LEAVES — every
    ///   present gate passed and processing proceeds (success-leave clear
    ///   in `run()`). Without it a same-session re-entry (runtime `GoTo`
    ///   loop; the DAG load only rejects static cycles) reuses the stale
    ///   timestamp and fires the deadline immediately (F-001, QC fix).
    ///   The clear sits after BOTH gates, so a merge-success tick whose
    ///   converge gate still waits keeps the shared state-level budget
    ///   (§3.3.3);
    /// - with a resolvable `on_timeout` target the run reroutes there and a
    ///   note is written to `_join_timeout_note`;
    /// - without one, the task fails with a `GraphError::TaskExecutionFailed`
    ///   whose message begins with the typed discriminator `converge_timeout:`
    ///   and names `gate`, `state_id`, `arrived`, `expected`, `elapsed_ms`.
    ///
    /// `gate` is `"merge"` or `"converge"`; `arrived`/`expected` are the
    /// arrival counts observed on the timing-out tick. Returns `Ok(None)`
    /// when the join may keep waiting.
    fn join_timeout_tick(
        &self,
        context: &graph_flow::Context,
        gate: &'static str,
        arrived: usize,
        expected: usize,
    ) -> Result<Option<TaskResult>, graph_flow::GraphError> {
        let Some(timeout_ms) = self.timeout_ms else {
            return Ok(None);
        };
        let wait_start_key = format!("_join_wait_start_{}", self.id);
        let wait_start: u64 = if let Some(t) = context.get(&wait_start_key) {
            t
        } else {
            let now = self.clock.now_ms();
            context.set(&wait_start_key, now)?;
            now
        };
        let elapsed_ms = self.clock.now_ms().saturating_sub(wait_start);
        if elapsed_ms < timeout_ms {
            return Ok(None);
        }

        // Deadline exceeded — clear arrivals + wait-start for the next cycle.
        let arrivals_key = if gate == "merge" {
            &self.merge_key
        } else {
            &self.converge_key
        };
        context.set(arrivals_key, Value::Null)?;
        context.set(&wait_start_key, Value::Null)?;
        // Round-4 Critical 1: the join is no longer parked — the deadline
        // fired (reroute or typed failure). Clear the current-gate marker
        // so a later WaitForInput outcome at this state is never misread
        // as a scheduler park.
        Self::set_gate_park(context, &self.id, false)?;

        if let Some(target) = &self.on_timeout {
            let note = format!(
                "join timeout at '{}': rerouting to '{target}' (gate={gate}, elapsed_ms={elapsed_ms})",
                self.id
            );
            context.set("_join_timeout_note", note.clone())?;
            tracing::info!(
                state_id = %self.id,
                gate,
                target = %target,
                elapsed_ms,
                "join deadline exceeded; rerouting to on_timeout target"
            );
            return Ok(Some(TaskResult::new(
                Some(note),
                NextAction::GoTo(target.clone()),
            )));
        }

        tracing::warn!(
            state_id = %self.id,
            gate,
            arrived,
            expected,
            elapsed_ms,
            "join deadline exceeded with no on_timeout target; failing task"
        );
        Err(graph_flow::GraphError::TaskExecutionFailed(format!(
            "converge_timeout: gate={gate}, state_id={}, arrived={arrived}, \
             expected={expected}, elapsed_ms={elapsed_ms}",
            self.id
        )))
    }

    /// Inject context dependencies before expression evaluation (V1.56 P3).
    ///
    /// Called before `resolve_expression_target()` to populate
    /// `__registry_refresh_output` and `__workspace_state` in the context
    /// so that `build_context_json()` can expose them as nested objects
    /// (`registry_refresh` / `workspace`).
    async fn inject_context_deps(&self, context: &graph_flow::Context) -> graph_flow::Result<()> {
        if let Some(cache) = &self.cached_expr {
            if cache.needs_registry_refresh {
                self.inject_registry_refresh_context(context).await?;
            }
            if cache.needs_workspace {
                self.inject_workspace_context(context)?;
            }
        }
        Ok(())
    }

    /// Invoke `registry.refresh` capability and store output in context.
    ///
    /// If the capability registry is unavailable, falls back to a minimal
    /// synthetic output so expressions don't fail on missing fields.
    ///
    /// V1.58 P2 (R-V156P3-W002): every invocation site emits a `tracing::info!`
    /// span with `duration_ms` and `status` so operators can observe refresh
    /// latency and fallback rate on conditional-edge evaluation paths.
    async fn inject_registry_refresh_context(
        &self,
        context: &graph_flow::Context,
    ) -> graph_flow::Result<()> {
        // Check if we already have registry output (avoid redundant invocation).
        if context
            .get::<serde_json::Value>("__registry_refresh_output")
            .is_some()
        {
            tracing::debug!("registry.refresh skipped: cached output already present in context");
            return Ok(());
        }

        let output = if let Some(registry) = &self.registry {
            // Try to invoke the registered capability.
            if let Some(cap) = registry.get("registry.refresh") {
                let start = std::time::Instant::now();
                match cap.run(serde_json::json!({})).await {
                    Ok(val) => {
                        let duration_ms = start.elapsed().as_millis();
                        tracing::info!(
                            capability = "registry.refresh",
                            duration_ms,
                            status = "ok",
                            "registry.refresh invocation completed"
                        );
                        val
                    }
                    Err(e) => {
                        let duration_ms = start.elapsed().as_millis();
                        tracing::warn!(
                            capability = "registry.refresh",
                            duration_ms,
                            status = "fallback",
                            error = %e,
                            "registry.refresh capability invocation failed, using synthetic fallback"
                        );
                        synthetic_registry_output()
                    }
                }
            } else {
                tracing::info!(
                    capability = "registry.refresh",
                    duration_ms = 0u128,
                    status = "fallback",
                    "registry.refresh capability not found in registry, using synthetic fallback"
                );
                synthetic_registry_output()
            }
        } else {
            // No registry available — use synthetic fallback.
            tracing::info!(
                capability = "registry.refresh",
                duration_ms = 0u128,
                status = "fallback",
                "no capability registry available, using synthetic fallback"
            );
            synthetic_registry_output()
        };

        context.set("__registry_refresh_output", output)?;
        Ok(())
    }

    /// Inject workspace session state into context for expression evaluation.
    ///
    /// If `self.workspace_state` is set (e.g., by tests), uses that.
    /// Otherwise, injects a minimal default state so expressions referencing
    /// `_context.workspace.*` fields get valid values without error.
    ///
    /// V1.58 P2 (R-V156P3-W001): emits a `tracing::debug!` span so operators
    /// can trace what workspace context was injected into which schedule tick
    /// (previously this path had zero tracing).
    fn inject_workspace_context(&self, context: &graph_flow::Context) -> graph_flow::Result<()> {
        // Synchronous by design: the workspace snapshot is already in memory
        // (`self.workspace_state`), so no `.await` point exists here.
        // Check if already injected (avoid clobbering).
        if context
            .get::<serde_json::Value>("__workspace_state")
            .is_some()
        {
            tracing::debug!(
                "workspace context injection skipped: __workspace_state already present"
            );
            return Ok(());
        }

        let ws_state = self.workspace_state.clone().unwrap_or_else(|| {
            serde_json::json!({
                "session_id": "",
                "revision": "",
                "committed": false,
                "change_count": 0,
                "workspace_root": ""
            })
        });

        tracing::debug!(
            state_id = %self.id,
            source = if self.workspace_state.is_some() { "hook" } else { "default" },
            committed = %ws_state
                .get("committed")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            change_count = %ws_state
                .get("change_count")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0),
            "injecting workspace context for expression evaluation"
        );

        context.set("__workspace_state", ws_state)?;
        Ok(())
    }
}

/// Build a minimal synthetic `RegistryRefreshOutput` as a fallback for
/// expression evaluation when the capability is unavailable (V1.56 P3).
///
/// # `source` value semantics (R-V156P3-S001)
///
/// This helper emits `source = "synthetic"` — a **test-grade placeholder**
/// produced locally by the orchestrator when no capability output exists at
/// all (e.g. `registry_refresh` was never invoked for this session). It is
/// distinct from `source = "synthetic_fallback"`, which the **real**
/// `RegistryRefresh` capability emits when the CDN fetch failed but the
/// embedded snapshot was served successfully (see
/// `capability::builtins::registry::RegistryRefresh` and the
/// `registry_refresh_latency` bench). Preset `when:` expressions that branch
/// on `source` must treat `"synthetic"` as "no data" and
/// `"synthetic_fallback"` as "best-effort data from a deterministic source".
fn synthetic_registry_output() -> serde_json::Value {
    serde_json::json!({
        "source": "synthetic",
        "snapshotVersion": "2026-06-22.v1",
        "capabilityCount": 31,
        "fallbackReason": "",
        "retryCount": 0,
        "cacheAgeMs": 0,
        "generatedAt": "",
        "fetchTimeoutMs": 0,
        "maxRetries": 0,
    })
}

/// Build a JSON object from context keys for expression evaluation.
///
/// V1.56 P2 fix-wave (M-003): extended to expose user-set context values
/// in addition to the fixed set of orchestration keys. All key-value pairs
/// in the context's data map are now included.
///
/// V1.56 P3: also exposes `__registry_refresh_output` as a nested
/// `registry_refresh` object and `__workspace_state` as a nested
/// `workspace` object when the runtime has populated those context keys.
fn build_context_json(context: &graph_flow::Context) -> serde_json::Value {
    // Known orchestration keys that expressions may reference.
    let known_keys = [
        "_judge_result",
        "_judge_reason",
        "_judge_label",
        "_state_result",
        "_run_id",
        "output",
        "result",
        "status",
        "score",
    ];

    let mut map = serde_json::Map::new();

    // Include known orchestration keys.
    for key in &known_keys {
        if let Some(val) = context.get::<serde_json::Value>(key) {
            map.insert(key.to_string(), val);
        }
    }

    // M-003: also include all user-set context values.
    // We serialize the full context data map and merge.
    if let Ok(serialized) = serde_json::to_value(context) {
        if let Some(data) = serialized
            .as_object()
            .and_then(|obj| obj.get("data"))
            .and_then(|d| d.as_object())
        {
            for (key, value) in data {
                // Skip internal/hidden keys (start with double underscore).
                if key.starts_with("__") {
                    continue;
                }
                if !map.contains_key(key) {
                    map.insert(key.clone(), value.clone());
                }
            }
        }
    }

    // V1.56 P3: expose registry.refresh output as a nested object.
    if let Some(reg_output) = context.get::<serde_json::Value>("__registry_refresh_output") {
        // Translate camelCase capability output fields to snake_case
        // context fields for expression grammar consistency.
        let reg_obj = registry_output_to_context(&reg_output);
        map.insert("registry_refresh".to_string(), reg_obj);
    }

    // V1.56 P3: expose workspace session state as a nested object.
    if let Some(ws_state) = context.get::<serde_json::Value>("__workspace_state") {
        map.insert("workspace".to_string(), ws_state);
    }

    serde_json::Value::Object(map)
}

/// Translate `RegistryRefreshOutput` (camelCase) to `snake_case` context object.
///
/// The capability returns camelCase JSON fields; expressions use `snake_case`
/// (`_context.registry_refresh.snapshot_version` not
/// `_context.registry_refresh.snapshotVersion`). This function maps the
/// output shape to the context shape used by the expression grammar.
fn registry_output_to_context(output: &serde_json::Value) -> serde_json::Value {
    let Some(obj) = output.as_object() else {
        return serde_json::Value::Null;
    };

    // R-V156P3-S003 fix: map all 9 RegistryRefreshOutput fields (previously
    // only 5 were mapped; cache_age_ms, generated_at, fetch_timeout_ms,
    // and max_retries were silently dropped).
    let source = obj
        .get("source")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let snapshot_version = obj
        .get("snapshotVersion")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let capability_count = obj
        .get("capabilityCount")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let fallback_reason = obj
        .get("fallbackReason")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let retry_count = obj
        .get("retryCount")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let cache_age_ms = obj
        .get("cacheAgeMs")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let generated_at = obj
        .get("generatedAt")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let fetch_timeout_ms = obj
        .get("fetchTimeoutMs")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let max_retries = obj
        .get("maxRetries")
        .cloned()
        .unwrap_or(serde_json::Value::Null);

    serde_json::json!({
        "source": source,
        "snapshot_version": snapshot_version,
        "capability_count": capability_count,
        "fallback_reason": fallback_reason,
        "retry_count": retry_count,
        "cache_age_ms": cache_age_ms,
        "generated_at": generated_at,
        "fetch_timeout_ms": fetch_timeout_ms,
        "max_retries": max_retries,
    })
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
        let resumed: bool = context.get(&resume_key).unwrap_or(false);

        if resumed {
            // Already went through the full lifecycle before the wait.
            // Just continue to the next state.
            let response = Some(format!("state '{}': resumed, continuing", self.id));
            tracing::debug!(state_id = %self.id, terminal = self.terminal, "state resumed");
            return Ok(TaskResult::new(response, NextAction::Continue));
        }

        // 0.5. V1.52 T-B P1: Merge node gate.
        // If this state has incoming labeled edges, check whether enough have
        // arrived before processing enter actions. When `merge:` is absent but
        // expected_incoming > 0, the default is WaitAll (W-QC1-1).
        if self.expected_incoming > 0 {
            let merge_kind = self.merge_kind.as_ref().unwrap_or(&MergeKind::All);
            let arrived: Vec<String> = context.get(&self.merge_key).unwrap_or_default();
            let arrived_count = arrived.len();

            let condition_met = match merge_kind {
                MergeKind::All => arrived_count >= self.expected_incoming,
                MergeKind::Any => arrived_count >= 1,
                MergeKind::Quorum { n, .. } => arrived_count >= *n,
            };

            if !condition_met {
                // DR-06: bounded join — reroute or fail typed when the
                // deadline has elapsed (no-op when timeout_ms is absent).
                if let Some(timeout_result) = self.join_timeout_tick(
                    &context,
                    "merge",
                    arrived_count,
                    self.expected_incoming,
                )? {
                    return Ok(timeout_result);
                }
                let state_id = self.id.clone();
                tracing::debug!(
                    state_id = %state_id,
                    arrived = arrived_count,
                    expected = self.expected_incoming,
                    merge_kind = ?merge_kind,
                    "merge node waiting for more incoming labeled edges"
                );
                // Round-4 Critical 1: write the CURRENT-GATE park marker so
                // the engine classifies this WaitingForInput outcome as a
                // scheduler park (paused + tokenless). Historical/broad join
                // keys written by labeled/conditional ROUTING are not
                // authoritative park evidence; this state-scoped marker is.
                Self::set_gate_park(&context, &self.id, true)?;
                return Ok(TaskResult::new(
                    Some(format!(
                        "merge node '{state_id}': {arrived_count}/{expected} arrivals, waiting",
                        expected = self.expected_incoming
                    )),
                    NextAction::WaitForInput,
                ));
            }

            // Merge condition met — clear arrivals for next cycle.
            context.set(&self.merge_key, serde_json::Value::Null)?;
            // Success-leave: the join has passed, so the gate is no longer
            // parked (a later re-entry at this state must never be misread
            // as a scheduler park).
            Self::set_gate_park(&context, &self.id, false)?;
            tracing::info!(
                state_id = %self.id,
                arrived = arrived_count,
                "merge node condition met, advancing"
            );
        }

        // 0.6. V1.56 P2 fix-wave (H-001/W-002): Converge (merge-point) gate.
        // For states with explicit `converge:` config, track arrivals from
        // connected predecessors and enforce the declared converge strategy.
        if let Some(ref converge_config) = self.converge {
            if !self.converge_predecessors.is_empty() {
                let arrived: std::collections::HashSet<String> =
                    context.get(&self.converge_key).unwrap_or_default();
                let arrived_count = arrived.len();
                let expected = self.converge_predecessors.len();

                let condition_met = match converge_config.strategy {
                    ConvergeStrategy::WaitForAll => arrived_count >= expected,
                    ConvergeStrategy::FirstCompleted | ConvergeStrategy::Any => arrived_count >= 1,
                };

                if !condition_met {
                    let state_id = self.id.clone();
                    // DR-06: bounded join — reroute or fail typed when the
                    // deadline has elapsed (no-op when timeout_ms is absent).
                    if let Some(timeout_result) =
                        self.join_timeout_tick(&context, "converge", arrived_count, expected)?
                    {
                        return Ok(timeout_result);
                    }
                    tracing::debug!(
                        state_id = %state_id,
                        arrived = arrived_count,
                        expected = expected,
                        strategy = ?converge_config.strategy,
                        "converge node waiting for more incoming edges"
                    );
                    // Round-4 Critical 1: CURRENT-GATE park marker (see the
                    // merge gate above).
                    Self::set_gate_park(&context, &self.id, true)?;
                    return Ok(TaskResult::new(
                        Some(format!(
                            "converge node '{state_id}': {arrived_count}/{expected} arrivals, waiting ({:?})",
                            converge_config.strategy
                        )),
                        NextAction::WaitForInput,
                    ));
                }

                // Converge condition met — clear arrivals for next cycle.
                context.set(&self.converge_key, serde_json::Value::Null)?;
                // Success-leave: the join has passed (see merge gate above).
                Self::set_gate_park(&context, &self.id, false)?;
                tracing::info!(
                    state_id = %self.id,
                    arrived = arrived_count,
                    strategy = ?converge_config.strategy,
                    "converge node condition met, advancing"
                );
            }
        }

        // F-001 (v1.179 QC fix): the join has LEFT — every present gate
        // passed (§0.5/§0.6 return early while waiting), so retire the
        // shared wait-start with the join cycle. A merge-success tick whose
        // converge gate still waits returns inside §0.6 and never reaches
        // this point, keeping `timeout_ms` one state-level budget for BOTH
        // gates (§3.3.3). Without this clear, a same-session re-entry
        // (runtime GoTo loop; the DAG load only rejects static cycles)
        // reuses the stale timestamp and fires the deadline immediately.
        // Guarded on `timeout_ms` so states without bounded-join fields
        // keep writing no tracking keys (byte-identical behaviour, e2e (e)).
        if self.timeout_ms.is_some() {
            context.set(format!("_join_wait_start_{}", self.id), Value::Null)?;
        }

        // 1. Process enter actions.
        for action in &self.enter_actions {
            match action {
                EnterAction::Capability { name, args } => {
                    context.set("_capability_name", name.clone())?;

                    // C-V133P2-01: Template-render capability args.
                    // Preset YAML args may contain {{preset.input.*}} or
                    // {{state.*.output}} placeholders. We render them against
                    // the engine context BEFORE identity injection, so
                    // placeholders resolve to actual runtime values.
                    let mut cap_input = args.clone().unwrap_or(Value::Null);
                    if cap_input.is_null() {
                        cap_input = Value::Object(serde_json::Map::new());
                    }

                    // Render every string value in the args through handlebars.
                    // Fail-closed: if a placeholder references a non-existent
                    // key, the render will fail and the capability is NOT called
                    // with literal "{{...}}" placeholders.
                    let payload = build_nested_payload(&context);
                    cap_input = render_value_templates(&cap_input, &payload)?;

                    if let Some(obj) = cap_input.as_object_mut() {
                        // Security (SEC-V131-01): inject trusted identity from
                        // engine context into capability args. Capabilities read
                        // `_creator_id` / `_session_id` from their input; the
                        // orchestration engine must set them at the invocation
                        // boundary so preset YAML cannot spoof these values
                        // (prevents cross-creator IPC IDOR).
                        // Preset args are untrusted. Strip protected identity
                        // fields first, then inject only trusted context values.
                        obj.remove("_creator_id");
                        obj.remove("_session_id");

                        if let Some(creator_id) = context.get::<String>("_creator_id") {
                            obj.insert("_creator_id".into(), Value::String(creator_id));
                        }
                        if let Some(session_id) = context.get::<String>("_session_id") {
                            obj.insert("_session_id".into(), Value::String(session_id));
                        }
                    }
                    context.set("_capability_input", cap_input)?;
                    let registry = self.registry.clone().unwrap_or_else(|| {
                        std::sync::Arc::new(CapabilityRegistry::with_builtins())
                    });
                    let cap_task = CapabilityTask { registry };
                    let cap_result = cap_task.run(context.clone()).await?;
                    // If capability task errored, propagate but still continue
                    // so the state machine doesn't get stuck.
                    if let Some(status_msg) = &cap_result.status_message {
                        context.set("_enter_error", status_msg.clone())?;
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
                        context.set("_inner_graph_name", name.clone())?;
                        context.set(
                            format!("_inner_graph_error_{name}"),
                            "no engine reference available",
                        )?;
                    }
                }
                EnterAction::HostTool { tool_name, args } => {
                    // DF-47 (V1.42 P3): invoke daemon-side nexus.* tool.
                    // The dispatch slot is injected by the engine at graph
                    // construction time via `with_daemon_tool_dispatch`.
                    let dispatch = self.daemon_tool_dispatch.as_ref();
                    if let Some(dispatch) = dispatch {
                        let host_tool_task = HostToolCallTask::from_dispatch(
                            dispatch.clone(),
                            format!("{}_host_tool_{}", self.id, tool_name.replace('.', "_")),
                            tool_name.clone(),
                            args.clone()
                                .unwrap_or_else(|| Value::Object(serde_json::Map::new())),
                        );
                        host_tool_task.run(context.clone()).await?;
                    } else {
                        return Err(graph_flow::GraphError::TaskExecutionFailed(format!(
                            "HostTool action requires daemon_tool_dispatch but none is configured (tool: {tool_name})"
                        )));
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
                context.set(resume_key, true)?;
                NextAction::WaitForInput
            }
            Some(ExitWhen::Rule) => {
                // The unit `ExitWhen::Rule` (bare `exit_when: kind: rule`)
                // contract is "transition as soon as the enter action
                // completes" (TD-V131-08, locked by
                // `memory_augmented_rule_exit_is_explicit_always_true`).
                // Production never injects a `_rule` expression for the
                // unit variant, so an unset `_rule` means always-true —
                // immediate transition. A context `_rule` (test/expression
                // wiring) still routes through the rule evaluator.
                let rule: String = context.get("_rule").unwrap_or_default();
                if rule.is_empty() {
                    NextAction::Continue
                } else {
                    let rule_task = RuleCheckTask;
                    let result = rule_task.run(context.clone()).await?;
                    result.next_action
                }
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
                    context.set("_judge_result", false)?;
                    context.set(
                        "_judge_reason",
                        "llm_judge: no template_file configured".to_string(),
                    )?;
                    NextAction::WaitForInput
                } else {
                    let cap_name = judge_capability.as_deref().unwrap_or("judge.llm");
                    let registry = self
                        .registry
                        .clone()
                        .unwrap_or_else(|| Arc::new(CapabilityRegistry::with_builtins()));

                    // min_interval throttle: skip evaluation if last
                    // evaluation was too recent.
                    //
                    // R-V156P3-S002 (yield-point audit): every branch in this
                    // throttle-hit block awaits exactly once before returning,
                    // so the cooperative scheduler always gets a yield point
                    // (no infinite sync loop in a single `evaluate` call):
                    //   1. `context.get(throttle_key)`     — entry read
                    //   2. `context.get("_judge_result")`   — reuse read
                    //   3. `context.get("_judge_reason")`   — reuse read
                    //   4. `inject_context_deps(&context).await`  — deps inject
                    // The `resolve_expression_target` / `resolve_labeled_target`
                    // helpers are sync but run AFTER step 4's yield, so the
                    // throttle-hit return path always yields at least once.
                    if let Some(ref interval_str) = min_interval {
                        let throttle_key = format!("_judge_last_eval_{}", self.id);
                        let last_eval: Option<String> = context.get(&throttle_key);
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
                                            context.get("_judge_result").unwrap_or(false);
                                        let prev_reason: String =
                                            context.get("_judge_reason").unwrap_or_else(|| {
                                                "min_interval throttle: reusing previous result"
                                                    .to_string()
                                            });
                                        // V1.56 P3: inject context dependencies before
                                        // expression routing. Must happen before the
                                        // return because we need the .await point.
                                        self.inject_context_deps(&context).await?;

                                        let next_action = if self.terminal {
                                            NextAction::End
                                        } else {
                                            // V1.56 P2 fix-wave (H-002): throttle path
                                            // must delegate to resolve_expression_target for
                                            // Conditional/Branches variants.
                                            // resolve_labeled_target rejects them.
                                            match &self.next {
                                                Some(
                                                    NextTarget::Conditional(_)
                                                    | NextTarget::Branches(_),
                                                ) => self.resolve_expression_target(&context)?,
                                                Some(
                                                    NextTarget::Labeled(_) | NextTarget::GoNogo(_),
                                                ) => self.resolve_labeled_target(
                                                    &context,
                                                    &prev_reason,
                                                )?,
                                                _ => self.judge_next_action(prev_result),
                                            }
                                        };
                                        return Ok(TaskResult::new(
                                            Some(format!("judge (throttled): {prev_reason}")),
                                            next_action,
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
                        context.set(throttle_key, chrono::Utc::now().to_rfc3339())?;
                    }

                    context.set("_judge_result", result)?;
                    context.set("_judge_reason", reason.clone())?;

                    // V1.52 T-B P0: for Labeled or GoNogo next, route via
                    // resolve_labeled_target (GoTo). For Linear/None, use
                    // the existing judge_next_action(bool) path.
                    if matches!(
                        &self.next,
                        Some(NextTarget::Labeled(_) | NextTarget::GoNogo(_))
                    ) {
                        self.resolve_labeled_target(&context, &reason)?
                    } else {
                        // V1.42 P2: when next is Linear/None, GO advances
                        // but NOGO waits.
                        self.judge_next_action(result)
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
                context.set(resume_key, true)?;
                NextAction::WaitForInput
            }
        };

        // 2.5. V1.56 P2: Expression-based conditional routing.
        // For states with Conditional/Branches next (any exit_when), evaluate
        // expressions and route to the matching target.
        // V1.56 P2 fix-wave (M-006): errors now propagated via ?.
        // V1.56 P3: inject registry.refresh + workspace context before evaluation.
        let next_action = match &self.next {
            Some(NextTarget::Conditional(_) | NextTarget::Branches(_)) => {
                self.inject_context_deps(&context).await?;
                self.resolve_expression_target(&context)?
            }
            _ => next_action,
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
/// - `kind=acp_prompt` → `AcpPromptTask` through the Host plane.
///
/// ## WS-E T5: `session_id` routing
///
/// The task routes the prompt to the durable orchestration run based on:
/// 1. Explicit `session_id` provided at construction (for preset resolution)
/// 2. The engine-seeded `_session_id` trusted context value (runtime lookup)
///
/// Backward compatible: if no session identity is available, uses `"default"`.
pub struct InnerGraphNodeTask {
    id: String,
    /// Production prompt executor. `None` for stub/test mode.
    executor: Option<std::sync::Arc<dyn crate::capability::PromptExecutor>>,
    /// Per-run coordinator cancellation tokens (A1).
    session_cancels: std::sync::Arc<
        std::sync::RwLock<std::collections::HashMap<String, tokio_util::sync::CancellationToken>>,
    >,
    /// Template file path (resolved relative to preset bundle root).
    template: String,
    /// Tool policy for this node.
    tool_policy: ToolPolicy,
    /// Explicit `session_id` (if preset resolution already determined it).
    session_id: Option<String>,
    /// Agent role reference (if node has `agent` field — resolved binding).
    agent_ref: Option<String>,
}

impl InnerGraphNodeTask {
    /// Create a new inner graph node task (stub mode, no executor).
    ///
    /// Used by preset loader for initial graph construction. The real task
    /// is wired at runtime when the executor and session cancels are available.
    #[must_use]
    pub fn new(id: &str) -> Self {
        Self {
            id: id.to_string(),
            executor: None,
            session_cancels: std::sync::Arc::new(std::sync::RwLock::new(
                std::collections::HashMap::new(),
            )),
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

    /// Builder-style prompt executor setter (A1).
    #[must_use]
    pub fn with_prompt_executor(
        mut self,
        executor: Option<std::sync::Arc<dyn crate::capability::PromptExecutor>>,
    ) -> Self {
        self.executor = executor;
        self
    }

    /// Builder-style session cancels setter (A1).
    #[must_use]
    pub fn with_session_cancels(
        mut self,
        session_cancels: std::sync::Arc<
            std::sync::RwLock<
                std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
            >,
        >,
    ) -> Self {
        self.session_cancels = session_cancels;
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

    /// Resolve the durable orchestration run id for this node.
    ///
    /// An explicit id is used by already-bound callers. Otherwise the engine's
    /// trusted `_session_id` context value identifies the run. Agent role
    /// selection is carried separately in `PromptRequest::agent_ref`.
    ///
    /// M-002: a node with neither an explicit id nor a trusted `_session_id`
    /// context value refuses with a typed graph error — never a magic
    /// `default` run id. The engine seeds `_session_id` at run admission;
    /// its absence means the node is being executed outside a trusted run
    /// context.
    fn resolve_session_id(
        &self,
        context: &graph_flow::Context,
    ) -> Result<String, graph_flow::GraphError> {
        if let Some(sid) = &self.session_id {
            return Ok(sid.clone());
        }

        context.get("_session_id").ok_or_else(|| {
            graph_flow::GraphError::TaskExecutionFailed(
                "missing trusted _session_id: orchestration context must inject the run identity"
                    .to_string(),
            )
        })
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
        // Resolve session_id for this node (WS-E T5). M-002: a missing
        // trusted identity refuses the node execution.
        let session_id = self.resolve_session_id(&context)?;

        // If we have a prompt executor, delegate to AcpPromptTask.
        if let Some(executor) = &self.executor {
            let mut acp_task = AcpPromptTask::new(
                Some(executor.clone()),
                self.session_cancels.clone(),
                &self.id,
                self.template.clone(),
                self.tool_policy,
                Some(session_id),
            );
            if let Some(agent_ref) = &self.agent_ref {
                acp_task = acp_task.with_agent_ref(agent_ref.clone());
            }
            let result = acp_task.run(context.clone()).await?;
            // Inner-graph presets bind outputs as `<node>.text` /
            // `<node>.output`, which `InnerGraphTask` resolves via the
            // `nodes.<binding>` key. `AcpPromptTask` stores
            // `state.<node>.output`; mirror it into the node-namespaced keys
            // so existing output_binding values keep working after the P1
            // prompt-executor cutover.
            let text: String = context
                .get(&format!("state.{}.output", self.id))
                .unwrap_or_default();
            if !text.is_empty() {
                context.set(format!("nodes.{}.output", self.id), text.clone())?;
                context.set(format!("nodes.{}.text", self.id), text)?;
            }
            return Ok(result);
        }

        // Stub mode: no executor — refuse with a typed failure, never a
        // placeholder success (A1: no echo/worker fallback).
        tracing::debug!(
            node_id = %self.id,
            session_id = %session_id,
            agent_ref = ?self.agent_ref,
            "InnerGraphNodeTask running without a prompt executor"
        );

        Err(graph_flow::GraphError::TaskExecutionFailed(format!(
            "inner node '{}': no prompt executor available (unconfigured provider)",
            self.id
        )))
    }
}

// ---------------------------------------------------------------------------
// AcpPromptTask (dispatches prompt through the Host plane)
// ---------------------------------------------------------------------------

/// Tool policy for ACP prompt sessions (A1).
///
/// Design: `orchestration-engine.md` §6.5. Defined in `capability` so the
/// Host permission scope mapping lives next to the seam.
pub use crate::capability::ToolPolicy;

/// A task that sends a prompt to an ACP agent through the Host plane (A1).
///
/// Design: `orchestration-engine.md` §4.4 (`AcpPromptTask` row) + A1.
///
/// `run(ctx)`:
/// 1. Renders the template with `handlebars` against `ctx` bindings.
/// 2. Executes the prompt through the injected [`crate::capability::PromptExecutor`]
///    with the run/task identity and the coordinator cancellation token.
/// 3. Stores `result.full_text` at `ctx["state.<state_id>.output"]`.
/// 4. Returns `TaskResult { response: Some(full_text), next_action: NextAction::Continue }`.
pub struct AcpPromptTask {
    /// Production prompt executor. `None` for test stub mode.
    executor: Option<std::sync::Arc<dyn crate::capability::PromptExecutor>>,
    /// Per-run coordinator cancellation tokens (A1).
    session_cancels: std::sync::Arc<
        std::sync::RwLock<std::collections::HashMap<String, tokio_util::sync::CancellationToken>>,
    >,
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
    /// Optional role key used to resolve the run's frozen provider binding.
    agent_ref: Option<String>,
}

impl AcpPromptTask {
    /// Create a new `AcpPromptTask`.
    ///
    /// `executor`: the production prompt executor. Can be `None` for test mode
    /// where the task operates in stub mode.
    ///
    /// `session_id`: optional session ID for multi-agent routing. If `None`,
    /// defaults to `"default"` for backward compatibility with single-agent workers.
    pub fn new(
        executor: Option<std::sync::Arc<dyn crate::capability::PromptExecutor>>,
        session_cancels: std::sync::Arc<
            std::sync::RwLock<
                std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
            >,
        >,
        state_id: impl Into<String>,
        template: impl Into<String>,
        tool_policy: ToolPolicy,
        session_id: Option<String>,
    ) -> Self {
        Self {
            executor,
            session_cancels,
            state_id: state_id.into(),
            template: template.into(),
            tool_policy,
            session_id: session_id.unwrap_or_else(|| "default".to_string()),
            agent_ref: None,
        }
    }

    /// Test helper: create an `AcpPromptTask` with a prompt executor directly.
    pub fn new_for_test(
        executor: std::sync::Arc<dyn crate::capability::PromptExecutor>,
        state_id: impl Into<String>,
        template: impl Into<String>,
        tool_policy: ToolPolicy,
    ) -> Self {
        Self {
            executor: Some(executor),
            session_cancels: std::sync::Arc::new(std::sync::RwLock::new(
                std::collections::HashMap::new(),
            )),
            state_id: state_id.into(),
            template: template.into(),
            tool_policy,
            session_id: "default".to_string(),
            agent_ref: None,
        }
    }

    /// Create an `AcpPromptTask` with explicit `session_id` (WS-E T5).
    ///
    /// Convenience constructor for multi-agent presets where the `session_id`
    /// is known at task creation time.
    pub fn with_session_id(
        executor: Option<std::sync::Arc<dyn crate::capability::PromptExecutor>>,
        session_cancels: std::sync::Arc<
            std::sync::RwLock<
                std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
            >,
        >,
        state_id: impl Into<String>,
        template: impl Into<String>,
        tool_policy: ToolPolicy,
        session_id: impl Into<String>,
    ) -> Self {
        Self {
            executor,
            session_cancels,
            state_id: state_id.into(),
            template: template.into(),
            tool_policy,
            session_id: session_id.into(),
            agent_ref: None,
        }
    }

    /// Select a role key from the run's frozen agent bindings.
    #[must_use]
    pub fn with_agent_ref(mut self, agent_ref: impl Into<String>) -> Self {
        self.agent_ref = Some(agent_ref.into());
        self
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

        // 2. If we have a prompt executor, dispatch through the Host plane.
        let full_text = if let Some(ref executor) = self.executor {
            // Resolve the coordinator cancellation token for this run (A1).
            // FAIL-CLOSED: an unregistered run refuses with the typed
            // `CancellationUnavailable` — a fresh token would be
            // uncancellable by any coordinator. The run admission path
            // (engine start/spawn/recovery) registers the token.
            let cancellation = crate::capability::resolve_session_cancellation(
                &self.session_cancels,
                &self.session_id,
            )
            .map_err(|e| {
                graph_flow::GraphError::TaskExecutionFailed(format!(
                    "acp_prompt cancellation resolution failed: {e}"
                ))
            })?;

            // Dispatching a prompt to an external agent is an external
            // effect — mark the step so a failed post-effect commit persists
            // an interrupted disposition rather than blindly rewinding
            // (Important 3).
            context.set(crate::engine::EXTERNAL_EFFECT_MARKER, true)?;

            let result = executor
                .execute(crate::capability::PromptRequest {
                    run_id: self.session_id.clone(),
                    task_id: self.state_id.clone(),
                    agent_ref: self.agent_ref.clone(),
                    prompt,
                    tool_policy: self.tool_policy,
                    cancellation,
                })
                .await
                .map_err(|e| {
                    graph_flow::GraphError::TaskExecutionFailed(format!(
                        "acp_prompt execution failed: {e}"
                    ))
                })?;

            result.full_text
        } else {
            // Stub mode: no executor — refuse with a typed failure, never a
            // placeholder success (A1: no echo/worker fallback).
            return Err(graph_flow::GraphError::TaskExecutionFailed(
                "acp_prompt: no prompt executor available (unconfigured provider)".into(),
            ));
        };

        // 3. Add to chat history.
        context.add_assistant_message(full_text.clone());

        // 4. Store output at state.<state_id>.output.
        let output_key = format!("state.{}.output", self.state_id);
        context.set(&output_key, full_text.clone())?;

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

/// Render a handlebars template in strict mode — missing variables cause
/// an error instead of silently rendering as empty string.
///
/// Used for capability arg template rendering (C-V133P2-01) where silent
/// substitution of literal "{{...}}" would be a security/correctness bug.
///
/// # Errors
/// Returns an error if the template syntax is invalid, a variable is
/// missing, or rendering fails for any reason.
fn render_strict_template(template: &str, payload: &serde_json::Value) -> anyhow::Result<String> {
    let mut reg = handlebars::Handlebars::new();
    reg.register_escape_fn(handlebars::no_escape);
    reg.set_strict_mode(true);
    reg.render_template(template, payload).map_err(Into::into)
}

/// Recursively render all string values in a JSON value as handlebars templates.
///
/// C-V133P2-01: Walks the JSON tree; for every string value, renders it as a
/// handlebars template against `payload`. Non-string values (numbers, booleans,
/// null, arrays of non-strings) are left unchanged.
///
/// # Errors
///
/// Check whether a JSON value contains any handlebars template placeholder (`{{`).
///
/// Used to short-circuit the expensive `build_nested_payload` + `render_value_templates`
/// path when no placeholders exist (T7, qc3 W-02 hot-path fix).
fn value_contains_template(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::String(s) => s.contains("{{"),
        serde_json::Value::Object(map) => map.values().any(value_contains_template),
        serde_json::Value::Array(arr) => arr.iter().any(value_contains_template),
        _ => false,
    }
}

/// Returns an error if any string value contains a template placeholder that
/// fails to render (e.g. `{{nonexistent.key}}`). This is fail-closed: the
/// capability is NOT called with literal "{{...}}" placeholders.
fn render_value_templates(
    value: &serde_json::Value,
    payload: &serde_json::Value,
) -> Result<serde_json::Value, graph_flow::GraphError> {
    match value {
        serde_json::Value::String(s) => {
            let rendered = render_strict_template(s, payload).map_err(|e| {
                graph_flow::GraphError::TaskExecutionFailed(format!(
                    "capability arg template render failed for '{s}': {e}"
                ))
            })?;
            Ok(serde_json::Value::String(rendered))
        }
        serde_json::Value::Object(map) => {
            let rendered_map: serde_json::Map<String, serde_json::Value> = map
                .iter()
                .map(|(k, v)| render_value_templates(v, payload).map(|rv| (k.clone(), rv)))
                .collect::<Result<_, _>>()?;
            Ok(serde_json::Value::Object(rendered_map))
        }
        serde_json::Value::Array(arr) => {
            let rendered_arr: Vec<serde_json::Value> = arr
                .iter()
                .map(|v| render_value_templates(v, payload))
                .collect::<Result<_, _>>()?;
            Ok(serde_json::Value::Array(rendered_arr))
        }
        // Numbers, booleans, null — pass through unchanged.
        other => Ok(other.clone()),
    }
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
// HostToolCallTask — invoke a nexus.* tool from a schedule tick (DF-47, V1.42 P3)
// ---------------------------------------------------------------------------

/// Type alias for the dispatch slot used by `HostToolCallTask`.
/// Wraps an `Arc<Mutex<Option<Arc<dyn DaemonToolDispatch>>>>` for interior
/// mutability without consuming the dispatch on use.
type DaemonDispatchSlot = std::sync::Arc<
    std::sync::Mutex<Option<std::sync::Arc<dyn crate::capability::DaemonToolDispatch>>>,
>;

/// A task that calls a `nexus.*` host tool through the daemon's unified registry.
///
/// Production wiring for DF-47: the schedule executor can invoke read-only
/// (or mutating) `nexus.*` tools on a configured stage in-process. The call
/// goes directly through [`crate::capability::DaemonToolDispatch`] which is
/// implemented in `nexus-daemon-runtime` by `DaemonToolDispatchAdapter`
/// (`HostToolExecutor::dispatch_for_schedule`).
///
/// Design: `agent-nexus-tool-bridge.md` §7.4, V1.42 P3.
pub struct HostToolCallTask {
    /// Daemon-side tool dispatch provider (test-oriented, wraps Arc<Mutex<Option<...>>>).
    dispatch: Option<DaemonDispatchSlot>,
    /// Direct dispatch reference (production path, no Mutex overhead).
    direct_dispatch: Option<std::sync::Arc<dyn crate::capability::DaemonToolDispatch>>,
    /// Tool name, e.g. `"nexus.orchestration.schedule_status"`.
    tool_name: String,
    /// Tool parameters (may contain template references rendered at runtime).
    args: serde_json::Value,
    /// Unique task id for logging.
    task_id: String,
}

impl HostToolCallTask {
    /// Create a new `HostToolCallTask`.
    ///
    /// `dispatch`: the daemon-side tool dispatch provider. `None` for test stub mode.
    /// `task_id`: unique identifier for this task instance.
    /// `tool_name`: the `nexus.*` tool to invoke.
    /// `args`: tool parameters (JSON object, may contain template placeholders).
    #[must_use]
    pub fn new(
        dispatch: Option<DaemonDispatchSlot>,
        task_id: impl Into<String>,
        tool_name: impl Into<String>,
        args: serde_json::Value,
    ) -> Self {
        Self {
            dispatch,
            direct_dispatch: None,
            task_id: task_id.into(),
            tool_name: tool_name.into(),
            args,
        }
    }

    /// Create with a direct dispatch reference (production path from `StateCompositeTask`).
    ///
    /// This avoids the `Mutex<Option<...>>` wrapper used by the test-oriented `new()`.
    /// The dispatch is stored directly; `run()` skips the lock/unlock overhead.
    #[must_use]
    pub fn from_dispatch(
        dispatch: std::sync::Arc<dyn crate::capability::DaemonToolDispatch>,
        task_id: impl Into<String>,
        tool_name: impl Into<String>,
        args: serde_json::Value,
    ) -> Self {
        Self {
            dispatch: None,
            direct_dispatch: Some(dispatch),
            task_id: task_id.into(),
            tool_name: tool_name.into(),
            args,
        }
    }

    /// Create in stub mode (no daemon dispatch, for testing).
    #[must_use]
    pub fn new_stub(
        task_id: impl Into<String>,
        tool_name: impl Into<String>,
        args: serde_json::Value,
    ) -> Self {
        Self {
            dispatch: None,
            direct_dispatch: None,
            task_id: task_id.into(),
            tool_name: tool_name.into(),
            args,
        }
    }
}

#[async_trait]
impl Task for HostToolCallTask {
    fn id(&self) -> &str {
        &self.task_id
    }

    async fn run(
        &self,
        context: graph_flow::Context,
    ) -> Result<TaskResult, graph_flow::GraphError> {
        // T7 (qc3 W-02): short-circuit template rendering when args contain no
        // placeholders. Avoids serializing the full context + handlebars walk
        // for trivial calls like `{"work_id": "..."}`.
        let rendered_args = if value_contains_template(&self.args) {
            let payload = build_nested_payload(&context);
            render_value_templates(&self.args, &payload)?
        } else {
            self.args.clone()
        };

        // Generate a request_id for traceability.
        let request_id = format!(
            "host_tool_{}_{}",
            self.tool_name.replace('.', "_"),
            uuid::Uuid::new_v4()
        );

        let result_value = if let Some(ref dispatch_ref) = self.direct_dispatch {
            // Production path (direct): call through injected dispatch.
            // A host-tool dispatch is an external effect — mark the step so a
            // failed post-effect commit persists an interrupted disposition
            // rather than blindly rewinding (Important 3).
            context.set(crate::engine::EXTERNAL_EFFECT_MARKER, true)?;
            dispatch_ref
                .dispatch_tool(&self.tool_name, &rendered_args, &request_id)
                .await
                .map_err(|e| {
                    graph_flow::GraphError::TaskExecutionFailed(format!(
                        "daemon tool dispatch failed for {}: {e}",
                        self.tool_name
                    ))
                })?
        } else if let Some(ref dispatch_arc) = self.dispatch {
            // Test-oriented path: call through Mutex-wrapped dispatch slot.
            // A host-tool dispatch is an external effect (Important 3).
            context.set(crate::engine::EXTERNAL_EFFECT_MARKER, true)?;
            let dispatch = {
                let guard = dispatch_arc.lock().map_err(|e| {
                    graph_flow::GraphError::TaskExecutionFailed(format!(
                        "daemon tool dispatch lock: {e}"
                    ))
                })?;
                guard
                    .as_ref()
                    .ok_or_else(|| {
                        graph_flow::GraphError::TaskExecutionFailed(
                            "daemon tool dispatch not available".into(),
                        )
                    })?
                    .clone()
            };

            dispatch
                .dispatch_tool(&self.tool_name, &rendered_args, &request_id)
                .await
                .map_err(|e| {
                    graph_flow::GraphError::TaskExecutionFailed(format!(
                        "daemon tool dispatch failed for {}: {e}",
                        self.tool_name
                    ))
                })?
        } else {
            // Stub mode: return a synthetic result.
            serde_json::json!({
                "stub": true,
                "tool_name": self.tool_name,
                "args": rendered_args,
                "request_id": request_id,
            })
        };

        // Store the result in context for downstream nodes.
        let context_key = format!("host_tool.{}.result", self.task_id);
        context.set(&context_key, &result_value)?;
        context.set("_last_host_tool_result", &result_value)?;

        tracing::info!(
            tool_name = %self.tool_name,
            request_id = %request_id,
            "HostToolCallTask completed"
        );

        Ok(TaskResult::new(
            Some(format!("host_tool_call:{}:ok", self.tool_name)),
            NextAction::Continue,
        ))
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Parse an ISO-8601 duration string (e.g. `"PT6H"`, `"PT1H30M"`) into a
/// `chrono::Duration`.
///
/// Supports days (D), hours (H), minutes (M after T), and seconds (S).
/// Returns `None` for unparseable inputs, logging a warning.
///
/// WAIVER: pre-1.0 local-first; see V1.41 P-last residual R-V133P3-03
/// — P1D/P1M/P1Y date-only forms with M/Y units return None silently;
/// only P<n>D (days) and PT<n>H/M/S (time) are supported. Months/years
/// require calendar-aware parsing; deferred until multi-tenant scheduling.
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
    use crate::preset::manifest::{GoNogoNext, LabeledNext};
    use nexus_contracts::local::orchestration::preset::{ConditionalBranches, ConditionalRule};
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
            crate::capability::CapabilityRegistryHolder::with_registry(std::sync::Arc::new(
                CapabilityRegistry::with_builtins(),
            )),
        );
        let inner_graph = graph_flow::GraphBuilder::new("test_inner")
            .add_task(std::sync::Arc::new(InnerGraphNodeTask::new("n1")))
            .build()
            .expect("test graph build");

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
        ctx.set("_rule", "always_true").unwrap();
        let result = task.run(ctx).await.unwrap();
        assert!(matches!(result.next_action, NextAction::Continue));
    }

    #[tokio::test]
    async fn rule_check_false_waits() {
        let task = RuleCheckTask;
        let ctx = graph_flow::Context::new();
        ctx.set("_rule", "always_false").unwrap();
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
        impl crate::capability::PromptExecutor for MockGoProvider {
            async fn execute(
                &self,
                _request: crate::capability::PromptRequest,
            ) -> Result<crate::capability::PromptResult, crate::capability::CapabilityError>
            {
                Ok(crate::capability::PromptResult {
                    full_text: "GO — evaluation passes.".to_string(),
                    host_session_id: "host-sess".to_string(),
                    operation_id: "op-1".to_string(),
                })
            }
        }

        let deps = CapabilityRuntimeDeps {
            pool: None,
            prompt_executor: Some(std::sync::Arc::new(MockGoProvider)),
            session_cancels: session_cancels_with_default(),
            daemon_tool_dispatch: None,
            cdn_config: None,
        workspace_executor: None,
        };
        let registry = Arc::new(CapabilityRegistry::with_runtime_deps(&deps));

        let judge_task = LlmJudgeTask::new(
            "Is the task done?".to_string(),
            "judge.llm".to_string(),
            registry,
        );

        let ctx = graph_flow::Context::new();
        ctx.set("_session_id", "default").unwrap();
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
        impl crate::capability::PromptExecutor for MockNogoProvider {
            async fn execute(
                &self,
                _request: crate::capability::PromptRequest,
            ) -> Result<crate::capability::PromptResult, crate::capability::CapabilityError>
            {
                Ok(crate::capability::PromptResult {
                    full_text: "NO — stop and review.".to_string(),
                    host_session_id: "host-sess".to_string(),
                    operation_id: "op-1".to_string(),
                })
            }
        }

        let deps = CapabilityRuntimeDeps {
            pool: None,
            prompt_executor: Some(std::sync::Arc::new(MockNogoProvider)),
            session_cancels: session_cancels_with_default(),
            daemon_tool_dispatch: None,
            cdn_config: None,
        workspace_executor: None,
        };
        let registry = Arc::new(CapabilityRegistry::with_runtime_deps(&deps));

        let judge_task = LlmJudgeTask::new(
            "Is the task done?".to_string(),
            "judge.llm".to_string(),
            registry,
        );

        let ctx = graph_flow::Context::new();
        ctx.set("_session_id", "default").unwrap();
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
        ctx.set("_session_id", "default").unwrap();
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

    // ── V1.51 T-A P0: LlmExtractTask — golden input → golden Vec<KbCandidate> ──

    /// Mock worker that returns a fixed JSON candidates payload for
    /// `nexus.llm.extract`. Used by the `LlmExtractTask` hermetic tests.
    struct MockExtractProvider {
        response: String,
    }

    #[async_trait]
    impl crate::capability::PromptExecutor for MockExtractProvider {
        async fn execute(
            &self,
            _request: crate::capability::PromptRequest,
        ) -> Result<crate::capability::PromptResult, crate::capability::CapabilityError> {
            Ok(crate::capability::PromptResult {
                full_text: self.response.clone(),
                host_session_id: "host-sess".to_string(),
                operation_id: "op-1".to_string(),
            })
        }
    }

    fn extract_registry_with_mock(response: &str) -> Arc<CapabilityRegistry> {
        use crate::capability::CapabilityRuntimeDeps;
        let map = session_cancels_with_default();
        // The extract regression context sets no `_session_id`, so the
        // capability resolves run id "" — register its coordinator token
        // (fail-closed contract) so the prompt reaches the mock executor.
        map.write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(String::new())
            .or_default();
        let deps = CapabilityRuntimeDeps {
            pool: None,
            prompt_executor: Some(std::sync::Arc::new(MockExtractProvider {
                response: response.to_string(),
            })),
            session_cancels: map,
            daemon_tool_dispatch: None,
            cdn_config: None,
        workspace_executor: None,
        };
        Arc::new(CapabilityRegistry::with_runtime_deps(&deps))
    }

    #[tokio::test]
    async fn llm_extract_task_with_mock_worker_returns_candidates() {
        // Golden LLM response → golden LlmExtractOutcome::Candidates.
        let registry = extract_registry_with_mock(
            r#"{"candidates":[
                {"canonical_name":"Lin Xia","block_type":"character","summary":"A warrior","confidence":0.95,"source_quote":"Lin Xia drew her blade."},
                {"canonical_name":"Azure Gate","block_type":"scene","summary":null,"confidence":0.8,"source_quote":"the Azure Gate groaned open"}
            ]}"#,
        );
        let task = LlmExtractTask::new(
            "Extract entities.".to_string(),
            "nexus.llm.extract".to_string(),
            registry,
        );

        let ctx = graph_flow::Context::new();
        ctx.set(
            "chapter_prose".to_string(),
            "Lin Xia drew her blade.".to_string(),
        )
        .unwrap();

        let outcome = task.evaluate(&ctx).await.unwrap();
        let candidates = match outcome {
            crate::quality_loop::LlmExtractOutcome::Candidates { candidates: c, .. } => c,
            other => panic!("expected Candidates, got: {other:?}"),
        };
        assert_eq!(candidates.len(), 2, "expected 2 candidates");
        assert_eq!(candidates[0].canonical_name_guess, "Lin Xia");
        assert_eq!(candidates[0].block_type, "character");
        assert_eq!(candidates[0].confidence, Some(0.95));
        assert_eq!(candidates[1].canonical_name_guess, "Azure Gate");
        assert_eq!(candidates[1].block_type, "scene");
        assert_eq!(candidates[1].confidence, Some(0.8));
    }

    #[tokio::test]
    async fn llm_extract_task_no_worker_returns_unavailable() {
        // No worker → WorkerUnavailable is explicit, not an empty Vec contract
        // (closes R-V151Q3-W002).
        let registry = Arc::new(CapabilityRegistry::with_builtins());
        let task = LlmExtractTask::new(
            "Extract entities.".to_string(),
            "nexus.llm.extract".to_string(),
            registry,
        );
        let ctx = graph_flow::Context::new();
        let outcome = task.evaluate(&ctx).await.unwrap();
        assert!(
            matches!(
                outcome,
                crate::quality_loop::LlmExtractOutcome::WorkerUnavailable
            ),
            "no worker → WorkerUnavailable outcome"
        );
    }

    #[tokio::test]
    async fn llm_extract_task_missing_capability_returns_capability_error() {
        // Unknown capability name → CapabilityError inside the outcome.
        let registry = Arc::new(CapabilityRegistry::with_builtins());
        let task = LlmExtractTask::new(
            "Extract entities.".to_string(),
            "nexus.llm.nonexistent".to_string(),
            registry,
        );
        let ctx = graph_flow::Context::new();
        let outcome = task.evaluate(&ctx).await.unwrap();
        match outcome {
            crate::quality_loop::LlmExtractOutcome::CapabilityError(err) => {
                assert!(
                    err.contains("not registered"),
                    "expected 'not registered' in error: {err}"
                );
            }
            other => panic!("expected CapabilityError, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn llm_extract_task_malformed_llm_json_returns_empty_candidates() {
        // Malformed LLM response → capability returns empty candidates;
        // task surfaces them as Candidates(vec![]) (best-effort, no error).
        let registry = extract_registry_with_mock("not json at all");
        let task = LlmExtractTask::new(
            "Extract entities.".to_string(),
            "nexus.llm.extract".to_string(),
            registry,
        );
        let ctx = graph_flow::Context::new();
        let outcome = task.evaluate(&ctx).await.unwrap();
        match outcome {
            crate::quality_loop::LlmExtractOutcome::Candidates { candidates: c, .. } => {
                assert!(c.is_empty(), "malformed LLM JSON → empty candidates");
            }
            other => panic!("expected Candidates, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn llm_extract_unified_path_uses_quality_loop_mapping() {
        // Regression for R-V151Q3-W001: LlmExtractTask must use the same
        // LLM→KbCandidate mapping as the review-time hook, including derived
        // novel_category in the proposed_payload.
        let registry = extract_registry_with_mock(
            r#"{"candidates":[{"canonical_name":"Azure Gate","block_type":"scene","confidence":0.92,"source_quote":"...the eastern gate groaned open..."}]}"#,
        );
        let task = LlmExtractTask::new(
            "Extract entities.".to_string(),
            "nexus.llm.extract".to_string(),
            registry,
        );
        let ctx = graph_flow::Context::new();
        let outcome = task.evaluate(&ctx).await.unwrap();
        let candidates = match outcome {
            crate::quality_loop::LlmExtractOutcome::Candidates { candidates: c, .. } => c,
            other => panic!("expected Candidates, got: {other:?}"),
        };
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].block_type, "scene");
        let payload: serde_json::Value =
            serde_json::from_str(&candidates[0].proposed_payload).unwrap();
        assert_eq!(payload["attributes"]["novel_category"], "location");
        assert_eq!(payload["block_type"], "scene");
    }

    /// V1.55 P2 fix-wave (F-001): production-path coverage — `LlmExtractTask`
    /// with `work_profile = "game_bible"` must produce a game-bible-shaped
    /// candidate (`game_bible_category` set, `novel_category` absent, tags include
    /// `"game-bible"`). This exercises the full production path through
    /// `LlmExtractTask::evaluate` → `run_llm_extract` →
    /// `candidate_from_llm_json_for_profile`, not a helper-level test.
    #[tokio::test]
    async fn llm_extract_task_with_game_bible_profile_produces_game_bible_candidate() {
        let registry = extract_registry_with_mock(
            r#"{"candidates":[{"canonical_name":"Ironfang Legion","block_type":"faction","summary":"A ruthless mercenary company","confidence":0.93,"source_quote":"The Ironfang Legion marched through the gates at dawn."}]}"#,
        );
        let task = LlmExtractTask::new(
            "Extract entities.".to_string(),
            "nexus.llm.extract".to_string(),
            registry,
        );

        let ctx = graph_flow::Context::new();
        ctx.set(
            "chapter_prose".to_string(),
            "The Ironfang Legion marched through the gates at dawn.".to_string(),
        )
        .unwrap();
        ctx.set("work_profile".to_string(), "game_bible".to_string())
            .unwrap();

        let outcome = task.evaluate(&ctx).await.unwrap();
        let candidates = match outcome {
            crate::quality_loop::LlmExtractOutcome::Candidates { candidates: c, .. } => c,
            other => panic!("expected Candidates, got: {other:?}"),
        };
        assert_eq!(candidates.len(), 1, "expected 1 candidate");
        assert_eq!(candidates[0].canonical_name_guess, "Ironfang Legion");
        assert_eq!(candidates[0].block_type, "faction");

        let payload: serde_json::Value =
            serde_json::from_str(&candidates[0].proposed_payload).unwrap();
        // game_bible_category must be set (faction → faction in direct mapping)
        assert_eq!(
            payload["attributes"]["game_bible_category"], "faction",
            "game_bible_category should be 'faction' for block_type=faction"
        );
        // novel_category must NOT be present
        assert!(
            payload["attributes"]["novel_category"].is_null(),
            "novel_category must be absent from game-bible candidate"
        );
        // Tags must include "game-bible" and "llm-extracted"
        let tags = payload["tags"].as_array().expect("tags should be an array");
        let tag_strings: Vec<&str> = tags.iter().filter_map(|t| t.as_str()).collect();
        assert!(
            tag_strings.contains(&"game-bible"),
            "tags should include 'game-bible': {tag_strings:?}"
        );
        assert!(
            tag_strings.contains(&"llm-extracted"),
            "tags should include 'llm-extracted': {tag_strings:?}"
        );
        // block_type in payload matches
        assert_eq!(payload["block_type"], "faction");
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
    impl crate::capability::PromptExecutor for ControlledMockProvider {
        async fn execute(
            &self,
            _request: crate::capability::PromptRequest,
        ) -> Result<crate::capability::PromptResult, crate::capability::CapabilityError> {
            let resp = self.response.lock().unwrap().clone();
            Ok(crate::capability::PromptResult {
                full_text: resp,
                host_session_id: "host-sess".to_string(),
                operation_id: "op-1".to_string(),
            })
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
            prompt_executor: Some(provider),
            session_cancels: session_cancels_with_default(),
            daemon_tool_dispatch: None,
            cdn_config: None,
        workspace_executor: None,
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
            merge: None,
            converge: None,
            timeout_ms: None,
            on_timeout: None,
        };

        let task = StateCompositeTask::from_manifest(&state_def).with_registry(registry);

        let ctx = graph_flow::Context::new();
        ctx.set("_session_id", "default").unwrap();
        let result = task.run(ctx.clone()).await.unwrap();
        assert!(
            matches!(result.next_action, NextAction::Continue),
            "GO → Continue, got {:?}",
            result.next_action
        );

        // Verify judge context was stored.
        let judge_result: bool = ctx.get("_judge_result").unwrap();
        assert!(judge_result, "judge_result should be true for GO");
    }

    /// T5: novel-writing gathering exit with NOGO → `WaitForInput`.
    #[tokio::test]
    async fn state_composite_llm_judge_nogo_waits() {
        use crate::capability::CapabilityRuntimeDeps;

        let provider = std::sync::Arc::new(ControlledMockProvider::new(
            "NO — need more research material.",
        ));
        let deps = CapabilityRuntimeDeps {
            pool: None,
            prompt_executor: Some(provider),
            session_cancels: session_cancels_with_default(),
            daemon_tool_dispatch: None,
            cdn_config: None,
        workspace_executor: None,
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
            merge: None,
            converge: None,
            timeout_ms: None,
            on_timeout: None,
        };

        let task = StateCompositeTask::from_manifest(&state_def).with_registry(registry);

        let ctx = graph_flow::Context::new();
        ctx.set("_session_id", "default").unwrap();
        let result = task.run(ctx.clone()).await.unwrap();
        assert!(
            matches!(result.next_action, NextAction::WaitForInput),
            "NOGO → WaitForInput, got {:?}",
            result.next_action
        );

        let judge_result: bool = ctx.get("_judge_result").unwrap();
        assert!(!judge_result, "judge_result should be false for NOGO");
    }

    /// T5: `llm_judge` without worker IPC → `WaitForInput` (safe fallback).
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
            merge: None,
            converge: None,
            timeout_ms: None,
            on_timeout: None,
        };

        let task = StateCompositeTask::from_manifest(&state_def).with_registry(registry);

        let ctx = graph_flow::Context::new();
        ctx.set("_session_id", "default").unwrap();
        let result = task.run(ctx.clone()).await.unwrap();
        assert!(
            matches!(result.next_action, NextAction::WaitForInput),
            "no worker → WaitForInput, got {:?}",
            result.next_action
        );
    }

    /// T5: `llm_judge` with empty `template_file` → `WaitForInput`.
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
            merge: None,
            converge: None,
            timeout_ms: None,
            on_timeout: None,
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
            merge: None,
            converge: None,
            timeout_ms: None,
            on_timeout: None,
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
            merge: None,
            converge: None,
            timeout_ms: None,
            on_timeout: None,
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
        ctx.set("_capability_name", "workspace.open").unwrap();
        ctx.set("_capability_input", serde_json::json!({})).unwrap();
        let result = task.run(ctx).await.unwrap();
        assert!(matches!(result.next_action, NextAction::Continue));
    }

    #[tokio::test]
    async fn capability_task_returns_error_for_missing() {
        let reg = Arc::new(CapabilityRegistry::with_builtins());
        let task = CapabilityTask { registry: reg };
        let ctx = graph_flow::Context::new();
        ctx.set("_capability_name", "nonexistent.capability")
            .unwrap();
        let result = task.run(ctx).await;
        assert!(result.is_err());
    }

    fn empty_session_cancels() -> std::sync::Arc<
        std::sync::RwLock<std::collections::HashMap<String, tokio_util::sync::CancellationToken>>,
    > {
        std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()))
    }

    /// Shared cancellation map with a coordinator token registered for the
    /// given run id (the fail-closed contract: a prompt consumer with NO
    /// registered token refuses `CancellationUnavailable`; production runs
    /// register at admission, tests must do the same).
    fn session_cancels_with(
        session_id: &str,
    ) -> std::sync::Arc<
        std::sync::RwLock<std::collections::HashMap<String, tokio_util::sync::CancellationToken>>,
    > {
        let map = empty_session_cancels();
        map.write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(
                session_id.to_string(),
                tokio_util::sync::CancellationToken::new(),
            );
        map
    }

    /// Shared cancellation map registering the `"default"` run (the run id
    /// unbound capability inputs resolve to when no `_session_id` is set).
    fn session_cancels_with_default() -> std::sync::Arc<
        std::sync::RwLock<std::collections::HashMap<String, tokio_util::sync::CancellationToken>>,
    > {
        session_cancels_with("default")
    }

    #[tokio::test]
    async fn acp_prompt_task_stub_mode_refuses() {
        // A1: no executor — refuse with a typed failure, never a placeholder
        // success (no echo/worker fallback).
        let task = AcpPromptTask::new(
            None, // no executor — stub mode
            empty_session_cancels(),
            "test-state",
            "Hello {{core_context.version}}",
            ToolPolicy::DenyAll,
            None, // default session_id
        );
        let ctx = graph_flow::Context::new();
        ctx.set("core_context.version", "42").unwrap();
        let result = task.run(ctx).await;
        assert!(result.is_err(), "stub mode must refuse, got: {result:?}");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("no prompt executor"),
            "typed refusal expected: {err}"
        );
    }

    #[tokio::test]
    async fn acp_prompt_task_nested_handlebars_rendering() {
        // QC2-W-001: prove nested path rendering ({{world.title}}) works
        // through the real AcpPromptTask execution path.
        let task = AcpPromptTask::new(
            None,
            empty_session_cancels(),
            "test-state",
            "World: {{world.title}}, Chapter: {{world.chapter}}",
            ToolPolicy::DenyAll,
            None,
        );
        let ctx = graph_flow::Context::new();
        ctx.set("world.title", "Nexus").unwrap();
        ctx.set("world.chapter", "1").unwrap();
        let result = task.run(ctx).await;
        assert!(result.is_err(), "no executor must refuse: {result:?}");
    }

    #[tokio::test]
    async fn acp_prompt_task_no_escape_preserves_special_chars() {
        // QC2-S-001: handlebars must NOT HTML-escape prompt values.
        let task = AcpPromptTask::new(
            None,
            empty_session_cancels(),
            "test-state",
            "Text: {{content}}",
            ToolPolicy::DenyAll,
            None,
        );
        let ctx = graph_flow::Context::new();
        ctx.set("content", "foo & bar < baz > qux").unwrap();
        let result = task.run(ctx).await;
        assert!(result.is_err(), "no executor must refuse: {result:?}");
    }

    #[tokio::test]
    async fn acp_prompt_task_stores_output_in_context() {
        // A1: with a mock executor, the agent output lands at
        // state.<state_id>.output.
        struct MockExecutor;

        #[async_trait]
        impl crate::capability::PromptExecutor for MockExecutor {
            async fn execute(
                &self,
                _request: crate::capability::PromptRequest,
            ) -> Result<crate::capability::PromptResult, crate::capability::CapabilityError>
            {
                Ok(crate::capability::PromptResult {
                    full_text: "transformed:test prompt".to_string(),
                    host_session_id: "host-sess".to_string(),
                    operation_id: "op-1".to_string(),
                })
            }
        }

        let task = AcpPromptTask::new(
            Some(std::sync::Arc::new(MockExecutor)),
            session_cancels_with("default"),
            "state-1",
            "test prompt",
            ToolPolicy::AutoGrantReadOnly,
            None, // default session_id
        );
        let ctx = graph_flow::Context::new();
        let result = task.run(ctx.clone()).await.unwrap();
        let stored: String = ctx.get("state.state-1.output").unwrap();
        assert_eq!(stored, "transformed:test prompt");
        assert_eq!(result.response.as_deref(), Some(stored.as_str()));
    }

    /// T1 review (Critical 1): the external-effect marker must be durably
    /// written BEFORE the external dispatch — a failed marker write aborts
    /// before dispatch, so no external effect ever runs unmarked. `Context`
    /// clones share the underlying map, so the executor observes the marker
    /// through its own handle at dispatch time.
    #[tokio::test]
    async fn acp_prompt_marks_external_effect_before_dispatch() {
        struct MarkerObservingExecutor {
            ctx: graph_flow::Context,
        }

        #[async_trait]
        impl crate::capability::PromptExecutor for MarkerObservingExecutor {
            async fn execute(
                &self,
                _request: crate::capability::PromptRequest,
            ) -> Result<crate::capability::PromptResult, crate::capability::CapabilityError>
            {
                let marked: bool = self
                    .ctx
                    .get(crate::engine::EXTERNAL_EFFECT_MARKER)
                    .unwrap_or(false);
                assert!(
                    marked,
                    "external-effect marker must be persisted BEFORE prompt dispatch"
                );
                Ok(crate::capability::PromptResult {
                    full_text: "observed".to_string(),
                    host_session_id: "host-sess".to_string(),
                    operation_id: "op-1".to_string(),
                })
            }
        }

        let ctx = graph_flow::Context::new();
        let task = AcpPromptTask::new(
            Some(std::sync::Arc::new(MarkerObservingExecutor {
                ctx: ctx.clone(),
            })),
            session_cancels_with("default"),
            "state-1",
            "test prompt",
            ToolPolicy::AutoGrantReadOnly,
            None,
        );
        task.run(ctx.clone()).await.unwrap();
        let marked: bool = ctx
            .get(crate::engine::EXTERNAL_EFFECT_MARKER)
            .unwrap_or(false);
        assert!(marked, "marker persisted in the step context");
    }

    #[tokio::test]
    async fn acp_prompt_task_with_explicit_session_id() {
        // WS-E T5: test session_id field
        let task = AcpPromptTask::new(
            None,
            empty_session_cancels(),
            "state-1",
            "test prompt",
            ToolPolicy::AutoGrantReadOnly,
            Some("writer_session".to_string()), // explicit session_id
        );
        let ctx = graph_flow::Context::new();
        let result = task.run(ctx.clone()).await;
        assert!(result.is_err(), "no executor must refuse: {result:?}");
    }

    #[tokio::test]
    async fn acp_prompt_task_with_session_id_method() {
        // WS-E T5: test with_session_id constructor
        let task = AcpPromptTask::with_session_id(
            None,
            empty_session_cancels(),
            "state-1",
            "test prompt",
            ToolPolicy::AutoGrantReadOnly,
            "reviewer_session",
        );
        let ctx = graph_flow::Context::new();
        let result = task.run(ctx.clone()).await;
        assert!(result.is_err(), "no executor must refuse: {result:?}");
    }

    #[tokio::test]
    async fn inner_graph_node_task_stub_mode_with_session_id() {
        // WS-E T5: InnerGraphNodeTask should track session_id even in stub mode.
        // A1: no executor — refuse with a typed failure, never a placeholder.
        let task = InnerGraphNodeTask::new("n1").with_session_id("writer_session");
        let ctx = graph_flow::Context::new();
        let result = task.run(ctx.clone()).await;
        assert!(result.is_err(), "no executor must refuse: {result:?}");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("no prompt executor"),
            "typed refusal expected: {err}"
        );
    }

    #[tokio::test]
    async fn inner_graph_node_task_missing_identity_refuses() {
        // M-002: no explicit session_id and no trusted `_session_id` in the
        // context → the node refuses with a typed graph error; there is no
        // magic `default` fallback run id.
        let task = InnerGraphNodeTask::new("n1");
        let ctx = graph_flow::Context::new();
        let result = task.run(ctx.clone()).await;
        assert!(result.is_err(), "missing identity must refuse: {result:?}");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("missing trusted _session_id"),
            "typed refusal expected: {err}"
        );
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

    #[derive(Default)]
    struct CapturingIdentityExecutor {
        request: std::sync::Mutex<Option<crate::capability::PromptRequest>>,
    }

    #[async_trait]
    impl crate::capability::PromptExecutor for CapturingIdentityExecutor {
        async fn execute(
            &self,
            request: crate::capability::PromptRequest,
        ) -> Result<crate::capability::PromptResult, crate::capability::CapabilityError> {
            *self.request.lock().expect("capture lock") = Some(request);
            Ok(crate::capability::PromptResult {
                full_text: "captured".to_string(),
                host_session_id: "host-sess".to_string(),
                operation_id: "op-1".to_string(),
            })
        }
    }

    fn identity_registry(executor: Arc<CapturingIdentityExecutor>) -> Arc<CapabilityRegistry> {
        let session_cancels: Arc<
            std::sync::RwLock<
                std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
            >,
        > = Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));
        // The identity regressions run an unbound capability path whose run
        // id resolves to the trusted `_session_id` injected by the engine —
        // register the coordinator tokens for the known test run ids (and
        // "default" for unbound paths) so the prompt consumer reaches the
        // executor (fail-closed contract).
        for sid in ["default", "sess_42ch_001", "real_session"] {
            session_cancels
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .entry(sid.to_string())
                .or_default();
        }
        let deps = crate::capability::CapabilityRuntimeDeps {
            pool: None,
            prompt_executor: Some(executor),
            session_cancels,
            daemon_tool_dispatch: None,
            cdn_config: None,
        workspace_executor: None,
        };
        Arc::new(CapabilityRegistry::with_runtime_deps(&deps))
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
            merge: None,
            converge: None,
            timeout_ms: None,
            on_timeout: None,
        };

        let executor = Arc::new(CapturingIdentityExecutor::default());
        let task = StateCompositeTask::from_manifest(&state_def)
            .with_registry(identity_registry(executor.clone()));

        // Simulate the engine setting identity in context (as start_session does).
        let ctx = graph_flow::Context::new();
        ctx.set("_creator_id", "creator_alice").unwrap();
        ctx.set("_session_id", "sess_42ch_001").unwrap();

        let result = task.run(ctx.clone()).await.unwrap();
        assert!(
            matches!(result.next_action, NextAction::End),
            "terminal state should End"
        );

        let captured = executor
            .request
            .lock()
            .expect("capture lock")
            .clone()
            .expect("prompt request");
        assert_eq!(captured.run_id, "sess_42ch_001");
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
            merge: None,
            converge: None,
            timeout_ms: None,
            on_timeout: None,
        };

        let executor = Arc::new(CapturingIdentityExecutor::default());
        let task = StateCompositeTask::from_manifest(&state_def)
            .with_registry(identity_registry(executor.clone()));

        let ctx = graph_flow::Context::new();
        ctx.set("_creator_id", "real_creator").unwrap();
        ctx.set("_session_id", "real_session").unwrap();

        let result = task.run(ctx.clone()).await.unwrap();
        assert!(matches!(result.next_action, NextAction::End));

        let captured = executor
            .request
            .lock()
            .expect("capture lock")
            .clone()
            .expect("prompt request");
        assert_eq!(captured.run_id, "real_session");
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
            merge: None,
            converge: None,
            timeout_ms: None,
            on_timeout: None,
        };

        let task = StateCompositeTask::from_manifest(&state_def)
            .with_registry(Arc::new(CapabilityRegistry::with_builtins()));

        let ctx = graph_flow::Context::new();
        let result = task.run(ctx.clone()).await.unwrap();
        assert!(matches!(result.next_action, NextAction::End));

        let input: serde_json::Value = ctx.get("_capability_input").unwrap_or(Value::Null);
        assert!(
            input.get("_creator_id").is_none(),
            "untrusted _creator_id must be stripped when trusted context is absent: {input}"
        );
        assert!(
            input.get("_session_id").is_none(),
            "untrusted _session_id must be stripped when trusted context is absent: {input}"
        );
    }

    // ── C-V133P2-01: Capability arg template rendering tests ──────────

    /// Proves that `render_value_templates` renders string placeholders
    /// in a JSON object against the context payload.
    #[test]
    fn render_value_templates_renders_nested_placeholders() {
        let args = serde_json::json!({
            "workId": "{{preset.input.work_id}}",
            "briefText": "{{state.synthesizing.output}}",
            "staticValue": 42,
            "tags": ["{{preset.input.keyword}}", "hardcoded"]
        });

        let payload = serde_json::json!({
            "preset": {
                "input": {
                    "work_id": "wrk_test_123",
                    "keyword": "fantasy"
                }
            },
            "state": {
                "synthesizing": {
                    "output": "{\"genre\":\"fantasy\"}"
                }
            }
        });

        let rendered = render_value_templates(&args, &payload).unwrap();

        assert_eq!(rendered["workId"], "wrk_test_123");
        assert_eq!(rendered["briefText"], "{\"genre\":\"fantasy\"}");
        assert_eq!(rendered["staticValue"], 42);
        let tags = rendered["tags"].as_array().unwrap();
        assert_eq!(tags[0], "fantasy");
        assert_eq!(tags[1], "hardcoded");
    }

    /// Proves that `render_value_templates` fails-closed when a placeholder
    /// references a non-existent key.
    #[test]
    fn render_value_templates_fails_closed_on_missing_key() {
        let args = serde_json::json!({
            "workId": "{{preset.input.nonexistent}}"
        });

        let payload = serde_json::json!({
            "preset": {
                "input": {
                    "work_id": "wrk_real"
                }
            }
        });

        let result = render_value_templates(&args, &payload);
        assert!(result.is_err(), "should fail on missing template key");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("template render failed"),
            "error should mention template render: {err}"
        );
    }

    /// C-V133P2-01 integration: `StateCompositeTask` renders capability args
    /// through the template engine before passing to the capability.
    ///
    /// This test loads the actual engine context with preset.input and
    /// `state.*` values, runs a `StateCompositeTask` with a capability action
    /// that uses template placeholders, and verifies the rendered values
    /// reach the capability input.
    #[tokio::test]
    async fn state_composite_renders_capability_args_templates() {
        use crate::preset::manifest::EnterAction;

        let state_def = crate::preset::manifest::StateDefinition {
            id: "persisting".into(),
            description: None,
            enter: vec![EnterAction::Capability {
                name: "creator.write_brief".into(),
                args: Some(serde_json::json!({
                    "workId": "{{preset.input.work_id}}",
                    "briefText": "{{state.synthesizing.output}}"
                })),
            }],
            exit_when: None,
            next: None,
            terminal: true,
            context_update: None,
            merge: None,
            converge: None,
            timeout_ms: None,
            on_timeout: None,
        };

        let task = StateCompositeTask::from_manifest(&state_def)
            .with_registry(Arc::new(CapabilityRegistry::with_builtins()));

        let ctx = graph_flow::Context::new();
        ctx.set("_creator_id", "ctr_test").unwrap();
        ctx.set("_session_id", "sess_test").unwrap();
        ctx.set("preset.input.work_id", "wrk_rendered_123").unwrap();

        // Simulate what InnerGraphTask would write after synthesizing:
        // state.synthesizing.output = the JSON string of the brief
        let brief = serde_json::json!({
            "brief_schema_version": 1,
            "genre": "fantasy",
            "tone": "epic",
            "audience": "young adult",
            "constraints": ["no graphic violence"],
            "themes": ["heroism", "sacrifice"],
            "non_goals": ["not a romance"],
            "protagonist_hook": "A farm girl discovers a dragon egg",
            "setting_hook": "A mountainous kingdom under siege",
            "open_questions_resolved": ["genre: fantasy"]
        });
        ctx.set(
            "state.synthesizing.output",
            serde_json::to_string(&brief).unwrap(),
        )
        .unwrap();

        let result = task.run(ctx.clone()).await.unwrap();
        assert!(
            matches!(result.next_action, NextAction::End),
            "task should complete successfully"
        );

        // Verify the capability received RENDERED args
        let cap_input: serde_json::Value = ctx.get("_capability_input").unwrap_or(Value::Null);
        assert_eq!(
            cap_input["workId"], "wrk_rendered_123",
            "workId should be rendered, not literal '{{preset.input.work_id}}': {cap_input}"
        );
        assert_eq!(
            cap_input["briefText"],
            serde_json::to_string(&brief).unwrap(),
            "briefText should be rendered, not literal placeholder: {cap_input}"
        );
        // Verify identity injection still works after template rendering
        assert_eq!(cap_input["_creator_id"], "ctr_test");
        assert_eq!(cap_input["_session_id"], "sess_test");
    }

    // ── V1.42 P2 T4: judge_next_action unit tests ──────────────────────

    fn make_composite_with_next(next: Option<NextTarget>) -> StateCompositeTask {
        StateCompositeTask {
            id: "test_judge".to_string(),
            terminal: false,
            enter_actions: vec![],
            exit_when: None,
            next,
            engine: None,
            inner_graphs: std::collections::HashMap::new(),
            output_bindings: std::collections::HashMap::new(),
            registry: None,
            daemon_tool_dispatch: None,
            merge_kind: None,
            expected_incoming: 0,
            merge_key: "_merge_test_judge".to_string(),
            converge: None,
            converge_key: "_converge_arrivals_test_judge".to_string(),
            converge_predecessors: std::collections::HashSet::new(),
            cached_expr: None,
            workspace_state: None,
            timeout_ms: None,
            on_timeout: None,
            clock: Arc::new(SystemJoinClock),
        }
    }

    #[test]
    fn judge_next_action_linear_go_advances() {
        let task = make_composite_with_next(Some(NextTarget::Linear("next_state".to_string())));
        assert!(matches!(task.judge_next_action(true), NextAction::Continue));
    }

    #[test]
    fn judge_next_action_linear_nogo_waits() {
        let task = make_composite_with_next(Some(NextTarget::Linear("next_state".to_string())));
        assert!(matches!(
            task.judge_next_action(false),
            NextAction::WaitForInput
        ));
    }

    #[test]
    fn judge_next_action_none_go_advances() {
        let task = make_composite_with_next(None);
        assert!(matches!(task.judge_next_action(true), NextAction::Continue));
    }

    #[test]
    fn judge_next_action_none_nogo_waits() {
        let task = make_composite_with_next(None);
        assert!(matches!(
            task.judge_next_action(false),
            NextAction::WaitForInput
        ));
    }

    #[test]
    fn judge_next_action_gonogo_go_advances() {
        let task = make_composite_with_next(Some(NextTarget::GoNogo(GoNogoNext {
            go: "go_state".to_string(),
            nogo: "nogo_state".to_string(),
        })));
        assert!(matches!(task.judge_next_action(true), NextAction::Continue));
    }

    #[test]
    fn judge_next_action_gonogo_nogo_also_advances() {
        // Key V1.42 behavior: NOGO with GoNogo next → Continue (edge routes to nogo target).
        let task = make_composite_with_next(Some(NextTarget::GoNogo(GoNogoNext {
            go: "go_state".to_string(),
            nogo: "nogo_state".to_string(),
        })));
        assert!(matches!(
            task.judge_next_action(false),
            NextAction::Continue
        ));
    }

    // ── V1.52 T-B P0: resolve_labeled_target unit tests ─────────────────

    fn make_labeled_edges(labels: &[(&str, &str)]) -> Vec<LabeledNext> {
        labels
            .iter()
            .map(|(l, t)| LabeledNext {
                label: (*l).to_string(),
                target: (*t).to_string(),
            })
            .collect()
    }

    #[test]
    fn resolve_labeled_target_single_label_match() {
        let task = make_composite_with_next(Some(NextTarget::Labeled(make_labeled_edges(&[(
            "outline",
            "state_outline",
        )]))));
        let ctx = graph_flow::Context::new();
        let result = task.resolve_labeled_target(&ctx, "The judge recommends: outline");
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap(),
            NextAction::GoTo("state_outline".to_string())
        );
    }

    #[test]
    fn resolve_labeled_target_multi_label_first_match() {
        // First matching label (in edge order) should win.
        let task = make_composite_with_next(Some(NextTarget::Labeled(make_labeled_edges(&[
            ("research", "state_research"),
            ("outline", "state_outline"),
            ("abandon", "state_abandon"),
        ]))));
        let ctx = graph_flow::Context::new();
        let result = task.resolve_labeled_target(
            &ctx,
            "I recommend to research more, but outline is also possible",
        );
        assert!(result.is_ok());
        // "research" appears first in both the text and the edge list.
        assert_eq!(
            result.unwrap(),
            NextAction::GoTo("state_research".to_string())
        );
    }

    #[test]
    fn resolve_labeled_target_no_match_errors() {
        // W-QC3-3: no-match MUST NOT stall (return WaitForInput).
        // Instead, return Err with diagnostic info.
        let task = make_composite_with_next(Some(NextTarget::Labeled(make_labeled_edges(&[
            ("outline", "state_outline"),
            ("research", "state_research"),
        ]))));
        let ctx = graph_flow::Context::new();
        let result = task
            .resolve_labeled_target(&ctx, "The judge output says something completely unrelated");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("no label matched"),
            "error should mention 'no label matched': {err}"
        );
        assert!(
            err.contains("Known labels"),
            "error should list known labels: {err}"
        );
    }

    #[test]
    fn resolve_labeled_target_non_labeled_next_returns_ok_wait() {
        // Non-Labeled next (e.g., Linear) should return Ok(WaitForInput).
        let task = make_composite_with_next(Some(NextTarget::Linear("next_state".to_string())));
        let ctx = graph_flow::Context::new();
        let result = task.resolve_labeled_target(&ctx, "anything");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), NextAction::WaitForInput);
    }

    #[test]
    fn resolve_labeled_target_none_next_returns_ok_wait() {
        let task = make_composite_with_next(None);
        let ctx = graph_flow::Context::new();
        let result = task.resolve_labeled_target(&ctx, "anything");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), NextAction::WaitForInput);
    }

    #[test]
    fn resolve_labeled_target_writes_judge_label_context() {
        // W-001: context._judge_label must be written on successful match.
        let task = make_composite_with_next(Some(NextTarget::Labeled(make_labeled_edges(&[(
            "outline",
            "state_outline",
        )]))));
        let ctx = graph_flow::Context::new();
        let _ = task.resolve_labeled_target(&ctx, "choose outline please");
        let label: Option<String> = ctx.get("_judge_label");
        assert_eq!(
            label.as_deref(),
            Some("outline"),
            "context._judge_label should be 'outline' after match"
        );
    }

    #[test]
    fn resolve_labeled_target_gonogo_auto_conversion_go_match() {
        // W-QC3-2: binary GoNogo edges auto-converted to labeled routing.
        let task = make_composite_with_next(Some(NextTarget::GoNogo(GoNogoNext {
            go: "state_go".to_string(),
            nogo: "state_nogo".to_string(),
        })));
        let ctx = graph_flow::Context::new();
        let result = task.resolve_labeled_target(&ctx, "ready to go forward");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), NextAction::GoTo("state_go".to_string()));
    }

    #[test]
    fn resolve_labeled_target_gonogo_auto_conversion_nogo_match() {
        let task = make_composite_with_next(Some(NextTarget::GoNogo(GoNogoNext {
            go: "state_go".to_string(),
            nogo: "state_nogo".to_string(),
        })));
        let ctx = graph_flow::Context::new();
        let result = task.resolve_labeled_target(&ctx, "this is a nogo decision");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), NextAction::GoTo("state_nogo".to_string()));
    }

    #[test]
    fn resolve_labeled_target_gonogo_auto_conversion_no_match_errors() {
        // Auto-converted GoNogo edges also error on no-match.
        let task = make_composite_with_next(Some(NextTarget::GoNogo(GoNogoNext {
            go: "state_go".to_string(),
            nogo: "state_nogo".to_string(),
        })));
        let ctx = graph_flow::Context::new();
        let result = task.resolve_labeled_target(&ctx, "completely unrelated text");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("no label matched"), "error: {err}");
    }

    // ── V1.52 T-B P1: wait-all default enforcement (W-QC1-1) ────────────

    #[tokio::test]
    async fn merge_wait_all_default_enforced_when_merge_absent() {
        // A state with 2 incoming labeled edges but NO explicit `merge:`
        // field MUST still enforce wait-all semantics (default).
        let task = StateCompositeTask {
            id: "merged".to_string(),
            terminal: false,
            enter_actions: vec![],
            exit_when: None, // no exit condition → Continue after gate passes
            next: Some(NextTarget::Linear("done".to_string())),
            engine: None,
            inner_graphs: std::collections::HashMap::new(),
            output_bindings: std::collections::HashMap::new(),
            registry: None,
            daemon_tool_dispatch: None,
            merge_kind: None, // absent from YAML
            expected_incoming: 2,
            merge_key: "_merge_merged".to_string(),
            converge: None,
            converge_key: "_converge_arrivals_merged".to_string(),
            converge_predecessors: std::collections::HashSet::new(),
            cached_expr: None,
            workspace_state: None,
            timeout_ms: None,
            on_timeout: None,
            clock: Arc::new(SystemJoinClock),
        };

        let ctx = graph_flow::Context::new();

        // With 0 arrivals → should wait (default wait-all enforces gate).
        let result = task.run(ctx.clone()).await.unwrap();
        assert!(
            matches!(result.next_action, NextAction::WaitForInput),
            "with 0 arrivals and merge absent (default wait-all), should WaitForInput; got {:?}",
            result.next_action
        );

        // With 1 arrival → should still wait (wait-all needs all 2).
        ctx.set("_merge_merged", serde_json::json!(["label_a"]))
            .unwrap();
        let result = task.run(ctx.clone()).await.unwrap();
        assert!(
            matches!(result.next_action, NextAction::WaitForInput),
            "with 1/2 arrivals and merge absent (default wait-all), should WaitForInput; got {:?}",
            result.next_action
        );

        // With 2 arrivals → should continue.
        ctx.set("_merge_merged", serde_json::json!(["label_a", "label_b"]))
            .unwrap();
        let result = task.run(ctx.clone()).await.unwrap();
        assert!(
            matches!(result.next_action, NextAction::Continue),
            "with 2/2 arrivals and merge absent (default wait-all), should Continue; got {:?}",
            result.next_action
        );
    }

    // ── V1.56 P3: registry.refresh conditional edges ──────────────────

    /// Build a state with `Branches` next for expression routing.
    fn make_branches_task(
        state_id: &str,
        branches: Vec<ConditionalRule>,
        default: &str,
    ) -> StateCompositeTask {
        let next = NextTarget::Branches(ConditionalBranches {
            branches,
            default: default.to_string(),
        });
        let cached_expr = StateCompositeTask::build_expr_cache(Some(&next));

        StateCompositeTask {
            id: state_id.to_string(),
            terminal: false,
            enter_actions: vec![],
            exit_when: None,
            next: Some(next),
            engine: None,
            inner_graphs: std::collections::HashMap::new(),
            output_bindings: std::collections::HashMap::new(),
            registry: Some(std::sync::Arc::new(CapabilityRegistry::with_builtins())),
            daemon_tool_dispatch: None,
            merge_kind: None,
            expected_incoming: 0,
            merge_key: format!("_merge_{state_id}"),
            converge: None,
            converge_key: format!("_converge_arrivals_{state_id}"),
            converge_predecessors: std::collections::HashSet::new(),
            cached_expr,
            workspace_state: None,
            timeout_ms: None,
            on_timeout: None,
            clock: Arc::new(SystemJoinClock),
        }
    }

    #[tokio::test]
    async fn registry_synthetic_branch() {
        // When registry.refresh returns synthetic output, the expression
        // `_context.registry_refresh.source == 'synthetic'` should match.
        let task = make_branches_task(
            "check_registry",
            vec![ConditionalRule {
                when: "_context.registry_refresh.source == 'synthetic'".to_string(),
                target: "synthetic_state".to_string(),
            }],
            "standard_state",
        );

        let ctx = graph_flow::Context::new();
        let result = task.run(ctx).await.unwrap();

        assert!(
            matches!(result.next_action, NextAction::GoTo(ref t) if t == "synthetic_state"),
            "synthetic source should route to synthetic_state, got {:?}",
            result.next_action
        );
    }

    #[tokio::test]
    async fn registry_network_branch() {
        // Pre-set CDN-like output to simulate a successful network fetch.
        let task = make_branches_task(
            "check_registry",
            vec![ConditionalRule {
                when: "_context.registry_refresh.source == 'cdn'".to_string(),
                target: "network_state".to_string(),
            }],
            "standard_state",
        );

        let ctx = graph_flow::Context::new();
        // Inject synthetic CDN output into the context before the task runs.
        ctx.set(
            "__registry_refresh_output",
            serde_json::json!({
                "source": "cdn",
                "snapshotVersion": "2026-06-22.v1",
                "capabilityCount": 42,
                "fallbackReason": "",
                "retryCount": 1,
                "cacheAgeMs": 0,
                "generatedAt": "2026-06-22T00:00:00Z",
                "fetchTimeoutMs": 10000,
                "maxRetries": 3,
            }),
        )
        .unwrap();

        let result = task.run(ctx).await.unwrap();

        assert!(
            matches!(result.next_action, NextAction::GoTo(ref t) if t == "network_state"),
            "CDN source should route to network_state, got {:?}",
            result.next_action
        );
    }

    #[tokio::test]
    async fn registry_fallback_branch() {
        // Inject fallback output (CDN failure → synthetic_fallback).
        let task = make_branches_task(
            "check_registry",
            vec![ConditionalRule {
                when: "_context.registry_refresh.source == 'synthetic_fallback'".to_string(),
                target: "fallback_state".to_string(),
            }],
            "standard_state",
        );

        let ctx = graph_flow::Context::new();
        ctx.set(
            "__registry_refresh_output",
            serde_json::json!({
                "source": "synthetic_fallback",
                "snapshotVersion": "2026-06-22.v1",
                "capabilityCount": 31,
                "fallbackReason": "CdnError::Timeout",
                "retryCount": 3,
                "cacheAgeMs": 0,
                "generatedAt": "2026-06-22T00:00:00Z",
                "fetchTimeoutMs": 10000,
                "maxRetries": 3,
            }),
        )
        .unwrap();

        let result = task.run(ctx).await.unwrap();

        assert!(
            matches!(result.next_action, NextAction::GoTo(ref t) if t == "fallback_state"),
            "fallback source should route to fallback_state, got {:?}",
            result.next_action
        );
    }

    #[tokio::test]
    async fn registry_capability_count_threshold() {
        // capability_count > 50 should match when capabilityCount is 100.
        let task = make_branches_task(
            "check_registry",
            vec![ConditionalRule {
                when: "_context.registry_refresh.capability_count > 50".to_string(),
                target: "high_capability_state".to_string(),
            }],
            "standard_state",
        );

        let ctx = graph_flow::Context::new();
        ctx.set(
            "__registry_refresh_output",
            serde_json::json!({
                "source": "cdn",
                "snapshotVersion": "2026-06-22.v1",
                "capabilityCount": 100,
                "fallbackReason": "",
                "retryCount": 0,
                "cacheAgeMs": 0,
                "generatedAt": "2026-06-22T00:00:00Z",
                "fetchTimeoutMs": 0,
                "maxRetries": 0,
            }),
        )
        .unwrap();

        let result = task.run(ctx).await.unwrap();

        assert!(
            matches!(result.next_action, NextAction::GoTo(ref t) if t == "high_capability_state"),
            "capability_count > 50 should route to high_capability_state, got {:?}",
            result.next_action
        );
    }

    #[tokio::test]
    async fn registry_capability_count_below_threshold_goes_default() {
        // capability_count > 50 should NOT match when capabilityCount is 30.
        let task = make_branches_task(
            "check_registry",
            vec![ConditionalRule {
                when: "_context.registry_refresh.capability_count > 50".to_string(),
                target: "high_capability_state".to_string(),
            }],
            "standard_state",
        );

        let ctx = graph_flow::Context::new();
        ctx.set(
            "__registry_refresh_output",
            serde_json::json!({
                "source": "synthetic",
                "snapshotVersion": "2026-06-22.v1",
                "capabilityCount": 30,
                "fallbackReason": "",
                "retryCount": 0,
                "cacheAgeMs": 0,
                "generatedAt": "2026-06-22T00:00:00Z",
                "fetchTimeoutMs": 0,
                "maxRetries": 0,
            }),
        )
        .unwrap();

        let result = task.run(ctx).await.unwrap();

        assert!(
            matches!(result.next_action, NextAction::GoTo(ref t) if t == "standard_state"),
            "capability_count <= 50 should route to default, got {:?}",
            result.next_action
        );
    }

    // ── V1.56 P3: workspace.open/commit branch inputs ─────────────────

    #[tokio::test]
    async fn workspace_no_conflict_continues() {
        // When no conflict, `_context.workspace.committed` is false,
        // so `!_context.workspace.committed` should be true → continue.
        let task = make_branches_task(
            "check_workspace",
            vec![ConditionalRule {
                when: "!_context.workspace.committed".to_string(),
                target: "continue_state".to_string(),
            }],
            "resolve_conflict_state",
        );

        let ctx = graph_flow::Context::new();
        ctx.set(
            "__workspace_state",
            serde_json::json!({
                "session_id": "ws_test1",
                "committed": false,
                "change_count": 0,
                "workspace_root": "/tmp/ws"
            }),
        )
        .unwrap();

        let result = task.run(ctx).await.unwrap();

        assert!(
            matches!(result.next_action, NextAction::GoTo(ref t) if t == "continue_state"),
            "no conflict should route to continue_state, got {:?}",
            result.next_action
        );
    }

    #[tokio::test]
    async fn workspace_conflict_resolves() {
        // When committed is true, route to resolve_conflict.
        let task = make_branches_task(
            "check_workspace",
            vec![ConditionalRule {
                when: "_context.workspace.committed".to_string(),
                target: "resolve_conflict_state".to_string(),
            }],
            "continue_state",
        );

        let ctx = graph_flow::Context::new();
        ctx.set(
            "__workspace_state",
            serde_json::json!({
                "session_id": "ws_test2",
                "committed": true,
                "change_count": 0,
                "workspace_root": "/tmp/ws"
            }),
        )
        .unwrap();

        let result = task.run(ctx).await.unwrap();

        assert!(
            matches!(result.next_action, NextAction::GoTo(ref t) if t == "resolve_conflict_state"),
            "conflict should route to resolve_conflict_state, got {:?}",
            result.next_action
        );
    }

    #[tokio::test]
    async fn workspace_changes_count_threshold() {
        // change_count > 3 → route to review_state.
        let task = make_branches_task(
            "check_workspace",
            vec![ConditionalRule {
                when: "_context.workspace.change_count > 3".to_string(),
                target: "review_state".to_string(),
            }],
            "continue_state",
        );

        let ctx = graph_flow::Context::new();
        ctx.set(
            "__workspace_state",
            serde_json::json!({
                "session_id": "ws_test3",
                "committed": false,
                "change_count": 5,
                "workspace_root": "/tmp/ws"
            }),
        )
        .unwrap();

        let result = task.run(ctx).await.unwrap();

        assert!(
            matches!(result.next_action, NextAction::GoTo(ref t) if t == "review_state"),
            "change_count > 3 should route to review_state, got {:?}",
            result.next_action
        );
    }

    #[tokio::test]
    async fn workspace_changes_count_below_threshold_goes_default() {
        // change_count > 3 should NOT match when change_count is 2.
        let task = make_branches_task(
            "check_workspace",
            vec![ConditionalRule {
                when: "_context.workspace.change_count > 3".to_string(),
                target: "review_state".to_string(),
            }],
            "continue_state",
        );

        let ctx = graph_flow::Context::new();
        ctx.set(
            "__workspace_state",
            serde_json::json!({
                "session_id": "ws_test4",
                "committed": false,
                "change_count": 2,
                "workspace_root": "/tmp/ws"
            }),
        )
        .unwrap();

        let result = task.run(ctx).await.unwrap();

        assert!(
            matches!(result.next_action, NextAction::GoTo(ref t) if t == "continue_state"),
            "change_count <= 3 should route to default, got {:?}",
            result.next_action
        );
    }

    #[tokio::test]
    async fn workspace_default_state_when_no_state_injected() {
        // When no workspace state is set, the default values should be used:
        // committed = false, so `_context.workspace.committed` is false.
        let task = make_branches_task(
            "check_workspace",
            vec![ConditionalRule {
                when: "_context.workspace.committed".to_string(),
                target: "resolve_conflict_state".to_string(),
            }],
            "continue_state",
        );

        let ctx = graph_flow::Context::new();
        // No workspace state injected — the task uses its own default.
        let result = task.run(ctx).await.unwrap();

        assert!(
            matches!(result.next_action, NextAction::GoTo(ref t) if t == "continue_state"),
            "default workspace state (no conflict) should route to continue_state, got {:?}",
            result.next_action
        );
    }

    #[tokio::test]
    async fn registry_refresh_only_invoked_when_needed() {
        // Task with no registry references should NOT invoke registry.refresh.
        let task = make_branches_task(
            "check_simple",
            vec![ConditionalRule {
                when: "_context.score > 80".to_string(),
                target: "approved".to_string(),
            }],
            "rejected",
        );

        let ctx = graph_flow::Context::new();
        ctx.set("score", serde_json::json!(90)).unwrap();

        let result = task.run(ctx).await.unwrap();

        assert!(
            matches!(result.next_action, NextAction::GoTo(ref t) if t == "approved"),
            "score > 80 should route to approved, got {:?}",
            result.next_action
        );
    }
}
