---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-07-02-v1.82-per-world-narrative-backend"
verdict: "Request Changes"
generated_at: "2026-07-02"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: volcengine-plan/ark-code-latest
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-07-02T14:45:00-07:00

## Scope
- plan_id: `2026-07-02-v1.82-per-world-narrative-backend` (dual-track wave — also covers P1 `2026-07-02-v1.82-soul-surface-completion`)
- Review range / Diff basis: `merge-base: b554b5aa … tip: 575f7a5d` = `git diff b554b5aa...575f7a5d`. **Use this exact range.**
- Working branch (verified): `iteration/v1.82`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- HEAD observed at review start: `3389a2f3be4bd38aaba9f43bec83a84cce64af29` (`3389a2f3 chore(v1.82): P0+P1 InReview (merged to integration)`). The assigned diff range remains reproducible and was reviewed exactly as `git diff b554b5aa...575f7a5d`.
- Files reviewed: 18 key files across P0/P1 and support docs/tests, including compass §§9–10, P0/P1 plans, V1.81 greploop lessons, `soul_narrative.rs`, `memory.rs`, migration/tests, and SOUL frontend query/client/components/tests.
- Commit range: exact assigned diff `b554b5aa...575f7a5d`.
- Deep review: triggered (signals: schema/database migration, new per-world polled read paths, LLM side-effect gating, dual backend/frontend wave, cache/performance-sensitive endpoint).
- Lenses applied: Performance Hot Path, Reliability/Side-Effect Gating, Data Migration, Frontend Polling/Invalidation, Contract Boundary.
- Tools run:
  - `git diff b554b5aa...575f7a5d`
  - `cargo clippy -p nexus-daemon-runtime -p nexus-local-db -- -D warnings`
  - `cargo test -p nexus-daemon-runtime -p nexus-local-db`
  - `pnpm --filter web run test`

## Findings

### 🔴 Critical
- None.

### 🟡 Warning

#### W-QC3-001 — P1 expects `/v1/local/narrative/worlds` to return a raw `World[]`, but the daemon returns `{ worlds: WorldState[] }`

**Risk**: Runtime reliability regression in the SOUL surface. The world-selector path added in P1 will fail against the actual daemon response shape, so selecting/loading worlds can break before the per-world narrative path is usable.

**Evidence**:
- Backend handler returns an object envelope, not an array:
  - `crates/nexus-daemon-runtime/src/api/handlers/narrative.rs:25-29` defines `ListWorldsResponse { worlds: Vec<WorldState> }`.
  - `crates/nexus-daemon-runtime/src/api/handlers/narrative.rs:43-55` returns `Ok(Json(ListWorldsResponse { worlds }))`.
- Frontend client/hook types and parses the same route as a raw array:
  - `apps/web/src/lib/nexus/browser-client.ts:411-413`: `listNarrativeWorlds(): Promise<World[]> { return this.get<World[]>('/v1/local/narrative/worlds'); }`
  - `apps/web/src/api/queries.ts:687-692`: `useNarrativeWorlds` returns `Promise<World[]>` directly.
  - `apps/web/src/components/memory/soul-section.tsx:105-107` passes `worlds.data ?? []` to `WorldSelector`.
  - `apps/web/src/components/soul/world-selector.tsx:58` immediately spreads `worlds` (`const sortedWorlds = [...worlds]`). If `worlds.data` is `{ worlds: [...] }`, this throws because the object is not iterable.
- The new P1 tests mask this mismatch by mocking the route as a raw array:
  - `apps/web/src/components/memory/soul-section.test.tsx:60-62`: `http.get('/v1/local/narrative/worlds', () => HttpResponse.json(worlds))`.

**Fix**: Either (a) align the client with the existing daemon shape (`Promise<{ worlds: World[] }>` and return `res.worlds` from the hook), or (b) intentionally change the daemon route/contract and update tests/docs accordingly. The smaller surgical fix is adapting `BrowserClient.listNarrativeWorlds`/`NexusClient` to the already-shipped `{ worlds }` envelope and updating P1 tests to mock that real shape.

**Source Type**: deep-lens: Contract Boundary / Frontend Reliability

#### W-QC3-002 — Per-world distinct-keyword recompute does not early-exit at 20; it drains every keyword row on cache miss/fingerprint change

**Risk**: The steady-state cached read path is good, but the changed-fragment path can still perform an unbounded JSON decode scan for a large world. The Assignment specifically called for the per-world distinct-keyword scan to be bounded by early-exit-at-20 plus the fingerprint cache. Current code reaches 20, then intentionally drains the remaining rows for an exact count, so the recompute is O(all fragment keyword JSON) whenever a world's fingerprint changes or a stats-only row is first created.

**Evidence**:
- `crates/nexus-local-db/src/soul_narrative.rs:181-187` documents early-exit streaming, but the implementation continues scanning after threshold:
  - `crates/nexus-local-db/src/soul_narrative.rs:228-239`: after `distinct.len() >= DISTINCT_KEYWORD_THRESHOLD`, the function enters a nested loop to drain remaining rows and decode every remaining `keywords` JSON string before returning.
- Tests lock in exact counts above the gate rather than threshold-bounded counts:
  - `crates/nexus-local-db/tests/soul_narrative_keyword_count.rs:57-60` expects 30.
  - `crates/nexus-local-db/tests/soul_narrative_per_world.rs:188-199` expects whole=25, world A=12, world B=8.
- The V1.82 assignment focus was to verify "per-World distinct-keyword scan is bounded (early-exit-at-20 + fingerprint cache)" and "No unbounded keyword JSON decode on every per-World poll." The cache prevents this on unchanged fingerprints, but the recompute branch itself is not early-exit bounded.

**Fix**: Decide the public semantics of `current_distinct_keyword_count`. If the gate only needs `>=20`, return a threshold-capped/saturated count (or add a separate `has_min_distinct_keywords` semantic) and stop decoding as soon as 20 distinct keywords are found. If an exact count is intentionally required by product/UI, document that explicit tradeoff in the plan/knowledge note and consider a stronger bounded aggregate strategy; as written, the implementation contradicts the assigned bounded-scan requirement.

**Source Type**: deep-lens: Performance Hot Path

### 🟢 Suggestion

#### S-QC3-001 — Update stale interface prose for per-world reflect semantics

`apps/web/src/lib/nexus/types.ts:348-354` still says `reflectSoulNarrative` is "whole-Creator" and "per-world narratives are out of scope," even though P-1/P0 added `world_id`. This is not a runtime blocker, but it is likely to mislead future maintainers working at the transport boundary. Update the method doc to match the generated `SoulNarrativeRequest` description.

**Source Type**: manual-reasoning

## Positive Reliability / Performance Notes

- Migration reliability: the composite-key migration uses an explicit by-name column-list copy (`20260704_000001...sql:34-43`), preserving V1.81 rows as `(creator_id, NULL)`, and adds the partial unique index for `world_id IS NULL` (`:29-30`). Tests cover V1.81 survival and duplicate Creator-level rejection (`soul_narrative_per_world.rs`).
- Read-path invariant: `reflect_soul` performs ownership before stats/synthesis (`memory.rs:1147-1165`), computes stats/gate before ACP, and returns `ungenerated/current/stale/insufficient_data` for `force=false` before the capability-registry lookup (`memory.rs:1190-1302`). The per-world negative-path test exists (`reflect_soul_per_world_no_force_ungenerated_no_llm_call`).
- Per-(creator, world) cache key: DAO reads/writes include `world_id IS ?`/bound `world_id` and the cache row carries `world_id` (`soul_narrative.rs:51-82`, `:93-118`, `:269-361`). Cached fingerprint hits return without keyword streaming (`soul_narrative.rs:315-328`).
- Synthesis cost: `force=true` is the only path to `capability_registry()`/`AcpSoulNarrativeSynthesizer` (`memory.rs:1304-1331`), and synthesis input is subset-scoped and capped via `list_fragments_limited(..., world_id, 100)` plus <=30 keywords/<=24 summaries/<=8 buckets (`memory.rs:1404-1459`).
- Frontend polling cadence is conservative (30s) and does not opt into background refetch; post-review invalidation scopes by creator narrative prefix. Query keys include `worldId`, so selected scope changes do not reuse stale narrative cache entries.

## Source Trace

- Finding ID: W-QC3-001
- Source Type: deep-lens: Contract Boundary / Frontend Reliability
- Source Reference: `crates/nexus-daemon-runtime/src/api/handlers/narrative.rs:25-55`; `apps/web/src/lib/nexus/browser-client.ts:411-413`; `apps/web/src/api/queries.ts:687-692`; `apps/web/src/components/memory/soul-section.test.tsx:60-62`
- Confidence: High

- Finding ID: W-QC3-002
- Source Type: deep-lens: Performance Hot Path
- Source Reference: `crates/nexus-local-db/src/soul_narrative.rs:181-239`; `crates/nexus-local-db/tests/soul_narrative_keyword_count.rs:57-60`; assignment focus requiring early-exit-at-20
- Confidence: High

- Finding ID: S-QC3-001
- Source Type: manual-reasoning
- Source Reference: `apps/web/src/lib/nexus/types.ts:348-354`
- Confidence: High

## Validation Evidence

- `cargo clippy -p nexus-daemon-runtime -p nexus-local-db -- -D warnings` — passed (after waiting for build-directory lock).
- `cargo test -p nexus-daemon-runtime -p nexus-local-db` — passed on rerun: 365 daemon-runtime unit tests, daemon integration tests, 289 local-db unit tests, local-db integration tests including `soul_narrative_keyword_count` (9), `soul_narrative_per_world` (3), and `sqlx_cache_intact` (1). Note: an earlier run failed after local `.sqlx/` files were transiently missing from the working tree; `git checkout -- .sqlx` restored tracked files, `git status --short -- .sqlx` returned clean, and the required test command then passed.
- `pnpm --filter web run test` — passed: 50 test files, 384 tests. Non-failing stderr included existing React Router future-flag warnings, existing act warnings in `EntityInspector`, and one MSW unhandled-request warning in `outline-page.test.tsx`; vitest exit code was success.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 1 |

**Verdict**: Request Changes
