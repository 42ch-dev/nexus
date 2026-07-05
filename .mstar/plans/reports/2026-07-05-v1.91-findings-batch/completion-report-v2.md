---
plan_id: 2026-07-05-v1.91-findings-batch
status: InReview
type: completion-report
author: "@fullstack-dev"
branch: "feature/v1.91-findings-batch"
commit: "1d5a0952a3544f10667227e971ae42d7ec17e029"
completed_at: "2026-07-05"
---

# Completion Report v2 — P1 Track B: Findings Batch Triage

## Summary

P1 implementation is complete and all pre-QC validation gates pass. The bulk findings triage endpoint, schema/codegen, and web UI multi-select + CSV export are implemented additively on top of the existing single-finding flows. No P0 (reading chrome) files were modified.

## Delivered scope

### Backend

- `PATCH /v1/daemon/findings/batch` handler added in `crates/nexus-daemon-runtime/src/api/handlers/findings.rs`.
- Creator-scoped via `read_active_creator_id`; reuses existing `update_finding` DAO per-ID in a loop (partial success, not a transaction).
- Cap enforced at 100 IDs; returns `too_many_findings` → HTTP 422.
- Response shape: `{ updated: number, not_found?: string[], conflict?: string[] }`.
- Integration tests in `crates/nexus-daemon-runtime/tests/findings_api.rs` cover:
  - happy path with exact `updated` count,
  - `not_found` IDs skipped and listed,
  - `conflict` IDs skipped and listed (illegal lifecycle transitions),
  - cap breach error.

### Schemas & codegen

- Added `schemas/daemon-api/findings/batch-update-findings-request.schema.json`.
- Added `schemas/daemon-api/findings/batch-update-findings-response.schema.json`.
- Regenerated Rust types in `crates/nexus-contracts/src/generated/daemon_api/findings/`.
- Regenerated TypeScript types in `packages/nexus-contracts/src/generated/daemon-api/findings/`.
- `@42ch/nexus-contracts` version bumped `0.19.0` → `0.19.1`.
- `schema_drift_detection` test passes.

### Frontend (`apps/web`)

- `NexusClient` interface extended with `batchUpdateFindings(...)`.
- `BrowserClient` implements the new endpoint.
- `adapter-contract.test.ts` updated with method parity + batch path coverage.
- `useBatchUpdateFindings` TanStack Query mutation hook added (`apps/web/src/api/queries.ts`).
- `findings-page.tsx` updated with:
  - per-row selection checkboxes,
  - select-all / clear selection,
  - bulk action bar (visible when ≥1 selected),
  - "Set status to…" bulk transition,
  - "Assign executor…" bulk `target_executor` assignment,
  - client-side CSV export of the currently filtered list.
- `findings-page.test.tsx` added covering selection, select-all/clear, bulk status, bulk executor, and CSV export.

## Definition of Done check

| DoD item | Status | Evidence |
|---|---|---|
| Bulk PATCH endpoint exists, additive, cap ≤100, creator-scoped, reuses DAO | ✅ | `crates/nexus-daemon-runtime/src/api/handlers/findings.rs` + tests |
| Integration test covers updated/not_found/conflict/cap | ✅ | `crates/nexus-daemon-runtime/tests/findings_api.rs` |
| Two JSON Schema files committed; codegen run; version bumped | ✅ | `schemas/daemon-api/findings/batch-update-findings-*.schema.json`; `@42ch/nexus-contracts@0.19.1` |
| Multi-select + bulk status + bulk executor in findings list | ✅ | `apps/web/src/pages/findings-page.tsx` |
| Client-side CSV export with documented columns | ✅ | `findings-page.tsx` + `findings-page.test.tsx` |
| Single-finding flows unchanged | ✅ | Existing tests still pass; no edits to single-edit paths |
| `cargo clippy --all`, `cargo test --all`, web typecheck/test pass | ✅ | See Verification section |
| P0 files untouched | ✅ | `git diff --name-only` shows no `apps/web/src/components/reading-chrome*` or DESIGN reading-chrome token changes |

## Verification evidence

All commands run from the worktree at `.worktrees/feature-v1.91-findings-batch`.

```text
cargo test --all                        # passed
cargo clippy --all -- -D warnings       # passed
cargo +nightly-2026-06-26 fmt --all --check  # passed
pnpm --filter web run typecheck         # passed
pnpm --filter web run test              # 411 passed
pnpm run validate-schemas               # 196 schemas valid
cargo test --test schema_drift_detection # passed
```

## Known limitations / deliberate trade-offs

- **No OCC / conflict UI**: per plan, single-author last-writer-wins, consistent with V1.77.
- **CSV is client-side only**: exported from the visible filtered rows; no server-side endpoint.
- **Native `<input type="checkbox">` used**: no `Checkbox` shadcn/ui primitive currently exists in the web app.
- **Partial success surfaced via toast**: `not_found` and `conflict` arrays are summarized in a toast; no persistent banner.

## Files changed

See commit `1d5a0952a3544f10667227e971ae42d7ec17e029` for the full diff. Key paths:

- `crates/nexus-daemon-runtime/src/api/handlers/findings.rs`
- `crates/nexus-daemon-runtime/src/api/errors.rs`
- `crates/nexus-daemon-runtime/src/api/mod.rs`
- `crates/nexus-daemon-runtime/tests/findings_api.rs`
- `schemas/daemon-api/findings/batch-update-findings-request.schema.json`
- `schemas/daemon-api/findings/batch-update-findings-response.schema.json`
- `crates/nexus-contracts/src/generated/daemon_api/findings/`
- `packages/nexus-contracts/src/generated/daemon-api/findings/`
- `packages/nexus-contracts/package.json`
- `apps/web/src/lib/nexus/types.ts`
- `apps/web/src/lib/nexus/browser-client.ts`
- `apps/web/src/lib/nexus/adapter-contract.test.ts`
- `apps/web/src/api/queries.ts`
- `apps/web/src/pages/findings-page.tsx`
- `apps/web/src/pages/findings-page.test.tsx`

## Next steps

1. PM to review this Completion Report and move plan to QC.
2. Run QC tri-review on `feature/v1.91-findings-batch` (or `iteration/v1.91` after merge).
3. QA verification.
4. Merge to `iteration/v1.91` (after P-1 / P0 integration if applicable).
5. Final closeout / compound in P-last.

## Sign-off

- Implementer: `@fullstack-dev`
- Branch: `feature/v1.91-findings-batch`
- Commit: `1d5a0952a3544f10667227e971ae42d7ec17e029`
