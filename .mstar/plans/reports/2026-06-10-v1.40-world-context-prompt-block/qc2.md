---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-10-v1.40-world-context-prompt-block"
verdict: "Request Changes"
generated_at: "2026-06-10"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1 (xai/grok-build-0.1)
- Review Perspective: security and correctness risk
- Report Timestamp: 2026-06-10T00:00:00Z (session start verified via git rev-parse)

## Scope
- plan_id: 2026-06-10-v1.40-world-context-prompt-block
- Review range / Diff basis: iteration/v1.40..feature/v1.40-world-context-prompt-block (equivalently 9a795624..5ba65359)
- Working branch (verified): feature/v1.40-world-context-prompt-block
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 8 (primary: crates/nexus-moment-context-assembly/src/world_context.rs (728 lines new), crates/nexus-moment-context-assembly/src/moment.rs, crates/nexus-moment-context-assembly/src/lib.rs, crates/nexus-orchestration/embedded-presets/novel-writing/preset.yaml, crates/nexus-orchestration/embedded-presets/novel-writing/prompts/outline-chapter.md, crates/nexus-orchestration/embedded-presets/novel-writing/prompts/draft-chapter.md, crates/nexus-orchestration/src/preset/mod.rs, .mstar/knowledge/specs/cli-spec.md)
- Commit range: 5ba65359 (HEAD) ← 11eb62b0 ← e2610a76 ← 9a795624 (base)
- Tools run: git diff --name-only + git log, full file reads of world_context.rs + callers + preset wiring + prompt templates + plan, cargo test -p nexus-moment-context-assembly (lib world_context tests: 12/12 passed), grep for call sites and creator/workspace threading.

## Findings

### 🔴 Critical
(none)

### 🟡 Warning
- **W-01: Integration that populates `preset.input.world_kb_block` for novel-writing produce-stage schedules is absent from the reviewed diff** (file: crates/nexus-orchestration/src/stage_gates.rs:92 (build_preset_input), crates/nexus-orchestration/src/stage_gates.rs:240 (build_schedule_for_stage), crates/nexus-daemon-runtime (no changes)).  
  `build_chapter_kb_block` + `WorldContextBlock::to_yaml` exist and are unit-tested. The novel-writing preset.yaml (v7) and both prompt templates declare `world_kb_block: "{{preset.input.world_kb_block}}"` and guard it with `{{#if world_kb_block}}`. However, `build_preset_input` (WorkFields) and `build_schedule_for_stage` do not call the new builder, do not read the Work's `world_id` or the chapter's `world_refs`, and do not insert a `world_kb_block` key. No call site in daemon-runtime or orchestration engine (within the diff) assembles the block at schedule start time for the `produce` stage.  
  → **Impact**: Primary AC "World-bound Work: outline and draft prompts contain World context block" is not satisfied by code in 9a795624..5ba65359. The templates are ready but the data flow that would make World-bound Works actually receive the block (while legacy worldless Works continue to receive none) is not delivered.  
  → {fix}: Implement the caller (likely in stage_gates.rs near produce-stage handling, or a thin orchestration capability wrapper per plan T4) that (a) looks up the Work's `world_id`, (b) for the target chapter reads its `world_refs` from frontmatter or chapter record, (c) calls `build_chapter_kb_block` with correct `ChapterKbBlockParams`, (d) serializes via `to_yaml()` and places the string under `preset.input.world_kb_block` (or equivalent) before the schedule for `outline_chapter`/`draft_chapter` is created. Add an integration test that exercises the full path and asserts the rendered prompt contains the `## World Context` section for a world-bound Work.

- **W-02: `build_chapter_kb_block` / `WorldKbQueryBuilder` perform world-scoped queries correctly but carry no creator/workspace identity; the reviewed diff provides no evidence that the (missing) call site threads `creator_id` + `workspace_slug` to enforce isolation** (file: crates/nexus-moment-context-assembly/src/world_context.rs:226 (fn signature), 230 (WorldKbQueryBuilder::new(&params.world_id)), 289 (query_for_canonical_name + with_block_type), 308 (query_all)).  
  Every `KbQuery` is constructed with the supplied `world_id`; `resolve_items_by_refs` further narrows by canonical_name + BlockType. `InMemoryKbStore` tests and the "missing world_id returns empty block" test demonstrate that a wrong world_id yields no cross-world data. However, the function takes only `world_id` (plus caller-supplied `world_refs` and chapter text). There is no `creator_id` or `workspace_slug` parameter, and the diff contains zero changes to any authz / ownership check around the new path. The checklist item "Does the orchestrator correctly thread creator_id + workspace_slug through the call chain?" cannot be answered affirmatively from the changes under review because the call chain itself stops at the library boundary.  
  → **Impact**: Correct isolation is delegated entirely to the (absent) caller and the underlying `KbStore` impl. If a future caller passes a `world_id` that the authenticated creator should not see, the block will be built. Prior P0 mandatory-binding work is assumed to have closed world ownership, but this P2 diff does not re-verify or extend that boundary for the prompt-time chapter slice.  
  → {fix}: (1) In the integration that calls `build_chapter_kb_block`, explicitly pass/validate that the `world_id` comes from a Work the current `creator_id` owns (or has access to via workspace). (2) Consider adding a narrow `build_chapter_kb_block_for_work` helper (or documented contract) that takes the Work entity (or at minimum `creator_id` + `workspace_slug` + `world_id`) and re-asserts the ownership check before querying, even if the check is "best effort" and the store remains the final enforcer. (3) Add a negative test that attempts to build a block for a world the test creator does not own and confirms it is rejected before the KB query (or returns a clear authorization error rather than an empty block).

- **W-03: No 404 / remediation surface for missing `world_id` at the `build_chapter_kb_block` API boundary** (file: crates/nexus-moment-context-assembly/src/world_context.rs:549 (test missing_world_id_returns_empty_block), 558 (call), 260 (constructs block with the ghost world_id anyway)).  
  Plan AC4: "Query API returns 404 / remediation correctly for missing world_id." The function signature requires `world_id: String`; when the world has no KB items the result is `Ok(Some(WorldContextBlock { world_id: "wld_ghost", characters_in_chapter: [], ... }))` — a block with the supplied (non-existent) id and empty sections. No `KbStoreError`, no `Option::None` distinguishing "world not found", and no remediation guidance. The plan states this path should surface 404/remediation. The higher-level narrative gateway may do so, but the dedicated chapter KB block entry point (the one actually used for prompts) does not.  
  → **Impact**: Callers that blindly trust a `world_id` from a chapter record (or from `preset.input`) and pass it here will receive a well-formed but empty block rather than an error they can turn into a user-visible remediation ("this Work references a World that no longer exists — re-link or migrate"). Silent empty context can mask data-integrity issues.  
  → {fix}: Either (a) make `build_chapter_kb_block` return `Result<Option<...>, ...>` where `None` means "world has no items but exists" vs. a distinct error or `Err(KbStoreError::WorldNotFound)` when the world itself is unknown (requires store API support), or (b) document explicitly in the function and plan that the 404/remediation contract lives one layer up and that this helper is intentionally "best-effort empty on missing data." If the latter, update AC4 wording. At minimum add a test that distinguishes "world id present in narrative but zero KB items" from "world id unknown to the system."

### 🟢 Suggestion
- **S-01: User/LLM-controlled content from KB (`body.summary`, `canonical_name`, attributes) flows into the prompt with only YAML structural escaping (`{:?}`)** (file: crates/nexus-moment-context-assembly/src/world_context.rs:195 (kb_to_item), 93 (to_yaml name/descriptor), 33 (prompt template ```yaml fence + raw {{world_kb_block}})).  
  `descriptor` is taken verbatim from `kb.body.summary` (or empty). `name` is `canonical_name`. Both are placed via `{:?}` (Rust debug formatting, which double-quotes and escapes control chars). The resulting YAML is then inserted raw inside a markdown code block in the outline/draft prompt and the model is told to "Honor these when planning" / "Stay consistent with these." This is the intended design (World context for consistency), but prior LLM-extracted KB content (P1 taxonomy) or user-supplied canonical_names can contain prompt-injection-style instructions. YAML quoting prevents *structural* breakage of the context block but does not sanitize semantic content.  
  → {fix / hardening}: (a) Add a short note in the module docs and/or the prompt templates that the block contains untrusted (previously LLM-generated) narrative content and that the model must treat it as data, not instructions. (b) Consider a future lightweight content filter or length cap on individual descriptors if abuse is observed. Not a blocker for this slice.

- **S-02: `resolve_active_rules` performs an unbounded `query_all()` + client-side filter on every chapter prompt** (file: crates/nexus-moment-context-assembly/src/world_context.rs:308 (query_all), 313 (filter novel_category foundation|rules)).  
  For worlds with large KB this repeats a full scan on every outline/draft step. Correctness is fine (world-scoped), but it is the only place in the new code that does not use a `block_type` or `canonical_name` narrow query.  
  → {fix}: Add a follow-up slice (or note in the plan) to push the taxonomy filter into the store (e.g., `query_for_novel_category` or a dedicated `BlockType` for rules) or cache the small foundation+rules set per world. Low risk for V1.40 but worth tracking.

- **S-03: Truncation marker is clear but placed only after the YAML; consider surfacing `truncated: true` inside the YAML structure itself** (file: crates/nexus-moment-context-assembly/src/world_context.rs:121 (if truncated push marker), 347 (set flag after pops)).  
  The marker `# [... truncated]` appears at the very end of the emitted YAML when budget is exceeded. The `WorldContextBlock.truncated` bool is not serialized into the YAML (only the comment). A model reading the block may not programmatically notice it was cut unless it parses the trailing comment.  
  → {fix}: Optionally emit a final `truncated: true` key (or a sentinel item) inside the YAML when applicable, in addition to (or instead of) the comment marker. Keeps the signal inside the data the model is told to honor.

- **S-04: No hermetic test in the diff that renders an actual novel-writing prompt containing a non-empty `world_kb_block`** (plan verification commands reference `nexus-daemon-runtime` and `nexus42` tests for `chapter_kb_block world_context`; only lib unit tests exist and pass).  
  The 12 world_context tests are good and cover the ACs for the builder (world_refs filter, fallback, empty, truncation, YAML shape, legacy skip convention). However, they stop at `build_chapter_kb_block` returning a `WorldContextBlock`. There is no test that feeds a world-bound Work + chapter with `world_refs` through `build_preset_input` / schedule creation / prompt rendering and asserts the `## World Context` section + YAML appears in the `outline-chapter` or `draft-chapter` prompt for that schedule.  
  → {fix}: Add at least one integration-level test (even if behind a feature gate or using the in-memory orchestration test harness) that exercises the end-to-end path once the caller in W-01 lands. This will also serve as the regression guard that legacy worldless Works continue to see the pre-P2 prompt shape.

## Source Trace
- Finding ID: W-01 (primary blocking)
- Source Type: manual code inspection + grep + absence in `git diff 9a795624..5ba65359`
- Source Reference: `git grep -n build_chapter_kb_block` (only world_context.rs + its tests + lib.rs re-export), `git grep -n world_kb_block` (only preset.yaml, two prompt .md, one comment in preset/mod.rs), inspection of `build_preset_input` (stage_gates.rs:92–150) and `build_schedule_for_stage` (240), plan ACs 1/4/6.
- Confidence: High (the integration literally does not exist in the reviewed tree)

- Finding ID: W-02
- Source Type: function signature + call graph analysis + checklist cross-check
- Source Reference: world_context.rs:226 (pub async fn build_chapter_kb_block<K: KbStore>(store: &K, params: &ChapterKbBlockParams)), 139 (world_refs: Vec<String> from caller), 150 (WorldKbQueryBuilder holds only world_id), stage_gates.rs (no creator/workspace in WorkFields or build path for this var), absence of any authz wrapper in the diff.
- Confidence: High

- Finding ID: W-03
- Source Type: test + implementation behavior
- Source Reference: world_context.rs:549 (`missing_world_id_returns_empty_block` explicitly asserts empty sections for "wld_ghost"), 558 (the call under test), 260 (block is still constructed with the ghost id), plan AC4 text.
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 4 |

**Verdict**: Request Changes

(The library implementation of the World context block builder is surgically clean, correctly world-scoped, and well-tested for the narrow contract it exposes. However, the reviewed diff does not contain the integration that would cause World-bound Works to actually receive the block in their `novel-writing` outline/draft prompts, nor does it demonstrate the creator/workspace threading or 404/remediation behavior claimed in the plan's acceptance criteria and explicit review checklist. Until the call site that closes the loop from Work → chapter `world_refs` → `build_chapter_kb_block` → `preset.input.world_kb_block` is present and verified, the primary security and correctness properties (mandatory binding for World-bound Works, no new worldless creation surface, isolation) cannot be confirmed as delivered.)
