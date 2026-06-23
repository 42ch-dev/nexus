# QC Report: V1.62 P1 — Performance & Reliability (Reviewer #3)

**Plan ID**: `2026-06-23-v1.62-manifest-schemas-and-host-validation`
**Reviewer**: qc-specialist-3 (Performance + Reliability)
**Review Date**: 2026-06-23
**Status**: Needs Discussion

## Scope

**plan_id**: `2026-06-23-v1.62-manifest-schemas-and-host-validation`
**Working branch** (verified): `feature/v1.62-manifest-schemas-and-host-validation`
**Review cwd** (verified): `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.62-p1-manifest`
**Review range / Diff basis**: `merge-base iteration/v1.62 @ f77b3de8 → 01514cf4` (6 commits)

## Summary

V1.62 P1 adds manifest-driven JSON-Schema validation to the WASM compute host. The implementation passes all correctness gates but has a **medium-severity reliability risk (W-001)**: unbounded recursion depth in `validate_against_schema`. An adversarial `manifest.json` with deeply-nested `properties` schemas, or a `ComputeInput` with deeply-nested array instances, could cause stack overflow.

All other performance/reliability gates pass:
- ✅ Validation library (`jsonschema` crate) is lightweight and well-maintained
- ✅ Validation only occurs when `schemas` block is declared (backward compatible)
- ✅ Validation errors carry meaningful JSON paths (no performance penalty)
- ✅ `cargo test -p nexus-wasm-host`: 40 tests PASS
- ✅ `cargo clippy -p nexus-wasm-host -- -D warnings`: PASS
- ✅ `cargo +nightly fmt --all --check`: PASS

**Recommendation**: Fix W-001 by adding a depth limit (e.g., 64 levels) to `validate_against_schema` before merge.

## Findings

### Critical
None

### Warning

#### W-001: Unbounded recursion depth in `validate_against_schema` — stack overflow risk

**Severity**: medium
**Location**: `crates/nexus-wasm-host/src/compute.rs` line 277 (`fn validate_against_schema`)
**Impact**: Stack overflow on adversarial input

**Context**: The `validate_against_schema` function validates JSON instances against declared schemas. It has two recursive call sites:

1. **Properties recursion** (line ~355): For each property in a schema's `properties` object, recursively validates the property value against its property schema.
2. **Items recursion** (line ~398): For each element in an array instance, recursively validates the element against the schema's `items` schema.

There is **no depth limit**. An adversarial module author could:
- Craft a `manifest.json` with a 1000-level nested `properties` schema:
  ```json
  {
    "type": "object",
    "properties": {
      "a": {
        "type": "object",
        "properties": {
          "a": { ... 1000 levels deep ... }
        }
      }
    }
  }
  ```
- Or send a `ComputeInput` with a 1000-level nested array instance.

Both would cause `validate_against_schema` to recurse ~1000 times, exceeding the Rust default stack size and causing a stack overflow crash (not a graceful `ComputeError`).

**Risk assessment**: This is a **denial-of-service vector**. While module authors are trusted (they control the `.wasm` blob), a compromised module or a buggy schema generator could exploit this. The fix is straightforward and low-cost.

**Mitigation required**: Add a `MAX_VALIDATION_DEPTH` constant (e.g., 64 levels) and a `depth_limit: usize` parameter to `validate_against_schema`. At function entry, check `if depth_limit == 0 { return Err(...) }`. Decrement `depth_limit` on each recursive call (lines 355 and 398). 64 levels is more than enough for any realistic compute envelope depth (KeyBlocks → attributes → nested objects/arrays).

**Test coverage**: Add two regression tests:
1. `deeply_nested_properties_rejected_by_depth_limit`: Adversarial 100-level nested `properties` schema → expect `ManifestValidationFailed` with "exceeded maximum validation depth".
2. `deeply_nested_items_rejected_by_depth_limit`: Adversarial 100-level nested array instance → expect same error.

### Suggestions

#### S1: Consider caching parsed `ModuleSchemas` in the engine's per-module cache

**Context**: `parse_manifest` (via `serde_json::from_str`) is called on every `compute()` invocation. For high-frequency modules (e.g., many parallel combat computations), this may add measurable overhead.

**Impact**: Minor (JSON parsing is fast for typical manifest sizes). Not blocking P1.

**Recommendation**: After P2 (which likely adds per-module metrics), profile `parse_manifest` in a high-frequency workload. If it exceeds ~1% of total compute time, consider caching the parsed `ModuleSchemas` in `WasmEngine::module_cache` (keyed by module ID). This requires cache invalidation when manifest files change (file timestamp check).

---

## Revalidation (2026-06-24, targeted re-review)

**Re-review trigger**: PM fix commit `d2e4390a` addressing W-001 (unbounded recursion).

### W-001 (unbounded recursion in validator) — RESOLVED
- Fix verified: `MAX_VALIDATION_DEPTH = 64` constant + `depth_limit: usize` parameter on `validate_against_schema`.
- Depth check is at function entry (short-circuit before any work).
- Decrement at both recursion sites (properties + items):
  - Line 360: `depth_limit - 1` for properties recursion
  - Line 404: `depth_limit - 1` for items recursion (verified by inspection of compute.rs)
- Failure mode: `ManifestValidationFailed { path, detail: "exceeded maximum validation depth (64)" }` — graceful, no panic, no stack overflow.
- 2 new tests cover both adversarial-schema + adversarial-instance paths:
  - `deeply_nested_properties_rejected_by_depth_limit`: 100-level nested properties schema → rejected with depth error ✅
  - `deeply_nested_items_rejected_by_depth_limit`: 100-level nested array instance → rejected with depth error ✅
- 64 covers any realistic compute envelope (typical 3-5 levels: KeyBlock → body → attributes/state → nested fields); ample headroom.

### Verification
- cargo test -p nexus-wasm-host: 42 tests PASS (40 previous + 2 new depth-limit tests)
- cargo clippy -p nexus-wasm-host -- -D warnings: PASS
- cargo +nightly fmt --all --check: PASS

### Updated Verdict
**Approve** — W-001 fully resolved; depth limit + new tests address the stack-overflow risk; no panic path remains in the validator.