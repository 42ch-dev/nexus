//! Narrative gateway — read-only trait for querying narrative state.
//!
//! The `NarrativeGateway` trait defines the read API for World/Timeline/Event
//! narrative state. It is consumed by context assembly and CLI/daemon local
//! APIs. Write/mutation operations are explicitly out of scope for V1.23.
//!
//! The trait accepts a `KbStore` type parameter so that world-scoped KB
//! assets can be queried alongside narrative state without coupling the
//! narrative crate to a concrete KB backend.

use crate::errors::NarrativeError;
use crate::fork_branch::ForkBranch;
use crate::narrative_context::{
    EventSnapshot, NarrativeContext, TimelinePosition, WorldState,
};
use crate::narrative_query::NarrativeQuery;
use crate::timeline_event::TimelineEvent;
use crate::world::World;
use nexus_kb::KbStore;
use std::collections::HashMap;
use std::sync::RwLock;

/// Read-only gateway for narrative state queries.
///
/// All methods are async to accommodate eventual storage backends.
/// The trait uses `#[allow(async_fn_in_trait)]` consistent with
/// `KbStore` in `nexus-kb`.
#[allow(async_fn_in_trait)]
pub trait NarrativeGateway {
    /// Get the current world state projection by world ID.
    async fn get_world_state(&self, world_id: &str) -> Result<WorldState, NarrativeError>;

    /// Get timeline events for a world, optionally filtered by branch.
    async fn get_timeline(
        &self,
        world_id: &str,
        branch_id: Option<&str>,
    ) -> Result<Vec<TimelineEvent>, NarrativeError>;

    /// Get a single event by its ID.
    async fn get_event(&self, event_id: &str) -> Result<TimelineEvent, NarrativeError>;

    /// Assemble aggregated narrative context for the given query.
    ///
    /// This is the primary entry point for context assembly: it resolves
    /// world state, current timeline position, and active event into a
    /// single [`NarrativeContext`].
    async fn get_narrative_context(
        &self,
        query: &NarrativeQuery,
    ) -> Result<NarrativeContext, NarrativeError>;

    /// List all worlds.
    async fn list_worlds(&self) -> Result<Vec<WorldState>, NarrativeError>;
}

/// In-memory narrative gateway for testing and development.
///
/// Stores worlds, timeline events, and fork branches in `HashMap`s behind
/// [`RwLock`] for thread safety. Accepts a [`KbStore`] implementation for
/// world-scoped KB asset queries.
pub struct InMemoryNarrativeGateway<K: KbStore> {
    worlds: RwLock<HashMap<String, World>>,
    events: RwLock<HashMap<String, TimelineEvent>>,
    forks: RwLock<HashMap<String, ForkBranch>>,
    /// KB store integration point — wired for future context enrichment
    /// (querying world-scoped KB assets alongside narrative state).
    /// Currently unused in read-only V1.23 gateway methods but kept as a
    /// required constructor parameter to ensure callers provide it.
    #[allow(dead_code)]
    kb_store: K,
}

impl<K: KbStore> InMemoryNarrativeGateway<K> {
    /// Create a new empty gateway backed by the given KB store.
    #[must_use]
    pub fn new(kb_store: K) -> Self {
        Self {
            worlds: RwLock::new(HashMap::new()),
            events: RwLock::new(HashMap::new()),
            forks: RwLock::new(HashMap::new()),
            kb_store,
        }
    }

    /// Insert a world into the gateway (test helper).
    #[allow(clippy::missing_panics_doc)]
    pub fn insert_world(&self, world: World) {
        let id = world.world_id.clone();
        let mut worlds = self.worlds.write().expect("worlds write lock");
        worlds.insert(id, world);
    }

    /// Insert a timeline event into the gateway (test helper).
    #[allow(clippy::missing_panics_doc)]
    pub fn insert_event(&self, event: TimelineEvent) {
        let id = event.timeline_event_id.clone();
        let mut events = self.events.write().expect("events write lock");
        events.insert(id, event);
    }

    /// Insert a fork branch into the gateway (test helper).
    #[allow(clippy::missing_panics_doc)]
    pub fn insert_fork(&self, fork: ForkBranch) {
        let id = fork.fork_branch_id.clone();
        let mut forks = self.forks.write().expect("forks write lock");
        forks.insert(id, fork);
    }

    /// Read lock on worlds.
    fn read_worlds(
        &self,
    ) -> Result<std::sync::RwLockReadGuard<'_, HashMap<String, World>>, NarrativeError> {
        self.worlds
            .read()
            .map_err(|e| NarrativeError::ValidationError(format!("lock error: {e}")))
    }

    /// Read lock on events.
    fn read_events(
        &self,
    ) -> Result<std::sync::RwLockReadGuard<'_, HashMap<String, TimelineEvent>>, NarrativeError>
    {
        self.events
            .read()
            .map_err(|e| NarrativeError::ValidationError(format!("lock error: {e}")))
    }

    /// Read lock on forks.
    fn read_forks(
        &self,
    ) -> Result<std::sync::RwLockReadGuard<'_, HashMap<String, ForkBranch>>, NarrativeError>
    {
        self.forks
            .read()
            .map_err(|e| NarrativeError::ValidationError(format!("lock error: {e}")))
    }

    /// Resolve fork info for a world (if it is a fork child).
    fn resolve_fork_info(&self, world: &World) -> (bool, Option<ForkBranch>) {
        if let Some(ref fork_id) = world.root_fork_branch_id {
            if let Ok(forks) = self.forks.read() {
                if let Some(fork) = forks.get(fork_id) {
                    return (true, Some(fork.clone()));
                }
            }
        }
        (false, None)
    }

    /// Project a [`World`] domain aggregate into a [`WorldState`] read model.
    fn project_world_state(&self, world: &World, include_fork: bool) -> WorldState {
        let (is_fork, fork_branch_id, parent_world_id, forked_from_event_id) = if include_fork {
            let (forked, fork_opt) = self.resolve_fork_info(world);
            if let Some(fork) = fork_opt {
                (
                    forked,
                    Some(fork.fork_branch_id),
                    Some(fork.parent_world_id),
                    Some(fork.forked_from_event_id),
                )
            } else {
                (false, None, None, None)
            }
        } else {
            (false, None, None, None)
        };

        WorldState {
            world_id: world.world_id.clone(),
            title: world.title.clone(),
            slug: world.slug.clone(),
            status: world.status.clone(),
            is_fork,
            fork_branch_id,
            parent_world_id,
            forked_from_event_id,
            canon_revision: world.canon_revision,
            current_timeline_head_id: world.current_timeline_head_id.clone(),
            current_time_pointer: world.current_time_pointer.clone(),
            created_at: world.created_at.clone(),
        }
    }

    /// Project a [`TimelineEvent`] into an [`EventSnapshot`].
    fn project_event_snapshot(event: &TimelineEvent) -> EventSnapshot {
        EventSnapshot {
            event_id: event.timeline_event_id.clone(),
            world_id: event.world_id.clone(),
            branch_id: event.branch_id.clone(),
            event_type: event.event_type.clone(),
            event_status: event.status.clone(),
            sequence_no: event.sequence_no,
            title: event.title.clone(),
            summary: event.summary.clone(),
            created_at: event.created_at.clone(),
        }
    }
}

#[allow(clippy::future_not_send)]
#[allow(clippy::significant_drop_tightening)]
impl<K: KbStore> NarrativeGateway for InMemoryNarrativeGateway<K> {
    // Note: significant_drop_tightening is allowed because project_world_state
    // acquires a separate forks lock (not the worlds lock), and the guard
    // lifetimes are safe even if clippy cannot verify this statically.
    async fn get_world_state(&self, world_id: &str) -> Result<WorldState, NarrativeError> {
        let state = {
            let worlds = self.read_worlds()?;
            let world = worlds
                .get(world_id)
                .ok_or_else(|| {
                    NarrativeError::ValidationError(format!("world not found: {world_id}"))
                })?;
            self.project_world_state(world, true)
        };
        Ok(state)
    }

    async fn get_timeline(
        &self,
        world_id: &str,
        branch_id: Option<&str>,
    ) -> Result<Vec<TimelineEvent>, NarrativeError> {
        let result = {
            let events = self.read_events()?;
            let mut result: Vec<TimelineEvent> = events
                .values()
                .filter(|e| {
                    if e.world_id != world_id {
                        return false;
                    }
                    if let Some(bid) = branch_id {
                        if e.branch_id != bid {
                            return false;
                        }
                    }
                    true
                })
                .cloned()
                .collect();
            result.sort_by_key(|e| e.sequence_no);
            result
        };
        Ok(result)
    }

    async fn get_event(&self, event_id: &str) -> Result<TimelineEvent, NarrativeError> {
        let events = self.read_events()?;
        events
            .get(event_id)
            .cloned()
            .ok_or_else(|| {
                NarrativeError::ValidationError(format!("event not found: {event_id}"))
            })
    }

    async fn get_narrative_context(
        &self,
        query: &NarrativeQuery,
    ) -> Result<NarrativeContext, NarrativeError> {
        // Phase 1: resolve world state (drop lock before continuing)
        let (world_state, timeline_head_id) = {
            let worlds = self.read_worlds()?;
            let world = worlds
                .get(&query.world_id)
                .ok_or_else(|| {
                    NarrativeError::ValidationError(format!(
                        "world not found: {}",
                        query.world_id
                    ))
                })?;
            let head = world.current_timeline_head_id.clone();
            let state = self.project_world_state(world, query.include_fork_info);
            (state, head)
        };

        // Phase 2: resolve timeline position
        let timeline_position = if let Some(ref branch_id) = query.branch_id {
            let (max_seq, current_event_id, is_fork) = {
                let events = self.read_events()?;
                let branch_events: Vec<&TimelineEvent> = events
                    .values()
                    .filter(|e| e.world_id == query.world_id && e.branch_id == *branch_id)
                    .collect();

                if branch_events.is_empty() {
                    (None, None, false)
                } else {
                    let max = branch_events.iter().map(|e| e.sequence_no).max().unwrap_or(0);
                    let cur = branch_events
                        .iter()
                        .find(|e| e.sequence_no == max)
                        .map(|e| e.timeline_event_id.clone());

                    let forks = self.read_forks()?;
                    let fork = forks
                        .values()
                        .any(|f| f.world_id == query.world_id && f.fork_branch_id == *branch_id);

                    (Some(max), cur, fork)
                }
            };

            max_seq.map(|idx| TimelinePosition {
                branch_id: branch_id.clone(),
                world_id: query.world_id.clone(),
                event_index: idx,
                is_fork,
                current_event_id,
            })
        } else if let Some(ref head_id) = timeline_head_id {
            let events = self.read_events()?;
            events.get(head_id).map(|e| TimelinePosition {
                branch_id: e.branch_id.clone(),
                world_id: query.world_id.clone(),
                event_index: e.sequence_no,
                is_fork: false,
                current_event_id: Some(e.timeline_event_id.clone()),
            })
        } else {
            None
        };

        // Phase 3: resolve event snapshot
        let event_snapshot = if let Some(ref event_id) = query.event_id {
            let events = self.read_events()?;
            events.get(event_id).map(Self::project_event_snapshot)
        } else if let Some(ref pos) = timeline_position {
            if let Some(ref eid) = pos.current_event_id {
                let events = self.read_events()?;
                events.get(eid).map(Self::project_event_snapshot)
            } else {
                None
            }
        } else {
            None
        };

        Ok(NarrativeContext {
            world: world_state,
            timeline_position,
            event_snapshot,
        })
    }

    async fn list_worlds(&self) -> Result<Vec<WorldState>, NarrativeError> {
        let result = {
            let worlds = self.read_worlds()?;
            worlds
                .values()
                .map(|w| self.project_world_state(w, false))
                .collect::<Vec<_>>()
        };
        Ok(result)
    }
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline_event::TimelineEventType;
    use nexus_contracts::{TimePolicy, Visibility};

    /// Helper: create a test world.
    fn make_world(id: &str, title: &str) -> World {
        World::new(
            id,
            "ctr_test",
            title,
            &title.to_lowercase().replace(' ', "-"),
            Visibility::Private,
            TimePolicy::Manual,
        )
    }

    /// Helper: create a test event on a branch.
    fn make_event(world_id: &str, branch_id: &str, seq: u64) -> TimelineEvent {
        TimelineEvent::new(
            world_id,
            branch_id,
            TimelineEventType::StoryAdvance,
            seq,
        )
    }

    // T1: get_world_state returns projected world state
    #[tokio::test]
    async fn test_get_world_state() {
        let gw = InMemoryNarrativeGateway::new(nexus_kb::InMemoryKbStore::new());
        let world = make_world("wld_1", "Test World");
        gw.insert_world(world);

        let state = gw.get_world_state("wld_1").await.unwrap();
        assert_eq!(state.world_id, "wld_1");
        assert_eq!(state.title, "Test World");
        assert_eq!(state.status, "active");
        assert!(!state.is_fork);
    }

    // T2: get_world_state returns error for missing world
    #[tokio::test]
    async fn test_get_world_state_not_found() {
        let gw = InMemoryNarrativeGateway::new(nexus_kb::InMemoryKbStore::new());
        let result = gw.get_world_state("wld_missing").await;
        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), NarrativeError::ValidationError(msg) if msg.contains("not found"))
        );
    }

    // T3: get_timeline returns events sorted by sequence
    #[tokio::test]
    async fn test_get_timeline_sorted() {
        let gw = InMemoryNarrativeGateway::new(nexus_kb::InMemoryKbStore::new());
        gw.insert_event(make_event("wld_1", "fbk_root", 3));
        gw.insert_event(make_event("wld_1", "fbk_root", 1));
        gw.insert_event(make_event("wld_1", "fbk_root", 2));

        let timeline = gw.get_timeline("wld_1", None).await.unwrap();
        assert_eq!(timeline.len(), 3);
        assert_eq!(timeline[0].sequence_no, 1);
        assert_eq!(timeline[1].sequence_no, 2);
        assert_eq!(timeline[2].sequence_no, 3);
    }

    // T4: get_timeline filters by branch
    #[tokio::test]
    async fn test_get_timeline_branch_filter() {
        let gw = InMemoryNarrativeGateway::new(nexus_kb::InMemoryKbStore::new());
        gw.insert_event(make_event("wld_1", "fbk_root", 1));
        gw.insert_event(make_event("wld_1", "fbk_fork", 1));

        let root = gw.get_timeline("wld_1", Some("fbk_root")).await.unwrap();
        assert_eq!(root.len(), 1);
        assert_eq!(root[0].branch_id, "fbk_root");
    }

    // T5: get_event returns single event
    #[tokio::test]
    async fn test_get_event() {
        let gw = InMemoryNarrativeGateway::new(nexus_kb::InMemoryKbStore::new());
        let event = make_event("wld_1", "fbk_root", 1);
        let id = event.timeline_event_id.clone();
        gw.insert_event(event);

        let fetched = gw.get_event(&id).await.unwrap();
        assert_eq!(fetched.timeline_event_id, id);
        assert_eq!(fetched.sequence_no, 1);
    }

    // T6: get_event returns error for missing event
    #[tokio::test]
    async fn test_get_event_not_found() {
        let gw = InMemoryNarrativeGateway::new(nexus_kb::InMemoryKbStore::new());
        let result = gw.get_event("evt_missing").await;
        assert!(result.is_err());
    }

    // T7: list_worlds returns all stored worlds
    #[tokio::test]
    async fn test_list_worlds() {
        let gw = InMemoryNarrativeGateway::new(nexus_kb::InMemoryKbStore::new());
        gw.insert_world(make_world("wld_1", "World One"));
        gw.insert_world(make_world("wld_2", "World Two"));

        let worlds = gw.list_worlds().await.unwrap();
        assert_eq!(worlds.len(), 2);
    }

    // T8: get_narrative_context assembles world + timeline + event
    #[tokio::test]
    async fn test_get_narrative_context_full() {
        let gw = InMemoryNarrativeGateway::new(nexus_kb::InMemoryKbStore::new());
        let mut world = make_world("wld_1", "Test World");
        let event = make_event("wld_1", "fbk_root", 1);
        world.current_timeline_head_id = Some(event.timeline_event_id.clone());
        gw.insert_world(world);
        gw.insert_event(event.clone());

        let query = NarrativeQuery::new("wld_1")
            .with_branch("fbk_root")
            .with_event(&event.timeline_event_id);

        let ctx = gw.get_narrative_context(&query).await.unwrap();
        assert_eq!(ctx.world.world_id, "wld_1");
        assert!(ctx.timeline_position.is_some());
        assert!(ctx.event_snapshot.is_some());
        assert_eq!(
            ctx.event_snapshot.unwrap().event_id,
            event.timeline_event_id
        );
    }

    // T9: get_narrative_context with only world_id returns world state
    #[tokio::test]
    async fn test_get_narrative_context_world_only() {
        let gw = InMemoryNarrativeGateway::new(nexus_kb::InMemoryKbStore::new());
        gw.insert_world(make_world("wld_1", "Test World"));

        let query = NarrativeQuery::new("wld_1");
        let ctx = gw.get_narrative_context(&query).await.unwrap();
        assert_eq!(ctx.world.world_id, "wld_1");
        assert!(ctx.timeline_position.is_none());
        assert!(ctx.event_snapshot.is_none());
    }

    // T10: get_narrative_context includes fork info when requested
    #[tokio::test]
    async fn test_get_narrative_context_with_fork_info() {
        let gw = InMemoryNarrativeGateway::new(nexus_kb::InMemoryKbStore::new());

        // Create parent world
        gw.insert_world(make_world("wld_parent", "Parent World"));

        // Create child world via fork
        let parent = {
            let worlds = gw.read_worlds().unwrap();
            worlds.get("wld_parent").unwrap().clone()
        };
        let (child, fork) = parent.fork("ctr_test", "evt_100").unwrap();
        let child_id = child.world_id.clone();
        gw.insert_world(child);
        gw.insert_fork(fork);

        let query = NarrativeQuery::new(&child_id).with_fork_info();
        let ctx = gw.get_narrative_context(&query).await.unwrap();

        assert!(ctx.world.is_fork);
        assert_eq!(ctx.world.parent_world_id.as_deref(), Some("wld_parent"));
    }

    // T11: TimelinePosition correctly identifies fork branches
    #[tokio::test]
    async fn test_timeline_position_fork_detection() {
        let gw = InMemoryNarrativeGateway::new(nexus_kb::InMemoryKbStore::new());

        gw.insert_world(make_world("wld_parent", "Parent World"));
        let parent = {
            let worlds = gw.read_worlds().unwrap();
            worlds.get("wld_parent").unwrap().clone()
        };
        let (child, fork) = parent.fork("ctr_test", "evt_100").unwrap();
        let child_id = child.world_id.clone();
        let fork_branch_id = fork.fork_branch_id.clone();
        gw.insert_world(child);
        gw.insert_fork(fork);

        // Add an event on the fork branch
        let event = make_event(&child_id, &fork_branch_id, 1);
        gw.insert_event(event);

        let query = NarrativeQuery::new(&child_id).with_branch(&fork_branch_id);
        let ctx = gw.get_narrative_context(&query).await.unwrap();

        let pos = ctx.timeline_position.unwrap();
        assert!(pos.is_fork);
        assert_eq!(pos.branch_id, fork_branch_id);
    }

    // T12: get_narrative_context returns error for missing world
    #[tokio::test]
    async fn test_get_narrative_context_missing_world() {
        let gw = InMemoryNarrativeGateway::new(nexus_kb::InMemoryKbStore::new());
        let query = NarrativeQuery::new("wld_missing");
        let result = gw.get_narrative_context(&query).await;
        assert!(result.is_err());
    }

    // T13: World state projection preserves key fields
    #[tokio::test]
    async fn test_world_state_projection_fields() {
        let gw = InMemoryNarrativeGateway::new(nexus_kb::InMemoryKbStore::new());
        let mut world = make_world("wld_1", "Detailed World");
        world.canon_revision = Some(5);
        world.current_time_pointer = Some("evt_42".to_string());
        gw.insert_world(world);

        let state = gw.get_world_state("wld_1").await.unwrap();
        assert_eq!(state.canon_revision, Some(5));
        assert_eq!(state.current_time_pointer.as_deref(), Some("evt_42"));
        assert_eq!(state.title, "Detailed World");
    }

    // T14: Event snapshot projection preserves key fields
    #[tokio::test]
    async fn test_event_snapshot_projection() {
        let gw = InMemoryNarrativeGateway::new(nexus_kb::InMemoryKbStore::new());
        let mut event = make_event("wld_1", "fbk_root", 7);
        event.title = Some("The Battle".to_string());
        event.summary = Some("A great battle occurred".to_string());
        let id = event.timeline_event_id.clone();
        gw.insert_event(event);

        let fetched = gw.get_event(&id).await.unwrap();
        assert_eq!(fetched.title.as_deref(), Some("The Battle"));
        assert_eq!(
            fetched.summary.as_deref(),
            Some("A great battle occurred")
        );
        assert_eq!(fetched.sequence_no, 7);
    }
}
