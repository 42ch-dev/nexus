//! The embedding identity tuple and its mismatch evidence.

/// One component of [`EmbeddingIdentity`]. Mismatch evidence names the
/// component(s) that changed so operators see *why* a rebuild is required.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityComponent {
    /// Stable identifier of the serving provider (platform-side namespace).
    ProviderId,
    /// The provider's model identifier.
    ModelId,
    /// Provider-owned opaque version token; MUST change whenever the served
    /// weights change — fine-tune, quantization, truncation, or any other
    /// alteration of vector semantics.
    ModelVersion,
    /// Declared vector dimension.
    Dim,
}

/// The embedding identity tuple `(provider_id, model_id, model_version, dim)`.
///
/// Every derived vector index records the identity it was built with (the
/// populate stamp). Identity comparison is structural equality over all four
/// components; **any** component difference invalidates the index. Mixing
/// vectors from different models/versions silently corrupts similarity
/// semantics, so the only safe behavior is an explicit rebuild.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddingIdentity {
    /// Stable identifier of the serving provider (platform-side namespace).
    pub provider_id: String,
    /// The provider's model identifier.
    pub model_id: String,
    /// Provider-owned opaque version token. MUST change whenever the served
    /// weights change — fine-tune, quantization, truncation, or any other
    /// alteration of vector semantics.
    pub model_version: String,
    /// Declared vector dimension.
    pub dim: usize,
}

impl EmbeddingIdentity {
    /// Structural equality over all four components — the "same embedding
    /// identity" test the protocol's verifier anchors on.
    #[must_use]
    pub fn matches(&self, other: &Self) -> bool {
        self == other
    }

    /// The components that differ from `other`, in tuple order
    /// (`ProviderId`, `ModelId`, `ModelVersion`, `Dim`). Empty iff `matches`.
    #[must_use]
    pub fn differing_components(&self, other: &Self) -> Vec<IdentityComponent> {
        let mut changed = Vec::with_capacity(4);
        if self.provider_id != other.provider_id {
            changed.push(IdentityComponent::ProviderId);
        }
        if self.model_id != other.model_id {
            changed.push(IdentityComponent::ModelId);
        }
        if self.model_version != other.model_version {
            changed.push(IdentityComponent::ModelVersion);
        }
        if self.dim != other.dim {
            changed.push(IdentityComponent::Dim);
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity() -> EmbeddingIdentity {
        EmbeddingIdentity {
            provider_id: "nexus-platform".to_owned(),
            model_id: "lore-embed".to_owned(),
            model_version: "weights-2".to_owned(),
            dim: 384,
        }
    }

    #[test]
    fn identical_identities_match_with_no_changed_components() {
        let a = identity();
        let b = identity();
        assert!(a.matches(&b));
        assert!(a.differing_components(&b).is_empty());
    }

    #[test]
    fn provider_id_change_is_named() {
        let expected = identity();
        let actual = EmbeddingIdentity {
            provider_id: "other-provider".to_owned(),
            ..identity()
        };
        assert!(!actual.matches(&expected));
        assert_eq!(
            actual.differing_components(&expected),
            vec![IdentityComponent::ProviderId]
        );
    }

    #[test]
    fn model_id_change_is_named() {
        let expected = identity();
        let actual = EmbeddingIdentity {
            model_id: "other-model".to_owned(),
            ..identity()
        };
        assert_eq!(
            actual.differing_components(&expected),
            vec![IdentityComponent::ModelId]
        );
    }

    #[test]
    fn model_version_change_is_named() {
        let expected = identity();
        let actual = EmbeddingIdentity {
            model_version: "weights-3".to_owned(),
            ..identity()
        };
        assert_eq!(
            actual.differing_components(&expected),
            vec![IdentityComponent::ModelVersion]
        );
    }

    #[test]
    fn dim_change_is_named() {
        let expected = identity();
        let actual = EmbeddingIdentity {
            dim: 768,
            ..identity()
        };
        assert_eq!(
            actual.differing_components(&expected),
            vec![IdentityComponent::Dim]
        );
    }

    #[test]
    fn multi_component_changes_are_named_in_tuple_order() {
        let expected = identity();
        let actual = EmbeddingIdentity {
            provider_id: "other-provider".to_owned(),
            model_version: "weights-3".to_owned(),
            dim: 512,
            ..identity()
        };
        assert_eq!(
            actual.differing_components(&expected),
            vec![
                IdentityComponent::ProviderId,
                IdentityComponent::ModelVersion,
                IdentityComponent::Dim,
            ]
        );
    }
}
