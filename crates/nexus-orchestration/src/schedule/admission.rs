//! Pure-function admission gate for schedule concurrency (spec §5.1).
//!
//! These types and the [`admit`] function are intentionally free of async /
//! database logic so that unit tests can cover every rule without a DB.

use std::collections::HashSet;

use nexus_contracts::local::schedule::{Schedule, ScheduleConcurrency, ScheduleId};
#[cfg(test)]
use nexus_contracts::local::schedule::ParallelWithIds;

/// The set of currently-running [`Schedule`] IDs for a single creator.
///
/// Used by [`admit`] to enforce concurrency rules.
#[derive(Debug, Clone, Default)]
pub struct RunningSet {
    /// Schedule IDs that are currently `Running` for this creator.
    ids: HashSet<ScheduleId>,
}

impl RunningSet {
    /// Empty running set — no schedules are running.
    pub fn empty() -> Self {
        Self::default()
    }

    /// Build a [`RunningSet`] from a list of [`Schedule`]s.
    ///
    /// Only the IDs are retained; callers are responsible for filtering to
    /// `ScheduleStatus::Running` before calling this.
    pub fn from(schedules: Vec<Schedule>) -> Self {
        Self {
            ids: schedules.into_iter().map(|s| s.id).collect(),
        }
    }

    /// Build a [`RunningSet`] directly from a set of IDs (crate-internal).
    pub(crate) fn from_ids(ids: HashSet<ScheduleId>) -> Self {
        Self { ids }
    }

    /// Returns `true` if no schedules are running.
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    /// Returns `true` if the given schedule ID is currently running.
    pub fn contains(&self, id: &ScheduleId) -> bool {
        self.ids.contains(id)
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
    pub fn empty() -> Self {
        Self::default()
    }

    /// Build from a list of completed schedule IDs.
    pub fn from(ids: Vec<ScheduleId>) -> Self {
        Self {
            ids: ids.into_iter().collect(),
        }
    }

    /// Returns `true` if the given schedule ID is completed/cancelled.
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
pub fn admit(
    candidate: &Schedule,
    running: &RunningSet,
    completed: &CompletedSet,
) -> bool {
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
        ScheduleConcurrency::Serial => running.is_empty(),
        ScheduleConcurrency::ParallelWith(whitelist) => {
            // Every running schedule must be in the whitelist.
            // If nothing is running, the constraint is vacuously true.
            running.ids.iter().all(|id| whitelist.schedule_ids.contains(id))
        }
        ScheduleConcurrency::ParallelAny => true, // ACP serialization at dispatch site
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_contracts::local::schedule::{CoreContextVersion, ScheduleStatus};

    fn sched(id: &str, c: ScheduleConcurrency, deps: Vec<&str>) -> Schedule {
        Schedule {
            id: ScheduleId(id.to_string()),
            creator_id: "c".to_string(),
            preset_id: "p".to_string(),
            preset_version: 1,
            status: ScheduleStatus::Pending,
            concurrency: c,
            depends_on: deps.into_iter().map(|d| ScheduleId(d.to_string())).collect(),
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
        let p = sched("A", ScheduleConcurrency::Serial, vec![]);
        assert!(admit(&p, &RunningSet::empty(), &CompletedSet::empty()));
    }

    #[test]
    fn serial_blocks_when_any_running() {
        let p = sched("A", ScheduleConcurrency::Serial, vec![]);
        let r = RunningSet::from(vec![sched("B", ScheduleConcurrency::Serial, vec![])]);
        assert!(!admit(&p, &r, &CompletedSet::empty()));
    }

    #[test]
    fn parallel_any_always_admits() {
        let p = sched("A", ScheduleConcurrency::ParallelAny, vec![]);
        let r = RunningSet::from(vec![sched("B", ScheduleConcurrency::Serial, vec![])]);
        assert!(admit(&p, &r, &CompletedSet::empty()));
    }

    #[test]
    fn parallel_with_allows_empty_running() {
        let p = sched(
            "A",
            ScheduleConcurrency::ParallelWith(ParallelWithIds {
                schedule_ids: vec![ScheduleId("B".to_string())],
            }),
            vec![],
        );
        assert!(admit(&p, &RunningSet::empty(), &CompletedSet::empty()));
    }
}
