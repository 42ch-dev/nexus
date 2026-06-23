---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-22-v1.60-df46-local-parity"
verdict: "Request Changes"
generated_at: "2026-06-23"
---

# QC1 Architecture/Maintainability Review — V1.60 P0 DF-46 Local Parity

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: MiniMax-M3
- Review Perspective: architecture coherence & maintainability risk
- Report Timestamp: 2026-06-23

## Scope
- plan_id: 2026-06-22-v1.60-df46-local-parity
- Review range / Diff basis: `7cec348d..a45e5b8f` (P0 Track A: 7 commits)
- Working branch (verified): iteration/v1.60
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 14 (3 new capability files + spec + tests + registry touch-ups)
- Commit range: 7cec348d..a45e5b8f
- Tools run: git diff, git log, grep, read on key sources

## Findings

### 🔴 Critical
_(none)_

### 🟡 Warning

#### W-001: `world.delta.apply` `kb_key_block` create path references non-existent `metadata_json` column
**Scope**: `crates/nexus-orchestration/src/capability/builtins/world.rs:601-606`
**Finding**: The `kb_key_block` create branch in `WorldDeltaApply::run` issues an INSERT into `kb_key_blocks` that lists `metadata_json` as a target column:

```sql
INSERT INTO kb_key_blocks
  (key_block_id, world_id, block_type, canonical_name, status,
   body_json, metadata_json)
 VALUES (?, ?, ?, ?, 'provisional', ?, '{}')
```

`kb_key_blocks` (created in `20260525_kb_key_blocks.sql` and extended by `202606190003_kb_key_blocks_provenance.sql`) has **no `metadata_json` column** — its schema is `key_block_id, world_id, block_type, canonical_name, status, revision, body_json, source_anchor_json, created_from_command_id, created_at, updated_at, source_work_id, source_chapter, source_provenance_kind`. This INSERT will fail with `no such column: metadata_json` at runtime the moment a delta package containing a `kb_key_block` create change is applied. The spec (`world-delta-propose-apply.md` §6.3) advertises this as a supported V1.60 feature, but none of the 9 in-file test vectors (`world_delta_apply_*`) exercise this branch — they only cover `world_metadata` title updates — so CI passes while the create path is silently broken. The spec also leaves the create path under-tested relative to its scope (insert, conflict semantics, body_json storage, lost-update guard not applicable).
**Recommendation**: Replace the raw INSERT with `SqliteKbStore::insert_key_block_in_tx(&mut *tx, KeyBlock { ... })` from `kb_store.rs:146` (already wires all 14 columns and is the canonical write path), add at least one test vector that exercises the create path against an isolated world (e.g. `world_delta_apply_kb_create_success`, `world_delta_apply_kb_create_rejects_duplicate_active`). If `metadata_json` storage is genuinely needed for V1.60, add a migration adding the column to `kb_key_blocks` first — do not write to a column the schema does not declare.

#### W-002: Catalog-row parser for R-V159P0-002 hardcodes positional column indices instead of header-driven lookup
**Scope**: `crates/nexus-daemon-runtime/src/capability_registry.rs:1148-1196` (replacement for the 28-element match list)
**Finding**: The auto-derivation refactor correctly closes R-V159P0-002 (no manual 28-id list), but the row parser pins column positions by hard-coded index: `cols[1] = id`, `cols[3] = status`, `cols[5] = registry_ref`. If a future maintainer inserts a new column in the middle of the `acp-capability-set.md` §4 table — even an innocuous `description` refinement — the auto-derivation silently starts reading the wrong cells (e.g. `status` becomes `shipped_in`, `registry_ref` becomes whatever follows). The previous 28-element match list was brittle but **loud** at compile time (typos → compile error); this parser is brittle and **silent** at runtime (any wrong cell value would still type-check). The 31/26/27 numbers stay correct only because registry tests assert specific shipped host-tool IDs that survive this mis-alignment.
**Recommendation**: Parse the table once at the start of the test to read the header row (`| id | description | status | shipped_in | registry row ref |`), then key the row parser by header name (e.g. `header_idx("status")`). Add a guard: if any required header is missing, fail the test with a clear error message naming the missing header. This decouples the test from future schema refactors and preserves the auto-derive invariant that motivated R-V159P0-002.

#### W-003: `script.section_status.update` capability referenced in P1 spec/preset but not shipped in this iteration (cross-track concern originating from P0 architecture)
**Scope**: `crates/nexus-orchestration/embedded-presets/script-writing/preset.yaml:32` (preset comments); `.mstar/knowledge/specs/script-profile.md:198, 323` (spec sections)
**Finding**: The P1 spec (`script-profile.md` §5.1 "Capabilities required") and preset header explicitly enumerate `script.section_status.update` as required for the `draft → reviewed` auto-transition. That capability does not exist in `CapabilityRegistry::with_builtins()` — only the 5 P0 DF-46 orchestration capabilities were added this iteration. The P1 plan's `requires_capabilities` for `script-writing` does **not** include `script.section_status.update` (only `creator.inject_prompt`, `acp.prompt`, `judge.llm`), so the runtime preset will load and execute without it, but the **spec/impl contract drift** is documented in three places (script-profile.md §5.1, §9.2, preset header comment) and the spec author does not flag it as deferred. From an architecture perspective, the spec promises a capability the plan does not ship. The intended resolution is V1.61 (per the QC1 assignment brief), but there is no `residual_findings` row or plan-acceptance-criterion foot-note pointing to that deferral — leaving the next maintainer to discover the drift by reading the registry and the spec side-by-side.
**Recommendation**: Add a residual row under `metadata.residual_findings` (or a plan-acceptance footer) explicitly noting "V1.61: ship `script.section_status.update` orchestration capability (spec §5.1 + §9.2 promise it; P1 plan shipped without it). Until shipped, the `draft → reviewed` transition must be invoked manually or by the preset author's CLI workflow." This is the kind of contract drift that compounds silently across iterations; even when the fix is V1.61, the audit chain should record the gap now. (Tracking visibility for `@project-manager`; the underlying capability work is P1/V1.61 scope, not P0.)

### 🟢 Suggestion

#### S-001: `world.rs` is 961 lines hosting 3 capabilities + the shared admission-gate helper; consider splitting by capability
**Scope**: `crates/nexus-orchestration/src/capability/builtins/world.rs` (961 lines)
**Finding**: The file is the largest in `capability/builtins/` and the only one hosting 3 distinct `Capability` impls. The shared `ensure_world_owned` helper is the right thing to keep colocated, but `WorldStateQuery`, `WorldDeltaPropose`, and `WorldDeltaApply` each have independent input structs, schemas, and run() logic. Splitting into `world_query.rs` + `world_delta.rs` (with the shared helper in a fourth file or in `mod.rs`) would (a) keep each file under 400 lines, (b) make it trivial to add the 4th V1.61 world capability (e.g. `world.event.archive`) without further bloating one file, and (c) follow the pattern already established by `reference_refresh.rs` (single capability per file). Not blocking — the file is well-structured, has clear section dividers, and tests pass.
**Recommendation**: V1.61 hygiene plan: split `world.rs` into `world_query.rs` (read paths) and `world_delta.rs` (propose+apply). Move `ensure_world_owned` into `mod.rs` as a `pub(super)` helper if it becomes a cross-file dependency.

#### S-002: Naming adjacency with `nexus.world.snapshot.get` host tool creates cognitive load
**Scope**: `crates/nexus-orchestration/src/capability/builtins/world.rs:185` (`WorldStateQuery::name()`); `crates/nexus-daemon-runtime/src/api/handlers/host_tool_handlers.rs:852` (`nexus.world.snapshot.get`)
**Finding**: Two read-world capabilities now coexist with overlapping verb: `nexus.world.snapshot.get` (host tool, daemon, returns just `WorldState`) and `nexus.world.state.query` (orchestration capability, preset-driven, returns `world + kb + timeline`). They are not duplicating — the orchestration variant joins KB and timeline slices the host tool omits — but the naming is close enough that a maintainer scanning the roster will pause to disambiguate. The spec (`world-delta-propose-apply.md` §6.1) makes the distinction clear in prose but the roster row alone is not.
**Recommendation**: Add a one-line clarifying note in the `nexus.world.state.query` roster row of `capability-registry.md` and `acp-capability-set.md` §4: e.g. `description: "...(orchestration; broader than host_tool world.snapshot.get)"`. Optionally consider renaming `world.snapshot.get` to `world.state.get` in a future spec-hygiene pass so the verbs line up.

#### S-003: Plan acceptance criteria do not require a per-capability "scaffold atomicity" test for the new DB write paths
**Scope**: Plan T7 acceptance: "≥1 success + ≥1 failure + ≥1 admission gate per ID. 5 IDs × 3 tests minimum = 15 vectors." (achieved)
**Finding**: The 3×5=15 test vectors cover success, cross-creator rejection, and invalid input per capability. They do **not** cover the **transaction commit failure** branch of `WorldDeltaApply` (what happens if `tx.commit()` returns an error mid-batch — the spec §5 step 5 calls for `TransientExternal`). The 15-test floor is met; this is a low-cost extra. Same observation for `ForkCreate`'s mid-flight failure on the `append_event` marker step (no test for the `fork marker append: ...` error path; the existing `fork_create_rejects_bad_fork_point` only exercises the validation gate before `append_event`).
**Recommendation**: Optional V1.61 enhancement: add one "transaction failure injects" test per write capability using a closed/exhausted pool or an FK violation. Not blocking for P0 merge.

## Cross-cutting Observations
- **Pattern consistency**: All 5 new handlers (`WorldStateQuery`, `WorldDeltaPropose`, `WorldDeltaApply`, `TimelineEventAppend`, `ForkCreate`) faithfully mirror the `nexus.reference.refresh` pattern: `Option<Arc<SqlitePool>>`, `with_pool`/`new`/`Default` constructors, `tracing::info!` on admission, `CapabilityError` variants. Pattern adherence is excellent.
- **Registration**: All 5 capabilities are registered in **all 3 constructors** of `CapabilityRegistry` (`with_builtins`, `with_builtins_and_pool`, `with_runtime_deps`). The PMid fix-wave (`4d322c7c`) corrects the hardcoded count 26→31 in the daemon-runtime test. The `with_runtime_deps` constructor correctly uses the pool-conditional `map_or_else` pattern, matching `ReferenceRefresh`.
- **Spec alignment (`world-delta-propose-apply.md`)**: The Draft overlay's delta-package structure, policy gates (creator ownership), atomicity contract, and per-capability I/O summary (§6.1–§6.5) are **all reflected** in the Rust implementation. `entity` allowed values, `field` constraints, and `proposed_changes[]` shape match. The agent-vs-runtime decision (§3, closes acp §8 line 223 Open Item) is correctly implemented: `propose` is agent-facing + no writes; `apply` is runtime-facing + tx + lost-update guard.
- **PD-01 boundary**: `fork.rs` module-level doc-comment (lines 7-13) explicitly distinguishes local timeline branching from platform community/social fork. `capability-registry.md` § `nexus.fork.create` carries the same distinction. Clear and well-placed.
- **Cross-track surface disjointness**: P0 (Track A) and P1 (Track B) touch disjoint code surfaces except for `preset_version_for_id` (mentioned in compass as the only collision point; P1 T6 wire-up is the only consumer). No unintended coupling.
- **Dead code**: None observed. Imports are used; tests are reachable.
- **Naming**: New symbols follow existing conventions (`WorldStateQuery`, `TimelineEventAppend`, `ForkCreate`, `ensure_world_owned`).
- **Documentation**: All public APIs have doc-comments. The `world.rs` module docstring is particularly strong (mentions the design rationale and the acp §8 Open Item resolution in the same paragraph).

## Source Trace
- Finding W-001: manual-reasoning from `world.rs:601` INSERT statement vs. `20260525_kb_key_blocks.sql:7` schema. Confidence: High.
- Finding W-002: manual-reasoning from `capability_registry.rs:1156` (`cols[3]`, `cols[5]`) and the §4 table header in `acp-capability-set.md:107`. Confidence: High.
- Finding W-003: grep `script.section_status.update` across `crates/nexus-orchestration/src/` returned 0 hits; spec lines 198 and 323 reference it. Confidence: High.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 3 |

**Verdict**: Request Changes

**Rationale**: The 5 new orchestration handlers follow the established `nexus.reference.refresh` pattern with high fidelity; registration is complete across all three constructors; the Draft overlay closes the acp §8 Open Item and the implementation matches it. However, three Warning-level issues block merge: (W-001) the `kb_key_block` create branch in `WorldDeltaApply` references a non-existent `metadata_json` column — the spec documents this as shipped but no test exercises it, leaving the bug latent until runtime; (W-002) the R-V159P0-002 auto-derive replacement hardcodes column indices, trading compile-time brittleness for silent runtime brittleness; (W-003) the P1 spec advertises a `script.section_status.update` capability this iteration does not ship, with no residual row tracking the deferral. The three Suggestions (file-size, naming adjacency, transaction-failure test coverage) are non-blocking and can defer. After W-001 and W-002 are addressed and W-003 is registered as a tracked residual, this plan is ready to merge to `main`.