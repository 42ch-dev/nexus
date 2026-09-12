//! Opaque stored-authorized principal bound to a service generation.

#[derive(Debug, Clone)]
pub struct Principal {
    creator_id: String,
    workspace_slug: String,
    generation: u64,
}

impl Principal {
    pub(crate) fn new(creator_id: String, workspace_slug: String, generation: u64) -> Self {
        Self {
            creator_id,
            workspace_slug,
            generation,
        }
    }

    pub(crate) fn verify_generation(&self, expected: u64) -> bool {
        self.generation == expected
    }

    pub(crate) fn creator_id(&self) -> &str {
        &self.creator_id
    }
}
