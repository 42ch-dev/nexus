---
report_kind: qc-consolidated
plan_id: "2026-06-16-v1.48-findings-producer"
generated_at: "2026-06-16"
pm_decision: "Request Changes → fix wave (P0-fix1)"
verdict_summary: "Approve: qc1, qc2; Request Changes: qc3"
---

# V1.48 P0 (findings-producer) — QC Tri-Review Consolidated

## Reviewer Verdicts

| Reviewer | Focus | Verdict | Critical | Warning | Suggestion |
|----------|-------|---------|----------|---------|------------|
| qc-specialist (qc1) | architecture/maintainability | Approve | 0 | 0 | 2 |
| qc-specialist-2 (qc2) | security/correctness | Approve | 0 | 1 | 6 |
| qc-specialist-3 (qc3) | performance/reliability | **Request Changes** | 0 | 3 | 2 |

**Consolidated verdict**: **Request Changes** (per mstar-review-qc gate: unresolved Warnings).

## Warnings to Fix This Round

| ID | Source | Issue | Decision |
|----|--------|-------|----------|
| W-1 | qc3 | `load_and_parse_review_report` reads unbounded `review-report.md` into memory (`std::fs::read_to_string`); no size cap | **Fix** (P0-fix1) |
| W-2 | qc3 | `persist_parsed_findings` issues N sequential `INSERT` round-trips for parsed findings; no transaction | **Fix** (P0-fix1) |
| W-3 | qc3 | Fallback `tracing::warn!` spans in `try_persist_parsed_findings` omit required `chapter` field per `archived/knowledge/novel-findings-maturity.md` §1.3 | **Fix** (P0-fix1) |

## Warnings Deferred as Residuals

| ID | Source | Issue | Residual target | Severity |
|----|--------|-------|------------------|----------|
| W-1 | qc2 | `load_and_parse_review_report` path resolution is app-controlled today; recommend safe-resolve guard or invariant test before P1/P2 prompt injection | V1.49 (P1+ injection touch point) | `low` |

## Suggestions (non-blocking, ack only)

- S-1 (qc1): Vocabulary lockstep between `review_report::KNOWN_FINDING_KINDS` and `FindingKind::ALL_STRS` — consider cross-crate test guard
- S-2 (qc1): Parser recognizes `-`/`*` bullets only (matches preset contract; robustness note only)
- S-1 (qc2): positive findings on sanitization, idempotency, AtomicU32, enum expansion, warn! choice, pure-parser design; rule_suggestion/body injection is intentional for quality loop and must be documented at consumer boundary in P1+
- S-2 (qc3): `RVM_COUNTER` AtomicU32 wrap is theoretically reachable in long-lived daemon (implausible in practice); consider 64-bit or ULID
- S-3 (qc3): Avoid `exists()` + `read_to_string()` double syscall — single `read_to_string` distinguishes "missing" from read errors

## PM Action

- Dispatch **P0-fix1** fix wave to `@fullstack-dev` covering W-1, W-2, W-3 (qc3-blocking).
- Defer W-1 (qc2) as **low** residual under `residual_findings["2026-06-16-v1.48-findings-producer"]` to be addressed in P1 (which is the prompt-injection consumer that surfaces `rule_suggestion`/`body`).
- Re-dispatch **targeted re-review** to `@qc-specialist-3` (the only blocking seat) for W-1, W-2, W-3 after the fix wave.
- qc-specialist and qc-specialist-2 are clear; no re-review needed from them this round.
