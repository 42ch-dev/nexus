---
report_kind: qc-consolidated
plan_id: "2026-06-23-v1.61-compute-capability-and-preset"
reviewer: "@project-manager"
consolidated_at: "2026-06-23"
verdict: "Approve"
seats:
  - { reviewer: qc-specialist,   index: 1, verdict: Approve, report: qc1.md, commit: 7f273480 }
  - { reviewer: qc-specialist-2, index: 2, verdict: Approve, report: qc2.md, commit: c089cd7e }
  - { reviewer: qc-specialist-3, index: 3, verdict: Approve, report: qc3.md, commit: 8c471c26 }
---

# QC Consolidated — V1.61 P3 (narrative.compute + combat-engine preset)

**Consolidated: APPROVE.** No fix-wave. All warnings are V1.61-acceptable (embedded trusted modules; same-creator worlds).

## Warnings → residual routing

| ID | Source | Finding | Severity | Target |
|----|--------|---------|----------|--------|
| R-V161P3-ARCH-001 | qc1 W-1 | Preset state machine has no-op wait states (apply_delta, advance_timeline); full cycle collapses into single capability call on load_world | low | P-last/V1.62 |
| R-V161P3-CORR-001 | qc2 W-1 | Partial apply on batch state_delta error — no enclosing transaction; earlier deltas may apply before a later one fails | low | P-last/V1.62 |
| R-V161P3-CORR-002 | qc2 W-2 | new_key_blocks world_id not re-asserted against admitted world before creation | low | P-last |
| R-V161P3-PERF-001 | qc3 | WasmEngine constructed per-pool, not singleton at daemon boot | low | P-last (T1 daemon wiring) |
| R-V161P3-PERF-002 | qc3 | WASM module recompiled on every run() — no cache | low | P-last (T1 daemon wiring) |
| R-V161P3-MAINT-001 | qc1 S-x | Dead prompt files (5 markdown files unreferenced by preset.yaml) | low | P-last |
| R-V161P3-MAINT-002 | qc1 S-x | Stale test function name registry_has_twenty_six_builtins (asserts 32) | nit | P-last |

## P-last integration risks (from P3 dev + QC)

P-last T1-T4 (daemon wiring) MUST address:
1. **Singleton WasmEngine** at daemon boot (not per-pool) — R-V161P3-PERF-001
2. **Module compilation cache** (Arc<RwLock<HashMap>>) — R-V161P3-PERF-002
3. Load embedded modules from include_dir! at startup
4. Scan ~/.nexus42/modules/ for user-installed modules

P3 clear to mark Done. Wave 4 (P-last) dispatch-ready.
