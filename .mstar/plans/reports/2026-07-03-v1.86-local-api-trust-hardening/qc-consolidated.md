---
report_kind: qc-consolidated
plan_id: 2026-07-03-v1.86-local-api-trust-hardening
iteration: V1.86
verdict: Approve
reviewers: [qc1, qc2, qc3]
consolidated_at: 2026-07-03
generated_at: 2026-07-03T13:30:00+08:00
---

# QC Consolidated — V1.86 Local API Trust-Boundary Hardening

## Tri-review result

| Reviewer | Agent | Lens | Verdict | Critical | Warning | Suggestion | Report | Commit |
|----------|-------|------|---------|----------|---------|------------|--------|--------|
| qc1 | qc-specialist | Architecture / maintainability | **Approve** | 0 | 0 | 5 | `reports/2026-07-03-v1.86-local-api-trust-hardening/qc1.md` | `0478c54b` |
| qc2 | qc-specialist-2 | **Security / correctness (deep; Security + Auth lens)** | **Approve** | 0 | 0 | 2 | `…/qc2.md` | `653d2408` |
| qc3 | qc-specialist-3 | Performance / reliability | **Approve** | 0 | 0 | 2 | `…/qc3.md` | `e3d5e0fe` |

**Consolidated verdict: Approve (3/3, 0 Critical / 0 Warning, clean).**

## Attack-path closure (qc2 deep review — the lead lens)

| # | Attack path (pre-V1.86) | Status | Evidence |
|---|--------------------------|--------|----------|
| 1 | Permissive CORS + keyless-localhost → remote reach from any website | **CLOSED** | T1 `160fa486`: Origin-allowlist `CorsLayer` + `require_allowed_origin` middleware. qc2 probed null/spoof/malformed Origin, OPTIONS state-change, allowlist drift — no bypass. |
| 2 | fs/* bypass when no workspace → arbitrary file R/W | **CLOSED** | T2 `71c97e9d`: unconditional deny in `admission_pipeline` + execute-side self-defense. qc2 traced all 3 `HostToolExecutor` caller entry points — none reach `execute_*` without the deny. |
| 3 | String-prefix path comparison → sibling-dir escape | **CLOSED** | T3 `9b56079d`: delegates to `resolve_guarded_path` (component-wise `Path::starts_with`). qc2 verified sibling-prefix + `..` + symlink escapes rejected on both read/write branches. |

**No bypass found** in any of the three declared attack paths. The regression tests reproduce the pre-fix attack paths (verified by qc2 to assert the security property, not just status codes).

## Implementation summary reviewed

P0 (T1-T3, security-urgent) + P1 same-class sweep (T4 coverage backfill `0eb9aa4f`; T5 `spawn_blocking` + async conversion `42335a16`; T7 TOCTOU note refresh `ec54e15a`; T6/T8 verified prior resolutions hold, no-op). Integration merge `b2cdcfd6`. Two regression-of-resolution residuals recorded earlier (`R-V186-REGRESS-M004`, `R-V186-REGRESS-W001`) — both resolved by this iteration's commits.

Verification (all three reviewers): `cargo test -p nexus-daemon-runtime` 387 lib + integration green; `cargo clippy -p nexus-daemon-runtime -- -D warnings` clean; `cargo +nightly-2026-06-26 fmt --all --check` clean; desktop crate 18 tests green.

## Residuals recorded from this QC round (PM → status.json)

| ID | Severity | Source | Scope | Decision | Target |
|----|----------|--------|-------|----------|--------|
| `R-V186-QC1-S005` | medium | qc1 S-005 | `host_tool_handlers.rs:2155-2156` manuscript body read still uses string-prefix `starts_with` (same class as Finding 3; `nexus.manuscript.body.read` tool, out of V1.86 fs/* scope per §13). Latent local/agent-exploitable path-traversal; remote vector closed by T1 so urgency reduced. PM elevates to medium on security-class grounds (qc1 flagged as maintainability suggestion under architecture lens). | defer to fast-follow | next iteration (or hotfix) |
| `R-V186-QC3-PERF-DOUBLE-RESOLVE` | low | qc3 F-1 | `resolve_guarded_path_async` runs twice per fs/* call (admission `must_exist=false` + execute `must_exist=true`); negligible for local single-user. | accept/defer | future perf iteration |
| `R-V186-QC3-PERF-ARC-CONFIG` | low | qc3 F-2 | `DaemonApiConfig` cloned per-request by axum `State`; `Arc<…>` wrapper would zero-cost. Clone sites at router construction are one-time (confirmed), so impact is small. | accept/defer | future perf iteration |

**Not tracked (accepted suggestions, documented here, not promoted to residuals):** qc1 S-001 (allowlist divergence helper), S-002 (double-resolve doc note — subsumed by R-V186-QC3-PERF-DOUBLE-RESOLVE), S-003 (workspace_path helper), S-004 (port resolution dedup); qc2's 2 minor suggestions (null-Origin literal test case, startup logging polish). These are clean-up refactors with no behavior/security impact; revisit opportunistically.

## Gate decision

0 Critical / 0 Warning across all three reviewers → **Approve**. Plan proceeds to QA verification (reproduce the three attack paths failing-without-fix), then `Done`, then Phase 3 iteration-close.

## Scope alignment note

All three reviewers verified cwd = repo root, branch = `iteration/v1.86`, Review range `merge-base(main, iteration/v1.86)..iteration/v1.86` reproduced identically. No subagent dispatched by any reviewer (leaf-executor discipline observed — unlike the Phase 1 product-manager recursive-dispatch deviation, which did not recur).
