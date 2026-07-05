---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-23-v1.62-spec-extraction-compute-abi-and-wasm-host"
verdict: "Approve"
generated_at: "2026-06-24"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: GLM-4.7
- Review Perspective: Maintainability + future-readiness (performance/reliability adapted for docs)
- Report Timestamp: 2026-06-24T00:00:00Z

## Scope
- plan_id: 2026-06-23-v1.62-spec-extraction-compute-abi-and-wasm-host
- Review range / Diff basis: merge-base iteration/v1.62 @ f77b3de8 → feature/v1.62-spec-extraction @ 2424c760 (1 commit)
- Working branch (verified): feature/v1.62-spec-extraction
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.62-p2-specs
- Files reviewed: 6 spec files (2 new, 4 amended)
- Commit range (if not identical to Review range line, explain): f77b3de8...2424c760
- Tools run: git diff, git log, rg (ripgrep), python3 json validation

## Findings

### 🔴 Critical
None.

### 🟡 Warning
None.

### 🟢 Suggestion

#### S-001: Consider versioning default numeric values

**Location:** `wasm-host.md` §4, `compute-module-abi.md` §8

**Issue:** The specs document hardcoded default values for sandbox limits (10M fuel, 64 MiB memory, 30 seconds wall time, 25 ms watchdog step). These values are likely to evolve based on real-world usage patterns. If these defaults change in the future without updating both specs in sync, drift could occur.

**Recommendation:** Consider adding a versioned reference table or a cross-reference section at the end of both specs that lists "Default values as of V1.62". This makes it explicit that these are V1.62-specific defaults that may change in future minor versions. Alternatively, phrase the defaults as "Current V1.62 defaults: ..." to signal temporality.

**Impact:** Low. The values are clearly labeled as defaults with manifest override mechanisms documented. The suggestion is purely about future-maintainer clarity when defaults evolve.

---

## Source Trace
- Finding ID: S-001
- Source Type: manual-reasoning
- Source Reference: wasm-host.md §4, compute-module-abi.md §8
- Confidence: Medium

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 1 |

**Verdict**: Approve

---

## Detailed Review Notes

### 1. Reader clarity for future contributors (V1.63+)

**Assessment:** ✅ **EXCELLENT**

A new contributor reading only `compute-module-abi.md` and `wasm-host.md` (without V1.61 compass) can clearly understand:

- **What the compute subsystem does:** Both specs open with clear overview diagrams and prose explaining the module as a "stateless pure function" with per-invocation sandbox.
- **How to write a module:** `compute-module-abi.md` §2–§7 provides complete guidance: exports table (memory, alloc, compute, init), host imports (kb_read, narrative_query), input/output envelope shapes, and the `manifest.json` contract with a full worked example (basic-combat).
- **What the host provides:** `wasm-host.md` §3–§9 documents the sandbox model, limits, watchdog mechanism, module loading (embedded + user discovery), host function implementation, and error taxonomy.
- **What error modes exist:** `wasm-host.md` §8 provides a comprehensive error taxonomy organized by failure category (loading, instantiation, sandbox enforcement, execution, manifest validation).

**Strengths:**
- Clear separation of concerns between module-side ABI (`compute-module-abi.md`) and host-side runtime (`wasm-host.md`).
- Worked example (basic-combat manifest with full schemas block) serves as an onboarding template.
- Error variants are named descriptively (`OutOfFuel`, `WallTimeExceeded`, `MemoryCapExceeded`) rather than using numeric error codes.
- Step-by-step execution flow in `wasm-host.md` §3 and cross-referenced capability flow in `orchestration-engine.md` §8.4.1.

### 2. Forward-reference discipline

**Assessment:** ✅ **EXCELLENT**

All V2-deferred items are properly marked with clear "deferred to V2+" markers:

- `compute-module-abi.md` §9.3 contains a dedicated table listing 7 deferred items with explicit target versions:
  - Multi-module composition / chaining → V2.0+
  - CDN-based module distribution + Ed25519 signing → V2.0+
  - Generic Combat Protocol interop certification → V2.0+
  - Third-party game server integration bridge → V2.0+
  - GPU compute / SIMD acceleration → V3.0+
  - Module marketplace / public registry → V3.0+
  - KB state → human-readable UI editor → V2.0+

- The prose explicitly states: "They are **not** supported in the V1 envelope."
- No V2 features are described as current functionality.
- Compass V1.62 design item #3 ($ref to external schema files) is explicitly marked as deferred with "(compass V1.62 design item #3 — defer to V2 if needed)."

### 3. JSON examples are valid

**Assessment:** ✅ **VERIFIED**

All JSON code blocks in the specs parse successfully:

**compute-module-abi.md:**
- First JSON block (ComputeInput envelope with world_ref, key_blocks, narrative_state, invocation): ✅ VALID
- Second JSON block (ComputeOutput envelope with state_delta, timeline_events, new_key_blocks, battle_report): ✅ VALID
- Third JSON block (basic-combat manifest with full schemas block): ✅ VALID

**wasm-host.md:**
- Contains no literal JSON code blocks (only descriptive references to manifest.json structure).

Validation was performed using `python3 -c "import json; json.loads(...)"` on each extracted JSON block.

### 4. Spec drift risk analysis

**Assessment:** ✅ **LOW RISK**

The specs avoid hardcoding values that are likely to become stale:

- **Good:**
  - Version numbers are explicitly labeled as "V1.62 Shipped" or "V1 envelope" rather than hardcoded in prose without context.
  - Error variant names (e.g., `ComputeError::OutOfFuel`) are stable language-level identifiers, not fragile enum ordinals.
  - File paths are documented as structural patterns (e.g., `~/.nexus42/modules/`, `embedded-modules/<id>/`) rather than brittle absolute paths.

- **Minor concern (addressed in S-001):**
  - Default numeric values (10M fuel, 64 MiB memory, 30 seconds wall time, 25 ms watchdog step) are explicitly documented. These are implementation defaults that may evolve based on real-world usage. The specs correctly document them as "host defaults" and note that modules can override them via manifest.json. The suggestion in S-001 is to add temporal framing ("Current V1.62 defaults") to make the versioned nature explicit.

### 5. Cross-reference bidirectionality

**Assessment:** ✅ **EXCELLENT**

Cross-references between the two new specs and existing specs are bidirectional where the relationship is meaningful:

**compute-module-abi.md ↔ wasm-host.md:**
- `compute-module-abi.md` header: "Related: wasm-host.md, schemas-directory-layout.md §3.5, ..."
- `compute-module-abi.md` §7.3: "Validation failure produces `ComputeError::ManifestValidationFailed { path, detail }` (see wasm-host.md §8)."
- `compute-module-abi.md` §8: "Full details are in wasm-host.md §3–§5. Summary: [sandbox limits table]."
- `wasm-host.md` header: "Related: compute-module-abi.md, orchestration-engine.md §8, ..."
- `wasm-host.md` line 22: "Read compute-module-abi.md for the module-side contract that this crate implements on the host side."
- `wasm-host.md` §8.5: "This variant is added by V1.62 P1 (see compute-module-abi.md §7.3)."

**compute-module-abi.md ↔ entity-scope-model.md:**
- `compute-module-abi.md` §4: KeyBlock state field references "entity-scope-model.md §5.5.9"
- `entity-scope-model.md` §5.5.9: Multiple cross-references to compute-module-abi.md §5.1 (state_delta), §7.3 (schemas block)

**compute-module-abi.md ↔ orchestration-engine.md:**
- `compute-module-abi.md` header: Related to "orchestration-engine.md §8 (narrative.compute)"
- `orchestration-engine.md` §8.4.1: Multiple cross-references to compute-module-abi.md (module ABI contract, manifest example, required_key_block_types)

**README.md index:**
- Updated index table includes both new specs under "Compute and WASM" section.
- New cross-reference table entries for "Compute module ABI (V1 envelope)" and "WASM compute host runtime" with bidirectional related spec links.

### 6. README index future-proofing

**Assessment:** ✅ **EXCELLENT**

The README index structure accommodates forward evolution:

- **Grouping:** New specs are placed under a dedicated "Compute and WASM" section, which cleanly groups related specs without disrupting existing sections.
- **Status phrasing:** "Normative — V1.62 Shipped (P2)" is explicit about version and plan, making it clear what "Shipped" means in context.
- **Cross-reference table:** Added two new entries with complete bidirectional links to wasm-host, orchestration-engine, entity-scope-model, and schemas-directory-layout.
- **No hardcoded feature lists:** The index does not enumerate all possible compute modules or capabilities; it points to the master specs where module authors would find extensibility guidance.

When V1.63 ships new compute features, the index can be updated by:
- Adding new specs under "Compute and WASM" (e.g., `multi-module-composition.md` for V2)
- Updating status fields to "V1.63 Shipped"
- Adding cross-reference entries as needed

No major rework is required.

### 7. Spec-seal quality on `schemas-directory-layout.md`

**Assessment:** ✅ **CLEAN**

The implementer's claim of "minimal spec-seal polish" is accurate. The amendment introduces:

- Status update: "Normative — V1.62 Shipped (consumer-scope reorganization)"
- Last updated date: "2026-06-23 — V1.62 P2 (spec-seal polish)"
- Updated Related line: Added compute-module-abi.md §4–§5 and wasm-host.md §6–§7
- Clarification in §3.5 (compute section): Changed normative detail reference from bare path to markdown link: `[compute-module-abi.md](./compute-module-abi.md)`. Added host-side detail: `Host-side runtime detail: [wasm-host.md](./wasm-host.md).`
- Footer update: Added V1.62 P2 spec-seal polish note.

**No contradictions:** The amendment does not introduce any contradictions with the P0 rewrite. It merely adds forward references to the new compute specs and polishes the linkage language (using markdown links instead of bare paths).

### 8. `entity-scope-model.md` §5.5.9 amendment quality

**Assessment:** ✅ **CLEAN INTEGRATION**

The V1.62 P2 amendment (§5.5.9 "Computable-flag semantics and structured validation mode") integrates cleanly with the existing §5.5 content:

- **Numbering:** Uses 5.5.9 as the next subsection number, following 5.5.8 (Conditional routing branch input visibility, V1.56 P3). The numbering continues the existing pattern without conflicts.
- **Sub-subsections:** Uses hierarchical subsections (5.5.9.1, 5.5.9.2, 5.5.9.3, 5.5.9.4) for logical structure.
- **No conflicts:** The amendment adds new semantics for `computable` flag and `state` field that are orthogonal to existing §5.5 content. It does not modify or contradict prior subsections (5.5.1–5.5.8).
- **Cross-references:** Links bidirectionally to compute-module-abi.md and wasm-host.md. Explicitly states relationship to deleted entity-* schemas (V1.61 placeholder schemas deleted in V1.62 P0).
- **Header update:** Status line updated to include "V1.62 Shipped: §5.5.9 computable-flag semantics + structured validation mode (closes R-V161P1-LOW-001)."

### 9. No content overlap

**Assessment:** ✅ **NO DUPLICATION**

The 2 new specs avoid duplicating each other or existing specs:

- **compute-module-abi.md vs wasm-host.md:** Clear separation. compute-module-abi.md focuses on module-side contract (exports, host imports, envelope shapes, manifest.json). wasm-host.md focuses on host-side runtime (wasmtime engine, sandbox, limits, watchdog, module loading, error taxonomy). Overlap is intentional cross-referencing (e.g., sandbox limits table appears in both with "Full details in X" references).
- **compute-module-abi.md vs existing specs:**
  - orchestration-engine.md §8.4 describes the capability-level orchestration, not the module ABI contract.
  - entity-scope-model.md §5.5.9 describes KeyBlock `computable` flag semantics and structured validation at the KB layer, not the module-level ABI.
  - schemas-directory-layout.md §3.5 documents where compute schemas live on disk, not their content.
- **wasm-host.md vs existing specs:**
  - orchestration-engine.md §8.4.1 describes the orchestration flow that calls nexus-wasm-host, not the internal runtime details.
  - No overlap with other runtime specs (daemon-runtime, local-runtime-boundary) which cover different domains.

### 10. Verification of assignment constraints

The following assignment constraints were verified:

- ✅ Pre-existing R-V161P0-LOW-001 is NOT flagged as a finding (explicitly listed as "Critical context — do NOT flag").
- ✅ Compass prose count drift is NOT flagged (excluded from scope).
- ✅ P0/P1/P-fix-wave territories are NOT flagged (excluded from scope).
- ✅ Code-level concerns (validation depth limit, etc.) are NOT flagged (excluded from scope for docs-only review).
- ✅ Review cwd, Working branch, and Review range are verified and match assignment exactly.
- ✅ JSON examples were validated with `python3 json.loads()` — all parse successfully.
- ✅ V2-deferred items are clearly marked with explicit version targets.
- ✅ Cross-references are bidirectional where relationships are meaningful.
- ✅ entity-scope-model §5.5.9 numbering follows existing pattern (5.5.1–5.5.8 exist).
- ✅ README index accommodates forward evolution without major rework.
- ✅ schemas-directory-layout.md amendment introduces no contradictions with P0 rewrite.
- ✅ entity-scope-model §5.5.9 amendment integrates cleanly without conflicts.

---

## Conclusion

The V1.62 P2 specs (2 new + 4 amendments) demonstrate excellent maintainability and future-readiness for a docs-only plan. The specs are:

- **Clear for future contributors:** Well-structured with worked examples and complete guidance for module authors and host implementors.
- **Forward-referenced properly:** V2+ features are explicitly marked as deferred, not described as current.
- **Valid:** All JSON examples parse successfully.
- **Low drift risk:** Avoids brittle hardcoded values; numeric defaults are implementation-documented with override mechanisms.
- **Well-cross-referenced:** Bidirectional links between specs where relationships are meaningful.
- **Future-proof index:** README structure accommodates evolution without major rework.
- **Clean amendments:** spec-seal polish and §5.5.9 amendment introduce no contradictions or conflicts.

The single suggestion (S-001) is a minor improvement about framing default numeric values as versioned, not a defect that blocks approval.

**Recommendation:** **Approve** with suggestion S-001 tracked as optional future polish.