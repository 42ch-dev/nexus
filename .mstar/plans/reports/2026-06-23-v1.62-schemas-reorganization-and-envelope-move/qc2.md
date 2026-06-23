---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-23-v1.62-schemas-reorganization-and-envelope-move"
verdict: "Approve"
generated_at: "2026-06-23"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1 (xai/grok-build-0.1)
- Review Perspective: Security and correctness risk (URI correctness, codegen fidelity, drift registration, consumer import hygiene, deleted-file integrity)
- Report Timestamp: 2026-06-23

## Scope
- plan_id: 2026-06-23-v1.62-schemas-reorganization-and-envelope-move
- Review range / Diff basis: merge-base iteration/v1.62 @ 3cefb6d9 → feature/v1.62-schemas-reorganization @ 126f041d — equivalent to `git diff 3cefb6d9..126f041d` (3 commits: f8f22d0a..126f041d; 203 files)
- Working branch (verified): feature/v1.62-schemas-reorganization
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus (currently checked out on the Working branch)
- Files reviewed: 203 (schemas tree + codegen output + drift test + boundary docs + consumer updates)
- Commit range: 126f041d33678a2510e3c8ed554033081c35e150 (HEAD matches Assignment)
- Tools run: pnpm run validate-schemas, pnpm run codegen, ./tooling/check-wire-drift.sh, cargo test --test schema_drift_detection, cargo check --workspace, rg + Python $ref resolution audit, grep for stale imports and deleted-file references

## Findings

### 🔴 Critical
(none)

### 🟡 Warning
(none)

### 🟢 Suggestion
- Consider adding a one-line comment in `crates/nexus-kb/src/key_block.rs` and `validation.rs` (near the historical `schemas/compute/entity-*` references) stating the V1.62 deletion date and manifest.json migration target. Current comments are accurate but pre-date the actual deletion; a small hygiene touch would make the historical note self-contained. (Low priority; not a correctness issue.)

## Source Trace
- Finding ID: (N/A — no blocking findings)
- Source Type: static verification + execution
- Source Reference: pnpm run validate-schemas (56/56 ✓), codegen (53 + common ✓), check-wire-drift.sh + cargo test (4/4 ✓), cargo check --workspace (clean), Python $ref audit (207 refs, all resolve), grep for stale generated:: paths (zero hits), ls + drift entry! count cross-check
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 1 |

**Verdict**: Approve

## Detailed Verification (Reviewer #2 — Security + Correctness Lens)

### 1. URI Correctness ($id / $ref)
- `pnpm run validate-schemas` passed cleanly on the post-reorg tree (56 files).
- All sampled moved schemas carry updated `$id`:
  - `schemas/platform/sync/bundle.schema.json` → `https://nexus42.invalid/schemas/platform/sync/bundle.schema.json`
  - `schemas/local-api/compute/compute-input.schema.json` → `.../local-api/compute/...`
  - `schemas/platform/http-bff/*` and remaining `domain/*` files likewise updated.
- 207 `$ref` occurrences across `schemas/`. A deterministic Python walk (resolving `https://nexus42.invalid/...` and relative paths against their declaring file's directory) reports **zero broken targets**. Cross-directory refs (domain → common, http-bff → domain/common, sync → domain/common) all resolve.
- `bundle-refinement.schema.json` (validation-only, allOf) correctly carries its new `$id` and is not a codegen source.

### 2. Codegen Correctness + Type Fidelity
- `pnpm run codegen` succeeded; regenerated 53 schemas + common types into both Rust (`crates/nexus-contracts/src/generated/`) and TS (`packages/nexus-contracts/src/generated/`).
- Spot-check of canonical wire types (post-move):
  - `ComputeInput`: `schema_version`, `world_ref`, `key_blocks`, `narrative_state`, `invocation` (matches pre-reorg expectation).
  - `ComputeOutput`: `schema_version`, `state_delta`, `timeline_events`, `new_key_blocks`, `battle_report`.
  - `Bundle`: full envelope fields (`bundle_id`, `deltas`, `bundle_apply_status`, …) present.
  - `Delta`: operation/payload structure intact.
- `./tooling/check-wire-drift.sh` + `cargo test --test schema_drift_detection` both PASS (4/4 tests). Drift map explicitly registers the new paths (platform/sync/*, local-api/compute/*, platform/http-bff/*).
- `cargo check --workspace` clean — no compile failures from missed consumer import updates.

### 3. SKIP_STRUCT_GENERATION_REL_PATHS Hygiene
- `tooling/codegen/src/schema-loader.ts`:
  ```ts
  const SKIP_STRUCT_GENERATION_REL_PATHS = new Set(['platform/sync/bundle-refinement.schema.json']);
  ```
- Path was correctly updated from the old `cloud-sync/bundle`. This prevents a duplicate `BundleRefinement` struct that would collide with the canonical `Bundle` generated from `platform/sync/bundle.schema.json`. **Critical correctness item — verified.**

### 4. Drift Detection Registration Completeness
- `build_schema_map()` in `crates/nexus-contracts/tests/schema_drift_detection.rs` contains entries for every moved wire schema under its **new** relative path.
- 56 schema files on disk; 54 `entry!` registrations. The small delta is expected and explained:
  - `common/common.schema.json` is definitions-only (intentionally not registered as a top-level struct).
  - `platform/sync/bundle-refinement.schema.json` is intentionally skipped from codegen (and thus from drift checks) per the skip-set.
  - A few http-bff schemas produce multiple top-level structs (registered as arrays).
- The glob-augmented `collect_all_schema_paths` ensures any unregistered schema file is still discovered for cache/ref resolution. No silent omission risk.

### 5. Consumer Import Hygiene
- Exhaustive grep for stale module paths returned zero hits:
  - No `generated::compute::` or `generated::cloud_sync::` references in Rust or TS.
  - No `@42ch/nexus-contracts` imports referencing the old `compute/` or `cloud-sync/` subpaths.
- All consumers that previously imported compute envelopes or sync payloads now correctly reference the new locations (`generated::local_api::compute::*`, `generated::platform::sync::*`, `generated::platform::http_bff::*`).
- `cargo check --workspace` would have caught any missed re-exports or partial renames; none surfaced.

### 6. Deleted-File Integrity (entity-attributes / entity-state)
- Two schemas were intentionally deleted (per-module shapes migrate to `manifest.json` in P1).
- Remaining references are **only**:
  - Historical comments in `crates/nexus-kb/src/{key_block,validation}.rs` (explaining the prior location and the migration rationale).
  - A single comment in the drift test file acknowledging the deletion.
  - Documentation/README notes that correctly describe the change.
- No active `$ref`, `use`, `import`, or runtime path still points at the deleted files. No dangling pointers in the wire surface or generated code.
- The deletion is the designed outcome (closes several V1.61 residuals by supersession).

### 7. Boundary Doc Rewrite (Security / Exposure Surface)
- `.mstar/knowledge/schemas-external-consumer-boundary.md` (renamed from the old platform-wire doc) correctly expands the rule:
  - `schemas/` now includes anything consumed by **any external client** (platform **or** future Local API clients such as WebApp).
  - Explicitly adds the `local-api/compute/` row.
  - Reiterates that per-module shapes are **not** in `schemas/` (they live in manifests).
- The rewrite does **not** broaden exposure beyond the intended compute envelope. No sensitive platform-internal types were accidentally promoted.
- Companion updates to `schemas/AGENTS.md`, `schemas/README.md`, and per-folder READMEs are present and consistent.

### 8. Sensitive Data / Runtime Behavior
- This plan is a pure reorganization + codegen cascade. No runtime data flows, persistence, or authz rules changed.
- No new secrets, PII handling, or network boundaries were introduced.
- The boundary doc change is documentary only; the actual Local API surface for compute was already present (just relocated).

### 9. Items Explicitly Not Raised (per Assignment)
- Pre-existing clippy lints (`R-V161P0-LOW-001` in preset.rs / http.rs) — verified pre-existing on base `3cefb6d9`; closed in P-last T5. Not a P0 regression.
- Compass prose count (33 vs 34 platform files) — already corrected in the layout spec §7.

## Verification Commands Executed (before report)
```bash
git branch --show-current          # feature/v1.62-schemas-reorganization
git rev-parse HEAD                 # 126f041d33678a2510e3c8ed554033081c35e150

pnpm run validate-schemas          # 56 valid
pnpm run codegen                   # OK (53 + common)
./tooling/check-wire-drift.sh      # PASS
cargo test --test schema_drift_detection  # 4/4 PASS
cargo check --workspace            # clean
rg 'bundle-refinement|cloud-sync/bundle' tooling/codegen/src/  # only new path
python3 $ref-resolution-audit      # 207 refs, 0 broken
grep -r 'generated::(compute|cloud_sync)' ...  # zero hits
ls ... | wc -l && rg 'entry!' ...  # 56 vs 54 (expected delta)
rg 'entity-attributes|entity-state' ... | grep -v archived  # only historical comments
```

All commands were re-run on the exact Working branch HEAD required by the Assignment.

## Conclusion
The reorganization is **correct** from a security and correctness standpoint:
- Every moved schema has a valid `$id`.
- Every `$ref` resolves.
- Codegen produces semantically identical types at the new module paths.
- The skip-list was updated for the renamed refinement file.
- Drift detection was updated for every moved wire schema.
- No consumer was left pointing at a stale generated path.
- Deleted schemas have no remaining active references.
- The boundary doc rewrite accurately describes the new consumer surface without accidental exposure.

**Verdict: Approve**
