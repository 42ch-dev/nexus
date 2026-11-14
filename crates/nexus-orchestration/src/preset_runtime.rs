//! Execution-only preset graph construction and provider binding creation.

use crate::capability::CapabilityRegistry;
use nexus_preset::loader::incoming_labeled_edge_counts;
use nexus_preset::manifest::{NextTarget, PresetManifest};
use nexus_preset::{load_embedded_preset, required_prompt_roles, LoadedPreset};
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
///
/// # Errors
///
/// Returns `graph_flow::GraphError` when the manifest's states/edges cannot
/// form a valid graph (unknown edge endpoints, malformed composite task).
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
            .map_or_default(|hs| hs.iter().map(std::string::ToString::to_string).collect());
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
            .map_or_default(|hs| hs.iter().map(std::string::ToString::to_string).collect());
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

/// Why an IR document cannot become an outer graph.
#[derive(Debug, thiserror::Error)]
pub enum IrOuterGraphError {
    /// The document carries a concept with no §8.2.1 outer-graph projection.
    #[error(transparent)]
    Projection(#[from] nexus_preset::WorkflowIrError),
    /// The projected topology is not a valid graph.
    #[error(transparent)]
    Graph(#[from] graph_flow::GraphError),
}

/// Build the wired outer graph from a Workflow IR document (§8.2.1).
///
/// Rows this builder covers: `entry`, `nodes[kind=outer_state|terminal]` and
/// their enter actions (`capability`, `host_tool`, `inner_graph`), every
/// `exit_when` kind, linear/go/nogo/labeled edges, `edges[kind=conditional]`
/// (routing stays with the source task's expression evaluation, exactly as the
/// YAML path leaves it), `terminal[]`/`nodes[kind=terminal]`, and the
/// `merge`/`converge`/`timeout_ms`/`on_timeout` join gate.
///
/// The document is projected onto the manifest's state carriers by
/// [`nexus_preset::export_preset`], so both intakes reach the same
/// [`crate::tasks::StateCompositeTask`] constructor: no second task type and no
/// second validation pass. Inner graphs and output bindings are not wired here
/// (P35 Task 2) — a state whose enter action launches one builds, and that
/// action fails closed at run time until the graph is attached.
///
/// # Errors
/// `Projection` when the document carries a concept the outer graph cannot
/// express; `Graph` when the wired topology is invalid (an `entry` naming no
/// outer task, or an edge to a task the projection never produced).
#[allow(clippy::needless_pass_by_value)] // dispatch is cloned into each task, mirroring build_wired_outer_graph
pub fn build_wired_outer_graph_from_ir(
    document: &nexus_contracts::generated::daemon_api::orchestration::WorkflowDocument,
    engine: &Arc<dyn crate::engine::OrchestrationEngine>,
    caps: &Arc<CapabilityRegistry>,
    daemon_tool_dispatch: Option<std::sync::Arc<dyn crate::capability::DaemonToolDispatch>>,
) -> Result<graph_flow::Graph, IrOuterGraphError> {
    use crate::tasks::StateCompositeTask;

    let mut manifest = nexus_preset::export_preset(document)?;

    // §8.2.1 `terminal[]`: a declared terminal state ends the run even when its
    // node kind is `outer_state`; the composite task overrides with `End`.
    for state in &mut manifest.states {
        state.terminal |= document.terminal.iter().any(|id| id == &state.id);
    }

    let mut builder = graph_flow::GraphBuilder::new(&manifest.preset.id);

    // Same merge/converge pre-scans the YAML builders run, so both intakes
    // hand the composite task identical join gates.
    let incoming_labeled = incoming_labeled_edge_counts(&manifest);
    let converge_predecessors = converge_predecessor_sets(&manifest);

    for state in &manifest.states {
        let incoming = *incoming_labeled.get(state.id.as_str()).unwrap_or(&0);
        let preds = converge_predecessors
            .get(state.id.as_str())
            .cloned()
            .unwrap_or_default();
        let mut task = StateCompositeTask::from_manifest(state)
            .with_expected_incoming(incoming)
            .with_converge_predecessors(preds)
            .with_engine(engine.clone())
            .with_registry(caps.clone());

        if let Some(provider) = engine.workspace_state_provider() {
            task = task.with_workspace_state_provider(provider);
        }
        if let Some(dispatch) = &daemon_tool_dispatch {
            task = task.with_daemon_tool_dispatch(dispatch.clone());
        }

        builder = builder.add_task(std::sync::Arc::new(task));
    }

    // §8.2.1 `entry`: the declared entry node is the initial outer task.
    builder = builder.set_start_task(&document.entry);

    // Transitions — the same wiring as `build_wired_outer_graph`; the parity
    // projection below fails if the two ever diverge.
    for state in &manifest.states {
        match &state.next {
            Some(NextTarget::Linear(next_id)) => {
                builder = builder.add_edge(&state.id, next_id);
            }
            Some(NextTarget::GoNogo(go_nogo)) => {
                builder = builder.add_conditional_edge(
                    &state.id,
                    |ctx| ctx.get::<bool>("_judge_result").unwrap_or(false),
                    &go_nogo.go,
                    &go_nogo.nogo,
                );
            }
            Some(NextTarget::Labeled(labeled_edges)) => {
                for edge in labeled_edges {
                    builder = builder.add_edge(&state.id, &edge.target);
                }
            }
            Some(NextTarget::Conditional(_) | NextTarget::Branches(_)) | None => {}
        }
    }

    Ok(builder.build()?)
}

/// Converge-predecessor sets for §8.2.1's join gate.
fn converge_predecessor_sets(
    manifest: &PresetManifest,
) -> HashMap<&str, std::collections::HashSet<String>> {
    let converge: std::collections::HashSet<&str> = manifest
        .states
        .iter()
        .filter(|state| state.converge.is_some())
        .map(|state| state.id.as_str())
        .collect();

    let mut predecessors: HashMap<&str, std::collections::HashSet<String>> = HashMap::new();
    for state in &manifest.states {
        let mut targets: Vec<&str> = Vec::new();
        match &state.next {
            Some(NextTarget::Linear(next_id)) => targets.push(next_id),
            Some(NextTarget::GoNogo(go_nogo)) => {
                targets.push(&go_nogo.go);
                targets.push(&go_nogo.nogo);
            }
            Some(NextTarget::Labeled(edges)) => {
                targets.extend(edges.iter().map(|edge| edge.target.as_str()));
            }
            Some(NextTarget::Conditional(cond)) => {
                targets.extend(cond.rules.iter().map(|rule| rule.target.as_str()));
                targets.push(&cond.default);
            }
            Some(NextTarget::Branches(branches)) => {
                targets.extend(branches.branches.iter().map(|rule| rule.target.as_str()));
                targets.push(&branches.default);
            }
            None => {}
        }
        for target in targets {
            if converge.contains(target) {
                predecessors
                    .entry(target)
                    .or_default()
                    .insert(state.id.clone());
            }
        }
    }
    predecessors
}

/// Normalized outer-graph description for the IR ↔ YAML parity tests.
///
/// Graphs cannot be compared byte-wise — tasks hold `Arc` handles, closures and
/// injected executors — so parity compares what the runner can observe about
/// the shape. Edges describe the routing a source task follows, including the
/// conditional branches the graph builder leaves to expression evaluation
/// instead of wiring as graph edges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OuterGraphProjection {
    /// Graph id.
    pub graph_id: String,
    /// `entry` (§8.2.1): the outer task the graph starts at.
    pub entry: Option<String>,
    /// Outer tasks in build order.
    pub tasks: Vec<ProjectedTask>,
    /// Routing edges as `(from, to, kind, label-or-condition)`.
    pub edges: Vec<ProjectedEdge>,
    /// Tasks that end the run: `terminal[]` and `nodes[kind=terminal]`.
    pub terminals: Vec<String>,
}

/// One outer task in an [`OuterGraphProjection`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectedTask {
    /// `Task::id()`.
    pub id: String,
    /// `"terminal"` when the task returns `NextAction::End`, else `"outer_state"`.
    pub kind: &'static str,
    /// Enter actions in declared order, in wire shape.
    pub enter: Vec<serde_json::Value>,
    /// Exit condition in wire shape (`rule`, `llm_judge`, `manual`, `graph_complete`, `timer`).
    pub exit_when: Option<serde_json::Value>,
    /// Join gate (`merge`, `converge`, `timeout_ms`, `on_timeout`); `None` for non-join states.
    pub join: Option<serde_json::Value>,
    /// Exit context update in wire shape.
    pub context_update: Option<serde_json::Value>,
}

/// One routing edge in an [`OuterGraphProjection`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectedEdge {
    /// Source task id.
    pub from: String,
    /// Target task id.
    pub to: String,
    /// `linear`, `go`, `nogo`, `labeled`, `conditional` or `default`.
    pub kind: &'static str,
    /// Edge label, or the `when` expression of a conditional branch.
    pub detail: Option<String>,
}

/// Project a built outer graph plus the states it was built from into the
/// normalized shape parity tests compare.
///
/// # Panics
/// Panics when the graph is missing a task for a projected state — the builder
/// and the projection disagree about the topology.
#[must_use]
pub fn outer_graph_projection(
    graph: &graph_flow::Graph,
    manifest: &PresetManifest,
) -> OuterGraphProjection {
    let mut tasks = Vec::with_capacity(manifest.states.len());
    let mut edges = Vec::new();
    let mut terminals = Vec::new();

    for state in &manifest.states {
        assert!(
            graph.get_task(&state.id).is_some(),
            "state `{}` has no task in graph `{}`",
            state.id,
            graph.id
        );
        if state.terminal {
            terminals.push(state.id.clone());
        }
        let join = (state.merge.is_some()
            || state.converge.is_some()
            || state.timeout_ms.is_some()
            || state.on_timeout.is_some())
        .then(|| {
            serde_json::json!({
                "merge": state.merge.as_ref().map(wire),
                "converge": state.converge.as_ref().map(wire),
                "timeout_ms": state.timeout_ms,
                "on_timeout": state.on_timeout,
            })
        });
        tasks.push(ProjectedTask {
            id: state.id.clone(),
            kind: if state.terminal {
                "terminal"
            } else {
                "outer_state"
            },
            enter: state.enter.iter().map(wire).collect(),
            exit_when: state.exit_when.as_ref().map(wire),
            join,
            context_update: state.context_update.as_ref().map(wire),
        });

        let mut push = |to: &str, kind: &'static str, detail: Option<String>| {
            edges.push(ProjectedEdge {
                from: state.id.clone(),
                to: to.to_string(),
                kind,
                detail,
            });
        };
        match &state.next {
            Some(NextTarget::Linear(next_id)) => push(next_id, "linear", None),
            Some(NextTarget::GoNogo(go_nogo)) => {
                push(&go_nogo.go, "go", None);
                push(&go_nogo.nogo, "nogo", None);
            }
            Some(NextTarget::Labeled(labeled)) => {
                for edge in labeled {
                    push(&edge.target, "labeled", Some(edge.label.clone()));
                }
            }
            Some(NextTarget::Conditional(cond)) => {
                for rule in &cond.rules {
                    push(&rule.target, "conditional", Some(rule.when.clone()));
                }
                push(&cond.default, "default", None);
            }
            Some(NextTarget::Branches(branches)) => {
                for rule in &branches.branches {
                    push(&rule.target, "conditional", Some(rule.when.clone()));
                }
                push(&branches.default, "default", None);
            }
            None => {}
        }
    }

    OuterGraphProjection {
        graph_id: graph.id.clone(),
        entry: graph.start_task_id().map(ToOwned::to_owned),
        tasks,
        edges,
        terminals,
    }
}

/// Wire shape of a manifest carrier (plain data, so serialization cannot fail).
fn wire<T: serde::Serialize>(value: &T) -> serde_json::Value {
    serde_json::to_value(value).expect("preset manifest carriers are plain serializable data")
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
    use crate::tasks::InnerGraphNodeTask;
    use nexus_preset::manifest::GraphNodeKind;

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
    use nexus_contracts::generated::daemon_api::orchestration::WorkflowDocument;
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

    /// A hand-written IR document: capability enter → manual wait → a
    /// conditional/default split with a fixup state → exactly one terminal.
    const IR_LINEAR: &str = r#"{
        "ir_version": 0,
        "id": "ir-linear",
        "version": 1,
        "revision": 0,
        "state": "candidate",
        "meta": {
            "description": "IR outer builder fixture",
            "kind": "creator",
            "run_intents": [],
            "requires_capabilities": []
        },
        "entry": "plan",
        "terminal": ["done"],
        "nodes": [
            {"id": "plan", "kind": "outer_state", "effect_class": "external",
             "enter": [{"kind": "capability", "name": "narrative.compute"}],
             "exit_when": {"kind": "rule"}},
            {"id": "review", "kind": "outer_state", "effect_class": "read",
             "exit_when": {"kind": "rule"}},
            {"id": "fixup", "kind": "outer_state", "effect_class": "pure",
             "exit_when": {"kind": "manual"}},
            {"id": "done", "kind": "terminal", "effect_class": "pure"}
        ],
        "edges": [
            {"id": "plan->review", "from": "plan", "to": "review", "kind": "linear"},
            {"id": "review->done", "from": "review", "to": "done", "kind": "conditional",
             "when": "_context.score > 1"},
            {"id": "review->fixup", "from": "review", "to": "fixup", "kind": "default"},
            {"id": "fixup->done", "from": "fixup", "to": "done", "kind": "linear"}
        ],
        "nests": [],
        "assets": [],
        "signals": [],
        "roles": [],
        "effect_summary": "external",
        "provenance": {"source": "compose"}
    }"#;

    /// `terminal[]` names an `outer_state` node: the declaration, not the node
    /// kind, is what makes the task end the run.
    const IR_TERMINAL_DECLARATION: &str = r#"{
        "ir_version": 0,
        "id": "ir-terminal-declaration",
        "version": 1,
        "revision": 0,
        "state": "candidate",
        "meta": {
            "description": "terminal[] on an outer_state",
            "kind": "creator",
            "run_intents": [],
            "requires_capabilities": []
        },
        "entry": "only",
        "terminal": ["only"],
        "nodes": [{"id": "only", "kind": "outer_state", "effect_class": "pure"}],
        "edges": [],
        "nests": [],
        "assets": [],
        "signals": [],
        "roles": [],
        "effect_summary": "pure",
        "provenance": {"source": "compose"}
    }"#;

    fn ir_document(text: &str) -> WorkflowDocument {
        serde_json::from_str(text).expect("fixture is a valid workflow document")
    }

    fn ir_outer_graph(document: &WorkflowDocument) -> graph_flow::Graph {
        let caps = Arc::new(test_capability_registry());
        let engine: Arc<dyn crate::engine::OrchestrationEngine> = Arc::new(UnreachableEngine);
        build_wired_outer_graph_from_ir(document, &engine, &caps, None)
            .expect("IR outer graph builds")
    }

    #[test]
    fn ir_outer_builder_projects_states_edges_and_terminal() {
        let document = ir_document(IR_LINEAR);
        let graph = ir_outer_graph(&document);
        let manifest =
            nexus_preset::export_preset(&document).expect("document projects onto states");
        let projection = outer_graph_projection(&graph, &manifest);

        assert_eq!(projection.graph_id, "ir-linear");
        assert_eq!(
            projection.entry.as_deref(),
            Some("plan"),
            "§8.2.1 entry selects the initial outer task"
        );
        assert_eq!(
            projection
                .tasks
                .iter()
                .map(|task| (task.id.as_str(), task.kind))
                .collect::<Vec<_>>(),
            [
                ("plan", "outer_state"),
                ("review", "outer_state"),
                ("fixup", "outer_state"),
                ("done", "terminal")
            ]
        );
        assert_eq!(
            projection.tasks[0].enter,
            [serde_json::json!({"kind": "capability", "name": "narrative.compute"})],
            "§8.2.1 capability enter action is carried by the owning state task"
        );
        assert_eq!(
            projection.tasks[2].exit_when,
            Some(serde_json::json!({"kind": "manual"})),
            "§8.2.1 manual exit_when"
        );
        assert_eq!(
            projection
                .edges
                .iter()
                .map(|edge| (
                    edge.from.as_str(),
                    edge.to.as_str(),
                    edge.kind,
                    edge.detail.as_deref()
                ))
                .collect::<Vec<_>>(),
            [
                ("plan", "review", "linear", None),
                ("review", "done", "conditional", Some("_context.score > 1")),
                ("review", "fixup", "default", None),
                ("fixup", "done", "linear", None)
            ]
        );
        assert_eq!(projection.terminals, ["done"], "§8.2.1 terminal[]");
    }

    #[tokio::test]
    async fn ir_exit_when_manual_waits_and_terminal_declaration_ends() {
        let fixup = ir_outer_graph(&ir_document(IR_LINEAR))
            .get_task("fixup")
            .expect("manual state task is wired");
        assert_eq!(
            fixup
                .run(graph_flow::Context::new())
                .await
                .expect("manual state runs")
                .next_action,
            graph_flow::NextAction::WaitForInput,
            "§8.2.1 manual exit_when returns WaitForInput"
        );

        let only = ir_outer_graph(&ir_document(IR_TERMINAL_DECLARATION))
            .get_task("only")
            .expect("declared terminal task is wired");
        assert_eq!(
            only.run(graph_flow::Context::new())
                .await
                .expect("terminal state runs")
                .next_action,
            graph_flow::NextAction::End,
            "§8.2.1 terminal[] makes an outer_state return End"
        );
    }
}

#[test]
fn gonogo_routing_uses_judge_result() {
    let manifest: PresetManifest = serde_yaml::from_str(
        r"
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
",
    )
    .unwrap();
    let graph = build_outer_graph(&manifest).unwrap();
    let context = graph_flow::Context::new();
    assert_eq!(
        graph.find_next_task("judge", &context).as_deref(),
        Some("retry")
    );
    context.set("_judge_result", true).unwrap();
    assert_eq!(
        graph.find_next_task("judge", &context).as_deref(),
        Some("done")
    );
    context.set("_judge_result", false).unwrap();
    assert_eq!(
        graph.find_next_task("judge", &context).as_deref(),
        Some("retry")
    );
}
