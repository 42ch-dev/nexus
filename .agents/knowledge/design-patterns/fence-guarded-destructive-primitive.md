---
module: nexus-local-db (writer fences), nexus-core-node (napi), packages/nexus-native (facade), apps/desktop-electron (reset seam), apps/web (recovery UX)
date: 2026-09-20
problem_type: design_pattern
category: design-patterns
severity: high
plan_id: 2026-09-19-v1.192-p0-electron-desktop-cutover
applies_when:
  - implementing a destructive local-state primitive (reset/clear/delete) that must run while a multi-process storage engine is live
  - reusing existing storage fences instead of inventing a second locking scheme
  - designing deterministic race tests for filesystem TOCTOU windows
  - threading a confirm/cancel outcome across a native→JS→IPC→UI seam
related_components:
  - nexus-local-db
  - nexus-core-node
  - packages/nexus-native
  - apps/desktop-electron
  - apps/web
tags:
  - fence
  - writer-protocol
  - toctou
  - unlinkat
  - o-nofollow
  - race-test
  - destructive-primitive
  - discriminated-outcome
---

# Fence-guarded destructive primitive (exact scope, descriptor identity, discriminated outcome)

## Context

v1.192 preserved the shipped, explicitly user-confirmed **local-state reset** (deleting the product's workspace `state.db`/`-wal`/`-shm` files so a corrupt store can be rebuilt) instead of refusing it at the host cutover. The reset had to run:

- while the storage engine is normally live (multiple writers/processes),
- **without** deleting anything outside the exact product-state files (no creative workspace trees, no harness/specs/user documents),
- through a native → napi → TS facade → Electron main → SPA chain where a "cancel" must never be mistaken for a completed reset.

The shipped design is one narrow native primitive behind the **existing** `nexus-local-db` writer fences, plus a descriptor-relative deletion path. This is the reusable pattern; the two ways it goes wrong without the guidance below are a post-fence TOCTOU on the deletion target and a cross-layer cancel that is mistaken for success.

## Guidance

### 1. Fence every target before the first byte is deleted — and reuse the existing fence

Scan all candidate stores first (no mutation), then acquire each target's **exclusive** admission lock before the first `remove_file`. The fence is the crate's existing `state.db.migration.lock` (the same lock regular writers take shared and migrations take exclusive), so a live writer or migration makes the whole reset refuse with a stable `owner_busy` — no store is half-reset. Acquisition is fail-fast (refuse, do not wait), matching the engine's owner rule.

Deliberately **do not** route through the writer-registration guard (`acquire_writer_guard`): that path inserts a registration row into the database being reset and requires protocol tables, while a reset must also clear a pre-protocol or corrupt store. The OS admission lock is the fence; the lock file itself is created if missing and **never deleted**.

### 2. Scope is an exact file list under trusted home

Only `<home>/.nexus42/creators/<creator_id>/workspaces/<slug>/{state.db,state.db-wal,state.db-shm}` are targets. Everything else — admission locks, `kb/`/`Pool/` siblings, TOML files, `creators/` non-store entries, user documents, harness/knowledge/specs — is preserved. The return value counts removed primary DBs (a store holding only WAL/SHM siblings is cleaned but not counted), so the caller's "how many stores were reset" signal stays meaningful.

### 3. Delete by admitted identity, never by re-entered path (post-fence TOCTOU)

Path-based `remove_file` after fencing is racy: between the check and the unlink, a rename or symlink swap can redirect the deletion into a rival directory or unlink a link the user did not confirm. Fix the primitive at the level where it is unambiguous:

```rust
// Admit the store directory as an open descriptor: walk every component below
// the trusted home with O_NOFOLLOW (the home itself may legitimately be a symlink,
// e.g. /tmp on macOS). A swapped component fails ELOOP/ENOTDIR and refuses.
let dir = AdmittedDir::admit(home, &relative_store_path)?; // owns the dir fd + file identities
// Record each existing state file's identity at admission:
//   fstatat(dir_fd, name, AT_SYMLINK_NOFOLLOW) -> (st_dev, st_ino); symlink/non-file -> refuse
fence_all_targets(&dirs)?;             // exclusive migration lock per target, canonical order
// Under the fence, re-verify identity and unlink relative to the descriptor:
dir.remove(&name)?;                    // fstatat again: still a regular file with the admitted
                                       // (st_dev, st_ino); then unlinkat(dir_fd, name, 0)
```

Properties this buys: a state file swapped for a symlink is **refused** (never followed, never removed); a store directory renamed aside and replaced is **inert** — the deletion follows the admitted directory, so the store the user confirmed is reset and the replacement keeps every byte; a store directory swapped for a symlink is refused at admission. Unix arm only: without directory descriptors (non-Unix), admission and removal fall back to `symlink_metadata` re-checks, which **narrow but do not close** the window — document that as a bound, do not claim parity.

Two traps to remember:

- **`SFlag::contains(S_IFREG)` is the wrong file-type test** (`S_IFLNK = 0o120000` includes `S_IFREG` bits). Compare the truncated type field: `from_bits_truncate(mode) == S_IFREG`.
- The workspace forbids `unsafe`, so the raw descriptor is owned by a small RAII struct closed via `nix::unistd::close` — never `OwnedFd::from_raw_fd`.

### 4. Make the races testable with a post-fence seam, not timing

A `#[doc(hidden)]` seam — `reset_local_state_with_post_fence_hook(home, hook)` — invokes `hook` after **every** target is fenced and before the first deletion; production passes a no-op. Tests drive the swap deterministically inside exactly that window (no sleeps), and the same tests must **fail against the pre-fix path-based deletion** (red → green evidence). Test both scenarios: file swapped for a symlink (refused, nothing deleted, link intact) and store directory renamed aside (admitted store reset; rival directory untouched).

The seam is worth its small public surface precisely because the alternative — sleep-based timing tests — is non-deterministic and would not pin the defect.

### 5. Type the outcome: cancel is not success

A destructive primitive that crosses process seams must return a **discriminated result**, not merely resolve:

```ts
type ResetLocalStateResult = { status: 'confirmed' } | { status: 'cancelled' };
// native:  reset_local_state(...) -> Ok(count) | Err(owner_busy|forbidden|invalid_input|internal)
// seam:    cancel/reject must become { status: 'cancelled' } — never a bare resolved promise
// UI:      proceed to recovery ONLY on { status: 'confirmed' };
//          cancelled keeps the pre-attempt error state, never reloads, never clears the error
```

The failure shape to avoid: the seam resolves `null` on cancel while UI consumers treat *any* resolved promise as confirmed — Cancel then bumps the reset nonce and clears the migration error even though no reset ran. Thread `{status}` through every tag (napi → facade → IPC → UI) and move the "confirmed-only" rule into every consumer. Related rules: the primitive is synchronous-in-effect and opens no database; the caller owns confirmation UI, service-close sequencing and recovery; native Cancel must be distinguishable from a rejected promise (failure rejects, cancel resolves `cancelled`).

### 6. Boundary checks at the facade, defense in depth in the native layer

The TS facade rejects a non-string/empty/**relative** `home` with a plain error **before** `loadNativeBinding()` is called; the Rust side re-checks absoluteness so a direct binding user still gets a structured `invalid_input`. Rejections carry the wire `CoreError` envelope in `Error.message` with stable codes: `owner_busy` (409, `details.resource`), `forbidden` (403, symlink/non-file), `invalid_input` (400), `internal` (500). Open no database, run no host logic; the native call runs its filesystem work on `spawn_blocking`.

### 7. Record residual bounds instead of promoting a weaker guarantee

The honest bounds shipped with this primitive:

- **Fence drift:** the migration fence is still acquired *by path*; an actor able to rename a store's parent between admission and fence acquisition could fence a different directory than the admitted one. That actor already has write permission over the same files, and deletion still cannot leave the admitted directory — a bound, not an escalation. Closing it needs an fd-relative fence API (out of scope).
- **Non-Unix arm:** re-checks only (`[UNVERIFIED]` on the platforms not exercised). No parity claim.
- **JS busy-path:** the live-writer refusal is proven at Rust level with a real admitted writer; Node has no flock API, so the JS test covers binding load, exact scope and structured refusal — state the coverage difference.

## Why This Matters

- **Destructive operations are the one place where "mostly right" is unacceptable.** Exact-file scope + fence-before-delete + identity-relative unlink together mean the user's confirmed intent (reset *this* store) cannot be converted into a different deletion by a concurrent actor.
- **The seam is where confirmation semantics die.** A primitive that is correct in Rust but lossy at the JS/IPC boundary produces the worst outcome class: the UI reports recovery that never happened.
- **Deterministic race tests are achievable** when the implementation exposes one internal seam; they are the only evidence that the TOCTOU is actually closed.

## When to Apply

- Any new destructive local-state operation (reset, clear, compact, prune) in the product's storage layout.
- Any filesystem deletion that must be safe against concurrent renames/symlink swaps on a shared machine.
- Any confirm/cancel flow threaded from native/CLI through a bridge to UI consumers.

## Examples

### Before / after — path deletion vs descriptor deletion

```rust
// Before: scan-then-delete by path. A swap after fencing redirects the unlink.
for target in targets { std::fs::remove_file(&target.path)?; }

// After: identity-bound removal — refused if the entry is no longer the admitted file.
fn remove(&self, name: &str) -> Result<(), PathEscape> {
    let meta = fstatat(self.dir_fd, name, AT_SYMLINK_NOFOLLOW)?;
    if from_bits_truncate(meta.mode) != S_IFREG
        || (meta.dev, meta.ino) != self.admitted_files[name] {
        return Err(PathEscape { path: name.into() });
    }
    unlinkat(self.dir_fd, name, 0) // resolves against the admitted directory inode
}
```

### Red → green race evidence (shape)

```text
# Pre-fix code under the same tests:
test a_state_file_swapped_for_a_symlink_after_fencing_is_refused ... FAILED (got Ok(1))
test a_store_directory_swapped_after_fencing_cannot_redirect_the_deletion ... FAILED
# Fixed code:
test result: ok. 9 passed; 0 failed   # desktop_reset.rs
```

## Evidence

- Native — `crates/nexus-local-db/src/lib.rs` (`reset_local_state`, `reset_local_state_with_post_fence_hook`, `AdmittedDir`), `src/writer_protocol.rs` (`acquire_store_reset_fence`); napi — `crates/nexus-core-node/src/lib.rs` (`reset_local_state`); facade — `packages/nexus-native/src/index.ts` (`resetLocalState`, absolute-home check).
- Rust pins — `crates/nexus-local-db/tests/desktop_reset.rs` (9 real-file cases incl. both post-fence races, live-writer refusal with a real admitted writer, exact-scope preservation); JS pins — `packages/nexus-native/tests/desktop-reset.test.mjs` (5 cases against the built unsigned binding).
- Seam consumers — `apps/desktop-electron/src/main.ts` (confirm + reset sequencing), `apps/web/src/lib/nexus/desktop-capabilities.ts` (`ResetLocalDatabaseResult`), `apps/web/src/components/setup/daemon-launch-gate.tsx` and `pages/setup-step-workspace.tsx` (confirmed-only recovery), with cancelled-path tests in both component suites.
- Fence design this reuses — [typed-authority-carriers-and-fences.md](../architecture-patterns/typed-authority-carriers-and-fences.md); trusted-home helpers used by the scan — [nexus-home-layout-path-helpers.md](../conventions/nexus-home-layout-path-helpers.md).
