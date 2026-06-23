---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-23-v1.62-spec-extraction-compute-abi-and-wasm-host"
verdict: "Approve"
generated_at: "2026-06-24"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1 (xai/grok-build-0.1)
- Review Perspective: Content correctness (security/correctness adapted for docs) — does the spec accurately describe what was implemented / what exists in V1.61 source + P0/P1 outcomes?
- Report Timestamp: 2026-06-24

## Scope
- plan_id: 2026-06-23-v1.62-spec-extraction-compute-abi-and-wasm-host
- Review range / Diff basis: merge-base iteration/v1.62 @ f77b3de8 → feature/v1.62-spec-extraction @ 2424c760 (1 commit)
- Working branch (verified): feature/v1.62-spec-extraction
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.62-p2-specs
- Files reviewed: 6 (2 new Masters + 4 amendments)
- Commit range: 2424c760bf4f014a378421598598eed1c425e98b
- Tools run: rg (ripgrep), python3 (JSON schema inspection), git branch/rev-parse/log/show, direct file reads of specs + cross-referenced source in main + .worktrees/v1.62-p1-manifest

## Verification Commands Executed (from worktree)
```bash
cd /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.62-p2-specs
git branch --show-current  # feature/v1.62-spec-extraction
git rev-parse HEAD         # 2424c760

# Module exports (basic-combat)
rg 'no_mangle|pub extern' /Users/bibi/workspace/organizations/42ch/nexus/modules/basic-combat/src/lib.rs

# Manifest + ModuleSchemas (P1)
rg 'pub struct ModuleManifest|pub struct ModuleSchemas|pub [a-z_]+:' .../v1.62-p1-manifest/crates/nexus-wasm-host/src/manifest.rs

# ComputeError variants (P1)
rg 'pub enum ComputeError|^\s+[A-Z][A-Za-z]+' .../v1.62-p1-manifest/crates/nexus-wasm-host/src/error.rs

# Sandbox defaults
rg 'fuel|memory_size|wall_time|Duration::from' /Users/bibi/workspace/organizations/42ch/nexus/crates/nexus-wasm-host/src/sandbox.rs

# Watchdog + host imports
rg 'watchdog|spawn_watchdog|epoch|AtomicBool' .../crates/nexus-wasm-host/src/compute.rs
rg 'kb_read|narrative_query' .../crates/nexus-wasm-host/src/host.rs

# Schema shape verification (top-level keys)
python3 -c 'import json; ... compute-input.schema.json / compute-output.schema.json'
```

## Findings

### 🔴 Critical
- None.

### 🟡 Warning
- None.

### 🟢 Suggestion
- None material. All content-correctness checkpoints passed with exact matches between spec prose and source.

## Detailed Cross-Check Results (qc2 focus)

### 1. `compute-module-abi.md` accuracy
- **Module exports table (§2)**: Signatures `alloc(len: u32) -> u32`, `init()`, `compute(in_ptr, in_len, out_ptr, out_cap) -> i64` **exactly match** `modules/basic-combat/src/lib.rs` (`#[no_mangle] pub extern "C" fn ...`).
- **Host import ABI (§3)**: `nexus::kb_read(id_ptr, id_len, out_ptr, out_cap) -> i64` and `nexus::narrative_query` signatures + behavior (O(1) lookup from pre-built `HostContext`, `-1`/`-2` sentinels) **exactly match** `crates/nexus-wasm-host/src/host.rs`.
- **`ComputeInput` envelope (§4)**: Fields `schema_version`, `world_ref`, `key_blocks`, `narrative_state`, `invocation` + required list **exactly match** `schemas/local-api/compute/compute-input.schema.json`.
- **`ComputeOutput` 4-part structure (§5)**: `schema_version`, `state_delta`, `timeline_events`, `new_key_blocks`, `battle_report` **exactly match** `schemas/local-api/compute/compute-output.schema.json`. Host apply order documented correctly.
- **Marshalling convention (§6)**: `alloc` ptr/len/cap, error sentinels `-1` (generic), `-2` (buffer too small) **exactly match** both module side and `compute.rs` / `host.rs` implementation.
- **`manifest.json` contract (§7)**: Required + optional fields + NEW `schemas` block (4 sub-objects: `key_block_attributes`, `key_block_state`, `invocation`, `battle_report`) **exactly match** `ModuleManifest` + `ModuleSchemas` in P1 `manifest.rs`. `ManifestValidationFailed { path, detail }` correctly referenced.
- Cross-refs to `wasm-host.md`, `entity-scope-model.md` §5.5.9, `orchestration-engine.md` §8, and `schemas-directory-layout.md` are bidirectional and accurate.

### 2. `wasm-host.md` accuracy
- **Engine lifecycle (§2)**: Single daemon-wide `WasmEngine`, `Arc<RwLock<HashMap>>` module cache, embedded + user discovery phases **match** `engine.rs` + `module_cache.rs` + boot sequence.
- **Sandbox limits defaults (§4)**: `DEFAULT_FUEL = 10_000_000`, `DEFAULT_MEMORY_MIB = 64`, `DEFAULT_WALL_TIME = Duration::from_secs(30)` **exactly match** `sandbox.rs` constants and `SandboxConfig::default()`.
- **Wall-time watchdog (§5)**: 25 ms step, `Arc<AtomicBool>` cancellation, `spawn_watchdog`, `Engine::increment_epoch()`, `epoch_deadline_trap`, `set_epoch_deadline(1)` **exactly match** `compute.rs`.
- **Embedded module loading (§6)**: `build.rs` compile-from-source + `include_dir!` + `MODULE_IDS: &["basic-combat"]` **match** `build.rs`.
- **User module discovery (§7)**: `~/.nexus42/modules/` single-level scan + manifest parse + cache insert + warn-on-failure **matches** `embedded.rs` / daemon-side loading.
- **Error taxonomy (§8)**: All `ComputeError` variants listed (including `ManifestValidationFailed { path, detail }` added by P1) **exactly match** `error.rs`. Taxonomy sections (loading, instantiation, sandbox, execution, manifest validation) correctly categorized.
- Host function implementation (§9) matches `host.rs` (immutable snapshot from `ComputeInput`, `kb_read` O(1) index, `narrative_query` pass-through).

### 3. Amendment correctness
- `entity-scope-model.md` §5.5.9 (computable-flag + structured validation):
  - Explicitly states "manifest-declared shapes (NOT deleted entity-attr/state schemas)".
  - Documents deletion of `schemas/compute/compute-entity-attributes.schema.json` and `compute-entity-state.schema.json` (P0).
  - Describes `computable: Option<bool>`, `state: Option<serde_json::Value>`, `ValidationMode::Structured` routing to `manifest.json` `schemas.*` fragments.
  - Correctly closes `R-V161P1-LOW-001` in header and body.
- `orchestration-engine.md` §5.2 + §8.4:
  - `narrative.compute` capability entry (name, input, behavior, side-effects on computable KeyBlocks) accurate per V1.61 P3 implementation + P2 spec-seal.
  - `combat-engine` preset (ID, primary capabilities, states, `load_world` → `narrative.compute` delegation) accurately described.
- `schemas-directory-layout.md` amendment: correctly reflects P0 tree shape (compute/ moved to `local-api/compute/`, entity-* deleted).
- `specs/README.md` Master index updates: accurate (new specs + V1.62 Shipped statuses).

### 4. No revisionist history
- Specs describe **post-V1.62** state.
- Correctly credit V1.61 compass grill decisions (Q3/Q6/Q8, Q1/Q6/Q10), P0 (schemas reorg), P1 (manifest `schemas` block + `ManifestValidationFailed`).
- No invented past behavior; no claims that contradict V1.61 compass or actual code.
- All "deferred to V2" items listed in §9 of `compute-module-abi.md` match V1.61 compass non-goals.

### 5. Cross-reference hygiene
- Bidirectional links between the two new Masters and the four amended specs are present and correct.
- No broken internal spec links detected via targeted grep.

## Source Trace
- Finding ID: (none — clean review)
- Source Type: manual code-vs-spec cross-check + rg + schema JSON inspection
- Source Reference: see "Verification Commands Executed" + detailed sections above
- Confidence: High (exact textual matches on signatures, constants, field names, error variants, and schema top-level shapes)

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 0 |

**Verdict**: Approve

## Notes for PM
- This is a **docs-only** plan (spec extraction + amendments). Content correctness review focused exclusively on fidelity between normative text and actual V1.61 + P0/P1 implementation.
- No material mis-descriptions found. Specs are accurate, complete, and correctly scoped.
- Pre-existing items (e.g., R-V161P0-LOW-001), compass prose count drift, and in-flight P1 qc3 depth-limit work were excluded per assignment.
- Ready for consolidated decision and subsequent QA (if not waived).

---

*Report generated by qc-specialist-2 per mstar-review-qc template. All checks executed from assigned worktree with verified branch + HEAD.*
