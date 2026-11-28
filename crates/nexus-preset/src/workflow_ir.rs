//! `PresetManifest` ↔ [`WorkflowDocument`] import adapter (Workflow IR v0).
//!
//! Import is a projection, never a second grammar: every executable preset carrier (`signals`,
//! `roles`, `initial_action`, `gates`, `cli_args`, `world_binding`, inner-graph
//! `output_binding`, every `exit_when` kind) lands in an IR field, and [`export_preset`] rebuilds
//! the manifest from those fields alone. [`PresetSourceIdentity`](crate::source_identity::PresetSourceIdentity) is untouched —
//! this adapter neither reads nor redefines the preset content hash.
//!
//! v0 mapping choices:
//! - Asset id == bundle-relative path, so a node `template` and its `WorkflowAssetRef.id` share one
//!   address. `content_hash` is BLAKE3 over the asset bytes; the bytes ride alongside the document
//!   in [`PresetIrImport::assets`] because the IR itself carries only the reference.
//! - An inner-step node id is `<inner_graph>/<node_id>`: two graphs may reuse a node id, and IR
//!   node ids are document-unique.
//! - A state `description` rides on `WorkflowNodeUi::description` (canvas chrome, no execution
//!   meaning) — the node record carries no other text field.
//! - Preset carriers are classified at their worst case because v0 has no capability-effect
//!   catalog: a capability call, ACP step, host tool or LLM judge is `external`; a
//!   `context_update` hook is `write_candidate`; everything else is `pure`. This can only
//!   over-report, never hide an effect.
//! - Export refuses nodes it did not produce (pattern slots, nest groups, joins, `judge` and
//!   `manual_wait`) instead of inventing preset fields: a `judge` node flattened onto a preset
//!   state would either drop its judgment or emit `next: go/nogo` with no `exit_when`, which the
//!   preset validator rejects.
//!
//! shortcut: legacy `next: { kind: conditional, rules: [...] }` exports as the equivalent `branches` form, upgrade: when a consumer needs the legacy spelling back.

use crate::manifest::{
    ConditionalBranches, ConditionalRule, ContextUpdateHook, ContextUpdateOp, ConvergeConfig,
    ConvergeStrategy, EnterAction, ExitWhen, GoNogoNext, GraphNode, GraphNodeKind, InitialAction,
    InnerGraph, LabeledNext, MergeKind, NextTarget, PresetCliArg, PresetCliArgType, PresetHeader,
    PresetKind, PresetManifest, RunIntent, SignalAction, SignalActionKind, SignalBinding,
    StateDefinition, WorldBinding, WorldBindingMode,
};
use include_dir::Dir;
use nexus_contracts::local::orchestration::preset_gate::{Gate, GateOp, PreviousPresetStatus};
use nexus_workflow_ir::{
    WorkflowAssetRef, WorkflowAssetRefKind, WorkflowCliArg, WorkflowCliArgType,
    WorkflowContextUpdate, WorkflowContextUpdateOp, WorkflowConverge, WorkflowConvergeStrategy,
    WorkflowDocument, WorkflowDocumentId, WorkflowDocumentState, WorkflowEdge, WorkflowEdgeKind,
    WorkflowEnterAction, WorkflowEnterActionKind, WorkflowExitWhen, WorkflowExitWhenKind,
    WorkflowGate, WorkflowGateKind, WorkflowGateOp, WorkflowGateStatus, WorkflowInitialAction,
    WorkflowInitialActionKind, WorkflowMerge, WorkflowMergeKind, WorkflowMeta, WorkflowMetaKind,
    WorkflowMetaRunIntentsItem, WorkflowNode, WorkflowNodeEffectClass, WorkflowNodeKind,
    WorkflowNodeStepKind, WorkflowNodeUi, WorkflowProvenance, WorkflowProvenanceSource,
    WorkflowRoleDefinition, WorkflowSignalBinding, WorkflowSignalBindingAction,
    WorkflowWorldBinding, WorkflowWorldBindingMode,
};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::Path;
use thiserror::Error;

const IR_VERSION: u64 = nexus_workflow_ir::IR_VERSION;

/// Adapter failure. Every refusal names the carrier it could not project.
#[derive(Debug, Error)]
pub enum WorkflowIrError {
    /// A referenced asset could not be read from the bundle.
    #[error("failed to read referenced asset `{path}`: {source}")]
    AssetRead {
        /// Bundle-relative asset path.
        path: String,
        /// Underlying read error.
        #[source]
        source: std::io::Error,
    },
    /// The preset or the document cannot be represented in the target shape.
    #[error("workflow IR conversion refused: {reason}")]
    NotProjectable {
        /// What could not be represented.
        reason: String,
    },
    /// The manifest marks more than one state `terminal: true`; a preset header names exactly one.
    #[error("preset declares {count} terminal states; a v0 document carries exactly one")]
    MultipleTerminalStates {
        /// Number of states marked terminal.
        count: usize,
    },
    /// A judged state's `next` is a `Conditional`/`Branches` form carrying no rules. A judged
    /// node's single `default` edge is what an unconditional advance projects to, so the export
    /// cannot tell this form from a bare `next` and would rewrite it.
    #[error("`{state}` has a conditional next with no rules; a bare `next` is the supported form")]
    EmptyConditionalRules {
        /// State whose `next` carries no rules.
        state: String,
    },
}

fn refuse(reason: impl Into<String>) -> WorkflowIrError {
    WorkflowIrError::NotProjectable {
        reason: reason.into(),
    }
}

/// Canonical bytes of a committed IR twin: `document` as pretty JSON with every object key sorted,
/// plus one trailing newline.
///
/// The sort is what makes a twin reproducible. `serde_json::Map` keeps insertion order whenever
/// anything in the build enables `serde_json/preserve_order` (the `nexus42` graph does, through
/// `agent-client-protocol`); without it a `serde_json::Value` carrier such as an `enter` step's
/// `args` would serialize to different bytes in the CLI than in this crate's own tests. Values are
/// untouched — only key order is normalized — so a twin still carries exactly the imported
/// document.
///
/// # Errors
///
/// Returns the `serde_json` error when the document cannot be converted or serialized.
pub fn canonical_twin_json(document: &WorkflowDocument) -> Result<String, serde_json::Error> {
    let value = sort_object_keys(&serde_json::to_value(document)?);
    let mut json = serde_json::to_string_pretty(&value)?;
    json.push('\n');
    Ok(json)
}

/// Recursively sort every object's keys (arrays keep their order, scalars are untouched).
fn sort_object_keys(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(members) => {
            let mut entries: Vec<_> = members.iter().collect();
            entries.sort_by(|left, right| left.0.cmp(right.0));
            serde_json::Value::Object(
                entries
                    .into_iter()
                    .map(|(key, member)| (key.clone(), sort_object_keys(member)))
                    .collect(),
            )
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(sort_object_keys).collect())
        }
        scalar => scalar.clone(),
    }
}

/// An imported document plus the referenced asset bytes it hashed.
#[derive(Debug, Clone)]
pub struct PresetIrImport {
    /// The imported IR document.
    pub document: WorkflowDocument,
    /// Asset bytes keyed by asset id (the bundle-relative path).
    pub assets: BTreeMap<String, Vec<u8>>,
}

/// Where [`import_preset_bundle`] reads referenced asset bytes from.
///
/// A source answers bundle-relative paths only: `import_preset_bundle` applies
/// `assert_template_file_safe` before the first read, so no implementation repeats that check.
pub trait AssetSource {
    /// Bytes at a bundle-relative `path`; `Ok(None)` means the source has no such entry.
    ///
    /// # Errors
    ///
    /// Returns the source's own failure — a sandbox escape, a non-regular target, or IO.
    /// A missing entry is `Ok(None)` for an embedded source and the source's own
    /// `NotFound` error for a filesystem source; callers treat both as missing.
    fn read_asset(&self, path: &str) -> Result<Option<Vec<u8>>, std::io::Error>;
}

/// A bundle directory on disk, confined to its opened root descriptor.
#[derive(Debug)]
pub struct FsAssetSource {
    root: crate::bundle_root::BundleRoot,
}

impl FsAssetSource {
    /// Open `root` once; every read resolves relative to that descriptor and is
    /// accepted only while the asset's own descriptor path stays inside it.
    ///
    /// # Errors
    ///
    /// Returns the open error when `root` is not a traversable directory.
    pub fn new(root: &Path) -> std::io::Result<Self> {
        Ok(Self {
            root: crate::bundle_root::BundleRoot::open(root)?,
        })
    }
}

impl AssetSource for FsAssetSource {
    fn read_asset(&self, path: &str) -> Result<Option<Vec<u8>>, std::io::Error> {
        self.root.read(path).map(Some)
    }
}

/// Assets compiled into the binary, from a [`include_dir::Dir`].
///
/// Pass the preset's own bundle directory: entries carry their path from the `include_dir!` root, so
/// each lookup is prefixed with the directory's own path (`prompts/x.md` -> `<preset>/prompts/x.md`).
#[derive(Debug, Clone, Copy)]
pub struct EmbeddedAssetSource<'a>(pub &'a Dir<'a>);

impl AssetSource for EmbeddedAssetSource<'_> {
    fn read_asset(&self, path: &str) -> Result<Option<Vec<u8>>, std::io::Error> {
        Ok(self
            .0
            .get_file(self.0.path().join(path))
            .map(|file| file.contents().to_vec()))
    }
}

// Import: PresetManifest -> WorkflowDocument
// ---------------------------------------------------------------------------

/// Import a preset manifest into a Workflow IR document, hashing every asset the manifest
/// references from `source`.
///
/// # Errors
///
/// Returns [`WorkflowIrError::AssetRead`] when a referenced asset is missing, unreadable, or
/// refused by `source` (a [`FsAssetSource`] escape, for one),
/// [`WorkflowIrError::NotProjectable`] when a referenced path is unsafe (absolute, `..`
/// traversal), the manifest header cannot form a document, or an inner graph's
/// `output_binding` names no node by its owner segment, and
/// [`WorkflowIrError::EmptyConditionalRules`] when a judged state's `next` is a
/// `Conditional`/`Branches` form carrying no rules. A non-judged state's
/// `{branches: [], default: <id>}` next is accepted and re-exports in that same form.
pub fn import_preset_bundle(
    manifest: &PresetManifest,
    source: &dyn AssetSource,
) -> Result<PresetIrImport, WorkflowIrError> {
    let mut references = crate::validation::collect_asset_file_references(manifest);
    references.sort_by(|a, b| a.1.cmp(&b.1));
    references.dedup_by(|a, b| a.1 == b.1);

    for (_, path) in &references {
        if let Err(reason) = crate::loader::assert_template_file_safe(path) {
            return Err(refuse(format!("unsafe asset path `{path}`: {reason}")));
        }
    }

    let mut assets = Vec::with_capacity(references.len());
    let mut asset_bytes = BTreeMap::new();
    for (_, path) in references {
        let bytes = match source.read_asset(&path) {
            Ok(Some(bytes)) => bytes,
            Ok(None) => {
                return Err(WorkflowIrError::AssetRead {
                    path,
                    source: std::io::Error::from(std::io::ErrorKind::NotFound),
                })
            }
            Err(source) => return Err(WorkflowIrError::AssetRead { path, source }),
        };
        assets.push(WorkflowAssetRef {
            id: path.clone(),
            path: path.clone(),
            kind: WorkflowAssetRefKind::Template,
            content_hash: Some(blake3::hash(&bytes).to_hex().to_string()),
        });
        asset_bytes.insert(path, bytes);
    }

    let terminal_count = manifest
        .states
        .iter()
        .filter(|state| state.terminal)
        .count();
    if terminal_count > 1 {
        return Err(WorkflowIrError::MultipleTerminalStates {
            count: terminal_count,
        });
    }

    // The projection runs last on purpose: every asset read and the terminal count above have
    // already reported their own defects, so a projection refusal (an unbound owner, for one)
    // cannot mask a missing or unsafe asset. Manifest-only scans are deterministic, so this order
    // is stable across runs.
    Ok(PresetIrImport {
        document: document_from_manifest(manifest, assets)?,
        assets: asset_bytes,
    })
}

/// Project a manifest into IR for semantic validation: no filesystem reads.
///
/// Asset references stay on their carriers (node `template`s, exit/context templates, enter
/// args, role prompts) and are not registered in [`WorkflowDocument::assets`]: validation sweeps
/// the carriers, so an unregistered reference cannot bypass the asset checks.
///
/// Unlike [`import_preset_bundle`] this does not refuse a manifest whose terminal markers disagree
/// with `preset.terminal` or whose asset files are missing or unsafe — validation reports those
/// defects on the projected document instead.
///
/// # Errors
///
/// Returns [`WorkflowIrError::NotProjectable`] when `preset.id` is not a valid IR id or an inner
/// graph's `output_binding` names no node by its owner segment, and
/// [`WorkflowIrError::EmptyConditionalRules`] when a judged state's `next` is a
/// `Conditional`/`Branches` form carrying no rules. A non-judged state's
/// `{branches: [], default: <id>}` next is accepted and re-exports in that same form.
pub fn import_preset_manifest(
    manifest: &PresetManifest,
) -> Result<WorkflowDocument, WorkflowIrError> {
    document_from_manifest(manifest, Vec::new())
}

fn document_from_manifest(
    manifest: &PresetManifest,
    assets: Vec<WorkflowAssetRef>,
) -> Result<WorkflowDocument, WorkflowIrError> {
    // A step id must be one segment: node ids are `<inner_graph>/<step>`, so a slashed step id
    // would export it under a different graph.
    if let Some((graph_name, step_id)) = slashed_inner_step(manifest) {
        return Err(refuse(format!(
            "inner graph `{graph_name}` step id `{step_id}` contains '/'; an inner step id is a single segment"
        )));
    }
    if let Some((graph_name, binding, owner)) = unknown_output_owner(manifest) {
        return Err(refuse(format!(
            "inner graph `{graph_name}` output binding `{binding}` names owner `{owner}`, which is no node in the graph"
        )));
    }
    let header = &manifest.preset;
    let id = WorkflowDocumentId::try_from(header.id.clone())
        .map_err(|e| refuse(format!("preset id `{}` is not an IR id: {e}", header.id)))?;

    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut edge_ids = HashSet::new();
    for state in &manifest.states {
        nodes.push(state_node(state));
        push_next_edges(state, &mut edges, &mut edge_ids)?;
    }
    if let Some(graphs) = &manifest.inner_graphs {
        for (graph_name, graph) in graphs {
            let owner = graph.output_binding.as_deref().map(output_binding_owner);
            for step in &graph.nodes {
                nodes.push(inner_step_node(
                    graph_name,
                    owner == Some(step.id.as_str()),
                    graph,
                    step,
                ));
            }
        }
    }

    let effect_summary = nexus_workflow_ir::effect_summary(&nodes);
    let terminal = if header.terminal.is_empty() {
        manifest
            .states
            .iter()
            .filter(|state| state.terminal)
            .map(|state| state.id.clone())
            .collect()
    } else {
        vec![header.terminal.clone()]
    };

    Ok(WorkflowDocument {
        ir_version: IR_VERSION,
        id,
        version: u64::from(header.version),
        revision: 0,
        content_hash: None,
        state: WorkflowDocumentState::Candidate,
        meta: WorkflowMeta {
            description: header.description.clone(),
            kind: match header.kind {
                PresetKind::Creator => WorkflowMetaKind::Creator,
                PresetKind::System => WorkflowMetaKind::System,
            },
            author: header.author.clone(),
            license: header.license.clone(),
            homepage: header.homepage.clone(),
            run_intents: header
                .run_intents
                .iter()
                .copied()
                .map(run_intent_ir)
                .collect(),
            requires_capabilities: header.requires_capabilities.clone(),
            cli_args: header.cli_args.iter().map(cli_arg_ir).collect(),
            gates: header.gates.iter().map(gate_ir).collect(),
            world_binding: header.world_binding.map(|binding| WorkflowWorldBinding {
                mode: match binding.mode {
                    WorldBindingMode::Required => WorkflowWorldBindingMode::Required,
                    WorldBindingMode::Optional => WorkflowWorldBindingMode::Optional,
                },
            }),
        },
        entry: header.initial.clone(),
        initial_action: header.initial_action.as_ref().map(initial_action_ir),
        terminal,
        nodes,
        edges,
        nests: Vec::new(),
        assets,
        signals: manifest.signals.iter().map(signal_ir).collect(),
        roles: manifest.roles.iter().map(role_ir).collect(),
        effect_summary,
        provenance: WorkflowProvenance {
            source: WorkflowProvenanceSource::ImportPreset,
            agent_run_id: None,
            parent_candidate_id: None,
            resolved_pins: Vec::new(),
        },
    })
}

fn state_node(state: &StateDefinition) -> WorkflowNode {
    WorkflowNode {
        id: state.id.clone(),
        kind: if state.terminal {
            WorkflowNodeKind::Terminal
        } else {
            WorkflowNodeKind::OuterState
        },
        effect_class: state_effect(state),
        ui: state
            .description
            .as_ref()
            .map(|description| WorkflowNodeUi {
                description: Some(description.clone()),
                label: None,
            }),
        enter: state.enter.iter().map(enter_action_ir).collect(),
        exit_when: state.exit_when.as_ref().map(exit_when_ir),
        merge: state.merge.as_ref().map(merge_ir),
        converge: state.converge.as_ref().map(|converge| WorkflowConverge {
            strategy: converge_strategy_ir(converge.strategy),
        }),
        context_update: state.context_update.as_ref().map(context_update_ir),
        timeout_ms: state.timeout_ms.map(|ms| ms.min(i64::MAX as u64)),
        on_timeout: state.on_timeout.clone(),
        bind: serde_json::Map::new(),
        depends_on: Vec::new(),
        pattern: None,
        role: None,
        step_kind: None,
        template: None,
        tool_policy: None,
        output_binding: None,
        child_workflow_id: None,
        collapsed_view: None,
    }
}

/// The node a preset `output_binding` names: the segment before the first dot.
///
/// This is the loader's rule (`loader.rs` validates the same segment) and the adapter's only owner
/// rule: the binding names its owner directly, so no step-list scan or tail-segment reading may
/// re-derive it. A dotless binding names that node itself.
pub(crate) fn output_binding_owner(binding: &str) -> &str {
    binding.split('.').next().unwrap_or(binding)
}

/// An inner graph `output_binding` whose owner segment names no node: `(graph, binding, owner)`.
pub(crate) type UnknownOutputOwner<'a> = (&'a str, &'a str, &'a str);

/// Every inner graph whose `output_binding` owner names no node in that graph, in graph-name order.
///
/// One detection for the projection refusal and validation's diagnostics: the loader rejects the
/// same shape as an unknown node, so a projection must refuse instead of dropping the binding.
/// A dotted step id (`review.v2` with binding `review.v2.text`) is exactly this case — the owner
/// is the first segment, which is no step.
pub(crate) fn unknown_output_owners(manifest: &PresetManifest) -> Vec<UnknownOutputOwner<'_>> {
    let Some(graphs) = manifest.inner_graphs.as_ref() else {
        return Vec::new();
    };
    graphs
        .iter()
        .filter_map(|(graph, inner)| {
            let binding = inner.output_binding.as_deref()?;
            let owner = output_binding_owner(binding);
            (!inner.nodes.iter().any(|step| step.id == owner)).then_some((
                graph.as_str(),
                binding,
                owner,
            ))
        })
        .collect()
}

/// The first inner graph whose `output_binding` owner names no node — the carrier a refusal names.
pub(crate) fn unknown_output_owner(manifest: &PresetManifest) -> Option<UnknownOutputOwner<'_>> {
    unknown_output_owners(manifest).into_iter().next()
}

/// Every inner step whose id contains `/`, as `(graph, step id)` in graph-name order.
///
/// The node-id encoding is `<inner_graph>/<step>` with one segment per step, so a slashed step id
/// cannot be represented; the projection refuses the first one and validation names all of them.
/// Graph names may contain `/`: the last segment is the step.
pub(crate) fn slashed_inner_steps(manifest: &PresetManifest) -> Vec<(&str, &str)> {
    let Some(graphs) = manifest.inner_graphs.as_ref() else {
        return Vec::new();
    };
    graphs
        .iter()
        .flat_map(|(graph, inner)| {
            inner
                .nodes
                .iter()
                .filter(|step| step.id.contains('/'))
                .map(|step| (graph.as_str(), step.id.as_str()))
        })
        .collect()
}

/// The first inner step whose id contains `/` — the carrier the projection refusal names.
pub(crate) fn slashed_inner_step(manifest: &PresetManifest) -> Option<(&str, &str)> {
    slashed_inner_steps(manifest).into_iter().next()
}

fn inner_step_node(
    graph_name: &str,
    owns_output: bool,
    graph: &InnerGraph,
    step: &GraphNode,
) -> WorkflowNode {
    WorkflowNode {
        id: format!("{graph_name}/{}", step.id),
        kind: WorkflowNodeKind::InnerStep,
        effect_class: WorkflowNodeEffectClass::External,
        step_kind: Some(WorkflowNodeStepKind::AcpPrompt),
        template: step.template_file.clone(),
        tool_policy: step.tool_policy.clone(),
        role: step.agent.clone(),
        depends_on: step
            .depends_on
            .iter()
            .map(|dependency| format!("{graph_name}/{dependency}"))
            .collect(),
        output_binding: owns_output.then(|| graph.output_binding.clone()).flatten(),
        bind: serde_json::Map::new(),
        ui: None,
        enter: Vec::new(),
        exit_when: None,
        merge: None,
        converge: None,
        context_update: None,
        timeout_ms: None,
        on_timeout: None,
        pattern: None,
        child_workflow_id: None,
        collapsed_view: None,
    }
}

/// Whether this state's gate is an `llm_judge`: the preset's only judged exit kind.
const fn is_judged(state: &StateDefinition) -> bool {
    matches!(state.exit_when, Some(ExitWhen::LlmJudge { .. }))
}

fn push_next_edges(
    state: &StateDefinition,
    edges: &mut Vec<WorkflowEdge>,
    used: &mut HashSet<String>,
) -> Result<(), WorkflowIrError> {
    let from = state.id.as_str();
    let judged = is_judged(state);
    match &state.next {
        None => {}
        Some(NextTarget::Linear(to)) => {
            let kind = if judged {
                WorkflowEdgeKind::Default
            } else {
                WorkflowEdgeKind::Linear
            };
            push_edge(edges, used, from, to, kind, None, None);
        }
        Some(NextTarget::GoNogo(next)) => {
            push_edge(
                edges,
                used,
                from,
                &next.go,
                WorkflowEdgeKind::Go,
                None,
                None,
            );
            push_edge(
                edges,
                used,
                from,
                &next.nogo,
                WorkflowEdgeKind::Nogo,
                None,
                None,
            );
        }
        Some(NextTarget::Labeled(labeled)) => {
            for edge in labeled {
                push_edge(
                    edges,
                    used,
                    from,
                    &edge.target,
                    WorkflowEdgeKind::Linear,
                    Some(edge.label.clone()),
                    None,
                );
            }
        }
        Some(NextTarget::Conditional(conditional)) => {
            push_conditional_edges(
                edges,
                used,
                from,
                judged,
                &conditional.rules,
                &conditional.default,
            )?;
        }
        Some(NextTarget::Branches(branches)) => {
            push_conditional_edges(
                edges,
                used,
                from,
                judged,
                &branches.branches,
                &branches.default,
            )?;
        }
    }
    Ok(())
}

fn push_conditional_edges(
    edges: &mut Vec<WorkflowEdge>,
    used: &mut HashSet<String>,
    from: &str,
    judged: bool,
    rules: &[ConditionalRule],
    default: &str,
) -> Result<(), WorkflowIrError> {
    if judged && rules.is_empty() {
        // A judged node's unconditional advance projects to one `Default` edge, which the export
        // folds back to a bare `next`; an empty rule list would therefore be silently rewritten.
        // A non-judged node keeps its `{branches: [], default}` spelling through that same export.
        return Err(WorkflowIrError::EmptyConditionalRules {
            state: from.to_string(),
        });
    }
    for rule in rules {
        push_edge(
            edges,
            used,
            from,
            &rule.target,
            WorkflowEdgeKind::Conditional,
            None,
            Some(rule.when.clone()),
        );
    }
    push_edge(
        edges,
        used,
        from,
        default,
        WorkflowEdgeKind::Default,
        None,
        None,
    );
    Ok(())
}

fn push_edge(
    edges: &mut Vec<WorkflowEdge>,
    used: &mut HashSet<String>,
    from: &str,
    to: &str,
    kind: WorkflowEdgeKind,
    label: Option<String>,
    when: Option<String>,
) {
    let base = format!("{from}->{to}");
    let mut id = base.clone();
    let mut n = 1;
    while !used.insert(id.clone()) {
        n += 1;
        id = format!("{base}#{n}");
    }
    edges.push(WorkflowEdge {
        id,
        from: from.to_string(),
        to: to.to_string(),
        kind,
        label,
        when,
    });
}

fn enter_action_ir(action: &EnterAction) -> WorkflowEnterAction {
    match action {
        EnterAction::Capability { name, args } => WorkflowEnterAction {
            kind: WorkflowEnterActionKind::Capability,
            name: Some(name.clone()),
            args: args.clone(),
            tool_name: None,
        },
        EnterAction::InnerGraph { name } => WorkflowEnterAction {
            kind: WorkflowEnterActionKind::InnerGraph,
            name: Some(name.clone()),
            args: None,
            tool_name: None,
        },
        EnterAction::HostTool { tool_name, args } => WorkflowEnterAction {
            kind: WorkflowEnterActionKind::HostTool,
            name: None,
            args: args.clone(),
            tool_name: Some(tool_name.clone()),
        },
    }
}

fn exit_when_ir(exit: &ExitWhen) -> WorkflowExitWhen {
    match exit {
        ExitWhen::LlmJudge {
            template_file,
            judge_capability,
            min_interval,
        } => WorkflowExitWhen {
            kind: WorkflowExitWhenKind::LlmJudge,
            template: template_file.clone(),
            judge_capability: judge_capability.clone(),
            min_interval: min_interval.clone(),
            duration: None,
        },
        ExitWhen::Timer { duration } => WorkflowExitWhen {
            kind: WorkflowExitWhenKind::Timer,
            duration: duration.clone(),
            template: None,
            judge_capability: None,
            min_interval: None,
        },
        ExitWhen::Rule => bare_exit_when(WorkflowExitWhenKind::Rule),
        ExitWhen::GraphComplete => bare_exit_when(WorkflowExitWhenKind::GraphComplete),
        ExitWhen::Manual => bare_exit_when(WorkflowExitWhenKind::Manual),
    }
}

const fn bare_exit_when(kind: WorkflowExitWhenKind) -> WorkflowExitWhen {
    WorkflowExitWhen {
        kind,
        template: None,
        judge_capability: None,
        min_interval: None,
        duration: None,
    }
}

fn merge_ir(merge: &MergeKind) -> WorkflowMerge {
    match merge {
        MergeKind::All => WorkflowMerge {
            kind: WorkflowMergeKind::All,
            n: None,
            m: None,
        },
        MergeKind::Any => WorkflowMerge {
            kind: WorkflowMergeKind::Any,
            n: None,
            m: None,
        },
        MergeKind::Quorum { n, m } => WorkflowMerge {
            kind: WorkflowMergeKind::Quorum,
            n: Some(u64::try_from(*n).unwrap_or(u64::MAX)),
            m: Some(u64::try_from(*m).unwrap_or(u64::MAX)),
        },
    }
}

const fn converge_strategy_ir(strategy: ConvergeStrategy) -> WorkflowConvergeStrategy {
    match strategy {
        ConvergeStrategy::WaitForAll => WorkflowConvergeStrategy::WaitForAll,
        ConvergeStrategy::FirstCompleted => WorkflowConvergeStrategy::FirstCompleted,
        ConvergeStrategy::Any => WorkflowConvergeStrategy::Any,
    }
}

fn context_update_ir(hook: &ContextUpdateHook) -> WorkflowContextUpdate {
    let (op, body, capability, patch_json, remove_path) = match &hook.op {
        ContextUpdateOp::Append { body } => (
            WorkflowContextUpdateOp::Append,
            body.clone(),
            None,
            None,
            None,
        ),
        ContextUpdateOp::StructMerge { patch } => (
            WorkflowContextUpdateOp::StructMerge,
            None,
            None,
            patch.clone().map(Option::unwrap_or_default),
            None,
        ),
        ContextUpdateOp::Replace { body } => (
            WorkflowContextUpdateOp::Replace,
            body.clone(),
            None,
            None,
            None,
        ),
        ContextUpdateOp::StructRemove { path } => (
            WorkflowContextUpdateOp::StructRemove,
            None,
            None,
            None,
            path.clone(),
        ),
        ContextUpdateOp::LlmSummarize { capability } => (
            WorkflowContextUpdateOp::LlmSummarize,
            None,
            Some(capability.clone()),
            None,
            None,
        ),
    };
    WorkflowContextUpdate {
        op,
        template: Some(hook.template_file.clone()),
        body,
        capability,
        patch: patch_json,
        path: remove_path,
    }
}

fn cli_arg_ir(arg: &PresetCliArg) -> WorkflowCliArg {
    WorkflowCliArg {
        name: arg.name.clone(),
        type_: match arg.r#type {
            PresetCliArgType::String => WorkflowCliArgType::String,
            PresetCliArgType::Integer => WorkflowCliArgType::Integer,
            PresetCliArgType::Boolean => WorkflowCliArgType::Boolean,
        },
        required: Some(arg.required),
        default: arg.default.clone(),
        description: (!arg.description.is_empty()).then(|| arg.description.clone()),
    }
}

fn gate_ir(gate: &Gate) -> WorkflowGate {
    let mut ir = WorkflowGate {
        kind: WorkflowGateKind::WorkField,
        field: None,
        op: None,
        value: None,
        path: None,
        must_exist: None,
        preset: None,
        status: None,
        scope: None,
    };
    match gate {
        Gate::WorkField { field, op } => {
            ir.field = Some(field.clone());
            let (op, value) = match op {
                GateOp::Equals { value } => (WorkflowGateOp::Equals, Some(value.clone())),
                GateOp::NotEquals { value } => (WorkflowGateOp::NotEquals, Some(value.clone())),
                GateOp::Required => (WorkflowGateOp::Required, None),
                GateOp::In { value } => (
                    WorkflowGateOp::In,
                    Some(serde_json::Value::Array(value.clone())),
                ),
                GateOp::NotIn { value } => (
                    WorkflowGateOp::NotIn,
                    Some(serde_json::Value::Array(value.clone())),
                ),
            };
            ir.op = Some(op);
            ir.value = value;
        }
        Gate::Filesystem { path, must_exist } => {
            ir.kind = WorkflowGateKind::Filesystem;
            ir.path = Some(path.clone());
            ir.must_exist = Some(*must_exist);
        }
        Gate::PreviousPreset {
            preset,
            status,
            scope,
        } => {
            ir.kind = WorkflowGateKind::PreviousPreset;
            ir.preset = Some(preset.clone());
            ir.status = Some(match status {
                PreviousPresetStatus::Complete => WorkflowGateStatus::Complete,
                PreviousPresetStatus::AnySession => WorkflowGateStatus::AnySession,
            });
            ir.scope = Some(scope.clone());
        }
    }
    ir
}

const fn run_intent_ir(intent: RunIntent) -> WorkflowMetaRunIntentsItem {
    match intent {
        RunIntent::WorkInit => WorkflowMetaRunIntentsItem::WorkInit,
        RunIntent::WorkContinue => WorkflowMetaRunIntentsItem::WorkContinue,
        RunIntent::KnowledgeIngest => WorkflowMetaRunIntentsItem::KnowledgeIngest,
        RunIntent::WorkMaintenance => WorkflowMetaRunIntentsItem::WorkMaintenance,
        RunIntent::SystemMaintenance => WorkflowMetaRunIntentsItem::SystemMaintenance,
    }
}

fn initial_action_ir(action: &InitialAction) -> WorkflowInitialAction {
    match action {
        InitialAction::SeedDirect => WorkflowInitialAction {
            kind: WorkflowInitialActionKind::SeedDirect,
            capability: None,
            template: None,
            payload_kind: None,
        },
        InitialAction::SeedExpansion {
            capability,
            template_file,
            payload_kind,
        } => WorkflowInitialAction {
            kind: WorkflowInitialActionKind::SeedExpansion,
            capability: Some(capability.clone()),
            template: template_file.clone(),
            payload_kind: payload_kind.clone(),
        },
    }
}

fn signal_ir(signal: &SignalBinding) -> WorkflowSignalBinding {
    WorkflowSignalBinding {
        name: signal.name.clone(),
        action: match &signal.on_receive.action {
            SignalActionKind::Pause => WorkflowSignalBindingAction::Pause,
            SignalActionKind::ForceTransition => WorkflowSignalBindingAction::ForceTransition,
        },
        target: signal.on_receive.target.clone(),
    }
}

fn role_ir(role: &crate::manifest::PresetRoleDefinition) -> WorkflowRoleDefinition {
    WorkflowRoleDefinition {
        id: role.id.clone(),
        description: role.description.clone(),
        system_prompt_file: role.system_prompt_file.clone(),
        recommended_skills: role.recommended_skills.clone(),
    }
}

fn state_effect(state: &StateDefinition) -> WorkflowNodeEffectClass {
    let mut rank = if state.enter.is_empty() { 0 } else { 3 };
    if matches!(state.exit_when, Some(ExitWhen::LlmJudge { .. })) {
        rank = rank.max(3);
    }
    if state.context_update.is_some() {
        rank = rank.max(2);
    }
    effect_from_rank(rank)
}

const fn effect_from_rank(rank: u8) -> WorkflowNodeEffectClass {
    match rank {
        3 => WorkflowNodeEffectClass::External,
        2 => WorkflowNodeEffectClass::WriteCandidate,
        1 => WorkflowNodeEffectClass::Read,
        _ => WorkflowNodeEffectClass::Pure,
    }
}

// Export: WorkflowDocument -> PresetManifest
// ---------------------------------------------------------------------------

/// Project a Workflow IR document back to a preset manifest.
///
/// # Errors
///
/// Returns [`WorkflowIrError::NotProjectable`] for IR-only nodes (pattern slots, nest groups,
/// joins), mixed edge families, or a required payload the document leaves empty.
pub fn export_preset(document: &WorkflowDocument) -> Result<PresetManifest, WorkflowIrError> {
    if document.ir_version > IR_VERSION {
        return Err(refuse(format!(
            "document ir_version {} is newer than supported {IR_VERSION}",
            document.ir_version
        )));
    }
    let (states, inner_graphs) = project_nodes(document)?;
    Ok(PresetManifest {
        preset: preset_header(document)?,
        states,
        inner_graphs,
        signals: document
            .signals
            .iter()
            .map(|signal| SignalBinding {
                name: signal.name.clone(),
                on_receive: SignalAction {
                    action: match &signal.action {
                        WorkflowSignalBindingAction::Pause => SignalActionKind::Pause,
                        WorkflowSignalBindingAction::ForceTransition => {
                            SignalActionKind::ForceTransition
                        }
                    },
                    target: signal.target.clone(),
                },
            })
            .collect(),
        roles: document
            .roles
            .iter()
            .map(|role| crate::manifest::PresetRoleDefinition {
                id: role.id.clone(),
                description: role.description.clone(),
                system_prompt_file: role.system_prompt_file.clone(),
                recommended_skills: role.recommended_skills.clone(),
            })
            .collect(),
    })
}

/// Inner step ids per graph, as the last-`/` rule derives them.
fn inner_graph_steps(document: &WorkflowDocument) -> BTreeMap<&str, BTreeSet<&str>> {
    let mut steps: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for node in &document.nodes {
        if node.kind == WorkflowNodeKind::InnerStep {
            if let Some((graph, step_id)) = node.id.rsplit_once('/') {
                steps.entry(graph).or_default().insert(step_id);
            }
        }
    }
    steps
}

/// Dependencies a preset cannot express, as `(node id, dependency, graph)`.
///
/// The loader resolves `depends_on` only against the graph's own step ids, so a dependency is
/// expressible exactly when its id, once stripped of its own graph prefix, names a step of that
/// graph. Everything else — another graph's step, or an id no step carries — is refused.
fn unexpressible_dependencies(document: &WorkflowDocument) -> Vec<(&str, &str, &str)> {
    let steps = inner_graph_steps(document);
    let mut refused = Vec::new();
    for node in &document.nodes {
        if node.kind != WorkflowNodeKind::InnerStep {
            continue;
        }
        let Some((graph, _)) = node.id.rsplit_once('/') else {
            continue;
        };
        for dependency in &node.depends_on {
            let local = dependency
                .strip_prefix(&format!("{graph}/"))
                .unwrap_or(dependency);
            if !steps
                .get(graph)
                .is_some_and(|step_ids| step_ids.contains(local))
            {
                refused.push((node.id.as_str(), dependency.as_str(), graph));
            }
        }
    }
    refused
}

fn project_nodes(document: &WorkflowDocument) -> Result<ProjectedNodes, WorkflowIrError> {
    let mut states = Vec::new();
    let mut steps: BTreeMap<String, Vec<GraphNode>> = BTreeMap::new();
    let mut bindings: BTreeMap<String, String> = BTreeMap::new();
    let outgoing = outgoing_edges(document);
    // Every step id per graph, so a binding can be checked against its graph before any node is
    // projected: import refuses an owner that names no step, and export must not hand out a
    // manifest that import would then refuse. The step is the last `/` segment and the graph is
    // everything before it — a graph name may itself contain `/`, and a nested key no state enters
    // is simply an unused graph.
    let graph_steps = inner_graph_steps(document);
    if let Some((node_id, dependency, graph)) =
        unexpressible_dependencies(document).into_iter().next()
    {
        return Err(refuse(format!(
            "inner step `{node_id}` depends on `{dependency}`, which is not a step of inner graph `{graph}`; a preset cannot express that dependency"
        )));
    }

    for node in &document.nodes {
        match &node.kind {
            WorkflowNodeKind::OuterState | WorkflowNodeKind::Terminal => {
                let next_edges = outgoing
                    .get(node.id.as_str())
                    .map_or(&[][..], |edges| edges.as_slice());
                states.push(state_from_node(node, next_edges)?);
            }
            WorkflowNodeKind::InnerStep => {
                let (graph, step_id) = node.id.rsplit_once('/').ok_or_else(|| {
                    refuse(format!(
                        "inner step `{}` is not `<inner_graph>/<node_id>`",
                        node.id
                    ))
                })?;
                let graph_key = graph.to_string();
                if let Some(binding) = &node.output_binding {
                    let owner = output_binding_owner(binding);
                    if !graph_steps
                        .get(graph)
                        .is_some_and(|steps| steps.contains(owner))
                    {
                        return Err(refuse(format!(
                            "inner graph `{graph_key}` output binding `{binding}` names owner `{owner}`, which is no node in the graph"
                        )));
                    }
                    // A graph exports exactly one binding: `output_binding_owner` binds it to a
                    // single step, so a second claim — same owner or another — has no projection.
                    if let Some(claimant) = bindings.get(&graph_key) {
                        return Err(refuse(format!(
                            "inner graph `{graph_key}` carries two output bindings (`{claimant}` and `{binding}`); a graph exports exactly one"
                        )));
                    }
                    bindings.insert(graph_key.clone(), binding.clone());
                }
                steps.entry(graph_key).or_default().push(GraphNode {
                    id: step_id.to_string(),
                    kind: GraphNodeKind::AcpPrompt,
                    depends_on: node
                        .depends_on
                        .iter()
                        .map(|dependency| {
                            // Validated by `unexpressible_dependencies`: the remainder names a step
                            // of this graph, which is the only form the loader resolves.
                            dependency
                                .strip_prefix(&format!("{graph}/"))
                                .unwrap_or(dependency)
                                .to_string()
                        })
                        .collect(),
                    template_file: node.template.clone(),
                    tool_policy: node.tool_policy.clone(),
                    agent: node.role.clone(),
                });
            }
            other => {
                return Err(refuse(format!(
                    "node `{}` kind `{:?}` has no preset projection",
                    node.id, other
                )))
            }
        }
    }

    refuse_dangling_graph_references(document, &steps)?;

    if steps.is_empty() {
        return Ok((states, None));
    }
    let mut graphs = BTreeMap::new();
    for (name, nodes) in steps {
        let output_binding = bindings.remove(&name);
        graphs.insert(
            name,
            InnerGraph {
                nodes,
                output_binding,
            },
        );
    }
    Ok((states, Some(graphs)))
}

/// Refuse a state `enter` reference that no inner step defines.
///
/// The loader rejects a dangling reference, and export must not overwrite a preset with a file the
/// loader refuses. `defined` is the graph-name set the inner steps export under.
fn refuse_dangling_graph_references(
    document: &WorkflowDocument,
    defined: &BTreeMap<String, Vec<GraphNode>>,
) -> Result<(), WorkflowIrError> {
    for node in &document.nodes {
        if !matches!(
            node.kind,
            WorkflowNodeKind::OuterState | WorkflowNodeKind::Terminal
        ) {
            continue;
        }
        for enter in &node.enter {
            if enter.kind != WorkflowEnterActionKind::InnerGraph {
                continue;
            }
            if let Some(name) = enter.name.as_deref() {
                if !defined.contains_key(name) {
                    return Err(refuse(format!(
                        "state `{}` enters inner graph `{name}`, but no inner step belongs to it; the document's graph structure cannot be exported",
                        node.id
                    )));
                }
            }
        }
    }
    Ok(())
}

fn preset_header(document: &WorkflowDocument) -> Result<PresetHeader, WorkflowIrError> {
    let version = u32::try_from(document.version).map_err(|_| {
        refuse(format!(
            "document version {} exceeds the preset version range",
            document.version
        ))
    })?;
    let terminal = match document.terminal.as_slice() {
        [terminal] => terminal.clone(),
        terminals => {
            return Err(refuse(format!(
                "document declares {} terminal nodes; a preset carries exactly one",
                terminals.len()
            )))
        }
    };
    Ok(PresetHeader {
        id: document.id.as_str().to_string(),
        version,
        kind: match &document.meta.kind {
            WorkflowMetaKind::Creator => PresetKind::Creator,
            WorkflowMetaKind::System => PresetKind::System,
        },
        description: document.meta.description.clone(),
        requires_capabilities: document.meta.requires_capabilities.clone(),
        run_intents: document
            .meta
            .run_intents
            .iter()
            .copied()
            .map(run_intent_preset)
            .collect(),
        initial: document.entry.clone(),
        terminal,
        world_binding: document
            .meta
            .world_binding
            .as_ref()
            .map(|binding| WorldBinding {
                mode: match binding.mode {
                    WorkflowWorldBindingMode::Required => WorldBindingMode::Required,
                    WorkflowWorldBindingMode::Optional => WorldBindingMode::Optional,
                },
            }),
        author: document.meta.author.clone(),
        homepage: document.meta.homepage.clone(),
        license: document.meta.license.clone(),
        initial_action: document
            .initial_action
            .as_ref()
            .map(initial_action_preset)
            .transpose()?,
        gates: document
            .meta
            .gates
            .iter()
            .map(gate_preset)
            .collect::<Result<Vec<_>, _>>()?,
        cli_args: document.meta.cli_args.iter().map(cli_arg_preset).collect(),
    })
}

/// Projected preset state list plus the optional inner-graph map.
type ProjectedNodes = (Vec<StateDefinition>, Option<BTreeMap<String, InnerGraph>>);

fn outgoing_edges(document: &WorkflowDocument) -> HashMap<&str, Vec<&WorkflowEdge>> {
    let mut outgoing: HashMap<&str, Vec<&WorkflowEdge>> = HashMap::new();
    for edge in &document.edges {
        outgoing.entry(edge.from.as_str()).or_default().push(edge);
    }
    outgoing
}

fn state_from_node(
    node: &WorkflowNode,
    edges: &[&WorkflowEdge],
) -> Result<StateDefinition, WorkflowIrError> {
    Ok(StateDefinition {
        id: node.id.clone(),
        description: node.ui.as_ref().and_then(|ui| ui.description.clone()),
        enter: node
            .enter
            .iter()
            .map(enter_action_preset)
            .collect::<Result<Vec<_>, _>>()?,
        exit_when: node.exit_when.as_ref().map(exit_when_preset),
        next: next_from_edges(&node.id, edges, is_judged_node(node))?,
        terminal: matches!(&node.kind, WorkflowNodeKind::Terminal),
        context_update: node
            .context_update
            .as_ref()
            .map(context_update_preset)
            .transpose()?,
        merge: node.merge.as_ref().map(merge_preset).transpose()?,
        converge: node.converge.as_ref().map(|converge| ConvergeConfig {
            strategy: converge_strategy_preset(converge.strategy),
        }),
        timeout_ms: node.timeout_ms,
        on_timeout: node.on_timeout.clone(),
    })
}

/// Whether this document node's gate is an `llm_judge` — the inverse side of [`is_judged`].
fn is_judged_node(node: &WorkflowNode) -> bool {
    node.exit_when
        .as_ref()
        .is_some_and(|exit| exit.kind == WorkflowExitWhenKind::LlmJudge)
}

fn next_from_edges(
    node_id: &str,
    edges: &[&WorkflowEdge],
    judged: bool,
) -> Result<Option<NextTarget>, WorkflowIrError> {
    let Some(first) = edges.first() else {
        return Ok(None);
    };

    if judged && edges.len() == 1 && matches!(first.kind, WorkflowEdgeKind::Default) {
        return Ok(Some(NextTarget::Linear(first.to.clone())));
    }

    if let Some(go) = edges
        .iter()
        .find(|edge| matches!(&edge.kind, WorkflowEdgeKind::Go))
    {
        let nogo = edges
            .iter()
            .find(|edge| matches!(&edge.kind, WorkflowEdgeKind::Nogo))
            .ok_or_else(|| refuse(format!("`{node_id}` has a `go` edge without a `nogo` edge")))?;
        if edges.len() != 2 {
            return Err(refuse(format!(
                "`{node_id}` has edges beyond its go/nogo pair"
            )));
        }
        return Ok(Some(NextTarget::GoNogo(GoNogoNext {
            go: go.to.clone(),
            nogo: nogo.to.clone(),
        })));
    }

    let is_conditional = |kind: &WorkflowEdgeKind| {
        matches!(
            kind,
            WorkflowEdgeKind::Conditional | WorkflowEdgeKind::Default
        )
    };
    if is_conditional(&first.kind) {
        let mut rules = Vec::new();
        let mut default = None;
        for edge in edges {
            match &edge.kind {
                WorkflowEdgeKind::Conditional => rules.push(ConditionalRule {
                    when: edge.when.clone().ok_or_else(|| {
                        refuse(format!("conditional edge `{}` carries no `when`", edge.id))
                    })?,
                    target: edge.to.clone(),
                }),
                WorkflowEdgeKind::Default => {
                    if default.is_some() {
                        return Err(refuse(format!(
                            "`{node_id}` has more than one default edge"
                        )));
                    }
                    default = Some(edge.to.clone());
                }
                _ => {
                    return Err(refuse(format!(
                        "`{node_id}` mixes conditional and non-conditional edges"
                    )))
                }
            }
        }
        let default = default.ok_or_else(|| {
            refuse(format!(
                "`{node_id}` has conditional edges without a default edge"
            ))
        })?;
        return Ok(Some(NextTarget::Branches(ConditionalBranches {
            branches: rules,
            default,
        })));
    }

    if edges.iter().any(|edge| edge.label.is_some()) {
        let mut labeled = Vec::new();
        for edge in edges {
            if !matches!(&edge.kind, WorkflowEdgeKind::Linear) {
                return Err(refuse(format!(
                    "`{node_id}` mixes a `{}` edge into its labeled edges",
                    edge.kind
                )));
            }
            let label = edge
                .label
                .clone()
                .ok_or_else(|| refuse(format!("edge `{}` carries no label", edge.id)))?;
            labeled.push(LabeledNext {
                label,
                target: edge.to.clone(),
            });
        }
        return Ok(Some(NextTarget::Labeled(labeled)));
    }

    if edges.len() == 1 && matches!(first.kind, WorkflowEdgeKind::Linear) {
        return Ok(Some(NextTarget::Linear(first.to.clone())));
    }

    Err(refuse(format!(
        "`{node_id}` has edges with no preset `next` projection"
    )))
}

fn enter_action_preset(action: &WorkflowEnterAction) -> Result<EnterAction, WorkflowIrError> {
    match &action.kind {
        WorkflowEnterActionKind::Capability => Ok(EnterAction::Capability {
            name: action
                .name
                .clone()
                .ok_or_else(|| refuse("capability enter action carries no name"))?,
            args: action.args.clone(),
        }),
        WorkflowEnterActionKind::InnerGraph => Ok(EnterAction::InnerGraph {
            name: action
                .name
                .clone()
                .ok_or_else(|| refuse("inner-graph enter action carries no name"))?,
        }),
        WorkflowEnterActionKind::HostTool => Ok(EnterAction::HostTool {
            tool_name: action
                .tool_name
                .clone()
                .ok_or_else(|| refuse("host-tool enter action carries no tool name"))?,
            args: action.args.clone(),
        }),
    }
}

fn exit_when_preset(exit: &WorkflowExitWhen) -> ExitWhen {
    match &exit.kind {
        WorkflowExitWhenKind::LlmJudge => ExitWhen::LlmJudge {
            template_file: exit.template.clone(),
            judge_capability: exit.judge_capability.clone(),
            min_interval: exit.min_interval.clone(),
        },
        WorkflowExitWhenKind::Timer => ExitWhen::Timer {
            duration: exit.duration.clone(),
        },
        WorkflowExitWhenKind::Rule => ExitWhen::Rule,
        WorkflowExitWhenKind::GraphComplete => ExitWhen::GraphComplete,
        WorkflowExitWhenKind::Manual => ExitWhen::Manual,
    }
}

fn merge_preset(merge: &WorkflowMerge) -> Result<MergeKind, WorkflowIrError> {
    Ok(match &merge.kind {
        WorkflowMergeKind::All => MergeKind::All,
        WorkflowMergeKind::Any => MergeKind::Any,
        WorkflowMergeKind::Quorum => MergeKind::Quorum {
            n: usize::try_from(
                merge
                    .n
                    .ok_or_else(|| refuse("quorum merge carries no `n`"))?,
            )
            .map_err(|_| refuse("quorum merge `n` is out of range"))?,
            m: usize::try_from(
                merge
                    .m
                    .ok_or_else(|| refuse("quorum merge carries no `m`"))?,
            )
            .map_err(|_| refuse("quorum merge `m` is out of range"))?,
        },
    })
}

const fn converge_strategy_preset(strategy: WorkflowConvergeStrategy) -> ConvergeStrategy {
    match strategy {
        WorkflowConvergeStrategy::WaitForAll => ConvergeStrategy::WaitForAll,
        WorkflowConvergeStrategy::FirstCompleted => ConvergeStrategy::FirstCompleted,
        WorkflowConvergeStrategy::Any => ConvergeStrategy::Any,
    }
}

fn context_update_preset(
    update: &WorkflowContextUpdate,
) -> Result<ContextUpdateHook, WorkflowIrError> {
    let op = match &update.op {
        WorkflowContextUpdateOp::Append => ContextUpdateOp::Append {
            body: update.body.clone(),
        },
        WorkflowContextUpdateOp::Replace => ContextUpdateOp::Replace {
            body: update.body.clone(),
        },
        WorkflowContextUpdateOp::StructMerge => ContextUpdateOp::StructMerge {
            patch: update
                .patch
                .clone()
                .map(|patch| (!patch.is_null()).then_some(patch)),
        },
        WorkflowContextUpdateOp::StructRemove => ContextUpdateOp::StructRemove {
            path: update.path.clone(),
        },
        WorkflowContextUpdateOp::LlmSummarize => ContextUpdateOp::LlmSummarize {
            capability: update
                .capability
                .clone()
                .ok_or_else(|| refuse("llm-summarize context update carries no capability"))?,
        },
    };
    Ok(ContextUpdateHook {
        op,
        template_file: update
            .template
            .clone()
            .ok_or_else(|| refuse("context update carries no template"))?,
    })
}

fn cli_arg_preset(arg: &WorkflowCliArg) -> PresetCliArg {
    PresetCliArg {
        name: arg.name.clone(),
        r#type: match &arg.type_ {
            WorkflowCliArgType::String => PresetCliArgType::String,
            WorkflowCliArgType::Integer => PresetCliArgType::Integer,
            WorkflowCliArgType::Boolean => PresetCliArgType::Boolean,
        },
        required: arg.required.unwrap_or(false),
        default: arg.default.clone(),
        description: arg.description.clone().unwrap_or_default(),
    }
}

fn gate_preset(gate: &WorkflowGate) -> Result<Gate, WorkflowIrError> {
    match &gate.kind {
        WorkflowGateKind::WorkField => {
            let field = gate
                .field
                .clone()
                .ok_or_else(|| refuse("work-field gate carries no field"))?;
            let op = match gate
                .op
                .as_ref()
                .ok_or_else(|| refuse("work-field gate carries no operator"))?
            {
                WorkflowGateOp::Equals => GateOp::Equals {
                    value: gate
                        .value
                        .clone()
                        .ok_or_else(|| refuse("equals gate carries no value"))?,
                },
                WorkflowGateOp::NotEquals => GateOp::NotEquals {
                    value: gate
                        .value
                        .clone()
                        .ok_or_else(|| refuse("not-equals gate carries no value"))?,
                },
                WorkflowGateOp::Required => GateOp::Required,
                WorkflowGateOp::In => GateOp::In {
                    value: gate_values(gate.value.as_ref())?,
                },
                WorkflowGateOp::NotIn => GateOp::NotIn {
                    value: gate_values(gate.value.as_ref())?,
                },
            };
            Ok(Gate::WorkField { field, op })
        }
        WorkflowGateKind::Filesystem => Ok(Gate::Filesystem {
            path: gate
                .path
                .clone()
                .ok_or_else(|| refuse("filesystem gate carries no path"))?,
            must_exist: gate.must_exist.unwrap_or(false),
        }),
        WorkflowGateKind::PreviousPreset => Ok(Gate::PreviousPreset {
            preset: gate
                .preset
                .clone()
                .ok_or_else(|| refuse("previous-preset gate carries no preset id"))?,
            status: match gate
                .status
                .as_ref()
                .ok_or_else(|| refuse("previous-preset gate carries no status"))?
            {
                WorkflowGateStatus::Complete => PreviousPresetStatus::Complete,
                WorkflowGateStatus::AnySession => PreviousPresetStatus::AnySession,
            },
            scope: gate.scope.clone().unwrap_or_else(|| "work".to_string()),
        }),
    }
}

fn gate_values(
    value: Option<&serde_json::Value>,
) -> Result<Vec<serde_json::Value>, WorkflowIrError> {
    match value {
        Some(serde_json::Value::Array(values)) => Ok(values.clone()),
        _ => Err(refuse("`in`/`not_in` gate value must be an array")),
    }
}

const fn run_intent_preset(intent: WorkflowMetaRunIntentsItem) -> RunIntent {
    match intent {
        WorkflowMetaRunIntentsItem::WorkInit => RunIntent::WorkInit,
        WorkflowMetaRunIntentsItem::WorkContinue => RunIntent::WorkContinue,
        WorkflowMetaRunIntentsItem::KnowledgeIngest => RunIntent::KnowledgeIngest,
        WorkflowMetaRunIntentsItem::WorkMaintenance => RunIntent::WorkMaintenance,
        WorkflowMetaRunIntentsItem::SystemMaintenance => RunIntent::SystemMaintenance,
    }
}

fn initial_action_preset(action: &WorkflowInitialAction) -> Result<InitialAction, WorkflowIrError> {
    match &action.kind {
        WorkflowInitialActionKind::SeedDirect => Ok(InitialAction::SeedDirect),
        WorkflowInitialActionKind::SeedExpansion => Ok(InitialAction::SeedExpansion {
            capability: action
                .capability
                .clone()
                .ok_or_else(|| refuse("seed-expansion initial action carries no capability"))?,
            template_file: action.template.clone(),
            payload_kind: action.payload_kind.clone(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A manifest whose judged `review` state carries `next` spelled by the caller.
    fn judged_manifest(next: &str) -> String {
        format!(
            r#"
preset:
  id: judged-branches
  version: 1
  kind: creator
  description: "judged state whose conditional next carries no rules"
  requires_capabilities: []
  initial: review
  terminal: done
states:
  - id: review
    exit_when:
      kind: llm_judge
      template_file: prompts/judge.md
      judge_capability: judge.llm
      min_interval: "PT1H"
    next: {next}
  - id: done
    terminal: true
"#
        )
    }

    /// A non-judged state whose `next` is the zero-rule branch form.
    const MANIFEST_WITH_EMPTY_BRANCHES: &str = r#"
preset:
  id: plain-branches
  version: 1
  kind: creator
  description: "non-judged state whose branch list is empty"
  requires_capabilities: []
  initial: review
  terminal: done
states:
  - id: review
    next: {branches: [], default: done}
  - id: done
    terminal: true
"#;

    const MANIFEST: &str = r#"
preset:
  id: round-trip
  version: 2
  kind: creator
  description: "round trip"
  requires_capabilities: [acp.prompt]
  run_intents: [work_continue]
  initial: start
  terminal: done
  author: "42ch"
  initial_action:
    kind: seed_expansion
    capability: creator.expand
    template_file: prompts/seed.md
  cli_args:
    - name: chapter
      type: integer
      required: true
      default: 3
      description: "Chapter number"
  gates:
    - kind: work_field
      field: status
      op: in
      value: [draft, review]
    - kind: filesystem
      path: "Works/{{work_ref}}/"
      must_exist: true
    - kind: previous_preset
      preset: research
      status: complete
signals:
  - name: advance
    on_receive:
      action: force_transition
      target: work
states:
  - id: start
    description: "decide"
    exit_when:
      kind: llm_judge
      template_file: prompts/judge.md
      judge_capability: judge.llm
      min_interval: "PT1H"
    next:
      go: work
      nogo: done
  - id: work
    enter:
      - kind: inner_graph
        name: work_graph
    exit_when:
      kind: graph_complete
    next:
      branches:
        - when: "_context.score > 1"
          target: done
      default: done
    context_update:
      template_file: prompts/hook.md
      op:
        kind: append
        body: "note"
  - id: done
    terminal: true
inner_graphs:
  work_graph:
    nodes:
      - id: draft
        kind: acp_prompt
        template_file: prompts/draft.md
        tool_policy: auto_grant_read_only
      - id: review
        kind: acp_prompt
        depends_on: [draft]
        template_file: prompts/review.md
        agent: writer
    output_binding: review.text
roles:
  - id: writer
    description: "Writer"
    system_prompt_file: prompts/writer.md
    recommended_skills: [novel]
"#;

    fn bundle() -> (tempfile::TempDir, PresetManifest) {
        let dir = tempfile::tempdir().expect("tempdir");
        for name in [
            "seed.md",
            "judge.md",
            "hook.md",
            "draft.md",
            "review.md",
            "writer.md",
        ] {
            std::fs::create_dir_all(dir.path().join("prompts")).expect("mkdir");
            std::fs::write(dir.path().join("prompts").join(name), name).expect("write asset");
        }
        let manifest = serde_yaml::from_str(MANIFEST).expect("manifest parses");
        (dir, manifest)
    }

    #[test]
    fn import_hashes_assets_and_round_trips_the_manifest() {
        let (dir, manifest) = bundle();
        let imported = import_preset_bundle(
            &manifest,
            &FsAssetSource::new(dir.path()).expect("bundle root"),
        )
        .expect("import");

        let document = &imported.document;
        assert_eq!(document.id.as_str(), "round-trip");
        assert_eq!(document.terminal, vec!["done".to_string()]);
        assert_eq!(document.assets.len(), 6);
        let judge = document
            .assets
            .iter()
            .find(|asset| asset.path == "prompts/judge.md")
            .expect("judge asset present");
        assert_eq!(
            judge.content_hash.as_deref(),
            Some(blake3::hash(b"judge.md").to_hex().to_string().as_str())
        );
        assert_eq!(imported.assets["prompts/judge.md"], b"judge.md");
        assert_eq!(
            document.effect_summary,
            nexus_workflow_ir::WorkflowDocumentEffectSummary::External
        );

        let exported = export_preset(document).expect("export");
        assert_eq!(
            serde_json::to_value(&exported).expect("exported json"),
            serde_json::to_value(&manifest).expect("manifest json")
        );
    }

    #[test]
    fn import_clamps_an_oversized_timeout_to_i64_max() {
        let state = StateDefinition {
            id: "join".to_string(),
            description: None,
            enter: Vec::new(),
            exit_when: None,
            next: None,
            terminal: false,
            context_update: None,
            merge: None,
            converge: None,
            timeout_ms: Some(u64::MAX),
            on_timeout: None,
        };

        assert_eq!(state_node(&state).timeout_ms, Some(i64::MAX as u64));
    }

    #[test]
    fn judged_next_projects_to_the_default_branch_and_round_trips() {
        let manifest: PresetManifest = serde_yaml::from_str(
            r#"
preset:
  id: judged-linear
  version: 1
  kind: creator
  description: "judged state advancing unconditionally"
  requires_capabilities: []
  initial: review
  terminal: done
states:
  - id: review
    exit_when:
      kind: llm_judge
      template_file: prompts/judge.md
      judge_capability: judge.llm
      min_interval: "PT1H"
    next: done
  - id: done
    terminal: true
"#,
        )
        .expect("manifest parses");
        let document = import_preset_manifest(&manifest).expect("import");

        assert_eq!(document.edges.len(), 1);
        assert_eq!(
            document.edges[0].kind,
            WorkflowEdgeKind::Default,
            "a judged state's unconditional advance is the branch a run takes: its default edge"
        );

        let exported = export_preset(&document).expect("export");
        assert_eq!(
            serde_json::to_value(&exported).expect("exported json"),
            serde_json::to_value(&manifest).expect("manifest json")
        );
    }

    #[test]
    fn conditional_next_with_no_rules_is_refused_on_a_judged_state() {
        for next in [
            "{ branches: [], default: done }",
            "{ kind: conditional, rules: [], default: done }",
        ] {
            let manifest: PresetManifest =
                serde_yaml::from_str(&judged_manifest(next)).expect("manifest parses");

            let error = import_preset_manifest(&manifest).expect_err("empty rules are refused");
            assert!(
                matches!(
                    &error,
                    WorkflowIrError::EmptyConditionalRules { state } if state == "review"
                ),
                "{next}: {error:?}"
            );
            assert!(error.to_string().contains("no rules"), "{next}: {error}");
        }
    }

    /// A non-judged state has no judge to route, so its zero-rule branch is a plain default edge —
    /// and `next_from_edges` exports that shape back as `branches: []`, not as a bare `next`.
    #[test]
    fn empty_branches_on_a_non_judged_state_round_trip() {
        let manifest: PresetManifest =
            serde_yaml::from_str(MANIFEST_WITH_EMPTY_BRANCHES).expect("manifest parses");
        let document = import_preset_manifest(&manifest).expect("import");
        assert_eq!(document.edges.len(), 1);
        assert_eq!(document.edges[0].kind, WorkflowEdgeKind::Default);
        assert_eq!(document.edges[0].to, "done");

        let exported = export_preset(&document).expect("export");
        assert_eq!(
            serde_json::to_value(&exported).expect("exported json"),
            serde_json::to_value(&manifest).expect("manifest json"),
            "a non-judged zero-rule branch keeps its `branches: []` spelling"
        );

        let reimported =
            import_preset_manifest(&exported).expect("the exported manifest imports again");
        assert_eq!(reimported.edges.len(), 1);
        assert_eq!(reimported.edges[0].kind, WorkflowEdgeKind::Default);
        assert_eq!(reimported.edges[0].to, "done");
    }

    #[test]
    fn export_keeps_the_single_inner_graph_output_binding() {
        let (dir, manifest) = bundle();
        let imported = import_preset_bundle(
            &manifest,
            &FsAssetSource::new(dir.path()).expect("bundle root"),
        )
        .expect("import");
        let binding = |node_id: &str| {
            imported
                .document
                .nodes
                .iter()
                .find(|node| node.id == node_id)
                .and_then(|node| node.output_binding.clone())
        };
        assert_eq!(
            binding("work_graph/review"),
            Some("review.text".to_string())
        );
        assert_eq!(binding("work_graph/draft"), None);

        let exported = export_preset(&imported.document).expect("one owner exports");
        assert_eq!(
            exported
                .inner_graphs
                .as_ref()
                .and_then(|graphs| graphs.get("work_graph"))
                .and_then(|graph| graph.output_binding.as_deref()),
            Some("review.text")
        );
    }

    #[test]
    fn export_refuses_a_second_output_binding_for_one_inner_graph() {
        let (dir, manifest) = bundle();
        let mut imported = import_preset_bundle(
            &manifest,
            &FsAssetSource::new(dir.path()).expect("bundle root"),
        )
        .expect("import");
        imported
            .document
            .nodes
            .iter_mut()
            .find(|node| node.id == "work_graph/draft")
            .expect("draft node")
            .output_binding = Some("draft.text".to_string());

        let error = export_preset(&imported.document)
            .expect_err("a graph carries exactly one output binding");
        assert!(matches!(error, WorkflowIrError::NotProjectable { .. }));
        let message = error.to_string();
        for expected in ["work_graph", "review.text", "draft.text"] {
            assert!(message.contains(expected), "{message}");
        }
    }

    #[test]
    fn import_binds_output_to_the_owner_segment() {
        let manifest: PresetManifest = serde_yaml::from_str(&MANIFEST.replace(
            "output_binding: review.text",
            "output_binding: review.output.text",
        ))
        .expect("manifest parses");
        let document = import_preset_manifest(&manifest).expect("import");
        let binding = |node_id: &str| {
            document
                .nodes
                .iter()
                .find(|node| node.id == node_id)
                .and_then(|node| node.output_binding.clone())
        };
        assert_eq!(
            binding("work_graph/review"),
            Some("review.output.text".to_string()),
            "the segment before the first dot names the owner"
        );
        assert_eq!(binding("work_graph/draft"), None);
    }

    #[test]
    fn import_and_export_keep_a_dotless_output_binding() {
        let manifest: PresetManifest = serde_yaml::from_str(
            &MANIFEST.replace("output_binding: review.text", "output_binding: review"),
        )
        .expect("manifest parses");
        let document = import_preset_manifest(&manifest).expect("import");
        let binding = |node_id: &str| {
            document
                .nodes
                .iter()
                .find(|node| node.id == node_id)
                .and_then(|node| node.output_binding.clone())
        };
        assert_eq!(
            binding("work_graph/review"),
            Some("review".to_string()),
            "a dotless binding names its own node"
        );
        assert_eq!(binding("work_graph/draft"), None);

        let exported = export_preset(&document).expect("export");
        assert_eq!(
            exported
                .inner_graphs
                .as_ref()
                .and_then(|graphs| graphs.get("work_graph"))
                .and_then(|graph| graph.output_binding.as_deref()),
            Some("review")
        );
        assert_eq!(
            serde_json::to_value(&exported).expect("exported json"),
            serde_json::to_value(&manifest).expect("manifest json"),
            "a dotless binding survives import and export unchanged"
        );
    }

    /// A dotted step id (`review.v2`) makes the loader's owner segment (`review`) name no step, so
    /// every projection refuses instead of dropping the binding.
    #[test]
    fn import_refuses_an_output_binding_whose_owner_is_no_step() {
        let manifest: PresetManifest = serde_yaml::from_str(
            &MANIFEST.replace("- id: review", "- id: review.v2").replace(
                "output_binding: review.text",
                "output_binding: review.v2.text",
            ),
        )
        .expect("manifest parses");

        let error = import_preset_manifest(&manifest)
            .expect_err("an owner segment that names no step is refused");
        assert!(matches!(error, WorkflowIrError::NotProjectable { .. }));
        let message = error.to_string();
        for expected in ["work_graph", "`review.v2.text`", "`review`"] {
            assert!(message.contains(expected), "{message}");
        }
    }

    /// An inner-graph name may contain `/`: the step is the last id segment, so export must not
    /// split at the first one and turn the real owner into a phantom.
    #[test]
    fn import_and_export_round_trip_a_graph_name_with_a_slash() {
        let (dir, _) = bundle();
        let manifest: PresetManifest = serde_yaml::from_str(
            &MANIFEST
                .replace("  work_graph:", "  lane/work_graph:")
                .replace("name: work_graph", "name: lane/work_graph"),
        )
        .expect("manifest parses");

        let imported = import_preset_bundle(
            &manifest,
            &FsAssetSource::new(dir.path()).expect("bundle root"),
        )
        .expect("import");
        let review = imported
            .document
            .nodes
            .iter()
            .find(|node| node.id == "lane/work_graph/review")
            .expect("slashed graph step node");
        assert_eq!(review.output_binding.as_deref(), Some("review.text"));

        let exported = export_preset(&imported.document).expect("export");
        assert_eq!(
            serde_json::to_value(&exported).expect("exported json"),
            serde_json::to_value(&manifest).expect("manifest json"),
            "a graph name with a slash survives import and export unchanged"
        );
    }

    /// One state enters a graph and a nested graph; the parent graph carries a same-graph
    /// dependency. Export must keep both expressible, and the loader must accept the result.
    const GRAPH_REFERENCE_MANIFEST: &str = r"
preset:
  id: graph-refs
  version: 1
  kind: creator
  description: graph references
  requires_capabilities: []
  run_intents: [work_init]
  initial: a
  terminal: done
states:
  - id: a
    enter:
      - kind: inner_graph
        name: lane
      - kind: inner_graph
        name: lane/utility
    exit_when: { kind: graph_complete }
    next: done
  - id: done
    terminal: true
inner_graphs:
  lane:
    nodes:
      - id: x
        kind: acp_prompt
      - id: y
        kind: acp_prompt
        depends_on: [x]
  lane/utility:
    nodes:
      - id: check
        kind: acp_prompt
";

    fn graph_reference_manifest() -> PresetManifest {
        serde_yaml::from_str(GRAPH_REFERENCE_MANIFEST).expect("manifest parses")
    }

    /// Threads 1 and 2, positive: document -> manifest -> loader. The exported preset names the
    /// graphs that own the steps and keeps the same-graph dependency local, and the loader accepts
    /// it.
    #[test]
    fn exported_graph_references_and_dependencies_load_cleanly() {
        let document = import_preset_manifest(&graph_reference_manifest()).expect("import");
        let exported = export_preset(&document).expect("export");
        let yaml = serde_yaml::to_string(&exported).expect("yaml");

        let loaded = crate::load_preset_from_str(
            &yaml,
            &crate::capability_catalog::BuiltinCapabilityCatalog,
        )
        .expect("the loader accepts the exported preset");

        let enters: Vec<&str> = loaded
            .manifest
            .states
            .iter()
            .flat_map(|state| state.enter.iter())
            .filter_map(|enter| match enter {
                crate::manifest::EnterAction::InnerGraph { name } => Some(name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(enters, vec!["lane", "lane/utility"]);

        let lane = loaded
            .manifest
            .inner_graphs
            .as_ref()
            .and_then(|graphs| graphs.get("lane"))
            .expect("lane graph loads");
        let y = lane
            .nodes
            .iter()
            .find(|node| node.id == "y")
            .expect("y step");
        assert_eq!(y.depends_on, vec!["x".to_string()]);
    }

    /// Thread 1, negative: steps that only exist under a nested graph name leave the state's
    /// `enter` dangling, and a preset cannot express that.
    #[test]
    fn export_refuses_a_state_entering_a_graph_with_no_steps() {
        let mut document = import_preset_manifest(&graph_reference_manifest()).expect("import");
        for node in &mut document.nodes {
            if node.id == "lane/x" || node.id == "lane/y" {
                node.id = node.id.replacen("lane/", "lane/nested/", 1);
            }
            for dependency in &mut node.depends_on {
                if dependency == "lane/x" {
                    *dependency = "lane/nested/x".to_string();
                }
            }
        }

        let error =
            export_preset(&document).expect_err("a dangling graph reference has no preset form");
        assert!(matches!(error, WorkflowIrError::NotProjectable { .. }));
        let message = error.to_string();
        for expected in ["state `a`", "lane"] {
            assert!(message.contains(expected), "{message}");
        }
    }

    /// Thread 2, negative: the loader resolves `depends_on` only within one graph, so a
    /// cross-graph dependency must be refused instead of exported as a local id.
    #[test]
    fn export_refuses_a_cross_graph_dependency() {
        let mut document = import_preset_manifest(&graph_reference_manifest()).expect("import");
        document
            .nodes
            .iter_mut()
            .find(|node| node.id == "lane/y")
            .expect("y step")
            .depends_on = vec!["lane/utility/check".to_string()];

        let error =
            export_preset(&document).expect_err("a cross-graph dependency has no preset form");
        assert!(matches!(error, WorkflowIrError::NotProjectable { .. }));
        let message = error.to_string();
        for expected in ["lane/y", "lane/utility/check", "cannot express"] {
            assert!(message.contains(expected), "{message}");
        }
    }

    /// A bare dependency id the graph does not carry is a loader "unknown node", so export refuses
    /// it too — the valid same-graph direction is covered by the round-trip test above.
    #[test]
    fn export_refuses_an_unknown_bare_dependency() {
        let mut document = import_preset_manifest(&graph_reference_manifest()).expect("import");
        document
            .nodes
            .iter_mut()
            .find(|node| node.id == "lane/y")
            .expect("y step")
            .depends_on = vec!["ghost".to_string()];

        let error = export_preset(&document).expect_err("an unknown dependency has no preset form");
        assert!(matches!(error, WorkflowIrError::NotProjectable { .. }));
        let message = error.to_string();
        for expected in ["lane/y", "ghost", "lane"] {
            assert!(message.contains(expected), "{message}");
        }
    }

    /// A step id is one id segment: a slashed step would export under a different graph than the
    /// state's `enter` names, so import refuses it.
    #[test]
    fn import_refuses_a_step_id_with_a_slash() {
        let manifest: PresetManifest =
            serde_yaml::from_str(&MANIFEST.replace("- id: draft", "- id: a/b"))
                .expect("manifest parses");

        let error = import_preset_manifest(&manifest).expect_err("a slashed step id is refused");
        assert!(matches!(error, WorkflowIrError::NotProjectable { .. }));
        let message = error.to_string();
        for expected in ["work_graph", "`a/b`", "single segment"] {
            assert!(message.contains(expected), "{message}");
        }
    }

    /// An inner graph no state enters is unreferenced, not malformed: it still exports, even when
    /// its name contains `/`.
    #[test]
    fn export_keeps_an_unreferenced_nested_graph() {
        let (dir, manifest) = bundle();
        let mut imported = import_preset_bundle(
            &manifest,
            &FsAssetSource::new(dir.path()).expect("bundle root"),
        )
        .expect("import");
        let mut orphan = imported
            .document
            .nodes
            .iter()
            .find(|node| node.id == "work_graph/draft")
            .expect("draft node")
            .clone();
        orphan.id = "lane/orphan/n1".to_string();
        orphan.output_binding = None;
        orphan.depends_on.clear();
        imported.document.nodes.push(orphan);

        let exported = export_preset(&imported.document).expect("an unreferenced graph exports");
        let orphan_graph = exported
            .inner_graphs
            .as_ref()
            .and_then(|graphs| graphs.get("lane/orphan"))
            .expect("the unreferenced nested graph keeps its key");
        assert_eq!(orphan_graph.nodes.len(), 1);
        assert_eq!(orphan_graph.nodes[0].id, "n1");
        assert!(orphan_graph.output_binding.is_none());
    }

    /// Export refuses the same shape as import: a binding whose owner is no sibling step must not
    /// leave as a manifest that import would then refuse.
    #[test]
    fn export_refuses_an_output_binding_whose_owner_is_no_step() {
        let (dir, manifest) = bundle();
        let mut imported = import_preset_bundle(
            &manifest,
            &FsAssetSource::new(dir.path()).expect("bundle root"),
        )
        .expect("import");
        imported
            .document
            .nodes
            .iter_mut()
            .find(|node| node.id == "work_graph/review")
            .expect("review node")
            .output_binding = Some("ghost.text".to_string());

        let error = export_preset(&imported.document)
            .expect_err("an owner that names no step has no projection");
        assert!(matches!(error, WorkflowIrError::NotProjectable { .. }));
        let message = error.to_string();
        for expected in ["work_graph", "`ghost.text`", "`ghost`"] {
            assert!(message.contains(expected), "{message}");
        }
    }

    #[test]
    fn export_refuses_ir_only_node_kinds() {
        let (dir, manifest) = bundle();
        let mut imported = import_preset_bundle(
            &manifest,
            &FsAssetSource::new(dir.path()).expect("bundle root"),
        )
        .expect("import");
        imported
            .document
            .nodes
            .iter_mut()
            .find(|node| node.id == "start")
            .expect("start node")
            .kind = WorkflowNodeKind::Judge;

        let error = export_preset(&imported.document).expect_err("judge node is IR-only");
        assert!(matches!(error, WorkflowIrError::NotProjectable { .. }));
        assert!(error.to_string().contains("no preset projection"));
    }

    #[test]
    fn export_refuses_duplicate_default_edges() {
        let (dir, manifest) = bundle();
        let mut imported = import_preset_bundle(
            &manifest,
            &FsAssetSource::new(dir.path()).expect("bundle root"),
        )
        .expect("import");
        imported.document.edges.push(WorkflowEdge {
            id: "work->start#second-default".to_string(),
            from: "work".to_string(),
            to: "start".to_string(),
            kind: WorkflowEdgeKind::Default,
            label: None,
            when: None,
        });

        let error =
            export_preset(&imported.document).expect_err("two default edges have no projection");
        assert!(matches!(error, WorkflowIrError::NotProjectable { .. }));
        assert!(error.to_string().contains("more than one default edge"));
    }

    #[test]
    fn export_refuses_labeled_families_with_foreign_edge_kinds() {
        let (dir, manifest) = bundle();
        let mut imported = import_preset_bundle(
            &manifest,
            &FsAssetSource::new(dir.path()).expect("bundle root"),
        )
        .expect("import");
        imported.document.edges.push(WorkflowEdge {
            id: "done->start#labeled".to_string(),
            from: "done".to_string(),
            to: "start".to_string(),
            kind: WorkflowEdgeKind::Linear,
            label: Some("retry".to_string()),
            when: None,
        });
        imported.document.edges.push(WorkflowEdge {
            id: "done->work#nogo".to_string(),
            from: "done".to_string(),
            to: "work".to_string(),
            kind: WorkflowEdgeKind::Nogo,
            label: Some("abort".to_string()),
            when: None,
        });

        let error =
            export_preset(&imported.document).expect_err("foreign kind has no labeled projection");
        assert!(matches!(error, WorkflowIrError::NotProjectable { .. }));
        assert!(error
            .to_string()
            .contains("mixes a `nogo` edge into its labeled edges"));
    }

    #[test]
    fn export_refuses_edges_beyond_the_go_nogo_pair() {
        let (dir, manifest) = bundle();
        let mut imported = import_preset_bundle(
            &manifest,
            &FsAssetSource::new(dir.path()).expect("bundle root"),
        )
        .expect("import");
        imported.document.edges.push(WorkflowEdge {
            id: "start->work#sibling".to_string(),
            from: "start".to_string(),
            to: "work".to_string(),
            kind: WorkflowEdgeKind::Linear,
            label: None,
            when: None,
        });

        let error =
            export_preset(&imported.document).expect_err("sibling edge has no preset projection");
        assert!(matches!(error, WorkflowIrError::NotProjectable { .. }));
        assert!(error.to_string().contains("beyond its go/nogo pair"));
    }

    #[test]
    fn import_refuses_multiple_terminal_states() {
        let dir = tempfile::tempdir().expect("tempdir");
        let manifest: PresetManifest = serde_yaml::from_str(
            r#"
preset:
  id: two-terminals
  version: 1
  kind: creator
  description: "two terminals"
  requires_capabilities: []
  initial: a
  terminal: a
states:
  - id: a
    terminal: true
  - id: b
    terminal: true
"#,
        )
        .expect("manifest parses");

        let error = import_preset_bundle(
            &manifest,
            &FsAssetSource::new(dir.path()).expect("bundle root"),
        )
        .expect_err("multi-terminal refused");
        assert!(matches!(
            error,
            WorkflowIrError::MultipleTerminalStates { count: 2 }
        ));
    }

    #[test]
    fn import_refuses_unsafe_asset_paths() {
        let dir = tempfile::tempdir().expect("tempdir");
        let manifest: PresetManifest =
            serde_yaml::from_str(&MANIFEST.replace("prompts/judge.md", "../evil.md"))
                .expect("manifest parses");

        let error = import_preset_bundle(
            &manifest,
            &FsAssetSource::new(dir.path()).expect("bundle root"),
        )
        .expect_err("parent traversal refused");
        assert!(matches!(error, WorkflowIrError::NotProjectable { .. }));
        assert!(error.to_string().contains("unsafe asset path `../evil.md`"));
    }
}
