---
plan_id: 2026-06-21-v1.53-df46-read-capability-slice
working_branch: feature/v1.53-df46-read-capability-slice
review_cwd: main worktree
review_range: e7b369d4..4d8fb458
consolidation_date: 2026-06-20
gate_verdict: Approve with Notes
gate_state: closed
---

# QC Consolidated — V1.53 P1 DF-46 Read Slice

**Plan**: `2026-06-21-v1.53-df46-read-capability-slice`
**Branch**: `feature/v1.53-df46-read-capability-slice`
**Range**: `e7b369d4..4d8fb458`
**Date**: 2026-06-20

## Reviewer verdicts

| QC | Reviewer index | Focus | Initial verdict | Final verdict |
|---|---|---|---|---|
| qc1 | 1 | architecture/maintainability | **Request Changes** | Approve with Notes |
| qc2 | 2 | security/correctness | Approve with Notes | (unchanged) |
| qc3 | 3 | performance/reliability | Approve with Notes | (unchanged) |

## Gate verdict (PM consolidation)

**Approve with Notes** — all 3 QC reviewers approve after the fix-wave.

Initial qc1 verdict was **Request Changes** for 1 HIGH + 1 MEDIUM finding:
- **HIGH R-V153P1QC1-001**: Cross-creator/world isolation missing in 3 handlers
- **MEDIUM R-V153P1QC1-002**: Failure/admission test coverage incomplete

Plus qc2 raised 3 same-theme MEDIUMs (R-V153P1QC2-001/002 cross-creator gap same as qc1's HIGH; R-V153P1QC2-003 daemon.health registry_ids exposure), and qc3 raised 1 MEDIUM (R-V153P1QC3-001 timeline.recent.get unbounded memory).

Fix-wave commit `4d8fb458` addressed all 4 must-fix items in a single commit:
1. Added `AdmissionGate::RequireWorldOwnership` enum variant
2. Added `ensure_world_accessible_for_creator(pool, creator_id, world_id)` helper (queries `narrative_worlds WHERE world_id = ? AND owner_creator_id = ?`); called BEFORE data fetch in 3 handlers
3. Added 9 new tests (3 cross-creator denial + 4 negative/admission + 2 LIMIT)
4. Added server-side LIMIT to `timeline.recent.get` (default 100, max 500)
5. Updated 2 stale V1.34-only comments

Targeted qc1 re-review upgraded verdict to Approve with Notes (1 new low finding surfaced in re-review).

## Findings summary

### Resolved (fix-wave at commit 4d8fb458)

- **R-V153P1QC1-001** (HIGH): cross-creator/world isolation — RESOLVED via `ensure_world_accessible_for_creator` helper + `RequireWorldOwnership` gate
- **R-V153P1QC1-002** (MEDIUM): failure/admission test coverage — RESOLVED with 9 new tests
- **R-V153P1QC1-003** (NIT): stale V1.34-only comments — RESOLVED
- **R-V153P1QC2-001** (MEDIUM, qc2): world/timeline cross-creator gap — RESOLVED (covered by QC1-001 fix)
- **R-V153P1QC2-002** (MEDIUM, qc2): kb_snapshot.read cross-creator gap — RESOLVED (covered by QC1-001 fix)
- **R-V153P1QC3-001** (MEDIUM, qc3): timeline.recent.get unbounded memory — RESOLVED with server-side LIMIT (default 100, max 500)

### Accepted as residuals (recorded in status.json)

| ID | Severity | Title | Target |
|---|---|---|---|
| R-V153P1QC1R-001 | low | Timeline SQL uses sanitized dynamic `LIMIT {limit_i64}` rather than `LIMIT ?` (sqlx offline cache requires manual regeneration) | V1.54+ (deferred; SAFETY comment documented) |
| R-V153P1QC2-003 | medium | `daemon.health` exposes full registry_ids list — should be gated behind additional policy check | V1.54+ (deferred to policy work) |
| R-V153P1QC2-004 | low | kb_store runtime sqlx query with `format!` for LIMIT (parameterized, not injectable) | V1.54+ (same as R-V153P0QC3-003; SAFETY comment present) |
| R-V153P1QC3-002 | low | Per-dispatch registry rebuild (R-V153P0QC3-001 still applies; same path) | V1.54+ (deferred optimization; see P0 residual R-V153P0QC3-001) |

### Nits (acknowledged, not residual)

- qc1: a few comments still describe the surface as V1.34-only after P1 expansion — fixed in fix-wave
- qc3: hardcoded `pool_healthy: true` in `daemon_health` handler — cosmetic
- qc3: pre-existing runtime query in `work_chapters::get_chapter` (not introduced by P1)

## Cascading impact

The fix-wave touched 6 files across 4 crates:
- `crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor.rs` (+346 lines)
- `crates/nexus-daemon-runtime/src/capability_registry.rs` (+7/-1)
- `crates/nexus-local-db/src/narrative_gateway.rs` (+85/-40) — `get_timeline` interface change
- `crates/nexus-local-db/src/narrative_write.rs` (+1/-1) — call site update
- `crates/nexus-moment-context-assembly/src/moment.rs` (+1/-1) — call site update
- `crates/nexus-narrative/src/gateway.rs` (+15/-7) — trait + InMemoryNarrativeGateway update

All call sites updated; cascading type check passed; clippy clean across all 4 crates.

## Final outcome

**P1 status**: Approved (Approve with Notes)
**Next**: PM marks P1 Done in `status.json`; merges `feature/v1.53-df46-read-capability-slice` → `iteration/v1.53`; dispatches V1.53 P-c (skills CLI cleanup).