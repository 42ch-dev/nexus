---
plan_id: 2026-06-22-v1.57-v156-carry-forwards-and-compliance
iteration: V1.57
wave: Wave 2
reviewer: @project-manager
consolidated_from:
  - qc-specialist (qc1.md) — Approve (commit 53892b0d)
  - qc-specialist-2 (qc2.md) — Approve (commit 3ad75f4f)
  - qc-specialist-3 (qc3.md) — Approve (commit a63861c8)
fix_wave_commits: []
generated_at: 2026-06-22
verdict: Approve
---

# QC Consolidated — V1.57 P2 V1.56 Carry-Forwards & Compliance

## Verdict

**Approve** (PM consolidated, 2026-06-22)

- qc1: **Approve** (10/10 ACs met; zero scope creep)
- qc2: **Approve** (1 low suggestion; non-blocking — deprecation timeline not documented)
- qc3: **Approve** (clean; 0 flakiness; codegen clean; DB migration not required)

## AC Disposition (10 AC)

| AC | Title | Disposition | Evidence |
|----|-------|-------------|----------|
| 1 | R-V156P1-M001 schema rename | met | `#[serde(alias = "agent_count", alias = "agentCount")]` on `capability_count` field; backward-compat |
| 2 | Reproducer test exists | met | 5 tests in `schema_rename_compliance.rs` |
| 3 | CLI surface new field name | met | `capabilityCount` in daemon capability-registry response; nexus42 tests pass |
| 4 | Agent surface reflects rename | met | `capabilityCount` in 3-caller dispatch |
| 5 | In-scope residuals documented | met | Only R-V156P1-M001 absorbed |
| 6 | Out-of-scope enumerated | met | R-V156P0-M001/M002 + P0 M003-M006 explicitly deferred to V1.58+ |
| 7 | cargo test -p nexus-contracts passes | met | 107/107 |
| 8 | cargo test -p nexus42 passes | met | 762 unit + all integration |
| 9 | cargo test -p nexus-daemon-runtime passes | met | 267 unit + all integration |
| 10 | cargo clippy clean | met | No warnings |

## Findings Summary

- qc1: 0 findings; 1 suggestion (merge commit could enumerate deferred residuals — non-blocking)
- qc2: 1 low suggestion (deprecation timeline not documented; non-blocking)
- qc3: 0 findings; 2 pre-existing test warnings in nexus-daemon-runtime (not introduced by P2)

## Verdict Rationale

P2 cleanly closes R-V156P1-M001 with backward-compat serde aliases. The 5 reproducer tests are hermetic and well-named. The local `agent_count` variable in `nexus-acp-host/src/registry.rs:388` was correctly excluded (different concept; ACP agent count, not capability count). All 10 ACs met; no regressions. Carry-forward R-V156P1-M001 closed.

## Action Items

- [x] All 3 qc-specialist reports committed (53892b0d, 3ad75f4f, a63861c8)
- [x] PM status.json update: P2 → Done
- [x] Carry-forward R-V156P1-M001 closed
- [ ] Wave 2 mid-QA dispatch (qa-engineer) — next
- [ ] Wave 3 dispatch (P3) — after mid-QA returns

## Handoff

- 3 QC reports + 1 consolidated = 4 reports on integration branch
- P2 ready for PM Done sign-off; merge_commit = 236c34a4
- Wave 3 (P3) cleared for dispatch
