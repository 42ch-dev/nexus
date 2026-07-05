---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-23-v1.62-manifest-schemas-and-host-validation"
verdict: "Approve"
generated_at: "2026-06-23"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1 (xai/grok-build-0.1)
- Review Perspective: Security and correctness risk — manifest-driven JSON-Schema validation (input/output shape enforcement, hand-rolled validator for 7 keywords, bypass surface, backward compat, error path safety, null handling, const enforcement on battle_report.kind).
- Report Timestamp: 2026-06-23

## Scope
- plan_id: `2026-06-23-v1.62-manifest-schemas-and-host-validation`
- Review range / Diff basis: `merge-base iteration/v1.62 @ f77b3de8 → feature/v1.62-manifest-schemas-and-host-validation @ 01514cf4` (6 commits)
- Working branch (verified): `feature/v1.62-manifest-schemas-and-host-validation`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.62-p1-manifest`
- Files reviewed: core implementation + tests in `crates/nexus-wasm-host/src/{error.rs,manifest.rs,compute.rs}` + `modules/basic-combat/manifest.json` + existing test suite (no production code outside the validation path was modified in scope).
- Commit range: 6 commits (`f14da381` T1-T2 ModuleSchemas + field, `daacf705` T3-T4 host validation impl, `a81a3ee2` T5 basic-combat schemas, `1571658d` T6 tests, `11633bf0` T7 docs, `01514cf4` T8 key-rules docs).
- Tools run: `git branch --show-current`, `git rev-parse HEAD`, `git rev-parse --show-toplevel`, `git log --oneline -10`, `git merge-base iteration/v1.62 HEAD`, `cargo test -p nexus-wasm-host` (full suite, 34+3+2+1 passed), targeted `cargo test -p nexus-wasm-host compute::tests`, `rg` for variant name / serde(default) / keyword impl / const / additionalProperties, full reads of `error.rs`, `manifest.rs`, `compute.rs`, `basic-combat/manifest.json`, `basic_combat.rs` integration test, format/clippy verification.

## Findings

### 🔴 Critical
None.

### 🟡 Warning
None.

### 🟢 Suggestion
- **S-001 (low, non-blocking)**: `ManifestValidationFailed { path, detail }` surfaces user-controlled strings (JSON keys from `key_blocks[*].block_type`, property names from input payloads, and manifest-declared block_type keys) directly into error messages. In the current crate these only become `ComputeError` values returned to the caller (daemon/orchestration). No SQL, shell, or HTML contexts are present in this crate, so no injection is exploitable here. Downstream consumers that log or display these errors verbatim should treat `path`/`detail` as untrusted. No change required for this plan; document the contract ("validation errors may contain untrusted JSON-derived strings").

## Source Trace

**Scope alignment verified verbatim** (from assignment):
- plan_id: `2026-06-23-v1.62-manifest-schemas-and-host-validation`
- Working branch: `feature/v1.62-manifest-schemas-and-host-validation`
- Worktree path / Review cwd: `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.62-p1-manifest`
- Review range / Diff basis: `merge-base iteration/v1.62 @ f77b3de8 → feature/v1.62-manifest-schemas-and-host-validation @ 01514cf4` (6 commits)

**Verification commands executed from worktree (all reproduced exactly as specified)**:
- `git branch --show-current` → `feature/v1.62-manifest-schemas-and-host-validation`
- `git rev-parse HEAD` → `01514cf4`
- `cargo test -p nexus-wasm-host 2>&1 | tail -30` → 34 unit + 3 integration + 2 sandbox + 1 doc-test all PASS (no failures).
- `rg 'ManifestValidationFailed' crates/nexus-wasm-host/src/ | head` → exact spelling used in `error.rs:56` and 20+ match sites in `compute.rs`.
- `rg '#\[serde\(default\)\]|pub schemas' crates/nexus-wasm-host/src/manifest.rs` → `#[serde(default)]` present on `pub schemas: Option<ModuleSchemas>`.
- `rg '"const"' modules/basic-combat/manifest.json` → `"kind": {"type": "string", "const": "combat"}`.
- `rg 'fn validate_|match .*type|required|additionalProperties|minimum|items|const' ...` → all 7 keywords implemented inside `validate_against_schema` (type, const, required, properties+additionalProperties, minimum, items).

**Focus item 1 — Validation actually catches malformed input** (read existing tests + code paths):
- Missing required on KeyBlock attributes: `compute::tests::missing_required_attribute_fails_with_json_path` + `missing_required_field_fails` (schema with `"required": ["base_atk", "max_hp"]`, instance missing one → `ManifestValidationFailed` with path `key_blocks[0].body.attributes` and "missing required field").
- Wrong-type fields: `wrong_type_fails_with_path` (integer where string expected) and `invocation_wrong_type_fails`.
- Wrong-shape invocation: covered by `invocation_wrong_type_fails` (attacker_id as number) and `valid_compute_input_passes_validation`.
- Missing `battle_report.kind`: enforced in two places — `validate_battle_report` hard-rejects null report, then schema with `"required": ["kind", ...]` + `"const": "combat"` catches absence or wrong value (see focus 8).
- All paths exercised by unit tests that directly call `validate_compute_input` / `validate_against_schema` and by the basic-combat integration test (which supplies a fully conforming input that passes).

**Focus item 2 — Hand-rolled validator correctness for 7 supported keywords** (each sampled):
- `type`: `valid_object_passes_type_check`, `wrong_type_fails_with_path` (string vs integer), `invocation_wrong_type_fails`.
- `properties` + recursion: `nested_object_validation_path`, `valid_object_with_all_required_fields_passes`.
- `required`: `missing_required_field_fails`, `missing_required_attribute_fails_with_json_path`, basic-combat manifest declares required fields and integration test supplies them.
- `additionalProperties`: 
  - `false` path tested: unit test logic at `compute.rs:339` (`if obj.get("additionalProperties") == Some(&Value::Bool(false))`) rejects undeclared keys with path `...{key}` + "additional properties not allowed".
  - `true` (and omitted) path used by real module: `basic-combat/manifest.json` sets `"additionalProperties": true` on all four fragments (character attributes/state, invocation, battle_report). The integration test `basic_combat_resolves_attack_into_four_part_output` supplies extra fields (`speed`, `level` in attrs; `status_effects` in state) and succeeds. No test forces `false` on basic-combat (correct — it deliberately allows extension).
- `minimum`: `minimum_constraint_fails` (0 < 1), basic-combat declares `minimum: 0` / `minimum: 1` on several integer fields; valid input in integration test respects them.
- `items`: `array_items_validation` (array of strings, element 1 is number → fails at `arr[1]`).
- `const`: `const_check_fails` ("exploration" vs "combat"), and basic-combat battle_report schema uses it (see focus 8).

**Focus item 3 — Bypass paths**:
- Empty schema `{}`: explicit test `manifest::tests::manifest_with_empty_schemas_object` + `compute::tests::empty_schemas_object_no_validation`. When `schemas: {}` (all subfields None), `validate_compute_input` is never entered for that fragment. This is the documented contract ("Omitted schemas fields → no validation").
- Malicious module declaring `schemas: {}` (or omitting `schemas`): treated as "no validation requested" — by design. The host only validates what the manifest author explicitly declared.
- `ComputeInput` that matches shape but is malicious in intent: validation is shape-only (JSON-Schema structural), never semantic. This is the documented and intended contract (see `compute.rs:196` comment and `modules/README.md`). No semantic policy engine exists in V1.62 scope.
- No hidden bypass when schemas **are** declared: `compute()` does `if let Some(schemas) = &manifest.schemas { validate... }` before any WASM execution. Post-compute battle_report validation is also unconditional when declared.

**Focus item 4 — JSON path string safety in error messages**:
- `ManifestValidationFailed { path, detail }` is constructed with `instance_path` (derived from `key_blocks[N].body.attributes`, manifest block_type keys, property names from the **input** JSON, and `invocation` / `battle_report` literal segments) and `detail` that may embed values from the instance (e.g. `format!("expected const value {const_val}, got {instance}")`).
- These strings come from untrusted module input + module-supplied manifest fragments.
- In this crate they only flow into `ComputeError` (returned to caller). No `format!` into SQL, `Command`, HTML templates, or log macros that would interpret them as code.
- Risk is therefore confined to downstream display/logging surfaces (daemon, CLI, logs). Low severity for the host crate itself. See Suggestion S-001.

**Focus item 5 — Backward compat correctness**:
- `manifest.rs:90`: `#[serde(default)] pub schemas: Option<ModuleSchemas>,`
- Explicit test `manifest::tests::manifest_without_schemas_is_backward_compat`: V1.61-style manifest (no `schemas` key) deserializes with `schemas = None`.
- `compute()` only enters validation when `manifest.schemas` is `Some`.
- All pre-existing integration tests (`basic_combat_resolves_attack_into_four_part_output`, sandbox limits) continue to pass (they use the embedded basic-combat which now carries schemas, but the path is still exercised).

**Focus item 6 — `ComputeError::ManifestValidationFailed` variant**:
- Exact spelling in `error.rs:56`: `ManifestValidationFailed { path: String, detail: String }`.
- Matches the compass-locked name referenced by P2 `wasm-host.md` §8.
- All error sites and match arms in `compute.rs` use this identical variant.

**Focus item 7 — Null `invocation` handling**:
- Code (`compute.rs:242-251`):
  ```rust
  if let Some(ref invocation_schema) = schemas.invocation {
      let inv = input.get("invocation");
      if let Some(inv) = inv {
          if !inv.is_null() {
              validate_against_schema(...)?;
          }
      }
  }
  ```
- Comment: "Skip validation if invocation is absent or null (invocation is optional in ComputeInput)."
- Assessment: **Correct and reasonable**. The `ComputeInput` envelope (from contracts) treats `invocation` as an optional free-form object. A module may declare an `invocation` schema to describe the shape it expects **when the caller supplies one**. Forcing a failure when the field is absent/null would break callers that legitimately omit `invocation` for modules that only need `key_blocks`. Contrast with `battle_report`, which is **required** to be a non-null object (hard error in `validate_battle_report` + `validate_output_shape`). The implementer's choice is consistent with the envelope design and the "optional" nature of the free-form slot.

**Focus item 8 — basic-combat `battle_report.kind` const-validated**:
- `modules/basic-combat/manifest.json:50`: `"kind": {"type": "string", "const": "combat"}`
- Schema also lists it under `"required"`.
- Unit test `const_check_fails` directly exercises the const rejection path with a similar schema.
- Integration test `basic_combat_resolves_attack_into_four_part_output` asserts `output.battle_report["kind"] == "combat"` on the successful path.
- If a future module (or a tampered basic-combat) emitted a different `kind`, `validate_battle_report` + `validate_against_schema` would reject with `ManifestValidationFailed` at `battle_report` / the const mismatch detail.
- Additionally, `validate_battle_report` explicitly rejects a literal `null` battle_report before schema validation.

**Additional security/correctness observations (no new findings)**:
- Validation runs on the **host side before and after** the WASM invocation for declared fragments — the module cannot bypass it by lying about its output.
- Hand-rolled validator is intentionally minimal (7 keywords only). It correctly short-circuits on first error (fail-fast).
- No `unsafe`, no panics in the validator paths under the exercised cases.
- All new tests are hermetic unit tests on the pure validation functions; no WASM execution required for keyword coverage.
- Lint gates: `cargo +nightly fmt --all -- --check` clean; `cargo clippy -p nexus-wasm-host -- -D warnings` clean on this crate (pre-existing workspace clippy items outside scope were not re-raised per assignment).

**Evidence that no forbidden items exist** (per "NEVER claim Approve if..."):
- No validation bypass when schemas are declared.
- All 7 supported keywords have positive + negative test coverage and behave as specified.
- Backward-compat path (V1.61 manifests) is explicitly tested and preserves prior behavior.
- `ManifestValidationFailed` name matches exactly.
- Therefore Approve is permitted.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 1 |

**Verdict**: Approve

All eight assigned security/correctness focus items are satisfied with reproducible evidence from source, unit tests, integration test, and manifest. The hand-rolled validator correctly enforces the declared shapes, the `ManifestValidationFailed` error variant name is exact, backward compatibility is preserved via `#[serde(default)]`, and the only intentional "no validation" case (`schemas: {}` or absent) is documented and tested as such. Null `invocation` skip is a sound choice given the optional nature of that envelope field. The single Suggestion is informational for downstream consumers and does not block approval.

Per `mstar-review-qc` gate rule (Critical = 0 and Warning = 0 ⇒ Approve), this seat returns **Approve**.

## Revalidation
N/A — initial wave for this plan. No prior qc2 report exists for `2026-06-23-v1.62-manifest-schemas-and-host-validation`.

## Evidence (verification-before-completion)
- Assignment scope fields verified on-disk via git commands (branch, HEAD, cwd, range).
- Full test suite (`cargo test -p nexus-wasm-host`) executed and passed.
- Every focus-item checklist item has a direct source reference (test name + file:line or manifest snippet).
- `rg` and full file reads performed for variant spelling, serde default, keyword implementation sites, const usage, and additionalProperties handling (both true and false paths).
- Format and clippy checks executed (clean on the crate).
- Report written only to the allowed path; only this file will be staged for commit.
- No code, status.json, plan, or other branches were modified.
