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
