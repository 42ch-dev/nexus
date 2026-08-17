---
module: nexus-agent-host
date: 2026-08-17
problem_type: testing_pattern
category: workflow-patterns
severity: medium
plan_id: 2026-08-17-v1.168-p2-dsh-native-provider
tags: [env-mutation, test-flake, process-env-lock, path-isolation, python-fixture, cargo-test]
applies_when: tests mutate process env (PATH / HOME / DSH_RUNTIME_BIN) while other tests spawn fixtures through the same env
---

# Serialize env-mutating tests against env-consuming fixture spawns

## Context

Rust test binaries run cases on multiple threads sharing **one process environment**. V1.168 added protocol-fixture tests that spawn Python mock CLIs (`mock_claude_cli.py`, `mock_codex_app_server.py`, `mock_dsh_agent.py`) while other tests replaced `PATH` for isolation.

## Problem

Fixture scripts use the `#!/usr/bin/env python3` shebang. The kernel resolves `python3` through the **current process `PATH` at spawn time**. A PATH-isolation test that ran concurrently could empty PATH exactly while a fixture test spawned its mock → spawn failed (or found the wrong interpreter) → flaky failures that only appeared under parallel `cargo test`.

## Guidance

1. Any test that **mutates** process env (PATH/HOME/… guards) or **spawns a subprocess whose resolution depends on env** must serialize with the others: one crate-wide lock (V1.168 uses `PROCESS_ENV_LOCK` in `nexus-agent-host/src/lib.rs`), held for the whole mutate+spawn window.
2. Prefer restoring env in a guard `Drop` so panics cannot leak the mutation to the next case.
3. When the fixture spawn chain is the victim (not the culprit), the lock must be taken by **both** sides — a guard on the mutating test alone does not protect a spawn happening mid-mutation in another thread.
4. Absolute-path fixture interpreters (no `env` shebang) avoid the whole class, but only when the tool really exists at that path on CI.

## Why This Matters

The failure reads as "flaky CI" but is a deterministic race on shared process state. Without the lock you get heisen-failures that pass locally (different scheduling) and block merges intermittently.

## Examples

```rust
// crates/nexus-agent-host/src/lib.rs
pub(crate) static PROCESS_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

// env-mutating test
let _guard = crate::PROCESS_ENV_LOCK.lock().unwrap();
let _path_guard = PathGuard::isolate(temp_dir);
// ... probe / scan assertions ...

// fixture-spawning test
let _guard = crate::PROCESS_ENV_LOCK.lock().unwrap();
// spawn mock_dsh_agent.py ...
```

## See also

- `workflow-patterns/shared-cargo-target-dir-worktree-stale-manifest-dir.md` — a different env-poisoning flake (compile-time, not runtime)
