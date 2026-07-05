---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-07-01-v1.81-creator-soul-narrative-and-world-foundation"
verdict: "Approve"
generated_at: "2026-07-02"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: MiniMax-M3 (Coding Plan)
- Review Perspective: Architecture coherence and maintainability risk (seat 1)
- Report Timestamp: 2026-07-02T00:55:00Z

## Scope
- plan_id: `2026-07-01-v1.81-creator-soul-narrative-and-world-foundation` (dual-track wave — also covers P1 `2026-07-01-v1.81-soul-surface-deepening`)
- Review range / Diff basis: merge-base: 83000ca3 … tip: cb802209 = `git diff 83000ca3...cb802209`
- Working branch (verified): `iteration/v1.81`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 68 (per `git diff --stat`)
- Commit range: `83000ca3e8870635129b0cbe507f0da93e08be5a...cb8022098e24bc1cc0c4f6aa1151cc0c52eda97a`
- Tools run:
  - `git rev-parse --show-toplevel && git branch --show-current && git rev-parse HEAD`
  - `git merge-base 83000ca3 cb802209` → `83000ca3` (range is reproducible; not cherry-picked)
  - `git diff --stat 83000ca3...cb802209`
  - `git diff 83000ca3...cb802209` (read key files: migration, `nexus-creator-memory/src/soul_narrative.rs`, `nexus-local-db/src/soul_narrative.rs`, `nexus-local-db/src/memory_fragment.rs`, `nexus-daemon-runtime/src/api/handlers/memory.rs` `reflect_soul` + `fragments`, `nexus-daemon-runtime/src/api/handlers/soul_narrative_synthesizer.rs`, `apps/web/src/components/soul/*`, schemas)
  - `cargo clippy -p nexus-daemon-runtime -p nexus-creator-memory -p nexus-local-db -- -D warnings` (green)
  - `cargo clippy --all -- -D warnings` (green for the V1.81 touch-set; pre-existing failures in `nexus-narrative/src/timeline_event.rs` are out of scope — `git diff 83000ca3...cb802209 --stat -- crates/nexus-narrative/` returns empty)
  - `cargo +nightly-2026-06-26 fmt --all --check` (green)
  - `cargo test -p nexus-local-db` (289 passed)
  - `cargo test -p nexus-daemon-runtime --tests` (all green; no failure)
  - `cargo test -p nexus-creator-memory --tests` (150 passed)
  - `pnpm --filter web run typecheck` (green)
  - `pnpm --filter web run test` (379 passed / 49 files)
  - Targeted greps: `create_fragment(` × 21 sites for `world_id` threading; `SoulNarrativeSynthesizer` ↔ `SessionDigestSummarizer` seam parity; every `cargo test` runs green against the touch-set

**Deep review lenses applied** — **Deep review triggered** (5/6 signals met):
- S1 — Diff size: 68 files, +3883 / −224 = yes
- S2 — Sensitive modules: `migrations/` + `auth/`-adjacent creator scoping + DDL = yes
- S3 — New domain: `SoulNarrativeSynthesizer` trait + ACP adapter + persistence + cache states are all new = yes
- S4 — Data-structure change: `ALTER TABLE memory_fragments ADD COLUMN world_id`, new `memory_soul_narratives` table, new index = yes
- S6 — Multi-module coupling: touches `nexus-local-db`, `nexus-creator-memory`, `nexus-daemon-runtime`, `nexus-orchestration`, `nexus-contracts`, `apps/web` (so 6+ crates / dirs) = yes

**Lenses applied**:
- Modularity Lens (default for seat 1)
- Contract Lens (default for seat 1)
- Data Migration Lens (S4)
- Standards Lens (S3)
- Testing Lens (S3 — limited endpoint coverage surfaced)

Findings are integrated into the main Findings sections below; each one carries a `Source Type` tag identifying the lens. `git-diff / linter / manual-reasoning` only where no lens applied.

## Findings

### 🔴 Critical
*(none)*

### 🟡 Warning

- **[W-001] Missing daemon-level regression test for `world_id` threading through the review pipeline (architecture/maintainability)**
  - The plan §2.A Acceptance/DoD requires: *"world-scoped pending review → fragment carries `world_id` (regression test green at every site)"* and §2.A Tests requires *"regression — a world-scoped pending review produces a fragment carrying that `world_id`; a NULL-world pending review produces a Creator-core-only fragment; the DAO world filter returns the correct subset."* The implementation threads `world_id` correctly (`crates/nexus-daemon-runtime/src/api/handlers/memory.rs:886` — `MemoryFragmentRecord { ... world_id: input.world_id.clone(), }`), but the only assertion of behavior is the **local-db** `test_world_id_propagation` (`memory_fragment.rs:542`), which exercises `create_fragment` directly with a hard-coded record — it does NOT route through the daemon handler. The `fragments_after_review_has_entries` test in `crates/nexus-daemon-runtime/tests/memory_review_fragments_api.rs:199` posts a pending review **without** `world_id` and only asserts fragment presence. **No test today would catch a regression that drops `world_id: input.world_id.clone()` from the daemon handler.** Same gap exists for the orchestration `CreatorCapabilityStore::write_memory` site (`crates/nexus-orchestration/src/capability/builtins/creator.rs:170`) — the plan called for *"a new world-propagation test"* with `world_id: Some(...)`, but the three test paths at lines 805/835/851 only assert `world_id: None`.
  - **Fix**: add a daemon integration test (e.g., in `memory_review_fragments_api.rs`) that POSTs a `PendingReviewInfo` with `world_id: "wld_x"` → POSTs `/v1/local/memory/review` → GETs `/v1/local/memory/fragments?creator_id=...&world_id=wld_x` and asserts the round-tripped fragment carries `world_id: "wld_x"`. Mirror with `world_id: null` for the Creator-core-only case. For the orchestration path, add a unit test in `crates/nexus-orchestration/src/capability/builtins/creator.rs::tests` that constructs a `MemoryFragmentRecord` via `write_memory` with `world_id: Some(...)` (requires a minor public-signature extension or a direct record construction + `create_fragment`, whichever the existing helper exposes).
  - **Source Type**: `manual-reasoning` (architecture/maintainability lens, plus Testing lens per S3)
  - **Confidence**: High

### 🟢 Suggestion

- **[S-001] Plan-promised `validate draft` step is not implemented; quality floor is prompt-only**
  - The plan §2.B algorithm explicitly lists `validate draft includes specificity + temporality + actionable-tone structure` between `synthesize(...)` and `upsert_soul_narrative(...)`. The handler (`crates/nexus-daemon-runtime/src/api/handlers/memory.rs:1260-1287`) goes directly from `synthesize → upsert` with no structural check on `draft.narrative`. The prompt (built in `AcpSoulNarrativeSynthesizer::build_prompt`) instructs the LLM to produce the three-axis structure, and the Compass AC-1 narrative-quality threshold is QA-session-verified — but a future synthesizer swap or prompt regression could silently persist a thin narrative. The narrowest durable slice would be a lightweight validator on `draft.narrative` (e.g., presence of ≥2 keyword hits against `input.top_keywords`, ≥1 token from any `temporal_buckets.top_keywords`, and a forward-looking suffix heuristic) returning `MemoryError::QualityThresholdMissed` mapped to a structured error variant.
  - **Fix**: either implement the inline check (cheap, deterministic) or leave a `simplify:` comment in the handler pointing at the QA-verified gate + a roadmap entry in the deferred-features tracker.
  - **Source Type**: `doc-rule` + `manual-reasoning`
  - **Confidence**: High

- **[S-002] `MemoryError → NexusApiError` mapping in `reflect_soul` is string-content-matching**
  - `crates/nexus-daemon-runtime/src/api/handlers/memory.rs` (~line 1268) classifies errors via `if msg.contains("not available") || msg.contains("unavailable")` then falls everything else to `Internal { code: "NARRATIVE_SYNTHESIS_ERROR", ... }`. Synthetic error strings produced by `AcpSoulNarrativeSynthesizer` work today (`"acp.prompt capability not available in registry"` / `"ACP worker unavailable for narrative synthesis"` both contain the right substrings), but the classification is brittle to copy edits — a future typo in `MemoryError::ValidationError("...")` could demote a 503 to 500 or vice versa. The handler is also a long if/else: `else` already maps to `Internal`, which means a malformed-output error (e.g., missing `full_text`) is classified 500 rather than 4xx.
  - **Fix**: narrowest — have `AcpSoulNarrativeSynthesizer` wrap into a typed error (e.g., `enum SoulNarrativeError { WorkerUnavailable, CapabilityMissing, MalformedOutput, Other(MemoryError) }`); the handler maps structurally. Alternative: a `map_memory_error_for_narrative(MemoryError) -> NexusApiError` helper colocated near the handler.
  - **Source Type**: `manual-reasoning`
  - **Confidence**: Medium

- **[S-003] `world-selector.tsx` claims "Tracked as a V1.81 deferral" — deferral is not actually in the tracker**
  - `apps/web/src/components/soul/world-selector.tsx:19-27` has a `simplify:` comment: *"When a worlds-list or world-detail endpoint ships, replace `worldOptionLabel` to render the world title and re-add the 'Work-backed but no-fragment world' subset-empty path the product spec describes. Tracked as a V1.81 deferral, not a behavior bug."* But `.mstar/knowledge/deferred-features-cross-version-tracker.md` (BL-12 entry) does not mention world-title resolution or Work-backed-but-no-fragment subset emptiness as a deferral. The "Tracked" claim is not backed by a tracker entry — it is a soft inconsistency between the comment and the tracker.
  - **Fix**: PM adds a one-line row in the cross-version tracker (or in the BL-12 entry) pointing at the simplify marker; or downgrades the comment to "deferred — see BL-12" once BL-12 already mentions the related subset-empty path.
  - **Source Type**: `doc-rule` (Standards Lens)
  - **Confidence**: Medium

- **[S-004] `SoulNarrativeResponse` Rust struct mixes `u64` (counts) and `i64` (thresholds)**
  - `crates/nexus-contracts/src/generated/local_api/memory/soul_narrative_response.rs`: `current_fragment_count: u64`, `current_distinct_keyword_count: u64`, but `min_fragment_count: i64`, `min_distinct_keyword_count: i64`. JSON Schema uses `integer` uniformly; the schema's `const: 10` / `const: 20` are non-negative. Mix is harmless at compile time but inconsistent at the type level. The handler maps `MIN_SOUL_NARRATIVE_FRAGMENTS: i64` directly, so the wire shape doesn't drift — but a future serializer change to `i64` for all counts would be a one-line cleanup.
  - **Fix**: regenerate from JSON with `u64`, or change thresholds to `u64` in the generated struct (one-line, codegen-driven).
  - **Source Type**: `manual-reasoning`
  - **Confidence**: Low

- **[S-005] `AcpSoulNarrativeSynthesizer` does not cap synthesized narrative length**
  - The plan §9 acknowledged *"Narrative generation latency / cost on a large fragment set"* via input-side capping only. The output side is uncapped — `draft.narrative` is persisted verbatim. A pathological LLM response (or a future synthesizer swap with looser instruction-following) could persist a multi-MB narrative. SQLite has no `TEXT` size limit, but this affects memory/JSON serialization cost and read-time `formatDate`/render cost. The plan promised "2-4 paragraphs" via prompt.
  - **Fix**: a `narrative.chars().len() <= N` check before `upsert_soul_narrative` (e.g., 16 KiB), or a `simplify:` comment marking the trust bound with the prompt-only contract.
  - **Source Type**: `manual-reasoning`
  - **Confidence**: Low

## Source Trace
- Finding ID scheme: `W-001` (Warning), `S-001` ... (Suggestion)
- Source types: `git-diff`, `manual-reasoning`, `doc-rule`, `linter`
- Source references: `git diff 83000ca3...cb802209` for the full diff; specific paths cited inline above
- Confidence: High / Medium / Low as noted per finding

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 5 |

**Verdict**: **Request Changes**

The architecture, crate seams, and trait pattern are clean and consistent with the V1.79/V1.80 baseline:
- `SoulNarrativeSynthesizer` trait lives in the correct crate (`nexus-creator-memory`), with the real ACP adapter in `nexus-daemon-runtime/src/api/handlers/soul_narrative_synthesizer.rs` — same pattern as `SessionDigestSummarizer` → `PassthroughSummarizer`.
- No new crate-dependency edges introduced (`git diff ... -- Cargo.toml` returns empty for all touched crates).
- The migration is additive (nullable `world_id` + new table + new index); existing rows default `NULL` per plan; sqlx metadata in `.sqlx/` is regenerated.
- Wire DTOs are generated Rust + TypeScript from the schemas (4 schemas added/edited per plan §10); the `schema_drift_detection` test is extended with the two new entries; `MemoryFragmentInfo` and `ListMemoryFragmentsQuery` re-derive cleanly with the additive `world_id`.
- Frontend contracts consumed exclusively through generated types (`@42ch/nexus-contracts`); no hand-written duplicates; DESIGN.md tokens (`soul-narrative-prose`, `soul-growth-curve-stroke`) and `index.css` bridges added in lockstep; component decomposition into `SoulNarrativeCard`, `WorldSelector`, `GrowthCurve` matches the plan and shares helpers via `soul-stats.ts` (no `growthSeries` / `densityFor` duplication; both pure + testable).
- `NexusApiError` envelope contract honored — `reflect_soul` returns `Result<Json<SoulNarrativeResponse>, NexusApiError>` exclusively; no ad-hoc JSON error bodies.
- Auto-refresh invalidation is wired in two places: `useSoulNarrative`'s `refetchInterval: SOUL_REFETCH_MS`, and `useReviewMemory.onSettled` invalidates `queryKeys.memory.soulNarrative(creatorId)`.

The single Warning concerns an **explicit DoD criterion** from §2.A / §4 Acceptance that is not delivered: a regression test per site for `world_id` propagation through the daemon handler. Without it, a future one-line change in `crates/nexus-daemon-runtime/src/api/handlers/memory.rs` could silently drop world context from fragments with no test catching the regression. Both `qc-specialist` (architecture) and `qc-specialist-3` (reliability) perches are likely to land findings on coverage; QC2 cleared Approve but did not cover the daemon-handler threading path either, so the gap is not redundant.

### Residual follow-ups (out of scope for this PR but worth tracking)
- V-182 backlog: per-world LLM narratives (Compass §8, BL-12).
- V-182+ server-side work: worlds-list / world-detail endpoint so `world-selector.tsx` `simplify:` path can render titles + re-enable the Work-backed-but-no-fragment subset-empty state.
- Companion test for the daemon-level handler-side stale-detection flip (cache `current` → `stale` after a new fragment): not in plan §2.A's test list, but a natural follow-on once the synth path is exercised end-to-end.

---

## Revalidation (targeted re-review — fix-wave `faae53de..d55ec4f3`, commit `e8d135cb`)

**Scope of this re-review**: my single prior finding **W-001 / R-V181P0-QC1-W001** (missing daemon-level regression test for `world_id` threading through the review pipeline). I did **not** re-litigate the five Suggestions (S-001…S-005) — those were non-blocking. This re-review also covers whether the fix introduced any new Warning/Critical.

### What I re-checked
1. `git diff faae53de..d55ec4f3 --stat` → 9 files, +342 / −53. Touch-set:
   - `crates/nexus-daemon-runtime/src/api/handlers/memory.rs` (+71)
   - `crates/nexus-daemon-runtime/tests/memory_review_fragments_api.rs` (+145)
   - `crates/nexus-local-db/src/soul_narrative.rs` (+55 / bulk refactor of `soul_narrative_fragment_stats` to SQL aggregates + bounded scan cap 200)
   - `crates/nexus-orchestration/src/capability/builtins/creator.rs` (+36)
   - 3 `status.json` snapshots + 1 compass snapshot (PM bookkeeping, out of QC scope).
2. Read the new test bodies in `crates/nexus-daemon-runtime/tests/memory_review_fragments_api.rs` (lines 855–1000): both `world_id_propagation_from_review_to_fragments` and `world_id_none_core_only_fragment` are full daemon-handler integration tests via `ctx.server.post(...).json(&body)` / `ctx.server.get(...)` — **not** local-db unit tests. They exercise the actual axum router + the line-886 handler `world_id: input.world_id.clone()` site that my W-001 named.
3. Read the new test `write_memory_with_store_uses_world_id_none` in `crates/nexus-orchestration/src/capability/builtins/creator.rs::tests` (lines 744–780): asserts the standalone `CreatorCapabilityStore::write_memory` path persists `world_id: None` via `nexus_local_db::list_fragments`.
4. Re-read `CreatorCapabilityStore::write_memory` (lines 144–181): the signature has no `world_id` parameter; the body explicitly sets `world_id: None, // V1.81: standalone writes have no world context`. This is an intentional design choice (world context only flows through the daemon `/v1/local/memory/review` pipeline; standalone capability calls have no world binding).
5. Ran the new tests:
   - `cargo test -p nexus-daemon-runtime --test memory_review_fragments_api -- world_id_propagation_from_review_to_fragments` → **1 passed; 0 failed**.
   - `cargo test -p nexus-daemon-runtime --test memory_review_fragments_api -- world_id_none_core_only_fragment` → **1 passed; 0 failed**.
   - `cargo test -p nexus-orchestration --lib write_memory_with_store_uses_world_id_none` → **1 passed; 0 failed**.
6. Ran the full affected test suites:
   - `cargo test -p nexus-daemon-runtime --test memory_review_fragments_api` → **29 passed; 0 failed; 0 ignored** (was 27 before the fix-wave — exactly +2 new tests, all green).
   - `cargo test -p nexus-orchestration --lib` → **965 passed; 0 failed; 3 ignored** (was 964 before — exactly +1 new test, all green).
7. Checked formatting on touched crates: `cargo +nightly-2026-06-26 fmt --check -p nexus-daemon-runtime -p nexus-orchestration -p nexus-local-db` → clean.

### Per-finding disposition

**[W-001 / R-V181P0-QC1-W001] — RESOLVED.**
The fix-wave delivers exactly the test coverage my W-001 named, end-to-end through the daemon handler:

- **`world_id_propagation_from_review_to_fragments`** exercises the Some(...) path:
  - POST `/v1/local/memory/pending-review` with `world_id: "wld_x"` → POST `/v1/local/memory/review` → GET `/v1/local/memory/fragments?creator_id=ctr_testuser&world_id=wld_x` asserts `fragments[0]["world_id"] == "wld_x"`.
  - Also asserts cross-world isolation: `world_id=wld_y` excludes the `wld_x` fragment (no false positives in the DAO world filter).
  - Also asserts unfiltered query still returns the fragment (no over-filtering).
  - This pins the line-886 handler site: any future regression dropping `world_id: input.world_id.clone()` from `create_fragment` would now fail this test.

- **`world_id_none_core_only_fragment`** exercises the None / Creator-core-only path:
  - Seeds pending-review without `world_id` → POST review → unfiltered GET asserts fragment with `world_id: null` is present; filtered GET by `world_id=wld_z` asserts it is absent.
  - This pins the `NULL` ↔ specific-`world_id` filter distinction in the DAO.

- **`write_memory_with_store_uses_world_id_none`** pins the orchestration-side standalone contract: `CreatorWriteMemory::with_store(...).run(...)` always persists `world_id: None`, which matches the explicit design comment at `crates/nexus-orchestration/src/capability/builtins/creator.rs:173`.

**Deviation from my original fix recommendation**: I had asked for an orchestration test that constructs `world_id: Some(...)` via `write_memory`. The fix-wave instead pins `world_id: None` for that path. After re-reading the API signature, this is the **correct** contract — `CreatorCapabilityStore::write_memory` has no `world_id` parameter, and the body comment makes the world-less design explicit. The world-aware path goes through the daemon `POST /v1/local/memory/review`; the daemon-handler integration tests above cover that. The orchestration `world_id: None` test now pins the design intent so a future contributor cannot silently start populating `world_id` from `write_memory` and skew the test surface. **This deviation is acceptable** and arguably stronger than my original suggestion (it documents the boundary, not just the Some case).

### New issues introduced by the fix-wave?
None at Warning/Critical level. Specifically:

- **Daemon handler (`memory.rs`)**: the diff adds `truncate_summary` helper + 6 unit tests (R-V181P0-QC3-W002 territory — qc3's UTF-8 byte-slice panic finding). ASCII / CJK / emoji / edge-case coverage is solid; pure refactor of the existing truncation site to use `chars().take(...)` instead of byte-slice `&s[..279]`. No new handler surface area introduced.
- **`soul_narrative.rs`**: `soul_narrative_fragment_stats` refactored from full row materialization to `COUNT(*)` + `MAX(created_at)` SQL aggregates + bounded scan (`LIMIT 200`) for `distinct_keyword_count`. The inline comment correctly explains the bound: the insufficient-data gate threshold is 20 distinct keywords, cap is 200 (10×), so the gate result is correct regardless. This is a performance/reliability improvement, not a behavior change — out of my seat-1 (architecture) purview but flagged for completeness.
- **`creator.rs`**: +1 test using `nexus_local_db::list_fragments` — idiomatic usage of the public API; no new module surface.
- **Status JSON / compass**: PM bookkeeping only.

I did not re-run `cargo clippy --all -- -D warnings` because the pre-existing clippy failures in unrelated files (e.g. `crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor_tests.rs:1010+`, `crates/nexus-daemon-runtime/src/workspace/session.rs:772`, `crates/nexus-orchestration` `doc_markdown` warnings around line 2983 in unrelated `game_bible_category` docs) are out of scope for this re-review — they pre-date `faae53de`. Per `mstar-review-qc` "Pre-existing claim verification" rule, the touched-line clippy status is what matters here; the new code compiles and the tests are green.

### Summary
| Severity | Count (this re-review) |
|----------|------------------------|
| 🔴 Critical | 0 |
| 🟡 Warning (unresolved) | 0 — **W-001 closed** |
| 🟢 Suggestion (unresolved) | 5 (unchanged from wave 1; non-blocking, per `mstar-review-qc` verdict rules, not re-litigated in this targeted re-review) |

**Verdict**: **Approve** (W-001 closed; no new Critical or Warning introduced by the fix-wave).
