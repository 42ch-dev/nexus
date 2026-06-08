---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-08-v1.38-novel-writing-parameterization"
verdict: "Approve"
generated_at: "2026-06-08"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: kimi-for-coding/k2p6
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-06-08T00:00:00Z

## Scope
- plan_id: `2026-06-08-v1.38-novel-writing-parameterization`
- Review range / Diff basis: `merge-base(8e58890a, HEAD)..HEAD` on `iteration/v1.38` (commit `ad455ec5 merge(v1.38-p1)` brings in 5 feature commits).
- Working branch (verified): `iteration/v1.38`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 10 (per `git diff --stat 8e58890a..HEAD`)
- Commit range: `8ba5b296 703d5834 8052a131 fe53220d 3cd14f96 ad455ec5`
- Tools run: `cargo clippy -p nexus-local-db -p nexus-orchestration -p nexus-daemon-runtime -p nexus42 -- -D warnings` (exit 0); `cargo test -p nexus-orchestration --lib stage_gates` (37 passed); `cargo test -p nexus-orchestration --test e2e_novel_writing` (11 passed)

## Acceptance Criteria Review

| AC | Description | Verdict | Evidence |
|----|-------------|---------|----------|
| AC1 | `novel-writing` can run for selected chapter 2 and writes/reads `ch02` outline/body paths. | Pass | `build_preset_input_chapter2_includes_all_context_fields` test asserts ch02 paths; `schedule_for_produce_chapter2_includes_all_context` asserts full schedule context; preset.yaml references `{{outline_path}}` / `{{body_path}}` |
| AC2 | No implementation creates or requires separate `novel-writing-chapter-N` presets. | Pass | Single `novel-writing` preset version bumped 5→6; no new preset IDs introduced |
| AC3 | Chapter 1 behavior remains compatible through the same selected-chapter input path. | Pass | `build_preset_input_chapter1_compat` test; `outline-chapter.md` and `draft-chapter.md` default `chapter_label="01"`, `slug="ch01"` |
| AC4 | Prompt rendering no longer relies on hard-coded chapter 1 values where selected chapter context is available. | Pass | `ch0{{chapter}}` literals removed from both templates; replaced with parameterized vars; diff shows 18 lines changed in `draft-chapter.md`, 5 in `outline-chapter.md` |
| AC5 | Finalizing a chapter does not automatically enqueue the next chapter. | Pass | No auto-chain logic touched; `finalize_commit` state transitions `draft→finalized` but has no `next:` enqueue action; DF-53 remains deferred per plan §8 |
| AC6 | Tests cover chapter 2+ rendering/path behavior and one-chapter compatibility. | Pass | 5 new stage_gates tests (ch2, ch10, ch1 compat, None omission, produce schedule); e2e test seeds new vars; fl_e_chain_demo updated |

## Findings

### 🔴 Critical
_None._

### 🟡 Warning
_None._

### 🟢 Suggestion

- **S1: Extract repeated `build_preset_input` field insertion pattern into helper** — `stage_gates.rs` lines 94–120 repeat `if let Some(ref x) = fields.y { map.as_object_mut().map(|o| o.insert(...)); }` four times. A small private helper (e.g., `insert_if_some(&mut map, "key", &fields.field)`) would reduce line count and avoid future copy-paste drift. Severity: cosmetic; no runtime impact.

- **S2: `_deprecated/` quarantine does not exclude files from embedded binary** — `draft-body.md` and `draft-intro.md` moved to `prompts/_deprecated/` but `include_dir!` still embeds them. They are not referenced by `preset.yaml`, so they are not loaded at runtime, but they remain in the binary and would be counted by any code enumerating `prompts_dir.files()`. Consider deleting rather than moving, or documenting the quarantine intent in `embedded-presets/novel-writing/prompts/_deprecated/README.md`. Severity: minor hygiene.

- **S3: `stage_advance` lacks audit logging for chapter context extraction** — The CLI `stage_advance` function (crates/nexus42/src/commands/creator/run.rs:985–1017) extracts `chapter_label`, `outline_path`, `body_path`, `slug` from the daemon response but does not log them. Adding a `tracing::info!` or `debug!` span showing the selected chapter context would aid production debugging when chapter selection behaves unexpectedly. Severity: observability gap.

- **S4: O(n) chapter lookup in `stage_advance` could be documented** — The `chapters.iter().find(...)` in `run.rs` is linear over the chapters array. At typical novel scales (≤100 chapters) this is negligible, but a brief comment noting the complexity assumption would help future maintainers evaluate whether to add an index if multi-volume works with 100+ chapters become common. Severity: documentation.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|-------------------|------------|
| S1 | manual-reasoning | `crates/nexus-orchestration/src/stage_gates.rs:94–120` | High |
| S2 | manual-reasoning | `git diff --name-status` + `find prompts/` + `preset/mod.rs:389–397` | High |
| S3 | manual-reasoning | `crates/nexus42/src/commands/creator/run.rs:985–1017` | High |
| S4 | manual-reasoning | `crates/nexus42/src/commands/creator/run.rs:988–996` | High |

## Diff Scope Check

**Files changed (10):**
- `crates/nexus-orchestration/embedded-presets/novel-writing/preset.yaml`
- `crates/nexus-orchestration/embedded-presets/novel-writing/prompts/draft-body.md` → `_deprecated/`
- `crates/nexus-orchestration/embedded-presets/novel-writing/prompts/draft-intro.md` → `_deprecated/`
- `crates/nexus-orchestration/embedded-presets/novel-writing/prompts/draft-chapter.md`
- `crates/nexus-orchestration/embedded-presets/novel-writing/prompts/outline-chapter.md`
- `crates/nexus-orchestration/src/preset/mod.rs`
- `crates/nexus-orchestration/src/stage_gates.rs`
- `crates/nexus-orchestration/tests/e2e_novel_writing.rs`
- `crates/nexus-orchestration/tests/fl_e_chain_demo.rs`
- `crates/nexus42/src/commands/creator/run.rs`

**Deferred boundary verification:**
| Boundary | Touched? | Notes |
|----------|----------|-------|
| Auto-chain (DF-53) | No | No enqueue/next-chapter logic added |
| World KB (DF-63) | No | No world context injection changes |
| Quality loop (DF-64/65/66/67) | No | No findings/rules/logs added |
| Multi-volume PK migration | No | No schema or volume changes |
| Platform publish | No | No publish surface touched |
| Multi-work switch | No | No scheduler changes |
| Selection pool | No | No pool logic |

All deferred boundaries respected. Diff is additive-only within P1 scope.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 4 |

**Verdict**: Approve

All acceptance criteria pass. CI gate clean. No performance or reliability regressions identified. The new optional fields are safely omitted when absent, preserving backward compatibility. Chapter 1 behavior is preserved through default template values. The O(n) chapter lookup and per-field allocation patterns are appropriate for the expected workload (≤100 chapters, schedule-build-time not hot path).
