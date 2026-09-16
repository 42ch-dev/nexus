---
module: nexus-core (actor admission, activity fencing, host authority, execution ownership)
date: 2026-09-17
problem_type: architecture_pattern
category: architecture-patterns
severity: high
plan_id: 2026-09-15-v1.190-p2-actor-memory-context-services
applies_when:
  - migrating a single-writer service authority out of a transport crate into the library core
  - adding a per-entity effect fence (Character/Workspace/run) that must refuse instead of queue
  - making an admitted identity impossible to forge from a serialized payload
  - enforcing one owner per workspace DB across processes and within one process
related_components:
  - nexus-core
  - nexus-daemon-runtime
  - nexus-local-db
  - nexus-agent-host
tags:
  - authority-carrier
  - opaque-token
  - raii-lease
  - non-blocking-fence
  - single-owner
  - actor-isolation
  - core-extraction
---

# Typed authority carriers and fence leases

## Context

v1.190 moved the actor, execution and host authorities out of `nexus-daemon-runtime` into `nexus-core`, behind a single `CoreService` that the daemon HTTP surface, the napi bridge, the TS service and the direct CLI all consume. Three different kinds of "who may do this" had to survive that move without becoming stringly-typed re-checks at each caller:

1. **Identity** — a request may *claim* a Creator/Character pair; only stored rows may *grant* it.
2. **Concurrency** — a Character's DB/file/provider effects must not interleave with its own lifecycle transition, across processes as well as within one.
3. **Ownership** — one workspace DB has exactly one writable engine, and a second `open` must be refused, not silently joined.

Each of these was previously enforced by ad-hoc predicates and locks at the daemon handler layer, which is exactly the layer that stopped being the sole entry point. The pattern that replaced them makes the authorization a *value you must hold*: an opaque token that only the authority can mint, or an RAII guard whose mere existence proves the check happened.

## Guidance

### 1. Mint admitted identity as an opaque, non-serializable value

Admission resolves stored owners once and returns a value that cannot be reconstructed from a request body:

```rust
/// Discriminant admitted from stored owners (never from payload claims).
///
/// Opaque on purpose: no `Serialize`/`Deserialize`, so a serialized context
/// cannot be replayed as authority.
#[derive(Debug, Clone)]
pub enum AdmittedActor {
    Creator { creator_id: String },
    Character { character_id: String },
}
```

The rule that makes this work is not the enum — it is that **no `Deserialize` impl exists**. A serialized `AdmittedActorContext` (the owned, serializable projection carrying owner, World, binding, branch, event, stored `lifecycle_epoch` and a bounded `KnowledgeView`) may cross a wire for display, but it can never re-enter as authorization. The serializable projection and the authority token are deliberately separate types; the projection "grants no authority".

Corollary: a request that carries an Actor pair must be classified as a pair or rejected — a partial pair is a stable 400, never an implicit legacy downgrade.

### 2. Fence effects with a two-layer, non-blocking lease

A per-entity lease that *waits* converts contention into a latency bug; a lease that only lives in-process does not survive a second core opening the same DB. Use both layers, and make both `try_`-only:

```rust
pub struct ActorActivityLease {   // shared: held across every DB/file/provider effect
    _read: OwnedRwLockReadGuard<()>,
    _os: OsSharedLock,
    owner_creator_id: String,
    character_id: String,
    epoch: i64,
}

pub struct CharacterTransitionLease { // exclusive: lifecycle/binding transition
    _write: OwnedRwLockWriteGuard<()>,
    _os: OsExclusiveLock,
    owner_creator_id: String,
    character_id: String,
    epoch: std::sync::atomic::AtomicI64,
}
```

- **Layer 1 — per-service `tokio::sync::RwLock`** (`ActorFenceTable`, a `Mutex<HashMap<String, Arc<RwLock<()>>>>` with live-Arc reclaim). Serializable only within one `CoreService`; it is what makes two leases in the same service contend.
- **Layer 2 — stable per-entity OS lock file** at `<state-db dir>/character_locks/<character_id>.lock`, one descriptor per live lease, via `std::fs::File::try_lock_shared` / `try_lock`. OS locks ride the open file description, so two *independently opened* cores, and a direct CLI transition, contend exactly as two processes would. Standard-library file locking (not a Unix-only `flock(2)` binding) keeps the contract enforced on every supported target instead of degrading to a process-local no-op off Unix.

Both acquisitions are `try_*` and every failure maps to the same retained refusal (a stable `character_busy` code), so a contended admission reports busy promptly instead of parking behind the holder. **A transition never waits out an in-flight effect, and an effect never waits out a transition.**

Two ordering rules make the lease exact rather than merely exclusive:

- **Re-read stored state under the fence.** `acquire_character_transition` takes the fence, *then* re-reads the row, so the epoch carried on the lease is the true pre-transition epoch. A key or transition decision built from a pre-fence read is stale the moment another transition commits between the read and the acquisition.
- **Write the committed epoch back onto the held lease.** `commit_character_transition(&mut lease, …)` updates the lease to the committed epoch so the host can retire old-epoch sessions *while the exclusive fence is still held*. A same-state (no-op) transition must not retire sessions — only a material epoch change does.

The lock file must carry no payload: on Windows, byte-range locks are enforced against other handles, so a fence file that is also opened for content would be self-defeating.

### 3. Make single ownership a reservation with RAII rollback

"One engine per workspace DB" needs a fence that a failed or panicking build cannot leave armed:

```rust
/// A held-but-not-yet-installed owner reservation.
///
/// RAII: dropping the guard without [`Self::install`] frees the slot, so a
/// failed — or panicking — build never fences its own workspace DB.
struct OwnerReservation { key: PathBuf, armed: bool }
```

`claim` prunes slots whose `Weak` handle is gone or settled, inserts `OwnerSlot::Building`, and refuses a live owner; `install` publishes the established owner and disarms; `Drop` removes the slot only when it is still `Building`. The eager release path additionally compares `Arc::ptr_eq` against the current handle so a stale release cannot evict a newer owner that claimed the same DB after this one closed.

Note the deliberate two-level shape: the OS `engine.lock` fences **cross-process** owners, and the process-wide registry fences **same-process** contenders that share one retained guard. Neither alone is sufficient — a per-service slot alone lets a second core over the same file build a second engine.

### 4. Separate effect fencing from session indexing, and give each one owner

Fencing (may this Character be touched now?) and session indexing (which Host session belongs to this Actor?) are different lifetimes and must not share a table:

- `nexus-core/src/actor_fence.rs` — Host-free leases; `P2` owns it.
- `nexus-core/src/actor_sessions.rs` — the process-lifetime registry; `P4` owns it and consumes the P2 lease (`admit_character_activity` delegates to `CoreService::acquire_actor_activity`), so owner, tombstone and `lifecycle_epoch` are indexed exactly once.

A registry that keeps its *own* second fence table duplicates the interlock and is the classic source of deadlock or of a lost §11.3 guarantee.

### 5. Guard a single-process authority with an established-owner slot

For authorities that are started rather than acquired, admit one owner before any async work and release the slot only on confirmed teardown:

```rust
let mut established = self.inner.host_authority_established.lock()…;
if *established { return Err(CoreError::OwnerBusy); }
*established = true;
```

A failed start clears the slot (retry stays possible); a close clears it only when the drain is confirmed — an unconfirmed close deliberately retains the slot, matching the retained-cleanup discipline of the rest of the teardown path.

## Why This Matters

- **Type-level authorization removes a whole class of regression.** When the only way to obtain `AdmittedActor` is stored admission, "did every new handler remember to check ownership?" stops being a review question. A new caller that forgets fails to compile, not in production.
- **Non-blocking fences keep admissions observable.** A queueing fence turns a user-visible 409 into an unbounded wait and makes cancellation unresponsive. Because busy is reported, the caller sees the real state.
- **The OS layer is what makes the guarantee true rather than local.** A single in-process `RwLock` looks correct in every unit test and fails precisely in the multi-process case (service + direct CLI) that motivates it.
- **RAII reservation removes the panic path.** A `bool` flag set before a fallible build and cleared in a success branch leaves the DB permanently fenced when the build fails or panics.

## When to Apply

- Moving a single-writer authority from a transport/host crate into the library core, or adding a second entry point (CLI, TS service, napi) to the same authority.
- Any per-entity effect that must be serialized but must **not** block: refuse-with-busy is a contract, not an implementation detail.
- Making an admitted identity or a stored admission carry authority across a wire — decide explicitly which projection is serializable and never let it be the authority token.
- Establishing "exactly one owner" for a resource that can be opened twice from the same process and from different processes.

## Examples

Authority carriers are consumed by the daemon/napi/CLI adapters; the wire projection is generated so adapters never hand-shape it.

### Before — caller re-derives authority from the payload

```rust
// Handler local: trust the body's actor ids, then re-run ownership predicates
// ad hoc. A new handler that forgets one predicate is silently over-permissive.
let actor = req.actor_ref.unwrap_or_default();
if !db.owns(actor.character_id, principal.creator_id)? { return forbidden(); }
let guard = registry.lock(actor.character_id).await; // blocking: serializes latency
```

### After — hold the admitted token, take the non-blocking lease

```rust
let ctx: AdmittedActorContext = service.admit_actor(&principal, actor_ref, viewpoint).await?;
let lease = service.acquire_actor_activity(&principal, &ctx.actor)?; // busy -> 409, never queues
// lease is held to the terminal effect; the exclusive transition path
// reports the same busy refusal instead of waiting for this effect.
```

Generated wire request/response DTOs remain the transport contract; `AdmittedActorContext` is the owned projection an adapter may serialize for display, and the opaque `AdmittedActor` inside it is the only thing the fence accepts.

## Evidence

- Identity carrier — `crates/nexus-core/src/actors.rs` (`AdmittedActor` with no serde impls, `AdmittedActorContext`, `classify_pair`, `CoreService::{admit_actor, acquire_actor_activity, acquire_character_transition, commit_character_transition, transition_character}`).
- Two-layer fence — `crates/nexus-core/src/actor_fence.rs` (`ActorFenceTable`, `ActorActivityLease`, `CharacterTransitionLease`, `OsSharedLock::try_acquire`, `OsExclusiveLock::try_acquire`, `Drop` unlocking explicitly).
- Fence behaviour pins — `crates/nexus-core/tests/actor_services.rs` (`transition_races_admitted_activity`, `activity_admission_is_busy_while_a_transition_lease_is_held`, `foreign_owner_routes_are_not_found_and_do_not_mutate`, `admission_deny_matrix_world_character_binding_mismatches`).
- Single-owner reservation — `crates/nexus-core/src/execution/lifecycle.rs` (`OWNERS`, `OwnerReservation`, `claim`/`install`/`Drop`, `release_owner_slot` with `Arc::ptr_eq`).
- Established-owner slot — `crates/nexus-core/src/host.rs` (`open_host` admission, failure-clears-slot, confirmed-close release); duplicate-open refusal covered by `crates/nexus-core/tests/host_actor_lifecycle.rs`.
- Single session-index owner — `crates/nexus-core/src/actor_sessions.rs` (`ActorSessionRegistry` delegating activity admission to the P2 lease rather than keeping a second fence table).
- Durable semantics this implements — [specs/actor-product-model.md](../../specs/actor-product-model.md) §11.
