---
plan_id: 2026-06-10-v1.40-world-create-and-validation
verdict: Approve
generated_at: 2026-06-10
---

# Code Review Consolidated — P0 world-create-and-validation (mandatory binding)

## Plan
- **plan_id**: `2026-06-10-v1.40-world-create-and-validation` (P0)
- **Working branch**: `feature/v1.40-world-create-and-validation` (HEAD `ec726e3a`)
- **Review range / Diff basis**: `iteration/v1.40..feature/v1.40-world-create-and-validation`
- **Iteration compass**: `.mstar/iterations/v1.40-novel-world-kb-delivery-compass-v1.md` (decision #4 = `required` after spec amendment `464d0fba`)
- **Primary spec**: `.mstar/knowledge/specs/novel-writing/workflow-profile.md` (mandatory binding amendment)

## Reviewer verdicts
| Reviewer | Lens | Verdict (initial) | Verdict (re-validation) |
| --- | --- | --- | --- |
| @qc-specialist | architecture coherence / maintainability | Request Changes (1C,2W,3S) | **Approve** (0C,0W,3S) |
| @qc-specialist-2 | security / correctness | Request Changes (0C,4W,5S) | **Approve** (0C,0W,5S historical) |
| @qc-specialist-3 | performance / reliability | Request Changes (0C,2W,2S) | **Approve** (0C,0W,2S) |

## Blocking findings (initial round) → all resolved in `d3a18d14`
| ID | Source | Title | Fix |
| --- | --- | --- | --- |
| C-1 | qc1 | 7 `findings_api.rs` integration tests broken | Added `seed_test_creator_and_world()`; updated helpers |
| W-1 | qc1 | HTTP 400 vs 422 inconsistency | Mapped `WORLD_ID_REQUIRED` / `INVALID_WORLD_ID` / `WORLD_CLEAR_FORBIDDEN` to `UNPROCESSABLE_ENTITY` (422) |
| W-2 | qc1 | Dead `map_or_else` fallback in `novel_scaffold.rs` | Removed / refactored to reachable path |
| W-01 / W-1 | qc2 / qc3 | `create_world` not atomic with scaffold tx | Added `create_world_tx(&mut Transaction)`; restructured scaffold to single tx |
| W-02 | qc2 | FK check lacks creator/workspace ownership filter | Tightened queries with `AND owner_creator_id = ?`; added cross-creator rejection test |
| W-03 | qc2 | PATCH can clear world_id (downgrade vector) | Added `WORLD_CLEAR_FORBIDDEN` 422 guard for V1.40+ novel Works |
| W-04 | qc2 | Insufficient adversarial test coverage | Added 7-value adversarial matrix test |
| W-2 | qc3 | POST handler lacks world_id existence validation | Added existence + ownership check before insert |

## QA
- @qa-engineer verdict: **Pass** (all 6 ACs green; HTTP 422 verified; atomicity verified; ownership FK verified; clippy green; nightly fmt exit 0)
- No new Critical/Warning surfaced by QA.

## Consolidated gate verdict
**Approve — proceed to merge `feature/v1.40-world-create-and-validation` → `iteration/v1.40`.**

## Residual findings (open)
None opened in this round — all blocking findings resolved and consensus. Suggestions from QC #1 (3), QC #2 (5), QC #3 (2) are non-blocking; recorded below for backlog hygiene if desired.

| ID | Severity | Title | Source | Owner | Target |
| --- | --- | --- | --- | --- | --- |
| R-V140P0-S1 | nit | 400 vs 422 documentation deviation (OpenAPI / handler docstring) | qc2 S-01 | @fullstack-dev | backlog |
| R-V140P0-S2 | nit | Consider adding `--force` unbind path for V1.41 (currently no escape hatch for world_id clear) | qc2 S-03 | @fullstack-dev | V1.41 |
| R-V140P0-S3 | low | sqlx offline metadata churn — verify CI re-prepare | qc3 S-1 | @ops-engineer | backlog |
| R-V140P0-S4 | low | Add tracing span around new mandatory binding checks for observability | qc3 S-2 | @fullstack-dev | V1.40 hardening |

## Acceptance criteria evidence
- AC1: `creator world create` + `list` + `show` all present (`crates/nexus42/src/commands/creator/world.rs`) ✓
- AC2: `create_world_tx` inside scaffold tx; FS scaffold rollback on DB failure ✓
- AC3: 422 + remediation `Run 'nexus42 creator world create --title ...' or 'nexus42 creator world list'` for invalid `world_id` ✓
- AC4: `world_refs` outline warning + finalize error + `--force` override tested ✓
- AC5: Mandatory binding enforced at create/init; legacy V1.39 worldless read path preserved (`is_world_bound: false` warn-only) ✓
- AC6: 65+ hermetic tests pass across 4 crates (scoped filtered: `world world_id world_refs findings`) ✓
- HTTP 422 verified in `crates/nexus-daemon-runtime/src/api/errors.rs`
- Atomicity verified in `crates/nexus-local-db/src/narrative_write.rs` + `crates/nexus-orchestration/src/capability/builtins/novel_scaffold.rs`
- Ownership FK verified by `create_work_with_other_creators_world_id_returns_error`
- PATCH clear rejection verified by `patch_work_with_world_id_clear_returns_forbidden`

## Notes for PM
- Merge target: `iteration/v1.40`.
- After merge: HEAD should include all commits `a903efd8..HEAD` + the spec amendment `464d0fba`.
- Status update: set plan `2026-06-10-v1.40-world-create-and-validation` to `Done`; register `R-V140P0-S1..S4` (suggestions) in root `residual_findings`.
- Note: P2 and P3 plans reference worldless behavior; spec amendment marked them for re-review post-P0 ship. PM will re-evaluate those plan scopes when P2/P3 implement is dispatched.