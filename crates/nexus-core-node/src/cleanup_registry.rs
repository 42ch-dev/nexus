//! Process-scoped tracked cleanup for bounded native finalizer workers.
//!
//! When a finalize worker cannot complete within the absolute budget, its
//! `JoinHandle` and `Arc<EnvState>` are registered here — never detached.
//! A process-level reaper periodically joins completed workers and removes
//! entries so successful cleanups do not leak registry slots.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use super::env_state::EnvState;

struct CleanupEntry {
    handle: Option<JoinHandle<()>>,
    /// Ownership retention: the environment stays alive while its registered
    /// cleanup worker is still running. Read by the registry diagnostic.
    #[allow(dead_code)]
    state: Arc<EnvState>,
    completed: Arc<AtomicBool>,
}

struct CleanupRegistryInner {
    entries: Vec<CleanupEntry>,
}

static REGISTRY: OnceLock<Mutex<CleanupRegistryInner>> = OnceLock::new();

const REAPER_IDLE_EXIT: Duration = Duration::from_millis(500);
const REAPER_POLL: Duration = Duration::from_millis(50);

struct ReaperOwner {
    handle: Mutex<Option<JoinHandle<()>>>,
    stop: Arc<AtomicBool>,
}

static REAPER: OnceLock<ReaperOwner> = OnceLock::new();

fn registry() -> &'static Mutex<CleanupRegistryInner> {
    REGISTRY.get_or_init(|| {
        Mutex::new(CleanupRegistryInner {
            entries: Vec::new(),
        })
    })
}

/// Join and remove every completed worker. Returns the number reaped.
pub fn reap_completed() -> usize {
    let mut reg = registry().lock().expect("cleanup registry mutex poisoned");
    let mut reaped = 0usize;
    let mut i = 0;
    while i < reg.entries.len() {
        let entry = &mut reg.entries[i];
        if entry.completed.load(Ordering::SeqCst) {
            if let Some(handle) = entry.handle.take() {
                let _ = handle.join();
            }
            reg.entries.swap_remove(i);
            reaped += 1;
        } else if entry.handle.as_ref().is_some_and(|h| h.is_finished()) {
            if let Some(handle) = entry.handle.take() {
                let _ = handle.join();
            }
            entry.completed.store(true, Ordering::SeqCst);
            reg.entries.swap_remove(i);
            reaped += 1;
        } else {
            i += 1;
        }
    }
    reaped
}

/// Outcome of an explicit reaper stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReaperStop {
    /// The reaper thread exited and was joined inside the deadline.
    pub joined: bool,
    /// Entries still registered when the stop completed. Never discarded.
    pub pending_entries: usize,
}

/// Stop and join the process reaper within `deadline`.
///
/// A confirmed finalize/process cleanup calls this so the reaper does not
/// outlive the finalizer's bounded budget. Registered entries are never
/// discarded: anything still tracked stays in the registry, is reported as
/// `pending_entries`, and a later `ensure_reaper` starts a fresh reaper for it.
#[must_use]
pub fn stop_reaper(deadline: Instant) -> ReaperStop {
    let Some(owner) = REAPER.get() else {
        return ReaperStop {
            joined: true,
            pending_entries: entry_count(),
        };
    };
    owner.stop.store(true, Ordering::SeqCst);

    let handle = owner.handle.lock().ok().and_then(|mut slot| slot.take());
    let mut joined = true;
    if let Some(handle) = handle {
        while !handle.is_finished() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        if handle.is_finished() {
            let _ = handle.join();
            // Only this now-joined reaper consumed the stop flag; reset it so a
            // future reaper for the remaining entries is not born stopped.
            owner.stop.store(false, Ordering::SeqCst);
        } else {
            // Still running: keep the handle (never strand it) and leave the
            // stop flag set so it exits as soon as it next polls.
            if let Ok(mut slot) = owner.handle.lock() {
                *slot = Some(handle);
            }
            joined = false;
        }
    }
    // Collect anything already finished so a stopped reaper cannot strand a
    // worker that has completed.
    reap_completed();
    ReaperStop {
        joined,
        pending_entries: entry_count(),
    }
}

fn entry_count() -> usize {
    registry().lock().map_or(0, |reg| reg.entries.len())
}

/// Register a timed-out LocalSet runtime thread. The handle is retained until reaped.
pub fn register_localset_thread(handle: JoinHandle<()>, state: Arc<EnvState>) {
    register_pending(handle, state, Arc::new(AtomicBool::new(false)));
}

/// Register a timed-out finalize worker. The handle is retained until reaped.
pub fn register_pending(handle: JoinHandle<()>, state: Arc<EnvState>, completed: Arc<AtomicBool>) {
    ensure_reaper();
    let mut reg = registry().lock().expect("cleanup registry mutex poisoned");
    reg.entries.push(CleanupEntry {
        handle: Some(handle),
        state,
        completed,
    });
}

/// The registry is process-global; tests that assert on its contents must not
/// interleave with each other.
#[cfg(test)]
static REGISTRY_TEST_LOCK: Mutex<()> = Mutex::new(());

/// Serialize a registry-asserting test against every other such test.
#[cfg(test)]
pub fn registry_test_lock() -> std::sync::MutexGuard<'static, ()> {
    REGISTRY_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Snapshot registry for tests.
#[cfg(test)]
pub fn registry_snapshot() -> (usize, usize) {
    reap_completed();
    let reg = registry().lock().expect("cleanup registry mutex poisoned");
    let pending = reg.entries.len();
    let with_owners = reg
        .entries
        .iter()
        .filter(|e| e.state.owner_slots_present())
        .count();
    (pending, with_owners)
}

fn join_finished_reaper_if_any() {
    let owner = REAPER.get_or_init(|| ReaperOwner {
        handle: Mutex::new(None),
        stop: Arc::new(AtomicBool::new(false)),
    });
    let mut slot = owner.handle.lock().expect("reaper handle mutex poisoned");
    if let Some(handle) = slot.take() {
        if handle.is_finished() {
            let _ = handle.join();
        } else {
            *slot = Some(handle);
        }
    }
}

fn ensure_reaper() {
    join_finished_reaper_if_any();
    let owner = REAPER.get_or_init(|| ReaperOwner {
        handle: Mutex::new(None),
        stop: Arc::new(AtomicBool::new(false)),
    });
    let mut slot = owner.handle.lock().expect("reaper handle mutex poisoned");
    if slot.is_some() {
        return;
    }
    let stop = owner.stop.clone();
    let handle = thread::Builder::new()
        .name("nexus-cleanup-reaper".into())
        .spawn(move || {
            let mut idle_since: Option<Instant> = None;
            loop {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                thread::sleep(REAPER_POLL);
                let reaped = reap_completed();
                let empty = registry()
                    .lock()
                    .expect("cleanup registry mutex poisoned")
                    .entries
                    .is_empty();
                if !empty {
                    idle_since = None;
                    continue;
                }
                if reaped > 0 {
                    idle_since = None;
                    continue;
                }
                let now = Instant::now();
                if let Some(idle) = idle_since {
                    if now.duration_since(idle) >= REAPER_IDLE_EXIT {
                        break;
                    }
                } else {
                    idle_since = Some(now);
                }
            }
        })
        .expect("spawn cleanup reaper");
    *slot = Some(handle);
}

/// Join a finished reaper thread (test/diagnostics only).
#[cfg(test)]
fn reaper_joined_for_test() -> bool {
    match REAPER.get() {
        None => true,
        Some(owner) => {
            let mut slot = owner.handle.lock().expect("reaper handle mutex poisoned");
            match slot.as_ref() {
                None => true,
                Some(handle) if handle.is_finished() => {
                    let finished = slot.take().expect("reaper handle");
                    finished.join().is_ok()
                }
                Some(_) => false,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use std::time::Duration;

    /// Drain any entry a previous test left behind so counts are local.
    fn drain_registry() {
        for _ in 0..200 {
            reap_completed();
            if registry_snapshot().0 == 0 {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn localset_thread_entry_is_reaped_after_join() {
        let _lock = crate::cleanup_registry::registry_test_lock();
        drain_registry();
        let done = Arc::new(AtomicBool::new(false));
        let flag = done.clone();
        let handle = thread::spawn(move || {
            thread::sleep(Duration::from_millis(15));
            flag.store(true, Ordering::SeqCst);
        });
        let state = Arc::new(EnvState::new());
        register_localset_thread(handle, state);
        let (pending, _) = registry_snapshot();
        assert_eq!(pending, 1, "LocalSet thread must be tracked");

        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            reap_completed();
            let (pending, _) = registry_snapshot();
            if pending == 0 {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        let (pending, _) = registry_snapshot();
        assert_eq!(
            pending, 0,
            "reaper must join and remove LocalSet thread entry"
        );
    }

    #[test]
    fn registry_reaps_completed_worker_and_removes_entry() {
        let _lock = crate::cleanup_registry::registry_test_lock();
        drain_registry();
        let done = Arc::new(AtomicBool::new(false));
        let flag = done.clone();
        let handle = thread::spawn(move || {
            thread::sleep(Duration::from_millis(20));
            flag.store(true, Ordering::SeqCst);
        });
        let state = Arc::new(EnvState::new());
        register_pending(handle, state, done);
        let (pending, _) = registry_snapshot();
        assert_eq!(pending, 1, "entry must be observable as pending");

        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            reap_completed();
            let (pending, _) = registry_snapshot();
            if pending == 0 {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        let (pending, _) = registry_snapshot();
        assert_eq!(pending, 0, "completed entry must be reaped, not permanent");
    }

    /// Q3-W5: the reaper stops and joins on demand inside a bounded deadline,
    /// and stopping it never strands a registered entry.
    #[test]
    fn reaper_stops_and_joins_without_stranding_entries() {
        let _lock = crate::cleanup_registry::registry_test_lock();
        drain_registry();

        // A worker that outlives the stop request.
        let done = Arc::new(AtomicBool::new(false));
        let handle = thread::spawn(|| {
            thread::sleep(Duration::from_millis(300));
        });
        let state = Arc::new(EnvState::new());
        register_pending(handle, state, done);
        assert!(ensure_reaper_running(), "the reaper must be running");

        // An already-expired deadline cannot join a live reaper; the handle must
        // be retained rather than dropped/lost.
        let stop = stop_reaper(Instant::now());
        if !stop.joined {
            assert!(
                ensure_reaper_running() || reaper_handle_present(),
                "an unjoined reaper handle must be retained, never stranded"
            );
        }
        assert_eq!(
            stop.pending_entries, 1,
            "an unfinished worker must remain tracked, never discarded"
        );

        // A later stop with a real budget joins the reaper, and the worker is
        // reaped once it finishes.
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let stop = stop_reaper(deadline);
            if stop.joined && registry_snapshot().0 == 0 {
                break;
            }
            if Instant::now() >= deadline {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(
            registry_snapshot().0,
            0,
            "the finished worker must be reaped, not stranded"
        );
    }

    /// Whether the reaper owner still holds a `JoinHandle` (running or joinable).
    fn reaper_handle_present() -> bool {
        REAPER.get().is_some_and(|owner| {
            owner
                .handle
                .lock()
                .map(|slot| slot.is_some())
                .unwrap_or(false)
        })
    }

    fn ensure_reaper_running() -> bool {
        match REAPER.get() {
            None => false,
            Some(owner) => owner
                .handle
                .lock()
                .map(|slot| slot.as_ref().is_some_and(|h| !h.is_finished()))
                .unwrap_or(false),
        }
    }

    #[test]
    fn reaper_terminates_after_registry_drains() {
        let _lock = crate::cleanup_registry::registry_test_lock();
        drain_registry();
        let done = Arc::new(AtomicBool::new(false));
        let flag = done.clone();
        let handle = thread::spawn(move || {
            thread::sleep(Duration::from_millis(10));
            flag.store(true, Ordering::SeqCst);
        });
        let state = Arc::new(EnvState::new());
        register_pending(handle, state, done);

        let deadline = Instant::now() + Duration::from_secs(3);
        while Instant::now() < deadline {
            reap_completed();
            let (pending, _) = registry_snapshot();
            if pending == 0 && reaper_joined_for_test() {
                return;
            }
            thread::sleep(Duration::from_millis(25));
        }
        let (pending, _) = registry_snapshot();
        assert_eq!(pending, 0, "registry must be empty");
        assert!(
            reaper_joined_for_test(),
            "reaper thread must terminate and be joinable after idle drain"
        );
    }
}
