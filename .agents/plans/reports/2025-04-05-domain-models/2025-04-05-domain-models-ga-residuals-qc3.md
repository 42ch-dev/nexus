---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2025-04-05-domain-models"
verdict: Approve
generated_at: 2026-04-06
---

# QC3 Report: V1.0 GA Residual Fix

**Reviewer**: @qc-specialist-3  
**Branch**: `fix/v1.0-ga-residuals`  
**Diff**: `main...HEAD` (4 files, +271/-5 lines)

---

## Verdict: **Approve** ✅

---

## 1. Schema Change: `unit_kind` Removal

**File**: `schemas/common/source-anchor.schema.json`

**Change**: Removed `"unit_kind"` from the `required` array in the `SourceSummaryRef.items` object.

```diff
- "required": ["story_manifest_id", "summary_unit_id", "unit_kind"],
+ "required": ["story_manifest_id", "summary_unit_id"],
```

### Assessment:
- **Rationale documented**: `status.json` closure_note states codegen already treats it as optional due to nested allOf/items limitation — schema now matches generated output.
- **Non-breaking**: `unit_kind` field is still present in `properties`; only the **required constraint** is relaxed. Existing data without `unit_kind` will now validate.
- **V1.0 safety**: Per `AGENTS.md` rule "Wire contracts must match schemas — no drift", this aligns schema with actual generated behavior.

**Finding**: None. Clean schema fix with documented rationale.

---

## 2. New Tests: Domain↔Contract Roundtrip

**File**: `crates/nexus-domain/src/contract_assertions.rs`

### `test_world_domain_contract_roundtrip`
- Creates a `World` domain object, converts to contract via `From`, validates all fields, converts back to domain, validates roundtrip fidelity.
- **Assertions**: `world_id`, `owner_creator_id`, `title`, `slug`, `visibility`, `time_policy`, `schema_version` — all checked.
- **Canon revision**: `Some(0)` asserted (correct default).

### `test_world_membership_domain_contract_roundtrip`
- Same pattern for `WorldMembership`.
- **Drift note documented**: Schema defines `["owner", "maintainer", "collaborator", "official_creator"]`; domain has `[Owner, Admin, Curator, Collaborator, Viewer]`. The roundtrip works because role is stored as `String`, but a future reconciliation is needed.

### Assessment:
- **Naming**: Clear, follows `test_<domain>_<concern>` pattern.
- **Isolation**: Each test is self-contained, no shared state.
- **Assertions**: Meaningful, checking actual values not just `is_some()`.
- **Drift documentation**: Good practice — the comment explains the known gap.

**Finding**: None. Well-structured tests with appropriate documentation of known limitations.

---

## 3. Error Display Tests

**File**: `crates/nexus-domain/src/errors.rs`

### Coverage: 18 error variants tested

All 18 `DomainError` variants now have `#[test] fn test_display_<variant_name>()` tests:

| Variant | Key Assertions |
|---------|---------------|
| `PermissionDenied` | Contains "permission denied" + custom message |
| `CreatorNotPaired` | Contains "not paired" |
| `InvalidTransition` | Contains "invalid state transition" + from/to values |
| `ImmutableConfirmedState` | Contains "immutable" |
| `AlreadyInState` | Contains "already in state" + state name |
| `InvalidState` | Contains "expected" + expected/actual values |
| `UnresolvedConflict` | Contains "unresolved hard conflict" + block ID |
| `TimelineConflict` | Contains "timeline conflict" |
| `CausalityViolation` | Contains "causality violation" |
| `RevisionMismatch` | Contains "revision mismatch" + expected/actual |
| `ValidationError` | Contains "validation error" + custom message |
| `ExcerptTooLong` | Contains "excerpt exceeds maximum length" + actual/max |
| `InvalidStorageConfig` | Contains "invalid storage configuration" |
| `InvalidForkWriteScope` | Contains "invalid fork write scope" |
| `InvalidUri` | Contains "invalid URI" + source_type/reason |
| `InvalidPhaseTransition` | Contains "invalid phase transition" + from/to |
| `ProvisionalRecordsExist` | Contains "provisional records exist" + count |
| `CreatorQuotaExceeded` | Contains "creator quota exceeded" |
| `InvalidIdFormat` | Contains "invalid ID format" |

### Assessment:
- **Naming**: `test_display_<snake_case_variant>` — consistent with Rust test conventions.
- **Assertions**: Each test verifies **message content** (using `assert!(msg.contains(...))`), not just that `.to_string()` doesn't panic — this is meaningful validation.
- **Assertion quality**: For variants with structured data (e.g., `RevisionMismatch { expected, actual }`), both numeric values are verified in the message.

**Finding**: None. Comprehensive, well-named, meaningful tests.

---

## 4. `#[allow(dead_code)]` on Unused Variants

**File**: `crates/nexus-domain/src/errors.rs`

```rust
/// Creator quota exceeded.
// V1.1: wire into ID validation and quota checks
#[allow(dead_code)]
#[error("creator quota exceeded: {0}")]
CreatorQuotaExceeded(String),

/// Invalid ID format.
// V1.1: wire into ID validation and quota checks
#[allow(dead_code)]
#[error("invalid ID format: {0}")]
InvalidIdFormat(String),
```

### Assessment:
- **Documentation**: Each has a comment indicating `// V1.1: wire into ID validation and quota checks` — explains why they're unused now and when they'll be used.
- **Appropriateness for V1.0**: The variants exist with proper `#[error(...)]` messages, so when they're eventually wired in V1.1, no semantic changes needed — just removal of `#[allow(dead_code)]`.
- **No safety concern**: `#[allow(dead_code)]` is scoped to the variant, not the whole module.

**Finding**: Acceptable. Well-documented technical debt with clear V1.1 plan.

---

## 5. `status.json` Updates

Three residual findings marked `lifecycle: resolved` with `closed_at` and `closure_note`:

| Finding | Resolution |
|---------|-----------|
| `unit_kind` schema drift | Removed from required — codegen already optional |
| Missing roundtrip tests | Added 2 tests with drift documentation |
| Error Display impl tests | Added 18 Display tests + `#[allow(dead_code)]` on unused variants |

`tech_debt_summary` correctly updated: V1.0 GA count 3→0, domain-models 4→1.

---

## 6. Verification Constraints

⚠️ **Cannot run verification commands** due to environment restrictions (`pnpm run codegen`, `cargo test -p nexus-domain` not permitted). 

**Manual code review confirms**:
- Schema change is minimal and safe
- Tests follow correct patterns
- `#[allow(dead_code)]` usage is appropriate

**Recommend**: @project-manager to run `pnpm run codegen && cargo test -p nexus-domain` locally before merge to confirm 154+ tests pass.

---

## Summary

| Category | Status |
|----------|--------|
| Schema change | ✅ Clean, documented |
| Roundtrip tests | ✅ Well-structured, good drift docs |
| Error Display tests | ✅ 18/18 variants covered, meaningful assertions |
| `#[allow(dead_code)]` | ✅ Appropriately scoped, V1.1 TODOs clear |
| Residual tracking | ✅ status.json properly updated |
| V1.0 safety | ✅ No breaking changes |

**No blocking issues identified.**

---

## Cross-Reviewer Notes

- **qc-specialist-1/2**: May want to verify test assertions are sufficient for the roundtrip tests — the `CreatorQuotaExceeded` and `InvalidIdFormat` are now technically dead code but semantically complete. The drift note in `test_world_membership_domain_contract_roundtrip` is a known limitation that should appear in V1.1 planning.
- **Runtime impact**: None — purely additive tests and schema relaxation.
- **Rollback urgency**: Low — change is additive (tests) or relaxing (schema). Rollback would require removing tests and restoring `unit_kind` to required array.

---

**Report prepared**: 2026-04-06  
**Next action**: @project-manager to run local verification and merge
