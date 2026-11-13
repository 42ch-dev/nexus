//! Compose: the four operations a creator applies to one candidate strategy document.
//!
//! Plan: `.mstar/plans/v239-p3-compose-ops.md` §Compose wire contract (Q2) / §Nest policy (D19).
//!
//! Every operation is one `apply` closure for
//! [`CoreService::commit_strategy_candidate`](crate::CoreService::commit_strategy_candidate), so it
//! runs at the base revision the client read and the store bumps the revision exactly once. A
//! refusal returns before the write: the row stays byte-identical and no revision is consumed.
//!
//! `nest` inserts one collapsed `pattern_slot` node carrying the exact pin plus one
//! `WorkflowNestBinding`; the daemon resolves `{id, version}` against `nexus-pattern` and records
//! the fragment's current `content_hash`, so a package cannot change under an unchanged version.
//! `unnest` removes exactly what `nest` inserted, and the document is byte-equal to the one before
//! the matching nest when nothing else was edited. `replace_subtree` re-pins the region in place
//! and leaves every other node, edge, binding and nest untouched. `bind_slot` writes one slot
//! binding on the slot node and validates the value against the manifest's slot declaration; an
//! unbound required slot stays a named diagnostic, never a default.
//!
//! Nest policy and effect rollup are the shared `nexus-workflow-ir` walk and lattice: this module
//! resolves pins into [`PatternBody`]s and never re-derives depth, cycles or the enum order.

use std::collections::{HashMap, HashSet};

use nexus_contracts::generated::daemon_api::orchestration::workflow_document::{
    WorkflowAssetRef, WorkflowNestBinding, WorkflowNestBindingExpansion,
    WorkflowNestBindingPinPolicy, WorkflowNestChild, WorkflowNestChildKind, WorkflowNode,
    WorkflowNodeEffectClass, WorkflowNodeKind, WorkflowPatternRef,
};
use nexus_contracts::generated::daemon_api::patterns::pattern_manifest::{
    NexusPatternSlotKind, PatternManifest, PatternManifestEffectClass,
};
use nexus_pattern::PatternPackage;
use nexus_workflow_ir::{
    document_summary, validate, PatternBody, PatternBodyKind, Violation, WorkflowDocument,
    IR_VERSION,
};
use serde_json::Value;

use crate::{CoreError, CoreResult, CoreService, Principal, StrategyCandidate};

/// One library package a creator asked to compose: an exact `{id, version}` reference. The daemon
/// resolves the content hash, so the caller never supplies one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatternPackageRef {
    /// Registry id, which is also the package directory name.
    pub id: String,
    /// Exact version label from the package manifest.
    pub version: String,
}

impl std::fmt::Display for PatternPackageRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}@{}", self.id, self.version)
    }
}

/// `nest`: pin a library package into a new collapsed slot under `parent_node`.
#[derive(Debug, Clone)]
pub struct NestOp {
    /// Stable nest id; the region address every later operation uses.
    pub nest_id: String,
    /// Id of the collapsed slot node this operation inserts.
    pub parent_node: String,
    /// Exact package to pin.
    pub package: PatternPackageRef,
    /// When the region expands into concrete nodes and edges.
    pub expansion: WorkflowNestBindingExpansion,
}

/// `unnest`: remove the collapsed slot at `nest_id`.
#[derive(Debug, Clone)]
pub struct UnnestOp {
    /// Nest id of the region to remove.
    pub nest_id: String,
}

/// `replace_subtree`: re-pin the region at `nest_id` to another exact package.
#[derive(Debug, Clone)]
pub struct ReplaceSubtreeOp {
    /// Nest id of the region to re-pin.
    pub nest_id: String,
    /// Exact replacement package.
    pub package: PatternPackageRef,
    /// Expansion recorded on the re-pinned region.
    pub expansion: WorkflowNestBindingExpansion,
}

/// `bind_slot`: bind one slot of the pattern pinned at `nest_id`.
#[derive(Debug, Clone)]
pub struct BindSlotOp {
    /// Nest id whose pattern declares the slot.
    pub nest_id: String,
    /// Slot name from the package manifest.
    pub slot: String,
    /// Bound value; its shape is validated against the manifest's slot kind.
    pub value: Value,
}

/// One compose operation: exactly one per request, never a multi-operation blob.
#[derive(Debug, Clone)]
pub enum ComposeOp {
    /// Add a library pattern as a collapsed slot.
    Nest(NestOp),
    /// Flatten a slot out of the candidate.
    Unnest(UnnestOp),
    /// Swap the nested region at a nest id.
    ReplaceSubtree(ReplaceSubtreeOp),
    /// Set one slot binding.
    BindSlot(BindSlotOp),
}

impl ComposeOp {
    /// The nest this operation addresses, named by every refusal.
    #[must_use]
    pub fn nest_id(&self) -> &str {
        match self {
            Self::Nest(op) => &op.nest_id,
            Self::Unnest(op) => &op.nest_id,
            Self::ReplaceSubtree(op) => &op.nest_id,
            Self::BindSlot(op) => &op.nest_id,
        }
    }
}

/// Why a compose operation was refused. Nothing was written when one is returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComposeRefusal {
    /// The registry has no available package for the requested exact pin.
    PackageUnavailable {
        /// The refused operation's nest id.
        nest_id: String,
        /// The requested pin.
        package: PatternPackageRef,
        /// The reader's reason, or that no directory declares the pin.
        reason: String,
    },
    /// No nest in this candidate carries the requested id.
    UnknownNest {
        /// The requested nest id.
        nest_id: String,
    },
    /// A nest with this id already exists; a compose op never forks a second region.
    DuplicateNest {
        /// The requested nest id.
        nest_id: String,
    },
    /// The slot node the nest would insert already exists.
    OccupiedParent {
        /// The requested nest id.
        nest_id: String,
        /// The node id that is already taken.
        parent_node: String,
    },
    /// The write would close a nest-reference cycle.
    NestCycle {
        /// The requested nest id.
        nest_id: String,
        /// The cycle path, oldest body first.
        path: Vec<String>,
    },
    /// The write would exceed the shared nest depth bound.
    NestTooDeep {
        /// The requested nest id.
        nest_id: String,
        /// The depth the write would reach.
        depth: usize,
    },
    /// The write would leave a structural violation the operation did not cause.
    InvalidDocument {
        /// The requested nest id.
        nest_id: String,
        /// Every violation, rendered.
        violations: Vec<String>,
    },
    /// The pinned package declares no such slot.
    SlotNotDeclared {
        /// The requested nest id.
        nest_id: String,
        /// The pattern the nest pins, as `id@version`.
        pattern: String,
        /// The slot the request named.
        slot: String,
    },
    /// The bound value does not match the manifest's slot kind.
    SlotKindMismatch {
        /// The requested nest id.
        nest_id: String,
        /// The slot the request named.
        slot: String,
        /// What the slot declaration accepts.
        expected: String,
    },
    /// The slot node is still referenced by the graph, so removing it would dangle.
    WiredSlot {
        /// The requested nest id.
        nest_id: String,
        /// The slot node that is still referenced.
        parent_node: String,
        /// What still references it.
        referrers: Vec<String>,
    },
}

impl ComposeRefusal {
    /// The stable lowercase reason a client branches on.
    #[must_use]
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::PackageUnavailable { .. } => "package_unavailable",
            Self::UnknownNest { .. } => "unknown_nest",
            Self::DuplicateNest { .. } => "duplicate_nest",
            Self::OccupiedParent { .. } => "occupied_parent",
            Self::NestCycle { .. } => "nest_cycle",
            Self::NestTooDeep { .. } => "nest_depth_exceeded",
            Self::InvalidDocument { .. } => "invalid_document",
            Self::SlotNotDeclared { .. } => "slot_not_declared",
            Self::SlotKindMismatch { .. } => "slot_kind_mismatch",
            Self::WiredSlot { .. } => "slot_still_wired",
        }
    }

    fn nest_id(&self) -> &str {
        match self {
            Self::PackageUnavailable { nest_id, .. }
            | Self::UnknownNest { nest_id }
            | Self::DuplicateNest { nest_id }
            | Self::OccupiedParent { nest_id, .. }
            | Self::NestCycle { nest_id, .. }
            | Self::NestTooDeep { nest_id, .. }
            | Self::InvalidDocument { nest_id, .. }
            | Self::SlotNotDeclared { nest_id, .. }
            | Self::SlotKindMismatch { nest_id, .. }
            | Self::WiredSlot { nest_id, .. } => nest_id,
        }
    }

    /// The non-mutating validation envelope's `details`, carrying the offending nest id and the
    /// cycle path or depth the shared walk reported.
    #[must_use]
    pub fn details(&self) -> Value {
        let mut details = serde_json::Map::new();
        details.insert("code".to_string(), Value::from("compose_refused"));
        details.insert("reason".to_string(), Value::from(self.reason()));
        details.insert("nest_id".to_string(), Value::from(self.nest_id()));
        details.insert("message".to_string(), Value::from(self.to_string()));
        match self {
            Self::PackageUnavailable { package, .. } => {
                details.insert("package_id".to_string(), Value::from(package.id.clone()));
                details.insert(
                    "package_version".to_string(),
                    Value::from(package.version.clone()),
                );
            }
            Self::OccupiedParent { parent_node, .. } => {
                details.insert("parent_node".to_string(), Value::from(parent_node.clone()));
            }
            Self::WiredSlot {
                parent_node,
                referrers,
                ..
            } => {
                details.insert("parent_node".to_string(), Value::from(parent_node.clone()));
                details.insert("referrers".to_string(), Value::from(referrers.clone()));
            }
            Self::NestCycle { path, .. } => {
                details.insert("path".to_string(), Value::from(path.clone()));
            }
            Self::NestTooDeep { depth, .. } => {
                details.insert("depth".to_string(), Value::from(*depth));
            }
            Self::InvalidDocument { violations, .. } => {
                details.insert("violations".to_string(), Value::from(violations.clone()));
            }
            Self::SlotNotDeclared { pattern, slot, .. } => {
                details.insert("pattern".to_string(), Value::from(pattern.clone()));
                details.insert("slot".to_string(), Value::from(slot.clone()));
            }
            Self::SlotKindMismatch { slot, expected, .. } => {
                details.insert("slot".to_string(), Value::from(slot.clone()));
                details.insert("expected".to_string(), Value::from(expected.clone()));
            }
            Self::UnknownNest { .. } | Self::DuplicateNest { .. } => {}
        }
        Value::Object(details)
    }

    /// The core error a refused operation returns; the adapter renders `details` verbatim.
    #[must_use]
    pub fn into_error(self) -> CoreError {
        CoreError::InputValidation {
            details: self.details(),
        }
    }
}

impl std::fmt::Display for ComposeRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::PackageUnavailable {
                nest_id,
                package,
                reason,
            } => write!(
                f,
                "nest `{nest_id}` cannot use package `{package}`: {reason}"
            ),
            Self::UnknownNest { nest_id } => {
                write!(f, "nest `{nest_id}` is not in this candidate")
            }
            Self::DuplicateNest { nest_id } => {
                write!(f, "nest `{nest_id}` already exists in this candidate")
            }
            Self::OccupiedParent {
                nest_id,
                parent_node,
            } => write!(
                f,
                "nest `{nest_id}` cannot insert slot `{parent_node}`: a node with that id exists"
            ),
            Self::NestCycle { nest_id, path } => write!(
                f,
                "nest `{nest_id}` would create a nest cycle: {}",
                path.join(" -> ")
            ),
            Self::NestTooDeep { nest_id, depth } => write!(
                f,
                "nest `{nest_id}` would nest {depth} levels deep; the bound is {}",
                nexus_workflow_ir::MAX_NEST_DEPTH
            ),
            Self::InvalidDocument {
                nest_id,
                violations,
            } => write!(
                f,
                "nest `{nest_id}` would leave the document invalid: {}",
                violations.join("; ")
            ),
            Self::SlotNotDeclared {
                nest_id,
                pattern,
                slot,
            } => write!(
                f,
                "pattern `{pattern}` of nest `{nest_id}` declares no slot `{slot}`"
            ),
            Self::SlotKindMismatch {
                nest_id,
                slot,
                expected,
            } => write!(f, "slot `{slot}` of nest `{nest_id}` expects {expected}"),
            Self::WiredSlot {
                nest_id,
                parent_node,
                referrers,
            } => write!(
                f,
                "nest `{nest_id}` slot `{parent_node}` is still referenced by {}",
                referrers.join(", ")
            ),
        }
    }
}

/// A committed candidate plus the diagnostics its content carries.
#[derive(Debug, Clone)]
pub struct ComposeOutcome {
    /// The committed candidate at the next revision.
    pub candidate: StrategyCandidate,
    /// Non-blocking findings about the committed document. Empty is the normal case.
    pub diagnostics: Vec<ComposeDiagnostic>,
}

/// One non-blocking finding. A diagnostic is reported, never defaulted away.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComposeDiagnostic {
    /// A required slot of a pinned package has no binding, so the candidate cannot simulate to
    /// success until it is bound.
    UnboundRequiredSlot {
        /// The nest that pins the package.
        nest_id: String,
        /// The pinned package.
        package: PatternPackageRef,
        /// The unbound slot name.
        slot: String,
    },
}

impl std::fmt::Display for ComposeDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnboundRequiredSlot {
                nest_id,
                package,
                slot,
            } => write!(
                f,
                "nest `{nest_id}`: pattern `{package}` requires slot `{slot}`, which is unbound"
            ),
        }
    }
}

impl CoreService {
    /// Apply one compose operation to the candidate at `base_revision`.
    ///
    /// The operation runs at the base the client read and the store commits it as the next
    /// revision; a stale base is the store's typed conflict. A refusal returns before the write, so
    /// the row and its revision are untouched.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InputValidation`] carrying a [`ComposeRefusal::details`] envelope for a
    /// refused operation, [`CoreError::StrategyCandidateConflict`] for a stale base, and the
    /// principal/store errors otherwise.
    pub async fn compose_strategy_candidate(
        &self,
        principal: &Principal,
        strategy_id: &str,
        base_revision: u64,
        op: ComposeOp,
    ) -> CoreResult<ComposeOutcome> {
        self.verify_principal(principal)?;
        let packages = self.compose_packages()?;
        let index = PackageIndex::new(&packages);
        let candidate = self
            .commit_strategy_candidate(principal, strategy_id, base_revision, |document| {
                apply_compose(document, &op, &index).map_err(ComposeRefusal::into_error)
            })
            .await?;
        let diagnostics = unbound_required_slots(&candidate.document, &index);
        Ok(ComposeOutcome {
            candidate,
            diagnostics,
        })
    }

    /// Every package under `<nexus_home>/patterns`, unavailable ones included: compose resolves
    /// pins against the same registry the listing serves.
    fn compose_packages(&self) -> CoreResult<Vec<PatternPackage>> {
        let patterns_dir = self.inner.nexus_home.join("patterns");
        nexus_pattern::scan_pattern_packages(&patterns_dir, IR_VERSION).map_err(|error| {
            CoreError::Internal {
                category: format!("pattern registry {}: {error}", patterns_dir.display()),
            }
        })
    }
}

/// Apply one operation to a copy of `document`, then hold the result to the shared nest rules and
/// the effect rollup.
fn apply_compose(
    document: &WorkflowDocument,
    op: &ComposeOp,
    packages: &PackageIndex<'_>,
) -> Result<WorkflowDocument, ComposeRefusal> {
    let mut next = document.clone();
    match op {
        ComposeOp::Nest(payload) => nest_pattern(&mut next, payload, packages)?,
        ComposeOp::Unnest(payload) => unnest_slot(&mut next, payload)?,
        ComposeOp::ReplaceSubtree(payload) => replace_subtree(&mut next, payload, packages)?,
        ComposeOp::BindSlot(payload) => bind_slot(&mut next, payload, packages)?,
    }
    settle(&mut next, op.nest_id(), packages)?;
    Ok(next)
}

/// Recompute the nest policy and the effect rollup on every operation.
///
/// `GraphCycle` is not compose policy: outer-state loops are legal for imported presets, which is
/// the exception the CLI's structural report already makes. Every other violation is a defect this
/// write must not preserve silently.
fn settle(
    document: &mut WorkflowDocument,
    nest_id: &str,
    packages: &PackageIndex<'_>,
) -> Result<(), ComposeRefusal> {
    let bodies = packages.bodies(document);
    let violations: Vec<Violation> = validate(&[document], &bodies)
        .err()
        .unwrap_or_default()
        .into_iter()
        .filter(|violation| !matches!(violation, Violation::GraphCycle { .. }))
        .collect();

    if let Some(path) = violations.iter().find_map(|violation| match violation {
        Violation::NestCycle { path } => Some(path.clone()),
        _ => None,
    }) {
        return Err(ComposeRefusal::NestCycle {
            nest_id: nest_id.to_owned(),
            path,
        });
    }
    if let Some(depth) = violations.iter().find_map(|violation| match violation {
        Violation::NestDepthExceeded { depth, .. } => Some(*depth),
        _ => None,
    }) {
        return Err(ComposeRefusal::NestTooDeep {
            nest_id: nest_id.to_owned(),
            depth,
        });
    }
    if !violations.is_empty() {
        return Err(ComposeRefusal::InvalidDocument {
            nest_id: nest_id.to_owned(),
            violations: violations.iter().map(ToString::to_string).collect(),
        });
    }

    let summary = document_summary(document, &bodies);
    document.effect_summary = summary;
    Ok(())
}

fn nest_pattern(
    document: &mut WorkflowDocument,
    op: &NestOp,
    packages: &PackageIndex<'_>,
) -> Result<(), ComposeRefusal> {
    let (_, manifest, content_hash) = resolve(packages, &op.nest_id, &op.package)?;
    if document.nests.iter().any(|nest| nest.id == op.nest_id) {
        return Err(ComposeRefusal::DuplicateNest {
            nest_id: op.nest_id.clone(),
        });
    }
    if document.nodes.iter().any(|node| node.id == op.parent_node) {
        return Err(ComposeRefusal::OccupiedParent {
            nest_id: op.nest_id.clone(),
            parent_node: op.parent_node.clone(),
        });
    }

    document.nodes.push(slot_node(
        &op.parent_node,
        &op.package,
        content_hash,
        class_of(manifest.effect_class),
    ));
    document.nests.push(WorkflowNestBinding {
        child: pattern_child(&op.package, content_hash),
        expansion: op.expansion,
        id: op.nest_id.clone(),
        parent_node: op.parent_node.clone(),
        pin_policy: WorkflowNestBindingPinPolicy::Exact,
    });
    Ok(())
}

/// Remove the collapsed region at `nest_id`.
///
/// The slot's children live in the pinned fragment, never in the candidate, so flattening moves
/// nothing: the operation removes the binding and the slot node `nest` inserted, together with that
/// node's own slot bindings and pin. A slot node another nest still owns is kept. A slot the graph
/// still references is refused instead, because removing it would dangle an authored edge.
fn unnest_slot(document: &mut WorkflowDocument, op: &UnnestOp) -> Result<(), ComposeRefusal> {
    let position = document
        .nests
        .iter()
        .position(|nest| nest.id == op.nest_id)
        .ok_or_else(|| ComposeRefusal::UnknownNest {
            nest_id: op.nest_id.clone(),
        })?;
    let parent_node = document.nests[position].parent_node.clone();

    let referrers = slot_referrers(document, &parent_node);
    if !referrers.is_empty() {
        return Err(ComposeRefusal::WiredSlot {
            nest_id: op.nest_id.clone(),
            parent_node,
            referrers,
        });
    }

    document.nests.remove(position);
    if !document
        .nests
        .iter()
        .any(|nest| nest.parent_node == parent_node)
    {
        document.nodes.retain(|node| node.id != parent_node);
    }
    Ok(())
}

fn replace_subtree(
    document: &mut WorkflowDocument,
    op: &ReplaceSubtreeOp,
    packages: &PackageIndex<'_>,
) -> Result<(), ComposeRefusal> {
    let (_, manifest, content_hash) = resolve(packages, &op.nest_id, &op.package)?;
    let Some(nest) = document.nests.iter_mut().find(|nest| nest.id == op.nest_id) else {
        return Err(ComposeRefusal::UnknownNest {
            nest_id: op.nest_id.clone(),
        });
    };
    let parent_node = nest.parent_node.clone();
    nest.child = pattern_child(&op.package, content_hash);
    nest.expansion = op.expansion;

    // The region keeps its nest id and every other authored edit: only the pin changes, in place.
    if let Some(node) = document
        .nodes
        .iter_mut()
        .find(|node| node.id == parent_node)
    {
        node.pattern = Some(pattern_ref(&op.package, content_hash));
        node.effect_class = class_of(manifest.effect_class);
    }
    Ok(())
}

fn bind_slot(
    document: &mut WorkflowDocument,
    op: &BindSlotOp,
    packages: &PackageIndex<'_>,
) -> Result<(), ComposeRefusal> {
    let Some(nest) = document.nests.iter().find(|nest| nest.id == op.nest_id) else {
        return Err(ComposeRefusal::UnknownNest {
            nest_id: op.nest_id.clone(),
        });
    };
    let Some(package) = pinned_pattern(&nest.child) else {
        return Err(ComposeRefusal::SlotNotDeclared {
            nest_id: op.nest_id.clone(),
            pattern: child_label(&nest.child),
            slot: op.slot.clone(),
        });
    };
    let parent_node = nest.parent_node.clone();
    let (_, manifest, _) = resolve(packages, &op.nest_id, &package)?;

    let Some(slot) = manifest
        .slots
        .iter()
        .find(|slot| slot.name.as_str() == op.slot)
    else {
        return Err(ComposeRefusal::SlotNotDeclared {
            nest_id: op.nest_id.clone(),
            pattern: package.to_string(),
            slot: op.slot.clone(),
        });
    };
    check_slot_value(&document.assets, slot.kind, &op.value).map_err(|expected| {
        ComposeRefusal::SlotKindMismatch {
            nest_id: op.nest_id.clone(),
            slot: op.slot.clone(),
            expected: expected.to_owned(),
        }
    })?;

    let Some(node) = document
        .nodes
        .iter_mut()
        .find(|node| node.id == parent_node)
    else {
        return Err(ComposeRefusal::InvalidDocument {
            nest_id: op.nest_id.clone(),
            violations: vec![format!(
                "nest `{}` names parent node `{parent_node}`, which the document does not hold",
                op.nest_id
            )],
        });
    };
    node.bind.insert(op.slot.clone(), op.value.clone());
    Ok(())
}

/// What the manifest's slot kind accepts, checked on the bound value.
fn check_slot_value(
    assets: &[WorkflowAssetRef],
    kind: NexusPatternSlotKind,
    value: &Value,
) -> Result<(), &'static str> {
    let Some(text) = value.as_str().filter(|text| !text.is_empty()) else {
        return Err(kind_expectation(kind));
    };
    match kind {
        NexusPatternSlotKind::String | NexusPatternSlotKind::Capability => Ok(()),
        NexusPatternSlotKind::Asset if assets.iter().any(|asset| asset.id == text) => Ok(()),
        NexusPatternSlotKind::Asset => Err(kind_expectation(kind)),
    }
}

const fn kind_expectation(kind: NexusPatternSlotKind) -> &'static str {
    match kind {
        NexusPatternSlotKind::String => "a non-empty string",
        NexusPatternSlotKind::Asset => "an asset id present in document.assets",
        NexusPatternSlotKind::Capability => "a non-empty capability id",
    }
}

/// What still references `node_id`, in document order.
fn slot_referrers(document: &WorkflowDocument, node_id: &str) -> Vec<String> {
    let mut referrers = Vec::new();
    if document.entry == node_id {
        referrers.push("entry".to_owned());
    }
    if document.terminal.iter().any(|id| id == node_id) {
        referrers.push("terminal".to_owned());
    }
    for edge in &document.edges {
        if edge.from == node_id || edge.to == node_id {
            referrers.push(format!("edge `{}`", edge.id));
        }
    }
    for node in &document.nodes {
        if node.id == node_id {
            continue;
        }
        if node.depends_on.iter().any(|id| id == node_id) {
            referrers.push(format!("node `{}` depends_on", node.id));
        }
        if node.on_timeout.as_deref() == Some(node_id) {
            referrers.push(format!("node `{}` on_timeout", node.id));
        }
    }
    referrers
}

/// Every required slot of every pinned package that carries no binding, in document order.
fn unbound_required_slots(
    document: &WorkflowDocument,
    packages: &PackageIndex<'_>,
) -> Vec<ComposeDiagnostic> {
    let mut diagnostics = Vec::new();
    for nest in &document.nests {
        let Some(package) = pinned_pattern(&nest.child) else {
            continue;
        };
        let Some(found) = packages.get(&package) else {
            continue;
        };
        let Some(manifest) = found.manifest() else {
            continue;
        };
        let bindings = document
            .nodes
            .iter()
            .find(|node| node.id == nest.parent_node)
            .map(|node| &node.bind);
        for slot in manifest.slots.iter().filter(|slot| slot.required) {
            let bound = bindings.is_some_and(|bindings| bindings.contains_key(slot.name.as_str()));
            if !bound {
                diagnostics.push(ComposeDiagnostic::UnboundRequiredSlot {
                    nest_id: nest.id.clone(),
                    package: package.clone(),
                    slot: slot.name.as_str().to_owned(),
                });
            }
        }
    }
    diagnostics
}

/// The package registry indexed by the exact pin a compose request names.
struct PackageIndex<'a> {
    by_pin: HashMap<(&'a str, &'a str), &'a PatternPackage>,
}

impl<'a> PackageIndex<'a> {
    fn new(packages: &'a [PatternPackage]) -> Self {
        let mut by_pin = HashMap::new();
        for package in packages {
            let Some(manifest) = package.manifest() else {
                continue;
            };
            by_pin
                .entry((manifest.id.as_str(), manifest.version.as_str()))
                .or_insert(package);
        }
        Self { by_pin }
    }

    fn get(&self, package: &PatternPackageRef) -> Option<&'a PatternPackage> {
        self.by_pin
            .get(&(package.id.as_str(), package.version.as_str()))
            .copied()
    }

    /// The pattern bodies reachable from `document`, so the shared walk follows resolved fragments
    /// instead of treating every pattern pin as a leaf.
    fn bodies(&self, document: &WorkflowDocument) -> Vec<PatternBody<'a>> {
        let mut bodies = Vec::new();
        let mut seen = HashSet::new();
        self.collect_bodies(document, &mut bodies, &mut seen);
        bodies
    }

    fn collect_bodies(
        &self,
        document: &WorkflowDocument,
        bodies: &mut Vec<PatternBody<'a>>,
        seen: &mut HashSet<String>,
    ) {
        for nest in &document.nests {
            let Some(pin) = pinned_pattern(&nest.child) else {
                continue;
            };
            if !seen.insert(pin.to_string()) {
                continue;
            }
            let Some(found) = self.get(&pin) else {
                continue;
            };
            let Some(manifest) = found.manifest() else {
                continue;
            };
            let (registry_id, version) = (manifest.id.as_str(), manifest.version.as_str());
            match found.fragment() {
                Some(fragment) => {
                    bodies.push(PatternBody {
                        registry_id,
                        version,
                        body: PatternBodyKind::Fragment(fragment),
                    });
                    self.collect_bodies(fragment, bodies, seen);
                }
                None => bodies.push(PatternBody {
                    registry_id,
                    version,
                    body: PatternBodyKind::Declared(class_of(manifest.effect_class)),
                }),
            }
        }
    }
}

/// The available package for an exact pin, with its manifest and resolved content hash.
fn resolve<'a>(
    packages: &PackageIndex<'a>,
    nest_id: &str,
    pin: &PatternPackageRef,
) -> Result<(&'a PatternPackage, &'a PatternManifest, &'a str), ComposeRefusal> {
    let unavailable = |reason: &str| ComposeRefusal::PackageUnavailable {
        nest_id: nest_id.to_owned(),
        package: pin.clone(),
        reason: reason.to_owned(),
    };
    let found = packages
        .get(pin)
        .ok_or_else(|| unavailable("no package directory declares this id and version"))?;
    match (found.manifest(), found.content_hash()) {
        (Some(manifest), Some(content_hash)) if found.available() => {
            Ok((found, manifest, content_hash))
        }
        _ => {
            Err(unavailable(found.reason().unwrap_or(
                "the package did not load its manifest and fragment",
            )))
        }
    }
}

/// The exact pattern pin a nest child carries, when it is a pattern child.
fn pinned_pattern(child: &WorkflowNestChild) -> Option<PatternPackageRef> {
    if child.kind != WorkflowNestChildKind::Pattern {
        return None;
    }
    Some(PatternPackageRef {
        id: child.registry_id.clone()?,
        version: child.version.clone()?,
    })
}

/// A nest child's label, for a refusal that must name what the nest points at.
fn child_label(child: &WorkflowNestChild) -> String {
    match child.kind {
        WorkflowNestChildKind::Pattern => PatternPackageRef {
            id: child.registry_id.clone().unwrap_or_default(),
            version: child.version.clone().unwrap_or_default(),
        }
        .to_string(),
        WorkflowNestChildKind::Workflow => {
            format!(
                "workflow:{}",
                child.workflow_id.as_deref().unwrap_or_default()
            )
        }
    }
}

/// The collapsed slot a `nest` inserts: the exact pin and the package's declared class.
fn slot_node(
    id: &str,
    pin: &PatternPackageRef,
    content_hash: &str,
    effect_class: WorkflowNodeEffectClass,
) -> WorkflowNode {
    WorkflowNode {
        bind: serde_json::Map::new(),
        child_workflow_id: None,
        collapsed_view: None,
        context_update: None,
        converge: None,
        depends_on: Vec::new(),
        effect_class,
        enter: Vec::new(),
        exit_when: None,
        id: id.to_owned(),
        kind: WorkflowNodeKind::PatternSlot,
        merge: None,
        on_timeout: None,
        output_binding: None,
        pattern: Some(pattern_ref(pin, content_hash)),
        role: None,
        step_kind: None,
        template: None,
        timeout_ms: None,
        tool_policy: None,
        ui: None,
    }
}

fn pattern_ref(pin: &PatternPackageRef, content_hash: &str) -> WorkflowPatternRef {
    WorkflowPatternRef {
        content_hash: Some(content_hash.to_owned()),
        registry_id: pin.id.clone(),
        version: pin.version.clone(),
    }
}

fn pattern_child(pin: &PatternPackageRef, content_hash: &str) -> WorkflowNestChild {
    WorkflowNestChild {
        content_hash: Some(content_hash.to_owned()),
        kind: WorkflowNestChildKind::Pattern,
        registry_id: Some(pin.id.clone()),
        version: Some(pin.version.clone()),
        workflow_id: None,
        workflow_revision: None,
    }
}

/// The node class a package's declared effect folds to.
const fn class_of(class: PatternManifestEffectClass) -> WorkflowNodeEffectClass {
    match class {
        PatternManifestEffectClass::Pure => WorkflowNodeEffectClass::Pure,
        PatternManifestEffectClass::Read => WorkflowNodeEffectClass::Read,
        PatternManifestEffectClass::WriteCandidate => WorkflowNodeEffectClass::WriteCandidate,
        PatternManifestEffectClass::External => WorkflowNodeEffectClass::External,
    }
}
