//! Query parameters for narrative read operations.
// note: world_id is the only required filter; branch and event are optional narrowings.

use serde::{Deserialize, Serialize};

/// note: Query parameters for narrative state lookups.
///
/// Supports filtering by `world_id` (required), and optionally narrowing
/// to a specific timeline branch or event.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NarrativeQuery {
    /// note: World ID — required for all queries.
    pub world_id: String,
    /// note: Filter to a specific branch within the world.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch_id: Option<String>,
    /// note: Filter to a specific event by ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<String>,
    /// note: Include fork ancestry in the result.
    pub include_fork_info: bool,
}

impl NarrativeQuery {
    /// note: Create a new query scoped to the given world.
    #[must_use]
    pub fn new(world_id: &str) -> Self {
        Self {
            world_id: world_id.to_string(),
            branch_id: None,
            event_id: None,
            include_fork_info: false,
        }
    }

    /// note: Narrow to a specific branch.
    #[must_use]
    pub fn with_branch(mut self, branch_id: &str) -> Self {
        self.branch_id = Some(branch_id.to_string());
        self
    }

    /// note: Narrow to a specific event.
    #[must_use]
    pub fn with_event(mut self, event_id: &str) -> Self {
        self.event_id = Some(event_id.to_string());
        self
    }

    /// note: Include fork ancestry information.
    #[must_use]
    pub const fn with_fork_info(mut self) -> Self {
        self.include_fork_info = true;
        self
    }
}
