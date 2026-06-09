---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-10-v1.40-world-context-prompt-block"
verdict: "Request Changes"
generated_at: "2026-06-10"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: volcengine/deepseek-v4-pro
- Review Perspective: architecture coherence and maintainability risk
- Report Timestamp: 2026-06-10T12:00:00Z

## Scope
- plan_id: 2026-06-10-v1.40-world-context-prompt-block
- Review range / Diff basis: iteration/v1.40..feature/v1.40-world-context-prompt-block (9a795624..5ba65359)
- Working branch (verified): feature/v1.40-world-context-prompt-block
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 8
- Commit range: 9a795624..5ba65359 (3 commits)
- Tools run: cargo test (lib + e2e), cargo clippy, git diff, grep

## Findings
### 🔴 Critical
- **C-001: e2e test regression — `preset.input.world_kb_block` not seeded** (`crates/nexus-orchestration/tests/e2e_novel_writing.rs:38-68`)  
  The preset YAML now passes `world_kb_block: "{{preset.input.world_kb_block}}"` as a capability arg for both `outline_chapter` and `draft_chapter` states. The template engine renders capability args in strict mode, so the missing var causes `e2e_novel_writing_traverses_all_outer_states` to panic with `Failed to access variable in strict mode Some("preset.input.world_kb_block")`. The test helper `seed_novel_writing_preset_input()` was not updated to seed this var.  
  -> Add `ctx.set("preset.input.world_kb_block", "").await;` to `seed_novel_writing_preset_input()` in `crates/nexus-orchestration/tests/e2e_novel_writing.rs` (after line 68).

### 🟡 Warning
- **W-001: Runtime wiring incomplete — `build_preset_input` does not populate `world_kb_block`** (`crates/nexus-orchestration/src/stage_gates.rs:92-167`)  
  The preset YAML wires `world_kb_block` as a template var for both outline and draft states, but `build_preset_input()` in `stage_gates.rs` does not include it. There is no call to `build_chapter_kb_block` anywhere in `nexus-daemon-runtime` or `nexus-orchestration` source. This means the World context block will always be empty (`""`) for actual novel-writing sessions — the feature is non-functional end-to-end. The plan marks T4 ("Optional orchestration capability wrapper") as optional, but without any runtime population path, AC1 ("World-bound Work: outline and draft prompts contain World context block with required fields") cannot be verified.  
  -> Either implement T4 (capability wrapper calling `build_chapter_kb_block`) in this plan, or defer the preset YAML wiring to a follow-up plan and revert the `world_kb_block` capability arg from `preset.yaml` for now. If deferring, add a residual tracking the wiring gap.

- **W-002: `chapter_text` field is dead code — heuristic fallback not implemented** (`crates/nexus-moment-context-assembly/src/world_context.rs:141`)  
  `ChapterKbBlockParams::chapter_text` is defined and documented as "Optional outline or body text for heuristic fallback" but is never read by `build_chapter_kb_block`. The spec (`novel-workflow-profile.md` §3.5.1.3) says characters/locations should be selected "from outline/body heuristics if needed" when `world_refs` is empty, but the current implementation only falls back to all characters/locations in the world — no text-based heuristic extraction.  
  -> Either implement the heuristic fallback using `chapter_text`, or remove the field and update the doc comment to clarify that the fallback is "all items" (not text-based heuristics). If deferring heuristics, register a residual.

### 🟢 Suggestion
- **S-001: `resolve_active_rules` queries all items then filters in Rust** (`crates/nexus-moment-context-assembly/src/world_context.rs:297-315`)  
  The function calls `builder.query_all()` which retrieves every KeyBlock in the world, then filters by `novel_category` in application code. For worlds with many items, this could be inefficient.  
  -> Consider adding a `novel_category` filter to `KbQuery` in `nexus-kb`, or use a `BlockType`-based pre-filter if foundation/rules items have predictable block types. Low priority for V1.40.

- **S-002: `apply_token_budget` recomputes full YAML on every pop — O(n²)** (`crates/nexus-moment-context-assembly/src/world_context.rs:327-348`)  
  Each iteration of the truncation loop calls `block.to_yaml()` which rebuilds the entire YAML string. For large worlds this is O(n²) in the number of items.  
  -> Track character count incrementally, or compute YAML once and truncate the string directly. Low priority; only matters for worlds with hundreds of items.

- **S-003: `to_yaml()` uses Debug format (`{:?}`) for string fields** (`crates/nexus-moment-context-assembly/src/world_context.rs:83-128`)  
  String values are formatted with `{:?}` which adds double-quote escaping. While this produces valid YAML for simple ASCII strings, it's not idiomatic YAML serialization and may produce unexpected output for strings containing special characters (newlines, Unicode).  
  -> Consider using `serde_yaml` for serialization or implement proper YAML string escaping. Low risk since World KB names/descriptors are typically short ASCII.

- **S-004: Module size — `world_context.rs` at 728 lines** (`crates/nexus-moment-context-assembly/src/world_context.rs`)  
  At 728 lines (with ~300 lines of tests), this is the second-largest module in the crate after `moment.rs` (1001 lines). The tests are inline rather than in a separate test file.  
  -> Consider moving tests to `tests/world_context_tests.rs` to keep the source module focused. Not blocking.

- **S-005: Pre-existing `runtime_compatibility.rs` test broken (not caused by this PR)** (`crates/nexus-moment-context-assembly/tests/runtime_compatibility.rs:8`)  
  This test requires the `cloud-stage` feature but is not gated behind `#[cfg(feature = "cloud-stage")]`. It was broken before this PR and is unrelated to the current changes.  
  -> Fix in a separate hygiene PR; not in scope for this review.

## Source Trace
- Finding ID: C-001
- Source Type: test-failure
- Source Reference: `cargo test -p nexus-orchestration -- e2e_novel_writing_traverses_all_outer_states`
- Confidence: High

- Finding ID: W-001
- Source Type: manual-reasoning
- Source Reference: `grep -rn "build_chapter_kb_block\|world_kb_block" crates/nexus-daemon-runtime/src/ crates/nexus-orchestration/src/` (no results in daemon; only test assertion in orchestration)
- Confidence: High

- Finding ID: W-002
- Source Type: manual-reasoning
- Source Reference: `grep -n "chapter_text" crates/nexus-moment-context-assembly/src/world_context.rs` (defined at line 141, only set to `None` in tests, never read in `build_chapter_kb_block`)
- Confidence: High

- Finding ID: S-001
- Source Type: manual-reasoning
- Source Reference: `crates/nexus-moment-context-assembly/src/world_context.rs:297-315`
- Confidence: Medium

- Finding ID: S-002
- Source Type: manual-reasoning
- Source Reference: `crates/nexus-moment-context-assembly/src/world_context.rs:327-348`
- Confidence: Medium

- Finding ID: S-003
- Source Type: manual-reasoning
- Source Reference: `crates/nexus-moment-context-assembly/src/world_context.rs:83-128`
- Confidence: Medium

- Finding ID: S-004
- Source Type: static-analysis
- Source Reference: `wc -l crates/nexus-moment-context-assembly/src/world_context.rs` → 728
- Confidence: Low

- Finding ID: S-005
- Source Type: test-failure
- Source Reference: `cargo test -p nexus-moment-context-assembly --test runtime_compatibility` (pre-existing, not in diff)
- Confidence: High

## Checklist Results

### Architecture + Maintainability (this reviewer's focus)

- [x] Does the new `world_context.rs` module follow the pattern of other modules in `nexus-moment-context-assembly`?  
  **Yes.** Module structure, re-exports, and `#[allow(clippy::future_not_send)]` pattern match `moment.rs` and `stage0.rs`.

- [x] Is the public API surface of `nexus-moment-context-assembly` (re)exporting `WorldKbQueryBuilder` and `build_chapter_kb_block` consistent with the architecture doc §6?  
  **Yes.** `lib.rs` re-exports match the architecture doc's read path: `build_chapter_kb_block`, `WorldKbQueryBuilder`, `WorldContextBlock`, `WorldContextItem`, `ChapterKbBlockParams`, `DEFAULT_WORLD_CONTEXT_TOKEN_BUDGET`.

- [x] Does `nexus-orchestration` correctly call into the new module without adding inline DB query (grill-me #12 lock)?  
  **Partially.** The preset YAML wires the template var correctly, and there is no inline DB query in orchestration. However, the runtime wiring (populating `world_kb_block` at schedule time) is missing — see W-001.

- [x] Does the `{{ world_kb_block }}` template var integrate cleanly with the existing template rendering?  
  **Yes.** The prompt templates use `{{#if world_kb_block}}` guards with `default: ""`, and the preset YAML passes it as a capability arg. The e2e test failure (C-001) is a test seeding issue, not a template design issue.

- [x] Is the legacy V1.39 worldless path correctly isolated (not exposed to new V1.40 worldless creation)?  
  **Yes.** `build_chapter_kb_block` requires a non-optional `world_id: String`. The prompt template uses `{{#if world_kb_block}}` guard with `default: ""`. Legacy worldless Works will have an empty string and skip the block.

- [x] Are the prompt template changes minimal and surgical?  
  **Yes.** Only 11 lines added to each prompt template (var declaration + `{{#if}}` block). No changes to existing sections.

- [x] Does the `cli-spec.md` debug note correctly point to `creator kb --scope world`?  
  **Yes.** The note at line 295 correctly documents the debug path and explains that no new subcommand is needed.

- [ ] Any maintainability smell: dead code, duplicate constants, unnecessary public surface, unused imports?  
  **W-002:** `chapter_text` field is dead code. **S-003:** `{:?}` formatting for YAML strings.

- [x] Is the new `world_context.rs` (728 lines) appropriately sized, or should it be split?  
  **Acceptable.** 728 lines is within range for a focused module. ~300 lines are tests. See S-004 for optional test extraction.

- [x] Does the `WorldKbQueryBuilder` correctly separate "query" from "format" (per architecture doc §6)?  
  **Yes.** `WorldKbQueryBuilder` handles query construction only (filter by `block_type`, `canonical_name`, or all). `build_chapter_kb_block` handles formatting (YAML block assembly, token budget, truncation). Clean separation.

### Shared Baseline

- [x] Regression risk: e2e test regression identified (C-001). No other regressions detected.
- [x] Security/correctness: No injection or path traversal concerns. Input is internal (World KB data).
- [x] Maintainability: Dead code (W-002), Debug-format YAML (S-003), O(n²) truncation (S-002).
- [x] Test coverage: 12 unit tests in `world_context.rs` covering all acceptance criteria. e2e test needs fix (C-001).

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 1 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 5 |

**Verdict**: Request Changes

**Rationale**: C-001 (e2e test regression) is blocking. W-001 (runtime wiring gap) means the feature is non-functional end-to-end and should be resolved before merge. W-002 (dead code) should be addressed or tracked as a residual.
