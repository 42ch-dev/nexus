---
module: nexus-contracts, rust-workspace
date: 2026-08-07
problem_type: test_failure
category: workflow-patterns
severity: medium
plan_id: 2026-08-06-v1.153-p0-spoke-091-pin-and-connect-v2-adaptation
symptoms: ["schema_drift_detection (crates/nexus-contracts/tests) fails with 199 \"Cannot read schemas/…: No such file or directory\" errors", "all 225 schema files exist on disk and in git (git status clean)", "re-running without source changes keeps failing — the same cached binary is reused"]
root_cause: a stale test binary in the shared CARGO_TARGET_DIR was compiled while a since-removed git worktree was active, baking in that worktree's CARGO_MANIFEST_DIR; Cargo fingerprinting sees unchanged sources and reuses the binary, so the env!()-derived workspace_root points at the deleted worktree directory and every relative schemas/ read fails with ENOENT
resolution_type: environment_setup
tags: [cargo, cargo-target-dir, git-worktree, stale-binary, manifest-dir, schema-drift-detection, test-failure]
---

# Shared CARGO_TARGET_DIR + Worktree Removal Poisons Compiled Test Binaries with a Stale CARGO_MANIFEST_DIR

## Problem

The repo's `schema_drift_detection` test (in `crates/nexus-contracts/tests/schema_drift_detection.rs`) failed with **199 `"Cannot read schemas/…: No such file or directory"` errors** — even though all 225 schema files existed on disk and in git. This looked like a wire-drift / codegen regression introduced by the iteration's merges. It was not: the failure was an environment staleness artifact of the shared cargo target cache combined with git worktree add/remove.

## Symptoms

- `schema_drift_detection` fails with ~199 `Cannot read schemas/<file>: No such file or directory` errors.
- `git status` is clean; all 225 schema files are present on disk; the schemas directory resolves correctly from the shell.
- Re-running the test without any source change fails identically — the same cached binary is reused.
- The failure appears right after a git worktree (which previously built the same crate) was removed with `git worktree remove --force`.

## What Didn't Work

- **Suspecting the merge / wire drift**: re-checking codegen outputs, schema files, and the `schema_drift_detection` test itself — all correct. The test's file list came from a directory scan that *succeeded*; only the `read` of each file failed, pointing at a path resolution problem, not a content problem.
- **Assuming the test logic regressed**: the test computes the workspace root at **compile time** (`env!("CARGO_MANIFEST_DIR")` → parent → parent, `schema_drift_detection.rs:1180-1186`). Nothing in the source changed.
- **Re-running `cargo test`**: Cargo's fingerprinting compares source hashes; the source was unchanged, so the cached binary stayed "fresh" and was reused verbatim.

## Solution

Force a rebuild of the affected test binary so it recompiles with the **current checkout's** correct `CARGO_MANIFEST_DIR`:

```sh
touch crates/nexus-contracts/tests/schema_drift_detection.rs
cargo test -p nexus-contracts --test schema_drift_detection
```

The touched source invalidates Cargo's fingerprint; the rebuilt binary bakes in the live checkout path, `workspace_root()` resolves to the real repo root, and all 199 schema reads succeed.

## Why This Works

The repo shares one target cache across all checkouts: `.envrc` sets `export CARGO_TARGET_DIR="${XDG_CACHE_HOME:-$HOME/.cache}/nexus-target"` — deliberately shared "across main checkout and git worktrees". The failing binary had been compiled while a git worktree was active; `env!("CARGO_MANIFEST_DIR")` is a compile-time constant, so the binary carried the **deleted worktree's** path. After `git worktree remove --force`, `workspace_root()` pointed into a nonexistent directory, so every `schemas/...` read returned ENOENT. Cargo considered the binary fresh because the *source* was unchanged — the baked-in path is invisible to its fingerprinting. Recompiling from the current checkout replaces the constant with the correct path.

## Prevention

- Whenever a repo uses a **shared `CARGO_TARGET_DIR`** AND git worktrees that get added/removed (the Morning Star iteration Phase 2 worktree pattern does exactly this), any test/binary that resolves paths from `env!("CARGO_MANIFEST_DIR")` can go stale after a worktree is removed.
- **Diagnostic signature**: path-relative reads fail with ENOENT despite the files existing; `git status` clean; source unchanged.
- **Remedy**: force a rebuild (`touch <test-file>` then `cargo test`) before investigating wire-drift regressions.
- Longer-term options: workspace-local `target/` dirs for worktrees (drop the shared cache), or resolve the workspace root at **runtime** (from `std::env::current_dir()` or a `CARGO_MANIFEST_DIR`-independent probe) instead of compile time.
