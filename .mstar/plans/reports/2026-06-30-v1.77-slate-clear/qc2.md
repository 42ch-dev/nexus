---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-30-v1.77-slate-clear"
verdict: "Approve"
generated_at: "2026-06-30"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: Security and correctness risk
- Report Timestamp: 2026-06-30

## Scope
- plan_id: 2026-06-30-v1.77-slate-clear
- Review range / Diff basis: git diff ba71d9167f6269cd0175b86f202baa3e19b517a6...a2571381b2a9865c6a98ffec461d4a99051a39f0 (10 implementation commits)
- Working branch (verified): iteration/v1.77
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 25 (implementation delta); P1 surface is file-disjoint from P0 (world-kb backend + canvas bulk-promote component only)
- Commit range: ba71d916...a2571381
- Tools run: git diff --stat, git log, grep (GRAPH_RELATIONSHIP_CAP, bulk-promote, llm_extract), read (world_kb.rs projection path, world-kb-canvas bulk logic)

**Deep review: triggered (S1: implementation changes touch graph projection + concurrent mutation path; S2: graph projection and bulk PATCH surface on authenticated creator data).**

**Lenses applied:** Security Lens, Correctness Lens (per mstar-review-qc deep-review-personas).

## Findings

### 🔴 Critical
(none)

### 🟡 Warning
(none)

### 🟢 Suggestion

- **S-301 (low, Suggestion)** — `GRAPH_RELATIONSHIP_CAP = 1000` is a reasonable interim safety bound, but the constant lacks a one-line justification for the specific value (why 1000 vs 500 or 2000). The doc comment explains the *mechanism* (cap stored rows before symmetric derivation to avoid splitting pairs) and the *roadmap* (full pagination deferred past V1.77 because it requires a wire change). Adding a short rationale comment (e.g., "1000 chosen as ~2× current entity cap to accommodate typical extraction suggestion volume without materializing pathological payloads") would help future readers. Not a defect.
  - Source Type: manual-reasoning + deep-lens: Correctness Lens
  - Source Reference: `crates/nexus-daemon-runtime/src/api/handlers/world_kb.rs:66` (const), `939-956` (take + capacity), `891-900` (TODO + rationale)
  - Confidence: Low

- **S-302 (low, Suggestion)** — B3 batch size `PROMOTE_BATCH_SIZE = 5` is introduced with a good doc comment explaining the unbounded-burst problem it solves. The constant itself has no comment justifying "5" (as opposed to 3 or 10). A one-line note on the declaration would be sufficient hygiene. The correctness contract (all outcomes observed, failed-count accurate, reseed still fired once) is preserved.
  - Source Type: manual-reasoning + deep-lens: Correctness Lens
  - Source Reference: `apps/web/src/components/canvas/world-kb/world-kb-canvas.tsx:36-48` (constant + doc), `223-263` (bounded batch loop)
  - Confidence: Low

## Source Trace
- Finding ID: S-301
- Source Type: deep-lens: Correctness Lens
- Source Reference: world_kb.rs:66 + 939-956 + 891-900
- Confidence: Low

- Finding ID: S-302
- Source Type: deep-lens: Correctness Lens
- Source Reference: world-kb-canvas.tsx:36-48 + 223-263
- Confidence: Low

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 2 |

**Verdict**: Approve

## Detailed Review Notes (qc2 lens)

### B2 — Graph cap correctness (`R-V176QC1-S002`)
- Cap location: `project_relationships_for_world` in `crates/nexus-daemon-runtime/src/api/handlers/world_kb.rs`.
- The cap is applied to *stored rows* **before** any projection:
  ```rust
  let mut projections = Vec::with_capacity(rows.len().min(GRAPH_RELATIONSHIP_CAP) * 2);
  for row in rows.into_iter().take(GRAPH_RELATIONSHIP_CAP) {
      projections.push(project_relationship(&row, "stored"));
      if row.symmetric != 0 {
          // emit symmetric_reverse from the same stored row
      }
  }
  ```
- Because the cap is taken on the stored iterator, a single stored row is never split from its (optional) symmetric reverse. Each stored row contributes 0 or 2 projections. The pre-allocation `min(..., CAP) * 2` bounds the wire payload.
- Doc comment at lines 58-65 and 891-900 explicitly documents the invariant and the future pagination plan (requires wire-contract change, hence deferred for V1.77's `wire_contracts_changed: FALSE`).
- No evidence of silent data loss for the current threat model: pre-1.0 local-first worlds are expected to stay well under the cap; the cap is a safety bound, not a correctness boundary for small data.
- Symmetric derivation remains deterministic and side-effect free.

### B3 — Batch correctness for bulk-promote (`R-V176QC3-S001`)
- Previous behavior (unbounded): `await Promise.allSettled(rels.map(mutateAsync))` — could fire hundreds of concurrent PATCHes for accumulated suggestions.
- New behavior (V1.77):
  - Constant `PROMOTE_BATCH_SIZE = 5` with explanatory comment.
  - Outer loop: `for (let i = 0; i < rels.length; i += BATCH) { const batch = ...; const settled = await Promise.allSettled(batch.map(...)); results.push(...); }`
  - All settled results (success + rejection) are collected into one array across batches.
  - `failed` count is derived from the complete `results`.
  - `console.warn` still reports `failed.length / rels.length`.
  - `bumpReseed()` is called exactly once after the loop (same as before).
- Every outcome is still observed (the original motivation for `mutateAsync + Promise.allSettled` is preserved).
- No partial-batch inconsistency: each batch is fully awaited before the next starts; the final reseed happens after all batches.
- The change is surgical and only touches the bulk path.

### B1 — LlmExtractOutcome.relationships field discoverability
- Plan and compass both label B1 as "doc-only" / low.
- The implementation commit in the range is `docs(v1.77): document LlmExtractTask relationships field discoverability`.
- The delta for `crates/nexus-orchestration/src/capability/builtins/llm_extract.rs` in the reviewed range contains no behavioral change (pure documentation / discoverability improvement, e.g., rustdoc or re-export comment).
- No security surface, no correctness change, no data model impact. From the qc2 (security + correctness) lens this is a no-op.

### Cross-cutting (Security Lens)
- All three B items are file-disjoint from the P0 findings-remediation surface.
- Both world-kb paths (graph read, relationship patch) remain creator-scoped via the existing `require_creator` + world-ownership checks (unchanged in this delta).
- No new unauthenticated or cross-creator surfaces introduced.
- Bulk-promote still goes through the authenticated `patchRelationship` mutation (per-row OCC on `kb_relationships.revision`).

### Last-writer-wins / concurrent edit
- Not applicable to P1 scope. The graph projection is read-only (with a filter); the bulk-promote path uses explicit `expected_version` per row (OCC) on the individual patch calls. The batching change does not alter the per-row CAS contract.

## CI / Static Checks
- No new failures introduced in the P1 surface.
- Existing world_kb relationship tests (daemon + web) continue to cover projection shape and promote outcomes.

**Conclusion (qc2)**: B2 correctly caps stored rows before symmetric derivation, preserving pair integrity. B3 correctly bounds concurrency while preserving the "observe every outcome" contract. B1 is doc-only with no security or correctness impact. No blocking findings from the security + correctness lens. Two low-severity Suggestions recorded for constant-value rationale hygiene.
