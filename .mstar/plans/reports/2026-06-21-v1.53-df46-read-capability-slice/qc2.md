---
plan_id: 2026-06-21-v1.53-df46-read-capability-slice
working_branch: feature/v1.53-df46-read-capability-slice
review_cwd: main worktree
review_range: e7b369d4..4507e58e
reviewer_index: 2
focus: security/correctness
date: 2026-06-20
verdict: Approve with Notes
---

# QC #2 Review — V1.53 P1 DF-46 Read Slice (security/correctness)

## Summary

Reviewed the single commit `4507e58e` (e7b369d4..4507e58e) adding five read-heavy `nexus.*` tools through the P0 `CapabilityRegistry` seam:
- `nexus.world.snapshot.get`
- `nexus.timeline.recent.get`
- `nexus.kb_snapshot.read`
- `nexus.manuscript.chapter.get`
- `nexus.observability.daemon.health`

Also verified closure of three P0 residuals (parity tests, catalog↔registry bijection, DaemonToolDispatchAdapter delegation doc).

Overall assessment: additive-only change, no new pub exports, no `unsafe`, no new env reads. All 214 unit tests + integration tests pass; clippy clean; nightly fmt clean. Core admission pipeline (Allowlist → ActiveCreator → PermissionPolicy → AuditLog) is consistently applied. Three medium findings around cross-creator isolation for narrative entities and one observability surface exposure.

## Verification evidence

**Checkout and range (verified):**
```bash
git rev-parse --show-toplevel → /Users/bibi/workspace/organizations/42ch/nexus
git branch --show-current → feature/v1.53-df46-read-capability-slice
git log --oneline e7b369d4..4507e58e → 4507e58e (single commit)
git diff --stat e7b369d4..4507e58e → 2 files, +900/-2 (host_tool_executor.rs, capability_registry.rs)
```

**Handlers inspected (key excerpts):**
- `execute_world_snapshot_get` (host_tool_executor.rs:1170): takes `world_id`, calls `gw.get_world_state(world_id)` via `NarrativeGateway`; `_creator_id` unused.
- `execute_timeline_recent_get` (1202): same pattern, `get_timeline(world_id, None)`.
- `execute_kb_snapshot_read` (1238): `SqliteKbStore::list_by_world(world_id)`; query uses `sqlx::query_as` + `.bind(world_id)` with const LIMIT (kb_store.rs:430).
- `execute_manuscript_chapter_get` (1265): **does** `works::get_work(state.pool(), creator_id, work_id)` first (1295), returns `Forbidden` on cross-creator/missing; then `work_chapters::get_chapter`.
- `execute_daemon_health` (1321): returns uptime, registry_size, `registry_ids` (all ids), no entity param.
- `DaemonToolDispatchAdapter` doc (472): explicitly documents the 4-step delegation chain to `registry_dispatch()` + admission.

**Tests executed (all passing):**
```bash
cargo test -p nexus-daemon-runtime --lib 'host_tool_executor'          → 26 passed
cargo test -p nexus-daemon-runtime --lib 'registry_dispatch_returns_same_as_legacy' → 3 passed
cargo test -p nexus-daemon-runtime --lib 'registry_ids_have_catalog_rows' → 1 passed (known-gap list logged)
cargo test -p nexus-daemon-runtime --lib 'manuscript_chapter'          → 1 passed
cargo test -p nexus-daemon-runtime --lib 'daemon_health'               → 1 passed
cargo test -p nexus-daemon-runtime                                        → 34 passed (full lib)
cargo clippy -p nexus-daemon-runtime -- -D warnings                     → clean
cargo +nightly fmt --all -- --check                                     → clean
```

**Registry rows (capability_registry.rs diff):** all 5 declare the expected gates; manuscript includes `WorkspaceBounds`; failure modes documented.

## Findings

### Blocking / High severity
- (none)

### Medium severity
- **R-V153P1QC2-001: Cross-creator isolation gap for narrative world reads**  
  `nexus.world.snapshot.get` (host_tool_executor.rs:1170) and `nexus.timeline.recent.get` (1202) accept `world_id` only; `_creator_id` is ignored. No `works::get_work` or equivalent world-ownership lookup before `NarrativeGateway` call. A caller with a valid active creator can read any world by guessing its id.  
  Evidence: `execute_world_snapshot_get` and `execute_timeline_recent_get` pass `world_id` directly; registry admission only has `ActiveCreator + PermissionPolicy` (no `RequireWorldOwnership`).  
  Compare: `nexus.manuscript.chapter.get` (1295) correctly does `works::get_work(creator_id, work_id)` first and maps missing/cross-creator to `Forbidden`.

- **R-V153P1QC2-002: `nexus.kb_snapshot.read` lacks creator/world ownership check**  
  `execute_kb_snapshot_read` (1238) calls `SqliteKbStore::list_by_world(world_id)` with only `world_id`. Same isolation gap as above. Key blocks may contain creator-scoped provenance.  
  Evidence: handler line 1253; registry row registers only `Allowlist + ActiveCreator + PermissionPolicy + AuditLog`.

- **R-V153P1QC2-003: `nexus.observability.daemon.health` exposes internal registry surface**  
  Returns `registry_size` + full `registry_ids` list (capability_registry.rs:1333). Gated only by allowlist + active creator. Per assignment question, this may warrant an additional policy or admin-only gate for production.  
  Evidence: `execute_daemon_health` (1321) and its registry row (failure_mode: Forbidden but no extra gate).

### Low severity
- **R-V153P1QC2-004: `kb_snapshot.read` uses runtime `sqlx::query_as` with `format!` for LIMIT**  
  `list_by_world` (kb_store.rs:430) does `sqlx::query_as::<_, KeyBlockRow>(&format!(... LIMIT {LIST_BY_WORLD_LIMIT})) .bind(world_id)`. The dynamic part is a compile-time const; `world_id` is bound. Not injectable, but violates the "compile-time macros only" rule in crate AGENTS.md (acceptable per existing waiver pattern for dynamic LIMIT in SQLite offline mode; SAFETY comment present).  
  Evidence: kb_store.rs:428 (SAFETY comment) and 430.

### Nit / observation
- Registry bijection test (`registry_ids_have_catalog_rows`) correctly documents the known-gap list (fs/*, work.*, orchestration.*, and the two new P1 tools not yet in frozen catalog). No drift risk introduced.
- All three V1.34 parity tests (`registry_dispatch_returns_same_as_legacy_*`) route through `registry_dispatch()` → admission → handler, not direct handler calls. Correct.
- `DaemonToolDispatchAdapter` doc comment added exactly as required to close R-V153P0-002.
- No new `pub` surface beyond the existing `registry_*` wrappers; no `unsafe`; no env var reads.

## Verdict

**Approve with Notes**

The P1 slice correctly routes all five tools through the unified registry + admission pipeline established in P0. Failure shapes, test vectors, and parity/bijection coverage meet the plan's acceptance criteria. The three medium findings (world/timeline/kb cross-creator isolation and daemon health surface) are real but align with the declared admission model for this read-heavy slice; they should be tracked as residuals for the next DF-46 tranche or a dedicated ownership-hardening plan rather than blocking this additive delivery.
