---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-10-v1.40-architecture-hygiene"
verdict: "Approve"
generated_at: "2026-06-09"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: volcengine-plan/deepseek-v4-pro
- Review Perspective: architecture coherence and maintainability risk
- Report Timestamp: 2026-06-09T23:00:00Z

## Scope
- plan_id: 2026-06-10-v1.40-architecture-hygiene
- Review range / Diff basis: iteration/v1.40..feature/v1.40-architecture-hygiene
- Working branch (verified): feature/v1.40-architecture-hygiene
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 6
- Commit range: 3c90c18f..dc7f81e7
- Tools run: cargo test -p nexus-orchestration --lib (embedded_rules + read_rules_layers), cargo clippy -p nexus-orchestration -- -D warnings, rg (stale references, layering checks)

## Findings
### 🔴 Critical
(None)

### 🟡 Warning
(None)

### 🟢 Suggestion
- **S-1** `crates/nexus-orchestration/src/stage_gates.rs:854`: Test comment says "Layer 1 is always available from embedded presets" — should say "embedded rules" to match the new `embedded_rules` module. This is a cosmetic stale comment; the test behavior is correct and passes. -> Update comment to "embedded rules" in a future cleanup pass.
- **S-2** `crates/nexus-orchestration/src/stage_gates.rs:850`: Test section header "V1.39 P3: rules reader tests" could note that V1.40 P0.5 migrated the internal implementation from `read_embedded_template` to `embedded_rules::WRITING_CRAFT` while preserving test behavior. Low priority; useful for future readers tracing the evolution. -> Add a brief V1.40 annotation to the section header.

## Source Trace
- Finding ID: S-1
- Source Type: manual-reasoning
- Source Reference: `crates/nexus-orchestration/src/stage_gates.rs:854`
- Confidence: High

- Finding ID: S-2
- Source Type: manual-reasoning
- Source Reference: `crates/nexus-orchestration/src/stage_gates.rs:850`
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 2 |

**Verdict**: Approve

## Architecture Coherence Assessment

### Checklist

- [x] **`embedded_rules.rs` follows `embedded_skills.rs` pattern**: Yes. Both are `pub mod` in `lib.rs`, both have doc comments with layout diagrams, both have inline tests. The key divergence (`include_str!` vs `include_dir!`) is justified: rules are flat markdown files, skills are a directory tree with a manifest. The doc comment explicitly distinguishes rules ("pure content layers") from presets ("state machines").
- [x] **`include_str!` path resolves correctly**: `include_str!("../embedded-rules/writing-craft.md")` from `src/embedded_rules.rs` resolves to `crates/nexus-orchestration/embedded-rules/writing-craft.md`. File exists and is the same content as the old location (100% similarity per `git diff --stat`).
- [x] **`read_rules_layers()` public API preserved**: Signature unchanged (`pub fn read_rules_layers(workspace_dir: &str, work_ref: &str) -> Option<String>`). Only internal implementation changed from `crate::preset::read_embedded_template("rules", ...)` to `crate::embedded_rules::WRITING_CRAFT`.
- [x] **No new layering violations**: `embedded_rules.rs` is a new module within `nexus-orchestration` — same crate, no cross-crate dependency introduced. No `nexus-kb` or other crate boundary crossed.
- [x] **`EmbeddedPresetId` / preset loader not modified**: `rg` confirms zero references to `EmbeddedPresetId` or `PresetId` in the source tree. The preset loader (`preset/mod.rs`) is untouched.
- [x] **Doc consistency**: `deferred-features-cross-version-tracker.md` DF-65 row updated from "V1.39 P3 Shipped (path fix V1.40 P0.5)" to "V1.40 P0.5 Shipped" with correct new path. `world-kb-runtime-architecture.md` §1 and §8 updated from `embedded-presets/rules/` to `embedded-rules/`. Both are internally consistent.
- [x] **V1.40 compass references**: `v1.40-novel-world-kb-delivery-compass-v1.md` references `world-kb-runtime-architecture.md` at lines 20 and 100 — both still valid after the doc update.
- [x] **No stale references to `embedded-presets/rules`**: `rg -n 'embedded-presets/rules'` in source files (excluding `target/` and `.git/`) returns zero matches. The old directory has been deleted (commit `de42f4c9`).
- [x] **`read_rules_layers` tests still cover same scenarios**: All 3 tests pass — Layer 1 from embedded, both layers when Layer 2 exists, empty Layer 2 skipped. Test assertions unchanged.
- [x] **No maintainability smells**: No dead imports, no duplicate constants, no unnecessary public surface. Clippy passes clean with `-D warnings`. `read_embedded_template` remains in use for preset templates (prompts, templates) — its removal from the rules path is correct and does not leave dead code.

### Commit-by-commit trace

| Commit | Description | Assessment |
|--------|-------------|------------|
| `8dda000c` | Add `embedded-rules` module, relocate `writing-craft.md` | Clean: new module with doc, const, and 3 tests. File rename is 100% identical. |
| `76e14828` | Read Layer 1 from `embedded_rules` module | Clean: replaces `read_embedded_template("rules", ...)` with `embedded_rules::WRITING_CRAFT`. No signature change. |
| `de42f4c9` | Remove `embedded-presets/rules/` directory | Clean: old directory deleted after migration. No orphan files. |
| `dc7f81e7` | Update DF-65 tracker and architecture doc paths | Clean: two doc files updated, paths consistent with new layout. |
