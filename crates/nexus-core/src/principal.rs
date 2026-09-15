//! Opaque stored-authorized principal bound to a service generation.
//!
//! `generation` is open-scoped: it equals the minting service's generation
//! (always `1` today — see `CoreInner::generation`) and never invalidates a
//! principal by itself. Stale selections are rejected by the service's disk
//! re-read (`verify_selected_context`), not by generation comparison.
#[derive(Debug, Clone)]
pub struct Principal {
    creator_id: String,
    workspace_slug: String,
    /// Open-scoped minting generation; see module docs. Never bumped.
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
