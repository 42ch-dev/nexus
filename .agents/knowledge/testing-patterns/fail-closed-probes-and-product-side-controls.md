---
title: Fail closed when a probe cannot answer, and prove repairs from the product side
category: testing-patterns
track: knowledge
last_updated: 2026-11-22
created: 2026-11-22
date: 2026-11-22
status: active
problem_type: testing_pattern
module: repo-wide probes and guards
severity: medium
symptoms:
  - "a test is 'fixed' but the negative control only changes the expectation, so a wrong product still passes"
  - "a guard silently disables itself when the tool it asks is missing, times out or errors"
  - "the same 'cannot determine X' branch is reused for 'X is absent' in five different places"
root_cause: "Two habits compound: a boolean probe collapses three outcomes (present, absent, unknown) into two, and a negative control perturbs the cheapest side — the expectation — instead of the product."
resolution_type: workflow_improvement
---

# Fail closed when a probe cannot answer, and prove repairs from the product side

## Context

One iteration repaired seven assertions and added one shell guard. Review found the same two defects over and over:

**Probe collapse.** `rustc --print sysroot --target <triple>` exits 0 for an uninstalled triple, so `has_wasm_target()` returned true unconditionally and the gate was a no-op in three crates. Then `Path::is_dir()` treated EACCES as "absent". Then a failed probe was folded into "target missing", silently deleting four tests. Then `git ls-files` failure produced an empty corpus reported as `PASS`. Then an unreadable tracked file was skipped. Then a failing `sort` truncated the list. Six places, one shape.

**Expectation-side controls.** A repair was "proved" by flipping the expected value. That shows the assertion compares; it does not show the product can fail it.

## Guidance

**Every probe has three outcomes, not two.** Write the branch explicitly:

```rust
match fs::metadata(path) {
    Ok(meta) if meta.is_dir() => Installed,
    Err(e) if e.kind() == ErrorKind::NotFound => NotInstalled,
    Ok(_) | Err(_) => panic!("cannot determine target state: {path}"),  // fail closed
}
```

Rules that follow:

- "I could not find out" is never "no". A build that cannot answer must stop.
- An empty result set from a filtered query is a failure, not a clean pass — zero files means the pathspec is wrong.
- No `2>/dev/null`, `|| true`, `or next`, or unchecked pipeline stage in a guard. Each one is a place where unknown becomes absent.
- A probe that depends on external state needs a rerun trigger for that state (`rerun-if-changed` on the directory whose contents decide the answer), and a second trigger for anything that changes *which* tool is asked (`rerun-if-env-changed=RUSTC`) — a delegating wrapper leaves the compiler fingerprint identical, so cargo will happily keep a stale answer.

**Controls come in three strengths; aim for the strongest.**

| Perturb | Proves | Verdict |
|---|---|---|
| change the expected value | the assertion compares | weakest — a wrong product still passes |
| change the fixture or seed | the assertion reads product output | acceptable for wiring |
| change the product (renderer, comparison, clap attribute, probe input) | the product can fail it | required for a repair |

For a guard over source text, a product-side control means perturbing the environment the guard reads — a wrapper script presenting a sysroot without the target, a `chmod 000` on a covered file, a `git` shim that fails — and observing the guard stop, not pass.

**State the guarantee's boundary where the guard is read.** A shell scanner matching declaration-shaped text cannot see `macro_rules!` expansion. Write that in the script header, next to the tracked-set and abort rules, so a reader does not infer a guarantee the tool cannot keep.

## Why This Matters

Every one of these defects produced a green run that proved nothing, and none of them failed loudly. The cost is not the bug — it is that a green suite stops being evidence. In this iteration the cron shadow and the wasm gate had been quietly no-ops for as long as they existed.

## When to Apply

- Writing a build script that probes for a toolchain capability.
- Writing any guard that shells out, reads a filtered file set, or walks a repository.
- Reviewing a "we proved it fails" claim: ask which side was perturbed.
- Any place the code branches on `exists()`, `is_dir()`, a command's exit status, or a non-empty result.
