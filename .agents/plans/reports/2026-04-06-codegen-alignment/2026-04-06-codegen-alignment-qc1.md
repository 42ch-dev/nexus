---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-04-06-codegen-alignment"
verdict: "Request Changes"
generated_at: "2026-04-07"
---

# QC Review Report — Plan E: Codegen & Contract Alignment

**Reviewer**: @qc-specialist (#1)  
**Primary Accent**: Architecture consistency, type safety, schema-codegen alignment  
**Secondary Accent**: Correctness verification, regression testing

---

## Executive Summary

Plan E successfully resolves the core codegen issues (CODEGEN-1, CODEGEN-2) with proper schema-driven iteration and definitions-only handling. However, **Task 3 is incomplete**: registry types were generated but `crates/nexus42/src/acp/registry.rs` was not updated to use them, leaving a discrepancy between the plan goal and actual implementation.

**Blocking Issues**: 1 (TASK3-1)  
**High Severity**: 0  
**Medium Severity**: 1 (DOC-1 outdated comment)  
**Low Severity**: 2

---

## Verification Evidence

### Commands Executed

```bash
# Git inspection
git log --oneline -20                     # Commit 87b3201 identified
git show 87b3201 --stat                   # 51 files changed, 1115 insertions, 670 deletions
git status                                # Clean working tree

# Linting (stable rustfmt limitation noted)
cargo fmt --check --all                   # Formatting diffs in generated files (expected per AGENTS.md)
rg -c "unwrap\(\)" --type rust            # 113 occurrences (existing baseline, not in changed code)

# Content verification
rg "serde_json::Value" crates/nexus42/src/acp/registry.rs  # 1 occurrence (extensions field)
rg "use nexus_contracts" crates/nexus42/src/acp/registry.rs  # 0 occurrences (hand-written types)
```

### Test Claims (from commit message)

> All verification passing:
> - cargo test --all: 480+ tests pass
> - cargo clippy --all -- -D warnings: clean
> - pnpm run typecheck: clean
> - pnpm run codegen: produces correct output

**Source Attribution**: Commit message claims (not independently verified due to permission restrictions).

---

## Findings

### BLOCKING

#### TASK3-1: Task 3 incomplete — registry.rs not updated to use generated types

**Severity**: CRITICAL  
**Category**: Correctness / Plan Incomplete  
**Scope**: `crates/nexus42/src/acp/registry.rs`

**Finding**:
Plan Task 3 states: "Replace `serde_json::Value` in `crates/nexus42/src/acp/registry.rs` with generated types"

However, `registry.rs` defines its own hand-written types (`Registry`, `AgentEntry`, `Distribution`, etc.) at lines 70-200+ and does **not** import from `nexus_contracts::generated::registry_manifest`.

Evidence:
```rust
// crates/nexus42/src/acp/registry.rs:70-79
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Registry {
    pub version: String,
    pub agents: Vec<AgentEntry>,
    #[serde(default)]
    pub extensions: Vec<serde_json::Value>,  // Still serde_json::Value
}
```

The generated types exist in `crates/nexus-contracts/src/generated/registry_manifest.rs` with proper nested structs (`AgentEntry`, `Distribution`, `NpxDistribution`, `BinaryDistribution`, `PlatformBinary`), but they are unused.

**Impact**:
- Plan task explicitly marked as done (`[x]`) but not actually implemented
- Duplicate type definitions create maintenance burden
- Violates single source of truth principle (schema → generated types)

**Root Cause**:
The module docstring (lines 25-32) claims:
> The codegen pipeline only produces flat structs, not nested types

This is **outdated** — the codegen now produces nested types correctly (verified in `registry_manifest.rs`).

**Recommendation**:
1. Update `registry.rs` to import types from `nexus_contracts::generated::registry_manifest`
2. Remove hand-written type definitions
3. Update the module docstring to reflect current codegen capabilities
4. Re-run tests to verify no breakage

**Traceability**: Plan E Task 3 → `schemas/acp-runtime/registry-manifest.schema.json` → `crates/nexus-contracts/src/generated/registry_manifest.rs` (unused)

---

### MEDIUM

#### DOC-1: Outdated comment in registry.rs

**Severity**: MEDIUM  
**Category**: Documentation Drift  
**Scope**: `crates/nexus42/src/acp/registry.rs:25-32`

**Finding**:
The module docstring states the codegen pipeline "only produces flat structs, not nested types" (line 28). This is now incorrect — the generated `registry_manifest.rs` contains proper nested types (`Distribution`, `NpxDistribution`, `BinaryDistribution`, `PlatformBinary`).

**Recommendation**:
Update or remove this comment block. If the plan is to keep hand-written types, document the actual reason (e.g., CDN JSON source, cache compatibility, additional methods).

---

### LOW

#### FMT-1: Generated file formatting diffs

**Severity**: LOW  
**Category**: Tooling Limitation  
**Scope**: `crates/nexus-contracts/src/generated/*.rs`

**Finding**:
`cargo fmt --check` shows import ordering diffs in generated files (e.g., `use crate::generated::common_types::{SourceAnchor}` vs `use crate::generated::common_types::SourceAnchor`).

**Context**:
This is expected behavior per AGENTS.md §"Rust development":
> stable `cargo fmt` cannot apply `ignore`, and formatting generated Rust would desync CI `verify-codegen` from `pnpm run codegen`

**Recommendation**:
No action required. CI uses `cargo +nightly fmt` with `.rustfmt.toml` ignore directive.

---

#### TEST-1: Test count claim not independently verified

**Severity**: LOW  
**Category**: Evidence Quality  
**Scope**: Plan E success criteria

**Finding**:
Commit message claims "480+ tests pass" and "cargo clippy clean", but QC could not independently verify due to bash permission restrictions for `cargo test` and `cargo clippy` (non-git commands).

**Context**:
This is a tooling limitation in the review environment, not a plan defect.

**Recommendation**:
For future reviews, ensure `cargo test` and `cargo clippy` are allowed in permission model for QC agents.

---

## Task-by-Task Review

### Task 1: Fix CommonTypes Codegen ✅ PASS

**Evidence**:
- `tooling/codegen/src/ts-generator.ts:350`: `for (const [name, def] of COMMON_DEFINITIONS.entries())` — iterates over map
- `tooling/codegen/src/rust-generator.ts:485`: Same pattern in Rust generator
- Generated `CommonTypes.ts` and `common_types.rs` reflect schema definitions

**Conclusion**: Resolves CODEGEN-1. Schema-driven iteration replaces hardcoded lists.

---

### Task 2: Generate Context Assembly Types ✅ PASS

**Evidence**:
- `tooling/codegen/src/schema-loader.ts:87-88`: `isDefinitionsOnly` now checks `properties` presence, not `SKIP_STRUCT_GENERATION`
- `schemas/platform/context-assembly-v1.schema.json`: Definitions-only schema (no top-level properties)
- Generated files exist:
  - `packages/nexus-contracts/src/generated/ContextAssemblyV1.ts` (41 lines, with inline array types)
  - `crates/nexus-contracts/src/generated/context_assembly_v1.rs` (88 lines, with parent-prefixed inline structs)
- `crates/nexus42/src/context/types.rs:8-9`: Re-exports from `nexus_contracts::generated`

**Conclusion**: Resolves CODEGEN-2 and CTX-R6. Context types now generated, not hand-written.

---

### Task 3: Registry Manifest Types ⚠️ PARTIAL

**Evidence**:
- `schemas/acp-runtime/registry-manifest.schema.json`: Defines nested definitions (AgentEntry, Distribution, etc.)
- `crates/nexus-contracts/src/generated/registry_manifest.rs`: Generated types exist (88 lines)
- `crates/nexus42/src/acp/registry.rs`: Still uses hand-written types, does not import generated

**Conclusion**: Types generated correctly, but Task 3 goal ("Replace serde_json::Value with generated types") not achieved. See TASK3-1.

---

## Architecture Review

### Schema-First Design ✅

The codegen pipeline now correctly enforces schema-as-single-source-of-truth:
- `COMMON_DEFINITIONS` map populated from `common.schema.json` (schema-loader.ts:58-74)
- CommonTypes generated by iterating map, not hardcoding (ts-generator.ts:350, rust-generator.ts:485)
- Definitions-only schemas now produce output (isDefinitionsOnly fix at schema-loader.ts:87-88)

### Type Safety Improvements ✅

- Inline array objects get parent-prefixed named types (e.g., `ContextAssembleResponseV1KeyBlock`)
- Hyphenated property names properly quoted in TS (`'prop-name'`)
- `$`-prefixed fields handled in Rust (`dollar_` prefix + serde rename)

### Residual Risk

**registry.rs duplication** (TASK3-1) creates:
- Drift risk: schema evolves, generated types update, hand-written types lag
- Testing gap: generated types untested, hand-written types tested
- Maintenance cost: two parallel type hierarchies for same data

---

## Cross-Reviewer Notes

**For QC #2 / #3**: 
- Verify `cargo test --all` and `cargo clippy` independently
- Check if any other files besides `registry.rs` were supposed to be updated for Task 3
- Review `nexus-sync/src/` changes for type compatibility (delta_bundle.rs, precheck.rs, outbox.rs all modified)

**Integration Risk**:
- `registry.rs` is called by `crates/nexus42/src/acp/client.rs` and `crates/nexus42/src/commands/agent.rs`
- Switching to generated types may affect cache serialization format or method signatures

---

## Checklist Results

- [x] Task 1: CommonTypes iteration over COMMON_DEFINITIONS map
- [x] Task 2: isDefinitionsOnly fix allows context-assembly-v1 types
- [x] Task 2: Generated ContextAssembleRequestV1 and ContextAssembleResponseV1
- [x] Task 2: types.rs imports from nexus-contracts
- [ ] **Task 3: registry.rs updated to use generated types** ← INCOMPLETE
- [x] Schema changes committed with generated output (per AGENTS.md)
- [?] No new `.unwrap()` in production code (baseline unchanged)
- [?] All tests pass (commit claims 480+, not independently verified)

---

## Gate Recommendation

**REQUEST CHANGES**

**Rationale**:
- Task 3 is explicitly marked complete but the stated goal ("Replace serde_json::Value with generated types") was not executed
- Generated types exist and are correct, but are unused — this is incomplete work
- Fix is straightforward: update imports, remove hand-written types, update docstring
- Estimated effort: 30 minutes to 1 hour

**Required Actions**:
1. Update `crates/nexus42/src/acp/registry.rs` to use `nexus_contracts::generated::registry_manifest::*`
2. Remove hand-written Registry, AgentEntry, Distribution, etc. structs
3. Update module docstring (lines 25-32) to reflect codegen capabilities
4. Run `cargo test --all` and `cargo clippy` to verify no breakage
5. Commit with message indicating Task 3 completion

**After Fix**:
- Re-run QC review (or delegate to QC #2/#3 for faster turnaround)
- Merge approval likely — no other blocking issues identified

---

## Traceability

| Finding | Source | References |
|---------|--------|------------|
| TASK3-1 | Plan Task 3, schema, generated file, hand-written file | `schemas/acp-runtime/registry-manifest.schema.json`, `crates/nexus-contracts/src/generated/registry_manifest.rs`, `crates/nexus42/src/acp/registry.rs` |
| DOC-1 | Code comment | `crates/nexus42/src/acp/registry.rs:25-32` |
| FMT-1 | Tooling output | `cargo fmt --check` |
| TEST-1 | Permission restriction | QC environment bash permissions |

---

**Reviewer**: @qc-specialist  
**Confidence**: High (code inspection, schema verification, diff analysis)  
**Next Action**: Return to @project-manager for task re-assignment or fix delegation