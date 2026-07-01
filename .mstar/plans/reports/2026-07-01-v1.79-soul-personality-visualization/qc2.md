---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-07-01-v1.79-soul-personality-visualization"
verdict: "Approve"
generated_at: "2026-07-01"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1 (xai/grok-build-0.1)
- Review Perspective: Security and correctness (seat #2)
- Report Timestamp: 2026-07-01

## Scope
- plan_id: 2026-07-01-v1.79-soul-personality-visualization
- Review range / Diff basis: merge-base: 0015694f (origin/main) .. tip: 37d19d51 (HEAD) — `git diff 0015694f...HEAD`. P1 focus.
- Working branch (verified): iteration/v1.79
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: memory.rs handler + decode_fragment_keywords, memory-fragment-info.schema.json, generated contracts (Rust + TS), soul viz components (soul-panel.tsx, keyword-frequency.tsx, temporal-drift.tsx, soul-stats.ts + .test.ts), memory-page.tsx
- Tools run: git diff 0015694f...HEAD (target paths), cargo test -p nexus-daemon-runtime --test memory_dto_roundtrip, pnpm run validate-schemas, manual source review + grep for injection vectors

## Findings

### 🔴 Critical
(none)

### 🟡 Warning
(none)

### 🟢 Suggestion
- (minor) The four `decode_fragment_keywords` unit tests are present and adequate for the documented degradation cases (valid array, empty, malformed JSON, non-string items). Consider adding a test for deeply nested arrays or very large arrays if production keyword lists can be unbounded, but current behavior (graceful empty) is safe.

## Source Trace
- Finding ID: (N/A — no blocking findings)
- Source Type: manual-reasoning + test execution + schema validation
- Source Reference: crates/nexus-daemon-runtime/src/api/handlers/memory.rs:924 (decode_fragment_keywords), schemas/local-api/memory/memory-fragment-info.schema.json:8 (required), soul-stats.ts:20 (fragmentKeywords guard), handler projection at 888-904
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 1 |

**Verdict**: Approve

## Key Checks Performed (seat #2 — security & correctness)

1. **Additive-only DTO guarantee**
   - Schema `required` remains exactly `["fragment_id", "summary"]`. `keywords` and `created_at` are optional properties.
   - `additionalProperties: false` is present.
   - Generated Rust (`MemoryFragmentInfo`) and TS (`MemoryFragmentInfo.ts`) contain only the four fields; no `session_id`, `creator_id`, or `ttl`.
   - Handler projection (memory.rs:890-904) explicitly constructs only `fragment_id`, `summary`, `keywords: Some(decode...)`, `created_at: Some(...)`. Internal DB row fields are not forwarded.

2. **`keywords` deserialization correctness**
   - `decode_fragment_keywords` uses `serde_json::from_str::<Vec<String>>(raw).unwrap_or_default()`.
   - All failure modes (malformed JSON, empty string, object instead of array, mixed types like `["ok", 42]`) degrade to `[]`.
   - Exactly four unit tests exist in the module and pass (`cargo test -p nexus-daemon-runtime --test memory_dto_roundtrip`):
     - `decode_fragment_keywords_parses_valid_json_array`
     - `decode_fragment_keywords_empty_array`
     - `decode_fragment_keywords_malformed_json_degrades_to_empty`
     - `decode_fragment_keywords_non_string_items_rejected`
   - The roundtrip test `fragments_response_round_trips_keywords_and_created_at` also passes.

3. **Per-creator scoping**
   - Fragments list path still calls `list_fragments_limited(state.pool(), &active_creator, limit_i64)` (or the legacy path with the same creator filter).
   - The projection change adds only read-only metadata fields; it does not alter the creator-scoped query or widen the result set.
   - No cross-creator leakage introduced.

4. **Client-side defensive guard**
   - `soul-stats.ts:fragmentKeywords` performs:
     - `Array.isArray(f.keywords)`
     - `typeof k === 'string' && k.trim().length > 0`
   - Malformed / missing / non-string data degrades to `[]` before any aggregation or rendering.
   - Pure aggregation tests (`soul-stats.test.ts`) cover the guard paths.

5. **Injection / unsafe rendering**
   - No `dangerouslySetInnerHTML`, `innerHTML`, or `ReactMarkdown` usage in any file under `apps/web/src/components/soul`.
   - All keyword / summary / count values are rendered as React text nodes or `title` attributes (safe).
   - Data flows only from the authenticated local fragments endpoint (creator-scoped) into presentational components.

## Additional Observations
- `pnpm run validate-schemas` reports 184/184 valid (including the updated memory-fragment-info.schema.json).
- The DTO extension is strictly additive and backward-compatible for existing consumers that only read `fragment_id` + `summary`.
- Soul visualization components correctly treat `keywords` and `created_at` as optional and handle empty/missing cases with honest empty states or fallbacks.
- No new endpoints, auth surfaces, or privileged operations were added; this is a read-only projection + client aggregation change.

## Revalidation Notes
N/A (initial review for this plan).
