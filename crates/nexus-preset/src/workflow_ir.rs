//! `PresetManifest` ↔ [`WorkflowDocument`] import adapter (Workflow IR v0).
//!
//! Import is a projection, never a second grammar: every executable preset carrier (`signals`,
//! `roles`, `initial_action`, `gates`, `cli_args`, inner-graph `output_binding`, every `exit_when`
//! kind) lands in an IR field, and [`export_preset`] rebuilds the manifest from those fields
//! alone. [`PresetSourceIdentity`](crate::source_identity::PresetSourceIdentity) is untouched —
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
//! shortcut: legacy `next: { kind: conditional, rules: [...] }` exports as the equivalent
//! `branches` form; upgrade when a consumer needs the legacy spelling back.

use crate::manifest::{
    ConditionalBranches, ConditionalRule, ContextUpdateHook, ContextUpdateOp, ConvergeConfig,
    ConvergeStrategy, EnterAction, ExitWhen, GoNogoNext, GraphNode, GraphNodeKind, InitialAction,
    InnerGraph, LabeledNext, MergeKind, NextTarget, PresetCliArg, PresetCliArgType, PresetHeader,
    PresetKind, PresetManifest, RunIntent, SignalAction, SignalActionKind, SignalBinding,
    StateDefinition,
};
use nexus_contracts::local::orchestration::preset_gate::{Gate, GateOp, PreviousPresetStatus};
use nexus_workflow_ir::{
    WorkflowAssetRef, WorkflowAssetRefKind, WorkflowCliArg, WorkflowCliArgType,
    WorkflowContextUpdate, WorkflowContextUpdateOp, WorkflowConverge, WorkflowConvergeStrategy,
    WorkflowDocument, WorkflowDocumentEffectSummary, WorkflowDocumentId, WorkflowDocumentState,
    WorkflowEdge, WorkflowEdgeKind, WorkflowEnterAction, WorkflowEnterActionKind, WorkflowExitWhen,
    WorkflowExitWhenKind, WorkflowGate, WorkflowGateKind, WorkflowGateOp, WorkflowGateStatus,
    WorkflowInitialAction, WorkflowInitialActionKind, WorkflowMerge, WorkflowMergeKind,
    WorkflowMeta, WorkflowMetaKind, WorkflowMetaRunIntentsItem, WorkflowNode,
    WorkflowNodeEffectClass, WorkflowNodeKind, WorkflowNodeStepKind, WorkflowNodeUi,
    WorkflowProvenance, WorkflowProvenanceSource, WorkflowRoleDefinition, WorkflowSignalBinding,
    WorkflowSignalBindingAction,
};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use thiserror::Error;

// IR version emitted by this adapter; v0 stays 0 until P1 parity.
const IR_VERSION: u64 = 0;

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
}

fn refuse(reason: impl Into<String>) -> WorkflowIrError {
    WorkflowIrError::NotProjectable {
        reason: reason.into(),
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

// ---------------------------------------------------------------------------
// Import: PresetManifest -> WorkflowDocument
// ---------------------------------------------------------------------------

/// Import a preset manifest into a Workflow IR document, hashing every asset the manifest
/// references from `bundle_root`.
///
/// # Errors
///
/// Returns [`WorkflowIrError::AssetRead`] when a referenced asset is missing or unreadable, and
/// [`WorkflowIrError::NotProjectable`] when a referenced path is unsafe (absolute, `..` traversal,
/// or a symlink resolving outside the bundle) or the manifest header cannot form a document.
pub fn import_preset_bundle(
    manifest: &PresetManifest,
    bundle_root: &Path,
) -> Result<PresetIrImport, WorkflowIrError> {
    let mut references = crate::validation::collect_asset_file_references(manifest);
    references.sort_by(|a, b| a.1.cmp(&b.1));
    references.dedup_by(|a, b| a.1 == b.1);

    // Fail closed before touching the filesystem: the manifest chooses these paths, and the same
    // rule the loader/validator enforce (`assert_template_file_safe`) applies here.
    for (_, path) in &references {
        if let Err(reason) = crate::loader::assert_template_file_safe(path) {
            return Err(refuse(format!("unsafe asset path `{path}`: {reason}")));
        }
    }
    let canonical_root =
        bundle_root
            .canonicalize()
            .map_err(|source| WorkflowIrError::AssetRead {
                path: ".".to_string(),
                source,
            })?;

    let mut assets = Vec::with_capacity(references.len());
    let mut asset_bytes = BTreeMap::new();
    for (_, path) in references {
        let canonical = canonical_root
            .join(&path)
            .canonicalize()
            .map_err(|source| WorkflowIrError::AssetRead {
                path: path.clone(),
                source,
            })?;
        if !canonical.starts_with(&canonical_root) {
            return Err(refuse(format!(
                "asset `{path}` resolves outside the bundle sandbox (symlink escape)"
            )));
        }
        let bytes = std::fs::read(&canonical).map_err(|source| WorkflowIrError::AssetRead {
            path: path.clone(),
            source,
        })?;
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

    Ok(PresetIrImport {
        document: document_from_manifest(manifest, assets)?,
        assets: asset_bytes,
    })
}

/// Project a manifest into IR for semantic validation: no filesystem reads, asset references carry
/// only their bundle-relative paths.
///
/// Unlike [`import_preset_bundle`] this does not refuse a manifest whose terminal markers disagree
/// with `preset.terminal` or whose asset files are missing or unsafe — validation reports those
/// defects on the document instead.
///
/// # Errors
///
/// Returns [`WorkflowIrError::NotProjectable`] when `preset.id` is not a valid IR id.
pub fn import_preset_manifest(
    manifest: &PresetManifest,
) -> Result<WorkflowDocument, WorkflowIrError> {
    let mut references = crate::validation::collect_asset_file_references(manifest);
    references.sort_by(|a, b| a.1.cmp(&b.1));
    references.dedup_by(|a, b| a.1 == b.1);
    let assets = references
        .into_iter()
        .map(|(_, path)| WorkflowAssetRef {
            id: path.clone(),
            path,
            kind: WorkflowAssetRefKind::Template,
            content_hash: None,
        })
        .collect();
    document_from_manifest(manifest, assets)
}

fn document_from_manifest(
    manifest: &PresetManifest,
    assets: Vec<WorkflowAssetRef>,
) -> Result<WorkflowDocument, WorkflowIrError> {
    let header = &manifest.preset;
    let id = WorkflowDocumentId::try_from(header.id.clone())
        .map_err(|e| refuse(format!("preset id `{}` is not an IR id: {e}", header.id)))?;

    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let mut edge_ids = HashSet::new();
    for state in &manifest.states {
        nodes.push(state_node(state));
        push_next_edges(state, &mut edges, &mut edge_ids);
    }
    if let Some(graphs) = &manifest.inner_graphs {
        for (graph_name, graph) in graphs {
            for step in &graph.nodes {
                nodes.push(inner_step_node(graph_name, graph, step));
            }
        }
    }

    let effect_summary = summary_effect(&nodes);
    // The header names the declared terminal; per-state markers ride on the node kinds. A preset
    // whose markers disagree with the header still projects, so validation can report the defect.
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
        timeout_ms: state
            .timeout_ms
            .map(|ms| i64::try_from(ms).unwrap_or(i64::MAX)),
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

fn inner_step_node(graph_name: &str, graph: &InnerGraph, step: &GraphNode) -> WorkflowNode {
    let owns_output = graph.output_binding.as_deref().is_some_and(|binding| {
        binding.rsplit_once('.').map_or(binding, |(owner, _)| owner) == step.id
    });
    WorkflowNode {
        id: format!("{graph_name}/{}", step.id),
        kind: WorkflowNodeKind::InnerStep,
        // An ACP prompt can reach any tool through its policy: worst case is external.
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

fn push_next_edges(
    state: &StateDefinition,
    edges: &mut Vec<WorkflowEdge>,
    used: &mut HashSet<String>,
) {
    let from = state.id.as_str();
    match &state.next {
        None => {}
        Some(NextTarget::Linear(to)) => {
            push_edge(edges, used, from, to, WorkflowEdgeKind::Linear, None, None);
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
            push_conditional_edges(edges, used, from, &conditional.rules, &conditional.default);
        }
        Some(NextTarget::Branches(branches)) => {
            push_conditional_edges(edges, used, from, &branches.branches, &branches.default);
        }
    }
}

fn push_conditional_edges(
    edges: &mut Vec<WorkflowEdge>,
    used: &mut HashSet<String>,
    from: &str,
    rules: &[ConditionalRule],
    default: &str,
) {
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
            n: Some(i64::try_from(*n).unwrap_or(i64::MAX)),
            m: Some(i64::try_from(*m).unwrap_or(i64::MAX)),
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
            Some(body.clone()),
            None,
            None,
            None,
        ),
        ContextUpdateOp::StructMerge { patch } => (
            WorkflowContextUpdateOp::StructMerge,
            None,
            None,
            Some(patch.clone()),
            None,
        ),
        ContextUpdateOp::Replace { body } => (
            WorkflowContextUpdateOp::Replace,
            Some(body.clone()),
            None,
            None,
            None,
        ),
        ContextUpdateOp::StructRemove { path } => (
            WorkflowContextUpdateOp::StructRemove,
            None,
            None,
            None,
            Some(path.clone()),
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
    // Every `EnterAction` variant reaches an outside caller (capability, inner graph, host tool)
    // and a judge exit calls a capability; v0 has no capability-effect catalog, so worst case.
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

const fn effect_rank(class: WorkflowNodeEffectClass) -> u8 {
    match class {
        WorkflowNodeEffectClass::External => 3,
        WorkflowNodeEffectClass::WriteCandidate => 2,
        WorkflowNodeEffectClass::Read => 1,
        WorkflowNodeEffectClass::Pure => 0,
    }
}

fn summary_effect(nodes: &[WorkflowNode]) -> WorkflowDocumentEffectSummary {
    let rank = nodes
        .iter()
        .map(|node| effect_rank(node.effect_class))
        .max()
        .unwrap_or(0);
    match rank {
        3 => WorkflowDocumentEffectSummary::External,
        2 => WorkflowDocumentEffectSummary::WriteCandidate,
        1 => WorkflowDocumentEffectSummary::Read,
        _ => WorkflowDocumentEffectSummary::Pure,
    }
}

// ---------------------------------------------------------------------------
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

fn project_nodes(document: &WorkflowDocument) -> Result<ProjectedNodes, WorkflowIrError> {
    let mut states = Vec::new();
    let mut steps: BTreeMap<String, Vec<GraphNode>> = BTreeMap::new();
    let mut bindings: BTreeMap<String, String> = BTreeMap::new();
    let outgoing = outgoing_edges(document);

    for node in &document.nodes {
        match &node.kind {
            WorkflowNodeKind::OuterState | WorkflowNodeKind::Terminal => {
                let next_edges = outgoing
                    .get(node.id.as_str())
                    .map_or(&[][..], |edges| edges.as_slice());
                states.push(state_from_node(node, next_edges)?);
            }
            WorkflowNodeKind::InnerStep => {
                let (graph, step_id) = node.id.split_once('/').ok_or_else(|| {
                    refuse(format!(
                        "inner step `{}` is not `<inner_graph>/<node_id>`",
                        node.id
                    ))
                })?;
                let graph_key = graph.to_string();
                if let Some(binding) = &node.output_binding {
                    bindings.insert(graph_key.clone(), binding.clone());
                }
                steps.entry(graph_key).or_default().push(GraphNode {
                    id: step_id.to_string(),
                    kind: GraphNodeKind::AcpPrompt,
                    depends_on: node
                        .depends_on
                        .iter()
                        .map(|dependency| {
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
        next: next_from_edges(&node.id, edges)?,
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
        timeout_ms: node
            .timeout_ms
            .map(|ms| {
                u64::try_from(ms)
                    .map_err(|_| refuse(format!("node `{}` has a negative timeout_ms", node.id)))
            })
            .transpose()?,
        on_timeout: node.on_timeout.clone(),
    })
}

fn next_from_edges(
    node_id: &str,
    edges: &[&WorkflowEdge],
) -> Result<Option<NextTarget>, WorkflowIrError> {
    let Some(first) = edges.first() else {
        return Ok(None);
    };

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
            body: update
                .body
                .clone()
                .ok_or_else(|| refuse("append context update carries no body"))?,
        },
        WorkflowContextUpdateOp::Replace => ContextUpdateOp::Replace {
            body: update
                .body
                .clone()
                .ok_or_else(|| refuse("replace context update carries no body"))?,
        },
        WorkflowContextUpdateOp::StructMerge => ContextUpdateOp::StructMerge {
            patch: update
                .patch
                .clone()
                .ok_or_else(|| refuse("struct-merge context update carries no patch"))?,
        },
        WorkflowContextUpdateOp::StructRemove => ContextUpdateOp::StructRemove {
            path: update
                .path
                .clone()
                .ok_or_else(|| refuse("struct-remove context update carries no path"))?,
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
        let imported = import_preset_bundle(&manifest, dir.path()).expect("import");

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
            WorkflowDocumentEffectSummary::External
        );

        let exported = export_preset(document).expect("export");
        assert_eq!(
            serde_json::to_value(&exported).expect("exported json"),
            serde_json::to_value(&manifest).expect("manifest json")
        );
    }

    #[test]
    fn export_refuses_ir_only_node_kinds() {
        let (dir, manifest) = bundle();
        let mut imported = import_preset_bundle(&manifest, dir.path()).expect("import");
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
        let mut imported = import_preset_bundle(&manifest, dir.path()).expect("import");
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
        let mut imported = import_preset_bundle(&manifest, dir.path()).expect("import");
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
        let mut imported = import_preset_bundle(&manifest, dir.path()).expect("import");
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

        let error =
            import_preset_bundle(&manifest, dir.path()).expect_err("multi-terminal refused");
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

        let error =
            import_preset_bundle(&manifest, dir.path()).expect_err("parent traversal refused");
        assert!(matches!(error, WorkflowIrError::NotProjectable { .. }));
        assert!(error.to_string().contains("unsafe asset path `../evil.md`"));
    }
}
