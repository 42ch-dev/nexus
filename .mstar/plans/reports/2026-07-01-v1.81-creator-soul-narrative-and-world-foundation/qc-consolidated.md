# QC Consolidated Decision — 2026-07-01-v1.81-creator-soul-narrative-and-world-foundation

**Wave**: V1.81 dual-track implement (P0 backend + P1 frontend), diff `83000ca3...cb802209`.
**Reviewers**: qc-specialist (architecture), qc-specialist-2 (security/correctness), qc-specialist-3 (performance/reliability).
**Consolidated by**: `@project-manager` on 2026-07-02.

## Per-seat verdicts

| Seat | Verdict | Critical | Warning | Suggestion |
|------|---------|----------|---------|------------|
| qc1 architecture | Request Changes | 0 | 1 (W-001) | 5 |
| qc2 security/correctness | **Approve** | 0 | 0 | 3 |
| qc3 performance/reliability | Request Changes | 0 | 2 (W-QC3-001/002) | 2 |

## Consolidated decision: **Request Changes → fix-wave**

2 of 3 seats Request Changes. Three blocking Warning findings must be fixed in-wave, then **targeted re-review** of the seats that raised them (qc1 + qc3). qc2 (Approve) is out of the re-review set unless a fix touches its scope.

### Blocking findings (fix-in-wave, owner `@fullstack-dev`)

1. **R-V181P0-QC1-W001** — Missing daemon-level regression test for `world_id` threading through the review pipeline. Implementation threads correctly (`crates/nexus-daemon-runtime/src/api/handlers/memory.rs:886`) but only a local-db unit test covers it; no daemon-integration test asserts `world_id: Some(...)` round-trips POST pending review → POST `/review` → GET `/fragments?world_id=…`. **Fix**: add the daemon integration test (both `Some` + `None`/core-only cases) + an orchestration `write_memory` propagation assertion.
2. **R-V181P0-QC3-W001** — `/memory/soul/reflect` read/status path scans + decodes every fragment on every read/poll (`memory.rs:1136-1145`, `soul_narrative.rs:106-123` `fetch_all` + JSON decode; frontend polls 30s). Violates the P-1 locked "bounded read-side keyword decode" guard. **Fix**: split cheap stale stats (`COUNT(*)`, `MAX(created_at)`) into SQL aggregates; cap/limit any Rust keyword decode used for the insufficient-data gate; ensure `force_regenerate=false` returns cached `current`/`stale` without scanning all fragment keyword JSON.
3. **R-V181P0-QC3-W002** — Summary truncation slices UTF-8 by byte offset (`memory.rs:1341-1345` `&frag.summary[..279]`) → panics on non-ASCII. **Fix**: truncate by Unicode scalar chars (`summary.chars().take(279)` + `…`), add a multi-byte regression test.

### Deferred findings → residuals (V1.82+; registered in `status.json`)

- R-V181P0-QC1-S001 — plan-promised `validate draft` step not implemented (quality floor is prompt-only). defer V1.82+.
- R-V181P0-QC1-S002 — `MemoryError → NexusApiError` mapping is string-content-matching (brittle). defer V1.82+.
- R-V181P0-QC1-S005 — synthesized narrative length uncapped at persist boundary. defer V1.82+.
- R-V181P0-QC1-S003 — `world-selector.tsx` simplify comment claims "Tracked" but tracker lacks the entry. PM lightweight task (this wave or P-last).
- Low-value/accepted (not registered as tracked residuals): qc1 S-004 (u64/i64 count mix — codegen-driven one-liner, accept), qc2 ×3 suggestions (concurrency note / synthesis-boundary doc / extra test pin — accept), qc3 S-QC3-001 (duplicate observers — accept), qc3 S-QC3-002 (MSW coverage for POST narrative — accept).

## Fix-wave plan

- Branch: `fix/v1.81-qc-worldid-test-and-perf` from `iteration/v1.81`.
- Owner: `@fullstack-dev` (all three fixes are backend: handler + DAO + tests).
- After fix merges → integration, **targeted re-review** (N=2): qc-specialist (W-001) + qc-specialist-3 (W-QC3-001/002), same `qc1.md` / `qc3.md` files (`## Revalidation`, update verdict). qc2 not re-reviewed (Approve, scope untouched).
- Re-review basis: the fix diff on `iteration/v1.81` (new HEAD after fix merge).
