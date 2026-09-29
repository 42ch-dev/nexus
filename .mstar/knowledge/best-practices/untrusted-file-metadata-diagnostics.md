---
module: crates/nexus-local-db (file_lock diagnostics; generalizes to on-disk metadata)
date: 2026-09-30
problem_type: best_practice
category: best-practices
severity: medium
plan_id: 2026-09-30-v1.201-p2-correctness-sweep
tags:
  - file-lock
  - diagnostics
  - logging
  - untrusted-input
  - fingerprint
  - bounded-read
  - stale-holder
related_components:
  - crates/nexus-orchestration
---

# Diagnostics over on-disk metadata: bounded reads, fingerprinted identities

## Context

`file_lock::try_acquire` writes a `.lock` body (`pid`, `holder_name`, `expires_at_ms`) next to the resource it guards. v1.201 added the `concurrency.md` §6.2 successful-acquire stale-holder diagnostic: after a successful flock, inspect the prior body before overwriting and emit a `tracing::warn!` identifying the displaced holder. Three QC convergence rounds (v1.201 P2, FX-A/A2/A3) distilled the safe shape.

## Guidance

- **Bound the read**: read the prior body through the already-open locked descriptor with a hard byte cap (4097 bytes). Anything larger is classified unparseable — never read unbounded attacker- or crash-grown files on a hot path while holding the lock.
- **Never echo raw on-disk text into logs** — not length-capped, not control-stripped, not prefix-grammar-validated. A "conforming prefix + arbitrary suffix" grammar still leaks (`cli:secret-token-value` passes a `cli:` check). Fingerprint unconditionally: `holder_name = "<fp:<sha256-first-12>>"`; keep the numeric fields (`pid`, `expires_at_ms`) verbatim. The fingerprint is diagnostic-only and never written back to the file.
- **Takeover is never conditioned on the diagnostic** — the OS `flock` already arbitrates; classification (fresh / stale-zombie / cleanly-released / unparseable) only selects the warn payload. Tests assert warn-present/warn-absent alongside the unchanged takeover outcomes (extend, don't invert).
- **Test the capture layer**: a tracing test harness filtered to INFO silently drops WARN events — extend the capture to WARN as part of the same change, and add a non-vacuous case (a secret-bearing stale body asserting the fingerprint form and the absence of the raw text).
- **Small private crypto helpers are acceptable** to avoid a dependency for a 12-hex fingerprint (NIST "abc" vector test pins correctness).

## Why This Matters

Lock metadata is written by our code but read back from disk, where crashes, tampering, or foreign tools may have left arbitrary bytes — including secrets typed into the wrong file. Every earlier "sanitize" variant (cap, strip, prefix-grammar) kept a raw-text path that QC correctly rejected twice; only unconditional fingerprinting closes the class.

## When to Apply

Any diagnostic that quotes content read back from disk or from a wire peer: lock bodies, stale PID files, cache manifests, peer advertisements. Identify by fingerprint + structural fields; never echo payload text.

## Examples

- `crates/nexus-local-db/src/file_lock.rs` — `diagnostic_holder_name` (unconditional `<fp:…>`), 4097-byte capped prior-body read, `stale_holder_warning_redacts_secret_bearing_identity` test.
- Spec anchor: `.mstar/specs/runtime/concurrency.md` §6.2 (restated four-case decision table, diagnostics-only).
