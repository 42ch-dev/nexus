//! Thin delegation wrappers over [`spoke_operations`] lifecycle helpers.
//!
//! Each wrapper enforces the call-boundary invariant (tracked spec §7):
//! operands are already spoke standard types at the call site. The wrappers
//! do not transform types internally — they pass operands straight through to
//! the underlying `spoke_operations` function and preserve the
//! [`SpokeResult`] return type verbatim.
//!
//! Where a wrapper's short name differs from the underlying spoke function
//! (e.g. [`apply_promote`] → `spoke_operations::apply_promote_acceptance`),
//! the rename is the only adaptation — the invariant itself stays in spoke.

use spoke_operations::{
    apply_promote_acceptance, assert_revision_match, merge_extension_maps,
    transition_knowledge_entry_status, validate_promote_request, BuildAssemblePacketInput,
    ExtensionMap, KnowledgeEntryForAssemble, SpokeResult,
};
use spoke_schemas::{AssemblePacket, Finding, KnowledgeEntry, PromoteRequest};

/// Delegate to [`spoke_operations::validate_promote_request`].
///
/// Operand: spoke [`PromoteRequest`] only. Validates promote shape and
/// lifecycle rules (candidate is `provisional`, not terminal, target ≠ self,
/// revision is a non-negative integer) without persisting.
#[must_use]
pub fn validate_promote(request: &PromoteRequest) -> SpokeResult<()> {
    validate_promote_request(request)
}

/// Delegate to [`spoke_operations::apply_promote_acceptance`].
///
/// Operand: spoke [`PromoteRequest`] only. Returns the promoted
/// [`KnowledgeEntry`] (`status: confirmed`, revision bumped). Does not
/// persist.
#[must_use]
pub fn apply_promote(request: &PromoteRequest) -> SpokeResult<KnowledgeEntry> {
    apply_promote_acceptance(request)
}

/// Delegate to [`spoke_operations::transition_knowledge_entry_status`].
///
/// Operand: spoke [`KnowledgeEntry`] only. Returns a new `KnowledgeEntry`
/// with the updated status; input is not mutated.
#[must_use]
pub fn transition_status(entry: &KnowledgeEntry, to: &str) -> SpokeResult<KnowledgeEntry> {
    transition_knowledge_entry_status(entry, to)
}

/// Delegate to [`spoke_operations::transition_finding_status`].
///
/// Operand: spoke [`Finding`] only. Returns a new `Finding` with the updated
/// status and `updated_at` set; input is not mutated.
#[must_use]
pub fn transition_finding_status(finding: &Finding, to: &str) -> SpokeResult<Finding> {
    spoke_operations::transition_finding_status(finding, to)
}

/// Delegate to [`spoke_operations::build_assemble_packet`].
///
/// Operands: spoke [`KnowledgeEntry`] slice only. Builds a wire-valid
/// [`AssemblePacket`] from the entries (order-preserving truncate only when
/// `max_entries` is `Some`).
///
/// # Spec vs spoke-reality note
///
/// Tracked spec §7.2 sketches this wrapper as
/// `(packet_id, entries: &[KnowledgeEntry], max_entries)`. The underlying
/// spoke function takes a [`BuildAssemblePacketInput`] struct whose
/// `knowledge_entries` field is `&[KnowledgeEntryForAssemble]` and which
/// also exposes a packet-level `extensions` slot. This wrapper honors the
/// §7.2 surface: it wraps each entry via
/// [`KnowledgeEntryForAssemble::from_entry`] (programmatic construction —
/// body wire is derived from the typed body) and passes `extensions: None`.
/// The lifecycle invariant (validation, truncation, namespace-key checks)
/// stays entirely in `spoke_operations`. If a future caller needs
/// packet-level extensions, §7.2 should be amended to expose spoke's richer
/// input shape rather than this wrapper growing new parameters.
pub fn build_assemble_packet(
    packet_id: &str,
    entries: &[KnowledgeEntry],
    max_entries: Option<usize>,
) -> SpokeResult<AssemblePacket> {
    let wrapped: Vec<KnowledgeEntryForAssemble> = entries
        .iter()
        .cloned()
        .map(KnowledgeEntryForAssemble::from_entry)
        .collect();
    spoke_operations::build_assemble_packet(BuildAssemblePacketInput {
        packet_id,
        knowledge_entries: &wrapped,
        extensions: None,
        max_entries,
    })
}

/// Delegate to [`spoke_operations::merge_extension_maps`].
///
/// Operands: spoke [`ExtensionMap`] only. Deep-merges two extension maps;
/// overlay wins on scalar conflicts. Returned map is independent of both
/// inputs (no shared aliases into nested objects).
#[must_use]
pub fn merge_extensions(base: &ExtensionMap, overlay: &ExtensionMap) -> ExtensionMap {
    merge_extension_maps(base, overlay)
}

/// Delegate to [`spoke_operations::assert_revision_match`].
///
/// Compares caller-supplied revisions before persist; the library performs
/// no storage I/O. Returns `Ok(())` when equal, otherwise a reject with
/// `RevisionConflict` or `StoredRevisionStale`.
#[must_use]
pub fn assert_revision(expected: u64, actual: u64) -> SpokeResult<()> {
    assert_revision_match(expected, actual)
}
