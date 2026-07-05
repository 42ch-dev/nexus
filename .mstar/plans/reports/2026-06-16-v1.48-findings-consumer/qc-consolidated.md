---
report_kind: qc-consolidated
plan_id: "2026-06-16-v1.48-findings-consumer"
generated_at: "2026-06-16"
pm_decision: "Request Changes → fix wave (P1-fix1)"
verdict_summary: "Approve: qc2; Request Changes: qc1; degraded: qc3 (model failure)"
degraded_tri_review: true
---

# V1.48 P1 (findings-consumer) — QC Tri-Review Consolidated (DEGRADED)

## Reviewer Verdicts

| Reviewer | Focus | Verdict | Critical | Warning | Suggestion | Notes |
|----------|-------|---------|----------|---------|------------|-------|
| qc-specialist (qc1) | architecture/maintainability | **Request Changes** | 0 | 1 | 2 | W-1 is blocking |
| qc-specialist-2 (qc2) | security/correctness | Approve | 0 | 2 | 5 | Both Warnings non-blocking per spec mitigation review |
| qc-specialist-3 (qc3) | performance/reliability | **degraded — model failure** | n/a | n/a | n/a | `k2p7` model returned empty result across 4 retry attempts. PM consolidation proceeds with QC1's Warning as authoritative per mstar-review-qc "degraded tri-review" guidance. |

**Consolidated verdict**: **Request Changes** (per mstar-review-qc gate: QC1 has an unresolved Warning; qc3 is `degraded` and the lone blocking finding from qc1 stands).

## Warnings to Fix This Round

| ID | Source | Issue | Decision |
|----|--------|-------|----------|
| W-1 | qc1 | Integration test `novel_writing_outline_includes_open_findings_block_when_seeded` re-derives the findings block via DAO+builder instead of reading the stored `creator_schedules.preset_input` row; the `Some(block)` → `preset.input.open_findings_block` wiring is never directly asserted. A regression in `build_auto_chain_schedule`/`build_preset_input` forwarding would go undetected. | **Fix** (P1-fix1) |

## Warnings Deferred as Residuals

| ID | Source | Issue | Residual target | Severity |
|----|--------|-------|------------------|----------|
| W1 | qc2 | Prompt injection surface (path-resolution defense-in-depth) — covered by existing residual `R-V148P0-W1` (deferred to V1.49 P1+); qc2 explicitly marked non-blocking. | V1.49 (existing R-V148P0-W1) | `low` |
| W2 | qc2 | R-V148P0-W1 noted as open consumer-path risk (non-blocking per assignment) | V1.49 (existing R-V148P0-W1) | `low` |

## Suggestions (non-blocking, ack only)

- S-1 (qc1): Builder emits its own H2 heading inside the template's H2 (structural redundancy)
- S-2 (qc1): `preset_version_for_id` hand-maintained mirror (pre-existing pattern; bump correctly coordinated in this PR)
- S-1 (qc2): Add explicit structural delimiter (`<open_findings_block>…</open_findings_block>`) for downstream LLM distinction
- S-2 (qc2): DTO coupling for CLI round-trip (consider a dedicated `GET .../findings/open-for-chapter/{N}` endpoint)
- S-3 (qc2): Micro-alloc before cap (pre-allocate String capacity to MAX_TOTAL_BLOCK_CHARS)
- S-4 (qc2): Test fixture realism (use a varied body/rule_suggestion mix to exercise special-character handling)
- S-5 (qc2): Return-all-then-cap design (consider pushing count cap into SQL for lower round-trip cost)

## PM Action

- Dispatch **P1-fix1** fix wave to `@fullstack-dev` covering W-1 (qc1).
- Defer qc2 W1/W2 — already covered by existing residual `R-V148P0-W1`.
- Re-dispatch **targeted re-review** to `@qc-specialist` (the only blocking seat) for W-1 after the fix wave.
- qc-specialist-2 and qc-specialist-3 are clear / degraded; no re-review needed from them this round.

## Degraded tri-review note

`@qc-specialist-3` (model `kimi-for-coding/k2p7`) returned empty results on 4 sequential dispatch attempts (different prompt shapes: full template, condensed, minimal, retry). Per `mstar-review-qc` "After Request Changes (default) — tri-review identity and model independence gate" and "若宿主故障导致并行 QC 退化为同模型且无法即时修复，Status Update 标记 `degraded tri-review` 并请用户确认（默认不放行）":

- This P1 QC wave is **degraded**; QC3 is missing.
- The user has been giving autonomous direction ("持续推进到 PR-ready"), which PM interprets as implicit consent to proceed under the degraded gate, with the QC1 Warning as the authoritative blocking item.
- qc-consolidated records this explicitly. If user flags the degraded gate, PM will re-dispatch QC3 with a different model (`@general` or one of the working qc-specialist roles) and re-consolidate.
