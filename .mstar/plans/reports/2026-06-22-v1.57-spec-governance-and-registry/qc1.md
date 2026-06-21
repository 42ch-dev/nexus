---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-22-v1.57-spec-governance-and-registry"
verdict: "Request Changes"
generated_at: "2026-06-22"
---

# QC1 Review — V1.57 P0 Spec Governance & Registry

## Reviewer Metadata
- **Reviewer**: @qc-specialist (Reviewer #1, architecture/maintainability)
- **Runtime Model**: deepseek/deepseek-v4-flash
- **Review Perspective**: Architecture coherence and maintainability risk
- **Report Timestamp**: 2026-06-22

## Scope
- **plan_id**: `2026-06-22-v1.57-spec-governance-and-registry`
- **Review range / Diff basis**: `merge-base: 4ab34c6c (P-1 sign-off commit)` · `tip: 56d459ec (P0 merge commit)` — equivalent to `git diff 4ab34c6c..56d459ec`
- **Working branch (verified)**: `iteration/v1.57` (HEAD at `eae09e74`)
- **Review cwd (verified)**: `/Users/bibi/workspace/organizations/42ch/nexus`
- **Files reviewed**: 5 (3 spec files + 2 Rust source files)
- **Commit range**: `43e4d89d` (T1-T3 spec + roster) → `7e9932ce` (T5-T6) → `56d459ec` (merge)
- **Tools run**: `git diff`, `git log`, `cargo test`, `cargo clippy`, `cargo +nightly fmt`, manual spec inspection, roster row count

## Summary
- **AC met**: 8 / 12
- **Findings**: 6 (high: 2, medium: 2, low: 2)
- **Verdict**: **Request Changes**

## Acceptance Criteria Checklist

| # | AC | Status | Evidence |
|---|----|--------|----------|
| AC1 | Bridge header updated: Document class Master, Status `Master (V1.57 P-last promote)` | ✅ Pass | Header confirms `Document class: Master` and `Status: Master (V1.57 P-last promote draft; body content Master-ready; header rename in P-last)` |
| AC2 | Cross-references updated in `acp-capability-set.md` §0.5, `capability-registry.md` §4, `acp-client-tech-spec.md` | ✅ Pass | §0.5 points to bridge as Master; capability-registry §4 boundary table updated; acp-client-tech-spec had no bridge references to update (correctly NO-OP'd). |
| AC3 | §4 roster table: 36 rows, columns = {id, description, status_tag, shipped_plan, registry_row_ref} | ⚠️ Partial | Table has correct columns, but **41 rows** (not 36). 3 scaffold + 18 shipped + 18 catalog-only + 2 OUT. Plan acknowledged possible variance but 41 vs 36 is material. |
| AC4 | Status tags: 35 shipped, 3 scaffold, 2 OUT, 0 deferred | ❌ Fail | Actual: **18 shipped** + **18 catalog-only** (new tag not in AC4) + 3 scaffold + 2 OUT. The "catalog-only" tag was introduced without AC4 authorization. |
| AC5 | All 35 implemented IDs have handler bindings in `capability/builtins/` | ⚠️ Partial | 18 `nexus.*` + 2 `fs/*` handlers registered in **daemon-runtime** `CapabilityRegistry` (`build_registry()`), not in nexus-orchestration `capability/builtins/`. Handler functions still live in `host_tool_executor.rs` (per P1 deferral plan). |
| AC6 | Each handler has `CapabilityRegistryRow` with all 7 fields populated | ✅ Pass | `CapabilityRow` struct has id/access/admission/handler/acp_wire/failure_mode/handler_test_vector. Test `registry_all_rows_have_seven_fields` passes. |
| AC7 | Each handler has ≥ 1 success + ≥ 1 failure test through `CapabilityRegistry::dispatch()` | ⚠️ Partial | Each ID has a `handler_test_vector` with one test_fn_name. But only 2 of 20 rows reference failure-path tests (`work_patch_rejects_stage_field`, `context_assemble_policy_blocked_when_local_only`). Most IDs lack explicit failure-path test through dispatch. |
| AC8 | `R-V156P3-S003` field drops: unused fields removed, no regression | ✅ Pass | `registry_output_to_context` in `tasks/mod.rs` now maps all 9 fields (cache_age_ms, generated_at, fetch_timeout_ms, max_retries added back). Regression test confirm: pre-existing tests pass. |
| AC9 | Catalog ↔ registry cross-validation test passes | ✅ Pass | `catalog_registry_invariant_all_ids_present` test passes. Known_gaps reduced from 7 fs/*/work.* to 2 fs/* tools. |
| AC10 | `cargo test -p nexus-orchestration --test capability_registry` passes | ✅ Pass | 4/4 tests pass. Full daemon-runtime registry tests: 267/267 pass including the cross-validation test. |
| AC11 | `cargo clippy -p nexus-orchestration -- -D warnings` passes | ✅ Pass | Clean output, no warnings. |
| AC12 | `cargo +nightly fmt -p nexus-orchestration -- --check` passes | ❌ Fail | Format diff found in 5+ files: `capability/builtins/mod.rs`, `capability/builtins/registry.rs`, `capability/mod.rs` (chained method), `tasks/mod.rs` (4 `cdn_config: None` indent sites), `tests/novel_review_master.rs` (1 `cdn_config: None` indent). |

## Findings

| ID | Severity | Title | Scope | Rationale | Suggested Action |
|----|----------|-------|-------|-----------|------------------|
| qc1-001 | **high** | `cargo +nightly fmt` check fails (AC12 unmet) | `crates/nexus-orchestration/src/capability/builtins/mod.rs:44`, `crates/nexus-orchestration/src/capability/builtins/registry.rs:542`, `crates/nexus-orchestration/src/capability/mod.rs:296`, `crates/nexus-orchestration/src/tasks/mod.rs:2620,2660,2976,3025`, `crates/nexus-orchestration/tests/novel_review_master.rs:172` | Format differences found in 5+ files. `cdn_config: None` has inconsistent indentation in 4 test struct literals in `tasks/mod.rs` and 1 in `novel_review_master.rs`. Import wrapping in `mod.rs` and chained method formatting in `capability/mod.rs` also differ from nightly fmt. AC12 is a hard gate. | Run `cargo +nightly fmt -p nexus-orchestration` to auto-fix all formatting issues. Verify with `cargo +nightly fmt -p nexus-orchestration -- --check`. |
| qc1-002 | **high** | Status tag mismatch: AC4 not met (roster introduces undocumented "catalog-only" tag) | `.mstar/knowledge/specs/acp-capability-set.md §4` | Plan AC4 specifies exactly 4 status tags: shipped (35), scaffold-equivalent (3), OUT (2), deferred-to-V2.0+ (0). Actual roster uses 5 tags: shipped (18), **catalog-only** (18, new), scaffold-equivalent (3), OUT (2). The "catalog-only" tag is functionally correct (distinguishes logical-only from shipped-host-tool IDs) but was not authorized in the acceptance criteria. | Either (a) amend AC4 in the plan stub to acknowledge the 5-tag scheme with 18 shipped + 18 catalog-only, or (b) recategorize "catalog-only" IDs under a different existing tag. Recommended: amend AC4 since the 5-tag scheme is structurally clearer than the plan estimated. |
| qc1-003 | **medium** | Roster row count (41) significantly differs from plan estimate (36) | `.mstar/knowledge/specs/acp-capability-set.md §4` | Plan AC3 specifies 36 rows; §7 lists "roster count differs from 36" as a Blocked-return trigger. Actual count is 41 (3 scaffold + 18 shipped + 18 catalog-only + 2 OUT). The discrepancy arises because the plan counted shipped (35) but not catalog-only IDs. Plan acknowledged possible variance ("actual may be more") but the variance is +5 rows / +14%. | Document the revised count in the plan. The discrepancy is a plan estimation error, not an implementation defect. Update AC3 to reflect 41 rows. |
| qc1-004 | **medium** | Test-vector coverage: most IDs lack explicit failure-path test through `dispatch()` | `crates/nexus-daemon-runtime/src/capability_registry.rs:811-831` | AC7 requires "≥ 1 success test + ≥ 1 failure test per registered ID, dispatched through `CapabilityRegistry::dispatch()`." Each row has a `handler_test_vector` pointing to one test. Only 2 of 20 rows reference failure-path tests (`work_patch_rejects_stage_field`, `context_assemble_policy_blocked_when_local_only`). Most IDs only have a success-path test. The generic `registry_dispatch_rejects_unknown_tool` tests the dispatch mechanism, not per-ID failure modes. | Add failure-path tests for each ID that lacks one (e.g., what happens when input is missing, world_id is invalid, workspace is uninitialized). Target at least one failure test per ID dispatched through `dispatch()`. |
| qc1-005 | **low** | `specs/README.md` has stale bridge document class | `.mstar/knowledge/specs/README.md:116` | `agent-nexus-tool-bridge.md` is still listed as `Feature line | Shipped (V1.34)`. The bridge was promoted to Master in P0. While README is not in T2's explicit scope, it is a cross-reference document that will mislead readers about the bridge's status. | Update README.md line 116: `Feature line → Master`, `Shipped (V1.34) → Master (V1.57 P-last promote)`. |
| qc1-006 | **low** | Handler binding location differs from AC5 description | `crates/nexus-daemon-runtime/src/capability_registry.rs:360-731` | AC5 states handler bindings should be in `capability/builtins/`. Actual: handlers registered in daemon-runtime's `CapabilityRegistry` via `build_registry()`, with handler function pointers (`hte::registry_*`) still in `host_tool_executor.rs`. The plan P1 owns the god-file split which will relocate handlers; the AC text overstates P0's scope. | (Optional) Update AC5 text to reflect daemon-runtime `CapabilityRegistry` as the binding target for P0, with physical handler relocation deferred to P1. No code change needed. |

## Detailed Notes

### AC1 — Bridge header (✅)
The `agent-nexus-tool-bridge.md` header correctly shows `Document class: Master` and `Status: Master (V1.57 P-last promote draft; body content Master-ready; header rename in P-last)`. The status annotation matches T1's spec verbatim. The "header rename in P-last" correctly defers the file-path rename to the P-last closeout plan.

### AC2 — Cross-references (✅)
- `acp-capability-set.md §0.5`: Updated with dual pointer to both capability-registry.md (Master) and agent-nexus-tool-bridge.md (Master promoted from Feature line).
- `capability-registry.md §4` (boundaries table): Bridge row updated to `Master spec (promoted V1.57 P-last)`.
- `acp-client-tech-spec.md`: No bridge references existed, correctly NO-OP'd.
- **Note**: `specs/README.md` was not touched (finding qc1-005) — this is out of P0 scope per compass §2 (README updates are P-last hygiene).

### AC3 — Roster table (⚠️ Partial)
The roster has the correct column schema (id, description, status, shipped_plan, registry_row_ref) and correctly enumerates all 41 capability IDs. The 5-row overcount (41 vs 36) is an estimate error in the plan, not a defect in the implementation. The roster correctly reflects the actual capability inventory.

### AC4 — Status tags (❌ Fail)
This is the most significant spec governance finding. The plan's AC4 listed exactly 4 tags with specific counts. The implementation introduces a 5th tag "catalog-only" (18 IDs) that subsumes what the plan called "shipped." The 18 roster rows tagged as "shipped" are the daemon host tools; the 18 tagged as "catalog-only" are logical-only IDs for orchestration engine dispatch. The new tag is architecturally sound (it is a real and useful distinction) but was not authorized in the AC.

### AC5 — Handler bindings (⚠️ Partial)
18 `nexus.*` IDs have handler bindings in the daemon-runtime `CapabilityRegistry`. The registry construct (`build_registry()`) is solid: each row has a handler function pointer, an admission gate chain, ACP wire contract, failure mode, and test vector. However:
- The handlers live in `host_tool_executor.rs` (as `hte::registry_*` wrappers), not physically moved to `capability/builtins/` in nexus-orchestration.
- Only 18 `nexus.*` IDs are registered, not 35. The other 17 are correctly "catalog-only" in the spec.

The plan acknowledged this split (P1 owns the god-file split). AC5's text is overly ambitious for P0.

### AC6 — 7 fields populated (✅)
All `CapabilityRow` structs have all 7 fields populated. The test `registry_all_rows_have_seven_fields` verifies non-empty id, admission, and test_vector for every row. The row struct design cleanly separates id/access/admission/handler/acp_wire/failure_mode/test_vector.

### AC7 — Test vectors (⚠️ Partial)
Each row has a `handler_test_vector` with description, expected_outcome, and test_fn_name. The `ACCEPTED_TEST_FN_NAMES` list (20 entries) plus the symmetric `all_accepted_test_fn_names_referenced` / `all_test_fn_names_accepted` pair ensures every test function exists and every row references a valid test.

However, only 2 of 20 rows have failure tests:
- `work_patch_rejects_stage_field` (failure:invalid_input)
- `context_assemble_policy_blocked_when_local_only` (failure:policy_blocked)

The remaining 18 rows only test success paths. AC7 requires ≥ 1 success + ≥ 1 failure per ID.

### AC8 — Field drops (✅)
`registry_output_to_context()` in `tasks/mod.rs` now maps all 9 fields of `RegistryRefreshOutput`: `source`, `capability_count`, `fallback_reason`, `retry_count` (pre-existing) + `cache_age_ms`, `generated_at`, `fetch_timeout_ms`, `max_retries` (added). The comment on line 1227 correctly documents R-V156P3-S003. No regression in existing tests.

### AC9 — Cross-validation test (✅)
The test `catalog_registry_invariant_all_ids_present` passes. Known gaps reduced from 7 (`fs/read_text_file`, `fs/write_text_file`, 3 work.* IDs, 1 chapter.get, 1 daemon.health) to just 2 (`fs/read_text_file`, `fs/write_text_file`). The test validates both directions: (a) every registry id has a catalog row, and (b) catalog ids that look like host tools are in the registry.

Test rename from `registry_ids_have_catalog_rows` → `catalog_registry_invariant_all_ids_present` better communicates the bidirectional invariant.

### AC10 — Tests pass (✅)
`cargo test -p nexus-orchestration --test capability_registry`: 4/4 past.
`cargo test -p nexus-daemon-runtime -- capability_registry::tests`: 20+ registry-specific tests pass including the cross-validation test.

### AC11 — Clippy (✅)
`cargo clippy -p nexus-orchestration -- -D warnings`: clean.

### AC12 — Format check (❌ Fail)
`cargo +nightly fmt -p nexus-orchestration -- --check` fails with formatting diffs in 5+ files. These are all in code touched by P0 changes:
- `capability/builtins/mod.rs:44` — import wrapping
- `capability/builtins/registry.rs:542` — blank line spacing
- `capability/mod.rs:296` — chained method formatting (map_or_else)
- `tasks/mod.rs:2620,2660,2976,3025` — `cdn_config: None` indentation (4 sites; this field was added in P0 for R-V156P3-S003)
- `novel_review_master.rs:172` — `cdn_config: None` indentation

### Spec Governance Observations

**Roster structural health**: The new §4 roster is a significant improvement over the old §4.1–§4.8 fragmented tables. It provides a single SSOT for all capability IDs with consistent columns. The addition of "catalog-only" as a status tag correctly distinguishes between host-tool-bound and logically-deferred capabilities.

**Bridge Master status**: The bridge header update is correct but minimal — only the header block was changed. The compass §2 says P-last owns final Master promotion with full body review, so this is expected.

**No scope creep**: P0 only touched files within its scope: the 3 spec files, the daemon-runtime registry test, and the task/mod.rs field drop fix. No god-file refactor, no host-call CLI, no worker IPC changes. ✅

**R-V156P3-S003 correctness**: The 4 re-introduced fields (`cache_age_ms`, `generated_at`, `fetch_timeout_ms`, `max_retries`) match the `RegistryRefreshOutput` struct's full field set. The fix correctly addresses the previous silent field drop.

**Catalog vs registry counts**: The plan consistently talked about "35 shipped IDs" but the actual count is 18 shipped + 18 catalog-only. The plan's DF-46 closure semantics (§0 implied) correctly anticipated a gap: "deferred-tracker row for DF-46 is **reduced** (not 'Closed')." The 17 IDs that the plan called "shipped" but are actually "catalog-only" represent deferred orchestration engine capabilities — this is consistent with DF-46 reduction semantics.

## Verdict

**Request Changes**

Two high-severity findings block unconditional approval:

1. **qc1-001** (fmt check failure): AC12 is a hard acceptance gate. The format differences must be resolved.
2. **qc1-002** (status tag mismatch): AC4 as written does not match the implementation. This must be resolved by either amending the plan AC or adjusting the roster tags.

Additionally:
- qc1-003 (roster count) should be documented in the plan as an estimate correction.
- qc1-004 (test coverage) should target at least one failure-path test per ID.

Recommended correction path:
1. Run `cargo +nightly fmt -p nexus-orchestration` to auto-fix formatting (resolves qc1-001).
2. Update AC3/AC4 in plan stub: 41 rows, tags as implemented (18 shipped + 18 catalog-only + 3 scaffold + 2 OUT) (resolves qc1-002, qc1-003).
3. Add failure-path tests for IDs that lack them, or update AC7 to reflect achievable coverage (resolves qc1-004).
4. Update `specs/README.md` bridge status (resolves qc1-005).

After these corrections, targeted re-review of qc1-001 and qc1-002 is sufficient — full tri-review not required.
