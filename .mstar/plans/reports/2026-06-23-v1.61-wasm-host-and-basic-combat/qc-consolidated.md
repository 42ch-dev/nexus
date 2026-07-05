---
report_kind: qc-consolidated
plan_id: "2026-06-23-v1.61-wasm-host-and-basic-combat"
reviewer: "@project-manager"
consolidated_at: "2026-06-23"
verdict: "Approve"
seats:
  - { reviewer: qc-specialist,   index: 1, verdict: Approve, report: qc1.md, commit: 936766ff }
  - { reviewer: qc-specialist-2, index: 2, verdict: Approve, report: qc2.md, commit: a5286bac }
  - { reviewer: qc-specialist-3, index: 3, verdict: Approve, report: qc3.md, commit: 334fdc06, revalidation_commit: 5b53a64e }
fix_wave:
  - trigger: "user-requested: gitignore embedded-modules + build.rs compile-from-source"
  - commit: 8aba63a6
  - revalidation: "qc3 targeted re-review — Approve (mtime gating sound, fresh build byte-identical, all tests pass)"
---

# QC Consolidated — V1.61 P2 (nexus-wasm-host + basic-combat)

| Seat | Focus | Verdict | Critical | Warning | Suggestion |
|------|-------|---------|----------|---------|------------|
| qc1 | Architecture | **Approve** | 0 | 0 | 5 |
| qc2 | Security/correctness | **Approve** | 0 | 1W | 3 |
| qc3 | Performance/reliability | **Approve** | 0 | 2W | 0 |
| qc3 revalidation | build.rs fix-wave | **Approve** | 0 | 0 | 0 |

**Consolidated: APPROVE.** One fix-wave (user-requested gitignore change) — revalidated by qc3.

Warnings (both acceptable for V1, tracked for P3/V1.62+):
- qc2 W-001: memory cap is grow-time only (instantiation-time allocation not capped). Acceptable V1 — embedded modules trusted; fuel+wall-time as primary bounds.
- qc3 W-001: watchdog thread spawn ~10-50μs/call — acceptable, monitor in P3.
- qc3 W-002: memory cap enforcement same as qc2 W-001.

Fix-wave: user requested `embedded-modules/` be gitignored. build.rs changed from guard → compile-from-source (mtime-gated, byte-identical output). qc3 revalidation: mtime gating sound, fresh build reproducible (SHA256 stable), all 17 tests pass, no runtime changes.

Residuals registered: R-V161P2-LOW-001..003 (doc drift + CI matrix — all P-last).

P2 clear to mark Done. Wave 3 (P3 narrative.compute + combat-engine preset) can proceed — depends on P1 (KB structured layer) + P2 (wasm-host crate), both merged to integration.
