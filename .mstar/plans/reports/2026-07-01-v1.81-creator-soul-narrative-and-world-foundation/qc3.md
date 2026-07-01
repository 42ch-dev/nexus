---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-07-01-v1.81-creator-soul-narrative-and-world-foundation"
verdict: "Request Changes"
generated_at: "2026-07-02"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: volcengine-plan/ark-code-latest
- Review Perspective: Performance and reliability risk (seat 3)
- Report Timestamp: 2026-07-02T01:08:00Z

## Scope
- plan_id: 2026-07-01-v1.81-creator-soul-narrative-and-world-foundation (dual-track wave — also covers P1 2026-07-01-v1.81-soul-surface-deepening)
- Review range / Diff basis: merge-base: 83000ca3 … tip: cb802209 = `git diff 83000ca3...cb802209`
- Working branch (verified): iteration/v1.81
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Verified HEAD: cb802209
- Files reviewed: 68 (per `git diff --stat 83000ca3...cb802209`)
- Commit range: 83000ca3e8870635129b0cbe507f0da93e08be5a...cb8022098e24bc1cc0c4f6aa1151cc0c52eda97a
- Deep review: triggered (S1: 68 files / 3,883 insertions; S2/S4: migration + schema/DDL; S6: crates + schemas + generated contracts + web UI)
- Lenses applied: Performance Lens, Reliability Lens, Data Migration Lens, Testing Lens
- Tools run:
  - `git rev-parse --show-toplevel && git branch --show-current && git rev-parse --short HEAD && git status --short`
  - `git diff --stat 83000ca3...cb802209 && git diff --name-only 83000ca3...cb802209`
  - `gitnexus_detect_changes({ base_ref: "83000ca3", scope: "compare", repo: "nexus" })` → high risk summary (25 changed symbols, 8 affected processes)
  - `cargo clippy -p nexus-daemon-runtime -p nexus-local-db -- -D warnings`
  - `cargo test -p nexus-daemon-runtime -p nexus-local-db`
  - `pnpm --filter web run test`

## Findings

### 🔴 Critical
None.

### 🟡 Warning
- **W-QC3-001 — `/memory/soul/reflect` read/status path scans and decodes every fragment on every read/poll.**
  - **Evidence**: `reflect_soul` computes stats before cache-state branching (`crates/nexus-daemon-runtime/src/api/handlers/memory.rs:1136-1145`), including the `force_regenerate=false` read/status case. `soul_narrative_fragment_stats` then runs `SELECT keywords, created_at FROM memory_fragments WHERE creator_id = ?` with `.fetch_all()` and decodes every JSON keyword array in Rust (`crates/nexus-local-db/src/soul_narrative.rs:106-123`). The frontend polls the narrative query every `SOUL_REFETCH_MS = 30_000` (`apps/web/src/api/queries.ts:668-698`).
  - **Impact**: A creator with a large fragment set pays O(total fragments + total keyword JSON bytes) database/materialization/JSON-decode cost every time the SOUL tab reads status, even when there is a cached current or stale narrative and no LLM call. This violates the P-1 locked performance guard that the read-side stats may decode **bounded** creator keyword rows (`.mstar/plans/2026-07-01-v1.81-prepare-spec-and-contracts.md:191-198`) and leaves the narrative-latency/cost risk only partially mitigated.
  - **Required fix**: Make the read/status stats path bounded or SQL-aggregated. At minimum, split cheap stale stats (`COUNT(*)`, `MAX(created_at)`) into SQL aggregates and cap/limit any Rust JSON keyword decode used for the insufficient-data gate, or maintain a normalized/summary keyword count. Ensure `force_regenerate=false` can return cached `current`/`stale` state without scanning all fragment keyword JSON.
  - **Source Type**: deep-lens: Performance Lens
  - **Confidence**: High

- **W-QC3-002 — Summary truncation slices UTF-8 by byte offset and can panic during synthesis input building.**
  - **Evidence**: `build_soul_narrative_synthesis_input` uses `frag.summary.len() > 280` and `&frag.summary[..279]` (`crates/nexus-daemon-runtime/src/api/handlers/memory.rs:1341-1345`). Rust string slicing requires a valid UTF-8 character boundary; fragment summaries can contain non-ASCII author text. P-1 explicitly requires summaries truncated to 280 Unicode scalar chars (`.mstar/plans/2026-07-01-v1.81-prepare-spec-and-contracts.md:173-185`).
  - **Impact**: A `force_regenerate=true` request can panic while building the capped LLM input if the 279th byte falls inside a multi-byte character. That converts an ordinary author text input into a request/task failure and bypasses the intended `NexusApiError` degradation path.
  - **Required fix**: Truncate by characters, not bytes, e.g. collect `summary.chars().take(279)` and append `…` only when truncation occurred (keeping the resulting string within the intended scalar-character cap), or use an existing safe truncate helper if present. Add a regression test with a multi-byte summary longer than the cap.
  - **Source Type**: deep-lens: Reliability Lens
  - **Confidence**: High

### 🟢 Suggestion
- **S-QC3-001 — Consider avoiding duplicate observers for the same whole-fragments query.** `SoulSection` mounts both `wholeFragments` and `activeFragments`; when `selectedWorld === null` they share the same key and 30s interval (`apps/web/src/components/memory/soul-section.tsx:55-68`). TanStack should dedupe in-flight fetches, but a single query object reused for the all-world active view would make the polling cost boundary more obvious.
- **S-QC3-002 — Add MSW coverage for the read-shaped POST narrative endpoint in broader Memory/SOUL tests.** `pnpm --filter web run test` passed, but several existing Memory/SOUL tests logged unhandled `POST /v1/local/memory/soul/reflect` requests. This did not fail the suite, but it weakens observability of accidental background/status calls in tests.

## Source Trace
- Finding ID: W-QC3-001
  - Source Type: deep-lens: Performance Lens
  - Source Reference: `crates/nexus-daemon-runtime/src/api/handlers/memory.rs:1136-1145`, `crates/nexus-local-db/src/soul_narrative.rs:106-123`, `apps/web/src/api/queries.ts:668-698`, P-1 plan §2.5#1
  - Confidence: High
- Finding ID: W-QC3-002
  - Source Type: deep-lens: Reliability Lens
  - Source Reference: `crates/nexus-daemon-runtime/src/api/handlers/memory.rs:1341-1345`, P-1 plan §2.4#3
  - Confidence: High
- Finding ID: S-QC3-001
  - Source Type: manual-reasoning
  - Source Reference: `apps/web/src/components/memory/soul-section.tsx:55-68`
  - Confidence: Medium
- Finding ID: S-QC3-002
  - Source Type: test-output
  - Source Reference: `pnpm --filter web run test` stderr unhandled MSW requests for `POST /v1/local/memory/soul/reflect`
  - Confidence: Medium

## Performance/Reliability Notes
- **Narrative synthesis latency/cost**: The actual ACP prompt input is capped (≤30 keywords, ≤24 summaries, ≤8 buckets) and uses `list_fragments_limited(..., 100)`, so no raw session digests or unbounded raw fragment bodies are sent to `acp.prompt`. `force_regenerate=false` returns cached `current`/`stale`/`ungenerated` without an ACP call, but still triggers the unbounded stats scan noted in W-QC3-001.
- **On-demand discipline**: No background LLM job was introduced; synthesis remains synchronous and explicit through `force_regenerate=true`.
- **DAO world filter**: The migration adds `(creator_id, world_id, created_at DESC)`, and dynamic world filters are parameterized with `// SAFETY:` comments in `list_fragments_limited` / `list_fragments_filtered`.
- **Threading reliability**: The review fragment path passes pending `world_id`; standalone creator capability writes intentionally use `None`.
- **Frontend polling**: `SOUL_REFETCH_MS = 30_000` is reasonable and background refetching is not enabled. Query invalidation after review/reflect is present.
- **Degradation/observability**: Synthesis errors map to `NexusApiError`; the main reliability gap found is the UTF-8 truncation panic path.

## Validation Evidence
- `cargo clippy -p nexus-daemon-runtime -p nexus-local-db -- -D warnings` — passed (after waiting for build-dir lock).
- `cargo test -p nexus-daemon-runtime -p nexus-local-db` — passed; final visible summary included `nexus-daemon-runtime` 354 unit tests passed, daemon integration/doc tests passed, `nexus-local-db` 289 unit tests plus integration/doc tests passed.
- `pnpm --filter web run test` — passed: 49 test files, 379 tests. Non-fatal stderr included React Router future-flag warnings, existing `act(...)` warnings, and unhandled MSW requests for `POST /v1/local/memory/soul/reflect`.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 2 |

**Verdict**: Request Changes

## Evidence (Completion Report v2 inputs)
- Verified branch + HEAD: `iteration/v1.81` @ `cb802209`
- Review range: `git diff 83000ca3...cb802209`
- Report path: `.mstar/plans/reports/2026-07-01-v1.81-creator-soul-narrative-and-world-foundation/qc3.md`
- No application-code edits performed.

## Revalidation

- Date: 2026-07-02
- Review range / Diff basis: fix-wave `git diff faae53de..d55ec4f3` (fix commit `e8d135cb`)
- Working branch / HEAD verified: `iteration/v1.81` @ `d55ec4f3`
- Re-checked files: `crates/nexus-local-db/src/soul_narrative.rs`, `crates/nexus-daemon-runtime/src/api/handlers/memory.rs`, `.sqlx/query-*.json`, and the new/changed daemon-runtime tests in the fix wave.
- Validation run: `cargo test -p nexus-daemon-runtime -p nexus-local-db` — passed. Final visible summaries included `nexus-daemon-runtime` unit/integration/doc tests passing (360 unit tests; all listed integration suites passed, including `memory_review_fragments_api` 29 tests) and `nexus-local-db` unit/integration/doc tests passing (289 unit tests plus listed integration/doc tests).

### Prior finding dispositions

- **W-QC3-001 / R-V181P0-QC3-W001 — Resolved for the original unbounded read-side scan.**
  - Evidence: `soul_narrative_fragment_stats` now computes `fragment_count` with `SELECT COUNT(*)` and `max_created_at` with `SELECT MAX(created_at)` via `query_scalar!`, and the only remaining Rust JSON keyword decode is behind `SELECT keywords ... ORDER BY created_at DESC LIMIT ?` with `KEYWORD_SCAN_CAP = 200` (`crates/nexus-local-db/src/soul_narrative.rs:111-139`).
  - The `reflect_soul` cached read/status path still computes stats before cache branching and retains the same gate/stale branches (`fragment_count < 10 || distinct_keyword_count < 20`; stale compares generation count + max timestamp), but it no longer materializes/decodes all keyword rows for cached `force_regenerate=false` reads (`crates/nexus-daemon-runtime/src/api/handlers/memory.rs:1136-1234`).

- **W-QC3-002 / R-V181P0-QC3-W002 — Resolved.**
  - Evidence: `truncate_summary()` uses `summary.chars().count()` and `summary.chars().take(max_chars - 1).collect()` before appending `…`, so it truncates on Unicode scalar boundaries rather than byte offsets (`crates/nexus-daemon-runtime/src/api/handlers/memory.rs:1312-1324`, used at `1355-1358`).
  - Regression tests exist for short/exact/ASCII-over-limit/CJK/emoji/empty cases (`crates/nexus-daemon-runtime/src/api/handlers/memory.rs:1587-1634`), including multi-byte cases that would have panicked under the old `&summary[..279]` byte slice.

### New issue found during revalidation

- **W-QC3-003 — The bounded keyword scan can under-count the insufficient-data gate and return `insufficient_data` for creators that actually meet the `current_distinct_keyword_count >= 20` contract.**
  - **Evidence**: The fix scans only the newest 200 keyword rows (`ORDER BY created_at DESC LIMIT 200`) and uses that capped set as `distinct_keyword_count` (`crates/nexus-local-db/src/soul_narrative.rs:132-153`). `reflect_soul` then applies the locked gate directly to that value before cache lookup (`crates/nexus-daemon-runtime/src/api/handlers/memory.rs:1147-1167`). The P-1 contract defines `current_distinct_keyword_count` as the current distinct keyword count across all Creator fragments and gates on `< 20` (`.mstar/plans/2026-07-01-v1.81-prepare-spec-and-contracts.md:191-198`).
  - **Why the `LIMIT 200` bound is not semantically sound**: Seeing `>= 20` distinct keywords in the capped set is sufficient to pass the gate, but seeing `< 20` in the newest 200 rows does **not** prove the whole Creator has `< 20` distinct keywords. A creator with 200 recent same-keyword fragments and 20 older distinct-keyword fragments would be misclassified as `insufficient_data`, including on a cached read before the endpoint can return an existing `current` or `stale` narrative.
  - **Impact**: False `insufficient_data` responses can hide an existing cached narrative and block regeneration for large creators whose diversity is outside the newest 200 fragments. This is a correctness/reliability regression introduced while fixing the original performance warning.
  - **Required fix**: Preserve the locked gate semantics while avoiding unbounded Rust materialization/decode. Options include an exact SQL-side distinct keyword aggregate over all creator rows (e.g. SQLite JSON1/normalized keyword table) or another approach that only returns `< 20` after proving fewer than 20 distinct keywords across the full creator set. Add a regression test for a creator whose newest capped rows have `<20` distinct keywords but older rows bring the full set to `>=20`.
  - **Source Type**: manual-reasoning / diff revalidation
  - **Confidence**: High

### Revalidation verdict

**Verdict**: Request Changes. The original unbounded scan and UTF-8 panic findings are resolved, and the required Rust test command passes, but the W001 fix-wave introduced a new blocking Warning (`W-QC3-003`) because the capped newest-200 keyword scan can under-count the locked insufficient-data gate.

## Revalidation

- Date: 2026-07-02
- Review range / Diff basis: round-2 fix `git diff 3868833b..35797fb0` (fix commit `d38ea098`, merge tip `35797fb0`)
- Working branch / HEAD verified: `iteration/v1.81` @ `35797fb0`
- Re-checked files: `crates/nexus-local-db/src/soul_narrative.rs`, `crates/nexus-local-db/tests/soul_narrative_keyword_count.rs`, `crates/nexus-local-db/Cargo.toml`, `Cargo.lock`, `.sqlx/query-488cf36d2a882d7ce813543a40b7d6c6e49be02e61e346c293a45ae02e466acf.json`, and the cached read/status call path in `crates/nexus-daemon-runtime/src/api/handlers/memory.rs`.
- Validation run: `cargo test -p nexus-local-db -p nexus-daemon-runtime` — passed. The new `soul_narrative_keyword_count` integration suite ran 5/5 tests successfully (`distinct_keywords_at_least_20_across_many_fragments`, `distinct_keywords_below_20_gate_fails`, `distinct_keywords_exactly_20_gate_passes`, `distinct_keywords_with_duplicates_still_sound`, `no_fragments_zero_distinct`), with the broader crate test command also passing (`nexus-daemon-runtime` unit/integration/doc tests and `nexus-local-db` unit/integration/doc tests).

### Round-2 disposition

- **W-QC3-003 / R-V181P0-QC3-W003 — Resolved for soundness.**
  - Evidence: the old `ORDER BY created_at DESC LIMIT ?` + `.fetch_all()` path is gone. `soul_narrative_fragment_stats` now uses a compile-time checked `sqlx::query!` with no `LIMIT`, streams rows via `.fetch(pool)` + `TryStreamExt::try_next()`, and accumulates keywords into a `HashSet<String>` (`crates/nexus-local-db/src/soul_narrative.rs:135-170`). For `<20` cases the stream reaches EOF before returning, so the failing gate is exact. For `>=20` cases the code reaches the threshold authoritatively, then drains the remaining stream and continues decoding before returning, so `current_distinct_keyword_count` remains exact rather than `>=20`-only semantics. This removes the false `insufficient_data` under-count from the newest-200 window.
  - Coverage: the new local-db regression tests cover `>=20` across many fragments, `<20`, exact boundary at 20, duplicates, and empty creators (`crates/nexus-local-db/tests/soul_narrative_keyword_count.rs`).

### Blocking regression found during round-2 revalidation

- **W-QC3-001 regression — cached read/status once again performs an unbounded keyword JSON decode scan.**
  - **Evidence**: `reflect_soul` still calls `soul_narrative_fragment_stats` before cache lookup and before branching on `force_regenerate` (`crates/nexus-daemon-runtime/src/api/handlers/memory.rs:1136-1170`). The round-2 stats function no longer materializes all rows with `fetch_all()`, but because it drains the remaining stream after reaching 20 distinct keywords in order to return an exact `current_distinct_keyword_count`, it still reads and decodes every matching `memory_fragments.keywords` JSON row before any cached `current`/`stale` response can return (`crates/nexus-local-db/src/soul_narrative.rs:146-165`). Thus `force_regenerate=false` cached reads still pay O(total creator fragments + total keyword JSON bytes) CPU/I/O work on every poll; only peak memory materialization was improved.
  - **Impact**: This reintroduces the core performance/reliability risk from W-QC3-001 for large creators and violates the round-2 no-regression requirement that the cached common path not trigger an unbounded decode scan unnecessarily. W-QC3-003 is sound, but the chosen exact-response implementation restores the unbounded read-side cost.
  - **Required fix direction**: keep the W-QC3-003 soundness property while avoiding full keyword decode on cached `force_regenerate=false` reads. Viable approaches include caching/maintaining an exact distinct keyword summary, documenting and implementing `current_distinct_keyword_count` as threshold-capped `>=20` semantics if the wire contract allows it, splitting the gate from the exact response field, or moving exact distinct counting to a bounded/indexed SQL-side representation rather than per-poll Rust JSON decode.
  - **Source Type**: manual-reasoning / diff revalidation
  - **Confidence**: High

### Dependency assessment

- `futures-util` is justified for `TryStreamExt::try_next()` on the `sqlx` stream and is workspace-consistent: the workspace already declares `futures-util = "0.3"`, other crates consume it via `{ workspace = true }`, and `nexus-local-db` now follows that pattern rather than introducing a new version.

### Revalidation verdict

**Verdict**: Request Changes. W-QC3-003 is resolved as a soundness bug and the 5 new regression tests pass, but round-2 reintroduced/continued the W-QC3-001 unbounded keyword JSON decode cost on cached read/status calls, so this targeted re-review cannot flip to Approve.
