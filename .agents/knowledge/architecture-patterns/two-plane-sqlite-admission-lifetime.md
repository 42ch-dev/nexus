---
module: nexus-local-db (writer protocol) + nexus-storage-guard (connection userdata) + nexus-core (memory provider open/close)
date: 2026-11-07
problem_type: architecture_pattern
category: architecture-patterns
severity: high
applies_when:
  - "An OS-level admission (lock or lease) must outlive individual handles and connections of a pooled resource"
  - "Deciding what a pool close event proves, and whether it can stand in for drained"
  - "Designing a cancellation-safe open that registers an admission before it can deliver a pool"
  - "Reviewing a connection factory that could silently run without the admission its pool was opened under"
related_components:
  - nexus-local-db
  - nexus-storage-guard
  - nexus-core
tags:
  - sqlite-pool-lifetime
  - writer-admission
  - close-event
  - cancellation-safety
  - os-file-locking
  - connection-userdata
  - fail-closed
  - ownership-plane
---

# Two-plane SQLite admission lifetime — open-pool ownership plus per-connection native disposal

## Context

The workspace writer protocol admits writers to one workspace `state.db` through OS advisory locks held as RAII `std::fs::File` handles: a shared migration lock and, for the engine owner, an exclusive engine lock. Those handles live in one `WriterAdmission`; the OS exclusion for that database exists exactly as long as at least one handle in that bundle is alive. Everything that can write through the workspace pool must therefore keep the bundle alive, and it must release it only when no real writer can still run.

A guarded pool is opened from that admission: `pool_with_context` builds the pool, and each connection gets the five writer scalar functions installed by `install_writer_functions` together with the connection-local writer identity. Two independent lifetimes now exist, and they do not coincide:

- **The pool** can be OPEN with zero connections, and a `GuardedPool::clone_pool()` raw handle can outlive the `GuardedPool` that opened it. SQLx recycling (`min_connections = 0`, idle timeout, max lifetime) can dispose the last idle connection of a pool that never closed.
- **A single connection** can outlive close initiation. It can be checked out, detached from the pool, or inside an open transaction while `Pool::close()` is already running, and it stays usable until it is returned and disposed.

Binding the admission to only one of these lifetimes leaves two silent holes. If the admission follows connections only, an open pool that reaps its last connection loses its OS exclusion and can then serve a *new* connection without any admission. If the admission is released when close *starts*, a checked-out connection is still writing while another process wins the exclusive fence and migrates. Both are cross-process safety failures: the next process is admitted, and a migration runs against a database a live writer still owns.

Under the locked sqlx 0.9, a close event cannot be used as the settlement signal. `Pool::close()` is a non-async function that synchronously runs `mark_closed()`, which stores `is_closed` and notifies the close-event listeners; the returned future is the drain (it re-acquires every permit, closes idle connections, then stores `size = 0`). `PoolInner::drop` runs the same `mark_closed()` on the last strong release. So a resolved close event means **close started**, not **drained**. It is the correct cancellation signal and the wrong proof that the OS exclusion may be released.

## Guidance

### 1. Give the admission one owner per real resource lifetime, not one owner for the pool object

Use two planes, each anchored where the underlying resource provably lives.

**Open-pool plane.** `pool_with_context` takes the published `Arc<WorkspaceWriterGuard>` and stores it in a pool-scoped slot (`PoolAdmissionSlot`). The slot is captured by the `after_connect` factory closure, so its lifetime is exactly the pool's `PoolInner` lifetime, independent of how many connections SQLx currently holds. It takes the guard exactly once and retires it exactly once. This is the plane that makes "open pool, zero connections" and "raw clone outlives the handle" safe.

**Native-disposal plane.** Every writer-function registration on a connection clones the same `Arc<WriterAdmission>` into its userdata box. SQLite invokes the registration's destroy callback exactly once — when the connection is disposed, and also when the registration itself fails. The last surviving `Arc` therefore outlives the close start and dies at real connection disposal. That is the plane that makes "checked-out connection during close" safe: dropping the pool slot's guard at close start cannot release the OS locks while any real connection is still alive, because that connection's userdata still holds the same lock handles.

The two planes are not the only holders of the admission, and the document should not be read that way. The OS locks are held by the union of: every `GuardedPool` handle that owns the published guard, the pool slot while the pool is open (retired at close start), every live connection's userdata pin, and an explicitly retained registry entry. The two planes are the two that must exist so that a pool can outlive its opening handle and a connection can outlive a close; the other holders are ordinary claims whose release is the caller's own business.

| State | Still holds the admission? |
|---|---|
| Pool open, zero connections | Yes — the pool slot, and any live handle |
| Close started, one connection checked out and usable | Yes — that connection's userdata pin (the slot has already retired) |
| Close started, no connections left, `GuardedPool` handle still alive | Yes — the handle's own guard |
| Close started, all `GuardedPool` handles dropped, no surviving `SqlitePool` clone, no connections, no retained entry | No — every holder has ended |
| Last `GuardedPool` handle dropped without an explicit `close()` | **Not necessarily released.** A surviving raw `SqlitePool` clone keeps the pool open, so the slot still holds the admission. Once the last strong SQLx handle ends, `PoolInner::drop` runs `mark_closed` → the slot retires, but that is still close start: a detached connection keeps its native pin, and an explicitly retained registry entry keeps its claim. The locks fall only when the last of those holders ends. |
| Pool open, reconnect after zero connections | Yes — same published admission and identity |

`PoolInner::drop` is therefore **not** a release edge, and dropping the last `GuardedPool` handle is not one either. `PoolInner::drop` performs the same `mark_closed` as `Pool::close()` — it retires the pool slot and notifies listeners — while physical connection disposal, and with it the last native pin, happens later at each connection's own teardown (a detached connection can outlive the pool entirely). Dropping the last `GuardedPool` handle only gives up that handle's pool and claim; a surviving raw clone or an explicitly retained admission continues to hold the OS locks. Release is the union ending, never a single drop.

### 2. Retire the open-pool anchor synchronously on close start, without an executor

The slot registers itself on the pool's public close event and retires in the waker:

```rust
struct PoolAdmissionSlot {
    guard: Mutex<Option<Arc<WorkspaceWriterGuard>>>,
    closed: Mutex<Option<Pin<Box<CloseEvent>>>>,
}

impl PoolAdmissionSlot {
    fn retire(&self) {
        let retired = self.guard.lock().expect("slot poisoned").take();
        drop(retired); // drop outside the lock, never awaiting, never panicking
    }

    fn watch(self: &Arc<Self>, pool: &SqlitePool) {
        let waker = Waker::from(Arc::new(PoolCloseWaker(Arc::downgrade(self))));
        let mut cx = Context::from_waker(&waker);
        let mut event = Box::pin(pool.close_event());
        if event.as_mut().poll(&mut cx).is_ready() {
            self.retire(); // the pool was already closed when the slot attached
        }
        *self.closed.lock().expect("slot poisoned") = Some(event);
    }
}

struct PoolCloseWaker(Weak<PoolAdmissionSlot>);

impl std::task::Wake for PoolCloseWaker {
    fn wake(self: Arc<Self>) {
        if let Some(slot) = self.0.upgrade() {
            slot.retire();
        }
    }
}
```

Three properties make this work:

- The close event is polled **once** with a hand-built waker. A `Pending` poll leaves that waker registered inside the listener node, so SQLx's own notification runs `wake` inline on the notifying thread — inside `Pool::close()`, or inside `PoolInner::drop` on whatever thread drops the last handle. No spawn, no driver, no periodic poll, and therefore no "cannot clear an OS anchor after runtime shutdown" hole.
- The waker holds a `Weak` of the slot. The listener cannot keep the slot alive, and the slot never points at the pool, so the ownership graph stays acyclic.
- The slot is also the fallback: if the pool is dropped without ever being closed, dropping the pool drops the factory closure, drops the slot, and retires the guard.

A close event that resolves while the pool is already closed also resolves immediately (`close_event()` yields an already-ready listener when `is_closed` is set), so attaching the slot after a close cannot miss the retire.

### 3. Fail closed at the connection factory

The factory must distinguish three cases and must never collapse them:

```rust
fn pinned_admission(
    slot: Option<&Arc<PoolAdmissionSlot>>,
) -> Result<Option<Arc<WriterAdmission>>, sqlx::Error> {
    match slot.map(|slot| slot.admission()) {
        None => Ok(None),                       // intentional internal pool: no admission
        Some(Some(pinned)) => Ok(Some(pinned)), // open pool: full admission + identity
        Some(None) => Err(sqlx::Error::Protocol( // retired while the pool is still open
            "guarded pool admission was retired while the pool was still open".into(),
        )),
    }
}
```

`None` is reserved for pools that intentionally carry no writer admission (internal protocol bookkeeping). A guarded pool that reaches a retired slot returns an error instead of installing writer functions without the admission the pool was opened under. This is what keeps an ordinary reconnect on an OPEN pool at the published identity rather than silently degrading to an unadmitted connection, and it means "the admission is really gone" is reported, never papered over.

### 4. Keep settlement bookkeeping out of the safety argument

The in-process registry tracks cooperative pools per `(canonical db path, mode)` but deliberately holds **no strong pool**: only the pool's `CloseEvent`, plus a `Weak` view of the live guard (an observed owner) or an explicitly retained guard (the documented raw-retention policy). An admission entry is removed when its claim count reaches zero **and** every cooperative pool for that key is quiescent.

That registry is a liveness and identity mechanism, not the lock-holding mechanism. Because the two planes hold the OS locks, a registry that settles late cannot over-release; the worst case is a short-lived spurious `OwnerBusy` for a migrator, which is fail-closed and bounded by the existing watcher and the migration-time quiescence poll.

Symmetric rule for the close chain: an owner that reports released must actually have closed **its own** pool. A closed-but-still-referenced `SqlitePool` clone must not keep an admission the close already reported released (its registration is withdrawn once the pool can no longer be used), and a dropped-but-unclosed provider pool must not leave a later migration waiting on a pool nobody can use. The provider/store close is therefore explicit and ordered — close the pinned provider's pool first, then this open's own pool, then release this open's admission claim and let the owning handle go.

### 5. Make construction and cancellation leave nothing behind

- Reserve the admission claim together with publication, **before** the awaited pool construction. A failed or cancelled construction then drops the claim instead of leaving an ownerless registry entry holding OS locks.
- If initialization has already built the pool and a later stage fails (for example version seeding), close the pool it just built on the error branch, then let the claim release. Close only the pool this open created; never another open's pool, a joined admission, or a raw-retained pool.
- A dropped store/provider needs no explicit close: dropping its `GuardedPool` handle gives up that handle's pool, and when it was the last handle `PoolInner::drop` marks the pool closed → slot retire, with each connection disposing at its own pace. A surviving raw clone keeps the pool open and therefore correctly keeps the admission.
- When an open joins an existing live admission, it must carry the **published** guard and identity, never substitute a supplied guard that lost the publication race. Retained raw opens promote the published admission to the retained owner rather than parking a claim beside it.

## Why This Matters

Every failure here is silent. Nothing raises an error when an open pool with no connections loses its admission: the next connect succeeds and installs no writer identity. Nothing raises an error when close start releases the OS locks while a checked-out connection writes: the query keeps working, and a second process's migration proceeds under it. The observable symptoms land far from the cause — a refused open with `OwnerBusy`, a migration that "sometimes" runs concurrently, a writer identity that vanishes after an idle period — and none of them points at pool lifetime.

Binding ownership to the real resources also removes a whole class of tempting but wrong fixes: holding a strong pool in the registry (keeps a dead pool alive and delays settlement forever), treating the close event as drained (releases under a live connection), disabling eviction with `min_connections >= 1` (a configuration patch over an ownership gap), or adding a mandatory "close helper" every caller must use (leaves the public raw close unsafe).

## When to Apply

- An OS admission, lease or lock must survive individual handles of a pooled resource: connection pools, cached clients, file watchers, any resource with a close-event API.
- Deciding whether a library's close/close-event completes at *call* or at *drain*. Read the locked source; do not infer it from the name.
- A factory callback (per-connection setup, per-item initialization) must not run without the admission its container was created under.
- Reviewing cancellation between "resource registered" and "resource delivered to its caller".
- A joinable/retained admission: deciding whether a second open reuses the published identity or creates a new one.

## Examples

### Before — close event treated as settlement

```rust
// The registry watcher releases the OS admission when the close event fires.
// Under sqlx 0.9 the event fires when close() is CALLED, while the returned
// future is still draining a checked-out connection.
let closed = pool.close_event();
driver.spawn(async move {
    closed.await;
    settle_admission(&key); // drops the only OS lock handles: too early
});
```

### Before — admission reacquired from the connection count

```rust
// "Quiescent" is inferred from the pool size, so an OPEN pool that reaped its
// last idle connection releases the admission and then serves a new,
// unadmitted connection.
if pool.size() == 0 {
    release_admission(&key);
}
```

### After — two planes, each tied to a real lifetime

```rust
let slot = owner.map(PoolAdmissionSlot::new);      // owns the published guard
let slot_for_factory = slot.clone();
let context_for_hook = context;
let pool = options
    .after_connect(move |conn, _meta| {
        let slot = slot_for_factory.clone();       // ties the slot to the pool
        let context = context_for_hook.clone();
        Box::pin(async move {
            let pinned = pinned_admission(slot.as_ref())?; // fail closed if retired
            install_writer_functions(conn, context, pinned).await
        })
    })
    .connect(&url)
    .await?;
if let Some(slot) = slot.as_ref() {
    slot.watch(&pool);                              // synchronous retire on close start
}
Ok(pool)
```

## Evidence

- Locked sqlx 0.9.0 close semantics — `sqlx-core-0.9.0/src/pool/mod.rs` (`Pool::close`, `close_event`), `src/pool/inner.rs` (`mark_closed`, `PoolInner::close`, `Drop for PoolInner`). `close()` marks closed and notifies immediately; the returned future waits for permits and zeroes `size`.
- Native pin lifecycle — `crates/nexus-storage-guard/src/lib.rs` (`WriterAdmission` holds the original migration/engine lock handles) and `crates/nexus-storage-guard/src/sqlite.rs` (`UserData` carries one `Arc<WriterAdmission>` per registration; `destroy_userdata` runs once per registration, including creation failure; debug `live_registration_count` proves the arithmetic).
- Open-pool plane — `crates/nexus-local-db/src/writer_protocol.rs` (`PoolAdmissionSlot`, `PoolCloseWaker` implementing `std::task::Wake`, `pinned_admission`, `pool_with_context` wiring the slot into `after_connect` and calling `watch` after connect).
- Registry relationship — same file: `RegisteredPool` (close event only), `AdmissionOwner::{Observed, Retained}`, `registration_open`, `admission_pools_quiesced`, `settle_released_admission`, `reserve_retained_admission`, `release_retained_writer_guards`.
- Construction and close chains — same file (`open_guarded_pool_with_claim`, `init_engine_pool`, `init_guarded_pool`, `Drop for GuardedPool`); `crates/nexus-core/src/memory_provider/storage.rs` (`MemoryStore::open`, `compose`, `close`); `crates/nexus-core/src/memory_provider/mod.rs` (`LocalMemoryProvider::open`, `close`); `crates/nexus-core/src/memory_binding.rs` and `crates/nexus-core/src/service.rs` (binding close, then this open's own pool, then claim release).
- Fixture coverage, not a run result — `crates/nexus-local-db/tests/writer_protocol.rs`: `raw_clone_close_keeps_both_admissions_until_connection_disposed`, `open_pool_keeps_its_admission_with_zero_connections`, `usable_pool_clone_blocks_another_process_until_settled`, `raw_process_retention_holds_until_explicit_release`, `joined_raw_open_retains_the_published_admission`, `raw_retention_keeps_the_published_identity`. `crates/nexus-core/tests/memory_provider_binding.rs`: `confirmed_close_releases_the_local_providers_direct_admission`, `cancelled_provider_open_after_seed_releases_its_admission`, `failed_provisioning_keeps_an_adopted_user_and_releases_its_own_admission`, `pending_guarded_migration_runs_after_a_normal_provider_close`, `successful_direct_writer_close_keeps_a_live_engine_owner_joinable`, `creator_close_keeps_a_joined_adopter_and_a_third_join`.

## Coverage gaps

- The synchronous retire relies on the locked `event-listener` release invoking registered wakers inline during notify. A dependency upgrade must re-verify that property rather than assume it.
- Cross-process exclusion is OS advisory locking on the lock files; it is not a claim about filesystems that do not honor it.
- Registry settlement can lag real disposal (watcher scheduling, or the next migration-time quiescence poll). That window produces a bounded, fail-closed spurious `OwnerBusy`, not an unsafe release; it is not characterized as zero-latency.
- Not covered: hard process kill (`SIGKILL`) and installed-deployment shutdown, where no Rust destructor runs and the OS reclaims descriptors on exit.
- The native plane assumes SQLite's documented exactly-once destroy callback per registration; a different SQLite build or FFI wrapper must re-establish that contract.
- Retained raw admissions are an explicit policy, not a lifetime guarantee: they hold the OS locks until `release_retained_writer_guards` or a guarded migration clears them, by design.
