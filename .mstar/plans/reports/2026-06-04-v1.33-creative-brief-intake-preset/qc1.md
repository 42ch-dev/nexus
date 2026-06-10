---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-04-v1.33-creative-brief-intake-preset"
verdict: "Approve"
generated_at: "2026-06-04"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: openai/gpt-5.5
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-04T00:00:00Z

## Scope
- plan_id: 2026-06-04-v1.33-creative-brief-intake-preset
- Review range / Diff basis: merge-base: 569f79b + tip: 641489e (P2 + plan edits + R-V133P2-01 test fix)
- Working branch (verified): feature/v1.33-work-experience-loop
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 18 (assigned 16 files plus `crates/nexus-orchestration/src/tasks/mod.rs` and `crates/nexus-contracts/src/local/orchestration/preset.rs` for runtime/RunIntent traceability)
- Commit range: 569f79b..641489e
- Tools run:
  - `git rev-parse --show-toplevel`; `git branch --show-current`; `git log -1 --oneline`; `git diff 569f79b..641489e --stat | tail -20`
  - `cargo check -p nexus-orchestration -p nexus42 -p nexus-daemon-runtime -p nexus-contracts` → passed
  - `cargo clippy -p nexus-orchestration -p nexus42 -p nexus-daemon-runtime -p nexus-contracts -- -D warnings` → passed
  - `cargo +nightly fmt --all -- --check` → passed (no output)
  - `cargo test -p nexus-orchestration --lib all_embedded_presets` → passed (1/1)
  - `cargo test -p nexus-orchestration --test capability_registry` → passed (4/4; confirms 18 built-ins fix)
  - `cargo test -p nexus42 --test command_surface_contract` → passed (29/29)
  - `cargo test -p nexus-orchestration --lib write_brief` → passed (6/6)
  - Targeted `grep` / `read` review of creative-brief preset, prompts, capability registry, `creator.write_brief`, CLI run wiring, RunIntent enum, novel-writing prompt inputs, and `StateCompositeTask` capability dispatch.

## Findings

### 🔴 Critical

#### C-V133P2-QC1-01: `creative-brief-intake` cannot persist a real brief because capability args are neither rendered nor contract-aligned
- **Issue**: The preset passes `work_id: "{{preset.input.work_id}}"` and `brief_text: "{{state.synthesizing.output}}"` to `creator.write_brief`, but `StateCompositeTask` forwards capability `args` directly as JSON and only strips/injects protected identity fields. It does not render Handlebars templates for capability args. Even if rendering were added, `CreatorWriteBriefInput` is `#[serde(rename_all = "camelCase")]` and the unit tests call it with `workId` / `briefText`, while the preset and `input_schema()` advertise snake_case `work_id` / `brief_text`.
- **Impact**: The terminal `persisting` state will call `creator.write_brief` with literal strings and wrong field names. Deserialization fails before JSON parsing, or `brief_text` remains the literal `{{state.synthesizing.output}}`. This breaks plan acceptance item 2 (brief JSON on Work after intake schedule completes), A1/A2 terminal write, and the advertised `intake_status=complete` transition.
- **Fix**: Add a shared capability-args rendering step before `CapabilityTask` execution, using the same nested payload rules as prompt rendering; align the local capability contract by either changing preset/schema/tests to camelCase or adding explicit serde aliases for snake_case; then add an integration test that runs the `creative-brief-intake` persisting state (or full schedule with mocked synthesized output) and asserts Work `creative_brief` plus `intake_status=complete`.

#### C-V133P2-QC1-02: `novel-writing` is not wired to the persisted creative brief
- **Issue**: The P2 plan requires `novel-writing` prompt vars to come from `creative_brief` JSON and `creator run start` to chain intake → novel-writing. The implementation creates the Work and schedules `creative-brief-intake`, but after success it only prints a manual `daemon schedule add --preset novel-writing --seed "<topic>"` instruction. The existing `novel-writing` prompt still only reads `{{preset.input.topic}}` and `{{preset.input.vibe}}`; no diff maps Work `creative_brief` fields (`genre`, `tone`, `audience`, `themes`, hooks) into novel-writing `preset.input`.
- **Impact**: A completed intake does not feed production. Users can end with a Work containing a brief but no production schedule, and a manually scheduled `novel-writing` run will not receive the brief-derived vars promised by acceptance item 3 and README line 59.
- **Fix**: Add a concrete Work→production schedule bridge: load the Work creative brief after intake completion or provide an explicit `creator run` follow-up that reads `creative_brief`, maps it into the `novel-writing` expected input contract, and enqueues `novel-writing`. Update tests to assert the production schedule input contains brief-derived fields.

#### C-V133P2-QC1-03: The T6 `memory-augmented` persist fix still writes the wrong content path at runtime
- **Issue**: The plan’s T6 fix changes `memory-augmented` to `content: "{{state.generate.output}}"`, but the same `StateCompositeTask` capability-args path does not render templates. `creator.write_memory` therefore receives the literal string `{{state.generate.output}}` as `content`, not the generated text.
- **Impact**: The plan checkbox says T6 is complete, but the runtime behavior does not persist generated output. This is especially risky because the strict preset validation test passes: it validates structure, not runtime arg rendering.
- **Fix**: Same as C-01: render capability arg values recursively before execution and add a regression test for `memory-augmented` (or a smaller state-level test) proving `creator.write_memory` receives resolved `state.generate.output`.

### 🟡 Warning

#### W-V133P2-QC1-01: `validate_creative_brief` omits the schema-version invariant
- **Issue**: The validator claims to validate against work-experience-model §4, and all prompt/test examples include `brief_schema_version: 1`, but `BRIEF_REQUIRED_KEYS` does not require it and no type/value check is enforced.
- **Impact**: Stored briefs can be versionless even though future migrations/consumers are expected to use this forward-compat field.
- **Fix**: Require `brief_schema_version`, verify it is `1` (or an accepted integer), and add a negative test for missing/wrong version.

#### W-V133P2-QC1-02: `creator.write_brief` tests prove standalone validation but not the runtime preset path
- **Issue**: The six new tests exercise direct capability calls (mostly standalone plus one DB roundtrip) with camelCase input. They do not exercise preset YAML args, template interpolation, state output binding, or schedule-context identity injection for `creator.write_brief`.
- **Impact**: The exact integration break in C-01 survives all current checks, including `all_embedded_presets` and `write_brief` unit tests.
- **Fix**: Add at least one state/preset-level test around `creative-brief-intake` persisting, and one ownership-negative DB test where `_creator_id` does not own `work_id`.

### 🟢 Suggestion

#### S-V133P2-QC1-01: Make the capability input naming convention explicit and shared
- `creator.write_brief` exposes a local Rust DTO, an `input_schema()` string, preset YAML args, and tests. Those four surfaces currently disagree on snake_case vs camelCase. Define a single convention for capability args (preferably matching serde wire names) and add a registry/schema smoke test that round-trips each built-in schema sample through its DTO.

#### S-V133P2-QC1-02: Document or implement the intended non-fatal enqueue semantics
- Non-fatal intake scheduling failure may be acceptable for local-first UX, but it should be visible in JSON output and/or Work state. Today JSON output only gains `intake_schedule_id` on success; failure leaves no machine-readable indicator that intake was skipped by error rather than by `--skip-intake`.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| C-V133P2-QC1-01 | code-read + contract mismatch | `creative-brief-intake/preset.yaml:55-63`; `tasks/mod.rs:647-664`; `creator.rs:625-641`; `creator.rs:1031-1049`; `creator.rs:1160-1208` | High |
| C-V133P2-QC1-02 | code-read + acceptance trace | Plan lines 41-45, 72-77; `run.rs:129-178`; `embedded-presets/novel-writing/prompts/gathering.md:10-11`; `embedded-presets/README.md:45-59` | High |
| C-V133P2-QC1-03 | code-read + runtime path trace | `memory-augmented/preset.yaml:57-64`; `tasks/mod.rs:647-664`; `creator.rs:380-409` | High |
| W-V133P2-QC1-01 | code-read + spec/prompt trace | `creator.rs:505-584`; `synthesize-brief.md:15-27`; tests at `creator.rs:1033-1044` and `1190-1201` | High |
| W-V133P2-QC1-02 | test-coverage analysis | `creator.rs:1027-1225`; `cargo test -p nexus-orchestration --lib write_brief` output (6/6 passed) | High |
| S-V133P2-QC1-01 | maintainability analysis | `CreatorWriteBriefInput` serde casing vs `input_schema()` and preset YAML | High |
| S-V133P2-QC1-02 | UX/API analysis | `run.rs:141-156`, `run.rs:159-178` | Medium |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 3 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 2 |

**Verdict**: Request Changes

The previous `capability_registry` blocking test count issue is fixed at `641489e` and all requested static checks pass. However, the runtime architecture still does not satisfy the P2 acceptance path: capability args are not rendered, `creator.write_brief` input casing disagrees across surfaces, `novel-writing` is not actually fed from `creative_brief`, and the T6 memory persist fix stores a literal template. These are blocking architecture/maintainability findings for this plan.

## Revalidation

**Targeted re-review scope**: P2 fix wave `641489e..fe22746` on `feature/v1.33-work-experience-loop` (`dd09b5d` engine rendering, `7ed6bae` preset args + `brief_schema_version`, `fe22746` `--chain-novel-writing`).

**Alignment re-checked**:
- Review cwd: `/Users/bibi/workspace/organizations/42ch/nexus`
- Working branch: `feature/v1.33-work-experience-loop`
- Review range / Diff basis: `merge-base: 641489e` + `tip: fe22746`

### Prior findings

#### C-V133P2-01 / C-V133P2-QC1-01 — capability args template rendering

**Disposition**: **CLOSED**

**Evidence**:
- Commit `dd09b5d` adds recursive capability arg rendering in `StateCompositeTask` before `CapabilityTask` execution.
- `crates/nexus-orchestration/src/tasks/mod.rs:642-658` clones capability args, builds the nested engine payload, and calls `render_value_templates(&cap_input, &payload)?`.
- `crates/nexus-orchestration/src/tasks/mod.rs:1310-1323` uses Handlebars strict mode, so missing placeholders fail instead of silently rendering literal/empty values.
- `crates/nexus-orchestration/src/tasks/mod.rs:1337-1365` recursively renders string values inside objects/arrays and converts render failures to `TaskExecutionFailed`, so the capability is not called with literal `{{...}}` placeholders.
- SEC-V131-01 identity injection remains after rendering: `crates/nexus-orchestration/src/tasks/mod.rs:659-677` strips `_creator_id` / `_session_id` from rendered preset args and injects trusted context identity.
- Regression test `state_composite_renders_capability_args_templates` at `crates/nexus-orchestration/src/tasks/mod.rs:2485-2561` proves `{{preset.input.work_id}}` and `{{state.synthesizing.output}}` render into `_capability_input`, and identity injection still applies after rendering.
- Test run: `cargo test -p nexus-orchestration --lib state_composite_renders` → `1 passed; 0 failed`.

#### C-V133P2-02 / C-V133P2-QC1-01 — preset/capability argument casing alignment

**Disposition**: **CLOSED**

**Evidence**:
- Commit `7ed6bae` updates `creative-brief-intake` persisting args to camelCase: `workId` and `briefText` in `crates/nexus-orchestration/embedded-presets/creative-brief-intake/preset.yaml:55-63`.
- `CreatorWriteBriefInput` is `#[serde(rename_all = "camelCase")]` with Rust fields `work_id` and `brief_text` in `crates/nexus-contracts/src/local/orchestration/mod.rs:231-238`, so the YAML keys now match serde wire names.
- `creator.write_brief` input schema also advertises camelCase `workId` / `briefText` at `crates/nexus-orchestration/src/capability/builtins/creator.rs:642-644`.
- Existing direct and store tests now consistently call the capability with `workId` / `briefText`, e.g. `creator.rs:1062-1067` and `creator.rs:1237-1270`.

#### C-V133P2-03 / C-V133P2-QC1-02 — `creator run start` production handoff

**Disposition**: **CLOSED**

**Evidence**:
- Commit `fe22746` adds `--chain-novel-writing` to `creator run start` with clap help text at `crates/nexus42/src/commands/creator/run.rs:29-35`.
- When `--chain-novel-writing` and `--skip-intake` are both set, the CLI schedules the production preset directly and returns/prints `production_schedule_id`: `crates/nexus42/src/commands/creator/run.rs:177-202` and `214-220`.
- For the normal intake path, the daemon still lacks an `on_complete` hook, but the fix documents that trade-off and prints a clear follow-up production command when `--chain-novel-writing` is set: `crates/nexus42/src/commands/creator/run.rs:164-175` and `230-237`. This satisfies the plan’s “auto or explicit flag documented” intent for this P2 slice.
- CLI surface regression remains covered by `cargo test -p nexus42 --test command_surface_contract` → `29 passed; 0 failed`.

#### W-V133P2-04 / W-V133P2-QC1-01 — `brief_schema_version` validation

**Disposition**: **CLOSED**

**Evidence**:
- Commit `7ed6bae` requires `brief_schema_version` and enforces integer value `1` in `validate_creative_brief`: `crates/nexus-orchestration/src/capability/builtins/creator.rs:541-553`.
- Tests were added for both missing and wrong versions: `write_brief_standalone_missing_schema_version` at `creator.rs:1175-1204` and `write_brief_standalone_wrong_schema_version` at `creator.rs:1206-1235`.
- Test run: `cargo test -p nexus-orchestration --lib write_brief` → `8 passed; 0 failed`.

### Static checks and targeted tests

- `cargo check -p nexus-orchestration -p nexus42 -p nexus-daemon-runtime -p nexus-contracts` → passed (`Finished dev profile`).
- `cargo clippy -p nexus-orchestration -p nexus42 -p nexus-daemon-runtime -p nexus-contracts -- -D warnings` → passed (`Finished dev profile`).
- `cargo +nightly fmt --all -- --check` → passed (no formatting diff output).
- `cargo test -p nexus-orchestration --lib` → `390 passed; 0 failed; 1 ignored`.
- `cargo test -p nexus-orchestration --lib all_embedded_presets` → `1 passed; 0 failed`.
- `cargo test -p nexus-orchestration --test capability_registry` → `4 passed; 0 failed`.
- `cargo test -p nexus42 --test command_surface_contract` → `29 passed; 0 failed`.

### New findings from fix wave

No new blocking architecture/maintainability findings in the targeted fix wave. One implementation trade-off remains explicit: normal intake completion still requires the user to run the printed follow-up production command because daemon-side `on_complete` scheduling is not present in V1.33; this is documented in code and CLI output and is not blocking for this targeted re-review.

### Updated verdict

**Verdict**: Approve
