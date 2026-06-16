---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-16-v1.48-findings-consumer"
verdict: "Request Changes"
generated_at: "2026-06-16"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: zhipuai-coding-plan/glm-5.2
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-16T08:35:00Z

## Scope
- plan_id: `2026-06-16-v1.48-findings-consumer`
- Review range / Diff basis: `merge-base: 975899e7895cacc34f4966c1e872c93cac670ace (origin/main pre-V1.48) + tip: 53108f79 (iteration/v1.48 HEAD)`; for P1 scope, focus on commits `7119350a..c6ba7622`
- Working branch (verified): `iteration/v1.48`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 11 P1 files (findings.rs DAO, findings_block.rs builder, stage_gates.rs WorkFields, auto_chain.rs enqueue path, preset/mod.rs, lib.rs, novel-writing/preset.yaml, outline-chapter.md, draft-chapter.md, findings_consumer.rs integration tests, nexus42 run.rs CLI path)
- Commit range (reviewed): `7119350a..c6ba7622` (T1 DAO at base `7119350a`; T2–T5 + clippy/fmt + merge at `c6ba7622`)
- Tools run: `cargo clippy --all -- -D warnings`, `cargo test -p nexus-orchestration --test findings_consumer`, `cargo test -p nexus-orchestration -- findings_block`, `cargo test -p nexus-local-db -- findings`, `git diff`, `git log`

## Findings

### 🔴 Critical

_None._

### 🟡 Warning

#### W-1 — Integration test re-derives the findings block instead of asserting the wired schedule row; `Some(block)` → `preset.input` mapping is untested

- **Concern**: The T3 integration test
  `novel_writing_outline_includes_open_findings_block_when_seeded` (in
  `crates/nexus-orchestration/tests/findings_consumer.rs`) calls
  `enqueue_auto_chain_schedule` (which internally runs
  `compute_open_findings_block_for_produce` → DAO → builder →
  `build_auto_chain_schedule` → `WorkFields { open_findings_block }` →
  `build_preset_input`), but then **re-derives** the block independently
  via `list_open_findings_for_chapter` + `build_open_findings_block` and
  asserts the rendered outline prompt contains it. It never reads back
  the stored `creator_schedules.preset_input` row to assert the block was
  actually threaded through the wiring.
- **Impact**: If the enqueue path silently dropped the block — e.g. a
  regression in `build_auto_chain_schedule` (new `open_findings_block`
  param not forwarded to `WorkFields`) or in `build_preset_input` (the
  injection block removed) — this test would still pass, because it
  reconstructs the block from the DAO independently. The T3 wiring claim
  ("Wire into `novel-writing` preset prompt paths") is therefore not
  directly guarded by any test.
- **Corroborating gap**: There is **no** unit test for
  `build_preset_input` or `build_auto_chain_schedule` with
  `open_findings_block: Some(...)`. Every existing test site
  (`stage_gates.rs`, `auto_chain.rs`) constructs `WorkFields` / calls
  `build_auto_chain_schedule` with `open_findings_block: None`. A
  repository-wide search for `open_findings_block:\s*Some` in
  `crates/nexus-orchestration/{src,tests}/` returns zero matches. The
  `Some → preset.input.open_findings_block` mapping is exercised only as
  a side effect inside `enqueue_auto_chain_schedule`, and that side
  effect is not asserted.
- **Fix (either is sufficient)**:
  1. In the integration test, after `enqueue_auto_chain_schedule`,
     read the stored `preset_input` JSON from `creator_schedules` and
     assert `preset_input["open_findings_block"]` is non-empty and
     contains the seeded finding titles (e.g. "World rule break"); or
  2. Add a `build_preset_input` unit test in `stage_gates.rs` that
     constructs `WorkFields { open_findings_block: Some("BLOCK-XYZ".into()), .. }`
     and asserts the returned JSON has
     `["open_findings_block"] == "BLOCK-XYZ"`.

### 🟢 Suggestion

#### S-1 — Builder emits its own `## Open findings (chapter N)` H2 inside the prompt template's `## Open Findings to Address` H2 (structural redundancy + DRY)

- `build_open_findings_block` prepends `## Open findings (chapter {chapter_label})\n\n`
  as the block's first line (lines 105, 143). The `outline-chapter.md`
  and `draft-chapter.md` prompt templates wrap the injected block under a
  separate `## Open Findings to Address` H2 heading. The rendered prompt
  therefore contains two consecutive H2 section headings for the same
  content (template intro heading + builder's own heading).
- The heading format string is also duplicated **within** the builder:
  `write!(out, "## Open findings (chapter {chapter_label})\n\n")` at line
  105 and `format!("## Open findings (chapter {chapter_label})\n\n")` at
  line 143 (for the "nothing appended past heading" guard). If the
  heading format ever changes, both must be updated in lockstep.
- **Suggested fix**: pick one heading owner. Either (a) drop the
  builder's internal heading and expose the chapter label as a separate
  concern, letting the template own the section intro; or (b) demote the
  template wrapper heading to an introductory paragraph and let the
  builder's heading be the section anchor. Either choice also collapses
  the DRY duplication inside the builder.

#### S-2 — `preset_version_for_id` remains a hand-maintained mirror of `preset.yaml` version (pre-existing pattern, awareness only)

- The v7→v8 bump is correctly coordinated in three places:
  `embedded-presets/novel-writing/preset.yaml` (`version: 8`),
  `auto_chain.rs::preset_version_for_id` (`"novel-writing" => 8`), and
  `preset/mod.rs` test assertion (`loaded.version, 8`). The overlay §2.4
  cross-ref documents the bump rationale.
- However, `preset_version_for_id` is a hand-maintained mirror — a future
  bump that updates the YAML but forgets the mapping would silently
  version-mismatch. The `preset/mod.rs` assertion guards the embedded
  load path but not the `preset_version_for_id` mirror. This is a
  pre-existing pattern (not introduced by P1); noting for awareness. No
  P1 action required.

## Source Trace
- Finding ID: W-1
  - Source Type: manual-reasoning + grep test-coverage audit
  - Source Reference: `rg "open_findings_block:\s*Some" crates/nexus-orchestration/{src,tests}/` (0 matches); `tests/findings_consumer.rs:247-274` (re-derivation instead of reading stored row); `git diff` of `stage_gates.rs` test sites (all `None`)
  - Confidence: High
- Finding ID: S-1
  - Source Type: git-diff + manual-reasoning
  - Source Reference: `findings_block.rs:105,143`; `prompts/outline-chapter.md` diff `## Open Findings to Address`; `prompts/draft-chapter.md` same
  - Confidence: High
- Finding ID: S-2
  - Source Type: git-diff
  - Source Reference: `preset.yaml:17`, `auto_chain.rs:1199`, `preset/mod.rs:312`
  - Confidence: High

## Architecture / Maintainability Assessment (focus area)

**Strengths** (aligned with Assignment checklist):

- **`FindingsBlockBuilder` factoring (T2)**: `build_open_findings_block` is a
  pure `&[Finding] -> String` with no DB pool in its signature. The DB I/O
  lives at the call site (`enqueue_auto_chain_schedule` for the auto-chain
  path; `assemble_open_findings_block` in the CLI). This mirrors the
  established `world_kb_block` pattern and keeps the builder trivially
  testable. ✓
- **Cap constants SSOT**: `MAX_FINDINGS=8`, `MAX_BODY_CHARS=400`,
  `MAX_TOTAL_BLOCK_CHARS=3200` are declared once at the module top
  (`findings_block.rs:24-31`) and referenced everywhere (builder loop +
  tests). The cap values match overlay §2.2 (8/400/3200), and the "Cap
  value note" in overlay §2.4 explicitly documents the authority decision
  (overlay wins over the plan's earlier "10/200" suggestion). ✓
- **`WorkFields.open_findings_block: Option<String>` (T3)**: optional field,
  defaults to `None`; `build_preset_input` coerces to empty string via
  `unwrap_or_default()` (same pattern as `world_kb_block`), so strict-mode
  template rendering never fails on a missing var. Consistent. ✓
- **Preset version bump (v7→v8)**: coordinated across `preset.yaml`,
  `preset_version_for_id`, and the `preset/mod.rs` test assertion;
  documented in overlay §2.4 cross-ref table. ✓
- **T1 DAO `list_open_findings_for_chapter`**: compile-time-checked
  `sqlx::query!` ✓; chapter-scoping predicate `(chapter = ? OR chapter IS NULL)`
  matches overlay §2.1 exactly ✓; ordering via `CASE severity … DESC, created_at ASC`
  matches §2.1 ✓; no count cap in the DAO (builder enforces §2.2 limits) ✓;
  creator-scoped for isolation ✓.
- **Test hermeticity**: all DAO + builder + integration tests use fresh
  `tempfile`-backed pools (`fresh_pool()` / `test_pool()`); no shared
  state. ✓
- **Spec overlay §2.4 cross-ref (T5)**: well-organized concern→locus→notes
  table; the cap-value authority decision is explicitly documented in the
  "Cap value note" paragraph, satisfying the authority hierarchy. ✓
- **Dual-path consistency (auto-chain vs CLI)**: both paths funnel through
  the shared `build_open_findings_block`; the CLI path correctly
  re-sorts via the shared `sort_open_findings` helper (its source query
  `list_findings` orders by `created_at DESC`, so the client-side re-sort
  is necessary and correctly applied). ✓

**Net**: the architecture is clean and maintainable. The one Warning
(W-1) is a test-wiring-assertion gap, not an architecture defect — the
production code is correctly wired; the gap is that no test directly
guards that wiring.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 2 |

**Verdict**: Request Changes

Rationale: one open Warning (W-1) — the T3 wiring (`Some(block)` →
stored schedule `preset.input.open_findings_block`) is not directly
asserted by any test. Per the QC gate rule, an unresolved Warning
mandates `Request Changes`. The fix is small and localized (add a
stored-row assertion or a `build_preset_input` unit test with
`Some(block)`). No Critical findings; the architecture and
maintainability of the P1 slice are sound.
