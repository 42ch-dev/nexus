---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-30-v1.77-findings-remediation-ui"
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
- plan_id: 2026-06-30-v1.77-findings-remediation-ui
- Review range / Diff basis: git diff ba71d9167f6269cd0175b86f202baa3e19b517a6...a2571381b2a9865c6a98ffec461d4a99051a39f0 (10 implementation commits)
- Working branch (verified): iteration/v1.77
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 25 (implementation delta, excluding plan/iteration/knowledge/status artifacts)
- Commit range: ba71d916...a2571381 (implementation HEAD before any QC report commits)
- Tools run: git diff --stat, git log, grep, read (targeted handler/DAO/client/mutation paths), cargo test -p nexus-daemon-runtime findings (prior baseline), vitest findings-mutation (targeted)

**Deep review: triggered (S1: ~1717 net implementation LOC across 25 files; S2: new PATCH authoring surface on authenticated creator-scoped findings; S5: plan explicitly locked D1a/D1b/D1c security decisions around transition authority + IDOR + last-writer-wins).**

**Lenses applied:** Security Lens, Correctness Lens, Input Validation Lens (per mstar-review-qc deep-review-personas).

## Findings

### 🔴 Critical
(none)

### 🟡 Warning
(none)

### 🟢 Suggestion

- **S-201 (low, Suggestion)** — Client-side `UpdateFindingRequest` usage is typed end-to-end via `@42ch/nexus-contracts`, but the `useUpdateFinding` mutation site does not perform an explicit runtime schema guard (e.g., `UpdateFindingRequestSchema.parse` or equivalent) before the network call. The UI already disables illegal transitions and only sends defined fields via `buildPatch`, and the server is authoritative (422 `invalid_transition` / `invalid_input` paths verified). This is defense-in-depth polish, not a correctness gap. Consider adding a thin client validation helper in a follow-up if the remediation surface grows beyond the current 7-field PATCH.
  - Source Type: manual-reasoning + deep-lens: Input Validation Lens
  - Source Reference: `apps/web/src/api/queries.ts:245` (mutationFn), `apps/web/src/components/findings/finding-detail-panel.tsx:50` (buildPatch), `lib/findings-lifecycle.ts:24` (isValidTransition)
  - Confidence: Medium

- **S-202 (low, Suggestion)** — Optimistic rollback test (`findings-mutation.test.tsx`) exercises the 422 `INVALID_TRANSITION` path and verifies list cache restoration, but does not assert that the detail-panel form state also re-syncs to server truth on error (via `onSettled` + `useEffect([finding.updated_at])`). Current behavior is correct (invalidation + re-render from list cache), but an explicit assertion would lock the "no stuck partial state" contract for future editors.
  - Source Type: manual-reasoning + deep-lens: Correctness Lens
  - Source Reference: `apps/web/src/api/findings-mutation.test.tsx:108-130` (INVALID_TRANSITION test), `queries.ts:277-284` (onError), `finding-detail-panel.tsx:89-91` (useEffect sync)
  - Confidence: Low

## Source Trace
- Finding ID: S-201
- Source Type: deep-lens: Input Validation Lens
- Source Reference: queries.ts:245 + finding-detail-panel.tsx:50 + contracts UpdateFindingRequest
- Confidence: Medium

- Finding ID: S-202
- Source Type: deep-lens: Correctness Lens
- Source Reference: findings-mutation.test.tsx:108 + queries.ts:277 + finding-detail-panel.tsx:89
- Confidence: Low

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 2 |

**Verdict**: Approve

## Detailed Review Notes (qc2 lens)

### Authorization / IDOR (Security Lens)
- Handler (`crates/nexus-daemon-runtime/src/api/handlers/findings.rs:380-451`):
  1. `read_active_creator_id(...)` (session-derived, not client-controlled).
  2. `works::get_work(state.pool(), &creator_id, &work_id)` — 404 if missing or not owned.
  3. `findings::update_finding(state.pool(), &creator_id, &finding_id, &patch, now)` — DAO predicate is `WHERE creator_id = ? AND finding_id = ?`.
- DAO (`crates/nexus-local-db/src/findings.rs:1038-1040`): `q = q.bind(creator_id); q = q.bind(finding_id);`.
- No path for a client to supply a different `creator_id`. IDOR surface is closed. Verified via code read + prior V1.49 findings_api.rs tests (still passing baseline).

### Transition Authority (Correctness + Security)
- Server is authoritative: `is_valid_transition(from, to)` at `findings.rs:172-189` (6-state adjacency table).
- `enforce_status_transition(...)` (lines 863-895) does the read-before-write guard and emits typed `IllegalTransition`.
- Handler maps ONLY `IllegalTransition` → `BadRequest { code: "invalid_transition", ... }` (422 via `errors.rs:254`); `InvalidEnum` → `invalid_input`. No string-prefix sniffing.
- UI (`lib/findings-lifecycle.ts`) disables invalid transitions as defense-in-depth; client can still be bypassed (test at `findings-mutation.test.tsx:108` confirms 422 rollback).
- Self-loop (`status: <current>`) correctly rejected as `INVALID_TRANSITION` (doc + test coverage from V1.49).

### Input Validation & XSS (Security Lens)
- All PATCH enum fields validated server-side before any write (`VALID_STATUSES`, `VALID_SEVERITIES`, `VALID_TARGET_EXECUTORS`).
- Rendered fields in detail panel (`title`, `description`, `rule_suggestion`) are plain text; no `dangerouslySetInnerHTML` anywhere in the delta (grep confirmed zero occurrences under `apps/web/src`).
- `buildPatch` sends only changed + defined fields; `rule_suggestion` tri-state handling mirrors the DAO (empty string clears).
- No HTML content is ever stored or rendered from finding text.

### Optimistic Update Correctness (Correctness Lens)
- `useUpdateFinding`:
  - `onMutate`: cancels list queries, snapshots **all** list caches for the work (`getQueriesData`), applies only defined patch fields.
  - `onError`: restores every snapshot.
  - `onSettled`: invalidates lists + detail.
- 422 rollback test exists and exercises the exact error path the assignment called out.
- No evidence of stuck partial state under the single-author-triage model (D1b).

### Last-Writer-Wins Threat Model (D1b LOCKED)
- Compass + spec + plan all document: single-author triage (author triages own findings; no concurrent automated producer writes `status`/`target_executor`).
- DAO uses simple `WHERE creator_id = ? AND finding_id = ?` UPDATE; no revision column.
- Threat model holds for V1.77 scope. If a future producer path begins mutating triage fields, this would need re-evaluation (not in scope here).

### Type-Safety
- `UpdateFindingRequest` imported directly from `@42ch/nexus-contracts` in `types.ts`, `queries.ts`, `finding-detail-panel.tsx`, and tests.
- No `any` / `as` escape hatches on the wire payload in the changed files.
- Adapter-contract test (`adapter-contract.test.ts`) asserts the PATCH shape reaches the client.

## CI / Static Checks
- No new CI failures introduced by the implementation delta (baseline `cargo test -p nexus-daemon-runtime findings` and vitest mutation tests were green prior to this review).
- No clippy or fmt drift visible in the security/correctness surface.

**Conclusion (qc2)**: The implementation correctly places the security and correctness authority on the server (transition guard, creator-scoped DAO predicate, typed error mapping). Client is a well-behaved consumer with defense-in-depth. No blocking findings from the security + correctness lens. Two low-severity Suggestions recorded for future hygiene.
