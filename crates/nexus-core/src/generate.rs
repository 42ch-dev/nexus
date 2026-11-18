//! Generate: one candidate document from a request, through a bounded lint-correct loop.
//!
//! Plan: `.mstar/plans/canvas-p6-retirement.md` Part C (D19); spec:
//! `.mstar/iterations/v1.247-hygiene/specs/generate-and-lint-loop.md` §2–§6.
//!
//! One kernel, three transports: the CLI, the agent tool and the HTTP route all call
//! [`CoreService::generate_strategy_candidate`], each passing its own [`PromptExecutor`] or `None`.
//! There is no trait between them — the surfaces are transports of one implementation, and the
//! all-three-reach-one-kernel test is the drift guard. What the kernel shares with Compose is
//! exactly two pieces: the pattern-registry scan ([`CoreService::compose_packages`]) and the CAS
//! commit ([`CoreService::commit_strategy_candidate`]), whose closure ignores the current document
//! because Generate produces a whole document rather than an operation on one.
//!
//! Round 1 materializes the request deterministically and touches no model. Rounds 2 and 3 hand
//! the previous round's document and **all** of that round's corrections to the corrector, whose
//! response is normalized back onto the kernel's identity and the request's field space, so
//! provenance and the D8 bound hold for every round. Lint is the three layers the rest of the
//! system already uses — structural, semantic, buildability — plus the pin and slot findings
//! Approve refuses on, and a round passes only when all of them are silent, so semantic warnings
//! never block. The ceiling is three rounds; exhaustion returns the LAST round's corrections and
//! persists nothing, because the commit is reached only from a passing round.

use nexus_capability::{PromptExecutor, PromptRequest, ToolPolicy};
use nexus_contracts::generated::daemon_api::orchestration::workflow_document::{
    WorkflowDocumentEffectSummary, WorkflowDocumentId, WorkflowDocumentState, WorkflowEdge,
    WorkflowMeta, WorkflowNestBinding, WorkflowNode,
};
use nexus_orchestration::preset_runtime::{
    validate_ir_buildability, IrOuterGraphError, IrTemplateSource,
};
use nexus_pattern::PatternPackage;
use nexus_preset::capability_catalog::BuiltinCapabilityCatalog;
use nexus_preset::validation::{validate_workflow_ir, DiagnosticCategory, ValidationDiagnostic};
use nexus_preset::workflow_ir::WorkflowIrError;
use nexus_workflow_ir::{
    document_summary, validate, PatternBody, Violation, WorkflowDocument, WorkflowProvenance,
    WorkflowProvenanceSource, IR_VERSION, MAX_NEST_DEPTH,
};

use crate::compose::ComposeDiagnostic;
use crate::strategy_candidates::conflict;
use crate::{CoreError, CoreResult, CoreService, Principal, StrategyCandidate};

/// At most three rounds: one deterministic materialization and two corrections.
const MAX_ROUNDS: usize = 3;

/// The D8 contract: the pattern-package / IR subset a generate request may declare.
///
/// The field space is the IR subset and nothing else — node declarations, edges, nest bindings and
/// the meta fields [`validate_workflow_ir`] reads (run intents, CLI args). There is no executable
/// code here and no flag that bypasses lint, so a request can only ask for a document; whether it
/// is structurally legal is decided by lint, never by the caller. The agent tool renders this type
/// as its input schema, the CLI parses arguments into it, and the HTTP body deserializes into it, so
/// all three surfaces carry one request type.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerateRequest {
    /// Entry node id. Replaces `preset.initial`.
    pub entry: String,
    /// Terminal node ids; a buildable document declares exactly one.
    pub terminal: Vec<String>,
    /// Node declarations: kind, id and the payload of that kind.
    pub nodes: Vec<WorkflowNode>,
    /// Document edges.
    pub edges: Vec<WorkflowEdge>,
    /// Nest bindings and pattern pins.
    pub nests: Vec<WorkflowNestBinding>,
    /// Document meta: description, kind, run intents, CLI args.
    pub meta: WorkflowMeta,
}

/// One actionable correction: what is wrong, where, and what to change it to.
///
/// `kind` is the stable `snake_case` machine identifier — the [`Violation`] variant, the semantic
/// diagnostic's [`DiagnosticCategory`], or the buildability refusal kind — so a caller can count
/// repeat kinds across rounds.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Correction {
    /// Stable `snake_case` machine identifier.
    pub kind: &'static str,
    /// The address inside the document: a document id, node id, nest id or dot path.
    pub location: String,
    /// What was actually seen.
    pub observed: String,
    /// The imperative change, naming what to change it to.
    pub correction: String,
}

/// A committed candidate plus the semantic warnings its passing round carried.
#[derive(Debug, Clone, serde::Serialize)]
pub struct GenerateOutcome {
    /// The committed candidate: the row revision is the CAS revision.
    pub candidate: StrategyCandidate,
    /// The passing round's warnings. They never block; they are reported.
    pub warnings: Vec<ValidationDiagnostic>,
}

impl GenerateOutcome {
    /// The wire projection the HTTP and agent-tool surfaces carry: the committed document, its
    /// revision, and the passing round's warnings.
    ///
    /// Both surfaces render this one object, so a candidate never reads differently depending on
    /// which transport committed it. `candidate.content_hash` and `candidate.revision` are already
    /// on the document the CAS stored, so nothing is restated here.
    #[must_use]
    pub fn wire(&self) -> serde_json::Value {
        serde_json::json!({
            "new_revision": self.candidate.revision,
            "document": self.candidate.document,
            "warnings": self.warnings,
        })
    }
}

/// The lint-correct loop ran out of rounds. Nothing was persisted and there is no resume.
///
/// One shape for both exits (spec §3): the ceiling after three failed rounds, and a failing round 1
/// with no corrector supplied. [`Self::details`] is the D20 payload every surface renders.
#[derive(Debug, Clone)]
pub struct GenerateExhaustion {
    rounds_attempted: usize,
    violations: Vec<Correction>,
    message: String,
}

impl GenerateExhaustion {
    const fn new(rounds_attempted: usize, violations: Vec<Correction>, message: String) -> Self {
        Self {
            rounds_attempted,
            violations,
            message,
        }
    }

    /// The D20 payload: one object, three renderings.
    #[must_use]
    pub fn details(&self) -> serde_json::Value {
        serde_json::json!({
            "code": "lint_exhausted",
            "rounds_attempted": self.rounds_attempted,
            "violations": self.violations,
            "persisted": false,
            "message": self.message,
        })
    }

    /// The core error a refused generate returns; the adapter renders `details` verbatim.
    #[must_use]
    pub fn into_error(self) -> CoreError {
        CoreError::InputValidation {
            details: self.details(),
        }
    }
}

impl CoreService {
    /// Generate one candidate for this strategy from `request`, at `base_revision`.
    ///
    /// Round 1 materializes the request; each failing round hands its corrections to `corrector`
    /// for a revised document. A survivor is committed through the same CAS statement Compose
    /// uses, so a first candidate inserts at revision 1 and a replacement bumps the row; a stale
    /// base is the store's typed conflict and is refused before any round runs.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InputValidation`] carrying a [`GenerateExhaustion::details`] envelope
    /// when the loop runs out of rounds (nothing was persisted), [`CoreError::Coded`] with code
    /// `corrector_failed` when the executor refuses or returns no document, [`CoreError::InvalidInput`]
    /// for a strategy id that is not a document id, [`CoreError::StrategyCandidateConflict`] for a
    /// stale base, and the principal/store errors otherwise.
    pub async fn generate_strategy_candidate(
        &self,
        principal: &Principal,
        strategy_id: &str,
        base_revision: u64,
        request: GenerateRequest,
        corrector: Option<&dyn PromptExecutor>,
    ) -> CoreResult<GenerateOutcome> {
        self.verify_principal(principal)?;
        // A stale base is refused from the row alone: three model rounds must not run for a write
        // the CAS refuses anyway.
        let current = self.read_strategy_candidate(principal, strategy_id).await?;
        if current.revision != base_revision {
            return Err(conflict(strategy_id, base_revision, current.revision));
        }
        let packages = self.compose_packages()?;
        let mut document = materialize(request, strategy_id)?;
        let mut round = 1;
        let warnings = loop {
            let (bodies, pin_diagnostics) = crate::compose::document_bodies(&document, &packages);
            // The rollup is derived from the bytes under lint, never trusted from the request or
            // the corrector: an under-declared fence would hide a write from Simulate and Approve.
            let summary = document_summary(&document, &bodies);
            let linted = lint(&document, &bodies, &pin_diagnostics, &packages);
            if linted.violations.is_empty() {
                document.effect_summary = summary;
                break linted.warnings;
            }
            let Some(corrector) = corrector else {
                return Err(GenerateExhaustion::new(
                    round,
                    linted.violations,
                    "no corrector was supplied, so round 1 could not be corrected; nothing was \
                     persisted, and the next step is a fresh generate request"
                        .to_owned(),
                )
                .into_error());
            };
            if round >= MAX_ROUNDS {
                return Err(GenerateExhaustion::new(
                    round,
                    linted.violations,
                    format!(
                        "the lint-correct loop reached its ceiling of {MAX_ROUNDS} rounds with the \
                         document still failing lint; nothing was persisted, and the next step is a \
                         fresh generate request"
                    ),
                )
                .into_error());
            }
            document.effect_summary = summary;
            document =
                correct(corrector, strategy_id, round, &document, &linted.violations).await?;
            round += 1;
        };

        let candidate = self
            .commit_strategy_candidate(principal, strategy_id, base_revision, move |_current| {
                Ok(document)
            })
            .await?;
        Ok(GenerateOutcome {
            candidate,
            warnings,
        })
    }
}

/// The provenance of a document this kernel generated.
///
/// It is the kernel's, never a model's: the three-surface drift guard reads
/// `provenance.source == generate`, so a corrector must not be able to set it (or invent lineage)
/// from the inside. Round 1 and every corrected round record this same origin.
const fn generated_provenance() -> WorkflowProvenance {
    WorkflowProvenance {
        source: WorkflowProvenanceSource::Generate,
        agent_run_id: None,
        parent_candidate_id: None,
        resolved_pins: Vec::new(),
    }
}

/// Build the round-1 document from the request alone.
///
/// Nothing is invented: every node, edge and nest is the request's, and the fields the request
/// does not carry (assets, signals, roles) stay empty rather than being synthesized. The effect
/// rollup and the revision are assigned outside — the rollup from the document's own bytes and the
/// revision by the CAS commit.
fn materialize(request: GenerateRequest, strategy_id: &str) -> CoreResult<WorkflowDocument> {
    let id = strategy_id
        .parse::<WorkflowDocumentId>()
        .map_err(|_| CoreError::InvalidInput {
            field: "strategy_id".into(),
            reason: format!("`{strategy_id}` is not a workflow document id"),
        })?;
    Ok(WorkflowDocument {
        ir_version: IR_VERSION,
        id,
        version: 1,
        revision: 0,
        content_hash: None,
        state: WorkflowDocumentState::Candidate,
        meta: request.meta,
        entry: request.entry,
        initial_action: None,
        terminal: request.terminal,
        nodes: request.nodes,
        edges: request.edges,
        nests: request.nests,
        assets: Vec::new(),
        signals: Vec::new(),
        roles: Vec::new(),
        effect_summary: WorkflowDocumentEffectSummary::Pure,
        provenance: generated_provenance(),
    })
}

/// One round's verdict: the corrections that block, and the warnings a passing round carries out.
struct Round {
    violations: Vec<Correction>,
    warnings: Vec<ValidationDiagnostic>,
}

/// The three existing lint layers, plus the pin and slot findings Approve refuses on.
///
/// Layers 1–3 are the ones the rest of the system uses. The last two groups are the byproduct of
/// the same pin walk Approve runs ([`crate::compose::document_bodies`] and
/// [`crate::compose::document_unbound_slots`]): a generated candidate must be held to the same
/// pin/slot standard as the gate that follows it, or the generator's "this is fine" is weaker than
/// Approve's refusal and the corrector is never told which pin or slot to fix. Approve's remaining
/// refusals are not lint: a missing/stale/mismatched trajectory is absence of simulation evidence,
/// and no correction can invent one. `GraphCycle` is the one asymmetry in the other direction —
/// Approve exempts it for imported presets, while a generated candidate is not an imported preset.
///
/// Layers 2 and 3 run even when 1 already failed, so a corrector receives the whole round's
/// corrections instead of rediscovering the remaining layers one round later. The pin findings come
/// before the slot findings, Approve's order: an unavailable package still declares slots, and the
/// pin is the root cause.
fn lint(
    document: &WorkflowDocument,
    bodies: &[PatternBody<'_>],
    pin_diagnostics: &[ComposeDiagnostic],
    packages: &[PatternPackage],
) -> Round {
    let mut violations = Vec::new();
    if let Err(found) = validate(&[document], bodies) {
        violations.extend(
            found
                .iter()
                .map(|violation| structural_correction(violation, document)),
        );
    }
    violations.extend(pin_diagnostics.iter().map(compose_correction));
    violations.extend(
        crate::compose::document_unbound_slots(document, packages)
            .iter()
            .map(compose_correction),
    );
    let semantic = validate_workflow_ir(document, &BuiltinCapabilityCatalog);
    violations.extend(semantic.errors().map(diagnostic_correction));
    if let Err(refusal) = validate_ir_buildability(document, IrTemplateSource::OwnSource) {
        violations.push(buildability_correction(&refusal, document));
    }
    Round {
        violations,
        warnings: semantic.warnings().cloned().collect(),
    }
}

/// Ask the corrector for a revised document, carrying the previous document and every correction.
///
/// `shortcut:` a fresh cancellation token per call is uncancellable, so a round in flight cannot be
/// cancelled by a caller; upgrade when a coordinator owns generate runs and registers a token.
async fn correct(
    corrector: &dyn PromptExecutor,
    strategy_id: &str,
    round: usize,
    document: &WorkflowDocument,
    corrections: &[Correction],
) -> CoreResult<WorkflowDocument> {
    use std::fmt::Write as _;

    let encoded = serde_json::to_string_pretty(document).map_err(|error| CoreError::Internal {
        category: format!("generate prompt: {error}"),
    })?;
    let mut prompt = String::with_capacity(encoded.len() + 256);
    prompt.push_str(
        "Revise the workflow IR document below so it passes structural, semantic and \
         buildability validation. Reply with the complete document as JSON and nothing else.\n\n\
         Document:\n",
    );
    prompt.push_str(&encoded);
    prompt.push_str("\n\nCorrections from the previous round:\n");
    for correction in corrections {
        let _ = writeln!(
            prompt,
            "- {} at {}: {} -> {}",
            correction.kind, correction.location, correction.observed, correction.correction
        );
    }
    let failed = |message: String| CoreError::Coded {
        code: "corrector_failed".into(),
        message,
    };
    let result = corrector
        .execute(PromptRequest {
            run_id: format!("generate:{strategy_id}"),
            task_id: format!("generate.correct.{round}"),
            agent_ref: None,
            prompt,
            tool_policy: ToolPolicy::DenyAll,
            cancellation: tokio_util::sync::CancellationToken::new(),
        })
        .await
        .map_err(|error| failed(format!("round {round}: {error}")))?;
    let mut document = parse_document(&result.full_text).ok_or_else(|| {
        failed(format!(
            "round {round} returned no workflow document; a corrector response that does not \
             parse is an executor failure, not an exhaustion"
        ))
    })?;
    // A corrected document is still this generation's candidate. Identity is the kernel's:
    // provenance is this generation's origin (the drift guard asserts it), the hash is bound at
    // approval, and the IR and authored versions come from the contract. The field space stays the
    // request's (D8): assets, signals, roles and initial_action are not a generate request's to add.
    // `document.id`, `state` and `revision` are enforced by the CAS commit.
    document.content_hash = None;
    document.ir_version = IR_VERSION;
    document.version = 1;
    document.initial_action = None;
    document.assets.clear();
    document.signals.clear();
    document.roles.clear();
    document.provenance = generated_provenance();
    Ok(document)
}

/// The document a corrector returned, tolerating the code fence a model usually wraps JSON in.
fn parse_document(text: &str) -> Option<WorkflowDocument> {
    let trimmed = text.trim();
    let body = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .unwrap_or(trimmed);
    let body = body.strip_suffix("```").unwrap_or(body).trim();
    serde_json::from_str(body).ok()
}

/// Every node id the document declares, for a correction that must name the legal alternatives.
fn legal_node_ids(document: &WorkflowDocument) -> String {
    if document.nodes.is_empty() {
        return "this document declares no node, so declare the node it names".to_owned();
    }
    let ids: Vec<&str> = document.nodes.iter().map(|node| node.id.as_str()).collect();
    format!(
        "the legal node ids in `{}` are: {}",
        document.id.as_str(),
        ids.join(", ")
    )
}

/// The stable `snake_case` identifier of one structural violation.
const fn structural_kind(violation: &Violation) -> &'static str {
    match violation {
        Violation::DuplicateDocumentId(_) => "duplicate_document_id",
        Violation::DuplicateNodeId { .. } => "duplicate_node_id",
        Violation::DanglingEdge { .. } => "dangling_edge",
        Violation::DanglingDependency { .. } => "dangling_dependency",
        Violation::EmptyNestId { .. } => "empty_nest_id",
        Violation::DuplicateNestId { .. } => "duplicate_nest_id",
        Violation::UnknownNestParent { .. } => "unknown_nest_parent",
        Violation::NestParentNotComposable { .. } => "nest_parent_not_composable",
        Violation::IncompleteNestChild { .. } => "incomplete_nest_child",
        Violation::GraphCycle { .. } => "graph_cycle",
        Violation::NestCycle { .. } => "nest_cycle",
        Violation::NestDepthExceeded { .. } => "nest_depth_exceeded",
    }
}

/// One layer-1 violation as an actionable correction.
///
/// The enumerable alternatives are listed from the document under lint: a dangling reference names
/// the missing id and every legal node id, and a non-composable nest parent names the kind it found
/// and the two kinds a nest accepts.
#[allow(clippy::too_many_lines)] // one match arm per Violation variant: per-kind formatters, one table
fn structural_correction(violation: &Violation, document: &WorkflowDocument) -> Correction {
    let (location, observed, correction) = match violation {
        Violation::DuplicateDocumentId(id) => (
            format!("document {id}"),
            format!("document id `{id}` is declared more than once in the set"),
            format!(
                "keep exactly one document with id `{id}`: a generated candidate is the only \
                 document in the set"
            ),
        ),
        Violation::DuplicateNodeId { node, .. } => (
            format!("node {node}"),
            format!("node id `{node}` is declared more than once"),
            format!(
                "rename or remove all but one node with id `{node}`; {}",
                legal_node_ids(document)
            ),
        ),
        Violation::DanglingEdge { edge, node, .. } => (
            format!("edge {edge}"),
            format!("edge `{edge}` references unknown node `{node}`"),
            format!(
                "point edge `{edge}` at two declared nodes, or declare `{node}`; {}",
                legal_node_ids(document)
            ),
        ),
        Violation::DanglingDependency { node, dependency, .. } => (
            format!("node {node}"),
            format!("node `{node}` depends on unknown node `{dependency}`"),
            format!(
                "add node `{dependency}` to this document or drop it from `{node}`'s \
                 `depends_on`; {}",
                legal_node_ids(document)
            ),
        ),
        Violation::EmptyNestId { .. } => (
            format!("document {}", document.id.as_str()),
            "a nest declares an empty id".to_owned(),
            "give the nest a non-empty id: a nest is addressed by its id, so an unnamed one cannot \
             be bound, flattened or reported"
                .to_owned(),
        ),
        Violation::DuplicateNestId { nest, .. } => (
            format!("nest {nest}"),
            format!("nest id `{nest}` is declared more than once"),
            format!(
                "rename all but one nest with id `{nest}`; nest ids are the address compose \
                 operations read"
            ),
        ),
        Violation::UnknownNestParent { nest, parent, .. } => (
            format!("nest {nest}"),
            format!("nest `{nest}` names unknown parent node `{parent}`"),
            format!(
                "declare a `pattern_slot` or `nest_group` node with id `{parent}`, or point the \
                 nest at one of the declared nodes; {}",
                legal_node_ids(document)
            ),
        ),
        Violation::NestParentNotComposable {
            nest,
            parent,
            kind,
            ..
        } => (
            format!("nest {nest}"),
            format!("nest `{nest}` parent `{parent}` is `{kind}`"),
            format!(
                "change nest `{nest}`'s parent from `{parent}` (kind `{kind}`) to a node of kind \
                 `pattern_slot` or `nest_group`, the only two composable kinds"
            ),
        ),
        Violation::IncompleteNestChild { nest, .. } => (
            format!("nest {nest}"),
            format!("nest `{nest}`'s child omits the pin its kind requires"),
            format!(
                "give nest `{nest}` a complete child pin: `{{\"kind\": \"pattern\", \
                 \"registry_id\": …, \"version\": …}}` or `{{\"kind\": \"workflow\", \
                 \"workflow_id\": …}}`"
            ),
        ),
        Violation::GraphCycle { path, .. } => (
            format!("path {}", path.join(" -> ")),
            format!("the node graph is not a DAG: {}", path.join(" -> ")),
            format!(
                "remove the `depends_on` entry or edge that closes the cycle at `{}`; the node \
                 graph must stay a directed acyclic graph",
                path.join(" -> ")
            ),
        ),
        Violation::NestCycle { path } => (
            format!("path {}", path.join(" -> ")),
            format!("the nest references form a cycle: {}", path.join(" -> ")),
            format!(
                "break the nest cycle at `{}`: a nest must not reach itself through its pattern \
                 pins",
                path.join(" -> ")
            ),
        ),
        Violation::NestDepthExceeded { depth, .. } => (
            format!("document {}", document.id.as_str()),
            format!("nest depth {depth} exceeds the ceiling {MAX_NEST_DEPTH}"),
            format!(
                "remove one nest level on the path this document reaches {depth} through; the \
                 ceiling is {MAX_NEST_DEPTH} unexpanded levels"
            ),
        ),
    };
    Correction {
        kind: structural_kind(violation),
        location,
        observed,
        correction,
    }
}

/// One pin or slot finding from Approve's own walk as an actionable correction.
///
/// The three reachable kinds are exactly the three Approve refuses on; `DroppedSlotBinding` is
/// reachable only from a compose operation that drops a binding, never from a whole-document walk,
/// so it keeps the plain message for the one caller that can produce it.
fn compose_correction(diagnostic: &ComposeDiagnostic) -> Correction {
    let (kind, location, correction) = match diagnostic {
        ComposeDiagnostic::StalePin {
            nest_id,
            package,
            recorded_hash,
            scanned_hash,
        } => (
            "stale_pin",
            format!("nest {nest_id}"),
            format!(
                "re-pin `{nest_id}`'s child `content_hash` to `{scanned_hash}` (the bytes the \
                 registry serves for `{package}` now), or restore the pinned bytes so they hash to \
                 `{recorded_hash}`: Approve refuses a pin it cannot confirm"
            ),
        ),
        ComposeDiagnostic::PinUnavailable {
            nest_id,
            package,
            recorded_hash,
            ..
        } => (
            "pin_unavailable",
            format!("nest {nest_id}"),
            format!(
                "pin a `{}` version the registry resolves under the patterns root, or drop nest \
                 `{nest_id}`; it records content hash `{recorded_hash}`, and Approve refuses a pin \
                 with no bytes to confirm",
                package.id
            ),
        ),
        ComposeDiagnostic::UnboundRequiredSlot {
            nest_id,
            package,
            slot,
        } => (
            "unbound_required_slot",
            format!("nest {nest_id}"),
            format!(
                "bind slot `{slot}` on `{nest_id}`'s parent node (`bind.{slot}` must name a \
                 value), or remove the nest: pattern `{package}` declares it required, and \
                 Approve refuses an unbound required slot"
            ),
        ),
        ComposeDiagnostic::DroppedSlotBinding { nest_id, slot } => (
            "dropped_slot_binding",
            format!("nest {nest_id}"),
            format!(
                "drop `bind.{slot}` from `{nest_id}`'s parent node: the flattened fragment \
                 addresses no slot by that name, so the binding has nowhere to land"
            ),
        ),
    };
    Correction {
        kind,
        location,
        observed: diagnostic.to_string(),
        correction,
    }
}

/// The stable `snake_case` identifier of one semantic diagnostic category.
const fn category_kind(category: DiagnosticCategory) -> &'static str {
    match category {
        DiagnosticCategory::Structural => "structural",
        DiagnosticCategory::Reachability => "reachability",
        DiagnosticCategory::TerminalConsistency => "terminal_consistency",
        DiagnosticCategory::IdMismatch => "id_mismatch",
        DiagnosticCategory::OrphanInnerGraph => "orphan_inner_graph",
        DiagnosticCategory::MissingAsset => "missing_asset",
        DiagnosticCategory::PathSafety => "path_safety",
        DiagnosticCategory::MissingCapability => "missing_capability",
        DiagnosticCategory::CapabilityArgDrift => "capability_arg_drift",
        DiagnosticCategory::SchemaCheckSkipped => "schema_check_skipped",
        DiagnosticCategory::RunIntents => "run_intents",
        DiagnosticCategory::DuplicateLabel => "duplicate_label",
        DiagnosticCategory::MergeIntegrity => "merge_integrity",
    }
}

/// One layer-2 error as an actionable correction.
///
/// The diagnostic carries a dot path and a message, not the offending value set, so each category's
/// sentence names the change and its constraint and leaves the concrete value to `observed`.
fn diagnostic_correction(diagnostic: &ValidationDiagnostic) -> Correction {
    Correction {
        kind: category_kind(diagnostic.category),
        location: diagnostic.path.clone(),
        observed: diagnostic.message.clone(),
        correction: category_correction(diagnostic.category, &diagnostic.path),
    }
}

/// What to change at `path` for one semantic category.
fn category_correction(category: DiagnosticCategory, path: &str) -> String {
    match category {
        DiagnosticCategory::Structural => format!(
            "rewrite `{path}` so it names a field this document declares; an unresolved reference \
             makes the candidate unloadable"
        ),
        DiagnosticCategory::Reachability => format!(
            "wire an edge path from `entry` to the declared terminal state, then re-check `{path}`"
        ),
        DiagnosticCategory::TerminalConsistency => format!(
            "make `{path}` agree with `terminal`: exactly one node of kind `terminal`, and the \
             document's `terminal` names it"
        ),
        DiagnosticCategory::IdMismatch => format!(
            "set the document id to the strategy this candidate is generated under; `{path}` \
             reports the pair that disagrees"
        ),
        DiagnosticCategory::OrphanInnerGraph => format!(
            "reference the inner graph `{path}` names from a state's `enter` action, or remove the \
             inner-step nodes that define it"
        ),
        DiagnosticCategory::MissingAsset => format!(
            "declare the asset `{path}` names in `assets` and ship it in the bundle, or remove the \
             reference"
        ),
        DiagnosticCategory::PathSafety => format!(
            "point `{path}` at a path inside the strategy's own directory: a reference that \
             escapes it is refused"
        ),
        DiagnosticCategory::MissingCapability => format!(
            "replace the capability `{path}` names with one the capability catalog registers, or \
             register it"
        ),
        DiagnosticCategory::CapabilityArgDrift => {
            format!("change the arguments at `{path}` to the capability's declared input schema")
        }
        DiagnosticCategory::SchemaCheckSkipped => format!(
            "restate the capability's input schema as valid JSON so `{path}` can be checked"
        ),
        DiagnosticCategory::RunIntents => format!(
            "set `meta.run_intents` so a `system`-kind strategy declares `system_maintenance`; \
             `{path}` names the mismatch"
        ),
        DiagnosticCategory::DuplicateLabel => format!(
            "give the labeled edges leaving one state distinct labels; `{path}` names the duplicate"
        ),
        DiagnosticCategory::MergeIntegrity => format!(
            "declare `merge`, `converge`, `timeout_ms` and `on_timeout` together on the join state \
             `{path}` names"
        ),
    }
}

/// One layer-3 refusal as an actionable correction.
fn buildability_correction(error: &IrOuterGraphError, document: &WorkflowDocument) -> Correction {
    let document_id = document.id.as_str();
    let (kind, location, observed, correction) = match error {
        IrOuterGraphError::Unmappable { concept, location } => (
            "unmappable",
            location.clone(),
            format!("`{concept}` has no §8.2.1 execution row"),
            format!(
                "replace `{concept}` at `{location}` with a concrete executable node: a \
                 composition concept expands into `outer_state` or `terminal` nodes before apply, \
                 so an applied document cannot carry one"
            ),
        ),
        IrOuterGraphError::Unprojected {
            concept,
            location,
            remedy,
        } => (
            "unprojected",
            location.clone(),
            format!("`{concept}` has no preset projection and none is planned"),
            remedy.clone(),
        ),
        IrOuterGraphError::UnresolvedTemplate { node, template, .. } => (
            "unresolved_template",
            format!("node {node}"),
            template.as_ref().map_or_else(
                || "the node declares a template the source cannot resolve".to_owned(),
                |template| format!("template `{template}` resolves to nothing"),
            ),
            format!(
                "author `{node}`'s instructions inline (an `enter` action's prompt text) instead \
                 of a `template` path: a generated candidate carries no bundle root, so no \
                 template can resolve"
            ),
        ),
        IrOuterGraphError::Projection(error) => (
            "projection",
            format!("document {document_id}"),
            error.to_string(),
            projection_correction(error),
        ),
        IrOuterGraphError::Graph(error) => (
            "graph",
            format!("document {document_id}"),
            error.to_string(),
            "point `entry` and every edge endpoint at nodes this document declares, and keep one \
             node per id: the builder refused the projected topology"
                .to_owned(),
        ),
    };
    Correction {
        kind,
        location,
        observed,
        correction,
    }
}

/// What to change when the document cannot be projected onto a preset at all.
fn projection_correction(error: &WorkflowIrError) -> String {
    match error {
        WorkflowIrError::AssetRead { path, .. } => format!(
            "reference an asset the document declares and its bundle holds; `{path}` could not be \
             read"
        ),
        WorkflowIrError::NotProjectable { reason } => format!(
            "rewrite what `{reason}` reports so it has a preset projection: an applied document is \
             projected onto preset carriers before it builds"
        ),
        WorkflowIrError::MultipleTerminalStates { count } => format!(
            "leave exactly one node of kind `terminal` and one entry in `terminal`; {count} nodes \
             are marked terminal"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_contracts::generated::daemon_api::orchestration::workflow_document::{
        WorkflowEdgeKind, WorkflowExitWhen, WorkflowExitWhenKind, WorkflowNodeEffectClass,
        WorkflowNodeKind,
    };
    use nexus_preset::validation::DiagnosticSeverity;
    use serde_json::json;

    fn document(nodes: &serde_json::Value, edges: &serde_json::Value) -> WorkflowDocument {
        serde_json::from_value(json!({
            "ir_version": 0,
            "id": "generated",
            "version": 1,
            "revision": 0,
            "state": "candidate",
            "meta": {
                "description": "generate fixture",
                "kind": "creator",
                "run_intents": ["work_init"],
                "requires_capabilities": []
            },
            "entry": "start",
            "terminal": ["done"],
            "nodes": nodes,
            "edges": edges,
            "nests": [],
            "assets": [],
            "signals": [],
            "roles": [],
            "effect_summary": "pure",
            "provenance": { "source": "generate" }
        }))
        .expect("fixture is a valid workflow document")
    }

    fn node(id: &str, kind: &str) -> serde_json::Value {
        json!({ "id": id, "kind": kind, "effect_class": "pure" })
    }

    fn legal_document() -> WorkflowDocument {
        document(
            &json!([node("start", "outer_state"), node("done", "terminal")]),
            &json!([{ "id": "start->done", "from": "start", "to": "done", "kind": "linear" }]),
        )
    }

    /// Every structural kind, with what its correction must name.
    fn structural_cases() -> Vec<(Violation, &'static str, &'static str)> {
        vec![
            (
                Violation::DuplicateDocumentId("generated".into()),
                "duplicate_document_id",
                "generated",
            ),
            (
                Violation::DuplicateNodeId {
                    document: "generated".into(),
                    node: "start".into(),
                },
                "duplicate_node_id",
                "start",
            ),
            (
                Violation::DanglingEdge {
                    document: "generated".into(),
                    edge: "start->ghost".into(),
                    node: "ghost".into(),
                },
                "dangling_edge",
                "ghost",
            ),
            (
                Violation::DanglingDependency {
                    document: "generated".into(),
                    node: "start".into(),
                    dependency: "ghost".into(),
                },
                "dangling_dependency",
                "ghost",
            ),
            (
                Violation::EmptyNestId {
                    document: "generated".into(),
                },
                "empty_nest_id",
                "non-empty id",
            ),
            (
                Violation::DuplicateNestId {
                    document: "generated".into(),
                    nest: "n1".into(),
                },
                "duplicate_nest_id",
                "n1",
            ),
            (
                Violation::UnknownNestParent {
                    document: "generated".into(),
                    nest: "n1".into(),
                    parent: "ghost".into(),
                },
                "unknown_nest_parent",
                "ghost",
            ),
            (
                Violation::NestParentNotComposable {
                    document: "generated".into(),
                    nest: "n1".into(),
                    parent: "start".into(),
                    kind: "outer_state".into(),
                },
                "nest_parent_not_composable",
                "outer_state",
            ),
            (
                Violation::IncompleteNestChild {
                    document: "generated".into(),
                    nest: "n1".into(),
                },
                "incomplete_nest_child",
                "n1",
            ),
            (
                Violation::GraphCycle {
                    document: "generated".into(),
                    path: vec!["a".into(), "b".into(), "a".into()],
                },
                "graph_cycle",
                "a -> b -> a",
            ),
            (
                Violation::NestCycle {
                    path: vec!["p@1".into(), "q@1".into(), "p@1".into()],
                },
                "nest_cycle",
                "p@1 -> q@1 -> p@1",
            ),
            (
                Violation::NestDepthExceeded {
                    document: "generated".into(),
                    depth: 4,
                },
                "nest_depth_exceeded",
                "4",
            ),
        ]
    }

    #[test]
    fn every_structural_kind_has_a_correction_naming_what_to_change() {
        let document = legal_document();
        let cases = structural_cases();
        assert_eq!(cases.len(), 12, "the enum carries twelve variants");
        for (violation, kind, expected) in cases {
            let correction = structural_correction(&violation, &document);
            assert_eq!(correction.kind, kind, "{violation:?}");
            assert!(
                correction.correction.contains(expected),
                "{kind}: `{}` must name `{expected}`",
                correction.correction
            );
            assert!(
                !correction.correction.is_empty() && correction.correction != correction.observed,
                "{kind}: a bare Display string is not a correction"
            );
        }
    }

    #[test]
    fn a_dangling_dependency_names_the_missing_id_and_the_legal_ids() {
        let document = legal_document();
        let correction = structural_correction(
            &Violation::DanglingDependency {
                document: "generated".into(),
                node: "start".into(),
                dependency: "ghost".into(),
            },
            &document,
        );
        assert_eq!(correction.kind, "dangling_dependency");
        assert_eq!(correction.location, "node start");
        assert!(correction.observed.contains("ghost"));
        for named in ["ghost", "start", "done"] {
            assert!(
                correction.correction.contains(named),
                "the correction must name `{named}`: {}",
                correction.correction
            );
        }
    }

    #[test]
    fn a_non_composable_parent_names_its_kind_and_both_accepted_kinds() {
        let correction = structural_correction(
            &Violation::NestParentNotComposable {
                document: "generated".into(),
                nest: "n1".into(),
                parent: "start".into(),
                kind: "outer_state".into(),
            },
            &legal_document(),
        );
        for named in ["outer_state", "pattern_slot", "nest_group"] {
            assert!(
                correction.correction.contains(named),
                "the correction must name `{named}`: {}",
                correction.correction
            );
        }
    }

    #[test]
    fn every_semantic_category_has_a_correction_naming_what_to_change() {
        let cases = [
            (DiagnosticCategory::Structural, "structural"),
            (DiagnosticCategory::Reachability, "reachability"),
            (
                DiagnosticCategory::TerminalConsistency,
                "terminal_consistency",
            ),
            (DiagnosticCategory::IdMismatch, "id_mismatch"),
            (DiagnosticCategory::OrphanInnerGraph, "orphan_inner_graph"),
            (DiagnosticCategory::MissingAsset, "missing_asset"),
            (DiagnosticCategory::PathSafety, "path_safety"),
            (DiagnosticCategory::MissingCapability, "missing_capability"),
            (
                DiagnosticCategory::CapabilityArgDrift,
                "capability_arg_drift",
            ),
            (
                DiagnosticCategory::SchemaCheckSkipped,
                "schema_check_skipped",
            ),
            (DiagnosticCategory::RunIntents, "run_intents"),
            (DiagnosticCategory::DuplicateLabel, "duplicate_label"),
            (DiagnosticCategory::MergeIntegrity, "merge_integrity"),
        ];
        assert_eq!(
            cases.len(),
            13,
            "the category enum carries thirteen variants"
        );
        for (category, kind) in cases {
            let diagnostic = ValidationDiagnostic {
                path: "states[0].next".to_owned(),
                message: "observed problem".to_owned(),
                severity: DiagnosticSeverity::Error,
                category,
            };
            let correction = diagnostic_correction(&diagnostic);
            assert_eq!(correction.kind, kind);
            assert_eq!(correction.location, "states[0].next");
            assert!(
                correction.correction.contains("states[0].next"),
                "{kind}: the correction must name where to change"
            );
            assert_ne!(correction.correction, correction.observed);
        }
    }

    #[test]
    fn every_buildability_refusal_kind_has_a_correction_naming_what_to_change() {
        let mut unfit = legal_document();
        unfit.nodes.push(WorkflowNode {
            id: "slot".to_owned(),
            kind: WorkflowNodeKind::PatternSlot,
            effect_class: WorkflowNodeEffectClass::Pure,
            ..serde_json::from_value(node("slot", "pattern_slot")).unwrap()
        });
        let cases = [
            (unfit, "unmappable", "executable node"),
            (
                {
                    let mut document = legal_document();
                    document.nodes.push(WorkflowNode {
                        id: "judge".to_owned(),
                        kind: WorkflowNodeKind::Judge,
                        effect_class: WorkflowNodeEffectClass::Pure,
                        ..serde_json::from_value(node("judge", "judge")).unwrap()
                    });
                    document
                },
                "unprojected",
                "outer_state",
            ),
            (
                {
                    let mut document = legal_document();
                    document.nodes[0].exit_when = Some(WorkflowExitWhen {
                        kind: WorkflowExitWhenKind::LlmJudge,
                        duration: None,
                        judge_capability: None,
                        min_interval: None,
                        template: Some("prompts/judge.md".to_owned()),
                    });
                    document
                },
                "unresolved_template",
                "inline",
            ),
            (
                {
                    let mut document = legal_document();
                    document.nodes.push(WorkflowNode {
                        id: "plain".to_owned(),
                        kind: WorkflowNodeKind::InnerStep,
                        effect_class: WorkflowNodeEffectClass::Pure,
                        ..serde_json::from_value(node("plain", "inner_step")).unwrap()
                    });
                    document
                },
                "projection",
                "preset projection",
            ),
            (
                {
                    let mut document = legal_document();
                    document.entry = "ghost".to_owned();
                    document
                },
                "graph",
                "`entry`",
            ),
        ];
        for (document, kind, expected) in cases {
            let error = validate_ir_buildability(&document, IrTemplateSource::OwnSource)
                .expect_err("the fixture must be refused");
            let correction = buildability_correction(&error, &document);
            assert_eq!(correction.kind, kind, "{error}");
            assert!(
                correction.correction.contains(expected),
                "{kind}: `{}` must name `{expected}`",
                correction.correction
            );
        }
    }

    #[test]
    fn the_legal_fixture_passes_every_layer() {
        let round = lint(&legal_document(), &[], &[], &[]);
        assert!(
            round.violations.is_empty(),
            "{:?}",
            round
                .violations
                .iter()
                .map(|correction| (correction.kind, correction.observed.clone()))
                .collect::<Vec<_>>()
        );
    }

    /// A document pinning a package the registry does not serve. Before `lint` read the pin
    /// findings, this round still failed buildability (a `pattern_slot` is unmappable), but nothing
    /// named the pin the registry cannot confirm — the one finding Approve refuses on.
    #[test]
    fn an_unresolvable_pin_is_a_correction_naming_the_pin() {
        let document: WorkflowDocument = serde_json::from_value(json!({
            "ir_version": 0,
            "id": "generated",
            "version": 1,
            "revision": 0,
            "state": "candidate",
            "meta": {
                "description": "generate fixture",
                "kind": "creator",
                "run_intents": ["work_init"],
                "requires_capabilities": []
            },
            "entry": "slot",
            "terminal": ["done"],
            "nodes": [node("slot", "pattern_slot"), node("done", "terminal")],
            "edges": [{ "id": "slot->done", "from": "slot", "to": "done", "kind": "linear" }],
            "nests": [{
                "id": "ghost-nest",
                "parent_node": "slot",
                "expansion": "expand_in_candidate",
                "pin_policy": "exact",
                "child": {
                    "kind": "pattern",
                    "registry_id": "ghost",
                    "version": "1.0.0",
                    "content_hash": "0".repeat(64)
                }
            }],
            "assets": [],
            "signals": [],
            "roles": [],
            "effect_summary": "pure",
            "provenance": { "source": "generate" }
        }))
        .expect("fixture is a valid workflow document");

        let (bodies, pin_diagnostics) = crate::compose::document_bodies(&document, &[]);
        let round = lint(&document, &bodies, &pin_diagnostics, &[]);
        let pin = round
            .violations
            .iter()
            .find(|correction| correction.kind == "pin_unavailable")
            .unwrap_or_else(|| {
                panic!(
                    "the round must name the pin Approve refuses: {:?}",
                    round.violations
                )
            });
        assert_eq!(pin.location, "nest ghost-nest");
        for named in ["ghost@1.0.0", "ghost-nest"] {
            assert!(
                pin.observed.contains(named) || pin.correction.contains(named),
                "the pin correction must name `{named}`: {pin:?}"
            );
        }
    }

    #[test]
    fn materialization_carries_the_request_and_invents_nothing() {
        let request: GenerateRequest = serde_json::from_value(json!({
            "entry": "start",
            "terminal": ["done"],
            "nodes": [node("start", "outer_state"), node("done", "terminal")],
            "edges": [{
                "id": "start->done", "from": "start", "to": "done",
                "kind": WorkflowEdgeKind::Linear.to_string()
            }],
            "nests": [],
            "meta": {
                "description": "generate fixture",
                "kind": "creator",
                "run_intents": [],
                "requires_capabilities": []
            }
        }))
        .unwrap();
        let document = materialize(request, "generated").unwrap();

        assert_eq!(document.id.as_str(), "generated");
        assert_eq!(document.state, WorkflowDocumentState::Candidate);
        assert_eq!(
            document.provenance.source,
            WorkflowProvenanceSource::Generate
        );
        assert_eq!(document.nodes.len(), 2);
        assert_eq!(document.edges.len(), 1);
        assert!(document.nests.is_empty() && document.assets.is_empty());
        assert!(document.content_hash.is_none());
    }

    #[test]
    fn a_corrector_response_wrapped_in_a_code_fence_still_parses() {
        let encoded = serde_json::to_string_pretty(&legal_document()).unwrap();
        assert!(parse_document(&encoded).is_some());
        assert!(parse_document(&format!("```json\n{encoded}\n```")).is_some());
        assert!(parse_document("not a document").is_none());
    }
}
