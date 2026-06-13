---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-13-v1.44-multi-volume-hardening"
verdict: "Approve"
generated_at: "2026-06-13"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: deepseek-v4-pro (volcengine-plan/deepseek-v4-pro)
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-13T23:00:00Z

## Scope
- plan_id: `2026-06-13-v1.44-multi-volume-hardening`
- Review range / Diff basis: `c54b1aa6..9c53d8f6`
- Working branch (verified): `iteration/v1.44`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 9
- Commit range: `22324ddc..b7d27aa7` (3 commits) + merge `9c53d8f6`
- Tools run: `cargo clippy --all -- -D warnings`, `cargo +nightly fmt --all --check`, `cargo test -p nexus-orchestration --test supervisor_cross_volume`, `cargo test -p nexus-local-db`, `git diff`, `git log`, `grep` (call-site audit)

## Findings
### 🔴 Critical
None.

### 🟡 Warning
None.

### 🟢 Suggestion
- **S-001 — `WorkFields.volume` is semantically stage-specific but lives on the shared struct**: The `volume` field is only meaningful for the `produce` stage (where `novel-writing` preset consumes `{{preset.input.volume}}`). It is a top-level field on `WorkFields` alongside `chapter`, `workspace_dir`, `world_id`, etc. In practice, callers only set `volume: Some(...)` when the stage is `"produce"` (supervisor `NextChapter` arm), so no incorrect data flows through. However, the struct doesn't enforce this constraint — a future caller could accidentally set `volume` for `research` or `review` stages. The existing codebase already has this pattern (`world_kb_block` is also stage-specific), so this is consistent. Consider documenting the stage-specificity in the field doc comment, or in a future refactor, using a stage-specific extension map.
  - Severity: Suggestion
  - Confidence: Medium

- **S-002 — `build_preset_input` volume injection uses `map()` for side effects**: The code uses `map.as_object_mut().map(|o| o.insert(...))` to inject `volume` into the JSON map. This is consistent with the existing `world_id` and `world_kb_block` injection patterns in the same function, but `map()` for side effects is less idiomatic than `if let Some(ref mut o) = map.as_object_mut() { o.insert(...); }`. Not a correctness issue — the `map()` call correctly mutates the inner object. Consider using `if let` in a future cleanup pass across all three injection sites.
  - Severity: Suggestion
  - Confidence: Low

## Source Trace
- Finding ID: S-001
- Source Type: manual-reasoning
- Source Reference: `crates/nexus-orchestration/src/stage_gates.rs` lines 87–93 (WorkFields struct), lines 215–221 (build_preset_input volume injection)
- Confidence: Medium

- Finding ID: S-002
- Source Type: manual-reasoning
- Source Reference: `crates/nexus-orchestration/src/stage_gates.rs` lines 218–221
- Confidence: Low

## Architecture Assessment

### F-002: `is_work_completed` hardening (work_chapters.rs)

**Design quality: Good.** The old implementation used two separate checks — `current_chapter >= total_planned_chapters` (flat, breaks when chapter numbers reset across volumes) and `list_chapters().len() == total` + `all finalized` (requires fetching all rows into memory). The new implementation replaces both with a single SQL aggregation query:

```sql
SELECT COUNT(*) AS total_rows,
       SUM(CASE WHEN status = 'finalized' THEN 1 ELSE 0 END) AS finalized_rows
FROM work_chapters WHERE work_id = ?
```

This is architecturally cleaner:
- **Single DB round-trip** instead of two (`SELECT works` + `list_chapters`)
- **No in-memory iteration** over chapter rows
- **Correctly volume-agnostic** — aggregates across all volumes without depending on chapter number semantics
- **Atomic check** — `total_rows == expected && finalized_rows == expected` in one comparison

The `current_chapter` column is correctly removed from the `SELECT` — it was never meaningful for cross-volume completion.

### F-004: Volume propagation through supervisor chain

**Design quality: Good.** The change threads `volume: Option<i32>` through three layers:

1. **Data layer**: `WorkFields.volume` (new field)
2. **Orchestration layer**: `build_auto_chain_schedule` and `enqueue_auto_chain_schedule` signatures extended
3. **Supervisor layer**: `enqueue_auto_chain_step` signature extended; `ChainAction::NextChapter` arm passes `Some(next_volume)`

The propagation path is linear and well-contained:
```
supervisor NextChapter arm
  → enqueue_auto_chain_step(..., Some(next_volume), ...)
    → enqueue_auto_chain_schedule(..., volume, ...)
      → build_auto_chain_schedule(..., volume, ...)
        → WorkFields { volume, ... }
          → build_preset_input → { "volume": 2, ... }
```

All call sites are updated (verified via `grep`):
- `supervisor.rs`: 3 call sites (NextChapter with `Some(next_volume)`, 2 other arms with `None`)
- `boot.rs`: 1 call site (`None` — daemon resume doesn't have volume context)
- `auto_chain.rs` unit tests: 7 call sites (all `None`)
- `supervisor_cross_volume.rs` integration tests: 2 call sites (`Some(2)` and `None`)
- `auto_chain.rs` integration tests: 1 call site (`None`)
- `fl_e_chain_demo.rs`: 1 `WorkFields` construction (`None`)
- `creator/run.rs`: 1 `WorkFields` construction (`None`)

### Test Architecture

**Coverage: Good.** 7 new regression tests across 2 crates:

| Test | Crate | What it covers |
|------|-------|----------------|
| `test_is_work_completed_multi_volume_all_finalized` | nexus-local-db | F-002 positive: 2-volume, all finalized |
| `test_is_work_completed_multi_volume_partial_vol2` | nexus-local-db | F-002 negative: vol2 has draft chapter |
| `test_is_work_completed_multi_volume_missing_vol2_rows` | nexus-local-db | F-002 edge: row count mismatch |
| `f004_supervisor_enqueue_includes_volume_in_preset_input` | nexus-orchestration | F-004 positive: volume in preset input |
| `f004_single_volume_enqueue_has_no_volume_in_input` | nexus-orchestration | F-004 negative: single-volume, no volume key |
| `f002_multi_volume_work_completed_all_volumes_finalized` | nexus-orchestration | F-002 integration: cross-crate verification |
| `f002_multi_volume_work_not_completed_partial_vol2` | nexus-orchestration | F-002 integration negative |

The tests are hermetic (each uses a fresh DB), cover positive/negative/edge cases, and include clear assertion messages referencing the finding IDs. The existing 4 supervisor_cross_volume tests (f001_*) remain green — no regression.

### Maintainability

- **No new dependencies** introduced
- **No new modules** created — changes are surgical within existing files
- **No schema migration** needed (V1.42 migration already provides multi-volume PK)
- **Backwards compatible**: `volume: Option<i32>` defaults to `None`; all non-multi-volume paths pass `None`
- **Doc comments** are thorough and reference the plan/finding IDs (F-002, F-004)
- **`// SAFETY:` comments** on runtime SQL queries follow the crate convention

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 2 |

**Verdict**: Approve

**Rationale**: The changes are architecturally sound — `is_work_completed` is hardened with a single SQL aggregation that correctly handles multi-volume Works, and the volume propagation chain is linear, well-contained, and all call sites are updated. Test coverage is thorough with 7 new hermetic regression tests. No Critical or Warning findings. Two minor Suggestions (S-001, S-002) are style/consistency observations that do not affect correctness or maintainability risk.
