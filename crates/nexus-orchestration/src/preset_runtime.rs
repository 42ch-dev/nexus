//! Execution-only preset graph construction and provider binding creation.

use crate::capability::CapabilityRegistry;
use nexus_preset::{load_embedded_preset, required_prompt_roles, LoadedPreset};
use nexus_preset::loader::incoming_labeled_edge_counts;
use nexus_preset::manifest::{NextTarget, PresetManifest};
use std::collections::HashMap;
use std::sync::Arc;

/// Build a complete binding map for an EMBEDDED preset using one provider id
/// (N-9).
///
/// Every effective prompt role (including `default`) is bound to the given
/// provider. Returns `None` when the preset cannot be resolved — the caller
/// must then refuse or stay inert, never publish a drive-enabled row with an
/// incomplete binding map.
///
/// The internal insertion paths (auto-chain, cron, review-master) use this
/// to derive bindings consistent with the daemon's configured providers;
/// the admission validator derives the SAME required-role set via
/// [`required_prompt_roles`], so the two can never drift.
#[must_use]
pub fn default_bindings_for_preset(
    preset_id: &str,
    provider_id: &str,
) -> Option<std::collections::HashMap<String, crate::run_state::AgentBinding>> {
    let caps = crate::capability::CapabilityRegistry::with_builtins();
    let loaded = load_embedded_preset(preset_id, &caps).ok()?;
    let roles = required_prompt_roles(&loaded);
    if roles.is_empty() {
        return Some(std::collections::HashMap::new());
    }
    let binding = crate::run_state::AgentBinding {
        provider_id: provider_id.to_string(),
        model: None,
    };
    Some(
        roles
            .into_iter()
            .map(|role| (role, binding.clone()))
            .collect(),
    )
}

/// Build the outer state-machine graph per §8.2.
///
/// Each `states[].id` → a composite `Task` that encodes the enter actions,
/// `exit_when` condition, and terminal semantics.
///
/// Note: template resolution is skipped here because `build_outer_graph` is
/// used in test contexts where inline template strings are expected. Production
/// code uses `build_wired_outer_graph` which resolves `template_file` paths.
pub fn build_outer_graph(
    manifest: &PresetManifest,
) -> Result<graph_flow::Graph, graph_flow::GraphError> {
    use crate::tasks::StateCompositeTask;
    use std::collections::HashMap;

    // graph-flow 0.8: graphs are immutable and assembled through the
    // consuming `GraphBuilder`; `build()` validates edge endpoints.
    let mut builder = graph_flow::GraphBuilder::new(&manifest.preset.id);

    // V1.52 T-B P1: pre-compute incoming labeled edge counts for merge nodes.
    let incoming_labeled = incoming_labeled_edge_counts(manifest);

    // V1.56 P2 fix-wave (H-001): pre-compute converge predecessor sets.
    // Count which states can route to each converge-annotated state.
    let mut converge_predecessors: HashMap<&str, std::collections::HashSet<&str>> = HashMap::new();
    for state in &manifest.states {
        let pred_id = state.id.as_str();
        // Collect all possible target states from this state's next.
        let mut targets: Vec<&str> = Vec::new();
        match &state.next {
            Some(NextTarget::Linear(ref next_id)) => targets.push(next_id),
            Some(NextTarget::GoNogo(ref go_nogo)) => {
                targets.push(&go_nogo.go);
                targets.push(&go_nogo.nogo);
            }
            Some(NextTarget::Labeled(ref edges)) => {
                for edge in edges {
                    targets.push(&edge.target);
                }
            }
            Some(NextTarget::Conditional(ref cond)) => {
                for rule in &cond.rules {
                    targets.push(&rule.target);
                }
                targets.push(&cond.default);
            }
            Some(NextTarget::Branches(ref branches)) => {
                for rule in &branches.branches {
                    targets.push(&rule.target);
                }
                targets.push(&branches.default);
            }
            None => {}
        }
        for target in targets {
            // Check if target is a converge state.
            let is_converge = manifest
                .states
                .iter()
                .any(|s| s.id == target && s.converge.is_some());
            if is_converge {
                converge_predecessors
                    .entry(target)
                    .or_default()
                    .insert(pred_id);
            }
        }
    }

    for state in &manifest.states {
        let incoming = *incoming_labeled.get(state.id.as_str()).unwrap_or(&0);
        let preds: std::collections::HashSet<String> = converge_predecessors
            .get(state.id.as_str())
            .map(|hs| hs.iter().map(std::string::ToString::to_string).collect())
            .unwrap_or_default();
        let task = StateCompositeTask::from_manifest(state)
            .with_expected_incoming(incoming)
            .with_converge_predecessors(preds);
        builder = builder.add_task(std::sync::Arc::new(task));
    }

    // Wire edges from state.next.
    for state in &manifest.states {
        match &state.next {
            Some(NextTarget::Linear(ref next_id)) => {
                builder = builder.add_edge(&state.id, next_id);
            }
            Some(NextTarget::GoNogo(ref go_nogo)) => {
                // V1.42 P2: conditional edge reads _judge_result from context.
                // `go` branch when true; `nogo` branch when false or absent.
                builder = builder.add_conditional_edge(
                    &state.id,
                    |ctx| ctx.get::<bool>("_judge_result").unwrap_or(false),
                    &go_nogo.go,
                    &go_nogo.nogo,
                );
            }
            Some(NextTarget::Labeled(ref labeled_edges)) => {
                // V1.52 T-B P0: N-way labeled routing.
                // Each labeled edge gets a regular add_edge for reachability
                // validation. Actual routing is via NextAction::GoTo(target)
                // in StateCompositeTask::resolve_labeled_target, which also
                // writes the matched label to context._judge_label.
                for edge in labeled_edges {
                    builder = builder.add_edge(&state.id, &edge.target);
                }
            }
            Some(NextTarget::Conditional(_) | NextTarget::Branches(_)) | None => {}
        }
    }

    builder.build()
}

/// Build the outer graph with engine + inner graph references wired into
/// composite tasks (for `start_session_with_preset`).
///
/// `daemon_tool_dispatch` is passed by value because it's cloned into each
/// composite task that contains `HostTool` enter actions.
///
/// `prompt_executor` / `session_cancels` (A1): the production prompt executor
/// and per-run coordinator cancellation tokens are wired into every inner
/// graph `acp_prompt` node at wired-graph build time — the loader-only
/// graphs stay executor-free for validation/test construction.
///
/// # Errors
/// Returns `graph_flow::GraphError` when an inner graph fails to build or the
/// outer graph topology is invalid.
#[allow(clippy::needless_pass_by_value, clippy::implicit_hasher)]
pub fn build_wired_outer_graph(
    loaded: &LoadedPreset,
    engine: &Arc<dyn crate::engine::OrchestrationEngine>,
    caps: &Arc<CapabilityRegistry>,
    daemon_tool_dispatch: Option<std::sync::Arc<dyn crate::capability::DaemonToolDispatch>>,
    prompt_executor: Option<std::sync::Arc<dyn crate::capability::PromptExecutor>>,
    session_cancels: std::sync::Arc<
        std::sync::RwLock<std::collections::HashMap<String, tokio_util::sync::CancellationToken>>,
    >,
) -> Result<graph_flow::Graph, graph_flow::GraphError> {
    use crate::tasks::StateCompositeTask;
    use std::collections::HashMap;

    let mut builder = graph_flow::GraphBuilder::new(&loaded.id);

    // A1: rebuild the inner graphs with the production prompt executor and
    // cancellation tokens wired into every `acp_prompt` node. Template files
    // resolve to actual content from the frozen source identity (N-7).
    let inner_graphs = build_inner_graphs(
        &loaded.manifest,
        &loaded.id,
        loaded.source_identity.as_ref(),
        prompt_executor,
        session_cancels,
    )?;

    // V1.52 T-B P1: pre-compute incoming labeled edge counts for merge nodes.
    // W-2 (v1.179 QC fix round 2): production wiring shares the exact scan
    // used by `validate_manifest` and `build_outer_graph`, keeping the
    // "loader accepts ⇔ runtime gates" parity invariant total.
    let incoming_labeled = incoming_labeled_edge_counts(&loaded.manifest);

    // V1.56 P2 fix-wave (H-001): pre-compute converge predecessor sets.
    let mut converge_predecessors: HashMap<&str, std::collections::HashSet<&str>> = HashMap::new();
    for state in &loaded.manifest.states {
        let pred_id = state.id.as_str();
        let mut targets: Vec<&str> = Vec::new();
        match &state.next {
            Some(NextTarget::Linear(ref next_id)) => targets.push(next_id),
            Some(NextTarget::GoNogo(ref go_nogo)) => {
                targets.push(&go_nogo.go);
                targets.push(&go_nogo.nogo);
            }
            Some(NextTarget::Labeled(ref edges)) => {
                for edge in edges {
                    targets.push(&edge.target);
                }
            }
            Some(NextTarget::Conditional(ref cond)) => {
                for rule in &cond.rules {
                    targets.push(&rule.target);
                }
                targets.push(&cond.default);
            }
            Some(NextTarget::Branches(ref branches)) => {
                for rule in &branches.branches {
                    targets.push(&rule.target);
                }
                targets.push(&branches.default);
            }
            None => {}
        }
        for target in targets {
            let is_converge = loaded
                .manifest
                .states
                .iter()
                .any(|s| s.id == target && s.converge.is_some());
            if is_converge {
                converge_predecessors
                    .entry(target)
                    .or_default()
                    .insert(pred_id);
            }
        }
    }

    for state in &loaded.manifest.states {
        let incoming = *incoming_labeled.get(state.id.as_str()).unwrap_or(&0);
        let preds: std::collections::HashSet<String> = converge_predecessors
            .get(state.id.as_str())
            .map(|hs| hs.iter().map(std::string::ToString::to_string).collect())
            .unwrap_or_default();
        let mut task = StateCompositeTask::from_manifest(state)
            .with_resolved_template(&loaded.id)
            .with_expected_incoming(incoming)
            .with_converge_predecessors(preds)
            .with_engine(engine.clone())
            .with_inner_graphs(inner_graphs.clone())
            .with_output_bindings(loaded.output_bindings.clone())
            .with_registry(caps.clone());

        // v1.188 P3: resolve `_context.workspace.*` from the engine's live
        // workspace state provider, so preset conditional edges see real
        // durable workspace state instead of a synthetic default.
        if let Some(provider) = engine.workspace_state_provider() {
            task = task.with_workspace_state_provider(provider);
        }

        // Wire daemon tool dispatch for HostTool enter actions (DF-47, V1.42 P3).
        if let Some(ref dispatch) = daemon_tool_dispatch {
            task = task.with_daemon_tool_dispatch(dispatch.clone());
        }

        builder = builder.add_task(std::sync::Arc::new(task));
    }

    // Wire edges.
    for state in &loaded.manifest.states {
        match &state.next {
            Some(NextTarget::Linear(ref next_id)) => {
                builder = builder.add_edge(&state.id, next_id);
            }
            Some(NextTarget::GoNogo(ref go_nogo)) => {
                builder = builder.add_conditional_edge(
                    &state.id,
                    |ctx| ctx.get::<bool>("_judge_result").unwrap_or(false),
                    &go_nogo.go,
                    &go_nogo.nogo,
                );
            }
            Some(NextTarget::Labeled(ref labeled_edges)) => {
                // V1.52 T-B P0: N-way labeled routing.
                for edge in labeled_edges {
                    builder = builder.add_edge(&state.id, &edge.target);
                }
            }
            Some(NextTarget::Conditional(_) | NextTarget::Branches(_)) | None => {}
        }
    }

    builder.build()
}
/// Build inner graphs per §8.2.
///
/// `inner_graphs.<name>.nodes[].kind=acp_prompt` → `AcpPromptTask`.
/// `inner_graphs.<name>.nodes[].depends_on` → `add_edge`.
///
/// ## WS-E T5: agent field propagation
///
/// Each node's `agent` field (if present) is stored in `InnerGraphNodeTask::agent_ref`.
/// At runtime the engine seeds the trusted child `_session_id` context value,
/// which `InnerGraphNodeTask::resolve_session_id` uses as the durable run
/// identity for the Host prompt route.
///
/// ## A1: prompt executor wiring
///
/// The production `PromptExecutor` and the per-run coordinator cancellation
/// tokens are wired into every `acp_prompt` node so graph prompt execution
/// goes through the Host plane — never an echo/worker fallback.
#[allow(clippy::implicit_hasher, clippy::needless_pass_by_value)] // DefaultHasher inner-graph map; owned executor/cancels args mirror the wired-graph API
fn build_inner_graphs(
    manifest: &PresetManifest,
    preset_id: &str,
    source_identity: Option<&nexus_preset::source_identity::PresetSourceIdentity>,
    prompt_executor: Option<std::sync::Arc<dyn crate::capability::PromptExecutor>>,
    session_cancels: std::sync::Arc<
        std::sync::RwLock<std::collections::HashMap<String, tokio_util::sync::CancellationToken>>,
    >,
) -> Result<HashMap<String, Arc<graph_flow::Graph>>, graph_flow::GraphError> {
    use nexus_preset::manifest::GraphNodeKind;
    use crate::tasks::InnerGraphNodeTask;

    let mut result = HashMap::new();

    if let Some(ref inner_graphs) = manifest.inner_graphs {
        for (name, ig) in inner_graphs {
            let mut builder = graph_flow::GraphBuilder::new(name);

            for node in &ig.nodes {
                // Determine kind (currently only acp_prompt supported).
                let task = match node.kind {
                    GraphNodeKind::AcpPrompt => {
                        // Resolve the template_file to ACTUAL content at build
                        // time (N-7): the Host must receive the rendered
                        // prompt, never the raw bundle-relative path. Embedded
                        // presets read from the compiled-in bundle; directory
                        // presets read from their resolved root. A missing
                        // template fails closed (the node refuses at runtime
                        // with a typed error rather than prompting with a path).
                        let template = node
                            .template_file
                            .as_deref()
                            .and_then(|path| match source_identity {
                                Some(nexus_preset::source_identity::PresetSourceIdentity::Embedded {
                                    preset_id: pid,
                                    ..
                                }) => nexus_preset::read_embedded_template(pid, path),
                                Some(nexus_preset::source_identity::PresetSourceIdentity::Directory {
                                    root,
                                    ..
                                }) => std::fs::read_to_string(root.join(path)).ok(),
                                None => nexus_preset::read_embedded_template(preset_id, path),
                            })
                            .unwrap_or_default();
                        let mut task = InnerGraphNodeTask::new(&node.id)
                            // Tool policy from node (parse from string)
                            .with_tool_policy(
                                node.tool_policy
                                    .as_ref()
                                    .and_then(|s| std::str::FromStr::from_str(s.as_str()).ok())
                                    .unwrap_or(crate::tasks::ToolPolicy::AutoGrantReadOnly),
                            )
                            // Resolved template content (rendered at runtime)
                            .with_template(template)
                            // A1: production prompt executor + cancellation tokens
                            .with_prompt_executor(prompt_executor.clone())
                            .with_session_cancels(session_cancels.clone());
                        // I-003: preserve agent absence as `None` so the
                        // executor selects the `default` role binding; never
                        // synthesize `Some("")`.
                        if let Some(agent_ref) = &node.agent {
                            task = task.with_agent_ref(agent_ref.clone());
                        }
                        task
                    }
                };
                builder = builder.add_task(std::sync::Arc::new(task));
            }

            // Wire edges from depends_on
            for node in &ig.nodes {
                for dep in &node.depends_on {
                    builder = builder.add_edge(dep, &node.id);
                }
            }

            // Terminate the inner graph explicitly. graph-flow reports
            // `ExecutionStatus::Completed` ONLY when a task returns
            // `NextAction::End`; a node's `NextAction::Continue` with no
            // outgoing edge instead parks the run at
            // `Paused { reason: "No outgoing edge found from current task" }`
            // while leaving `current_task_id` unchanged. `InnerGraphTask`
            // treats a paused child as "resume and keep stepping", so a sink
            // node would be re-executed up to its 256-poll bound — re-running
            // the node's external prompt every time and never letting the
            // parent step finish. Sink nodes are exactly the nodes that no
            // `depends_on` edge targets, so an end node plus one edge per sink
            // makes the child reach `Completed` after its real work.
            // A graph that already declares a node named `end` owns that id;
            // registering the terminal task would silently REPLACE that node.
            // Such a graph keeps its authored topology.
            let declares_end = ig.nodes.iter().any(|node| node.id == "end");
            if !declares_end {
                let depended_upon: std::collections::HashSet<&str> = ig
                    .nodes
                    .iter()
                    .flat_map(|node| node.depends_on.iter().map(std::string::String::as_str))
                    .collect();
                let sinks: Vec<&str> = ig
                    .nodes
                    .iter()
                    .map(|node| node.id.as_str())
                    .filter(|id| !depended_upon.contains(id))
                    .collect();
                builder = builder.add_task(std::sync::Arc::new(
                    crate::system_preset::EndTask::new(format!("inner graph '{name}' completed")),
                ));
                for sink in sinks {
                    builder = builder.add_edge(sink, "end");
                }
            }

            result.insert(name.clone(), Arc::new(builder.build()?));
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_preset::load_preset_from_str;

    fn test_capability_registry() -> CapabilityRegistry {
        CapabilityRegistry::with_builtins()
    }

    /// Engine stub for wiring-only tests: every method panics. The
    /// merge-gate wait path returns before any engine call, so a panic
    /// here would mean the test drove the wrong code path.
    struct UnreachableEngine;

    #[async_trait::async_trait]
    impl crate::engine::OrchestrationEngine for UnreachableEngine {
        async fn run_step(
            &self,
            _: &crate::engine::SessionId,
        ) -> Result<crate::engine::StepOutcome, crate::engine::EngineError> {
            unimplemented!("wiring test must not execute engine steps")
        }
        async fn new_session(
            &self,
            _: crate::engine::SessionKey,
            _: crate::engine::Context,
        ) -> Result<crate::engine::SessionId, crate::engine::EngineError> {
            unimplemented!()
        }
        async fn start_session_with_graph(
            &self,
            _: &str,
            _: std::sync::Arc<graph_flow::Graph>,
        ) -> Result<crate::engine::SessionId, crate::engine::EngineError> {
            unimplemented!()
        }
        async fn get_status(
            &self,
            _: &crate::engine::SessionId,
        ) -> Result<crate::engine::SessionStatus, crate::engine::EngineError> {
            unimplemented!()
        }
        async fn signal(
            &self,
            _: &crate::engine::SessionId,
            _: crate::engine::EngineSignal,
        ) -> Result<(), crate::engine::EngineError> {
            unimplemented!()
        }
        async fn list_active(
            &self,
            _: crate::engine::SessionFilter,
        ) -> Result<Vec<crate::engine::SessionSummary>, crate::engine::EngineError> {
            unimplemented!()
        }
        async fn spawn_child_session(
            &self,
            _: crate::engine::ChildSessionParams,
        ) -> Result<crate::engine::SessionId, crate::engine::EngineError> {
            unimplemented!()
        }
        async fn attach_existing_child_session(
            &self,
            _: &str,
            _: std::sync::Arc<graph_flow::Graph>,
        ) -> Result<Option<crate::engine::SessionId>, crate::engine::EngineError> {
            unimplemented!("wiring test must not execute engine steps")
        }
        async fn start_session_with_preset(
            &self,
            _: &LoadedPreset,
        ) -> Result<crate::engine::SessionId, crate::engine::EngineError> {
            unimplemented!()
        }
        async fn get_context(
            &self,
            _: &crate::engine::SessionId,
        ) -> Result<graph_flow::Context, crate::engine::EngineError> {
            unimplemented!()
        }
        async fn get_current_task_id(
            &self,
            _: &crate::engine::SessionId,
        ) -> Result<Option<String>, crate::engine::EngineError> {
            unimplemented!("wiring test must not query session cursors")
        }
        async fn has_runner(&self, _: &crate::engine::SessionId) -> bool {
            unimplemented!("wiring test must not query runner existence")
        }
        async fn recover_sessions(&self, _: Vec<crate::engine::SessionSummary>) {
            unimplemented!("wiring test must not recover sessions")
        }
        async fn ensure_recovered_runner(
            &self,
            _: &crate::engine::SessionId,
        ) -> Result<(), crate::engine::EngineError> {
            unimplemented!("wiring test must not reattach runners")
        }
        async fn start_session_with_preset_for_creator(
            &self,
            _: &LoadedPreset,
            _: &str,
        ) -> Result<crate::engine::SessionId, crate::engine::EngineError> {
            unimplemented!()
        }
    }

    #[tokio::test]
    async fn wired_builder_gates_implicit_merge_like_validate_path() {
        // W-2 (v1.179 QC fix round 2): the production builder
        // `build_wired_outer_graph` must wire the same `expected_incoming`
        // the loader's bounded-join validation accepts — both now scan
        // through `incoming_labeled_edge_counts`. Fixture: implicit
        // labeled-edge merge (no `merge:`) with two incoming edges; a
        // single arrival must hold the wired gate at 1/2.
        let yaml = r"
preset:
  id: wired-merge-parity
  version: 1
  kind: creator
  description: test
  requires_capabilities: []
  initial: j1
  terminal: done
states:
  - id: j1
    exit_when: { kind: rule }
    next: j2
  - id: j2
    exit_when: { kind: llm_judge }
    next:
      - label: go
        target: m
      - label: research
        target: m
  - id: m
    next: done
  - id: done
    terminal: true
";
        let caps = test_capability_registry();
        let loaded = load_preset_from_str(yaml, &caps).expect("implicit labeled-edge merge loads");

        let engine: Arc<dyn crate::engine::OrchestrationEngine> = Arc::new(UnreachableEngine);
        let graph = build_wired_outer_graph(
            &loaded,
            &engine,
            &Arc::new(caps),
            None,
            None,
            std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
        )
        .expect("wired outer graph builds");

        let task = graph
            .get_task("m")
            .expect("merge state task present in wired graph");
        let ctx = graph_flow::Context::new();
        ctx.set("_merge_m", vec!["go".to_string()])
            .expect("context set");
        let result = task.run(ctx).await.expect("merge gate check runs");

        assert_eq!(
            result.next_action,
            graph_flow::NextAction::WaitForInput,
            "one arrival of two must leave the wired merge gate waiting"
        );
        let response = result
            .response
            .expect("waiting merge node returns a message");
        assert!(
            response.contains("1/2 arrivals, waiting"),
            "wired builder must wire expected_incoming=2 from the shared \
             incoming_labeled_edge_counts scan; got: {response}"
        );
    }

}

    #[test]
    fn gonogo_routing_uses_judge_result() {
        let manifest: PresetManifest = serde_yaml::from_str(r"
preset:
  id: routing
  version: 1
  kind: creator
  description: Judge routing regression
  initial: judge
  terminal: done
states:
  - id: judge
    exit_when: { kind: llm_judge }
    next: { go: done, nogo: retry }
  - id: retry
    next: judge
  - id: done
    terminal: true
").unwrap();
        let graph = build_outer_graph(&manifest).unwrap();
        let context = graph_flow::Context::new();
        assert_eq!(graph.find_next_task("judge", &context).as_deref(), Some("retry"));
        context.set("_judge_result", true).unwrap();
        assert_eq!(graph.find_next_task("judge", &context).as_deref(), Some("done"));
        context.set("_judge_result", false).unwrap();
        assert_eq!(graph.find_next_task("judge", &context).as_deref(), Some("retry"));
    }
