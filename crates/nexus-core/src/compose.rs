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
//! the fragment's current `content_hash`. Each operation re-resolves the pins it actually reads and
//! refuses a package whose bytes no longer hash to the recorded value, so the bytes it interprets
//! are the bytes the document identifies; a stale pin the operation does not reach is reported as
//! a diagnostic instead, so one changed region cannot wedge every other operation.
//! `replace_subtree` is the only operation allowed to establish a new pin, and it says so by
//! re-pinning exactly one region. `unnest` flattens the pinned fragment's body into the parent
//! level, reports every slot binding it had to drop, and refuses when the body declares something
//! the parent level cannot carry. `bind_slot` writes one slot binding on the slot node and
//! validates the value against the manifest's slot declaration; an unbound required slot stays a
//! named diagnostic, never a default.
//!
//! Nest policy and effect rollup are the shared `nexus-workflow-ir` walk and lattice: this module
//! resolves pins into [`PatternBody`]s and never re-derives depth, cycles or the enum order.

#[cfg(feature = "execution")]
use std::collections::BTreeSet;
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

use crate::strategy_candidates::conflict;
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

/// `unnest`: flatten the collapsed region at `nest_id` into the parent level.
#[derive(Debug, Clone)]
pub struct UnnestOp {
    /// Nest id of the region to flatten.
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
    /// Flatten a pinned region into the parent level.
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
    /// The package on disk no longer hashes to the pin the document recorded, so the operation
    /// would read different bytes than the document identifies.
    PinMismatch {
        /// The requested nest id.
        nest_id: String,
        /// The pinned package.
        package: PatternPackageRef,
        /// The hash the document recorded.
        recorded_hash: String,
        /// The hash the registry now serves.
        scanned_hash: String,
    },
    /// The nest has no fragment body to move into the parent level, so flattening would delete
    /// content rather than preserve it.
    Unflattenable {
        /// The requested nest id.
        nest_id: String,
        /// Why the child's body cannot be inlined.
        reason: String,
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
            Self::PinMismatch { .. } => "pin_mismatch",
            Self::Unflattenable { .. } => "nest_unflattenable",
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
            | Self::PinMismatch { nest_id, .. }
            | Self::Unflattenable { nest_id, .. } => nest_id,
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
            Self::PinMismatch {
                package,
                recorded_hash,
                scanned_hash,
                ..
            } => {
                details.insert("package_id".to_string(), Value::from(package.id.clone()));
                details.insert(
                    "package_version".to_string(),
                    Value::from(package.version.clone()),
                );
                details.insert(
                    "recorded_content_hash".to_string(),
                    Value::from(recorded_hash.clone()),
                );
                details.insert(
                    "scanned_content_hash".to_string(),
                    Value::from(scanned_hash.clone()),
                );
            }
            Self::UnknownNest { .. } | Self::DuplicateNest { .. } | Self::Unflattenable { .. } => {}
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
            Self::PinMismatch {
                nest_id,
                package,
                recorded_hash,
                scanned_hash,
            } => write!(
                f,
                "nest `{nest_id}` pinned `{package}` at content hash `{recorded_hash}`, but the \
                 registry now serves `{scanned_hash}`: the package changed under the same version"
            ),
            Self::Unflattenable { nest_id, reason } => {
                write!(f, "nest `{nest_id}` cannot be flattened: {reason}")
            }
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
    /// Flattening removed the slot node, so the binding it carried has no slot to address.
    DroppedSlotBinding {
        /// The nest that was flattened.
        nest_id: String,
        /// The bound slot name that had to be dropped.
        slot: String,
    },
    /// A pin outside the current operation's reach no longer hashes to what the document
    /// recorded. Reported, never refused: one stale region cannot wedge every other operation,
    /// and refusing here would make two stale regions refuse each other's fix.
    StalePin {
        /// The nest that records the pin.
        nest_id: String,
        /// The pinned package.
        package: PatternPackageRef,
        /// The hash the document recorded.
        recorded_hash: String,
        /// The hash the registry now serves.
        scanned_hash: String,
    },
    /// A recorded pin whose package the registry does not resolve, so there are no bytes to
    /// compare. Reported for the same reason a changed pin is: the pin cannot be confirmed today,
    /// and an approval that freezes it would claim bytes nobody can check.
    PinUnavailable {
        /// The nest that records the pin.
        nest_id: String,
        /// The pinned package.
        package: PatternPackageRef,
        /// The hash the document recorded.
        recorded_hash: String,
        /// Why resolution failed: no directory declares the pin, or the package did not load.
        reason: String,
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
            Self::DroppedSlotBinding { nest_id, slot } => write!(
                f,
                "nest `{nest_id}`: flattening dropped slot binding `{slot}`, which the flattened \
                 fragment does not address"
            ),
            Self::StalePin {
                nest_id,
                package,
                recorded_hash,
                scanned_hash,
            } => write!(
                f,
                "nest `{nest_id}` pinned `{package}` at content hash `{recorded_hash}`, but the \
                 registry now serves `{scanned_hash}`: replace this region to adopt the new bytes"
            ),
            Self::PinUnavailable {
                nest_id,
                package,
                recorded_hash,
                reason,
            } => write!(
                f,
                "nest `{nest_id}` pinned `{package}` at content hash `{recorded_hash}`, but the \
                 package is unavailable ({reason}): restore it before approving or consuming this \
                 region"
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
        // The conflict class multiplies under contention, so a stale base is refused from the row
        // alone: never after scanning the whole pattern registry for pins it will not use.
        let current = self.read_strategy_candidate(principal, strategy_id).await?;
        if current.revision != base_revision {
            return Err(conflict(strategy_id, base_revision, current.revision));
        }
        let packages = self.compose_packages()?;
        let index = PackageIndex::new(&packages);
        let mut diagnostics = Vec::new();
        let candidate = self
            .commit_strategy_candidate(principal, strategy_id, base_revision, |document| {
                let (next, findings) =
                    apply_compose(document, &op, &index).map_err(ComposeRefusal::into_error)?;
                diagnostics = findings;
                Ok(next)
            })
            .await?;
        diagnostics.extend(unbound_required_slots(&candidate.document, &index));
        Ok(ComposeOutcome {
            candidate,
            diagnostics,
        })
    }

    /// Every package under `<nexus_home>/patterns`, unavailable ones included: compose resolves
    /// pins against the same registry the listing serves.
    pub(crate) fn compose_packages(&self) -> CoreResult<Vec<PatternPackage>> {
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
///
/// The pins the operation itself resolves must still hash to what the document recorded, so a
/// changed package refuses the write instead of being reinterpreted. Every other recorded pin the
/// walk reaches is reported as a diagnostic, never refused: a stale region the operation does not
/// use must not wedge it. The operation's own diagnostics travel back with the document.
fn apply_compose(
    document: &WorkflowDocument,
    op: &ComposeOp,
    packages: &PackageIndex<'_>,
) -> Result<(WorkflowDocument, Vec<ComposeDiagnostic>), ComposeRefusal> {
    verify_pins(document, op, packages)?;
    let mut next = document.clone();
    let mut diagnostics = Vec::new();
    match op {
        ComposeOp::Nest(payload) => nest_pattern(&mut next, payload, packages)?,
        ComposeOp::Unnest(payload) => diagnostics = unnest_slot(&mut next, payload, packages)?,
        ComposeOp::ReplaceSubtree(payload) => replace_subtree(&mut next, payload, packages)?,
        ComposeOp::BindSlot(payload) => bind_slot(&mut next, payload, packages)?,
    }
    settle(&mut next, op.nest_id(), packages, &mut diagnostics)?;
    Ok((next, diagnostics))
}

/// Every pin the current operation resolves and uses must still hash to what the document
/// recorded.
///
/// Only the addressed region is checked: `nest` records a new pin, `bind_slot` reads the pattern
/// of the region it binds, and `unnest` reads that region's body. `replace_subtree` is the one
/// operation allowed to establish a new pin, so its target is exempt. A stale pin anywhere else
/// is a diagnostic from the shared walk ([`PackageIndex::bodies`]) — refusing it here would let
/// two stale regions refuse each other's fix.
fn verify_pins(
    document: &WorkflowDocument,
    op: &ComposeOp,
    packages: &PackageIndex<'_>,
) -> Result<(), ComposeRefusal> {
    let nest_id = match op {
        ComposeOp::Nest(_) | ComposeOp::ReplaceSubtree(_) => return Ok(()),
        ComposeOp::Unnest(payload) => &payload.nest_id,
        ComposeOp::BindSlot(payload) => &payload.nest_id,
    };
    // An unknown nest or an unresolved child is the operation's own refusal, not this check's.
    let Some(nest) = document.nests.iter().find(|nest| nest.id == *nest_id) else {
        return Ok(());
    };
    let Some(pin) = pinned_pattern(&nest.child) else {
        return Ok(());
    };
    verify_pin(&nest.id, &pin, nest.child.content_hash.as_deref(), packages)
}

/// Resolve one pin and compare the scanned hash with the hash the document recorded. A pin the
/// document left unresolved has nothing to compare and is not this check's business.
fn verify_pin(
    nest_id: &str,
    pin: &PatternPackageRef,
    recorded: Option<&str>,
    packages: &PackageIndex<'_>,
) -> Result<(), ComposeRefusal> {
    let Some(recorded) = recorded else {
        return Ok(());
    };
    let (_, _, scanned) = resolve(packages, nest_id, pin)?;
    if scanned == recorded {
        return Ok(());
    }
    Err(ComposeRefusal::PinMismatch {
        nest_id: nest_id.to_owned(),
        package: pin.clone(),
        recorded_hash: recorded.to_owned(),
        scanned_hash: scanned.to_owned(),
    })
}

/// Recompute the nest policy and the effect rollup on every operation.
///
/// `GraphCycle` is not compose policy: outer-state loops are legal for imported presets, which is
/// the exception the CLI's structural report already makes. Every other violation is a defect this
/// write must not preserve silently. The walk also reports every recorded pin whose bytes changed.
fn settle(
    document: &mut WorkflowDocument,
    nest_id: &str,
    packages: &PackageIndex<'_>,
    diagnostics: &mut Vec<ComposeDiagnostic>,
) -> Result<(), ComposeRefusal> {
    let bodies = packages.bodies(document, diagnostics);
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

/// Flatten the collapsed region at `nest_id` into the parent level.
///
/// The slot's children live in the pinned fragment, never in the candidate, so flattening loads
/// that fragment and moves its nodes, edges and its own nests into this level under fresh ids. A
/// graph reference the slot node owned is retargeted — incoming edges, dependencies and timeouts
/// to the fragment's entry, outgoing edges and terminal membership from its terminals — so an
/// authored edge survives the flatten. The slot node and its nest binding are removed with it.
///
/// The slot bindings the node carried are dropped: the flattened body declares no slot the parent
/// level could address them by, and inventing a default is what this module refuses to do. Every
/// dropped binding is reported so the editor can rebind it on the flattened body. Anything else the
/// body or the removed slot owns and the parent level cannot carry — a role, asset or signal a moved
/// child refers to, the slot's own dependencies and timeout reroute — refuses the flatten before any
/// write instead of being dropped.
fn unnest_slot(
    document: &mut WorkflowDocument,
    op: &UnnestOp,
    packages: &PackageIndex<'_>,
) -> Result<Vec<ComposeDiagnostic>, ComposeRefusal> {
    let position = document
        .nests
        .iter()
        .position(|nest| nest.id == op.nest_id)
        .ok_or_else(|| ComposeRefusal::UnknownNest {
            nest_id: op.nest_id.clone(),
        })?;
    let parent_node = document.nests[position].parent_node.clone();
    let Some(pin) = pinned_pattern(&document.nests[position].child) else {
        return Err(unflattenable(
            &op.nest_id,
            format!(
                "child `{}` is a workflow, so its body cannot be inlined",
                child_label(&document.nests[position].child)
            ),
        ));
    };
    let (package, _, _) = resolve(packages, &op.nest_id, &pin)?;
    let Some(body) = package.fragment() else {
        return Err(unflattenable(
            &op.nest_id,
            format!("package `{pin}` has no fragment body to move"),
        ));
    };
    // The body's own pins become this candidate's graph, so they are checked before it moves.
    verify_body_pins(body, packages, &mut HashSet::new())?;
    if let Some(lost) = lost_declarations(document, body, &parent_node) {
        return Err(unflattenable(&op.nest_id, lost));
    }

    let (node_ids, entry, terminals) = remap_body_ids(document, &op.nest_id, body, &pin)?;
    let outgoing = document.terminal.iter().any(|id| id == &parent_node)
        || document.edges.iter().any(|edge| edge.from == parent_node);
    if outgoing && terminals.is_empty() {
        return Err(unflattenable(
            &op.nest_id,
            format!("package `{pin}` declares no terminal node to keep the slot's outgoing wiring"),
        ));
    }

    move_body(document, body, &node_ids);
    let diagnostics = dropped_bindings(document, &parent_node, &op.nest_id);
    retarget_wiring(document, &parent_node, &entry, &terminals);

    document.nests.remove(position);
    document.nodes.retain(|node| node.id != parent_node);
    Ok(diagnostics)
}

fn unflattenable(nest_id: &str, reason: String) -> ComposeRefusal {
    ComposeRefusal::Unflattenable {
        nest_id: nest_id.to_owned(),
        reason,
    }
}

/// Every pin a body records must still hash to what it recorded: flattening turns that body into
/// this candidate's graph, so its pins are the pins the operation consumes.
fn verify_body_pins(
    body: &WorkflowDocument,
    packages: &PackageIndex<'_>,
    seen: &mut HashSet<String>,
) -> Result<(), ComposeRefusal> {
    for nest in &body.nests {
        let Some(pin) = pinned_pattern(&nest.child) else {
            continue;
        };
        verify_pin(&nest.id, &pin, nest.child.content_hash.as_deref(), packages)?;
        if !seen.insert(pin.to_string()) {
            continue;
        }
        if let Some(fragment) = packages
            .get(&pin)
            .and_then(nexus_pattern::PatternPackage::fragment)
        {
            verify_body_pins(fragment, packages, seen)?;
        }
    }
    Ok(())
}

/// The resolved pattern bodies the shared walk follows, plus the pin findings it reports.
///
/// For a caller that validates without writing (Approve): same index and same walk as compose,
/// and the one [`verify_pin`] comparison: an approval refuses on the first pin it cannot confirm.
#[cfg(feature = "execution")]
#[must_use]
pub fn document_bodies<'a>(
    document: &'a WorkflowDocument,
    packages: &'a [PatternPackage],
) -> (Vec<PatternBody<'a>>, Vec<ComposeDiagnostic>) {
    let mut diagnostics = Vec::new();
    let bodies = PackageIndex::new(packages).bodies(document, &mut diagnostics);
    (bodies, diagnostics)
}

/// The unbound required slots of a whole document, for a caller that does not run a compose
/// operation. Same declaration rule and same fragment walk as [`unbound_required_slots`].
#[cfg(feature = "execution")]
pub fn document_unbound_slots(
    document: &WorkflowDocument,
    packages: &[PatternPackage],
) -> Vec<ComposeDiagnostic> {
    unbound_required_slots(document, &PackageIndex::new(packages))
}

/// One `{id, version, content_hash}` pin an approval freezes.
#[cfg(feature = "execution")]
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ResolvedPatternPin {
    pub id: String,
    pub version: String,
    pub content_hash: String,
}

/// Every resolved pattern pin reachable from `document`, deduped and sorted by id, version and
/// hash so the frozen snapshot has deterministic bytes. A carrier without a recorded hash is an
/// unresolved pin (a deferred nest) and contributes none of its own; the resolved pins its
/// fragment records still travel, because that body will be consumed at Apply.
#[cfg(feature = "execution")]
pub fn resolved_pins(
    document: &WorkflowDocument,
    packages: &[PatternPackage],
) -> Vec<ResolvedPatternPin> {
    let mut pins = BTreeSet::new();
    collect_resolved_pins(
        document,
        &PackageIndex::new(packages),
        &mut HashSet::new(),
        &mut pins,
    );
    pins.into_iter()
        .map(|(id, version, content_hash)| ResolvedPatternPin {
            id,
            version,
            content_hash,
        })
        .collect()
}

#[cfg(feature = "execution")]
fn collect_resolved_pins(
    document: &WorkflowDocument,
    packages: &PackageIndex<'_>,
    seen: &mut HashSet<String>,
    pins: &mut BTreeSet<(String, String, String)>,
) {
    for nest in &document.nests {
        let Some(pin) = pinned_pattern(&nest.child) else {
            continue;
        };
        if let Some(content_hash) = nest.child.content_hash.as_deref() {
            pins.insert((pin.id.clone(), pin.version.clone(), content_hash.to_owned()));
        }
        if !seen.insert(pin.to_string()) {
            continue;
        }
        if let Some(fragment) = packages
            .get(&pin)
            .and_then(nexus_pattern::PatternPackage::fragment)
        {
            collect_resolved_pins(fragment, packages, seen, pins);
        }
    }
}

/// What flattening would lose silently: body-level declarations a moved child still refers to, and
/// the control references the removed slot node itself owns. `None` when the move carries
/// everything the body needs.
fn lost_declarations(
    document: &WorkflowDocument,
    body: &WorkflowDocument,
    parent_node: &str,
) -> Option<String> {
    let mut lost = Vec::new();
    for role in &body.roles {
        if body
            .nodes
            .iter()
            .any(|node| node.role.as_deref() == Some(role.id.as_str()))
        {
            lost.push(format!("role `{}`", role.id));
        }
    }
    for asset in &body.assets {
        let referenced = body.nodes.iter().any(|node| {
            node.template.as_deref() == Some(asset.id.as_str())
                || node
                    .bind
                    .values()
                    .any(|value| value.as_str() == Some(asset.id.as_str()))
        });
        if referenced {
            lost.push(format!("asset `{}`", asset.id));
        }
    }
    for signal in &body.signals {
        let target = signal.target.as_deref();
        if target.is_some_and(|target| body.nodes.iter().any(|node| node.id == target)) {
            lost.push(format!("signal `{}`", signal.name));
        }
    }
    if let Some(slot) = document.nodes.iter().find(|node| node.id == parent_node) {
        if !slot.depends_on.is_empty() {
            lost.push(format!(
                "the slot's depends_on `{}`",
                slot.depends_on.join("`, `")
            ));
        }
        if slot.on_timeout.is_some() {
            lost.push("the slot's on_timeout".to_owned());
        }
    }
    if lost.is_empty() {
        return None;
    }
    Some(format!(
        "the parent level does not carry {}, so flattening would drop it",
        lost.join(", ")
    ))
}

/// A flattened body's node ids remapped to this level, plus the entry and terminals they answer
/// to.
type LevelIds = (HashMap<String, String>, String, Vec<String>);

/// Remap the body's node ids into this level, keeping its entry and terminals addressable.
///
/// Fragment ids are this level's ids, so every one must survive a collision with what is here. The
/// removed slot's own ids stay reserved: reusing them would silently alias a new node to a
/// reference the editor still holds.
fn remap_body_ids(
    document: &WorkflowDocument,
    nest_id: &str,
    body: &WorkflowDocument,
    pin: &PatternPackageRef,
) -> Result<LevelIds, ComposeRefusal> {
    let mut taken: HashSet<String> = document.nodes.iter().map(|node| node.id.clone()).collect();
    let mut node_ids = HashMap::new();
    for node in &body.nodes {
        let fresh = fresh_id(&node.id, &mut taken);
        node_ids.insert(node.id.clone(), fresh);
    }
    let Some(entry) = node_ids.get(body.entry.as_str()).cloned() else {
        return Err(unflattenable(
            nest_id,
            format!("package `{pin}` declares no entry node to receive the slot's wiring"),
        ));
    };
    let terminals = body
        .terminal
        .iter()
        .filter_map(|id| node_ids.get(id).cloned())
        .collect();
    Ok((node_ids, entry, terminals))
}

/// Move the pinned body's nodes, edges and nests into the candidate level.
fn move_body(
    document: &mut WorkflowDocument,
    body: &WorkflowDocument,
    node_ids: &HashMap<String, String>,
) {
    let mut taken_edges: HashSet<String> =
        document.edges.iter().map(|edge| edge.id.clone()).collect();
    let mut taken_nests: HashSet<String> =
        document.nests.iter().map(|nest| nest.id.clone()).collect();

    for node in &body.nodes {
        let mut moved = node.clone();
        if let Some(id) = node_ids.get(&node.id) {
            moved.id.clone_from(id);
        }
        // A reference that names no fragment node cannot survive the move: repointing it would
        // wire a stranger's id, so it is dropped.
        moved.depends_on = node
            .depends_on
            .iter()
            .filter_map(|dependency| node_ids.get(dependency).cloned())
            .collect();
        moved.on_timeout = node
            .on_timeout
            .as_deref()
            .and_then(|timeout| node_ids.get(timeout).cloned());
        document.nodes.push(moved);
    }
    for edge in &body.edges {
        let (Some(from), Some(to)) = (node_ids.get(&edge.from), node_ids.get(&edge.to)) else {
            continue;
        };
        let mut moved = edge.clone();
        moved.id = fresh_id(&edge.id, &mut taken_edges);
        moved.from.clone_from(from);
        moved.to.clone_from(to);
        document.edges.push(moved);
    }
    for nest in &body.nests {
        let Some(parent) = node_ids.get(&nest.parent_node) else {
            continue;
        };
        let mut moved = nest.clone();
        moved.id = fresh_id(&nest.id, &mut taken_nests);
        moved.parent_node.clone_from(parent);
        document.nests.push(moved);
    }
}

/// The slot bindings flattening has to drop, named one by one.
fn dropped_bindings(
    document: &WorkflowDocument,
    parent_node: &str,
    nest_id: &str,
) -> Vec<ComposeDiagnostic> {
    document
        .nodes
        .iter()
        .find(|node| node.id == parent_node)
        .map(|node| {
            node.bind
                .keys()
                .map(|slot| ComposeDiagnostic::DroppedSlotBinding {
                    nest_id: nest_id.to_owned(),
                    slot: slot.clone(),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Point everything that referenced the slot node at the flattened body's boundary nodes.
fn retarget_wiring(
    document: &mut WorkflowDocument,
    parent_node: &str,
    entry: &str,
    terminals: &[String],
) {
    if document.entry == parent_node {
        entry.clone_into(&mut document.entry);
    }
    let mut taken_edges: HashSet<String> =
        document.edges.iter().map(|edge| edge.id.clone()).collect();
    let mut retargeted = Vec::with_capacity(document.edges.len());
    for edge in std::mem::take(&mut document.edges) {
        let from_slot = edge.from == parent_node;
        let to_slot = edge.to == parent_node;
        if !from_slot && !to_slot {
            retargeted.push(edge);
            continue;
        }
        let froms = if from_slot {
            terminals.to_vec()
        } else {
            vec![edge.from.clone()]
        };
        let to = if to_slot {
            entry.to_owned()
        } else {
            edge.to.clone()
        };
        for (index, from) in froms.iter().enumerate() {
            let mut copy = edge.clone();
            copy.from.clone_from(from);
            copy.to.clone_from(&to);
            if index > 0 {
                copy.id = fresh_id(&edge.id, &mut taken_edges);
            }
            retargeted.push(copy);
        }
    }
    document.edges = retargeted;

    let mut terminals_at_this_level = Vec::with_capacity(document.terminal.len());
    for id in std::mem::take(&mut document.terminal) {
        if id == parent_node {
            terminals_at_this_level.extend(terminals.iter().cloned());
        } else {
            terminals_at_this_level.push(id);
        }
    }
    document.terminal = terminals_at_this_level;

    for node in &mut document.nodes {
        if node.id == parent_node {
            continue;
        }
        node.depends_on = node
            .depends_on
            .iter()
            .map(|dependency| {
                if dependency == parent_node {
                    entry.to_owned()
                } else {
                    dependency.clone()
                }
            })
            .collect();
        if node.on_timeout.as_deref() == Some(parent_node) {
            node.on_timeout = Some(entry.to_owned());
        }
    }
}

/// An id that names nothing already taken at this level: the fragment's own id when it is free,
/// otherwise its first free `~n` successor. Deterministic, so a replay produces the same graph.
fn fresh_id(base: &str, taken: &mut HashSet<String>) -> String {
    if taken.insert(base.to_owned()) {
        return base.to_owned();
    }
    let mut index = 2;
    loop {
        let candidate = format!("{base}~{index}");
        if taken.insert(candidate.clone()) {
            return candidate;
        }
        index += 1;
    }
}

/// Re-pin the region at `nest_id` to another exact package.
///
/// This is the only operation allowed to establish a new pin: the region's recorded hash is
/// rewritten from the registry's current bytes, which is why [`verify_pins`] exempts it. Every
/// binding the region already carries is retained and re-validated against the replacement
/// pattern's declarations, so the retention can never say something `bind_slot` would refuse.
fn replace_subtree(
    document: &mut WorkflowDocument,
    op: &ReplaceSubtreeOp,
    packages: &PackageIndex<'_>,
) -> Result<(), ComposeRefusal> {
    let (_, manifest, content_hash) = resolve(packages, &op.nest_id, &op.package)?;
    let Some(nest) = document.nests.iter().find(|nest| nest.id == op.nest_id) else {
        return Err(ComposeRefusal::UnknownNest {
            nest_id: op.nest_id.clone(),
        });
    };
    let parent_node = nest.parent_node.clone();

    // One manifest-binding rule for explicit binding and for what a replacement retains.
    if let Some(node) = document.nodes.iter().find(|node| node.id == parent_node) {
        for (slot, value) in &node.bind {
            check_binding(
                &document.assets,
                manifest,
                &op.nest_id,
                &op.package,
                slot,
                value,
            )?;
        }
    }

    if let Some(nest) = document.nests.iter_mut().find(|nest| nest.id == op.nest_id) {
        nest.child = pattern_child(&op.package, content_hash);
        nest.expansion = op.expansion;
    }

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

    check_binding(
        &document.assets,
        manifest,
        &op.nest_id,
        &package,
        &op.slot,
        &op.value,
    )?;

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

/// The one manifest-binding rule: the slot must be declared, and the value must satisfy the
/// declaration. `bind_slot` and the bindings `replace_subtree` retains are held to it alike.
fn check_binding(
    assets: &[WorkflowAssetRef],
    manifest: &PatternManifest,
    nest_id: &str,
    pin: &PatternPackageRef,
    slot: &str,
    value: &Value,
) -> Result<(), ComposeRefusal> {
    let Some(declared) = manifest
        .slots
        .iter()
        .find(|declared| declared.name.as_str() == slot)
    else {
        return Err(ComposeRefusal::SlotNotDeclared {
            nest_id: nest_id.to_owned(),
            pattern: pin.to_string(),
            slot: slot.to_owned(),
        });
    };
    check_slot_value(assets, declared.kind, value).map_err(|expected| {
        ComposeRefusal::SlotKindMismatch {
            nest_id: nest_id.to_owned(),
            slot: slot.to_owned(),
            expected: expected.to_owned(),
        }
    })
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

/// Every required slot of every pinned package that carries no binding, in document order,
/// descending into the resolved pattern bodies the shared walk follows.
fn unbound_required_slots(
    document: &WorkflowDocument,
    packages: &PackageIndex<'_>,
) -> Vec<ComposeDiagnostic> {
    let mut diagnostics = Vec::new();
    collect_unbound_required_slots(document, packages, &mut HashSet::new(), &mut diagnostics);
    diagnostics
}

/// One document's required-slot findings plus those of every fragment its pins reach. A nested
/// pattern's slots live on the slot node inside that fragment, so the binding is read from the
/// document being walked, never from the candidate root.
fn collect_unbound_required_slots(
    document: &WorkflowDocument,
    packages: &PackageIndex<'_>,
    seen: &mut HashSet<String>,
    diagnostics: &mut Vec<ComposeDiagnostic>,
) {
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
        if !seen.insert(package.to_string()) {
            continue;
        }
        if let Some(fragment) = found.fragment() {
            collect_unbound_required_slots(fragment, packages, seen, diagnostics);
        }
    }
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
    ///
    /// Every carrier of a recorded pin is reported when the pin cannot be confirmed — a
    /// [`ComposeDiagnostic::StalePin`] for changed bytes, a [`ComposeDiagnostic::PinUnavailable`]
    /// for a package the registry no longer resolves — the operation's own region included, so a
    /// successful write never leaves a pin it read silently unverified.
    fn bodies(
        &self,
        document: &WorkflowDocument,
        diagnostics: &mut Vec<ComposeDiagnostic>,
    ) -> Vec<PatternBody<'a>> {
        let mut bodies = Vec::new();
        let mut seen = HashSet::new();
        self.collect_bodies(document, &mut bodies, &mut seen, diagnostics);
        bodies
    }

    fn collect_bodies(
        &self,
        document: &WorkflowDocument,
        bodies: &mut Vec<PatternBody<'a>>,
        seen: &mut HashSet<String>,
        diagnostics: &mut Vec<ComposeDiagnostic>,
    ) {
        for nest in &document.nests {
            let Some(pin) = pinned_pattern(&nest.child) else {
                continue;
            };
            // The recorded hash is checked before this walk de-dupes the pin and before a package
            // that failed to load is skipped, so a carrier whose bytes changed or whose package is
            // gone is always reported.
            self.report_unusable_pin(nest, &pin, diagnostics);
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
                    self.collect_bodies(fragment, bodies, seen, diagnostics);
                }
                None => bodies.push(PatternBody {
                    registry_id,
                    version,
                    body: PatternBodyKind::Declared(class_of(manifest.effect_class)),
                }),
            }
        }
    }

    /// Report one carrier whose recorded pin cannot be confirmed against the registry, through the
    /// one [`verify_pin`] comparison: bytes that changed are a [`ComposeDiagnostic::StalePin`], and
    /// a pin whose package does not resolve is a [`ComposeDiagnostic::PinUnavailable`]. A carrier
    /// with no recorded hash names nothing to confirm and is not reported.
    fn report_unusable_pin(
        &self,
        nest: &WorkflowNestBinding,
        pin: &PatternPackageRef,
        diagnostics: &mut Vec<ComposeDiagnostic>,
    ) {
        let Some(recorded_hash) = nest.child.content_hash.as_deref() else {
            return;
        };
        match verify_pin(&nest.id, pin, Some(recorded_hash), self) {
            Err(ComposeRefusal::PinMismatch {
                package,
                recorded_hash,
                scanned_hash,
                ..
            }) => diagnostics.push(ComposeDiagnostic::StalePin {
                nest_id: nest.id.clone(),
                package,
                recorded_hash,
                scanned_hash,
            }),
            Err(ComposeRefusal::PackageUnavailable {
                package, reason, ..
            }) => diagnostics.push(ComposeDiagnostic::PinUnavailable {
                nest_id: nest.id.clone(),
                package,
                recorded_hash: recorded_hash.to_owned(),
                reason,
            }),
            Ok(()) | Err(_) => {}
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

/// The exact pattern pin a nest child carries, when it is a resolved pattern child.
///
/// A pin is resolved only with both halves of `{id, version}`: an empty or absent half names no
/// package, so treating it as resolved would mislabel what the nest points at.
fn pinned_pattern(child: &WorkflowNestChild) -> Option<PatternPackageRef> {
    if child.kind != WorkflowNestChildKind::Pattern {
        return None;
    }
    let id = child.registry_id.clone().filter(|id| !id.is_empty())?;
    let version = child
        .version
        .clone()
        .filter(|version| !version.is_empty())?;
    Some(PatternPackageRef { id, version })
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

#[cfg(test)]
mod tests {
    use super::*;

    fn child(registry_id: Option<&str>, version: Option<&str>) -> WorkflowNestChild {
        WorkflowNestChild {
            content_hash: None,
            kind: WorkflowNestChildKind::Pattern,
            registry_id: registry_id.map(ToOwned::to_owned),
            version: version.map(ToOwned::to_owned),
            workflow_id: None,
            workflow_revision: None,
        }
    }

    #[test]
    fn a_pattern_pin_is_resolved_only_with_both_halves() {
        let pin = PatternPackageRef {
            id: "research-loop".to_owned(),
            version: "1.0.0".to_owned(),
        };
        assert_eq!(
            pinned_pattern(&child(Some("research-loop"), Some("1.0.0"))),
            Some(pin)
        );
        // A pin missing or emptying either half names no package, so the resolved test compares
        // the version too instead of falling through on a non-empty id.
        assert_eq!(pinned_pattern(&child(Some("research-loop"), None)), None);
        assert_eq!(
            pinned_pattern(&child(Some("research-loop"), Some(""))),
            None
        );
        assert_eq!(pinned_pattern(&child(None, Some("1.0.0"))), None);
        assert_eq!(pinned_pattern(&child(Some(""), Some("1.0.0"))), None);
        assert_eq!(
            pinned_pattern(&WorkflowNestChild {
                kind: WorkflowNestChildKind::Workflow,
                ..child(Some("research-loop"), Some("1.0.0"))
            }),
            None
        );
    }

    #[test]
    fn a_flattened_id_stays_unique_at_its_level() {
        let mut taken: HashSet<String> = ["a".to_owned(), "a~2".to_owned()].into();
        assert_eq!(fresh_id("b", &mut taken), "b");
        assert_eq!(fresh_id("a", &mut taken), "a~3");
        assert_eq!(fresh_id("a", &mut taken), "a~4");
    }
}
