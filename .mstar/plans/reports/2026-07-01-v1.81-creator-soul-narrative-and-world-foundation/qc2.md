---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-07-01-v1.81-creator-soul-narrative-and-world-foundation"
verdict: "Approve"
generated_at: "2026-07-02"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: Security and correctness risk (seat 2)
- Report Timestamp: 2026-07-02T00:52:00Z

## Scope
- plan_id: 2026-07-01-v1.81-creator-soul-narrative-and-world-foundation (dual-track wave — also covers P1 2026-07-01-v1.81-soul-surface-deepening)
- Review range / Diff basis: merge-base: 83000ca3 … tip: cb802209 = `git diff 83000ca3...cb802209`
- Working branch (verified): iteration/v1.81
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 68 (per `git diff --stat`)
- Commit range: 83000ca3e8870635129b0cbe507f0da93e08be5a...cb8022098e24bc1cc0c4f6aa1151cc0c52eda97a
- Tools run:
  - `git rev-parse --show-toplevel && git branch --show-current && git rev-parse HEAD`
  - `git diff 83000ca3...cb802209 --stat`
  - `git diff 83000ca3...cb802209` (key files: migration, soul_narrative.rs, memory_fragment.rs, memory.rs reflect_soul + fragments, soul_narrative_synthesizer.rs, world-selector.tsx + tests, schemas)
  - `cargo test -p nexus-daemon-runtime -p nexus-local-db -p nexus-creator-memory` (all green)
  - `pnpm --filter web run test` (379 passed)
  - Grep/Read for creator scoping, world_id threading, stale logic, insufficient gate, acp.prompt call site, DTO roundtrip, additionalProperties:false

**Deep review lenses applied** (triggered by: new endpoint + LLM boundary + data migration + creator-scoped auth surface):
- Creator-scoping / auth lens
- Migration safety lens
- LLM/Agent privilege boundary lens (prompt injection, tool surface, output mutation)
- Stale/insufficient-data correctness lens
- Concurrency (upsert race) lens
- DTO / contract fidelity lens
- Frontend world-projection honesty lens

## Findings

### 🔴 Critical
None.

### 🟡 Warning
None.

### 🟢 Suggestion
- **S-QC2-001 (concurrency documentation)**: `reflect_soul` uses `INSERT OR REPLACE` (last-writer-wins) for `memory_soul_narratives`. For the single-creator local model this is acceptable, but the handler does not serialize concurrent `force_regenerate=true` calls. Add a one-line comment or test noting "last-writer-wins is intentional; synthesis is idempotent and cheap under local single-writer assumption." (low cost, clarifies future scaling boundary).
- **S-QC2-002 (narrative contract clarity)**: The narrative is explicitly display-only (no write-back to SOUL.md or any mutation). The prompt + `tool_policy: "deny_all"` + output shape already enforce this, but a short doc comment in `soul_narrative_synthesizer.rs` or the endpoint would make the invariant machine-readable for future reviewers.
- **S-QC2-003 (test coverage for world filter in synthesis path)**: `build_soul_narrative_synthesis_input` hard-codes `world_id: None` (correct — narrative is always Creator-whole). A unit test that asserts the DAO call uses `None` would pin this invariant against accidental threading of a `world_id` from the request (which does not exist on `SoulNarrativeRequest`).

## Source Trace
- Finding ID: N/A (no blocking findings)
- Source Type: git-diff + manual code review + test execution + schema inspection
- Source Reference: `git diff 83000ca3...cb802209`, key paths listed in Scope, cargo/pnpm test output
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 3 |

**Verdict**: Approve

## Evidence (Completion Report v2 inputs)
- Verified branch + HEAD: `iteration/v1.81` @ `cb802209`
- Cargo tests: all crates under test passed (nexus-daemon-runtime, nexus-local-db, nexus-creator-memory)
- pnpm web tests: 379 passed (including new `soul-narrative-card`, `world-selector`, `growth-curve` suites)
- Report commit: (will be captured after `git commit`)
- No application-code edits performed

---

## Detailed Security & Correctness Notes (seat 2 lens)

### 1. Creator-scoping / auth
- `reflect_soul` reads `active_creator` from `config.toml` via `read_active_creator_id`.
- `req.creator_id` is validated to equal `active_creator` → 403 with explicit reason.
- `is_valid_creator_id` check (ctr_ prefix) is applied.
- Every downstream DAO call (`soul_narrative_fragment_stats`, `get_soul_narrative`, `list_fragments_limited` for input, `upsert_soul_narrative`) is passed `&active_creator` only.
- `world_id` query param on `/memory/fragments` is **never** used for auth; it is a filter on an already creator-scoped result set.
- **No cross-creator narrative read/write surface exists in this diff.**

### 2. Migration safety
- `ALTER TABLE memory_fragments ADD COLUMN world_id TEXT;` — additive, nullable. Existing rows become NULL (Creator-core-only) — no data loss.
- New table `memory_soul_narratives` with `PRIMARY KEY (creator_id)` — enforces single-row-per-creator upsert semantics.
- Index `idx_memory_fragments_creator_world_created` is additive.
- No foreign keys (intentional per plan — provenance tag, not ownership).
- Rollback story is implicit (drop column + drop table) and safe because the change is purely additive.

### 3. LLM/Agent boundary (privilege)
- Synthesis input is built exclusively from the **creator's own** fragments (capped: ≤30 keywords, ≤24 summaries ≤280 chars, ≤8 temporal buckets).
- `acp.prompt` is called with fixed prompt template + data + `"tool_policy": "deny_all"`.
- `_creator_id` and `_session_id` are passed for tracing only.
- Output (`full_text`) is stored as display-only narrative; no code path writes it back to SOUL.md, manuscript, or any mutable state.
- Insufficient-data gate runs **before** any ACP call.
- **Prompt injection surface exists in theory** (untrusted fragment summaries/keywords), but:
  - Data originates from the same creator's prior sessions.
  - No privileged capability is granted to the LLM (deny_all).
  - No mutation side-effect.
- Verdict: acceptable for V1.81 local-first model.

### 4. Stale / insufficient-data correctness
- Stale = `fragment_count_at_generation != current` **OR** `max_fragment_created_at_at_generation != current max`.
- Insufficient gate: `fragment_count < 10 || distinct_keyword_count < 20` (constants defined at top of handler).
- Gate evaluated on **current** stats, before cache check and before any synthesis.
- `force_regenerate` only bypasses "current + not-stale" path; it still respects the insufficient gate.
- `SoulNarrativeResponse` always returns current stats + min thresholds so UI can render the gate consistently.

### 5. Concurrency
- Two concurrent `/soul/reflect?force_regenerate=true` for the same creator can race on `upsert_soul_narrative`.
- Implementation: `INSERT OR REPLACE` (last-writer-wins).
- For local single-creator use this is acceptable (narrative is idempotent, synthesis is not a long critical section).
- No advisory lock or queue is used. Acceptable per plan scope.

### 6. DTO correctness
- `world_id: Option<String>` added to `MemoryFragmentInfo`, `ListMemoryFragmentsQuery`.
- New `SoulNarrativeRequest` / `SoulNarrativeResponse` schemas generated from `schemas/`.
- All schemas declare `"additionalProperties": false`.
- Round-trip test (`memory_dto_roundtrip`) and generated Rust/TS types exist.
- No handwritten DTO drift observed.

### 7. Frontend correctness (P1)
- `WorldSelector` + `deriveWorldOptions` only ever sees fragments returned by the already-creator-scoped `/memory/fragments` query.
- Selecting a world passes the id (or null) as `world_id` query param — re-scopes the same creator's data.
- Narrative card is world-agnostic by design (always uses the whole-Creator narrative endpoint).
- Growth-curve and stats components receive the filtered fragment list from the selector.
- No body-ownership violation (narrative is read/display; manuscript editing paths are untouched).
- All new tests (world-selector, soul-narrative-card, growth-curve) pass.

### 8. Test & CI evidence
- `cargo test -p nexus-daemon-runtime -p nexus-local-db -p nexus-creator-memory` — full suite green (including new migration idempotency, list_fragments_filtered world tests, doc-tests).
- `pnpm --filter web run test` — 379 passed, including the three new SOUL surface test files introduced in this wave.
- No pre-existing failures introduced by this diff.

## Revalidation Notes (if targeted re-review occurs)
N/A — initial review.

**Verdict**: Approve
(0 Critical, 0 Warning; all security/correctness concerns addressed or acceptable under documented local-first model.)
