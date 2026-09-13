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

/// Lifecycle phase of a registered cleanup entry (observable for tests).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupEntryPhase {
    Pending,
    Reaped,
}

struct CleanupEntry {
    handle: Option<JoinHandle<()>>,
    state: Arc<EnvState>,
    registered_at: Instant,
    completed: Arc<AtomicBool>,
    phase: CleanupEntryPhase,
}

struct CleanupRegistryInner {
    entries: Vec<CleanupEntry>,
}

static REGISTRY: OnceLock<Mutex<CleanupRegistryInner>> = OnceLock::new();
static REAPER_STARTED: OnceLock<()> = OnceLock::new();

fn registry() -> &'static Mutex<CleanupRegistryInner> {
    REGISTRY.get_or_init(|| Mutex::new(CleanupRegistryInner { entries: Vec::new() }))
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
            entry.phase = CleanupEntryPhase::Reaped;
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
            entry.phase = CleanupEntryPhase::Reaped;
            reg.entries.swap_remove(i);
            reaped += 1;
        } else {
            i += 1;
        }
    }
    reaped
}

/// Register a timed-out finalize worker. The handle is retained until reaped.
pub fn register_pending(
    handle: JoinHandle<()>,
    state: Arc<EnvState>,
    completed: Arc<AtomicBool>,
) {
    ensure_reaper();
    let mut reg = registry().lock().expect("cleanup registry mutex poisoned");
    reg.entries.push(CleanupEntry {
        handle: Some(handle),
        state,
        registered_at: Instant::now(),
        completed,
        phase: CleanupEntryPhase::Pending,
    });
}

/// Snapshot registry for tests and diagnostics.
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

fn ensure_reaper() {
    REAPER_STARTED.get_or_init(|| {
        thread::Builder::new()
            .name("nexus-cleanup-reaper".into())
            .spawn(|| {
                loop {
                    thread::sleep(Duration::from_millis(50));
                    let reaped = reap_completed();
                    if reaped == 0 {
                        let reg = registry().lock().expect("cleanup registry mutex poisoned");
                        if reg.entries.is_empty() {
                            thread::sleep(Duration::from_millis(200));
                        }
                    }
                }
            })
            .expect("spawn cleanup reaper");
    });
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
}
