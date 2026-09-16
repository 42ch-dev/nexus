---
module: rust-workspace build tooling (.envrc, git worktrees, CARGO_TARGET_DIR)
date: 2026-09-17
problem_type: workflow_issue
category: workflow-patterns
severity: high
plan_id: 2026-09-15-v1.190-p6-independent-rust-products
applies_when:
  - running more than one cargo build concurrently from separate git worktrees
  - setting or reviewing the repo's CARGO_TARGET_DIR / direnv layout
  - cleaning up a feature worktree after its branch merges
  - diagnosing disk exhaustion on an agent host during a multi-track iteration
tags:
  - cargo-target-dir
  - git-worktree
  - direnv
  - envrc
  - disk-exhaustion
  - merge-gate
  - agent-parallelism
---

# Scoped cargo target dirs per worktree, with a merge-gate cleanup

## Problem

Multi-track iterations build the same Rust workspace from several git worktrees at once. With one shared `CARGO_TARGET_DIR`, every track's artifacts accumulate in a single directory that nothing ever prunes, and each concurrent `cargo` invocation contends on the same target-dir lock. During v1.190 six concurrent feature targets consumed **98 GiB** in a single iteration and degraded the host.

The distinct-but-often-confused symptom is per-track artifact collision: two worktrees writing the same shared cache can embed a path or blob that belongs to a different checkout (a separate failure mode recorded in [shared-cargo-target-dir-worktree-stale-manifest-dir.md](shared-cargo-target-dir-worktree-stale-manifest-dir.md)).

## Symptoms

- `df`/`du` shows tens of GiB under the shared cache after a few parallel tracks; nothing in the repo attributes the growth to a specific track.
- Concurrent `cargo` runs serialize on the shared target-dir lock instead of building in parallel.
- After merging one track, there is no way to tell which artifacts belonged to it, so nothing can be reclaimed safely.
- The disk pressure surfaces as unrelated failures (build errors, flaky tool calls) rather than as a build-system error.

## What Didn't Work

- **A single shared `CARGO_TARGET_DIR` for main + all worktrees.** This was the original, deliberate choice ("share the build cache across worktrees") and it is what produced the 98 GiB. Sharing optimizes for sequential reuse, which is the opposite of the multi-track case.
- **Manual judgment about which subdirectories are safe to delete.** Without per-track ownership there is no reliable boundary; partial `rm -rf` of `target/debug/{deps,incremental}` reclaims space but cannot be attributed or verified.
- **Relying on `cargo clean` per track.** It clears whichever dir `CARGO_TARGET_DIR` currently resolves to, so with a shared dir the first track to finish nukes the cache every other track is still using.
- **Setting the target dir in config files.** `build.target-dir` in root `Cargo.toml` is not a valid package/workspace key; project `.cargo/config.toml` cannot expand `$HOME`/XDG and absolute paths are not portable for an OSS repo; user-level `~/.cargo/config.toml` pollutes every Rust project on the machine.

## Solution

Keep the single source of truth in the repo's `.envrc` (direnv), and have it **auto-detect the checkout context** — main checkout and integration worktree share the canonical dir, each feature worktree gets its own suffixed dir:

```sh
# .envrc (repo root)
_nexus_wt_file=.git
if [ -f "$_nexus_wt_file" ] && grep -qs 'worktrees' "$_nexus_wt_file"; then
    _nexus_dir=$(basename "$PWD")
    case "$_nexus_dir" in
        iteration-*)
            # Integration worktree — canonical shared target (merge target)
            export CARGO_TARGET_DIR="${XDG_CACHE_HOME:-$HOME/.cache}/nexus-target"
            ;;
        *)
            # Feature worktree — isolated target (clean after merge)
            export CARGO_TARGET_DIR="${XDG_CACHE_HOME:-$HOME/.cache}/nexus-target-${_nexus_dir}"
            ;;
    esac
    unset _nexus_dir
else
    # Main checkout — canonical shared target
    export CARGO_TARGET_DIR="${XDG_CACHE_HOME:-$HOME/.cache}/nexus-target"
fi
unset _nexus_wt_file
```

Detection is `-f .git` (a worktree has a `.git` *file* pointing at the main repo, a normal checkout has a `.git` *directory*) plus a `worktrees` grep, so no per-worktree setup is required. Enable once per checkout with `direnv allow`.

**Merge gate (hard):** before merging a feature branch into the integration branch, remove that feature's scoped cache:

```sh
rm -rf ~/.cache/nexus-target-<dirname>     # this feature's cache, precise by name
rm -rf ~/.cache/nexus-target-*             # all feature caches; canonical dir untouched
git worktree remove .worktrees/<name> && git worktree prune
```

Integration verification (`cargo check --workspace`) runs from the integration worktree against the canonical `~/.cache/nexus-target`, so it never depends on a feature's cache. `du -sh ~/.cache/nexus-target-*` shows every feature's size at a glance.

## Why This Works

The target dir becomes a **function of the checkout identity**, which gives three properties the shared dir cannot have:

1. **Attribution.** A directory named after the worktree is unambiguously owned by one track, so cleanup is mechanical rather than judgmental.
2. **Isolation.** Concurrent tracks do not contend on one target-dir lock and cannot observe each other's artifacts; the integration target is a first-class member of the scheme rather than an accident of ordering.
3. **Bounded growth with an exit.** Nothing prunes a cache automatically, so the design pairs each feature dir with a merge-gate deletion — the moment the track's artifacts stop being useful is exactly the moment the branch merges. Making that a gate (not advice) is what prevented a repeat.

The `.envrc` location matters: `CARGO_TARGET_DIR` already overrides any config file when both are set, so keeping the single SSOT there avoids a second, conflicting declaration. Do **not** set it in `~/.cargo/config.toml`.

## When to Apply

- Any repo where agents or developers build the same workspace from multiple git worktrees concurrently.
- Setting up a new worktree — confirm the resolved dir with `cargo metadata --no-deps --format-version 1 | jq -r .target_directory` before building.
- Closing a track: run the cleanup before `git worktree remove`, and treat a skipped cleanup as a gate failure, not housekeeping.
- Disk-pressure triage on a build host: count and size the scoped dirs first; the per-track boundary is what makes the number actionable.

## Examples

### Before — one shared dir for every checkout

```sh
export CARGO_TARGET_DIR="${XDG_CACHE_HOME:-$HOME/.cache}/nexus-target"
# main + integration + N feature worktrees all write here;
# 6 parallel tracks → 98 GiB, no per-track boundary, no cleanup owner
```

### After — context-aware scoping plus a gate

```sh
# main checkout / integration worktree
cargo metadata --no-deps --format-version 1 | jq -r .target_directory
# → /Users/<u>/.cache/nexus-target

# feature worktree .worktrees/v1190-p6
# → /Users/<u>/.cache/nexus-target-v1190-p6

# before merging that branch:
rm -rf ~/.cache/nexus-target-v1190-p6
git worktree remove .worktrees/v1190-p6 && git worktree prune
```

## Evidence

- Implemented — repo-root `.envrc` (worktree-context detection, canonical vs suffixed target dirs) and the `AGENTS.md` §Development Policy build-cache section (layout table, `direnv allow` step, "Merge gate — feature-branch target cleanup (HARD)", `du -sh ~/.cache/nexus-target-*`, anti-patterns).
- Motivation — six concurrent feature targets consumed 98 GiB in `/tmp` during v1.190 and degraded the host; the merge-gate rule was added for that lesson and the `.envrc` scoping followed.
- Related failure mode with the same variable — [shared-cargo-target-dir-worktree-stale-manifest-dir.md](shared-cargo-target-dir-worktree-stale-manifest-dir.md) (a stale test binary baked a removed worktree's `CARGO_MANIFEST_DIR` into a shared cache). That doc's long-term remedy — per-worktree target dirs — is the layout this note specifies.
