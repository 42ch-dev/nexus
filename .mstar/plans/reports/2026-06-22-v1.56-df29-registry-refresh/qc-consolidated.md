---
report_kind: qc-consolidated
consolidated_by: "@project-manager"
plan_id: "2026-06-22-v1.56-df29-registry-refresh"
compiled_at: "2026-06-22"
---

# QC Consolidated Report — V1.56 P1 (DF-29 `registry.refresh`)

## Reviewer Verdicts

| Reviewer | Focus | Verdict | Findings (C/H/M/L) | Report |
|----------|-------|---------|-------------------|--------|
| qc-specialist (R#1) | Architecture coherence & maintainability | Approve with comments | 0/0/3/6 | `qc1.md` |
| qc-specialist-2 (R#2) | Security & correctness | **Request Changes** | **1**/2/0/4 | `qc2.md` |
| qc-specialist-3 (R#3) | Performance & reliability | Approve with comments | 0/0/2/4 | `qc3.md` |

**Aggregated**: **1 Critical** / **2 High** / 5 Medium / 14 Low across 3 reviews. **qc2 Request Changes blocks merge to mid-QA.**

## Blocking Findings (must fix before mid-QA)

### qc2 — Critical
- **C-001 — CDN URL injection / SSRF**: `--cdn-url` accepted verbatim; `fetch_from_cdn` uses `reqwest::Client::builder().timeout(...).build()` then `client.get(&cdn.url)` with **no scheme guard, no HTTPS enforcement, no redirect policy, no private-IP/localhost/metadata block, no body size limit**. This is exactly Risk #3 in the plan stub — called out pre-implementation, not mitigated.

### qc2 — High
- **H-001 — Network errors are stringly-typed**: no domain errors for timeout vs. 5xx vs. parse; loses retry-classification information needed for the documented fallback policy.
- **H-002 — No CLI/boot validation of `--cdn-url`**: empty, whitespace, `http://`, `localhost`, private ranges all accepted; same SSRF surface extends to daemon-start flag parsing.

## Combined Findings (Medium-severity, register as residuals after fix-wave closes)

| ID | Reviewer | Title |
|----|----------|-------|
| M-001 | qc1 F-101 | Breaking output schema rename (`agent_count` → `capability_count`) — wire-contract field rename; verify downstream consumers |
| M-002 | qc1 F-102 | Global mutable state pattern (RwLock-static for `CdnConfig`) — recommend constructor-injected for testability |
| M-003 | qc1 F-103 | `force` parameter ignored in host handler |
| M-004 | qc3 F-001 | No tracing instrumentation in capability handler |
| M-005 | qc3 F-002 | `reqwest::Client` per invocation, no connection reuse |

Low-severity (S-001..S-014): help text lacks security warning, snapshot version is not content hash, deterministic retry backoff without jitter, no latency benchmark, `generated_at` prevents full hash determinism, no structured metrics, etc. — deferrable.

## PM Gate Verdict

**REQUEST CHANGES** — V1.56 P1 implementation **NOT accepted as-is**. qc2 identified Critical SSRF surface + 2 High URL validation/typed-errors issues. Per mstar-review-qc verdict rules, an unresolved Critical requires `Request Changes`.

## Fix-Wave Outcome (2026-06-22)

**Fix-wave dispatched** to `@fullstack-dev-2`. Implementation: `b887ce57` + merge `27bc1b09`.

**Targeted re-review** by `@qc-specialist-2` only (N=1) → **Approve** (see `qc2-revalidation.md`).

| Blocking finding | Closure status |
|---|---|
| C-001 SSRF | ✅ closed (HTTPS-only + redirect policy `limited(0)` + private-IP block + 8 MiB body cap + post-DNS enforcement; 6+ negative tests) |
| H-001 typed errors | ✅ closed (`CdnError` enum with 11 variants; `Display`+`Error` impls; `fetch_from_cdn` returns `Result<_, CdnError>`; `fallback_reason` = typed) |
| H-002 URL validation | ✅ closed (early `validate_cdn_url_static` in `daemon/mod.rs` (foreground + background), `daemon_run.rs`, `boot.rs`; rejects empty/whitespace/non-HTTPS/private-IP at parse) |

**Re-review introduced**: 3 cosmetic Suggestions (body-size hammer test, redirect injection test, naming prefix consistency) — register as low residuals, not fix-wave blockers.

## Updated PM Gate Verdict (post-fix-wave)

**APPROVE** — V1.56 P1 implementation now accepted. All blocking findings closed. Medium findings + re-review Suggestions deferred as residuals. Ready for mid-QA dispatch.

## Action Items (in order)

1. ~~PM dispatches P1 fix-wave~~ ✅ done (commits `b887ce57` + `27bc1b09`)
2. ~~Targeted re-review by qc-specialist-2 only~~ ✅ done (`qc2-revalidation.md` Approve)
3. Register 3 re-review Suggestions as low residuals in `status.json`.
4. Dispatch **mid-QA** for P1 (verify AC mapping + 7-key gate against `iteration/v1.56` HEAD `27bc1b09`).
5. After mid-QA Pass: mark P1 plan status as `Done`.

## Handoff

- P1 implementer `@fullstack-dev-2` enters fix-wave mode.
- Wave 1 acceptance now gated on P1 fix-wave + re-review pass + mid-QA pass.

## Git

- Working branch: `iteration/v1.56`
- Reviewed range: `a264c383..d3a03e06`
- QC report commits: `b53976d4` (qc1), `60912809` (qc2), `45f54bdd` (qc3) — review-only
- P1 implementation commit: `d3a03e06` (feature) — will get fix-wave follow-up commits before mid-QA