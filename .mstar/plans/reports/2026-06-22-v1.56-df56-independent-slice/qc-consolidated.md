---
report_kind: qc-consolidated
consolidated_by: "@project-manager"
plan_id: "2026-06-22-v1.56-df56-independent-slice"
compiled_at: "2026-06-22"
---

# QC Consolidated Report — V1.56 P2 (DF-56 Independent Slice)

## Reviewer Verdicts

| Reviewer | Focus | Verdict | Findings (C/H/M/L) | Report |
|----------|-------|---------|-------------------|--------|
| qc-specialist (R#1) | Architecture coherence & maintainability | **Request Changes** | 0/2/1/2 | `qc1.md` |
| qc-specialist-2 (R#2) | Security & correctness | **Request Changes** | 0/3/2/4 | `qc2.md` |
| qc-specialist-3 (R#3) | Performance & reliability | **Request Changes** | 2/0/6/5 | `qc3.md` |

**Aggregated**: 2 Critical / 5 High / 9 Medium / 11 Low across 3 reviews. **All Request Changes.**

## Blocking Findings (must fix before mid-QA)

### Critical
- **H-001 (qc3, performance)** — Merge-point runtime missing. `ConvergeConfig`/`ConvergeStrategy` wire types defined + 37 expression tests pass, but **runtime enforcement NOT wired** into `StateCompositeTask` or `resolve_expression_target`. AC gap: plan promises "merge points accept multiple incoming edges; wait-for-all / first-arrival semantics are configurable and tested" — not met. This is a **scope gap**, not just a bug — implementer declared the surface but not the runtime.
- **H-002 (qc3, performance)** — Throttled `llm_judge` + Conditional/Branches next causes `TaskExecutionFailed`. The min_interval throttle path calls `resolve_labeled_target()` which explicitly rejects `Conditional`/`Branches` variants. **Hard runtime failure for valid preset configs.**

### High
- **W-001 (qc1, qc2)** — Stale integration test `reject_conditional_next_not_yet_supported` in `tests/preset_validation.rs:107` expects old `ConditionalNotYetSupported` rejection. P2 intentionally accepts conditional next on any state. **CI failure** blocks gate.
- **W-002 (qc1, qc2)** — `ConvergeConfig` types defined and parseable but **runtime enforcement missing** (same as H-001 from qc3 — independent corroboration).
- **W-003 (qc2)** — Expression parser/evaluator has no recursion depth limit — user-installable presets can supply deeply nested `when:` expressions causing stack overflow (availability / reliability risk).

## Combined Findings (Medium, register as residuals after fix-wave closes)

| ID | Reviewer | Title |
|----|----------|-------|
| M-001 | qc1 W-003 | Null comparison semantics differ spec ("comparison with null is always false except `!= null`") vs impl (standard JSON equality where `null == null` → `true`) — needs spec alignment |
| M-002 | qc2 | Missing converge fan-in validation (loader doesn't check `≥2 incoming edges`) |
| M-003 | qc2 | Overly narrow `build_context_json` whitelist (fixed key set; user-set context values not exposed) |
| M-004 | qc3 | Expression AST re-parsed every transition (no caching) |
| M-005 | qc3 | No integration tests for `resolve_expression_target` (only unit tests for parser/evaluator) |
| M-006 | qc3 | Silent expression eval failures (errors swallowed, no propagate) |

Low (deferred backlog): S-001..S-011 from 3 reviewers.

## PM Gate Verdict

**REQUEST CHANGES** — V1.56 P2 implementation **NOT accepted as-is**. Multiple critical scope gaps + runtime failures:
- **Scope gap**: converge runtime not wired (declared surface only)
- **Runtime bug**: throttle path rejects valid Conditional/Branches
- **CI failure**: stale test not updated
- **DoS risk**: parser depth unbounded

Per mstar-review-qc, 2 Critical + 3 High require `Request Changes`.

## Action Items (in order)

1. **PM dispatches P2 fix-wave** to `@fullstack-dev` (P2 implementer):
   - **Wire converge runtime** (H-001 / W-002): add merge-point arrival tracking in `StateCompositeTask` + `resolve_expression_target`; implement `wait_for_all` (default), `first_completed` (cancel others), `any` (idempotent). Use `tokio::sync::Notify` or `JoinSet`. Integration tests for 2-way, 3-way, error path, first_completed cancel, any idempotent.
   - **Fix throttle bug** (H-002): `resolve_labeled_target` in throttle path must delegate to `resolve_expression_target` (or return `Continue`) for Conditional/Branches variants.
   - **Update stale test** (W-001): `tests/preset_validation.rs:107` should expect Conditional acceptance on any state kind, not rejection.
   - **Add parser depth limit** (W-003): max recursion depth (e.g. 32) with typed error on overflow.
   - **Align null comparison semantics** (M-001): PM decision needed (spec says "null is always false except `!= null`"; JSON equality says `null == null` is true). Recommend following JSON semantics for usability; update spec accordingly.
2. After fix-wave complete: **targeted re-review** by `qc-specialist-2` (security — covers converge correctness + parser depth + null semantics) + `qc-specialist-3` (performance — covers runtime enforcement + throttle fix) = 2 seats re-review (or full tri if PM prefers).
3. If re-review `Approve`: dispatch mid-QA for P2.
4. After mid-QA Pass: mark P2 plan status as `Done`.
5. If re-review still Critical/High: re-dispatch fix-wave.

## Handoff

- P2 implementer `@fullstack-dev` enters fix-wave mode.
- Wave 2 acceptance gated on P2 fix-wave + re-review + mid-QA.
- Wave 3 (P3) still blocked on Wave 2 closure.

## Git

- Working branch: `iteration/v1.56`
- Reviewed range: `a457a8ee..4da874db`
- QC report commits: `df8a5204` (qc1), `3c42fae9` (qc2), `ff7829b4` (qc3) — review-only
- P2 implementation commits: `ee678812` (feature) + `4da874db` (merge) — will get fix-wave follow-up before mid-QA