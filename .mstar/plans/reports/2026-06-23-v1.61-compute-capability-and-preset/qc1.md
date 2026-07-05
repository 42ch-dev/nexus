---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-23-v1.61-compute-capability-and-preset"
verdict: "Approve"
generated_at: "2026-06-23"
---

# Code Review Report — qc1 (Architecture & Maintainability)

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: grok-build-0.1
- Review Perspective: Architecture coherence and maintainability risk (capability trait pattern, preset state-machine fidelity to compass Q7, preset registration consistency, module structure of `narrative_compute.rs`)
- Report Timestamp: 2026-06-23

## Scope
- plan_id: 2026-06-23-v1.61-compute-capability-and-preset
- Review range / Diff basis: 6e0bb90b..feature/v1.61-compute-capability-and-preset
- Working branch (verified): iteration/v1.61
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 14 (primary: `crates/nexus-orchestration/src/capability/builtins/narrative_compute.rs` — 964 lines; `crates/nexus-orchestration/embedded-presets/combat-engine/preset.yaml`; `crates/nexus-orchestration/src/capability/mod.rs`; `crates/nexus-orchestration/src/capability/builtins/mod.rs`; `crates/nexus-orchestration/src/auto_chain.rs`; `crates/nexus-orchestration/tests/capability_registry.rs`; `crates/nexus-orchestration/Cargo.toml`; plan `.mstar/plans/2026-06-23-v1.61-compute-capability-and-preset.md`; 5 prompt files under `embedded-presets/combat-engine/prompts/`)
- Commit range: equivalent to `git diff 6e0bb90b..feature/v1.61-compute-capability-and-preset` — 2 commits: `c5cc060f feat(v1.61-P3): narrative.compute capability + combat-engine preset` + `66f87000 docs(v1.61-P3): mark plan InReview with completion report`
- Tools run: `git rev-parse/show-toplevel/branch`, `git diff --stat`, `cargo check -p nexus-orchestration` (clean), `cargo clippy -p nexus-orchestration --lib --tests` (only 2 minor `unused variable` warnings in test code), `cargo +nightly fmt --check -p nexus-orchestration` (clean), `cargo test -p nexus-orchestration --lib narrative_compute` (16/16 pass), `cargo test -p nexus-orchestration --lib preset::tests::all_embedded_presets` (pass), `cargo test -p nexus-orchestration --lib capability::` (275/275 pass, includes `registry_has_32_builtins`, `registry_lookup_each_builtin`, `registry_iter_returns_all`), `cargo test -p nexus-orchestration --lib auto_chain::` (26/26 pass, includes `preset_version_mapping_matches_yaml_includes_cron_presets`), `cargo test -p nexus-orchestration --test capability_registry` (4/4 pass), manual source review of `apply_state_delta` / `apply_json_delta` / `apply_op_to_field` / `handle_compute_error` / `run()`, comparison against existing capability patterns in `world.rs` / `kb_extract_work.rs`, comparison against `novel-writing/preset.yaml` reference preset, grep for prompt-file references

## Findings

### 🔴 Critical
None.

### 🟡 Warning
- **W-1 (architecture coherence — preset state-machine stages collapse into one capability call)**:
  The compass Q7 explicitly defines the `combat-engine` workflow as a 5-stage state machine `load_world → compute → apply_delta → advance_timeline → done`, with P3 T6 ("State machine: load_world → compute → apply_delta → advance_timeline → done") instructing the implementer to **encode each stage as a distinct state** in `combat-engine/preset.yaml`. The implementation takes a different route: the entire cycle (load world + WASM compute + apply state_delta + append timeline events + create new KeyBlocks) is performed **atomically inside the single `narrative.compute` capability call**, which is bound only to the `load_world` state's `enter`. The downstream states `apply_delta` and `advance_timeline` carry **no `enter` actions** and only `exit_when: kind: manual` — they are no-op wait states. As a result:
  - The state-machine structure **misrepresents the actual execution semantics**: a future reader inspecting the preset would reasonably expect each state to do something, but 3 of 4 non-terminal states do nothing.
  - Compass Q7's stated intent (state-machine-as-protocol) is not realized; instead the state machine becomes a thin shell over a single monolithic capability invocation.
  - This is **not** a correctness defect (the `preset::tests::all_embedded_presets_pass_strict_validation_gate` test passes, and the `full_cycle` integration test exercises the happy/error paths), but it is a **design-divergence finding** the assignment specifically asks about ("are the stages well-defined? does it align with compass Q7?"). Two viable cleanups exist for P-last or a follow-up: (a) split `narrative.compute` into 4 capability calls (one per stage) so each `enter:` actually does work — heavier refactor, requires re-validating the WasmEngine reuse story (QC3 R1) and the WASM module cache (QC3 R2) — or (b) reduce the state machine to `load_world → done` with the capability doing all work, and rename state id to `compute` to match; this is a 10-line change to `preset.yaml` and removes the misleading "5 stages" surface.

### 🟢 Suggestion
- **S-1 (dead prompt files — maintenance noise)**: Five Markdown files under `crates/nexus-orchestration/embedded-presets/combat-engine/prompts/` (`compute.md`, `load-world.md`, `apply-delta.md`, `advance-timeline.md`, `done.md`) are **never referenced** by `combat-engine/preset.yaml` (no `system_prompt_file`, `prompt_file`, or `template_file` field exists in the manifest). A grep across `crates/nexus-orchestration/src/**/*.rs` for the filenames returns zero matches. The loader only resolves prompt paths via `assert_template_file_safe` / `collect_template_file_entries` from declared `template_file` / `prompt_file` / `system_prompt_file` keys in the YAML — so these five files ship inside the embedded bundle but are unreachable at runtime. Combined with W-1, they look like vestigial artifacts of an earlier "5 stages, one prompt each" prototype that was collapsed into the single capability. Either delete them, or wire them into the YAML (e.g. as `template_file` on `exit_when: kind: llm_judge` transitions) before future maintainers waste cycles wondering why they exist.
- **S-2 (test function name drift)**: `crates/nexus-orchestration/tests/capability_registry.rs:13` still declares `async fn registry_has_twenty_six_builtins()` while the assertion is now `assert_eq!(reg.len(), 32)`. The pre-V1.60 comment block was updated (correctly) but the **function identifier** is stale. Rename to `registry_has_thirty_two_builtins` (matching the lib-side `registry_has_32_builtins` already updated) so the test name and assertion match. One-line change.
- **S-3 (capability naming style consistency)**: Recent orchestration-scope capabilities follow the `nexus.<area>.<verb>` convention (`nexus.world.state.query`, `nexus.world.delta.apply`, `nexus.timeline.event.append`, `nexus.fork.create`). The new capability ships as plain `narrative.compute` — a single-segment name with no `nexus.*` prefix and no dotted structure. Compass Q7 explicitly writes `narrative.compute` (so this is **not** a deviation from PM-locked compass), but for downstream consumers of `CapabilityRegistry::iter()` the name will read differently from every other orchestration-scope capability shipped since V1.60. Worth flagging for the next spec amendment (`acp-capability-set.md`) so the catalog and runtime registry converge on one naming style.
- **S-4 (timeline position hard-coded to 0)**: `narrative_compute.rs:203` ships `"timeline_position": 0` in the `ComputeInput.narrative_state` envelope. The dev's Completion Report acknowledges this ("V1.61: default to start of timeline"). Worth promoting this limitation to a tracked residual rather than a comment in code, because any future combat-engine consumer that depends on incremental timeline position will get silently-wrong ordering from the second call onward.
- **S-5 (default `module_id` couples the capability to a specific embedded module)**: `default_module_id()` returns `"basic-combat"` — fine for V1.61's single-module scope, but the implicit coupling will surprise future maintainers who add a second module. Consider either (a) making `module_id` required and removing the default, or (b) resolving the default from `nexus_wasm_host::embedded_module_ids()` at runtime so it auto-tracks whatever ships in `embedded-modules/`.
- **S-6 (`#[allow(clippy::too_many_lines)]` on `run()` is justified but worth documenting)**: `narrative_compute.rs:144` — the `run()` function carries `#[allow(clippy::too_many_lines)]`. The lifecycle (parse input → admission → load KB → load narrative state → build envelope → invoke WASM → cap check → apply delta → create new blocks → append events → return) is genuinely sequential and a 7-step diagram in the doc-comment helps the reader. Acceptable for V1.61; if `run()` grows further, consider extracting the post-compute stage (apply + create + append) into a helper to keep `run()` under ~80 lines.
- **S-7 (module structure of `narrative_compute.rs` at 964 lines is acceptable, not a god-module)**: For comparison, sibling capabilities are `world.rs` (1050), `kb_extract_work.rs` (481), `fork.rs` (282), `timeline.rs` (292). The file is well-organized into 9 explicit sections separated by `// ─── ...` banners (input parsing, `apply_state_delta` + helpers, `create_new_key_blocks`, `append_timeline_events`, `handle_compute_error`, tests). Each helper has a single clear responsibility. **Not** a god-module; the length is driven by the deep state-delta merge logic (which is exercised by 9 unit tests) and not by mixed concerns. No action needed — flagging only because the assignment asked for an honest assessment.
- **S-8 (apply_state_delta atomicity gap is correctness-adjacent, defer to P-last)** — **already flagged by qc2 W-1**: per-delta `update_key_block` is not wrapped in a transaction; an error mid-loop leaves prefix deltas durable. Belongs in the same V1.61+ follow-up as W-1 if `narrative.compute` is split per state; otherwise a separate residual. Recording here for visibility; qc2 owns the disposition.
- **S-9 (`create_new_key_blocks` does not re-assert world_id scope)** — **already flagged by qc2 W-2**: an emitted `KeyBlock` whose `world_id` differs from the admitted `parsed.world_id` will be inserted into a foreign world. qc2 S-1 already recommends the re-check. Recording for completeness; qc2 owns the disposition.

## Source Trace
- W-1: `crates/nexus-orchestration/embedded-presets/combat-engine/preset.yaml:40–69` (4 non-terminal states; only `load_world` has `enter:` with a capability call) vs `narrative_compute.rs:144–297` (`run()` performs load KB + read world state + WASM compute + apply state_delta + create new KeyBlocks + append timeline events atomically). Compass anchor: `.mstar/iterations/v1.61-programmable-narrative-progression-delivery-compass-v1.md` §0 Q7 row + §1.1 P3 T6 + §7 canvas (lists `combat-engine preset state machine` as one of the canvas's covered topics).
- S-1: file enumeration (`crates/nexus-orchestration/embedded-presets/combat-engine/prompts/*.md`) vs grep across `crates/nexus-orchestration/src/**/*.rs` returning zero matches. Loader contract: `crates/nexus-orchestration/src/preset/loader.rs:289–425` (template_file / system_prompt_file resolution only from manifest keys).
- S-2: `crates/nexus-orchestration/tests/capability_registry.rs:13` (function name) + `:27` (assertion `assert_eq!(reg.len(), 32)`).
- S-3: `.mstar/iterations/v1.61-programmable-narrative-progression-delivery-compass-v1.md` Q7 row (locked `narrative.compute`) vs `crates/nexus-orchestration/src/capability/mod.rs:140–146` (built-in naming list mixing `narrative.compute` with `nexus.*` namespace).
- S-4: `narrative_compute.rs:200–204` (hardcoded `"timeline_position": 0` in `narrative_state`).
- S-5: `narrative_compute.rs:87–90` (`default_module_id()` returning `"basic-combat"`); coupling target at `crates/nexus-wasm-host/src/embedded.rs:31–44` (`embedded_module_ids()` enumerates all compiled-in modules).
- S-6: `narrative_compute.rs:143` (`#[allow(clippy::too_many_lines)]`); function spans `:144–297` (153 lines incl. blank lines).
- S-7: file sizes via `wc -l crates/nexus-orchestration/src/capability/builtins/{world,kb_extract_work,fork,timeline,narrative_compute}.rs`; section banners at `narrative_compute.rs:300, 520, 546, 579, 625`.
- S-8 / S-9: see `.mstar/plans/reports/2026-06-23-v1.61-compute-capability-and-preset/qc2.md` (W-1, W-2, S-1) — re-listed here only for cross-reviewer traceability.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 9 |

**Verdict**: Approve

### Verdict rationale

The implementation is **functionally correct**: all 16 `narrative_compute` tests pass, the `preset::tests::all_embedded_presets_pass_strict_validation_gate` gate (which iterates every embedded preset including `combat-engine`) passes, the `registry_has_32_builtins` + `registry_lookup_each_builtin` + `preset_version_mapping_matches_yaml_includes_cron_presets` sync tests all pass, `cargo clippy -p nexus-orchestration --lib --tests` emits only 2 unused-variable warnings on `kb_a`/`kb_b` in the test code (no new clippy errors attributable to this PR — the 24 `clippy::approx_constant` / `clippy::items-after-statements` / `clippy::doc_markdown` errors flagged at `-D warnings` are pre-existing in `preset/expr.rs` / `tasks/mod.rs` / `tests/review_report.rs`, none of which are in the diff), and `cargo +nightly fmt --check` is clean. The capability pattern correctly mirrors the established `world.rs` (V1.60 P0) shape: `Option<Arc<SqlitePool>>` + `new()` + `with_pool()` + `ensure_world_owned()` admission gate + structured `CapabilityError`. The registry wiring is correct across all three registry factories (`with_builtins`, `with_builtins_and_pool`, `with_runtime_deps`).

The single Warning (W-1) is a **design divergence**, not a correctness defect: the state machine in `combat-engine/preset.yaml` advertises 5 stages but only 1 (`load_world`) actually invokes a capability; the remaining 3 non-terminal states are no-op wait states. The functional path still works (full_cycle test exercises it), and the dev's Completion Report honestly enumerates the deferred items. The fix is small (rename state id to `compute`, drop the three empty states — 10-line preset edit) but is best done alongside the `WasmEngine`-per-pool (QC3 R1) and per-`run()` module recompile (QC3 R2) follow-ups already flagged for P-last, so I am recording W-1 as a tracked Warning for PM/PM's residual register rather than blocking this wave. All Suggestions (S-1 … S-9) are maintainability nits or already-covered by qc2/qc3 and are non-blocking.

### Recommended follow-ups (P-last or V1.61+)

These are **not** V1.61 P3 blockers; surfaced for PM's residual register (`status.json` → `residual_findings[<plan-id>]`):

| ID | Severity | Title | Source |
|---|---|---|---|
| R-V161P3-ARCH-001 | low | `combat-engine` preset state-machine stages collapse into single capability call — either split `narrative.compute` into per-stage capabilities, or collapse the state machine to `compute → done` and rename `load_world` to `compute` | qc1 W-1 |
| R-V161P3-MAINT-002 | low | Five prompt files under `combat-engine/prompts/` are not referenced by `preset.yaml`; either wire them or delete them | qc1 S-1 |
| R-V161P3-MAINT-003 | nit | `tests/capability_registry.rs::registry_has_twenty_six_builtins` function name is stale; rename to `registry_has_thirty_two_builtins` to match lib-side test | qc1 S-2 |
| R-V161P3-CORRECT-004 | medium | `apply_state_delta` is not atomic across the delta list (already qc2 W-1) — wrap in sqlx transaction or document "best-effort prefix apply" | qc2 W-1 |
| R-V161P3-CORRECT-005 | low | `create_new_key_blocks` does not re-assert that emitted `KeyBlock.world_id == parsed.world_id` (already qc2 W-2) — add re-check before insert | qc2 W-2 |
| R-V161P3-PERF-006 | low | `NarrativeCompute::with_pool()` constructs a new `WasmEngine` per pool (already qc3 R1) — add `with_pool_and_engine()` constructor for daemon injection | qc3 R1 |
| R-V161P3-PERF-007 | low | Embedded WASM module is recompiled on every `run()` call (already qc3 R2) — add `Arc<RwLock<HashMap<String, WasmModule>>>` cache keyed by `module_id` | qc3 R2 |

R-V161P3-ARCH-001 is qc1's primary contribution; R-V161P3-MAINT-002 and R-V161P3-MAINT-003 are qc1-only housekeeping; the rest are echoed for visibility (qc2 / qc3 own their respective findings).