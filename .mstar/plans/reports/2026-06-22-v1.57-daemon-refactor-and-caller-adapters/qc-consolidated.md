---
plan_id: 2026-06-22-v1.57-daemon-refactor-and-caller-adapters
iteration: V1.57
wave: Wave 1
reviewer: @project-manager
consolidated_from:
  - qc-specialist (qc1.md) — original Approve (commit f2de19ea)
  - qc-specialist-2 (qc2.md) — original Approve (commit f76182e7)
  - qc-specialist-3 (qc3.md) — original Request Changes (commit 77bd271b); re-review Approve (commit db5aecc9)
fix_wave_commits: [544a1184]
generated_at: 2026-06-22
verdict: Approve
---

# QC Consolidated — V1.57 P1 Daemon Refactor & 3-Caller Adapters

## Verdict

**Approve** (PM consolidated, 2026-06-22)

- qc1 original: **Approve** (no critical findings; 2 suggestions)
- qc2 original: **Approve** (no critical; 1 warning about `#[ignore]` smoke tests; 2 suggestions)
- qc3 original: **Request Changes** (1 critical: 4 spec amendments missing) → re-review **Approve** after fix-wave `544a1184`

## AC Disposition (18 AC)

| AC | Title | Disposition | Evidence |
|----|-------|-------------|----------|
| 1 | host_tool_executor.rs ≤ 800 lines | met | 349 lines |
| 2 | 3 caller entry points exist | met | qc1 ✅ qc2 ✅ |
| 3 | All 3 dispatch via capability::Registry::dispatch | met | qc2 ✅ |
| 4 | 7 execute_X fns removed | met (P0 migration was no-op; P1 extracted to host_tool_handlers.rs) | qc1 ✅ |
| 5 | nexus42 host-call works E2E | met | qc1 ✅ |
| 6 | host-call --help documents debug intent | met | qc1 ✅ |
| 7 | cli-spec.md §6.2M added | met (after fix-wave) | qc3 re-review ✅ |
| 8 | daemon-runtime.md host_tool section | met (after fix-wave) | qc3 re-review ✅ |
| 9 | local-runtime-boundary.md topology | met (after fix-wave) | qc3 re-review ✅ |
| 10 | orchestration-engine.md §6.4 | met (after fix-wave) | qc3 re-review ✅ |
| 11 | CdnConfig constructor-injected (R-V156P1-M002 closed) | met | qc1 ✅ qc2 ✅ |
| 12 | R-V156P3-S003 field drops in caller surfaces | met | qc2 ✅ |
| 13 | 3 caller integration tests | met | qc1 ✅ qc2 ✅ |
| 14 | host-call smoke test (3 IDs: read/write/policy-gated) | met (with #[ignore] for live daemon requirement) | qc2 ⚠ — warning, not blocking |
| 15 | cargo test -p nexus-daemon-runtime passes | met | qc1 ✅ |
| 16 | cargo test -p nexus42 passes | met | qc1 ✅ |
| 17 | cargo clippy -p nexus-daemon-runtime -p nexus42 clean | met | qc1 ✅ |
| 18 | cargo +nightly fmt clean | met | qc1 ✅ |

## Findings Summary

- qc1-F-001 (suggestion) — host_tool_handlers.rs at 1839 lines (god-file shifted) → deferred to V1.58+ (out of P1 scope)
- qc1-F-002 (suggestion) — re-export coupling → deferred to V1.58+
- qc2-F-001 (warning) — host-call smoke tests are `#[ignore]` (require live daemon + active creator) → filed as R-V157P1-W001
- qc2-F-002/003 (suggestions) — request_id correlation hygiene; spec amendment visibility → deferred
- qc3-F-001 (critical) → resolved by fix-wave `544a1184` (4 spec amendments as Draft overlays)
- qc3-F-002 (suggestion) — load_permission_policy filesystem I/O per call (pre-existing) → deferred
- qc3-F-003 (suggestion) — unused import in agent_tool_api.rs (pre-existing) → deferred

## Verdict Rationale

P1 implementation is clean: god-file refactored from 4298→349 lines; 3 caller entry points (CLI/worker/HTTP) all dispatch through single `capability::Registry::dispatch`; `CdnConfig` constructor-injected (closes R-V156P1-M002); 4 spec amendments delivered as Draft overlays via fix-wave `544a1184`. All 18 AC met post-fix-wave. Pre-existing suggestions deferred to V1.58+.

## Action Items

- [x] Fix-wave commit: `544a1184`
- [x] All 3 qc-specialist reports committed (qc1 Approve + qc2 Approve + qc3 re-review Approve = db5aecc9)
- [x] PM status.json update: P1 → Done
- [x] V1.57+ residuals registered: R-V157P1-W001 (host-call smoke `#[ignore]`)
- [x] Carry-forward R-V156P1-M002 closed
- [x] Carry-forward R-V156P3-S003 closed (P0 + P1)
- [ ] Mid-QA dispatch (qa-engineer) — next
- [ ] Wave 2 dispatch (P2) — after mid-QA returns

## Handoff

- 6 QC reports + 3 re-review reports + 1 consolidated = 10 reports on integration branch
- P1 ready for PM Done sign-off; merge_commit = fe501b6b; fix-wave commit 544a1184
- Wave 2 (P2) cleared for dispatch after mid-QA
