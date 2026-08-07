---
module: nexus-spoke-adapter
date: 2026-08-08
problem_type: security_issue
category: architecture-patterns
severity: high
symptoms:
  - "A peer scoped only for module A could run WASM for any installed module B"
  - "Gate verified one module id while the executor resolved a different one"
root_cause: "The gate checked the id from session/entry state, but the orchestrator merged the request's computable map and re-resolved the module id after the gate — the gated id and the executed id diverged."
resolution_type: code_fix
tags: [wasm, module-scope, security, gate-vs-execution, key-presence-pin]
---

# Module-scope gate bypass: gate id ≠ executed id (fix: key-presence pin)

## Problem

A per-peer `module_scope` allowlist gates which WASM modules a Connect peer may
invoke. The first implementation checked the module id resolved from session
state, but the compute orchestrator then merged `request.computable` and
re-resolved/loaded that id — so a peer scoped for `basic-combat` could pass the
gate with a staged id and execute ANY host-installed module by overriding
`computable.module_id` after the gate.

## Symptoms

- L2 review found: gate passes `resolve_compute_module_id` (session → entry
  body) but `ComputablePort::compute` merges `request.computable` and loads
  the request's id.
- A second-order variant: the gate compared the override only via
  `Value::as_str()` — non-string values (`42`, `{}`, `null`) bypassed the
  comparison while still shadowing the staged id in the merged state.

## What Didn't Work

- Comparing the override with `as_str()` only: non-string JSON values skipped
  the check (latent while the entry-body tier was dead code, but re-armed the
  bypass if that seam ever preserved `body.computable` maps).
- Re-checking only at the gate: any second resolution point downstream can
  diverge.

## Solution

Pin on **key presence**, and pin the FINAL id the executor will load:

```rust
// In verify_compute_gates, before any WASM:
if let Some(override_id) = request.computable.get("module_id") {
    match override_id.as_str() {
        Some(s) if s == gated_id => {}          // same-id override: serve
        _ => return deny("module_not_scoped"),  // non-string OR different: deny
    }
}
// The gated id is the ONLY id that reaches load_module (no second resolution).
```

Plus: a shared `is_safe_module_id` path-safety helper used by BOTH the gate and
the loader (they cannot drift), and the denial test installs a genuinely
installed-but-unscoped module (non-vacuous) with zero-side-effect assertions.

## Why This Works

The vulnerability is a **gate-vs-execution id divergence**. Pinning on key
presence closes the non-string shadowing path; pinning the gated id through
orchestration closes the divergence itself; the shared helper closes drift
between the two guards.

## Prevention

- When adding a scoped-execution surface (WASM, plugins, subprocesses): assert
  "the id the gate verified == the id the executor loads" as an invariant —
  find every resolution point, not just the first.
- For JSON override comparisons: key-presence (`contains_key`) semantics, not
  type-narrowed access.
- Denial tests must prove non-vacuousness (a REAL installed-but-unscoped
  target), not just "not found".
