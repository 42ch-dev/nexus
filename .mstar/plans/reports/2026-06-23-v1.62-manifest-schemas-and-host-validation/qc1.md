---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-23-v1.62-manifest-schemas-and-host-validation"
verdict: "Approve"
generated_at: "2026-06-24"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: MiniMax-M3
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-24

## Scope
- plan_id: `2026-06-23-v1.62-manifest-schemas-and-host-validation`
- Review range / Diff basis: `merge-base iteration/v1.62 @ f77b3de8 → feature/v1.62-manifest-schemas-and-host-validation @ 01514cf4` (6 commits: f14da381, daacf705, a81a3ee2, 1571658d, 11633bf0, 01514cf4)
- Working branch (verified): `feature/v1.62-manifest-schemas-and-host-validation`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.62-p1-manifest`
- Files reviewed: 8 source files (manifest.rs, compute.rs, error.rs, lib.rs, AGENTS.md, basic_combat.rs, modules/README.md, basic-combat/manifest.json) + Cargo.toml
- Commit range (if not identical to Review range line, explain): identical (6 commits in the diff range)
- Tools run: `git log`, `git diff --stat`, `cargo check -p nexus-wasm-host`, `cargo test -p nexus-wasm-host`, `cargo clippy -p nexus-wasm-host -- -D warnings`, `cargo +nightly fmt --all --check`, `rg` (validation functions, callers, ModuleManifest consumers), `python3` (V1.61 manifest backward-compat shape check)

## Findings

### 🔴 Critical
None.

### 🟡 Warning

**W-001 — `modules/README.md` has a duplicated `schemas` block heading (doc-quality)** — Source: `modules/README.md:72-90`. The implementer diff inserts a new `### The \`schemas\` block (V1.62+)` heading at line 72 immediately above the existing V1.61 example (lines 74-88) and the original `#### The \`schemas\` block (V1.62+)` heading is retained at line 90. Net effect: a stray `###` heading sits above an orphan `basic-combat` example with no preamble, then the real `####` heading repeats the same title. A module author reading the README sees two sections named "The `schemas` block (V1.62+)" and may miss the actual schema reference. **Fix**: delete the inserted `### The \`schemas\` block (V1.62+)` heading at line 72 (it has no body), keep the `#### The \`schemas\` block (V1.62+)` heading at line 90 (and ideally convert it to `###` so the example block above it belongs to it).

**W-002 — Plan stub recommended `jsonschema` crate but implementer chose hand-rolled validator (architecture-divergence, acceptable)** — Source: `.mstar/plans/2026-06-23-v1.62-manifest-schemas-and-host-validation.md:30` says "Resolve design item #1 (validation library — `jsonschema` crate recommended)" while the implementer delivered a hand-rolled 7-keyword validator in `compute.rs:275-375`. The compass §5 design item #1 framed this as a deliberate trade-off ("compile time vs spec compliance") and the implementer's choice is documented in the file header (lines 192-196). This is an acceptable implementation-level resolution of a design item, but the plan stub wording ("jsonschema crate recommended") creates an audit-trail mismatch that future reviewers will notice. **Resolution recommended (non-blocking)**: the plan stub T3 wording should be edited to drop the recommendation and reframe as "choose between hand-rolled / `jsonschema` / `json-schema-validator` per design item #1" — this belongs in a P-mid housekeeping commit, not a P1 code change. (Per QC NEVER rules, this is a non-blocking note for PM, not a fix for the implementer.)

### 🟢 Suggestion

**S-001 — Hand-rolled validator keyword coverage is asymmetric** — `compute.rs:275-375` supports `type`, `properties`, `required`, `additionalProperties`, `minimum`, `items`, `const` but **not** `maximum`, `enum`, `oneOf`/`anyOf`/`allOf`, `pattern`, `format`, `minLength`/`maxLength`, `minItems`/`maxItems`, `uniqueItems`, or `nullable`. The asymmetry between `minimum` and `maximum` is the most likely footgun for module authors who assume JSON-Schema semantics. Consider adding a note in `modules/README.md` §`schemas` block listing the supported keyword subset explicitly (it does already, but `maximum` is conspicuously absent from the list — readers will wonder if it's an oversight). The "defer `$ref` to V2" decision is in the compass and is fine; the rest are minor.

**S-002 — Validation logic could be factored into a dedicated `validate.rs` module** — `compute.rs:189-408` adds ~220 lines of validation code (3 functions + 2 helpers + a doc-comment block) into what is otherwise a tight `compute()` orchestrator. A `validate.rs` module exposing `validate_input(input: &Value, schemas: &ModuleSchemas) -> Result<()>` and `validate_battle_report(output: &ComputeOutput, schema: &Value) -> Result<()>` would keep `compute()` focused on engine lifecycle and make the validator independently testable. The hand-rolled validator functions are not currently `pub`, so this is a pure refactor. Not urgent; V1.63+ cleanup.

**S-003 — `ComputeInput` is serialized to JSON twice in `compute()`** — `compute.rs:50-55` calls `serde_json::to_value(input)` for validation, then `serde_json::to_vec(input)` for the WASM payload. For the typical `basic-combat` envelope (~1 KiB) the cost is negligible, but the code could share the `Value` (validate against the `Value`, then `to_vec` the same `Value`) for a small perf win and to remove the duplication. Not urgent.

**S-004 — No end-to-end `engine.compute()` test for backward-compat (manifest with `schemas: None`)** — `manifest.rs:193-206` proves that a legacy V1.61 manifest (no `schemas` key) deserializes with `schemas = None`, but no integration test feeds such a manifest through `engine.compute()`. The `if let Some(schemas) = &manifest.schemas` guard at `compute.rs:49` is straightforward enough that this is a "nice-to-have" rather than a gap, but a `tests/backward_compat.rs` test that runs a non-schemas manifest through the full pipeline (in-memory `WasmEngine`, no actual `wasmtime` needed) would prove the invariant end-to-end and guard against future regressions.

**S-005 — `validate_battle_report` null-check is dead code** — `compute.rs:259-264` checks `if report.is_null()` and returns `ManifestValidationFailed`. But `validate_output_shape` at `compute.rs:454-460` (called earlier inside `run_invocation` at line 184) already returns `OutputSchemaMismatch` if `battle_report` is null, so by the time `validate_battle_report` runs the report is guaranteed non-null. The defensive check is harmless but unreachable for the normal path. Trivial cleanup.

**S-006 — Test name `no_schemas_means_no_validation` is slightly misleading** — `compute.rs:775-793`: the test sets up a `ModuleSchemas` with `key_block_attributes: Some({"monster": ...})` and feeds it a `key_block` of `block_type: "character"`. Validation is skipped because the `block_type` key is not in the schema's `HashMap`, not because "no schemas" is set. A more honest name would be `non_matching_block_type_skips_validation` or `schema_for_undeclared_block_type_skips_validation`. The behavior under test is correct; the name just doesn't reflect it.

**S-007 — `ModuleSchemas` struct requires a code change to add a 5th validated aspect** — `manifest.rs:54-68`: the struct has 4 fixed `Option<...>` fields. A future 5th (e.g. `module_state` for cross-call state, or `invocation_response`) requires adding a field + updating `validate_compute_input` / `validate_battle_report` + updating `compute()`'s gates. A `HashMap<SchemaAspect, Value>` (with `SchemaAspect` as an enum) would be more flexible, at the cost of type safety and IDE discoverability. The current 4-field design is fine for V1.62's scope; flagging for future design discussions.

## Source Trace

- Finding ID: F-001
- Source Type: manual-reasoning
- Source Reference: `crates/nexus-wasm-host/src/compute.rs:42-66` (`compute()` validation placement)
- Confidence: High

- Finding ID: F-002
- Source Type: git-diff
- Source Reference: `git diff f77b3de8..HEAD -- modules/README.md` (lines 67-180 of the new file)
- Confidence: High

- Finding ID: F-003
- Source Type: doc-rule
- Source Reference: `.mstar/plans/2026-06-23-v1.62-manifest-schemas-and-host-validation.md:30` vs `crates/nexus-wasm-host/src/compute.rs:192-196` and `crates/nexus-wasm-host/Cargo.toml` (no `jsonschema` dependency)
- Confidence: High

- Finding ID: F-004
- Source Type: manual-reasoning
- Source Reference: `crates/nexus-wasm-host/src/compute.rs:275-375` (hand-rolled validator keyword coverage)
- Confidence: High

- Finding ID: F-005
- Source Type: manual-reasoning
- Source Reference: `crates/nexus-wasm-host/src/compute.rs:189-408` (validator code placement in compute.rs)
- Confidence: High

- Finding ID: F-006
- Source Type: manual-reasoning
- Source Reference: `crates/nexus-wasm-host/src/compute.rs:50,55` (two serializations)
- Confidence: High

- Finding ID: F-007
- Source Type: manual-reasoning
- Source Reference: `crates/nexus-wasm-host/src/manifest.rs:193-206` (deserialization-only backward-compat test)
- Confidence: High

- Finding ID: F-008
- Source Type: manual-reasoning
- Source Reference: `crates/nexus-wasm-host/src/compute.rs:259-264` (dead-code null check) vs `compute.rs:454-460` (pre-existing null check)
- Confidence: High

- Finding ID: F-009
- Source Type: manual-reasoning
- Source Reference: `crates/nexus-wasm-host/src/compute.rs:775-793` (test name)
- Confidence: Medium

- Finding ID: F-010
- Source Type: manual-reasoning
- Source Reference: `crates/nexus-wasm-host/src/manifest.rs:54-68` (ModuleSchemas struct shape)
- Confidence: Medium

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 7 |

**Verdict**: Approve

### Architectural assessment (per assignment focus items)

1. **`ModuleSchemas` struct design** — The 4 optional fragment fields (`key_block_attributes`, `key_block_state`, `invocation`, `battle_report`) directly map to the 4 places the host validates, and each is `Option<...>` with `#[serde(default)]` so they backward-compatibly parse as `None`. The `HashMap<String, Value>` for per-`block_type` is forward-compatible (new block types can be added without changing the struct). The shape is right for V1.62's scope; the design comment at the struct level is clear. Adding a 5th aspect later requires a small struct change — see S-007 for the trade-off.

2. **Hand-rolled JSON-Schema validator decision** — The implementer's choice is explicitly documented in `compute.rs:192-196` ("Chosen over the jsonschema crate to keep compile times low and avoid pulling in heavy transitive dependencies for a handful of keyword checks (per compass §5 design item #1)"). The supported keyword subset is listed in the docstring (`type`, `properties`, `required`, `additionalProperties`, `minimum`, `items`, `const`). This is a defensible V1 trade-off: the schemas only need to validate the contract between host and module author, not arbitrary JSON-Schema documents. The risk is the keyword subset becoming a maintenance burden as module authors demand more keywords (`maximum`, `enum`, `oneOf`, `pattern`, etc.) — see S-001. The hand-rolled implementation itself is well-structured (recursive `validate_against_schema`, clear `ManifestValidationFailed` errors with JSON paths, fail-fast per design item #2). The divergence from the plan stub's "jsonschema crate recommended" wording is logged in W-002 as a non-blocking housekeeping item for PM.

3. **Validation placement in `compute.rs`** — The pre-invocation + post-invocation gates at `compute.rs:48-52` and `compute.rs:58-63` are coherent: input validation runs *before* `run_invocation` (no wasted WASM work on malformed input), output validation runs *after* (so we don't validate a report that was never produced). The two `if let Some(schemas)` gates are slightly duplicated and could be unified, but the current split is readable. The bigger maintainability concern is that ~220 lines of validator code lives in `compute.rs` rather than a `validate.rs` module — see S-002 for the recommended refactor. Pre-existing structure of `compute.rs` already mixed lifecycle/orchestration concerns, so this is incremental, not novel.

4. **Test structure** — 14 new tests, all well-named. Coverage of the meaningful equivalence classes is good: valid input pass, wrong type with JSON path, missing required field, all-required-present, minimum constraint, const check, array items, nested object, valid input per schemas, missing required attribute (the headline scenario from the plan stub), invocation wrong type, non-matching block_type skips validation, empty `schemas` object = no validation, basic-combat manifest parses with schemas. Naming is mostly accurate; `no_schemas_means_no_validation` (S-006) is the one exception. Tests are gated on `#[cfg(test)]` modules, do not depend on a live WASM runtime, and exercise both the unit (`validate_against_schema`) and integration (`validate_compute_input`) layers.

5. **`modules/README.md` §`schemas` block documentation** — The §`schemas` block content at lines 90-176 is well-structured: it explains when each fragment is validated, lists the supported keyword subset, gives a full authoring example using the actual basic-combat schemas block, and explicitly notes the fail-fast behavior. A module author who has read the compass can follow it; a module author who hasn't can follow it too, because the example shows the JSON shape. The only doc issue is the duplicate heading at line 72 (W-001) — fixable in a 1-line edit.

6. **Backward-compat invariant** — Enforced by code (`if let Some(schemas) = &manifest.schemas` at `compute.rs:49` and `compute.rs:59`) and by test (`manifest_without_schemas_is_backward_compat` at `manifest.rs:193-206` proves `serde_json::from_str` accepts a V1.61 manifest and yields `schemas: None`). The end-to-end variant (a V1.61 manifest running through `engine.compute()`) is not explicitly tested (S-004), but the `if let Some` guard is so trivial that this is a gap in *test coverage* not in *correctness*. The V1.61 manifests in the wild (which have no `schemas` key) will work unchanged.

7. **`Eq` derive removal from `ModuleManifest`** — `manifest.rs:71`: `Eq` was dropped because `serde_json::Value` (used in `ModuleSchemas`) does not implement `Eq`. The `ModuleSchemas` struct itself keeps `#[allow(clippy::derive_partial_eq_without_eq)]` with a justification comment at `manifest.rs:52-53`. I verified no consumer in the workspace relies on `ModuleManifest: Eq` (the only external usage is in `crates/nexus-orchestration/src/capability/builtins/narrative_compute.rs:274` which clones the manifest; `Clone` is still derived). The `ModuleCache` (`module_cache.rs:48`) uses `HashMap<String, Arc<CachedModule>>` keyed on the module id, not on the manifest. Safe change. Clippy workspace-level `pedantic` is satisfied (the `#[allow(...)]` is justified).

### Pre-merge sanity checks (all green)

- `cargo check -p nexus-wasm-host` — **PASS** (0.59s)
- `cargo test -p nexus-wasm-host` — **PASS** (34 unit tests + 3 basic_combat integration + 2 sandbox_limits + 1 doc-test = 40 tests, all green)
- `cargo clippy -p nexus-wasm-host -- -D warnings` — **PASS** (0.54s, no warnings)
- `cargo +nightly fmt --all --check` — **PASS** (no diff)
- V1.61 manifest shape constructible — **PASS** (Python sanity check; matches `manifest_without_schemas_is_backward_compat` Rust test)

### Acceptance against the plan stub

- T1 (`ModuleManifest::schemas: Option<ModuleSchemas>`) — **DONE** (`manifest.rs:91`)
- T2 (`ModuleSchemas` struct with 4 optional fields) — **DONE** (`manifest.rs:54-68`)
- T3 (host-side validation in `compute.rs` + `ComputeError::ManifestValidationFailed`) — **DONE** (`compute.rs:42-66`, `error.rs:52-56`)
- T4 (backward compat: omit `schemas` → no validation) — **DONE** (gated by `if let Some(schemas)`, tested by `manifest_without_schemas_is_backward_compat`)
- T5 (basic-combat `manifest.json` enriched with all 4 schema fragments) — **DONE** (`modules/basic-combat/manifest.json:13-60`)
- T6 (4 test scenarios) — **DONE** (14 new tests, exceeding the 4 minimum)
- T7 (`modules/README.md` documents the `schemas` block) — **DONE, with W-001 cosmetic fix needed**
- T8 (`crates/nexus-wasm-host/AGENTS.md` mentions manifest-driven validation) — **DONE** (`AGENTS.md:55-61`)

### Resolved per the assignment's "do NOT flag" list

- R-V161P0-LOW-001 (pre-existing clippy lints in `preset.rs`/`http.rs` test targets) — **not flagged** (per P-last T5 closeout, not in P1's scope)
- Compass prose platform count correction — **not flagged** (already corrected in this branch, not a P1 concern)
- P0 nested-modules codegen decision — **not flagged** (out of P1's scope; P0 territory)

## Handoff
- All architectural concerns from the assignment (hand-rolled validator, validation placement, backward-compat enforcement) have been honestly raised and assessed. The hand-rolled validator is documented and trade-off-aware; the validation placement is coherent though factored poorly; backward compat is enforced by both code and test.
- The single blocking-grade concern (W-001, README duplicate heading) is a 1-line doc fix, not a code change. Per the QC NEVER rules, I do not edit code or docs from a QC role; the implementer can fix W-001 in a follow-up commit if PM chooses. The current commit's "Approve" stands regardless.
- W-002 (plan stub wording) is a PM housekeeping item, not a P1 code change.
- All Suggestions (S-001 through S-007) are non-blocking; the implementer may address any of them in V1.63+ or a follow-up wave.
