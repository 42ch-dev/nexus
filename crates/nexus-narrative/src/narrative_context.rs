//! Narrative context read-model types for World/Timeline/Event state.
//!
//! These types provide an aggregated, read-only view of narrative state for
//! consumption by `NarrativeGateway`, `nexus-moment-context-assembly`, and
//! CLI/daemon local APIs. They are projections from domain aggregates — not
//! the authoritative domain types themselves.

use serde::{Deserialize, Serialize};

/// Aggregated narrative context for a given scope.
///
/// Assembles world state, current timeline position, and (optionally) the
/// active event snapshot into a single read-model for context assembly.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NarrativeContext {
    /// World scope for this context.
    pub world: WorldState,
    /// Current timeline position within the world (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeline_position: Option<TimelinePosition>,
    /// Active event snapshot at the current position (if any).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_snapshot: Option<EventSnapshot>,
}

/// Read-model projection of world state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorldState {
    /// World ID.
    pub world_id: String,
    /// World title / name.
    pub title: String,
    /// World slug.
    pub slug: String,
    /// Current status string (e.g. "active", "archived", "paused").
    pub status: String,
    /// Whether this world is a fork of another world.
    pub is_fork: bool,
    /// Fork branch ID, if this world was created by forking.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fork_branch_id: Option<String>,
    /// Parent world ID, if this is a fork.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_world_id: Option<String>,
    /// ID of the event at which this world was forked.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub forked_from_event_id: Option<String>,
    /// Canon revision counter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub canon_revision: Option<u64>,
    /// Current timeline head event ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_timeline_head_id: Option<String>,
    /// Current time pointer event ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_time_pointer: Option<String>,
    /// Creation timestamp (RFC 3339).
    pub created_at: String,
}

/// Read-model projection of a timeline position.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TimelinePosition {
    /// Timeline branch ID.
    pub branch_id: String,
    /// World this timeline belongs to.
    pub world_id: String,
    /// Index / sequence number of the current position in the timeline.
    pub event_index: u64,
    /// Whether this timeline is on a fork branch.
    pub is_fork: bool,
    /// Current event at this position (if resolved).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_event_id: Option<String>,
}

/// Read-model projection of an event snapshot.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EventSnapshot {
    /// Event ID.
    pub event_id: String,
    /// World this event belongs to.
    pub world_id: String,
    /// Branch this event is on.
    pub branch_id: String,
    /// Event type string (e.g. "`story_advance`", "`state_update`").
    pub event_type: String,
    /// Event status string (e.g. "canon", "provisional").
    pub event_status: String,
    /// Sequence number within the branch.
    pub sequence_no: u64,
    /// Event title (if set).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Event summary (if set).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Creation timestamp (RFC 3339).
    pub created_at: String,
}
