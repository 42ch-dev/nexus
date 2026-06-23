---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-23-v1.62-schemas-reorganization-and-envelope-move"
verdict: "Request Changes"
generated_at: "2026-06-23"
---

# Code Review Report — V1.62 P0 (qc1)

## Reviewer Metadata

- Reviewer: `@qc-specialist` (Reviewer #1 — Architecture coherence + maintainability)
- Runtime Agent ID: `qc-specialist`
- Runtime Model: MiniMax-M3
- Review Perspective: Architecture coherence + maintainability risk (per `mstar-roles` qc-specialist-1 parameter table)
- Report Timestamp: 2026-06-23 (ISO-8601)

## Scope

- **plan_id**: `2026-06-23-v1.62-schemas-reorganization-and-envelope-move`
- **Review range / Diff basis**: `merge-base iteration/v1.62 @ 3cefb6d9 → feature/v1.62-schemas-reorganization @ 126f041d` — equivalent to `git diff 3cefb6d9..126f041d` (3 commits: f8f22d0a..126f041d; 203 files)
- **Working branch (verified)**: `feature/v1.62-schemas-reorganization`
- **Review cwd (verified)**: `/Users/bibi/workspace/organizations/42ch/nexus` (per `git rev-parse --show-toplevel`; branch confirmed via `git branch --show-current`)
- **Commit range**: `126f041d33678a2510e3c8ed554033081c35e150` (HEAD on Working branch)
- **Files reviewed**: 203 files changed (+1017/-926 LOC per `git diff --stat` summary)
- **Tools run**:
  - `git rev-parse --show-toplevel` + `git branch --show-current` — alignment
  - `git log --oneline 3cefb6d9..126f041d` — commit range
  - `git diff --stat 3cefb6d9..126f041d` — diff scope
  - `pnpm run codegen` — idempotency check (re-run produces no diff on `*/generated/`)
  - `pnpm run validate-schemas` — all 56 schema files valid
  - `./tooling/check-wire-drift.sh` — drift detection (4/4 pass)
  - `cargo check -p nexus-contracts` + `cargo check -p nexus-cloud-sync` + `cargo check -p nexus-wasm-host` + `cargo check -p nexus-kb` + `cargo check -p nexus42` + `cargo build --all` — clean
  - `cargo test -p nexus-contracts --test schema_drift_detection` — pass
  - `cargo test -p nexus-contracts` + `cargo test -p nexus-cloud-sync` + `cargo test -p nexus-wasm-host` — pass
  - `cargo clippy -p nexus-contracts -- -D warnings` — clean (workspace-wide clippy is the canonical gate; see Critical-context below)
  - `rg` / `grep` — exhaustive doc-comment + cross-reference audit
  - Static review: `tooling/codegen/src/{rust-generator,ts-generator,schema-loader}.ts`, `crates/nexus-contracts/src/generated/mod.rs`, `crates/nexus-wasm-host/src/lib.rs`, `crates/nexus-contracts/tests/schema_drift_detection.rs`

## Findings

### Critical

_(none)_

### Warning

#### W-001 — Doc-comment drift in `crates/nexus-kb/` references DELETED schemas (7 sites)

- **Issue**: `crates/nexus-kb/src/validation.rs` (3 sites) and `crates/nexus-kb/src/key_block.rs` (4 sites) still contain doc-comment references to `schemas/compute/entity-attributes.schema.json` and `schemas/compute/entity-state.schema.json`. **Both schemas were deleted in this P0** (per compass §1.1 P0 T5 and `schemas-directory-layout.md` §5 historical renames). Examples:

  - `crates/nexus-kb/src/key_block.rs:3-5`: `` /// types for which per-`block_type` structured schemas exist in /// `schemas/compute/entity-attributes.schema.json` and /// `schemas/compute/entity-state.schema.json`. ``
  - `crates/nexus-kb/src/validation.rs:6-8`: `` /// `KeyBlock`s against per-`block_type` structured schemas from /// `schemas/compute/entity-state.schema.json` and /// `schemas/compute/entity-attributes.schema.json`. ``

  The P0 plan §T14 "Update consumer code that imports the renamed generated types" was interpreted as covering Rust imports (which are clean — `rg 'EntityAttributes|EntityState' crates/ --type rust` returns 0 hits) but missed doc-comment text pointers that still point at the deleted files.

- **Impact**: A future contributor reading the doc-comments will search for `schemas/compute/entity-attributes.schema.json` and find nothing — the file is gone. This is the **exact hygiene failure** that `schemas-directory-layout.md` §4 "Content hygiene" warns against: "Stale `acp-runtime` / `cloud-sync` / `compute` references — Remove from active plans/docs". The implementer fixed this in the layout spec itself but missed the `nexus-kb` Rust doc-comments.
- **Fix**: Replace the 7 doc-comment references with the new authoritative location. Per compass §1.1 P0 T5, per-module shapes now live in `modules/<id>/manifest.json` `schemas` block (V1.62 P1). Suggested rewording (apply to both files):

  > `types for which per-block_type structured schemas are declared in each module's manifest.json schemas block (V1.62 P1)`

- **Severity rationale**: This is **Warning**, not Critical — the runtime code is correct (no broken imports; the type `serde_json::Value` for `state` and `attributes` is unchanged), `cargo build --all` passes, and the drift test passes. The risk is **hygiene drift** that compounds over time. The implementer already updated the layout spec, READMEs, and schemas/AGENTS.md to the same end; this is the last 7 doc-comment stragglers in `nexus-kb` source files.

#### W-002 — Stale source reference in `crates/nexus-wasm-host/AGENTS.md` Q8 row

- **Issue**: `crates/nexus-wasm-host/AGENTS.md` Q8 in the Architecture decision table reads: "Standard 4-part envelope from `schemas/compute/`." After P0, the compute envelope lives at `schemas/local-api/compute/`. This is the AGENTS.md for the crate that **actually imports the type** (`nexus-wasm-host/src/lib.rs:52-54` now uses `nexus_contracts::generated::local_api::compute::*`), so the AGENTS.md contradicts the crate's own wiring.

- **Impact**: Future readers of `nexus-wasm-host/AGENTS.md` will think the compute envelope is at the old path. The cross-reference between the AGENTS.md table and the actual code has drifted.
- **Fix**: Update Q8 row to: "Standard 4-part envelope from `schemas/local-api/compute/`." A second-row follow-up to update the §Architecture reference sentence (V1 envelope ABI overview already uses the correct envelope type names).

#### W-003 — Stale source reference in `.mstar/knowledge/specs/canonical-hash.md`

- **Issue**: `.mstar/knowledge/specs/canonical-hash.md` line 20 (and line 49) still references the old path: `Wire shapes: schemas/domain/ (delta.schema.json, bundle.schema.json).` Per P0 T2, both files moved to `schemas/platform/sync/`. This is a **Companion** class spec (per `.mstar/knowledge/specs/AGENTS.md`) and therefore normative for OSS implementation; its source-of-truth pointer must match the new tree.

- **Impact**: Spec doc contradicts the new boundary truth and could mislead implementers wiring `nexus-cloud-sync/src/canonical_hash.rs` against the wrong path. The doc itself acknowledges the file lives at `crates/nexus-cloud-sync/src/canonical_hash.rs` (correct), but the wire-shapes pointer is stale.
- **Fix**: Update line 20 to: `Wire shapes: schemas/platform/sync/ (delta.schema.json, bundle.schema.json).` Update line 49 (References section) to `schemas/platform/sync/bundle.schema.json`.

### Suggestion

#### S-001 — Brittle `common_types` mod.rs splice in `rust-generator.ts`

- **Location**: `tooling/codegen/src/rust-generator.ts:289-298`.
- **Observation**: The codegen inserts `pub mod common_types;` at the front of the `common/` mod.rs to keep the synthetic `common_types` module grouped with other `common/` content. The code uses `lines.splice(childNames.length === 0 ? lines.length : 0, 0, 'pub mod common_types;')` — this places `common_types` at index 0, **ahead of** any alphabetically-sorted children. Today the only other child is `version_ref`, so `common_types` (c) → `version_ref` (v) is also the alphabetical order. **But** if a future contributor adds `bundle.rs` or `block.rs` to `common/`, the splice will break alphabetical ordering.
- **Recommendation**: Sort the `common/` mod.rs declaration order with the rest of the children (`common_types` first, then alphabetically), OR move `common_types` into the `for (const child of childNames)` loop by treating it as a synthetic child and inserting it into the children map before iteration. Cleaner: simply put `common_types` declaration **inside** the alphabetical `for` loop by pre-seeding `node.children` with a synthetic `common_types` child before walking.

#### S-002 — "Two ways to import the same type" is a documented feature; consider explicit "prefer flat" guidance

- **Observation**: The nested module tree (`generated::platform::sync::bundle::Bundle`) plus the root-level flat re-export (`generated::Bundle` re-exported as `nexus_contracts::Bundle`) means the same type can be imported via two paths. Today only `nexus-wasm-host/src/lib.rs` deliberately uses the nested path; all other consumer crates use the flat path. This is a **stable convention** but `schemas/AGENTS.md` documents the dual surface without saying which to prefer.
- **Recommendation**: Add a single sentence to `schemas/AGENTS.md` "Codegen Flow" section: "Consumers should prefer the **flat** import (`use nexus_contracts::Bundle`) for forward-compatibility — the nested path (`use nexus_contracts::generated::platform::sync::bundle::Bundle`) is intended for cross-crate ABI surfaces where the consumer wants explicit visibility (e.g. `nexus-wasm-host` re-exports `ComputeInput` via the nested path so external WASM modules depend on a single crate)." This avoids future contributors being unsure which to use.

#### S-003 — `bundle-refinement.schema.json` carries a frozen `VERIFICATION NOTE (SYNC-R8)` audit block in its `description`

- **Observation**: `schemas/platform/sync/bundle-refinement.schema.json` line 6 contains a 6-line audit note (`VERIFICATION NOTE (SYNC-R8): ... Verified 2026-04-08.`) inside the JSON Schema `description` field. This is fine for an immutable audit trail, but as the file evolves (the move to `platform/sync/` was a non-content rename), the "Verified 2026-04-08" stamp will go stale.
- **Recommendation**: Future content edits to `bundle-refinement.schema.json` should either (a) move the audit note out of the JSON `description` (where it pollutes downstream consumers) into a sibling metadata block, or (b) treat the description as frozen and update the audit stamp only on a dedicated refresh. **Not urgent** — this is informational and the file's content is unchanged in P0.

#### S-004 — `nexus-wasm-host/src/lib.rs` nested-path comment can include the AGENTS.md-pointer cross-ref

- **Observation**: `crates/nexus-wasm-host/src/lib.rs:50-55` correctly uses the nested path and the comment explains "These are the generated wire types for `schemas/local-api/compute/`." After W-002 lands, `nexus-wasm-host/AGENTS.md` will match — but consider adding a comment in `lib.rs` that points to the AGENTS.md, mirroring how `schemas/AGENTS.md` "Codegen Flow" cross-references `tooling/AGENTS.md`. **Optional.**

## Source Trace

- **F-001 (W-001)**: `rg 'schemas/compute/' crates/nexus-kb/` → 7 hits in `validation.rs` + `key_block.rs`. The files referenced (`entity-attributes.schema.json`, `entity-state.schema.json`) no longer exist on disk (`ls schemas/compute/` → No such file or directory). Compass §1.1 P0 T5 mandated the deletion. Source: `git diff --stat 3cefb6d9..126f041d` confirms these files are absent from the P0 diff (renamed-to-delete). Confidence: **High**.
- **F-002 (W-002)**: `rg 'schemas/compute/' crates/nexus-wasm-host/AGENTS.md` → 1 hit at Q8 row. The crate's own source (`crates/nexus-wasm-host/src/lib.rs:52-54`) uses `generated::local_api::compute::*`, confirming the AGENTS.md is stale. Confidence: **High**.
- **F-003 (W-003)**: `rg 'schemas/domain/bundle|schemas/domain/delta' .mstar/knowledge/specs/canonical-hash.md` → 1 hit at line 49. Compass §1.1 P0 T2 (T2 moved 3 sync-payload files from `domain/` → `platform/sync/`). Confidence: **High**.
- **F-004 (S-001)**: `rust-generator.ts:289-298` — `lines.splice(... 0, 'pub mod common_types;')`. Manual review: the splice inserts at the front, ahead of any alphabetically-sorted children. Today it works because `common_types` < `version_ref` alphabetically, but the splice is hard-coded to position 0. Confidence: **Medium** (depends on whether a future `common/` child will be alphabetically before `common_types`).
- **F-005 (S-002)**: `rg 'use nexus_contracts::generated::' crates/ --type rust` → 12 crates; `rg 'use nexus_contracts::generated::(platform|local_api|domain|common)::' crates/` → 1 hit (`nexus-wasm-host/src/lib.rs`). Schema doc at `schemas/AGENTS.md` "Codegen Flow" describes both paths. Confidence: **Medium** (architectural preference; not a defect).
- **F-006 (S-003)**: `head -10 schemas/platform/sync/bundle-refinement.schema.json` → line 6 carries the VERIFICATION NOTE in the JSON `description` field. Compass §1.1 P0 T2 confirms the rename only (`bundle.schema.json` → `bundle-refinement.schema.json`), no content change. Confidence: **High** (audit-trail observation).
- **F-007 (S-004)**: `crates/nexus-wasm-host/src/lib.rs:50-55` comment text. Confidence: **Low** (optional polish).

## Architecture / maintainability assessment (Reviewer #1 focus)

Per the Assignment's six focus areas:

1. **Consumer-scope tree coherence**: **PASS**. `schemas/{common,domain,platform/{http-bff,sync},local-api/compute}/` cleanly maps to consumer boundaries (shared wire entities, platform HTTP, platform sync, local API cross-language). The `bundle.schema.json` (codegen canonical) vs `bundle-refinement.schema.json` (validation-only, codegen-skipped) split is **clear and well-documented** in `schemas-directory-layout.md` §3.2 and `schemas/platform/sync/README.md`. The `SKIP_STRUCT_GENERATION_REL_PATHS` mechanism in `schema-loader.ts` enforces the skip at codegen time. **Borderline placement note** (folded into W-001–W-003 narrative, not raised as a finding): `platform/sync/bundle-refinement.schema.json` could in principle live under `platform/sync/refinements/` if more refinement files appear, but with only one refinement today the current placement is **fine** and explicitly intentional.

2. **Codegen nested-modules decision**: **PASS with S-001/S-002 suggestions**. The nested + flat-re-export pattern (`generated::{common,domain,platform::{http_bff,sync},local_api::compute}::<module>` nested + `generated::Bundle` flat) is **architecturally sound**:
   - Mirrors the `schemas/` consumer-scope tree (single source of truth)
   - Preserves the existing flat public API (`nexus_contracts::Bundle` etc.) so no consumer rewrites were required beyond P0 T14 path updates
   - The recursive `mod.rs` generation in `rust-generator.ts:269-339` + `ts-generator.ts:147-177` handles arbitrary depth (`local-api/compute/`, future `local-api/orchestration/`, etc.) without code changes
   - The flat re-export chain (lib.rs → generated/mod.rs → per-folder/mod.rs → leaf) is verified at compile time (`cargo build --all` clean)
   - **Maintainability cost**: 256 LOC delta in `rust-generator.ts` + 175 LOC delta in `ts-generator.ts` for the recursive tree walker. This is non-trivial but the alternative (a single flat `generated/` with no folder structure) would not mirror the schemas/ tree and would lose the consumer-scope benefits. **Trade-off is justified.**
   - **Scaling**: as `local-api/` grows (compass design item #4 — future `local-api/orchestration/`, `local-api/schedule/`), the codegen needs no changes. New types just appear at the right nested location and the recursive walker handles it.

3. **Boundary doc rewrites**: **PASS with W-003 caveat**. `schemas-external-consumer-boundary.md` (renamed from `schemas-wire-platform-sync-boundary.md`) correctly states the new rule: "A JSON Schema file belongs in `schemas/` **only if it is consumed by an external client** — either `nexus-platform` (wire) **OR** an external Local API client". The `What still lives in schemas/ today` table includes the new `schemas/local-api/compute/` row with the correct consumer note ("external WASM modules + future WebApp"). The renamed doc supersedes the old title with a "Supersedes" header line. **Internal coherence**: `schemas-directory-layout.md` + `schemas-external-consumer-boundary.md` + `schemas/AGENTS.md` + `schemas/README.md` are mutually consistent on the boundary rule, the tree shape, and the codegen flow. **W-003** is the only straggler (canonical-hash.md Companion spec still points at old path).

4. **`bundle-refinement` naming**: **PASS**. The naming is unambiguous about role (refinement, not canonical). The doc comments in `schemas-directory-layout.md` §3.2, `schemas-external-consumer-boundary.md` "What still lives in schemas/", `schemas/platform/sync/README.md`, and the schema's own `description` field (with the `VERIFICATION NOTE (SYNC-R8)` block) all consistently emphasize the canonical-vs-refinement split. The `SKIP_STRUCT_GENERATION_REL_PATHS = new Set(['platform/sync/bundle-refinement.schema.json'])` is **path-anchored** (not basename-anchored), so a future contributor cannot accidentally create a different `bundle-refinement.schema.json` under another folder and have it silently skipped. **Naming is defensible.**

5. **Cross-references**: **PASS for schema files**, **W-001/W-002/W-003 for code/docs**. Every `$id` and `$ref` URI in the 56 schema files uses the new path (`https://nexus42.invalid/schemas/platform/sync/bundle.schema.json`, etc.). `rg 'cloud-sync|invalid/schemas/compute/' schemas/ --type json` returns 0 hits — no stale URIs. The only stale references are in **doc-comments and prose**:
   - `crates/nexus-kb/src/validation.rs` + `key_block.rs` (7 doc-comment refs to deleted `schemas/compute/entity-*`) — **W-001**
   - `crates/nexus-wasm-host/AGENTS.md` Q8 row (refers to `schemas/compute/`) — **W-002**
   - `.mstar/knowledge/specs/canonical-hash.md` lines 20 + 49 (refers to `schemas/domain/bundle.schema.json`) — **W-003**
   These are 3 separate hygiene stragglers, none affecting runtime behavior.

6. **Profile B invariant preservation**: **PASS**. `git diff 3cefb6d9..126f041d -- .mstar/status.json .mstar/archived/ .mstar/notes.json` shows only `.mstar/archived/knowledge/schemas-boundary.md` (1 line updated: stale pointer to `schemas-wire-platform-sync-boundary.md` → `schemas-external-consumer-boundary.md`). `status.json`, `notes.json`, `archived/plans/`, and `archived/plans-done.json` are **untouched**. Profile B invariants (`plans[]` non-Done only, layout invariant, no forbidden narrative fields) preserved.

### Verification summary

- `pnpm run codegen` idempotent (re-run → 0 diff on `*/generated/`): **PASS**
- `pnpm run validate-schemas`: 56 valid, 0 invalid: **PASS**
- `cargo test --test schema_drift_detection`: 4/4 pass: **PASS**
- `cargo check` on `nexus-contracts` + `nexus-cloud-sync` + `nexus-wasm-host` + `nexus-kb` + `nexus42` + `cargo build --all`: **PASS** (no errors, no warnings)
- `cargo test -p nexus-contracts` + `cargo test -p nexus-cloud-sync` + `cargo test -p nexus-wasm-host`: **PASS**
- `cargo clippy -p nexus-contracts -- -D warnings`: **PASS** (no errors on nexus-contracts)
- `cargo clippy -p nexus-contracts --tests -- -D warnings`: 8 pre-existing errors (7 `unnecessary_hashes` in `preset.rs` + 1 `redundant_clone` in `http.rs`) — **verified pre-existing on base `3cefb6d9`** per `git checkout 3cefb6d9 --` test. Tracked as `R-V161P0-LOW-001` with target `V1.62 P-last T5`. Per the Assignment's NEVER rule, this is **not raised as a P0 finding** — it would be a false positive. Noted here for completeness.
- Compass §1.3 prose drift (33 → 34 platform/http-bff files): per the Assignment's NEVER rule, **not raised**. The implementer already corrected the count in `schemas-directory-layout.md` §7 inventory ("currently 56 `*.schema.json`"). Noted here for completeness.
- Tree shape matches compass §1.3 exactly: 3 common + 10 domain + 34 http-bff + 7 sync + 2 local-api/compute = **56 schema files** (matches `pnpm run validate-schemas`).
- Per-folder READMEs created for `platform/http-bff/`, `platform/sync/`, `local-api/compute/` per compass §1.1 P0 T15: **PASS**.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 4 |

**Verdict**: **Request Changes**

The three Warning findings are doc-comment drift (W-001: 7 sites in `nexus-kb`, W-002: 1 site in `nexus-wasm-host/AGENTS.md`, W-003: 1 site in `canonical-hash.md`) — they are not runtime defects but violate the explicit "stale path risk" hygiene rule that `schemas-external-consumer-boundary.md` itself writes down ("do not reference `schemas/cli-sync/`, `schemas/meta/`, `schemas/acp-runtime/`, `schemas/cloud-sync/`, or `schemas/compute/`"). The implementer enforced this rule in the layout spec, in the per-folder READMEs, and in the schemas themselves; the stragglers in `nexus-kb`, `nexus-wasm-host/AGENTS.md`, and `canonical-hash.md` are exactly the kind of hygiene gap that compounds as more plans land on top of P0 (P1 will reference `manifest.json` schemas; P2 will author `compute-module-abi.md` + `wasm-host.md`; both will be misled by stale doc-comments pointing at deleted paths).

The architectural foundation is **sound**: the nested + flat codegen pattern works, the consumer-scope tree is internally consistent, the boundary rule is unambiguous, the `bundle-refinement` naming is defensible, all `$ref`/`$id` URIs are correct, and Profile B invariants are preserved. The three Warnings are fixable in a single commit (a docs-only fix that doesn't touch generated output) and should be closed before this plan moves to P-mid/P-last.

---

*QC Reviewer #1 (qc-specialist) — Architecture coherence + maintainability focus. Pre-existing clippy lints (`R-V161P0-LOW-001`) and compass §1.3 file-count prose drift intentionally not raised per Assignment NEVER rule; verified pre-existing / corrected in `schemas-directory-layout.md` §7.*
