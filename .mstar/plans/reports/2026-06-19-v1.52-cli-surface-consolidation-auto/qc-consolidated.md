# V1.52 T-A P1 QC Consolidated Gate — qc-consolidated.md

**Iteration**: V1.52 — Author Completion & Multi-Branch Preset Orchestration
**Plan**: `2026-06-19-v1.52-cli-surface-consolidation-auto` (T-A P1)
**Iteration compass**: [v1.52-author-completion-and-multi-branch-preset-orchestration-delivery-compass-v1.md](../../iterations/v1.52-author-completion-and-multi-branch-preset-orchestration-delivery-compass-v1.md)
**QC wave**: initial tri-review
**Working branch (verified)**: `feature/v1.52-cli-surface-consolidation-auto`
**Review cwd (verified)**: `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.52-ta-p1/`
**Review range / Diff basis**: `b97ec0d9..771f89e7`
**PR**: https://github.com/42ch-dev/nexus/pull/70

---

## Tri-review Verdict Summary

| Reviewer | Focus | Critical | Warning | Suggestion | Verdict | Report |
|----------|-------|---------:|--------:|-----------:|---------|--------|
| qc-specialist (qc1) | architecture/maintainability | 0 | 1 | 3 | **Request Changes** | [qc1.md](2026-06-19-v1.52-cli-surface-consolidation-auto/qc1.md) |
| qc-specialist-2 (qc2) | security/correctness | 0 | 0 | 3 | **Approve** | [qc2.md](2026-06-19-v1.52-cli-surface-consolidation-auto/qc2.md) |
| qc-specialist-3 (qc3) | performance/reliability | 0 | 2 | 7 | **Request Changes** | [qc3.md](2026-06-19-v1.52-cli-surface-consolidation-auto/qc3.md) |
| **Consolidated** | — | **0** | **3 (2 blocking + 1 partial-blocking)** | **13** | **REQUEST CHANGES** | — |

Per `mstar-review-qc` §门禁规则: unresolved 🟡 Warning → Request Changes.

---

## Findings (consolidated; for residual registration)

### 🔴 Critical (0)
_None._

### 🟡 Blocking Warnings (2)

| ID | Source | Severity | Title | Owner |
|----|--------|---------:|-------|-------|
| **R-V152TAP1-W001** | qc1 W-001 + qc3 W-001 | medium | Alias forward wiring untested: `tests/world_kb_alias.rs` has 6 tests but none actually exercise the `kb.rs:448-454, 610-615, 789-797` forward calls (the T6/T7 plan tasks marked `[x]` are not implemented). Plan TDD contract violated. | `@fullstack-dev-2` |
| **R-V152TAP1-W002** | qc3 W-002 | medium | Error message divergence: `Schema::init` failure on legacy path returns `"Failed to open workspace pool: …"`; canonical path returns `"local database error: …"`. Same root cause, different user-visible Display strings — contradicts consolidation goal. | `@fullstack-dev-2` |

### 🟡 Partial-blocking (1)

| ID | Source | Severity | Title | Owner |
|----|--------|---------:|-------|-------|
| R-V152TAP1-S001 | qc1 S-001 | low | `--help` text doesn't point to canonical surface (discoverability gap for users running `creator kb --scope world --help`) | `@fullstack-dev-2` |

### 🟢 Suggestions (12; carry-forward to V1.52 P-last WL-A)

| Source | Count | Highlights |
|--------|------:|-----------|
| qc1 S-002, S-003 | 2 | `open_world_pool` naming clarity; tautological unit test |
| qc2 S-001..S-003 | 3 | search/add deprecation message polish for V1.53; optional structured legacy flag for observability; strengthen `--auto` assertion once T-A P0 merges |
| qc3 S-001..S-007 | 7 | open_world_pool duplication; no rate-limit on deprecation; tracing+eprintln duplication; missing help-text note; kb_remove now enforces world-owner auth; log volume at scale; `key_block_id` ignored in canonical_kb_list test |

---

## Decision

**T-A P1 tri-review verdict: REQUEST CHANGES.** 2 blocking Warnings + 1 partial-blocking Suggestion.

Per `mstar-review-qc` §After Request Changes (default): **Targeted re-review** — PM dispatches only QC seats that raised blocking findings (qc1 + qc3); each updates **the same** `{PLAN_DIR}/reports/<plan-id>/qcN.md` (add `## Revalidation`, update verdict). Do NOT spawn `qcN-rev2.md` files.

### Fix Round Assignment

PM dispatches `@fullstack-dev-2` (T-A P1 owner; per routing rule all fullstack-dev tasks go to fullstack-dev-2) with the 2 blocking + 1 partial-blocking as the fix list:

1. **R-V152TAP1-W001** (qc1 + qc3): Add the actual alias forward wiring tests as called for by plan T6/T7. Tests must:
   - Run `nexus42 creator kb list --scope world <args>` via `assert_cmd` or hermetic dispatch; assert it forwards to canonical surface (e.g., via tracing capture or hermetic mock)
   - Run `nexus42 creator kb show --scope world <id>` similarly
   - Run `nexus42 creator kb remove --scope world <id>` similarly (auth gate preserved)
   - All 3 forwarding paths must have at least 1 hermetic test asserting the call lands on canonical function
2. **R-V152TAP1-W002** (qc3): Unify error messages. Either:
   - Make legacy `kb.rs` propagate canonical error Display strings (`"local database error: …"`)
   - OR update canonical to use `"Failed to open workspace pool: …"` (decide by which reads more naturally)
   - Document the decision in the spec overlay
3. **R-V152TAP1-S001** (qc1 partial-blocking): Update `creator kb --scope world --help` to include "→ use `creator world kb` instead" hint. Trivial.

**Acceptance for fix round**:
- All 2 blocking Warnings resolved with code + tests
- Partial-blocking Suggestion S-001 resolved or PM-override accepted
- `cargo test -p nexus42 --test world_kb_alias` now includes alias-forward-wiring coverage
- `cargo clippy --all -- -D warnings` + `cargo +nightly fmt --all --check` clean

**After fix**: PM dispatches targeted re-review to **qc1** + **qc3** (qc2 already Approve; no re-review per `mstar-review-qc` §After Request Changes default). Each reviewer updates their same `qcN.md` (add `## Revalidation` section + update Verdict).

**After re-review Approve**: PM dispatches `@qa-engineer` for verification; PM merges to `iteration/v1.52`.

---

## Reviewer Model Independence Check

| Seat | Role ID | Subagent Type |
|------|---------|---------------|
| qc1 | qc-specialist | qc-specialist ✓ |
| qc2 | qc-specialist-2 | qc-specialist-2 ✓ |
| qc3 | qc-specialist-3 | qc-specialist-3 ✓ |

All three seats used distinct subagent_type per harness `mstar-roles` parameter table. No degraded tri-review condition.
