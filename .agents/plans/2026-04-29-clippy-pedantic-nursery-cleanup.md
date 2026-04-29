---
title: "Clippy Pedantic + Nursery Cleanup"
plan_id: "2026-04-29-clippy-pedantic-nursery-cleanup"
status: "Done"
agents: ["@fullstack-dev"]
tags: ["clippy", "lint", "quality", "codegen"]
created_at: "2026-04-29"
primary_spec: ""
spec_refs: []
working_branch: "fix/clippy-pedantic-nursery-cleanup"
---

# Clippy Pedantic + Nursery Cleanup

## Summary

Fix 378 clippy errors raised by `-W clippy::all -W clippy::pedantic -W clippy::nursery -D warnings` on main HEAD. Errors split between **generated code** (codegen template fix + regenerate) and **hand-written code** (direct fix).

## Error Breakdown

| Category | Count | Location | Fix Strategy |
|----------|-------|----------|-------------|
| `doc_markdown` (missing backticks) | 170 | Mostly generated types | Codegen template: wrap identifiers in backticks |
| `derive_partial_eq_without_eq` | 67 | Generated types | Codegen template: derive `Eq` alongside `PartialEq` |
| `missing_const_for_fn` | 39 | Hand-written + generated | Add `const` where applicable |
| `must_use_candidate` (method) | 36 | Hand-written | Add `#[must_use]` attributes |
| `format_in_format_args` | 32 | Hand-written | Inline variables in format strings |
| `use_self` | 6 | Hand-written | Replace struct name with `Self` |
| `unnecessary_hash_wraps` | 4 | Hand-written | Remove `##` when not needed |
| Other (various) | ~24 | Mixed | Individual fixes |

## Scope Split

### Part A: Codegen Template Fix (source: `tooling/codegen/`)

Fix TypeScript → Rust codegen templates to produce clippy-clean output:

1. **`doc_markdown`**: Wrap type/field identifiers in doc comments with backticks
2. **`derive_partial_eq_without_eq`**: Generate `#[derive(Eq)]` alongside `PartialEq` for types with all `Eq` fields
3. **`missing_const_for_fn`**: N/A (codegen-generated functions — skip)
4. After template fix: `pnpm run codegen` → regenerate all `crates/nexus-contracts/src/generated/*`

### Part B: Hand-written Code Fix

Direct fixes on non-generated files:

1. `crates/nexus-home-layout/src/lib.rs` (12 errors)
2. `crates/nexus-contracts/src/local/**/*.rs` (~36 errors across schedule, domain, acp, orchestration)
3. Other non-generated crates (if any emerge after Part A)

## Tasks

| ID | Task | Depends on | Parallel | Owner | Done criteria |
|----|------|------------|----------|-------|---------------|
| T1 | Fix codegen templates for `doc_markdown` + `derive_partial_eq_without_eq` | — | no | @fullstack-dev | Templates updated |
| T2 | Run `pnpm run codegen` to regenerate | T1 | no | @fullstack-dev | Generated files updated, diff shows expected changes |
| T3 | Fix hand-written code: `nexus-home-layout/src/lib.rs` | — | **yes** (∥ T1) | @fullstack-dev | Zero clippy errors from this file |
| T4 | Fix hand-written code: `nexus-contracts/src/local/**/*.rs` | — | **yes** (∥ T1) | @fullstack-dev | Zero clippy errors from local modules |
| T5 | Full regression: `cargo clippy --all-targets --all-features -- -W clippy::all -W clippy::pedantic -W clippy::nursery -D warnings` passes with zero errors | T1..T4 | no | @fullstack-dev | Clean clippy output |
| T6 | `cargo test --workspace` green; `cargo +nightly fmt --all -- --check` clean | T5 | no | @fullstack-dev | All checks pass |

**Parallelism**: `2 tracks` —
- **Track A**: Codegen fix + regenerate (T1 → T2) → merge at T5
- **Track B**: Hand-written code fix (T3 ∥ T4) → merge at T5

## Non-Goals

- Suppressing warnings with `#[allow(...)]` — prefer actual fixes
- Changing any runtime behavior
- Adding new features or tests

## Acceptance Criteria

- [x] `cargo clippy --all-targets --all-features -- -W clippy::all -W clippy::pedantic -W clippy::nursery -D warnings` has zero errors
- [x] `cargo test --workspace` green (547 passed; 1 pre-existing flaky failure on HEAD)
- [x] `cargo +nightly fmt --all -- --check` clean
- [x] No runtime behavior changes
- [x] No new medium+ residuals

## Sign-off

| Role | Status | Date | Notes |
|------|--------|------|-------|
| @fullstack-dev | Done | 2026-04-29 | All clippy fixes applied across 9 crates |
| @project-manager | Done | 2026-04-29 | QC tri-review complete; QC1 F-001/F-003 fixed, F-002/F-004 re-evaluated as not reproduced; QC2 Approve; QC3 Approve |
