//! The `EmbeddingProvider` seam and the OSS `NoEmbeddings` posture.

use async_trait::async_trait;
use thiserror::Error;

use crate::EmbeddingIdentity;

/// Errors from embedding providers. The only in-repo implementation
/// ([`NoEmbeddings`]) always reports [`Unavailable`]; platform
/// implementations extend this additively.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum EmbeddingError {
    /// Embeddings are a Nexus Platform-provided feature; the OSS product
    /// ships no embedding execution.
    #[error("no embedding provider is available — embeddings are a Nexus Platform-provided feature and the OSS product ships no execution")]
    Unavailable,
}

/// Embedding provider seam implemented and injected by the Nexus Platform.
///
/// The platform constructs its provider and injects it as
/// `Arc<dyn EmbeddingProvider>` at the daemon composition root; provider
/// construction, credentials, endpoints, and model selection are owned
/// entirely platform-side. The OSS config surface gains nothing: no config
/// keys, no environment variables, no CLI flags. When no platform injection
/// exists, the constructed value is [`NoEmbeddings`].
///
/// Async + object-safe with `Send + Sync` supertraits: the platform
/// implementation is network-backed, and the provider is shared across async
/// daemon tasks as `Arc<dyn EmbeddingProvider>`.
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// The identity this provider serves vectors under.
    fn identity(&self) -> &EmbeddingIdentity;

    /// Embed `texts` as vectors of the identity's declared `dim`.
    ///
    /// Providers must not return vectors whose length differs from `dim` —
    /// the verifier ([`crate::verify_vector_dimensions`]) treats that as
    /// corruption.
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingError>;
}

/// The OSS default: no platform provider injected.
///
/// It documents the OSS posture in code — embeddings are platform-provided,
/// the OSS product ships no execution — and lets consumers be written against
/// `Arc<dyn EmbeddingProvider>` before the platform lands. Every call to
/// [`embed`](Self::embed) fails with [`EmbeddingError::Unavailable`].
#[derive(Debug, Clone, Default)]
pub struct NoEmbeddings;

#[async_trait]
impl EmbeddingProvider for NoEmbeddings {
    fn identity(&self) -> &EmbeddingIdentity {
        // No real serving identity exists without a platform provider. While
        // `NoEmbeddings` is the injected provider, the composition root must
        // pass `None` (no provider) to `verify_index` — this placeholder is
        // never a valid populate stamp.
        static NO_PROVIDER_IDENTITY: EmbeddingIdentity = EmbeddingIdentity {
            provider_id: String::new(),
            model_id: String::new(),
            model_version: String::new(),
            dim: 0,
        };
        &NO_PROVIDER_IDENTITY
    }

    async fn embed(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        Err(EmbeddingError::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[tokio::test]
    async fn no_embeddings_embed_is_unavailable() {
        let provider = NoEmbeddings;
        let result = provider.embed(&["hello".to_owned()]).await;
        assert_eq!(result, Err(EmbeddingError::Unavailable));
    }

    #[tokio::test]
    async fn no_embeddings_is_object_safe_behind_arc_dyn() {
        let provider: Arc<dyn EmbeddingProvider> = Arc::new(NoEmbeddings);
        let result = provider.embed(&[]).await;
        assert_eq!(result, Err(EmbeddingError::Unavailable));
    }
}
