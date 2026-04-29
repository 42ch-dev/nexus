//! Pure-function admission gate for schedule concurrency (spec §5.1).
//!
//! These types and the [`admit`] function are intentionally free of async /
//! database logic so that unit tests can cover every rule without a DB.
//!
//! Concurrency checks are scoped per-creator: serial schedules for creator A
//! do not block serial schedules for creator B.

use std::collections::{HashMap, HashSet};

#[cfg(test)]
use nexus_contracts::local::schedule::ParallelWithIds;
use nexus_contracts::local::schedule::{Schedule, ScheduleConcurrency, ScheduleId};

/// The set of currently-running [`Schedule`] IDs, keyed by creator.
///
/// Used by [`admit`] to enforce per-creator concurrency rules.
/// Each entry maps `creator_id → set of running schedule IDs`.
#[derive(Debug, Clone, Default)]
pub struct RunningSet {
    /// Creator-scoped running schedule IDs.
    by_creator: HashMap<String, HashSet<ScheduleId>>,
    /// Flat set of all running IDs (for quick `contains` checks).
    all_ids: HashSet<ScheduleId>,
}

impl RunningSet {
    /// Empty running set — no schedules are running.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Build a [`RunningSet`] from a list of [`Schedule`]s.
    ///
    /// Only the IDs are retained; callers are responsible for filtering to
    /// `ScheduleStatus::Running` before calling this.
    #[must_use]
    pub fn from(schedules: Vec<Schedule>) -> Self {
        let mut by_creator: HashMap<String, HashSet<ScheduleId>> = HashMap::new();
        let mut all_ids: HashSet<ScheduleId> = HashSet::new();
        for s in schedules {
            by_creator
                .entry(s.creator_id.clone())
                .or_default()
                .insert(s.id.clone());
            all_ids.insert(s.id);
        }
        Self {
            by_creator,
            all_ids,
        }
    }

    /// Build a [`RunningSet`] directly from a set of `(creator_id, schedule_id)` pairs.
    ///
    /// Used internally by the supervisor to construct a scoped running set.
    pub(crate) fn from_entries(entries: HashSet<(String, ScheduleId)>) -> Self {
        let mut by_creator: HashMap<String, HashSet<ScheduleId>> = HashMap::new();
        let mut all_ids: HashSet<ScheduleId> = HashSet::new();
        for (creator_id, schedule_id) in entries {
            by_creator
                .entry(creator_id)
                .or_default()
                .insert(schedule_id.clone());
            all_ids.insert(schedule_id);
        }
        Self {
            by_creator,
            all_ids,
        }
    }

    /// Returns `true` if no schedules are running for the given creator.
    #[must_use]
    pub fn is_empty_for_creator(&self, creator_id: &str) -> bool {
        self.by_creator
            .get(creator_id)
            .is_none_or(std::collections::HashSet::is_empty)
    }

    /// Returns `true` if no schedules are running at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.all_ids.is_empty()
    }

    /// Returns `true` if the given schedule ID is currently running.
    #[must_use]
    pub fn contains(&self, id: &ScheduleId) -> bool {
        self.all_ids.contains(id)
    }

    /// Returns the set of running schedule IDs for a specific creator.
    fn ids_for_creator(&self, creator_id: &str) -> Vec<&ScheduleId> {
        self.by_creator
            .get(creator_id)
            .map(|ids| ids.iter().collect())
            .unwrap_or_default()
    }
}

/// The set of schedule IDs that have reached a terminal "success" status
/// (Completed or Cancelled) for dependency satisfaction.
///
/// A dependency is considered *satisfied* when its ID appears in this set.
/// `Failed` dependencies do **not** satisfy — they require user intervention
/// (spec §4: "A Failed dependency blocks auto-start").
#[derive(Debug, Clone, Default)]
pub struct CompletedSet {
    ids: HashSet<ScheduleId>,
}

impl CompletedSet {
    /// Empty completed set — no schedules have completed.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Build from a list of completed schedule IDs.
    #[must_use]
    pub fn from(ids: Vec<ScheduleId>) -> Self {
        Self {
            ids: ids.into_iter().collect(),
        }
    }

    /// Returns `true` if the given schedule ID is completed/cancelled.
    #[must_use]
    pub fn contains(&self, id: &ScheduleId) -> bool {
        self.ids.contains(id)
    }
}

/// Evaluate whether `candidate` may be admitted to start running.
///
/// Returns `true` if all three conditions are met:
///
/// 1. **Concurrency rule** (per [`ScheduleConcurrency`] variant):
///    - `Serial`: running set must be empty
///    - `ParallelWith(ids)`: every currently-running schedule must be in `ids`
///      (or running set is empty)
///    - `ParallelAny`: always passes (ACP serialization at dispatch site)
///
/// 2. **Dependency rule**: every entry in `candidate.depends_on` must appear
///    in `completed` (i.e. Completed or Cancelled, not Failed).
///
/// 3. No other constraints at this layer — spec §5.2 ACP single-worker
///    constraint is enforced at the `AcpPromptTask` dispatch site.
#[must_use]
pub fn admit(candidate: &Schedule, running: &RunningSet, completed: &CompletedSet) -> bool {
    // 1. Concurrency gate
    if !check_concurrency(candidate, running) {
        return false;
    }

    // 2. Dependency gate
    for dep in &candidate.depends_on {
        if !completed.contains(dep) {
            return false;
        }
    }

    true
}

fn check_concurrency(candidate: &Schedule, running: &RunningSet) -> bool {
    match &candidate.concurrency {
        ScheduleConcurrency::Serial => running.is_empty_for_creator(&candidate.creator_id),
        ScheduleConcurrency::ParallelWith(whitelist) => {
            // Every running schedule for this creator must be in the whitelist.
            // Schedules from other creators are irrelevant.
            let creator_ids = running.ids_for_creator(&candidate.creator_id);
            creator_ids
                .iter()
                .all(|id| whitelist.schedule_ids.contains(id))
        }
        ScheduleConcurrency::ParallelAny => true, // ACP serialization at dispatch site
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_contracts::local::schedule::{CoreContextVersion, ScheduleStatus};

    fn sched(id: &str, creator_id: &str, c: ScheduleConcurrency, deps: Vec<&str>) -> Schedule {
        Schedule {
            id: ScheduleId(id.to_string()),
            creator_id: creator_id.to_string(),
            preset_id: "p".to_string(),
            preset_version: 1,
            status: ScheduleStatus::Pending,
            concurrency: c,
            depends_on: deps
                .into_iter()
                .map(|d| ScheduleId(d.to_string()))
                .collect(),
            current_core_context_version: CoreContextVersion(0),
            current_session_id: None,
            scheduled_at: None,
            label: None,
            created_at: String::new(),
            updated_at: String::new(),
            terminated_at: None,
        }
    }

    #[test]
    fn serial_admits_when_running_empty() {
        let p = sched("A", "c1", ScheduleConcurrency::Serial, vec![]);
        assert!(admit(&p, &RunningSet::empty(), &CompletedSet::empty()));
    }

    #[test]
    fn serial_blocks_when_same_creator_running() {
        let p = sched("A", "c1", ScheduleConcurrency::Serial, vec![]);
        let r = RunningSet::from(vec![sched("B", "c1", ScheduleConcurrency::Serial, vec![])]);
        assert!(!admit(&p, &r, &CompletedSet::empty()));
    }

    #[test]
    fn serial_allows_different_creator_running() {
        // Creator A serial schedule should not be blocked by creator B's serial schedule
        let p = sched("A", "creator-a", ScheduleConcurrency::Serial, vec![]);
        let r = RunningSet::from(vec![sched(
            "B",
            "creator-b",
            ScheduleConcurrency::Serial,
            vec![],
        )]);
        assert!(admit(&p, &r, &CompletedSet::empty()));
    }

    #[test]
    fn parallel_any_always_admits() {
        let p = sched("A", "c1", ScheduleConcurrency::ParallelAny, vec![]);
        let r = RunningSet::from(vec![sched("B", "c1", ScheduleConcurrency::Serial, vec![])]);
        assert!(admit(&p, &r, &CompletedSet::empty()));
    }

    #[test]
    fn parallel_with_allows_empty_running() {
        let p = sched(
            "A",
            "c1",
            ScheduleConcurrency::ParallelWith(ParallelWithIds {
                schedule_ids: vec![ScheduleId("B".to_string())],
            }),
            vec![],
        );
        assert!(admit(&p, &RunningSet::empty(), &CompletedSet::empty()));
    }

    #[test]
    fn parallel_with_ignores_other_creator_running() {
        // Creator A's ParallelWith should not consider creator B's running schedules
        let p = sched(
            "A",
            "creator-a",
            ScheduleConcurrency::ParallelWith(ParallelWithIds {
                schedule_ids: vec![ScheduleId("X".to_string())],
            }),
            vec![],
        );
        // Creator B has a running schedule "Z" not in the whitelist
        let r = RunningSet::from(vec![sched(
            "Z",
            "creator-b",
            ScheduleConcurrency::Serial,
            vec![],
        )]);
        assert!(admit(&p, &r, &CompletedSet::empty()));
    }

    #[test]
    fn cross_creator_dependency_still_works() {
        // Creator B's schedule can depend on creator A's completed schedule
        let p = sched("B", "creator-b", ScheduleConcurrency::Serial, vec!["A"]);
        let completed = CompletedSet::from(vec![ScheduleId("A".to_string())]);
        assert!(admit(&p, &RunningSet::empty(), &completed));
    }
}
