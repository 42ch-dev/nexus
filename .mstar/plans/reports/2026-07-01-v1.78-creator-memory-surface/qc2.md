---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-07-01-v1.78-creator-memory-surface"
verdict: "Approve"
generated_at: "2026-07-01"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1
- Review Perspective: Security and correctness risk (primary focus per role parameters)
- Report Timestamp: 2026-07-01T01:45:00Z

## Scope
- **plan_id**: `2026-07-01-v1.78-creator-memory-surface` (primary; consolidated review covers full V1.78 Wave 1 = P0 + P1)
- **Review range / Diff basis**: `merge-base: 116296d0 (origin/main)` + `tip: 04a411c2 (iteration/v1.78 HEAD)` — equivalent to `git diff 116296d0...04a411c2`
- **Working branch (verified)**: `iteration/v1.78`
- **Review cwd (verified)**: `/Users/bibi/workspace/organizations/42ch/nexus`
- **Files reviewed**: ~83 (heavy surface: 14 schemas + codegen + handler normalization + 5 NexusClient methods + memory page + DESIGN tokens + P1 slate-clear residuals + tests)
- **Commit range**: 116296d0...04a411c2
- **Tools run**:
  - `git rev-parse --show-toplevel`, `git branch --show-current`, `git rev-parse HEAD` (verified cwd/branch/HEAD)
  - `git diff 116296d0...04a411c2 --stat`
  - `cargo clippy -p nexus-contracts -p nexus-daemon-runtime -- -D warnings` (clean)
  - `pnpm --filter web typecheck` (initial failure due to stale contracts build; resolved after `pnpm --filter @42ch/nexus-contracts run build`; clean post-build)
  - `cargo test -p nexus-daemon-runtime --test memory_dto_roundtrip` (7/7 passed)
  - Deep reads: handlers/memory.rs (auth + limit + delete + summarizer), schemas/*, browser-client.ts (delete transport + memory methods), queries.ts (useActiveCreatorId), memory-detail-panel.tsx (raw_digest render), findings-lifecycle.test.ts (golden adjacency), plan + compass excerpts
- **Deep review**: triggered (≥2 signals: new contract surface + untrusted-content rendering path + authorization surface on every memory route)

## Verification of Review Context Gate
- Cwd: `/Users/bibi/workspace/organizations/42ch/nexus` (git toplevel matches)
- Branch: `iteration/v1.78` (matches Assignment)
- HEAD: `04a411c22252d6f95de398fcf9a0162db6f8e688` (matches Assignment tip)
- Diff basis reproducible via `git diff 116296d0...04a411c2`

## Findings

### 🔴 Critical
None.

### 🟡 Warning
None (unresolved).

**Notes on transient signals (resolved during review, not defects):**
- Initial `pnpm --filter web typecheck` surfaced missing exports from `@42ch/nexus-contracts` (CountPendingReviewsResponse, ListPendingReviewsResponse, PendingReviewInfo, etc.). Root cause: contracts package `dist/` was stale relative to the new generated barrel (0.13.0). After `pnpm --filter @42ch/nexus-contracts run build` the types were present and typecheck passed. No code change required; this is a review-environment build-order artifact, not a shape drift or omission in the changed surface.
- `cargo clippy` and the Rust round-trip test were green on first run.

### 🟢 Suggestion
- Consider adding a first-class `GET /v1/local/creators/active` (or equivalent) so `useActiveCreatorId` does not need to derive from `listSessions({limit:1})`. Current derivation is consistent with existing canvas patterns and the documented single-active-creator model, but a dedicated surface would make the invariant more explicit for future consumers. (Non-blocking; already noted as "future surface" in the queries.ts comment.)
- The raw-digest `<pre>` area in the inspector could benefit from an explicit "copy" affordance for long digests (usability, not security). The current scrollable pre is safe (React text escaping).
- The negative-limit clamp (`resolve_query_limit`) is now stricter (absent → 50, negative → 1). Consider a brief comment in the schema description or handler doc noting the wire behavior for future readers.

## Deep Review Lenses Applied

### Security Lens (Authorization + IDOR)
- **Handler enforcement**: Every memory route (`create_pending_review`, `list_pending_reviews`, `count_pending_reviews`, `delete_pending_review`, `review_memory`, `list_memory_fragments`) calls `read_active_creator_id(state.nexus_home())` and returns `AuthRequired` (401-equivalent) or `Forbidden` (403) on mismatch. The DTO normalization (hand-written structs → generated `nexus_contracts` types) preserved all enforcement sites verbatim. No regression.
- **Frontend derivation (`useActiveCreatorId`)**: Derives from `listSessions({ limit: 1 })[0]?.creator_id`. This matches the documented daemon model (single active creator in config.toml / `~/.nexus42/`). The client still transmits the derived `creator_id` on every call; the server rejects cross-creator attempts. No client-side leak of another creator's data is possible without the daemon itself advertising a wrong active id (out of scope for this surface).
- **Delete transport (`deletePendingReview`)**: Sends `creator_id` as query param (`DELETE /.../{id}?creator_id=...`). Matches the documented route contract (`DeletePendingReviewQuery`). Handler performs two checks: (1) `params.creator_id == active_creator` (403), (2) row's `creator_id == params.creator_id` (403). The double ownership guard plus active-creator context eliminates IDOR for pending reviews. Correct.
- **Schema vs handler caps**: Validation limits (pending_id/session_id/world_id ≤128, raw_digest ≤64KB, task_kind ≤64) remain handler-owned (explicitly documented in the plan and schema descriptions). Schemas are intentionally minimal; no drift introduced.

### Untrusted-Input Lens (raw_digest rendering + summarizer)
- **UI rendering**: `raw_digest` is placed in `<pre>{pending.raw_digest}</pre>` inside `MemoryDetailPanel`. React escapes text content — no `dangerouslySetInnerHTML` or equivalent. Safe.
- **Passthrough summarizer**: Confirmed in `process_review_queue` + `PassthroughSummarizer`:
  - Prepends `# UNTRUSTED: sourced from session_capture digest` + provenance (`creator_id`, `session_id`, `task_kind`, `world_id`, `captured_at`).
  - Truncates at 256 KiB with warning log.
  - Unit tests assert the header and truncation behavior.
- **LLM/agent boundary**: The review path in this wave is purely passthrough (no LLM invocation). No new prompt-injection surface introduced. The `UNTRUSTED` provenance is the documented defense for downstream consumers. Preserved from prior implementation.

### Schema Correctness Lens (optionality + integer width + skip_serializing_if)
- Generated types now use `skip_serializing_if` for `Option<String>` fields (`world_id`, etc.) so `None` is omitted rather than serialized as `null`. Round-trip test explicitly covers "omits_null_world_id".
- Integer width: `count` / `promoted` / `fragmented` / `dropped` moved from `usize` (hand-written) to `i64` (generated). Handler mapping and response construction updated; round-trip test asserts integer shapes.
- Negative `?limit=`: Now clamped to 1 via `resolve_query_limit` (was previously allowing 0 or negative behavior that could reach `.clamp(1, MAX_LIMIT)` in a surprising way). Security-relevant correctness improvement (prevents degenerate pagination).
- All 14 schemas under `schemas/local-api/memory/` match the plan's "authoritative DTO" transcriptions. Codegen barrel exports are present and consumed.

### Status-Transition Correctness Lens (P1 slate-clear residuals)
- Client-side disabled-button guards for invalid status transitions are defense-in-depth.
- Golden adjacency test added in `findings-lifecycle.test.ts` directly mirrors the DAO table in `crates/nexus-local-db/src/findings.rs:172-189` (self-transitions false; terminals have no outbound edges; cross-work invalidation and GRAPH_RELATIONSHIP_CAP tests also added).
- No client/server adjacency drift introduced. Server still returns 422 on invalid transitions.

### State-Transition / Data-Consistency / Injection / Sensitive-Data
- All reviewed paths are creator-scoped; no cross-creator data exposure.
- No path feeds `raw_digest` (untrusted session content) into privileged operations without the UNTRUSTED header + truncation.
- No new injection or path-traversal surfaces.
- `pending_id` / `session_id` etc. are treated as opaque identifiers; format validation (ctr_ prefix) is applied where relevant.

## Source Trace (selected high-signal items)
- Auth enforcement sites: `crates/nexus-daemon-runtime/src/api/handlers/memory.rs:51-59` (create), `191-199` (list), `311-319` (count), `365-376` (delete), `468-476` (review), `696-704` (fragments) — all preserved post-normalization.
- Limit clamp: `resolve_query_limit` + `fetch_pending_reviews_by_creator` (query! + map) — lines ~245-285.
- Delete double-check: lines 368-423 (active match + row ownership).
- Summarizer provenance + truncate: `PassthroughSummarizer::summarize` ~658-676; tests ~789-839.
- UI render: `apps/web/src/components/memory/memory-detail-panel.tsx:81-87` (`<pre>` text child).
- useActiveCreatorId derivation: `apps/web/src/api/queries.ts:436-448`.
- delete transport: `apps/web/src/lib/nexus/browser-client.ts:422-430` (query param via `delete` helper).
- Golden test: `apps/web/src/lib/findings-lifecycle.test.ts:85-109`.
- Round-trip test: `crates/nexus-daemon-runtime/tests/memory_dto_roundtrip.rs` (7 cases).

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 (unresolved) |
| 🟢 Suggestion | 3 (non-blocking) |

**Verdict**: Approve

## Revalidation (if applicable)
N/A — initial wave.

---

**End of QC2 report.**
