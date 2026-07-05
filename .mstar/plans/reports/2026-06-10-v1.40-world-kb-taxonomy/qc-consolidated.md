---
plan_id: 2026-06-10-v1.40-world-kb-taxonomy
verdict: Approve
generated_at: 2026-06-10
---

# Code Review Consolidated — P1 world-kb-taxonomy

## Plan
- **plan_id**: `2026-06-10-v1.40-world-kb-taxonomy` (P1)
- **Working branch**: `feature/v1.40-world-kb-taxonomy` (HEAD `2f5cc6c3`)
- **Review range / Diff basis**: `iteration/v1.40..feature/v1.40-world-kb-taxonomy`
- **Iteration compass**: `.mstar/iterations/v1.40-novel-world-kb-delivery-compass-v1.md` (DF-63 W2)
- **Primary spec**: `.mstar/knowledge/specs/entity-scope-model.md` §5.1.1 (Shipped V1.40 P1)

## Reviewer verdicts
| Reviewer | Lens | Verdict (initial) | Verdict (re-validation) |
| --- | --- | --- | --- |
| @qc-specialist | architecture coherence / maintainability | Request Changes (1C,2W,4S) | **Approve** (0C,0W,4S) |
| @qc-specialist-2 | security / correctness | Request Changes (1C,3W,4S) | **Approve** (0C,0W,0S) |
| @qc-specialist-3 | performance / reliability | **Approve** (0C,0W,3S) | n/a — no re-review needed |

## Blocking findings (initial round) → all resolved in `fbd301c4` + 6 fix commits
| ID | Source | Title | Fix |
| --- | --- | --- | --- |
| C-001 | qc1 / qc2 C1 | `SqliteKbStore` has zero body validation; production unprotected | Added `validation_mode: ValidationMode` to `SqliteKbStore`; `with_validation_mode(pool, mode)` constructor; `insert_key_block` + `update_key_block` call `validate_body` + `validate_canonical_name`; `kb_extract_work.rs` parses structured body JSON |
| W-001 | qc1 / qc2 W1 | Advisory `novel_category → block_type` mapping was dead code | Added `tracing` workspace dep to `nexus-kb`; implemented `default_block_type_for_category()`; `tracing::warn!` on mismatch |
| W-002 | qc1 | `block_type` stored via `Debug` format (fragile) | Replaced with `serde_json::to_string(&kb.block_type)` (stable snake_case); `parse_block_type` accepts both snake_case (new) and PascalCase (legacy) for backward compat |
| W2 | qc2 | No `canonical_name` format validation | Added `validate_canonical_name` rejecting empty, control chars, path separators, shell metacharacters, >256 chars; applied to both `InMemoryKbStore` and `SqliteKbStore`; grammar documented in `entity-scope-model.md` §5.1.1 |
| W3 | qc2 | Validation errors were opaque `String` | Introduced `ValidationKind` enum (7 variants) + `ValidationError { kind, field, message }`; `KbStoreError::Validation(ValidationError)` for structured pattern-matching; legacy `String` variant retained for backward compat |

## QA
- @qa-engineer verdict: **Pass** (all 5 ACs green; 27 filtered AC tests + 21 `kb_store` tests pass; legacy PascalCase roundtrip ok; clippy clean; nightly fmt clean; spec status consistent)
- No new Critical/Warning surfaced by QA.

## Consolidated gate verdict
**Approve — proceed to merge `feature/v1.40-world-kb-taxonomy` → `iteration/v1.40`.**

## Residual findings (open)
| ID | Severity | Title | Source | Owner | Target |
| --- | --- | --- | --- | --- | --- |
| R-V140P1-S1 | low | `NOVEL_CATEGORIES` list duplicated between Rust code and `kb-extract/prompts/extract.md` — drift risk | qc1 S-001 / qc2 S3 | @fullstack-dev | V1.40 hardening |
| R-V140P1-S2 | low | `test_invalid_block_type_via_deserialization` test name misleading (tests serde, not store path) | qc1 S-004 | @fullstack-dev | backlog |
| R-V140P1-S3 | low | No concurrent-uniqueness race test under Novel mode | qc2 S1 | @fullstack-dev | V1.41 |
| R-V140P1-S4 | low | `local-db-schema.md` missing direct pointer to `nexus-kb::validation` from schema doc | qc2 S4 | @fullstack-dev | V1.40 hardening |
| R-V140P1-S5 | low | String allocations in validation error paths (acceptable for low-frequency inserts) | qc3 S-2 | @fullstack-dev | backlog |
| R-V140P1-S6 | low | No benchmarks for validation path | qc3 S-3 | @fullstack-dev | backlog |

## Acceptance criteria evidence
- AC1: 5 `block_type` tests pass (invalid `block_type` rejected at deserialization)
- AC2: 16 `novel` tests pass (per-block-type happy + missing-category rejection)
- AC3: 4 `uniqueness` tests pass
- AC4: 1 `world_refs` test pass (resolve by `canonical_name`)
- AC5: 1 `kb_extract` test pass (sample extract output passes validation)
- SqliteKbStore validation: 21 tests pass (valid novel + missing/invalid `novel_category` rejection + canonical_name rejection + PascalCase legacy roundtrip)
- Backward compat: `parse_block_type` accepts both snake_case (new) and PascalCase (legacy)

## Notes for PM
- Merge target: `iteration/v1.40`.
- After merge: HEAD should include all P1 commits.
- Status update: set plan `2026-06-10-v1.40-world-kb-taxonomy` to `Done`; register `R-V140P1-S1..S6` (low suggestions) in root `residual_findings`.