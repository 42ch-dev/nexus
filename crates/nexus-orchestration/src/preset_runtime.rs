//! Execution-only preset graph construction and provider binding creation.

use crate::capability::CapabilityRegistry;
use nexus_contracts::generated::daemon_api::orchestration::workflow_document::{
    WorkflowDocument, WorkflowNodeKind,
};
use nexus_preset::loader::{extract_output_bindings, incoming_labeled_edge_counts};
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
    /// The document uses a composition concept with no §8.2.1 executable row.
    #[error(
        "ir_unmappable: {concept} at {location} has no §8.2.1 execution row \
         (a composition concept expands into concrete nodes before execution)"
    )]
    Unmappable {
        /// The unmappable IR concept, by its document spelling.
        concept: String,
        /// Where it sits: a node id or a document path.
        location: String,
    },
    /// §8.2.1 defines the concept's execution counterpart, but this adapter
    /// has no manifest projection for it.
    #[error(
        "ir_unprojected: {concept} at {location} has no preset projection in the IR \
         adapter (nexus-preset project_nodes), although §8.2.1 defines its execution \
         counterpart"
    )]
    Unprojected {
        /// The unprojected IR concept, by its document spelling.
        concept: String,
        /// Where it sits: a node id or a document path.
        location: String,
    },
    /// An inner-step template resolves to nothing, so its task would prompt the
    /// Host with zero instructions.
    #[error(
        "ir_unresolved_template: inner-step node {node} template {template:?} resolves to \
         no content in the preset's embedded bundle; the step must render a non-empty prompt"
    )]
    UnresolvedTemplate {
        /// The `<inner_graph>/<step>` node id, as the document spells it.
        node: String,
        /// The bundle-relative template path the step names, if any.
        template: Option<String>,
    },
    /// The adapter refused a payload the outer mapping carries but cannot read
    /// (a malformed edge family or inner-step id), with no §8.2.1 concept named.
    #[error(transparent)]
    Projection(#[from] nexus_preset::WorkflowIrError),
    /// The projected topology is not a valid graph.
    #[error(transparent)]
    Graph(#[from] graph_flow::GraphError),
}

/// Refuse a document whose concepts have no projection into this adapter.
///
/// Runs before [`nexus_preset::export_preset`] and before any task is
/// constructed, so the caller gets the concept by name instead of the
/// adapter's projection refusal and no partially built graph escapes.
///
/// The refusal distinguishes two gaps. `judge` / `manual_wait` / `join` DO have
/// a §8.2.1 execution counterpart, but this adapter projects their carriers
/// (`exit_when`, `merge`, `converge`, `timeout_ms`, `on_timeout`) through
/// `nodes[kind=outer_state]` only, so the kind spelling is unprojected.
/// `pattern_slot` / `nest_group` have no execution row at all.
fn refuse_unmappable(document: &WorkflowDocument) -> Result<(), IrOuterGraphError> {
    for node in &document.nodes {
        let adapter_gap = match node.kind {
            WorkflowNodeKind::Judge
            | WorkflowNodeKind::ManualWait
            | WorkflowNodeKind::Join => true,
            WorkflowNodeKind::PatternSlot | WorkflowNodeKind::NestGroup => false,
            WorkflowNodeKind::OuterState
            | WorkflowNodeKind::InnerStep
            | WorkflowNodeKind::Terminal => continue,
        };
        let concept = format!("node kind {}", node.kind);
        let location = format!("node {}", node.id);
        return Err(if adapter_gap {
            IrOuterGraphError::Unprojected { concept, location }
        } else {
            IrOuterGraphError::Unmappable { concept, location }
        });
    }
    match document.terminal.as_slice() {
        [_] => Ok(()),
        terminals => Err(IrOuterGraphError::Unprojected {
            concept: "terminal declaration".to_string(),
            location: format!(
                "document.terminal ({} entries; a preset carries exactly one)",
                terminals.len()
            ),
        }),
    }
}

/// Refuse an inner-step template that resolves to no content.
///
/// A document carries no bundle root, so [`build_inner_graphs`] resolves every
/// `template` against the embedded bundle keyed by document id. A missing or
/// empty path would reach the Host as a zero-instruction prompt via
/// `unwrap_or_default()`, so it is refused here, by node and path, before any
/// task is built. The YAML intake needs no such check: its loader validates
/// assets against the bundle root at load time.
fn refuse_unresolved_templates(
    manifest: &PresetManifest,
    preset_id: &str,
) -> Result<(), IrOuterGraphError> {
    let Some(graphs) = &manifest.inner_graphs else {
        return Ok(());
    };
    for (name, graph) in graphs {
        for step in &graph.nodes {
            let template = step.template_file.as_deref();
            let resolved = template
                .and_then(|path| nexus_preset::read_embedded_template(preset_id, path))
                .is_some_and(|content| !content.trim().is_empty());
            if !resolved {
                return Err(IrOuterGraphError::UnresolvedTemplate {
                    // The document spells inner-step ids `<inner_graph>/<step>`.
                    node: format!("{name}/{}", step.id),
                    template: template.map(std::string::ToString::to_string),
                });
            }
        }
    }
    Ok(())
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
/// second validation pass.
///
/// `inner_graphs.<name>` reaches the composite tasks through the same
/// `build_inner_graphs` chain the YAML path uses (`AcpPromptTask` nodes,
/// `depends_on` edges, the sink `end` task, the production prompt executor and
/// per-run cancellation tokens), and `output_binding` through the same
/// [`extract_output_bindings`] derivation. The document names its
/// templates as bundle-relative asset paths and carries no bundle root, so the
/// embedded bundle keyed by document id is the only resolvable template source;
/// a step whose template resolves to nothing is refused
/// ([`IrOuterGraphError::UnresolvedTemplate`]) instead of prompting the Host
/// with zero instructions.
///
/// # Errors
/// `Unmappable` / `Unprojected` when the document uses a concept this adapter
/// cannot project (named, with its node id or document path), before any task is
/// built; `UnresolvedTemplate` when an inner-step template resolves to no
/// content; `Projection` when a payload the mapping does carry cannot be read;
/// `Graph` when the wired topology is invalid (an `entry` naming no outer task,
/// or an edge to a task the projection never produced) or an inner graph fails
/// to build.
#[allow(clippy::needless_pass_by_value, clippy::implicit_hasher)] // dispatch is cloned into each task and the cancel map is the engine's own, mirroring build_wired_outer_graph
pub fn build_wired_outer_graph_from_ir(
    document: &WorkflowDocument,
    engine: &Arc<dyn crate::engine::OrchestrationEngine>,
    caps: &Arc<CapabilityRegistry>,
    daemon_tool_dispatch: Option<std::sync::Arc<dyn crate::capability::DaemonToolDispatch>>,
    prompt_executor: Option<std::sync::Arc<dyn crate::capability::PromptExecutor>>,
    session_cancels: std::sync::Arc<
        std::sync::RwLock<std::collections::HashMap<String, tokio_util::sync::CancellationToken>>,
    >,
) -> Result<graph_flow::Graph, IrOuterGraphError> {
    use crate::tasks::StateCompositeTask;

    refuse_unmappable(document)?;

    let mut manifest = nexus_preset::export_preset(document)?;

    // I-000440: a template that resolves to nothing would build a graph whose
    // inner tasks prompt the Host with zero instructions. Refuse it here, by
    // node and path, before any task exists.
    refuse_unresolved_templates(&manifest, document.id.as_str())?;

    // §8.2.1 `terminal[]`: a declared terminal state ends the run even when its
    // node kind is `outer_state`; the composite task overrides with `End`.
    for state in &mut manifest.states {
        state.terminal |= document.terminal.iter().any(|id| id == &state.id);
    }

    let inner_graphs = build_inner_graphs(
        &manifest,
        document.id.as_str(),
        None,
        prompt_executor,
        session_cancels,
    )?;
    let output_bindings = extract_output_bindings(&manifest);

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
            .with_inner_graphs(inner_graphs.clone())
            .with_output_bindings(output_bindings.clone())
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
    /// Inner graphs in name order (§8.2.1 `inner_graphs.<name>`).
    pub inner_graphs: Vec<ProjectedInnerGraph>,
}

/// One inner graph in an [`OuterGraphProjection`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectedInnerGraph {
    /// `inner_graphs.<name>`.
    pub name: String,
    /// Node whose output is exported as the parent state's output.
    pub output_binding: Option<String>,
    /// Nodes in declared order.
    pub nodes: Vec<ProjectedInnerNode>,
    /// `depends_on` edges in node order.
    pub edges: Vec<ProjectedEdge>,
}

/// One inner-graph node in a [`ProjectedInnerGraph`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectedInnerNode {
    /// Node id within its graph.
    pub id: String,
    /// `"acp_prompt"`.
    pub kind: &'static str,
    /// Template path the node renders (resolved against the bundle at build time).
    pub template_file: Option<String>,
    /// Tool policy string as declared.
    pub tool_policy: Option<String>,
    /// Role id the node runs as.
    pub agent: Option<String>,
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
/// Inner graphs are projected from `manifest.inner_graphs` — the carrier both
/// intakes build them from — because a built `graph_flow::Graph` exposes no
/// child-graph accessor. That the composite tasks actually hold the handles is
/// what a scripted trace asserts, not this projection (a handle that was never
/// attached fails the run).
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
        inner_graphs: project_inner_graphs(manifest),
    }
}

/// Inner-graph shape of a manifest, in name order.
fn project_inner_graphs(manifest: &PresetManifest) -> Vec<ProjectedInnerGraph> {
    manifest
        .inner_graphs
        .as_ref()
        .map_or_else(Vec::new, |graphs| {
            graphs
                .iter()
                .map(|(name, inner)| ProjectedInnerGraph {
                    name: name.clone(),
                    output_binding: inner.output_binding.clone(),
                    nodes: inner
                        .nodes
                        .iter()
                        .map(|node| ProjectedInnerNode {
                            id: node.id.clone(),
                            kind: match node.kind {
                                nexus_preset::manifest::GraphNodeKind::AcpPrompt => "acp_prompt",
                            },
                            template_file: node.template_file.clone(),
                            tool_policy: node.tool_policy.clone(),
                            agent: node.agent.clone(),
                        })
                        .collect(),
                    edges: inner
                        .nodes
                        .iter()
                        .flat_map(|node| {
                            node.depends_on.iter().map(|dependency| ProjectedEdge {
                                from: dependency.clone(),
                                to: node.id.clone(),
                                kind: "depends_on",
                                detail: None,
                            })
                        })
                        .collect(),
                })
                .collect()
        })
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
                        // presets read from their resolved root. Resolution
                        // failure is refused upstream — loader asset validation
                        // for YAML presets, `refuse_unresolved_templates` for IR
                        // documents — so the default below is unreachable for a
                        // loaded preset.
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

    /// An IR document with one inner graph: `generate` launches it,
    /// `generate_with_context → refine` run as `acp_prompt` nodes, and
    /// `refine` carries the `output_binding`. The graph id and template paths
    /// name the embedded `memory-augmented` bundle, so templates resolve
    /// without a caller-supplied bundle root.
    const IR_INNER_GRAPH: &str = r#"{
        "ir_version": 0,
        "id": "memory-augmented",
        "version": 1,
        "revision": 0,
        "state": "candidate",
        "meta": {
            "description": "IR inner graph fixture",
            "kind": "creator",
            "run_intents": [],
            "requires_capabilities": []
        },
        "entry": "generate",
        "terminal": ["done"],
        "nodes": [
            {"id": "generate", "kind": "outer_state", "effect_class": "external",
             "enter": [{"kind": "inner_graph", "name": "generate_graph"}],
             "exit_when": {"kind": "rule"}},
            {"id": "generate_graph/generate_with_context", "kind": "inner_step",
             "effect_class": "external", "step_kind": "acp_prompt",
             "template": "prompts/generate-with-memory.md",
             "tool_policy": "auto_grant_read_only"},
            {"id": "generate_graph/refine", "kind": "inner_step",
             "effect_class": "external", "step_kind": "acp_prompt",
             "depends_on": ["generate_graph/generate_with_context"],
             "template": "prompts/recall-memory.md",
             "tool_policy": "auto_grant_read_only",
             "output_binding": "refine.text"},
            {"id": "done", "kind": "terminal", "effect_class": "pure"}
        ],
        "edges": [
            {"id": "generate->done", "from": "generate", "to": "done", "kind": "linear"}
        ],
        "nests": [],
        "assets": [],
        "signals": [],
        "roles": [],
        "effect_summary": "external",
        "provenance": {"source": "compose"}
    }"#;

    /// Records every inner-node prompt in dispatch order.
    #[derive(Default)]
    struct RecordingExecutor {
        calls: std::sync::Mutex<Vec<(String, String)>>,
    }

    #[async_trait::async_trait]
    impl crate::capability::PromptExecutor for RecordingExecutor {
        async fn execute(
            &self,
            request: crate::capability::PromptRequest,
        ) -> Result<crate::capability::PromptResult, crate::capability::CapabilityError> {
            self.calls
                .lock()
                .expect("recording executor lock")
                .push((request.task_id.clone(), request.prompt.clone()));
            Ok(crate::capability::PromptResult {
                full_text: format!("{} output", request.task_id),
                host_session_id: format!("host:{}", request.run_id),
                operation_id: "op-1".to_string(),
            })
        }
    }

    /// Engine stub for the inner-graph trace: a spawned child really runs its
    /// inner graph through `graph_flow`, so the recorded prompts prove the
    /// built graph carried the inner tasks and their `depends_on` edges.
    struct ChildRunningEngine {
        children: tokio::sync::Mutex<
            std::collections::HashMap<String, (Arc<graph_flow::Graph>, graph_flow::Session)>,
        >,
        session_cancels: Arc<
            std::sync::RwLock<
                std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
            >,
        >,
    }

    #[async_trait::async_trait]
    // The child registry guard legitimately spans the step: a session is
    // mutated in place, so it cannot be released before `execute_session`.
    #[allow(clippy::significant_drop_tightening)]
    impl crate::engine::OrchestrationEngine for ChildRunningEngine {
        async fn run_step(
            &self,
            session_id: &crate::engine::SessionId,
        ) -> Result<crate::engine::StepOutcome, crate::engine::EngineError> {
            let result = {
                let mut children = self.children.lock().await;
                let (graph, session) = children
                    .get_mut(&session_id.0)
                    .expect("child session is registered");
                graph
                    .execute_session(session)
                    .await
                    .map_err(crate::engine::EngineError::GraphFlow)?
            };
            Ok(match result.status {
                graph_flow::ExecutionStatus::Completed => crate::engine::StepOutcome::Completed {
                    response: result.response,
                },
                graph_flow::ExecutionStatus::WaitingForInput => {
                    crate::engine::StepOutcome::WaitingForInput {
                        response: result.response,
                    }
                }
                graph_flow::ExecutionStatus::Paused {
                    next_task_id,
                    reason,
                } => crate::engine::StepOutcome::Paused {
                    next_task_id,
                    reason,
                },
            })
        }

        async fn spawn_child_session(
            &self,
            params: crate::engine::ChildSessionParams,
        ) -> Result<crate::engine::SessionId, crate::engine::EngineError> {
            let id = {
                let children = self.children.lock().await;
                format!("{}:child:{}", params.parent_session_id, children.len() + 1)
            };
            let start = params
                .inner_graph
                .start_task_id()
                .unwrap_or_default()
                .to_string();
            let mut session = graph_flow::Session::new_from_task(id.clone(), &start);
            session.context = params.initial_context;
            // Production seeds the trusted child run identity at admission
            // (`spawn_child_session_internal`); every inner node resolves it.
            session
                .context
                .set("_session_id", id.clone())
                .map_err(crate::engine::EngineError::GraphFlow)?;
            self.session_cancels
                .write()
                .expect("session cancels lock")
                .insert(id.clone(), tokio_util::sync::CancellationToken::new());
            self.children
                .lock()
                .await
                .insert(id.clone(), (params.inner_graph, session));
            Ok(crate::engine::SessionId(id))
        }

        async fn attach_existing_child_session(
            &self,
            _: &str,
            _: Arc<graph_flow::Graph>,
        ) -> Result<Option<crate::engine::SessionId>, crate::engine::EngineError> {
            Ok(None)
        }

        async fn get_status(
            &self,
            _: &crate::engine::SessionId,
        ) -> Result<crate::engine::SessionStatus, crate::engine::EngineError> {
            Ok(crate::engine::SessionStatus::Running)
        }

        async fn get_context(
            &self,
            session_id: &crate::engine::SessionId,
        ) -> Result<graph_flow::Context, crate::engine::EngineError> {
            Ok(self
                .children
                .lock()
                .await
                .get(&session_id.0)
                .expect("child session is registered")
                .1
                .context
                .clone())
        }

        async fn new_session(
            &self,
            _: crate::engine::SessionKey,
            _: crate::engine::Context,
        ) -> Result<crate::engine::SessionId, crate::engine::EngineError> {
            unimplemented!("inner-graph trace does not drive the outer engine")
        }
        async fn start_session_with_graph(
            &self,
            _: &str,
            _: Arc<graph_flow::Graph>,
        ) -> Result<crate::engine::SessionId, crate::engine::EngineError> {
            unimplemented!("inner-graph trace does not drive the outer engine")
        }
        async fn signal(
            &self,
            _: &crate::engine::SessionId,
            _: crate::engine::EngineSignal,
        ) -> Result<(), crate::engine::EngineError> {
            // `InnerGraphTask` resumes a paused child; the scripted child graph
            // holds no pause state, so the resume is a no-op here.
            Ok(())
        }
        async fn list_active(
            &self,
            _: crate::engine::SessionFilter,
        ) -> Result<Vec<crate::engine::SessionSummary>, crate::engine::EngineError> {
            unimplemented!("inner-graph trace does not drive the outer engine")
        }
        async fn get_current_task_id(
            &self,
            _: &crate::engine::SessionId,
        ) -> Result<Option<String>, crate::engine::EngineError> {
            unimplemented!("inner-graph trace does not query child cursors")
        }
        async fn has_runner(&self, _: &crate::engine::SessionId) -> bool {
            unimplemented!("inner-graph trace does not query runner existence")
        }
        async fn recover_sessions(&self, _: Vec<crate::engine::SessionSummary>) {
            unimplemented!("inner-graph trace does not recover sessions")
        }
        async fn ensure_recovered_runner(
            &self,
            _: &crate::engine::SessionId,
        ) -> Result<(), crate::engine::EngineError> {
            unimplemented!("inner-graph trace does not reattach runners")
        }
        async fn start_session_with_preset(
            &self,
            _: &LoadedPreset,
        ) -> Result<crate::engine::SessionId, crate::engine::EngineError> {
            unimplemented!("inner-graph trace does not start presets")
        }
        async fn start_session_with_preset_for_creator(
            &self,
            _: &LoadedPreset,
            _: &str,
        ) -> Result<crate::engine::SessionId, crate::engine::EngineError> {
            unimplemented!("inner-graph trace does not start presets")
        }
    }

    fn ir_document(text: &str) -> WorkflowDocument {
        serde_json::from_str(text).expect("fixture is a valid workflow document")
    }

    fn no_cancels() -> Arc<
        std::sync::RwLock<std::collections::HashMap<String, tokio_util::sync::CancellationToken>>,
    > {
        Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()))
    }

    fn ir_outer_graph(document: &WorkflowDocument) -> graph_flow::Graph {
        let caps = Arc::new(test_capability_registry());
        let engine: Arc<dyn crate::engine::OrchestrationEngine> = Arc::new(UnreachableEngine);
        build_wired_outer_graph_from_ir(document, &engine, &caps, None, None, no_cancels())
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

    /// A copy of [`IR_LINEAR`] with one node's kind replaced.
    fn ir_with_node_kind(node_index: usize, kind: &str) -> WorkflowDocument {
        let mut document: serde_json::Value =
            serde_json::from_str(IR_LINEAR).expect("fixture is JSON");
        document["nodes"][node_index]["kind"] = serde_json::json!(kind);
        serde_json::from_value(document).expect("mutated fixture is a valid workflow document")
    }

    /// A copy of [`IR_LINEAR`] declaring `terminals`.
    fn ir_with_terminals(terminals: &[&str]) -> WorkflowDocument {
        let mut document: serde_json::Value =
            serde_json::from_str(IR_LINEAR).expect("fixture is JSON");
        document["terminal"] = serde_json::json!(terminals);
        serde_json::from_value(document).expect("mutated fixture is a valid workflow document")
    }

    /// The typed refusal for a document the mapping cannot express; no graph.
    #[track_caller]
    fn assert_refused(document: &WorkflowDocument, concept: &str) -> IrOuterGraphError {
        let caps = Arc::new(test_capability_registry());
        let engine: Arc<dyn crate::engine::OrchestrationEngine> = Arc::new(UnreachableEngine);
        let Err(error) =
            build_wired_outer_graph_from_ir(document, &engine, &caps, None, None, no_cancels())
        else {
            panic!("unmappable document is refused, not partially built");
        };
        let message = error.to_string();
        assert!(
            matches!(
                error,
                IrOuterGraphError::Unmappable { .. } | IrOuterGraphError::Unprojected { .. }
            ),
            "refusal is typed by gap, not the adapter projection: {message}"
        );
        assert!(message.contains(concept), "`{concept}` is named: {message}");
        error
    }

    /// §8.2.1 has the row, the adapter has no projection for the node kind.
    #[track_caller]
    fn assert_unprojected(document: &WorkflowDocument, concept: &str, location: &str) {
        let error = assert_refused(document, concept);
        let message = error.to_string();
        assert!(
            matches!(error, IrOuterGraphError::Unprojected { .. }),
            "gap is the adapter projection, not §8.2.1: {message}"
        );
        assert!(message.contains("ir_unprojected"), "{message}");
        assert!(
            message.contains("no preset projection in the IR adapter"),
            "the adapter is named as the gap: {message}"
        );
        assert!(message.contains(location), "location named: {message}");
    }

    /// §8.2.1 has no execution row for the concept.
    #[track_caller]
    fn assert_unmappable(document: &WorkflowDocument, concept: &str, location: &str) {
        let error = assert_refused(document, concept);
        let message = error.to_string();
        assert!(
            matches!(error, IrOuterGraphError::Unmappable { .. }),
            "gap is the absent §8.2.1 row: {message}"
        );
        assert!(message.contains("ir_unmappable"), "{message}");
        assert!(message.contains(location), "location named: {message}");
    }

    #[test]
    fn ir_refuses_judge_node_kind_as_an_adapter_gap() {
        assert_unprojected(&ir_with_node_kind(1, "judge"), "judge", "review");
    }

    #[test]
    fn ir_refuses_manual_wait_node_kind_as_an_adapter_gap() {
        assert_unprojected(&ir_with_node_kind(1, "manual_wait"), "manual_wait", "review");
    }

    #[test]
    fn ir_refuses_join_node_kind_as_an_adapter_gap() {
        assert_unprojected(&ir_with_node_kind(1, "join"), "join", "review");
    }

    #[test]
    fn ir_refuses_pattern_slot_node_kind_by_name() {
        assert_unmappable(&ir_with_node_kind(1, "pattern_slot"), "pattern_slot", "review");
    }

    #[test]
    fn ir_refuses_nest_group_node_kind_by_name() {
        assert_unmappable(&ir_with_node_kind(1, "nest_group"), "nest_group", "review");
    }

    #[test]
    fn ir_refuses_zero_terminals_by_name() {
        assert_unprojected(&ir_with_terminals(&[]), "terminal", "0 entries");
    }

    #[test]
    fn ir_refuses_two_terminals_by_name() {
        assert_unprojected(
            &ir_with_terminals(&["done", "review"]),
            "terminal",
            "2 entries",
        );
    }

    /// A copy of [`IR_INNER_GRAPH`] with one inner step's `template` replaced.
    fn ir_with_inner_template(node_index: usize, template: serde_json::Value) -> WorkflowDocument {
        let mut document: serde_json::Value =
            serde_json::from_str(IR_INNER_GRAPH).expect("fixture is JSON");
        document["nodes"][node_index]["template"] = template;
        serde_json::from_value(document).expect("mutated fixture is a valid workflow document")
    }

    /// I-000440: an unresolvable template refuses at build time; no graph.
    #[test]
    fn ir_refuses_an_unresolved_inner_template_before_building() {
        let caps = Arc::new(test_capability_registry());
        let engine: Arc<dyn crate::engine::OrchestrationEngine> = Arc::new(UnreachableEngine);
        // `generate_graph/refine` names a path the document's bundle lacks.
        let document = ir_with_inner_template(2, serde_json::json!("prompts/missing.md"));
        let Err(error) =
            build_wired_outer_graph_from_ir(&document, &engine, &caps, None, None, no_cancels())
        else {
            panic!("a document whose template resolves to nothing is refused, not built");
        };
        match &error {
            IrOuterGraphError::UnresolvedTemplate { node, template } => {
                assert_eq!(node, "generate_graph/refine", "the node is named");
                assert_eq!(
                    template.as_deref(),
                    Some("prompts/missing.md"),
                    "the template path is named"
                );
            }
            other => panic!("refusal must be typed ir_unresolved_template: {other}"),
        }
        let message = error.to_string();
        assert!(message.contains("generate_graph/refine"), "{message}");
        assert!(message.contains("prompts/missing.md"), "{message}");
    }

    /// I-000440: a step declaring no template at all is refused the same way.
    #[test]
    fn ir_refuses_an_inner_step_without_a_template() {
        let caps = Arc::new(test_capability_registry());
        let engine: Arc<dyn crate::engine::OrchestrationEngine> = Arc::new(UnreachableEngine);
        let document = ir_with_inner_template(2, serde_json::Value::Null);
        let Err(IrOuterGraphError::UnresolvedTemplate { node, template }) =
            build_wired_outer_graph_from_ir(&document, &engine, &caps, None, None, no_cancels())
        else {
            panic!("an inner step with no template is refused, not built");
        };
        assert_eq!(node, "generate_graph/refine");
        assert_eq!(template, None, "no template path is reported as absent");
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

    #[tokio::test]
    async fn ir_inner_graph_runs_nodes_and_writes_the_output_binding() {
        let document = ir_document(IR_INNER_GRAPH);
        let caps = Arc::new(test_capability_registry());
        let session_cancels = no_cancels();
        let engine = Arc::new(ChildRunningEngine {
            children: tokio::sync::Mutex::new(std::collections::HashMap::new()),
            session_cancels: session_cancels.clone(),
        });
        let executor = Arc::new(RecordingExecutor::default());
        let engine_ref: Arc<dyn crate::engine::OrchestrationEngine> = engine.clone();
        let graph = build_wired_outer_graph_from_ir(
            &document,
            &engine_ref,
            &caps,
            None,
            Some(executor.clone()),
            session_cancels,
        )
        .expect("IR graph with an inner graph builds");

        // The projection carries the inner shape the parity tests compare.
        let manifest =
            nexus_preset::export_preset(&document).expect("document projects onto states");
        let projection = outer_graph_projection(&graph, &manifest);
        assert_eq!(projection.inner_graphs.len(), 1, "one inner graph declared");
        let inner = &projection.inner_graphs[0];
        assert_eq!(inner.name, "generate_graph");
        assert_eq!(inner.output_binding.as_deref(), Some("refine.text"));
        assert_eq!(
            inner
                .nodes
                .iter()
                .map(|node| (node.id.as_str(), node.kind))
                .collect::<Vec<_>>(),
            [
                ("generate_with_context", "acp_prompt"),
                ("refine", "acp_prompt")
            ]
        );
        assert_eq!(
            inner
                .edges
                .iter()
                .map(|edge| (edge.from.as_str(), edge.to.as_str(), edge.kind))
                .collect::<Vec<_>>(),
            [("generate_with_context", "refine", "depends_on")]
        );

        // Scripted trace: the state's enter action launches the attached graph.
        let task = graph.get_task("generate").expect("state task is wired");
        let context = graph_flow::Context::new();
        context
            .set("_session_id", "run:1")
            .expect("parent run identity");
        let result = task.run(context.clone()).await.expect("inner graph runs");

        assert_eq!(result.next_action, graph_flow::NextAction::Continue);
        let calls = executor
            .calls
            .lock()
            .expect("recording executor lock")
            .clone();
        assert_eq!(
            calls
                .iter()
                .map(|(task_id, _)| task_id.as_str())
                .collect::<Vec<_>>(),
            ["generate_with_context", "refine"],
            "the depends_on edge runs the sink node after its dependency"
        );
        assert!(
            calls[0].1.contains("Generate with Memory Context"),
            "template resolves from the node's asset path; got: {}",
            calls[0].1
        );
        assert!(
            calls[1].1.contains("Recall Memories"),
            "template resolves from the node's asset path; got: {}",
            calls[1].1
        );
        assert_eq!(
            context.get::<String>("state.generate.output").as_deref(),
            Some("refine output"),
            "output_binding `refine.text` writes the bound node's output"
        );
    }

    /// IR ↔ YAML parity (§8.2 / §8.2.1).
    ///
    /// Parity is behavioural equality for the runner's observables, not byte
    /// equality of graph values: tasks hold `Arc` handles, closures and
    /// injected executors, so their addresses and iteration order are not
    /// execution semantics. Each test builds the YAML preset and the equivalent
    /// IR document, then compares
    ///
    /// 1. `outer_graph_projection` — entry, task ids/kinds, ordered enter
    ///    actions, exit/join/context-update carriers, routing edges with their
    ///    label or `when`, terminals and inner-graph shape; and
    /// 2. a scripted trace driven through `Graph::execute_session` — task
    ///    invocation order, the edge each `Continue` selects, `NextAction`
    ///    waits, the terminal result and the recorded capability / host-tool /
    ///    prompt effects.
    ///
    /// Two comparisons cannot be made literally and are compensated instead of
    /// weakened:
    ///
    /// * `graph_flow::Graph` exposes no edge accessor (`tasks`/`edges` are
    ///   private), so edge parity rests on the projection *plus* the trace's
    ///   selected edges — a dropped wiring surfaces as a `Paused` step with no
    ///   outgoing edge, not as a silently shorter edge list.
    /// * The YAML path starts at `states[0]` (graph-flow's first task) while the
    ///   IR path honours `document.entry`, so the linear IR fixture lists its
    ///   entry node out of order and projections are compared with tasks
    ///   ordered by id and edges grouped by source state. Neither is a runner
    ///   observable; conditional rule order *within* a state is preserved
    ///   because it is first-match execution semantics.
    mod parity {
        use super::super::{
            build_wired_outer_graph, build_wired_outer_graph_from_ir, outer_graph_projection, wire,
            OuterGraphProjection, ProjectedEdge, ProjectedTask,
        };
        use super::{ir_document, no_cancels, ChildRunningEngine, UnreachableEngine};
        use crate::capability::{
            Capability, CapabilityError, CapabilityRegistry, DaemonToolDispatch, PromptExecutor,
            PromptRequest, PromptResult,
        };
        use crate::engine::OrchestrationEngine;
        use graph_flow::{Context, ExecutionStatus, Graph, NextAction, Session};
        use nexus_preset::load_preset_from_str;
        use nexus_preset::manifest::PresetManifest;
        use std::collections::{BTreeMap, HashMap};
        use std::sync::{Arc, Mutex};

        // ----- fixtures ----------------------------------------------------

        /// `entry` / `outer_state` / capability enter / rule exit / linear edge /
        /// terminal node. The IR document lists `review` first so that `entry`
        /// cannot be satisfied positionally.
        const YAML_LINEAR: &str = r"
preset:
  id: parity-linear
  version: 1
  kind: creator
  description: parity linear fixture
  requires_capabilities: []
  initial: plan
  terminal: done
states:
  - id: plan
    enter:
      - kind: capability
        name: parity.record
        args: { note: plan }
    exit_when: { kind: rule }
    next: review
  - id: review
    exit_when: { kind: rule }
    next: done
  - id: done
    terminal: true
";

        const IR_LINEAR: &str = r#"{
            "ir_version": 0,
            "id": "parity-linear",
            "version": 1,
            "revision": 0,
            "state": "candidate",
            "meta": {"description": "parity linear fixture", "kind": "creator", "run_intents": [], "requires_capabilities": []},
            "entry": "plan",
            "terminal": ["done"],
            "nodes": [
                {"id": "review", "kind": "outer_state", "effect_class": "pure", "exit_when": {"kind": "rule"}},
                {"id": "plan", "kind": "outer_state", "effect_class": "read",
                 "enter": [{"kind": "capability", "name": "parity.record", "args": {"note": "plan"}}],
                 "exit_when": {"kind": "rule"}},
                {"id": "done", "kind": "terminal", "effect_class": "pure"}
            ],
            "edges": [
                {"id": "plan->review", "from": "plan", "to": "review", "kind": "linear"},
                {"id": "review->done", "from": "review", "to": "done", "kind": "linear"}
            ],
            "nests": [], "assets": [], "signals": [], "roles": [],
            "effect_summary": "read",
            "provenance": {"source": "compose"}
        }"#;

        /// manual and timer exits: both wait, both resume into `Continue`.
        const YAML_WAIT: &str = r"
preset:
  id: parity-wait
  version: 1
  kind: creator
  description: parity wait fixture
  requires_capabilities: []
  initial: handoff
  terminal: done
states:
  - id: handoff
    exit_when: { kind: manual }
    next: tick
  - id: tick
    exit_when: { kind: timer, duration: PT1H }
    next: done
  - id: done
    terminal: true
";

        const IR_WAIT: &str = r#"{
            "ir_version": 0,
            "id": "parity-wait",
            "version": 1,
            "revision": 0,
            "state": "candidate",
            "meta": {"description": "parity wait fixture", "kind": "creator", "run_intents": [], "requires_capabilities": []},
            "entry": "handoff",
            "terminal": ["done"],
            "nodes": [
                {"id": "handoff", "kind": "outer_state", "effect_class": "pure", "exit_when": {"kind": "manual"}},
                {"id": "tick", "kind": "outer_state", "effect_class": "pure", "exit_when": {"kind": "timer", "duration": "PT1H"}},
                {"id": "done", "kind": "terminal", "effect_class": "pure"}
            ],
            "edges": [
                {"id": "handoff->tick", "from": "handoff", "to": "tick", "kind": "linear"},
                {"id": "tick->done", "from": "tick", "to": "done", "kind": "linear"}
            ],
            "nests": [], "assets": [], "signals": [], "roles": [],
            "effect_summary": "pure",
            "provenance": {"source": "compose"}
        }"#;

        /// judge exit with go/nogo edges. `parity.judge` is scripted per run.
        const YAML_JUDGE: &str = r"
preset:
  id: parity-judge
  version: 1
  kind: creator
  description: parity judge fixture
  requires_capabilities: []
  initial: assess
  terminal: done
states:
  - id: assess
    exit_when:
      kind: llm_judge
      template_file: prompts/parity-judge.md
      judge_capability: parity.judge
    next:
      go: pass
      nogo: revise
  - id: pass
    exit_when: { kind: rule }
    next: done
  - id: revise
    exit_when: { kind: rule }
    next: done
  - id: done
    terminal: true
";

        const IR_JUDGE: &str = r#"{
            "ir_version": 0,
            "id": "parity-judge",
            "version": 1,
            "revision": 0,
            "state": "candidate",
            "meta": {"description": "parity judge fixture", "kind": "creator", "run_intents": [], "requires_capabilities": []},
            "entry": "assess",
            "terminal": ["done"],
            "nodes": [
                {"id": "assess", "kind": "outer_state", "effect_class": "external",
                 "exit_when": {"kind": "llm_judge", "template": "prompts/parity-judge.md", "judge_capability": "parity.judge"}},
                {"id": "pass", "kind": "outer_state", "effect_class": "pure", "exit_when": {"kind": "rule"}},
                {"id": "revise", "kind": "outer_state", "effect_class": "pure", "exit_when": {"kind": "rule"}},
                {"id": "done", "kind": "terminal", "effect_class": "pure"}
            ],
            "edges": [
                {"id": "assess->pass", "from": "assess", "to": "pass", "kind": "go"},
                {"id": "assess->revise", "from": "assess", "to": "revise", "kind": "nogo"},
                {"id": "pass->done", "from": "pass", "to": "done", "kind": "linear"},
                {"id": "revise->done", "from": "revise", "to": "done", "kind": "linear"}
            ],
            "nests": [], "assets": [], "signals": [], "roles": [],
            "effect_summary": "external",
            "provenance": {"source": "compose"}
        }"#;

        /// conditional + default edges and a context-update hook.
        const YAML_CONDITIONAL: &str = r#"
preset:
  id: parity-conditional
  version: 1
  kind: creator
  description: parity conditional fixture
  requires_capabilities: []
  initial: review
  terminal: approve
states:
  - id: review
    exit_when: { kind: rule }
    context_update:
      op: { kind: append, body: reviewed }
      template_file: prompts/parity-append.md
    next:
      branches:
        - when: "_context.score > 1"
          target: approve
      default: revise
  - id: revise
    exit_when: { kind: rule }
    next: approve
  - id: approve
    terminal: true
"#;

        const IR_CONDITIONAL: &str = r#"{
            "ir_version": 0,
            "id": "parity-conditional",
            "version": 1,
            "revision": 0,
            "state": "candidate",
            "meta": {"description": "parity conditional fixture", "kind": "creator", "run_intents": [], "requires_capabilities": []},
            "entry": "review",
            "terminal": ["approve"],
            "nodes": [
                {"id": "review", "kind": "outer_state", "effect_class": "read",
                 "exit_when": {"kind": "rule"},
                 "context_update": {"op": "append", "template": "prompts/parity-append.md", "body": "reviewed"}},
                {"id": "revise", "kind": "outer_state", "effect_class": "pure", "exit_when": {"kind": "rule"}},
                {"id": "approve", "kind": "terminal", "effect_class": "pure"}
            ],
            "edges": [
                {"id": "review->approve", "from": "review", "to": "approve", "kind": "conditional", "when": "_context.score > 1"},
                {"id": "review->revise", "from": "review", "to": "revise", "kind": "default"},
                {"id": "revise->approve", "from": "revise", "to": "approve", "kind": "linear"}
            ],
            "nests": [], "assets": [], "signals": [], "roles": [],
            "effect_summary": "read",
            "provenance": {"source": "compose"}
        }"#;

        /// inner graph: enter action, `acp_prompt` nodes, `depends_on` edges,
        /// role binding, asset template and `output_binding`. The document id names
        /// the embedded bundle, so both intakes resolve the same templates.
        const YAML_INNER: &str = r"
preset:
  id: memory-augmented
  version: 1
  kind: creator
  description: parity inner-graph fixture
  requires_capabilities: []
  initial: generate
  terminal: done
roles:
  - id: writer
    description: parity writer
    system_prompt_file: prompts/generate-with-memory.md
    recommended_skills: [parity-skill]
states:
  - id: generate
    enter:
      - kind: inner_graph
        name: generate_graph
    exit_when: { kind: graph_complete }
    next: done
  - id: done
    terminal: true
inner_graphs:
  generate_graph:
    output_binding: refine.text
    nodes:
      - id: generate_with_context
        kind: acp_prompt
        template_file: prompts/generate-with-memory.md
        tool_policy: auto_grant_read_only
      - id: refine
        kind: acp_prompt
        depends_on: [generate_with_context]
        template_file: prompts/recall-memory.md
        tool_policy: auto_grant_read_only
        agent: writer
";

        const IR_INNER: &str = r#"{
            "ir_version": 0,
            "id": "memory-augmented",
            "version": 1,
            "revision": 0,
            "state": "candidate",
            "meta": {"description": "parity inner-graph fixture", "kind": "creator", "run_intents": [], "requires_capabilities": []},
            "entry": "generate",
            "terminal": ["done"],
            "nodes": [
                {"id": "generate", "kind": "outer_state", "effect_class": "external",
                 "enter": [{"kind": "inner_graph", "name": "generate_graph"}],
                 "exit_when": {"kind": "graph_complete"}},
                {"id": "generate_graph/generate_with_context", "kind": "inner_step", "effect_class": "external",
                 "step_kind": "acp_prompt", "template": "prompts/generate-with-memory.md",
                 "tool_policy": "auto_grant_read_only"},
                {"id": "generate_graph/refine", "kind": "inner_step", "effect_class": "external",
                 "step_kind": "acp_prompt", "depends_on": ["generate_graph/generate_with_context"],
                 "template": "prompts/recall-memory.md", "tool_policy": "auto_grant_read_only",
                 "role": "writer", "output_binding": "refine.text"},
                {"id": "done", "kind": "terminal", "effect_class": "pure"}
            ],
            "edges": [
                {"id": "generate->done", "from": "generate", "to": "done", "kind": "linear"}
            ],
            "nests": [],
            "assets": [
                {"id": "generate_prompt", "path": "prompts/generate-with-memory.md", "kind": "prompt"},
                {"id": "recall_prompt", "path": "prompts/recall-memory.md", "kind": "prompt"}
            ],
            "signals": [],
            "roles": [
                {"id": "writer", "description": "parity writer",
                 "system_prompt_file": "prompts/generate-with-memory.md",
                 "recommended_skills": ["parity-skill"]}
            ],
            "effect_summary": "external",
            "provenance": {"source": "compose"}
        }"#;

        /// `host_tool` enter action, dispatched in-process.
        const YAML_HOST_TOOL: &str = r"
preset:
  id: parity-host-tool
  version: 1
  kind: creator
  description: parity host-tool fixture
  requires_capabilities: []
  initial: status
  terminal: done
states:
  - id: status
    enter:
      - kind: host_tool
        tool_name: nexus.orchestration.schedule_status
        args: { schedule_id: sched-1 }
    exit_when: { kind: rule }
    next: done
  - id: done
    terminal: true
";

        const IR_HOST_TOOL: &str = r#"{
            "ir_version": 0,
            "id": "parity-host-tool",
            "version": 1,
            "revision": 0,
            "state": "candidate",
            "meta": {"description": "parity host-tool fixture", "kind": "creator", "run_intents": [], "requires_capabilities": []},
            "entry": "status",
            "terminal": ["done"],
            "nodes": [
                {"id": "status", "kind": "outer_state", "effect_class": "read",
                 "enter": [{"kind": "host_tool", "tool_name": "nexus.orchestration.schedule_status",
                            "args": {"schedule_id": "sched-1"}}],
                 "exit_when": {"kind": "rule"}},
                {"id": "done", "kind": "terminal", "effect_class": "pure"}
            ],
            "edges": [{"id": "status->done", "from": "status", "to": "done", "kind": "linear"}],
            "nests": [], "assets": [], "signals": [], "roles": [],
            "effect_summary": "read",
            "provenance": {"source": "compose"}
        }"#;

        /// Non-execution document fields: `initial_action`, `signals`, `roles`,
        /// `cli_args`, `gates`, `run_intents`, `effect_class`/`effect_summary`.
        const YAML_METADATA: &str = r"
preset:
  id: parity-metadata
  version: 1
  kind: creator
  description: parity metadata fixture
  requires_capabilities: []
  run_intents: [work_init]
  initial: only
  terminal: done
  initial_action: { kind: seed_direct }
  cli_args:
    - name: chapter
      type: integer
      required: false
      description: chapter number
  gates:
    - kind: filesystem
      path: 'Works/{{work_ref}}/'
      must_exist: true
roles:
  - id: writer
    description: parity writer
    system_prompt_file: prompts/parity-writer.md
    recommended_skills: [parity-skill]
signals:
  - name: pause_run
    on_receive:
      action: pause
states:
  - id: only
    exit_when: { kind: rule }
    next: done
  - id: done
    terminal: true
";

        const IR_METADATA: &str = r#"{
            "ir_version": 0,
            "id": "parity-metadata",
            "version": 1,
            "revision": 0,
            "state": "candidate",
            "meta": {
                "description": "parity metadata fixture",
                "kind": "creator",
                "run_intents": ["work_init"],
                "requires_capabilities": [],
                "cli_args": [
                    {"name": "chapter", "type": "integer", "required": false, "description": "chapter number"}
                ],
                "gates": [
                    {"kind": "filesystem", "path": "Works/{{work_ref}}/", "must_exist": true}
                ]
            },
            "entry": "only",
            "initial_action": {"kind": "seed_direct"},
            "terminal": ["done"],
            "nodes": [
                {"id": "only", "kind": "outer_state", "effect_class": "pure", "exit_when": {"kind": "rule"}},
                {"id": "done", "kind": "terminal", "effect_class": "pure"}
            ],
            "edges": [{"id": "only->done", "from": "only", "to": "done", "kind": "linear"}],
            "nests": [],
            "assets": [
                {"id": "writer_prompt", "path": "prompts/parity-writer.md", "kind": "prompt"}
            ],
            "signals": [{"name": "pause_run", "action": "pause"}],
            "roles": [
                {"id": "writer", "description": "parity writer",
                 "system_prompt_file": "prompts/parity-writer.md",
                 "recommended_skills": ["parity-skill"]}
            ],
            "effect_summary": "pure",
            "provenance": {"source": "compose"}
        }"#;

        /// join gate: merge + converge + bounded timeout on one state.
        const YAML_JOIN: &str = r"
preset:
  id: parity-join
  version: 1
  kind: creator
  description: parity join fixture
  requires_capabilities: []
  initial: assess
  terminal: done
states:
  - id: assess
    exit_when:
      kind: llm_judge
      template_file: prompts/parity-judge.md
      judge_capability: parity.judge
    next:
      - label: go
        target: gate
      - label: keep
        target: gate
  - id: gate
    merge: { kind: all }
    converge: { strategy: wait_for_all }
    timeout_ms: 600000
    on_timeout: done
    exit_when: { kind: rule }
    next: done
  - id: done
    terminal: true
";

        const IR_JOIN: &str = r#"{
            "ir_version": 0,
            "id": "parity-join",
            "version": 1,
            "revision": 0,
            "state": "candidate",
            "meta": {"description": "parity join fixture", "kind": "creator", "run_intents": [], "requires_capabilities": []},
            "entry": "assess",
            "terminal": ["done"],
            "nodes": [
                {"id": "assess", "kind": "outer_state", "effect_class": "external",
                 "exit_when": {"kind": "llm_judge", "template": "prompts/parity-judge.md", "judge_capability": "parity.judge"}},
                {"id": "gate", "kind": "outer_state", "effect_class": "pure",
                 "merge": {"kind": "all"},
                 "converge": {"strategy": "wait_for_all"},
                 "timeout_ms": 600000,
                 "on_timeout": "done",
                 "exit_when": {"kind": "rule"}},
                {"id": "done", "kind": "terminal", "effect_class": "pure"}
            ],
            "edges": [
                {"id": "assess->gate:go", "from": "assess", "to": "gate", "kind": "linear", "label": "go"},
                {"id": "assess->gate:keep", "from": "assess", "to": "gate", "kind": "linear", "label": "keep"},
                {"id": "gate->done", "from": "gate", "to": "done", "kind": "linear"}
            ],
            "nests": [], "assets": [], "signals": [], "roles": [],
            "effect_summary": "external",
            "provenance": {"source": "compose"}
        }"#;

        /// `terminal[]` names an `outer_state`: the declaration, not the node
        /// kind, is what makes the task end the run.
        const YAML_TERMINAL: &str = r"
preset:
  id: parity-terminal
  version: 1
  kind: creator
  description: parity terminal fixture
  requires_capabilities: []
  initial: only
  terminal: only
states:
  - id: only
    terminal: true
";

        const IR_TERMINAL: &str = r#"{
            "ir_version": 0,
            "id": "parity-terminal",
            "version": 1,
            "revision": 0,
            "state": "candidate",
            "meta": {"description": "parity terminal fixture", "kind": "creator", "run_intents": [], "requires_capabilities": []},
            "entry": "only",
            "terminal": ["only"],
            "nodes": [{"id": "only", "kind": "outer_state", "effect_class": "pure"}],
            "edges": [],
            "nests": [], "assets": [], "signals": [], "roles": [],
            "effect_summary": "pure",
            "provenance": {"source": "compose"}
        }"#;

        // ----- scripted effects -------------------------------------------

        /// Shared logs for everything a scripted run can observe.
        #[derive(Default, Clone)]
        struct Recorder {
            capabilities: Arc<Mutex<Vec<(String, serde_json::Value)>>>,
            prompts: Arc<Mutex<Vec<(String, String)>>>,
            host_tools: Arc<Mutex<Vec<String>>>,
        }

        #[derive(Debug, PartialEq)]
        struct Recorded {
            capabilities: Vec<(String, serde_json::Value)>,
            prompts: Vec<(String, String)>,
            host_tools: Vec<String>,
        }

        impl Recorder {
            /// Drain what ran since the last drain, so each side is compared
            /// against only its own effects.
            fn drain(&self) -> Recorded {
                Recorded {
                    capabilities: std::mem::take(
                        &mut *self.capabilities.lock().expect("capability log lock"),
                    ),
                    prompts: std::mem::take(&mut *self.prompts.lock().expect("prompt log lock")),
                    host_tools: std::mem::take(
                        &mut *self.host_tools.lock().expect("host tool log lock"),
                    ),
                }
            }
        }

        /// A capability with a fixed output that records each invocation.
        struct ScriptedCapability {
            name: &'static str,
            output: serde_json::Value,
            recorder: Recorder,
        }

        #[async_trait::async_trait]
        impl Capability for ScriptedCapability {
            fn name(&self) -> &'static str {
                self.name
            }

            fn input_schema(&self) -> &'static str {
                r#"{"type":"object"}"#
            }

            fn output_schema(&self) -> &'static str {
                r#"{"type":"object"}"#
            }

            async fn run(
                &self,
                input: serde_json::Value,
            ) -> Result<serde_json::Value, CapabilityError> {
                self.recorder
                    .capabilities
                    .lock()
                    .expect("capability log lock")
                    .push((self.name.to_string(), input));
                Ok(self.output.clone())
            }
        }

        fn scripted_registry(
            recorder: &Recorder,
            scripted: &[(&'static str, serde_json::Value)],
        ) -> Arc<CapabilityRegistry> {
            crate::capability::test_support::registry_with(
                scripted
                    .iter()
                    .map(|(name, output)| {
                        Box::new(ScriptedCapability {
                            name,
                            output: output.clone(),
                            recorder: recorder.clone(),
                        }) as Box<dyn Capability>
                    })
                    .collect(),
            )
        }

        /// Records each dispatched host-tool call by name.
        struct ScriptedDispatch {
            recorder: Recorder,
        }

        #[async_trait::async_trait]
        impl DaemonToolDispatch for ScriptedDispatch {
            async fn dispatch_tool(
                &self,
                tool_name: &str,
                _: &serde_json::Value,
                _: &str,
            ) -> Result<serde_json::Value, CapabilityError> {
                self.recorder
                    .host_tools
                    .lock()
                    .expect("host tool log lock")
                    .push(tool_name.to_string());
                Ok(serde_json::json!({"status": "ok"}))
            }
        }

        /// Records each inner-node prompt through the same seam production uses.
        struct ParityExecutor {
            recorder: Recorder,
        }

        #[async_trait::async_trait]
        impl PromptExecutor for ParityExecutor {
            async fn execute(
                &self,
                request: PromptRequest,
            ) -> Result<PromptResult, CapabilityError> {
                self.recorder
                    .prompts
                    .lock()
                    .expect("prompt log lock")
                    .push((request.task_id.clone(), request.prompt.clone()));
                Ok(PromptResult {
                    full_text: format!("{} output", request.task_id),
                    host_session_id: format!("host:{}", request.run_id),
                    operation_id: "parity-op".to_string(),
                })
            }
        }

        // ----- harness -----------------------------------------------------

        /// One intake's built graph, the manifest it was built from and its
        /// normalized projection.
        struct Built {
            graph: Graph,
            manifest: PresetManifest,
            projection: OuterGraphProjection,
        }

        /// One side of a comparison: build result plus what the scripted run
        /// observed.
        struct Evidence {
            built: Built,
            trace: Trace,
            recorded: Recorded,
        }

        struct Parity {
            recorder: Recorder,
            caps: Arc<CapabilityRegistry>,
            engine: Arc<dyn OrchestrationEngine>,
            dispatch: Option<Arc<dyn DaemonToolDispatch>>,
            executor: Option<Arc<dyn PromptExecutor>>,
            cancels: Arc<std::sync::RwLock<HashMap<String, tokio_util::sync::CancellationToken>>>,
        }

        impl Parity {
            fn scripted(scripted: &[(&'static str, serde_json::Value)]) -> Self {
                let recorder = Recorder::default();
                Self {
                    caps: scripted_registry(&recorder, scripted),
                    recorder,
                    engine: Arc::new(UnreachableEngine),
                    dispatch: None,
                    executor: None,
                    cancels: no_cancels(),
                }
            }

            /// Host-tool fixtures need the daemon dispatch slot filled.
            fn with_host_tool(scripted: &[(&'static str, serde_json::Value)]) -> Self {
                let mut parity = Self::scripted(scripted);
                parity.dispatch = Some(Arc::new(ScriptedDispatch {
                    recorder: parity.recorder.clone(),
                }));
                parity
            }

            /// Inner-graph fixtures need a child-session engine and a prompt
            /// executor: the same wiring production injects.
            fn with_inner_graphs() -> Self {
                let recorder = Recorder::default();
                let cancels = no_cancels();
                Self {
                    caps: scripted_registry(&recorder, &[]),
                    engine: Arc::new(ChildRunningEngine {
                        children: tokio::sync::Mutex::new(HashMap::new()),
                        session_cancels: cancels.clone(),
                    }),
                    recorder: recorder.clone(),
                    dispatch: None,
                    executor: Some(Arc::new(ParityExecutor { recorder })),
                    cancels,
                }
            }

            fn yaml_side(&self, yaml: &str) -> Built {
                let loaded = load_preset_from_str(yaml, self.caps.as_ref())
                    .expect("parity YAML preset loads");
                let graph = build_wired_outer_graph(
                    &loaded,
                    &self.engine,
                    &self.caps,
                    self.dispatch.clone(),
                    self.executor.clone(),
                    self.cancels.clone(),
                )
                .expect("YAML wired outer graph builds");
                let manifest = loaded.manifest;
                let projection = outer_graph_projection(&graph, &manifest);
                Built {
                    graph,
                    manifest,
                    projection,
                }
            }

            fn ir_side(&self, ir: &str) -> Built {
                let document = ir_document(ir);
                let graph = build_wired_outer_graph_from_ir(
                    &document,
                    &self.engine,
                    &self.caps,
                    self.dispatch.clone(),
                    self.executor.clone(),
                    self.cancels.clone(),
                )
                .expect("IR outer graph builds");
                let mut manifest =
                    nexus_preset::export_preset(&document).expect("document projects onto states");
                // The builder flags `terminal[]` on the projected states itself.
                // The projection needs the same manifest, so the flag is mirrored
                // here; the scripted trace is what proves the builder set it (a
                // builder that skipped it returns `Continue`, not `End`).
                for state in &mut manifest.states {
                    state.terminal |= document.terminal.iter().any(|id| id == &state.id);
                }
                let projection = outer_graph_projection(&graph, &manifest);
                Built {
                    graph,
                    manifest,
                    projection,
                }
            }

            /// Compare the normalized projections and the scripted traces of
            /// the YAML and IR intakes for one workflow pair.
            async fn compare(
                &self,
                yaml: &str,
                ir: &str,
                initial: &[(&str, serde_json::Value)],
                probes: &[&str],
            ) -> (Evidence, Evidence) {
                let yaml_built = self.yaml_side(yaml);
                let ir_built = self.ir_side(ir);
                assert_projection_parity(&yaml_built, &ir_built);

                let yaml_trace = run_trace(&yaml_built, initial, probes).await;
                let yaml_recorded = self.recorder.drain();
                let ir_trace = run_trace(&ir_built, initial, probes).await;
                let ir_recorded = self.recorder.drain();

                assert_eq!(
                    yaml_trace, ir_trace,
                    "§8.2/§8.2.1 trace parity: task invocation order, selected edges, NextAction, waits, terminal"
                );
                assert_eq!(
                    yaml_recorded, ir_recorded,
                    "§8.2/§8.2.1 effect parity: capability, host-tool and prompt invocations"
                );
                (
                    Evidence {
                        built: yaml_built,
                        trace: yaml_trace,
                        recorded: yaml_recorded,
                    },
                    Evidence {
                        built: ir_built,
                        trace: ir_trace,
                        recorded: ir_recorded,
                    },
                )
            }
        }

        /// Tasks ordered by id: build order is not a runner observable (the
        /// graph stores tasks in a map), and the IR fixture may declare its
        /// entry node anywhere in `nodes[]`.
        fn sorted_tasks(projection: &OuterGraphProjection) -> Vec<ProjectedTask> {
            let mut tasks = projection.tasks.clone();
            tasks.sort_by(|left, right| left.id.cmp(&right.id));
            tasks
        }

        /// Edges grouped by source state with declaration order preserved
        /// inside a group — conditional rule order is first-match semantics.
        fn routing_edges(projection: &OuterGraphProjection) -> Vec<ProjectedEdge> {
            let mut edges = projection.edges.clone();
            edges.sort_by(|left, right| left.from.cmp(&right.from));
            edges
        }

        fn sorted_terminals(projection: &OuterGraphProjection) -> Vec<String> {
            let mut terminals = projection.terminals.clone();
            terminals.sort();
            terminals
        }

        fn assert_projection_parity(yaml: &Built, ir: &Built) {
            assert_eq!(
                yaml.projection.graph_id, ir.projection.graph_id,
                "both intakes build the graph under the workflow id"
            );
            assert_eq!(
                yaml.projection.entry, ir.projection.entry,
                "§8.2.1 entry: the initial outer Task id"
            );
            assert_eq!(
                sorted_tasks(&yaml.projection),
                sorted_tasks(&ir.projection),
                "§8.2/§8.2.1 states → tasks: id, kind, enter actions, exit/join/context-update carriers"
            );
            assert_eq!(
                routing_edges(&yaml.projection),
                routing_edges(&ir.projection),
                "§8.2/§8.2.1 edges: (from, to, kind, label-or-when) in per-state declaration order"
            );
            assert_eq!(
                sorted_terminals(&yaml.projection),
                sorted_terminals(&ir.projection),
                "§8.2.1 terminal[] / §8.2 terminal: tasks that end the run"
            );
            assert_eq!(
                yaml.projection.inner_graphs, ir.projection.inner_graphs,
                "§8.2/§8.2.1 inner graphs: nodes, templates, roles, depends_on edges, output binding"
            );
        }

        #[derive(Debug, PartialEq)]
        struct TraceStep {
            task: String,
            action: &'static str,
            next: Option<String>,
        }

        #[derive(Debug, PartialEq)]
        struct Trace {
            steps: Vec<TraceStep>,
            probes: BTreeMap<String, serde_json::Value>,
        }

        const MAX_TRACE_STEPS: usize = 16;

        /// Drive a graph the way the runner does: one `execute_session` per
        /// step. `WaitForInput` is followed by the engine's advance signal
        /// (the resumed key the composite task reads), so waits are part of the
        /// compared trace instead of a dead end.
        async fn run_trace(
            built: &Built,
            initial: &[(&str, serde_json::Value)],
            probes: &[&str],
        ) -> Trace {
            let start = built
                .graph
                .start_task_id()
                .expect("graph declares a start task");
            let mut session = Session::new_from_task("parity-trace".to_string(), start);
            for (key, value) in initial {
                session
                    .context
                    .set(*key, value.clone())
                    .expect("initial trace context");
            }

            let mut steps = Vec::new();
            for _ in 0..MAX_TRACE_STEPS {
                let task = session.current_task_id.clone();
                let result = built
                    .graph
                    .execute_session(&mut session)
                    .await
                    .expect("scripted graph runs");
                match result.status {
                    ExecutionStatus::Completed => {
                        steps.push(TraceStep {
                            task,
                            action: "end",
                            next: None,
                        });
                        return Trace {
                            steps,
                            probes: probe(&session.context, probes),
                        };
                    }
                    ExecutionStatus::WaitingForInput => {
                        session
                            .context
                            .set(format!("_state_{task}_resumed"), true)
                            .expect("resume signal");
                        steps.push(TraceStep {
                            task,
                            action: "wait",
                            next: None,
                        });
                    }
                    ExecutionStatus::Paused { next_task_id, .. } => {
                        steps.push(TraceStep {
                            task,
                            action: "continue",
                            next: Some(next_task_id),
                        });
                    }
                }
            }
            panic!("scripted trace did not terminate within {MAX_TRACE_STEPS} steps: {steps:?}");
        }

        fn probe(context: &Context, keys: &[&str]) -> BTreeMap<String, serde_json::Value> {
            keys.iter()
                .map(|key| {
                    (
                        (*key).to_string(),
                        context
                            .get::<serde_json::Value>(key)
                            .unwrap_or(serde_json::Value::Null),
                    )
                })
                .collect()
        }

        fn action_label(action: &NextAction) -> String {
            match action {
                NextAction::End => "end".to_string(),
                NextAction::WaitForInput => "wait".to_string(),
                NextAction::Continue => "continue".to_string(),
                NextAction::ContinueAndExecute => "continue_and_execute".to_string(),
                NextAction::GoTo(target) => format!("goto:{target}"),
            }
        }

        /// Run one state task directly with a scripted context and return its
        /// observable outcome. Join gates need externally supplied arrivals, so
        /// they are compared here instead of through a whole-graph trace.
        async fn run_state(
            graph: &Graph,
            state: &str,
            seed: &[(&str, serde_json::Value)],
        ) -> (String, Option<String>) {
            let context = Context::new();
            for (key, value) in seed {
                context.set(*key, value.clone()).expect("scripted context");
            }
            let result = graph
                .get_task(state)
                .expect("state task is wired")
                .run(context)
                .await
                .expect("state runs");
            (action_label(&result.next_action), result.response)
        }

        fn expected_steps(steps: &[(&str, &'static str, Option<&str>)]) -> Vec<TraceStep> {
            steps
                .iter()
                .map(|(task, action, next)| TraceStep {
                    task: (*task).to_string(),
                    action,
                    next: next.map(ToString::to_string),
                })
                .collect()
        }

        // ----- §8.2.1 rows -------------------------------------------------

        #[tokio::test]
        async fn parity_entry_selects_the_declared_initial_task() {
            // §8.2.1 `entry` → the initial outer Task. The IR fixture lists
            // `review` first, so `entry: plan` cannot be satisfied positionally
            // (the YAML side is positional and must therefore declare `plan`
            // first). Removing `set_start_task` from the IR builder starts the
            // IR run at `review` and fails this test.
            let parity =
                Parity::scripted(&[("parity.record", serde_json::json!({"recorded": "plan"}))]);
            let (yaml, ir) = parity.compare(YAML_LINEAR, IR_LINEAR, &[], &[]).await;

            assert_eq!(ir.built.manifest.states[0].id, "review");
            assert_eq!(yaml.built.projection.entry.as_deref(), Some("plan"));
            assert_eq!(
                ir.built.projection.entry.as_deref(),
                Some("plan"),
                "§8.2.1 entry selects the initial outer task by id, not by position"
            );
            assert_eq!(
                yaml.trace.steps.first().map(|step| step.task.as_str()),
                Some("plan"),
                "the runner starts at the entry task"
            );
        }

        #[tokio::test]
        async fn parity_outer_state_nodes_become_one_task_each() {
            // §8.2.1 `nodes[kind=outer_state]` / §8.2 `states[].id`.
            let parity =
                Parity::scripted(&[("parity.record", serde_json::json!({"recorded": "plan"}))]);
            let (yaml, ir) = parity.compare(YAML_LINEAR, IR_LINEAR, &[], &[]).await;

            assert_eq!(
                sorted_tasks(&ir.built.projection)
                    .iter()
                    .map(|task| (task.id.as_str(), task.kind))
                    .collect::<Vec<_>>(),
                [
                    ("done", "terminal"),
                    ("plan", "outer_state"),
                    ("review", "outer_state")
                ],
                "one outer task per state, plus the terminal"
            );
            assert_eq!(
                ir.trace.steps.len(),
                yaml.trace.steps.len(),
                "the same number of states execute on both intakes"
            );
        }

        #[tokio::test]
        async fn parity_capability_enter_action_invokes_the_capability() {
            // §8.2.1 `outer_state.enter[kind=capability]` / §8.2
            // `states[].enter[*].kind=capability` → `CapabilityTask`.
            let parity =
                Parity::scripted(&[("parity.record", serde_json::json!({"recorded": "plan"}))]);
            let (yaml, ir) = parity
                .compare(YAML_LINEAR, IR_LINEAR, &[], &["_capability_output"])
                .await;

            assert_eq!(
                ir.built
                    .projection
                    .tasks
                    .iter()
                    .find(|task| task.id == "plan")
                    .expect("plan task is projected")
                    .enter,
                [
                    serde_json::json!({"kind": "capability", "name": "parity.record", "args": {"note": "plan"}})
                ],
                "the capability enter action is carried in declaration order"
            );
            assert_eq!(
                ir.recorded.capabilities,
                [(
                    ("parity.record").to_string(),
                    serde_json::json!({"note": "plan"})
                )],
                "the state's enter action invokes the named capability once with its args"
            );
            assert_eq!(
                ir.trace.probes["_capability_output"],
                serde_json::json!({"recorded": "plan"}),
                "the capability output lands in the context the next state reads"
            );
            assert_eq!(ir.recorded.capabilities, yaml.recorded.capabilities);
        }

        #[tokio::test]
        async fn parity_host_tool_enter_action_dispatches_in_process() {
            // §8.2.1 `outer_state.enter[kind=host_tool]` → `HostToolCallTask`
            // invoked inline (never a graph node), failing closed without the
            // injected dispatch.
            let parity = Parity::with_host_tool(&[]);
            let (yaml, ir) = parity.compare(YAML_HOST_TOOL, IR_HOST_TOOL, &[], &[]).await;

            assert_eq!(
                ir.recorded.host_tools,
                ["nexus.orchestration.schedule_status"],
                "the host tool is dispatched once through the daemon seam"
            );
            assert_eq!(ir.recorded.host_tools, yaml.recorded.host_tools);
            assert_eq!(
                ir.built
                    .projection
                    .tasks
                    .iter()
                    .map(|task| task.id.as_str())
                    .collect::<Vec<_>>(),
                ["status", "done"],
                "host_tool creates no extra graph task"
            );
        }

        #[tokio::test]
        async fn parity_inner_graph_enter_action_launches_the_child_graph() {
            // §8.2.1 `enter[kind=inner_graph]` / §8.2 same → `InnerGraphTask`
            // over the attached child graph.
            let parity = Parity::with_inner_graphs();
            let (_, ir) = parity
                .compare(
                    YAML_INNER,
                    IR_INNER,
                    &[("_session_id", serde_json::json!("run:1"))],
                    &["state.generate.output"],
                )
                .await;

            assert_eq!(
                ir.built
                    .projection
                    .tasks
                    .iter()
                    .find(|task| task.id == "generate")
                    .expect("generate task is projected")
                    .enter,
                [serde_json::json!({"kind": "inner_graph", "name": "generate_graph"})]
            );
            assert_eq!(
                ir.recorded.prompts.len(),
                2,
                "the enter action launched the child graph and it ran its nodes"
            );
        }

        #[tokio::test]
        async fn parity_inner_step_acp_prompt_nodes_become_inner_tasks() {
            // §8.2.1 `nodes[kind=inner_step] step_kind=acp_prompt` / §8.2
            // `inner_graphs.<name>.nodes[].kind=acp_prompt` → `AcpPromptTask`.
            let parity = Parity::with_inner_graphs();
            let (_, ir) = parity
                .compare(
                    YAML_INNER,
                    IR_INNER,
                    &[("_session_id", serde_json::json!("run:1"))],
                    &[],
                )
                .await;

            let inner = &ir.built.projection.inner_graphs[0];
            assert_eq!(inner.name, "generate_graph");
            assert_eq!(
                inner
                    .nodes
                    .iter()
                    .map(|node| (
                        node.id.as_str(),
                        node.kind,
                        node.template_file.as_deref(),
                        node.agent.as_deref()
                    ))
                    .collect::<Vec<_>>(),
                [
                    (
                        "generate_with_context",
                        "acp_prompt",
                        Some("prompts/generate-with-memory.md"),
                        None
                    ),
                    (
                        "refine",
                        "acp_prompt",
                        Some("prompts/recall-memory.md"),
                        Some("writer")
                    )
                ],
                "each inner step keeps its template path and role binding"
            );
            assert_eq!(
                ir.recorded
                    .prompts
                    .iter()
                    .map(|(task_id, _)| task_id.as_str())
                    .collect::<Vec<_>>(),
                ["generate_with_context", "refine"]
            );
        }

        #[tokio::test]
        async fn parity_depends_on_edges_order_inner_nodes() {
            // §8.2.1 `edges[kind=depends_on]` / `inner_step.depends_on` / §8.2
            // `depends_on` → `add_edge(dependency, step)` in the inner graph.
            let parity = Parity::with_inner_graphs();
            let (_, ir) = parity
                .compare(
                    YAML_INNER,
                    IR_INNER,
                    &[("_session_id", serde_json::json!("run:1"))],
                    &[],
                )
                .await;

            assert_eq!(
                ir.built.projection.inner_graphs[0]
                    .edges
                    .iter()
                    .map(|edge| (edge.from.as_str(), edge.to.as_str(), edge.kind))
                    .collect::<Vec<_>>(),
                [("generate_with_context", "refine", "depends_on")],
                "the dependency is projected as a dependency edge"
            );
            assert_eq!(
                ir.recorded
                    .prompts
                    .iter()
                    .map(|(task_id, _)| task_id.as_str())
                    .collect::<Vec<_>>(),
                ["generate_with_context", "refine"],
                "the wired edge is what runs the dependency before its dependant"
            );
        }

        #[tokio::test]
        async fn parity_output_binding_writes_the_state_output() {
            // §8.2.1 `output_binding` / §8.2
            // `inner_graphs.<name>.output_binding` → `InnerGraphTask` post-run
            // export to `state.<state>.output`.
            let parity = Parity::with_inner_graphs();
            let (_, ir) = parity
                .compare(
                    YAML_INNER,
                    IR_INNER,
                    &[("_session_id", serde_json::json!("run:1"))],
                    &["state.generate.output"],
                )
                .await;

            assert_eq!(
                ir.built.projection.inner_graphs[0]
                    .output_binding
                    .as_deref(),
                Some("refine.text"),
                "the binding travels with the inner graph"
            );
            assert_eq!(
                ir.trace.probes["state.generate.output"],
                serde_json::json!("refine output"),
                "the bound node's output is exported onto the outer state"
            );
        }

        #[tokio::test]
        async fn parity_exit_when_rule_continues() {
            // §8.2.1 `exit_when.kind=rule` / §8.2 → `RuleCheckTask` semantics;
            // an unset `_rule` is the unit rule and always transitions.
            let parity =
                Parity::scripted(&[("parity.record", serde_json::json!({"recorded": "plan"}))]);
            let (_, ir) = parity.compare(YAML_LINEAR, IR_LINEAR, &[], &[]).await;

            assert_eq!(
                ir.built
                    .projection
                    .tasks
                    .iter()
                    .find(|task| task.id == "review")
                    .expect("review task is projected")
                    .exit_when,
                Some(serde_json::json!({"kind": "rule"}))
            );
            assert_eq!(
                ir.trace.steps,
                expected_steps(&[
                    ("plan", "continue", Some("review")),
                    ("review", "continue", Some("done")),
                    ("done", "end", None),
                ]),
                "rule exits continue instead of waiting"
            );
        }

        #[tokio::test]
        async fn parity_exit_when_manual_waits_then_resumes() {
            // §8.2.1 `exit_when.kind=manual` / §8.2 → `ManualWaitTask`
            // returning `NextAction::WaitForInput`.
            let parity = Parity::scripted(&[]);
            let (_, ir) = parity.compare(YAML_WAIT, IR_WAIT, &[], &[]).await;

            assert_eq!(
                ir.built
                    .projection
                    .tasks
                    .iter()
                    .find(|task| task.id == "handoff")
                    .expect("handoff task is projected")
                    .exit_when,
                Some(serde_json::json!({"kind": "manual"}))
            );
            assert_eq!(
                ir.trace.steps.first(),
                Some(&TraceStep {
                    task: "handoff".to_string(),
                    action: "wait",
                    next: None,
                }),
                "a manual exit waits for input before advancing"
            );
            assert_eq!(
                ir.trace.steps,
                expected_steps(&[
                    ("handoff", "wait", None),
                    ("handoff", "continue", Some("tick")),
                    ("tick", "wait", None),
                    ("tick", "continue", Some("done")),
                    ("done", "end", None),
                ]),
                "the resumed state continues to its declared successor"
            );
        }

        #[tokio::test]
        async fn parity_exit_when_timer_waits_for_input() {
            // §8.2.1 `exit_when.kind=timer` → `TimerWaitTask`, returning
            // `WaitForInput` (the clock signal is DR-12).
            let parity = Parity::scripted(&[]);
            let (_, ir) = parity.compare(YAML_WAIT, IR_WAIT, &[], &[]).await;

            assert_eq!(
                ir.built
                    .projection
                    .tasks
                    .iter()
                    .find(|task| task.id == "tick")
                    .expect("tick task is projected")
                    .exit_when,
                Some(serde_json::json!({"kind": "timer", "duration": "PT1H"})),
                "the timer duration is carried, not dropped"
            );
            assert!(
                ir.trace
                    .steps
                    .iter()
                    .any(|step| step.task == "tick" && step.action == "wait"),
                "a timer exit waits for input: {:?}",
                ir.trace.steps
            );
        }

        #[tokio::test]
        async fn parity_exit_when_llm_judge_go_continues_and_nogo_waits() {
            // §8.2.1 `exit_when.kind=llm_judge` / §8.2 → `JudgeTask` semantics
            // after the enter actions: go continues, no-go does not.
            let go = Parity::scripted(&[(
                "parity.judge",
                serde_json::json!({"result": true, "reason": "parity go"}),
            )]);
            let (_, go_ir) = go
                .compare(
                    YAML_JUDGE,
                    IR_JUDGE,
                    &[],
                    &["_judge_result", "_judge_label"],
                )
                .await;
            assert_eq!(
                go_ir
                    .built
                    .projection
                    .tasks
                    .iter()
                    .find(|task| task.id == "assess")
                    .expect("assess task is projected")
                    .exit_when,
                Some(serde_json::json!({
                    "kind": "llm_judge",
                    "template_file": "prompts/parity-judge.md",
                    "judge_capability": "parity.judge",
                }))
            );
            assert_eq!(
                go_ir.trace.steps,
                expected_steps(&[
                    ("assess", "continue", Some("pass")),
                    ("pass", "continue", Some("done")),
                    ("done", "end", None),
                ]),
                "a go verdict takes the go branch"
            );
            assert_eq!(go_ir.trace.probes["_judge_result"], serde_json::json!(true));

            let nogo = Parity::scripted(&[(
                "parity.judge",
                serde_json::json!({"result": false, "reason": "parity nogo"}),
            )]);
            let (_, nogo_ir) = nogo
                .compare(
                    YAML_JUDGE,
                    IR_JUDGE,
                    &[],
                    &["_judge_result", "_judge_label"],
                )
                .await;
            assert_eq!(
                nogo_ir.trace.steps,
                expected_steps(&[
                    ("assess", "continue", Some("revise")),
                    ("revise", "continue", Some("done")),
                    ("done", "end", None),
                ]),
                "a no-go verdict takes the nogo branch"
            );
            assert_eq!(
                nogo_ir.trace.probes["_judge_result"],
                serde_json::json!(false)
            );
        }

        #[tokio::test]
        async fn parity_exit_when_graph_complete_continues_after_the_inner_graph() {
            // §8.2.1 `exit_when.kind=graph_complete` / §8.2 → the inner graph
            // completion lets the outer task return `Continue`.
            let parity = Parity::with_inner_graphs();
            let (_, ir) = parity
                .compare(
                    YAML_INNER,
                    IR_INNER,
                    &[("_session_id", serde_json::json!("run:1"))],
                    &[],
                )
                .await;

            assert_eq!(
                ir.built
                    .projection
                    .tasks
                    .iter()
                    .find(|task| task.id == "generate")
                    .expect("generate task is projected")
                    .exit_when,
                Some(serde_json::json!({"kind": "graph_complete"}))
            );
            assert_eq!(
                ir.trace.steps,
                expected_steps(&[
                    ("generate", "continue", Some("done")),
                    ("done", "end", None),
                ]),
                "graph_complete continues to the declared successor"
            );
        }

        #[tokio::test]
        async fn parity_join_gate_carries_merge_converge_and_timeout() {
            // §8.2.1 `nodes[kind=join]`, `merge`, `converge`, `timeout_ms`,
            // `on_timeout` → the `StateCompositeTask` join gate. `nodes[kind=
            // join]` itself has no preset projection (`export_preset` refuses
            // it; the named refusal is Task 4), so the join is expressed by an
            // `outer_state` carrying the join carriers — which is what the
            // builder maps. Arrivals are supplied directly because a whole-graph
            // trace cannot fabricate the second branch's arrival.
            let parity = Parity::scripted(&[(
                "parity.judge",
                serde_json::json!({"result": true, "reason": "parity go"}),
            )]);
            let yaml = parity.yaml_side(YAML_JOIN);
            let ir = parity.ir_side(IR_JOIN);
            assert_projection_parity(&yaml, &ir);

            assert_eq!(
                ir.projection
                    .tasks
                    .iter()
                    .find(|task| task.id == "gate")
                    .expect("gate task is projected")
                    .join,
                Some(serde_json::json!({
                    "merge": {"kind": "all"},
                    "converge": {"strategy": "wait_for_all"},
                    "timeout_ms": 600_000,
                    "on_timeout": "done",
                })),
                "the join gate keeps its merge, converge, deadline and reroute"
            );

            let partial: [(&str, serde_json::Value); 2] = [
                ("_merge_gate", serde_json::json!(["go"])),
                ("_converge_arrivals_gate", serde_json::json!(["assess"])),
            ];
            let complete: [(&str, serde_json::Value); 2] = [
                ("_merge_gate", serde_json::json!(["go", "keep"])),
                ("_converge_arrivals_gate", serde_json::json!(["assess"])),
            ];

            assert_eq!(
                run_state(&yaml.graph, "gate", &partial).await,
                run_state(&ir.graph, "gate", &partial).await,
                "a partial join waits identically on both intakes"
            );
            assert_eq!(
                run_state(&yaml.graph, "gate", &partial).await.0,
                "wait",
                "one of two labeled arrivals holds the gate"
            );
            assert_eq!(
                run_state(&yaml.graph, "gate", &complete).await,
                run_state(&ir.graph, "gate", &complete).await,
                "a satisfied join advances identically on both intakes"
            );
            assert_eq!(
                run_state(&yaml.graph, "gate", &complete).await.0,
                "continue"
            );
        }

        #[tokio::test]
        async fn parity_terminal_declaration_ends_the_run() {
            // §8.2.1 `terminal[]` or `nodes[kind=terminal]` / §8.2
            // `terminal: <id>` → the task returns `NextAction::End`. The IR
            // fixture declares `terminal: ["only"]` on an `outer_state`, so
            // removing the builder's terminal-flag branch leaves it
            // `outer_state`, and the run pauses instead of ending.
            let parity = Parity::scripted(&[]);
            let (_, ir) = parity.compare(YAML_TERMINAL, IR_TERMINAL, &[], &[]).await;

            assert_eq!(
                sorted_terminals(&ir.built.projection),
                ["only"],
                "the declared terminal is the task that ends the run"
            );
            assert_eq!(
                ir.built.projection.tasks[0].kind, "terminal",
                "terminal[] flags the projected state"
            );
            assert_eq!(
                ir.trace.steps,
                expected_steps(&[("only", "end", None)]),
                "terminal[] on an outer_state returns End"
            );
        }

        #[tokio::test]
        async fn parity_edges_linear_go_and_nogo_route_identically() {
            // §8.2.1 `edges[kind=linear|go|nogo|default]` / §8.2 `next: <id>`
            // → outer edges plus the source task's `NextAction` routing.
            let linear =
                Parity::scripted(&[("parity.record", serde_json::json!({"recorded": "plan"}))]);
            let (_, linear_ir) = linear.compare(YAML_LINEAR, IR_LINEAR, &[], &[]).await;
            assert_eq!(
                routing_edges(&linear_ir.built.projection)
                    .iter()
                    .map(|edge| (edge.from.as_str(), edge.to.as_str(), edge.kind))
                    .collect::<Vec<_>>(),
                [("plan", "review", "linear"), ("review", "done", "linear")],
                "linear edges are projected from both intakes"
            );
            assert_eq!(
                linear_ir.trace.steps,
                expected_steps(&[
                    ("plan", "continue", Some("review")),
                    ("review", "continue", Some("done")),
                    ("done", "end", None),
                ]),
                "each Continue selects its linear successor at run time"
            );

            let judge = Parity::scripted(&[(
                "parity.judge",
                serde_json::json!({"result": true, "reason": "parity go"}),
            )]);
            let (_, judge_ir) = judge.compare(YAML_JUDGE, IR_JUDGE, &[], &[]).await;
            assert_eq!(
                routing_edges(&judge_ir.built.projection)
                    .iter()
                    .map(|edge| (edge.from.as_str(), edge.to.as_str(), edge.kind))
                    .collect::<Vec<_>>(),
                [
                    ("assess", "pass", "go"),
                    ("assess", "revise", "nogo"),
                    ("pass", "done", "linear"),
                    ("revise", "done", "linear")
                ],
                "go/nogo edges are projected with their branch kind"
            );
        }

        #[tokio::test]
        async fn parity_edges_conditional_and_default_route_by_expression() {
            // §8.2.1 `edges[kind=conditional]` / §8.2 `next.kind=conditional`
            // → the source task evaluates the branch; the graph adds no edge for
            // the taken branch, so the selected target is what the trace pins.
            let mut evidence = Vec::new();
            for score in [serde_json::json!(5), serde_json::json!(0)] {
                let parity = Parity::scripted(&[]);
                let (_, ir) = parity
                    .compare(
                        YAML_CONDITIONAL,
                        IR_CONDITIONAL,
                        &[("score", score.clone())],
                        &[],
                    )
                    .await;
                evidence.push((score, ir));
            }

            let (_, first) = &evidence[0];
            assert_eq!(
                routing_edges(&first.built.projection)
                    .iter()
                    .map(|edge| (
                        edge.from.as_str(),
                        edge.to.as_str(),
                        edge.kind,
                        edge.detail.as_deref()
                    ))
                    .collect::<Vec<_>>(),
                [
                    (
                        "review",
                        "approve",
                        "conditional",
                        Some("_context.score > 1")
                    ),
                    ("review", "revise", "default", None),
                    ("revise", "approve", "linear", None)
                ],
                "the branch expression and the default are both projected"
            );
            assert_eq!(
                first.trace.steps,
                expected_steps(&[
                    ("review", "continue", Some("approve")),
                    ("approve", "end", None),
                ]),
                "score 5 matches the conditional branch"
            );

            let (_, second) = &evidence[1];
            assert_eq!(
                second.trace.steps,
                expected_steps(&[
                    ("review", "continue", Some("revise")),
                    ("revise", "continue", Some("approve")),
                    ("approve", "end", None),
                ]),
                "score 0 falls through to the default target"
            );
        }

        #[tokio::test]
        async fn parity_non_execution_fields_create_no_tasks() {
            // §8.2.1 `initial_action`, `signals[]`, `roles[]`, `assets[]`,
            // `effect_class`/`effect_summary`, `meta.*` → no graph `Task`.
            // These rows carry data the runner consumes around task execution
            // (or not at all), so parity is asserted on the carriers the builder
            // hands the runtime plus the unchanged task shape and trace.
            let parity = Parity::scripted(&[]);
            let (yaml, ir) = parity.compare(YAML_METADATA, IR_METADATA, &[], &[]).await;

            assert_eq!(
                wire(&yaml.built.manifest.preset),
                wire(&ir.built.manifest.preset),
                "§8.2.1 initial_action / meta.run_intents / meta.cli_args / meta.gates keep their header carriers"
            );
            assert_eq!(
                wire(&yaml.built.manifest.signals),
                wire(&ir.built.manifest.signals),
                "§8.2.1 signals[] keep their bindings"
            );
            assert_eq!(
                wire(&yaml.built.manifest.roles),
                wire(&ir.built.manifest.roles),
                "§8.2.1 roles[] keep their definitions"
            );
            assert_eq!(
                ir.built
                    .projection
                    .tasks
                    .iter()
                    .map(|task| task.id.as_str())
                    .collect::<Vec<_>>(),
                ["only", "done"],
                "§8.2.1 assets[]/effect_class/effect_summary/meta.* add no graph task"
            );
            assert_eq!(
                ir.trace.steps,
                expected_steps(&[("only", "continue", Some("done")), ("done", "end", None),])
            );
        }
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
