//! Opaque stored-authorized principal bound to a service generation.

#[derive(Debug, Clone)]
pub struct Principal {
    creator_id: String,
    workspace_slug: String,
    generation: u64,
}

impl Principal {
    pub(crate) const fn new(creator_id: String, workspace_slug: String, generation: u64) -> Self {
        Self {
            creator_id,
            workspace_slug,
            generation,
        }
    }

    pub(crate) const fn verify_generation(&self, expected: u64) -> bool {
        self.generation == expected
    }

    #[must_use]
    pub fn creator_id(&self) -> &str {
        &self.creator_id
    }

    #[must_use]
    pub fn workspace_slug(&self) -> &str {
        &self.workspace_slug
    }
}
