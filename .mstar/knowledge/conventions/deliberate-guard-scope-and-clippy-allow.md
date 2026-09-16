---
module: rust-workspace (nexus-core, nexus-agent-host, nexus-local-db guard sites)
date: 2026-09-17
problem_type: convention
category: conventions
severity: medium
plan_id: 2026-09-15-v1.190-p3-execution-authority-services
applies_when:
  - a lock guard or RAII guard must be held across an entire operation and clippy flags significant_drop_tightening
  - clearing a pedantic clippy finding under `cargo clippy --all --all-targets -- -D warnings`
  - adding an #[allow] and choosing between fn / statement / module / crate scope
  - reviewing whether a lint suppression hides a real contention or lock-order bug
tags:
  - clippy
  - significant-drop-tightening
  - lock-guard
  - lint-suppression
  - allow-with-reason
  - rust
---

# Deliberately wide guard scopes and lint suppression

## Context

`cargo clippy --all --all-targets -- -D warnings` is a hard CI gate in this repo, with `pedantic` + `nursery` enabled workspace-wide. The `nursery` lint `significant_drop_tightening` fires whenever a guard-like temporary is dropped later than clippy believes necessary:

```
warning: temporary with significant `Drop` can be early dropped
  --> src/lib.rs:7:9
   |
 7 |     let g = M.lock().unwrap();
   |         ^ temporary `g` is currently being dropped at the end of its contained scope
   = note: this might lead to unnecessary resource contention
   = help: for further information visit …#significant_drop_tightening
```

For hot paths the suggestion is right and should be taken. But in an authority/registry layer the guard very often **must** span the whole operation — dropping it early would release a fence, an ownership claim or an atomic section mid-way and break an invariant. Those sites cannot be refactored to satisfy the lint, so the finding has to be suppressed explicitly. Getting the suppression *scope* wrong is the failure mode this note records.

## Guidance

### 1. Decide first whether the finding is real

Take the suggestion when the guard's remaining lifetime holds no invariant. The v1.190 lint sweep did exactly that in the majority of cases: it removed the finding rather than suppressing it wherever a real fix existed, and reserved suppression for the structural remainder.

Classify every finding as one of:

| Class | Action |
|---|---|
| Guard's later lifetime is unnecessary | Apply the fix (merge the temporary with its use, scope it in a block, explicit `drop`). |
| Guard spans a deliberate critical section, effect window or fence | Suppress at **fn level** with a one-line reason. |
| Test fixture holds a guard for readability across the visible scope | Module-level `#![allow]` on the test module with the reason. |

### 2. Put the `#[allow]` on the **function**, not on the statement

This is the concrete, non-obvious mechanic: **a statement-level attribute does not suppress the lint.** The lint attaches to the guard-binding temporary and is reported against its enclosing scope, so an `#[allow]` placed on the `let` line is ignored.

Verified with clippy 0.1.98 on two otherwise identical crates (`significant_drop_tightening` set to `warn`):

```rust
// Statement-level — does NOT suppress. Lint still fires.
pub fn statement_level() -> usize {
    #[allow(clippy::significant_drop_tightening)]
    let g = M.lock().unwrap();
    let n = g.len();
    std::hint::black_box(n);
    let extra = 1;
    n + extra
}
```

```rust
// Fn-level — suppresses. This is the form every site in the repo uses.
#[allow(clippy::significant_drop_tightening)] // the guard deliberately spans the whole operation
pub fn fn_level() -> usize {
    let g = M.lock().unwrap();
    let n = g.len();
    std::hint::black_box(n);
    let extra = 1;
    n + extra
}
```

Observed result: the statement-level crate still emits `significant_drop_tightening`; the fn-level crate emits none. Equivalently, the guard can be scoped into an inner block and `drop`ped explicitly — that removes the finding instead of suppressing it, which is preferable when it does not obscure the control flow.

### 3. Never widen the scope past the owning item

- **Never** `#![allow]` at crate root or `lib.rs` for this lint; per-crate blanket allows are explicitly rejected in this repo (three such crate-level allows in a legacy subsystem were flagged as debt and removed).
- Scope fn-level for one operation; module-level (inner `#![allow]` on a `#[cfg(test)] mod tests`) only when an entire test module holds guards across awaits by design.
- Always carry a one-line reason comment. `Do not suppress with #[allow(...)] without a brief justification comment` is a standing repo rule, and the sweep added the reason to every site it created.

### 4. Keep the guard's own invariant documented at the site

The reason comment should name the invariant, not restate the lint:

```rust
// The guard deliberately spans the whole operation
// → the session/registry lock must cover admission, the effect and the terminal write
#[allow(clippy::significant_drop_tightening)]
```

A comment that only says "clippy false positive" tells the next reader nothing about whether it is still true.

### 5. Distinguish suppression from a genuine lock-order defect

`significant_drop_tightening` reports *unnecessary contention*, but the same shape can also be the visible symptom of a real lock-order problem. Before suppressing, confirm the wide scope is **required**, not convenient:

- If two guards are held together, check the acquisition order is globally consistent and state it (a cross-crate lock-order cycle is a deadlock, not a lint).
- If the wide scope exists only because a value must outlive an `.await`, the bound guard may itself be an anti-pattern (`await_holding_lock` territory) — prefer restructuring to holding the lock only around the mutation.

Related but distinct lints that co-occur at these sites and take the same scope discipline: `await_holding_lock`, `used_underscore_binding` (rename instead of suppressing), and `needless_pass_by_value` (take a reference when the callee only reads).

## Why This Matters

- **A wrong-scope suppression fails the gate anyway**, so it wastes a full `--all-targets` round-trip and leaves the author believing the site is clean.
- **Blanket allows destroy the signal.** Once a crate root allows the lint, every *future* genuinely-tightenable guard in that crate stops being reported — the lint becomes decorative.
- **Suppressions are load-bearing claims.** Each one asserts "this guard must live this long". Reviewing them requires the invariant to be written down; the one-line reason is the only durable evidence that the claim was ever true.

## When to Apply

- Clearing any pedantic/nursery clippy finding on a guard, lock, subscriber or other significant-`Drop` temporary.
- Choosing where to place an `#[allow]` (statement vs fn vs module vs crate) — the statement scope is a no-op for this lint.
- Reviewing a diff that adds an `#[allow]`: require the reason to name the invariant, and check the scope is the narrowest item that still works.
- Any analysis of "the lock is held too long" complaints: decide whether it is contention tuning or a required critical section.

## Examples

The v1.190 core-into-daemon extraction created a coherent set of these sites. All are fn-level with the same reason string, and each holds its guard across a whole operation deliberately:

- `crates/nexus-core/src/execution/lifecycle.rs` — `OwnerReservation::claim` / `install` (the process-wide owner-slot registry guard spans the whole reservation).
- `crates/nexus-core/src/execution/peer_tools.rs` — `evict_peer` (registry inner lock spans session lookup, identity check, removal and eviction accounting).
- `crates/nexus-core/src/host.rs` — `HostHandle::execute` (carries both the fn-level drop allow and `too_many_lines`).
- `crates/nexus-core/src/memory.rs` — `list_character_pending_reviews`, count and fragment-list readers (guard spans the owned-Character read plus projection).
- `crates/nexus-core/src/execution/run_events.rs` — `try_register_live` and the atomic-section site, with the cap invariant stated in a preceding comment.
- Test-side precedent — module-level `#![allow(clippy::significant_drop_tightening)]` on test modules whose fixtures hold guards across the visible test scope (`nexus-agent-host` provider test modules), with the reason inline.

### Before — statement-level allow (lint survives)

```rust
pub fn reserve(&self, …) -> CoreResult<()> {
    #[allow(clippy::significant_drop_tightening)]
    let mut maps = self.maps();
    Self::reject_if_closed(&maps)?;
    // … the guard must outlive all of this …
    Ok(())
}
```

### After — fn-level allow with the invariant named

```rust
/// # Errors
/// Returns capacity or shutdown conflicts.
#[allow(clippy::significant_drop_tightening)] // the guard deliberately spans the whole operation
pub fn reserve(&self, …) -> CoreResult<()> {
    let mut maps = self.maps();
    Self::reject_if_closed(&maps)?;
    // … the guard must outlive all of this …
    Ok(())
}
```

## Evidence

- Repo rule — root `AGENTS.md` §Development Policy: clippy `pedantic` + `nursery` as `warn`, CI runs `--all -- -D warnings`, and "do not suppress with `#[allow(...)]` without a brief justification comment".
- Observed behaviour — clippy 0.1.98 (`48a229ceae`, 2026-09-01): statement-level `#[allow(clippy::significant_drop_tightening)]` on the guard binding does not suppress the finding; the equivalent fn-level attribute does. Reproduced on two minimal crates with `significant_drop_tightening = "warn"` in `[lints.clippy]`.
- Sweep outcome — the v1.190 lint rounds removed the finding wherever a real fix existed and reserved the fn-level allow for the guards that intentionally span their operation; `cargo clippy --all --all-targets -- -D warnings` reached 0 errors with `cargo check --all --all-targets` at 0 errors and `cargo +nightly-2026-06-26 fmt --all --check` clean.
- Related guard-lifetime semantics — [typed-authority-carriers-and-fences.md](../architecture-patterns/typed-authority-carriers-and-fences.md) covers *why* those particular guards must span the operation (fence leases and owner reservations).
