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
        } else if entry
            .handle
            .as_ref()
            .is_some_and(|h| h.is_finished())
        {
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
                if idle_since.is_none() {
                    idle_since = Some(now);
                } else if now.duration_since(idle_since.unwrap()) >= REAPER_IDLE_EXIT {
                    break;
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

    #[test]
    fn registry_reaps_completed_worker_and_removes_entry() {
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

    #[test]
    fn reaper_terminates_after_registry_drains() {
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
