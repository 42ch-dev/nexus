# Parallel worktrees need separate Cargo target directories

## Symptom

A `cargo check -p <crate>` inside one git worktree failed with a type error naming a symbol that exists only in
a different worktree:

```
the trait bound `PathBuf: AssetSource` is not satisfied
  --> crates/nexus-core/src/strategy_candidates.rs:66
```

Both worktrees were cut from the same commit; the failing worktree's `crates/nexus-preset/src/workflow_ir.rs`
matched `main` byte for byte and contained no such trait. The error came from a compiled artifact left by the
other worktree.

## Cause

Every worktree shared one `CARGO_TARGET_DIR`. Two checkouts hold the same package (`nexus-preset v0.1.0`) at
different paths, and the shared target directory's fingerprints can be satisfied by the other checkout's output,
so a build compiles against the wrong `nexus-preset`.

`touch` on the file that "should not have the symbol" forced a rebuild and the check passed in seconds — the
signature of a stale artifact, not a source defect.

## Rule

One target directory per worktree:

```bash
CARGO_TARGET_DIR="$HOME/.cache/nexus-target/<worktree-name>" cargo test -p <crate>
```

Alternatively, serialize builds: never run two worktrees' cargo concurrently against one shared directory.

## Diagnosis order

When a cross-worktree type error names a symbol that does not exist in the current worktree:

1. `md5`/`git diff` the named file against `main` — confirm the symbol is absent.
2. `touch` the file and re-run — a pass confirms the cache.
3. Only then treat it as a source defect.

## Cost of not following it

A whole round was spent reporting the error as "pre-existing" and re-verifying it on `main`, where it did not
reproduce at all. The false report then had to be checked and discarded by the coordinator.
