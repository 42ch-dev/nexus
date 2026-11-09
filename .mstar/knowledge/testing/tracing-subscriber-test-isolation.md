---
module: nexus workspace test suite (tracing subscribers, parallel harness)
date: 2026-11-09
problem_type: bug
category: testing
severity: medium
source_iteration: 2026-11-09-followup-critical-high
created: 2026-11-09
last_updated: 2026-11-09
status: active
tags:
  - tracing
  - tracing-subscriber
  - flaky-tests
  - parallel-tests
  - ci
applies_when:
  - '"A test asserts on captured tracing events via a per-test subscriber and passes locally but fails on CI"'
  - '"Reviewing or writing tests that install `tracing::subscriber::with_default` under the default parallel harness"'
---

# Tracing-capture tests are process-global; isolate them like any other global

## Context

`crates/nexus-orchestration/src/quality_loop.rs` has a test that installs a `tracing_subscriber` layer with `tracing::subscriber::with_default` and then asserts the captured INFO line contains per-dimension scores. It passed 12/12 locally (`--exact`, darwin) and failed once in CI with an empty capture vector (`test result: FAILED. 895 passed; 1 failed`), taking the whole `Rust tests (orchestration-core)` job down.

## Guidance

1. **`tracing` caches callsite interest process-wide.** The first subscriber that reaches a callsite fixes its `Interest`; a later per-test subscriber can be skipped for a callsite another test already visited. The cache is not per-thread and not reset between tests, so a capture test's result depends on which sibling test ran first — the classic "passes alone, fails in the suite" shape.
2. **Rebuild the cache inside the scoped subscriber**, before invoking the code under test:
   `tracing::callsite::rebuild_interest_cache();` immediately after entering `with_default`.
3. **Serialize the tests that share one callsite** with a process-wide test mutex (the repo's `TRACE_TEST_LOCK` precedent in `apps/nexus42/src/commands/acp_trace.rs`). Serializing only the *writer-touching* tests is enough — do not force `--test-threads=1` globally.
4. **Keep the behavioural assertion.** The fix removes the race, not the check: the captured-log assertion and the verdict-dimension assertions both stay. Removing or narrowing the assertion is the failure mode to avoid ("suppress the flake").
5. **When a new CI leg runs a feature-gated test target, check that the leg actually enables the feature the tests are `cfg`'d on.** A `required-features` target or a `#[cfg(all(test, feature = "..."))]` module is silently skipped by `cargo test -p <pkg>` without the feature; the guard then exists only locally. Pair every gated target with an explicit feature-enabled CI step in an existing job (no new cache key).

## Why This Matters

A non-deterministic capture test is worse than a missing test: it blocks unrelated PRs, and the tempting fix (drop the assertion, add `#[ignore]`, serialize everything) either deletes the contract or taxes the whole suite. The root cause is shared mutable state (`tracing`'s interest cache), so the fix belongs at that boundary.

## When to Apply

Any test asserting on `tracing` output through a locally installed subscriber, in any crate, whenever the default parallel harness runs siblings that also emit the same callsite.

## Examples

- 2026-11-09 follow-up (`feature/followup-critical-high`): `quality_loop::tests::outline_five_q_nogo_info_logs_dimensions` — added a process-wide `TRACING_TEST_LOCK` over the five tests that share the `outline_five_q_check` INFO callsite, plus `rebuild_interest_cache()` inside `with_default`; the log-content assertion is unchanged; CI leg unaffected.
- Related precedent: I-000329/`acp_trace.rs` writer-singleton race — same shape (process-global state), same medicine (per-test mutex + behaviour assertions), different global.
