//! Fail-closed derived-index protocol: verify before any vector read.

use crate::identity::{EmbeddingIdentity, IdentityComponent};

/// One embedded entry in a derived vector index. Every record carries the
/// identity it was embedded under; records are only ever read after
/// [`verify_index`] returned [`IndexVerdict::Usable`].
#[derive(Debug, Clone, PartialEq)]
pub struct IndexRecord {
    /// Identity this record's vector was embedded under.
    pub identity: EmbeddingIdentity,
    /// The embedded vector.
    pub vector: Vec<f32>,
}

/// The populate stamp: the identity a populate pass recorded, plus its
/// scope-completeness marker. This is the verification anchor — an
/// unverified store is never served.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PopulateStamp {
    /// Identity recorded when the populate pass ran.
    pub identity: EmbeddingIdentity,
    /// Whether the populate pass covered the full scope. `false` means
    /// populate was interrupted or partial.
    pub scope_complete: bool,
}

/// State of a derived vector index. A derived index is **rebuildable data**;
/// the authoritative store is the World KB (lore entries) / memory fragments.
#[derive(Debug, Clone, PartialEq)]
pub struct DerivedIndexState {
    /// Index records built by the last populate pass.
    pub records: Vec<IndexRecord>,
    /// Populate stamp of the last populate pass; `None` = index never built.
    pub populate_stamp: Option<PopulateStamp>,
}

/// Mismatch evidence: the identity component(s) that differ, so operators see
/// *why* a rebuild is required.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentityMismatch {
    /// The changed component(s), in tuple order.
    pub changed: Vec<IdentityComponent>,
}

/// Why a rebuild is required.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RebuildReason {
    /// The index was populated under a different embedding identity than the
    /// one currently expected.
    IdentityMismatch(IdentityMismatch),
    /// The populate stamp matches but the scope is incomplete (populate
    /// interrupted or partial).
    IncompleteScope,
}

/// Verdict of [`verify_index`] / [`verify_vector_dimensions`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexVerdict {
    /// Safe to read: the populate stamp's identity equals the expected
    /// identity and the scope is complete. May carry zero records — a freshly
    /// scoped world legitimately has no entries.
    Usable,
    /// Must be rebuilt before use; never served as-is. Lexical fallback is an
    /// explicit, caller-taken decision, never a silent degrade.
    RebuildRequired(RebuildReason),
    /// No provider, or no populate stamp at all (index never built).
    Unavailable,
}

/// Fail-closed verification of a derived vector index against the *currently
/// expected* embedding identity, before any vector read.
///
/// `expected` is `None` when no provider is available (the composition root
/// holds `NoEmbeddings` / no injection) — the index cannot be verified and is
/// [`IndexVerdict::Unavailable`]. Verdicts (normative spec §3):
///
/// - matching populate stamp + complete scope → `Usable` (even with zero
///   records — a freshly scoped world legitimately has no entries);
/// - no populate stamp → `Unavailable` (index never built);
/// - any identity component differs → `RebuildRequired(IdentityMismatch(..))`
///   with the changed component(s) named;
/// - populate stamp present but scope incomplete →
///   `RebuildRequired(IncompleteScope)`.
#[must_use]
pub fn verify_index(
    expected: Option<&EmbeddingIdentity>,
    index: &DerivedIndexState,
) -> IndexVerdict {
    // No provider → nothing can be verified against, never served.
    let Some(expected) = expected else {
        return IndexVerdict::Unavailable;
    };
    // No populate stamp → index never built.
    let Some(stamp) = &index.populate_stamp else {
        return IndexVerdict::Unavailable;
    };
    // Any identity component difference invalidates the index (fail-closed).
    if !expected.matches(&stamp.identity) {
        return IndexVerdict::RebuildRequired(RebuildReason::IdentityMismatch(IdentityMismatch {
            changed: expected.differing_components(&stamp.identity),
        }));
    }
    if !stamp.scope_complete {
        return IndexVerdict::RebuildRequired(RebuildReason::IncompleteScope);
    }
    IndexVerdict::Usable
}

/// Shape check on a provider's output against the declared identity.
///
/// Any vector whose length differs from `identity.dim` is treated as
/// **corruption** — those vectors were not produced under the declared
/// identity (the effective dim differs), so an index built from them must be
/// rebuilt (normative spec §4).
#[must_use]
pub fn verify_vector_dimensions(
    identity: &EmbeddingIdentity,
    vectors: &[Vec<f32>],
) -> IndexVerdict {
    if vectors.iter().any(|vector| vector.len() != identity.dim) {
        IndexVerdict::RebuildRequired(RebuildReason::IdentityMismatch(IdentityMismatch {
            changed: vec![IdentityComponent::Dim],
        }))
    } else {
        IndexVerdict::Usable
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn expected() -> EmbeddingIdentity {
        EmbeddingIdentity {
            provider_id: "nexus-platform".to_owned(),
            model_id: "lore-embed".to_owned(),
            model_version: "weights-2".to_owned(),
            dim: 384,
        }
    }

    fn stamp(identity: EmbeddingIdentity, scope_complete: bool) -> PopulateStamp {
        PopulateStamp {
            identity,
            scope_complete,
        }
    }

    fn empty_index(populate_stamp: Option<PopulateStamp>) -> DerivedIndexState {
        DerivedIndexState {
            records: Vec::new(),
            populate_stamp,
        }
    }

    // --- verify_index verdicts (locked, architect) ---

    #[test]
    fn empty_index_with_matching_complete_stamp_is_usable() {
        let expected = expected();
        let index = empty_index(Some(stamp(expected.clone(), true)));
        assert_eq!(verify_index(Some(&expected), &index), IndexVerdict::Usable);
    }

    #[test]
    fn populated_index_with_matching_complete_stamp_is_usable() {
        let expected = expected();
        let index = DerivedIndexState {
            records: vec![IndexRecord {
                identity: expected.clone(),
                vector: vec![0.0; 384],
            }],
            populate_stamp: Some(stamp(expected.clone(), true)),
        };
        assert_eq!(verify_index(Some(&expected), &index), IndexVerdict::Usable);
    }

    #[test]
    fn no_populate_stamp_is_unavailable() {
        let expected = expected();
        let index = empty_index(None);
        assert_eq!(
            verify_index(Some(&expected), &index),
            IndexVerdict::Unavailable
        );
    }

    #[test]
    fn missing_provider_is_unavailable_even_with_a_complete_stamp() {
        let expected = expected();
        let index = empty_index(Some(stamp(expected, true)));
        assert_eq!(verify_index(None, &index), IndexVerdict::Unavailable);
    }

    #[test]
    fn provider_id_mismatch_requires_rebuild_naming_provider_id() {
        let expected = expected();
        let actual = EmbeddingIdentity {
            provider_id: "other-provider".to_owned(),
            ..expected.clone()
        };
        let index = empty_index(Some(stamp(actual, true)));
        assert_eq!(
            verify_index(Some(&expected), &index),
            IndexVerdict::RebuildRequired(RebuildReason::IdentityMismatch(IdentityMismatch {
                changed: vec![IdentityComponent::ProviderId],
            }))
        );
    }

    #[test]
    fn model_id_mismatch_requires_rebuild_naming_model_id() {
        let expected = expected();
        let actual = EmbeddingIdentity {
            model_id: "other-model".to_owned(),
            ..expected.clone()
        };
        let index = empty_index(Some(stamp(actual, true)));
        assert_eq!(
            verify_index(Some(&expected), &index),
            IndexVerdict::RebuildRequired(RebuildReason::IdentityMismatch(IdentityMismatch {
                changed: vec![IdentityComponent::ModelId],
            }))
        );
    }

    #[test]
    fn model_version_mismatch_requires_rebuild_naming_model_version() {
        let expected = expected();
        let actual = EmbeddingIdentity {
            model_version: "weights-3".to_owned(),
            ..expected.clone()
        };
        let index = empty_index(Some(stamp(actual, true)));
        assert_eq!(
            verify_index(Some(&expected), &index),
            IndexVerdict::RebuildRequired(RebuildReason::IdentityMismatch(IdentityMismatch {
                changed: vec![IdentityComponent::ModelVersion],
            }))
        );
    }

    #[test]
    fn dim_mismatch_requires_rebuild_naming_dim() {
        let expected = expected();
        let actual = EmbeddingIdentity {
            dim: 512,
            ..expected.clone()
        };
        let index = empty_index(Some(stamp(actual, true)));
        assert_eq!(
            verify_index(Some(&expected), &index),
            IndexVerdict::RebuildRequired(RebuildReason::IdentityMismatch(IdentityMismatch {
                changed: vec![IdentityComponent::Dim],
            }))
        );
    }

    #[test]
    fn complete_stamp_with_incomplete_scope_requires_rebuild() {
        let expected = expected();
        let index = empty_index(Some(stamp(expected.clone(), false)));
        assert_eq!(
            verify_index(Some(&expected), &index),
            IndexVerdict::RebuildRequired(RebuildReason::IncompleteScope)
        );
    }

    #[test]
    fn identity_mismatch_dominates_incomplete_scope() {
        let expected = expected();
        let actual = EmbeddingIdentity {
            model_id: "other-model".to_owned(),
            ..expected.clone()
        };
        let index = empty_index(Some(stamp(actual, false)));
        assert_eq!(
            verify_index(Some(&expected), &index),
            IndexVerdict::RebuildRequired(RebuildReason::IdentityMismatch(IdentityMismatch {
                changed: vec![IdentityComponent::ModelId],
            }))
        );
    }

    // --- provider output shape (corruption, normative spec §4) ---

    #[test]
    fn vector_length_mismatch_is_corruption_requiring_rebuild() {
        let expected = expected();
        let vectors = vec![vec![0.0; 384], vec![0.0; 383]];
        assert_eq!(
            verify_vector_dimensions(&expected, &vectors),
            IndexVerdict::RebuildRequired(RebuildReason::IdentityMismatch(IdentityMismatch {
                changed: vec![IdentityComponent::Dim],
            }))
        );
    }

    #[test]
    fn matching_vector_lengths_are_usable() {
        let expected = expected();
        let vectors = vec![vec![0.5; 384], vec![0.25; 384]];
        assert_eq!(
            verify_vector_dimensions(&expected, &vectors),
            IndexVerdict::Usable
        );
    }

    #[test]
    fn empty_output_is_usable() {
        let expected = expected();
        let vectors: Vec<Vec<f32>> = Vec::new();
        assert_eq!(
            verify_vector_dimensions(&expected, &vectors),
            IndexVerdict::Usable
        );
    }
}
