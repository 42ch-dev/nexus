---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-27-v1.71-canvas-strategy-write-boundary"
verdict: "Request Changes"
generated_at: "2026-06-28"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: volcengine-plan/ark-code-latest
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-06-28

## Scope
- plan_id: 2026-06-27-v1.71-canvas-strategy-write-boundary
- Review range / Diff basis: git diff 39493026..HEAD -- schemas/local-api/canvas/ crates/nexus-contracts/src/generated/local_api/canvas/ crates/nexus-contracts/src/generated/local_api/mod.rs crates/nexus-contracts/src/generated/mod.rs crates/nexus-contracts/tests/schema_drift_detection.rs crates/nexus-daemon-runtime/src/api/errors.rs crates/nexus-daemon-runtime/src/api/handlers/mod.rs crates/nexus-daemon-runtime/src/api/handlers/strategy.rs crates/nexus-daemon-runtime/src/api/mod.rs packages/nexus-contracts/package.json packages/nexus-contracts/src/generated/index.ts packages/nexus-contracts/src/generated/local-api/canvas/ apps/web/DESIGN.md apps/web/DESIGN.dark.md apps/web/src/index.css apps/web/src/components/canvas/strategy-canvas.tsx apps/web/src/lib/canvas/preset-yaml.ts apps/web/src/lib/canvas/use-strategy-data.ts apps/web/src/lib/nexus/browser-client.test.ts apps/web/src/lib/nexus/browser-client.ts apps/web/src/lib/nexus/types.ts apps/web/tailwind.config.ts
- Working branch (verified): iteration/v1.71
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 37 changed files in the assigned diff range
- Commit range: 39493026..HEAD
- Tools run:
  - `git diff 39493026..HEAD -- ...assigned paths...` (captured full assigned diff)
  - `cargo test -p nexus-daemon-runtime` (passed: 336 unit tests plus integration/doc tests; warnings only)
  - `cargo test -p nexus-contracts` (passed: unit/integration/doc tests)
  - `pnpm --filter web test` (passed: 17 files / 139 tests; React Router future warnings only)
  - `pnpm --filter web build` (initially failed because `@42ch/nexus-contracts` dist had not been built; after `pnpm --filter @42ch/nexus-contracts run build`, rerun passed with Vite chunk-size warning)

## Findings

### 🔴 Critical

- **C1 — Revision precondition is not atomic across concurrent requests, so same-revision writers can silently overwrite each other** -> Add a per-strategy write lock or an equivalent compare-and-swap style commit around the full `load_user_preset_yaml` -> `base_revision` check -> validation -> persistence sequence. Today every handler reads `preset.yaml`, checks `req.base_revision != current_revision`, mutates an in-memory YAML value, and writes it back without any shared lock. Two requests that both start from revision `N` can both pass the precondition and race to write `revision: N+1`; the later rename wins and no client receives the required 409. This violates the plan's A2/A4 requirement that stale `base_revision` is rejected and that revision increments exactly once in the same committed image.
  - Evidence: `crates/nexus-daemon-runtime/src/api/handlers/strategy.rs:389-459` (`patch_state`), `597-663` (`patch_transition`), and `688-774` (`patch_prompt_template`) all perform read/check/write without a lock; tests cover a stale single request but not two simultaneous same-base requests.
  - Proposed residual id: `R-V171-P0-QC3-C1` (`severity: critical`) — Strategy patch revision check is TOCTOU under concurrent writers.

- **C2 — Prompt-template patches can leave prompt content changed while `revision:` remains unchanged** -> Make prompt-template writes participate in the same durable transaction as the YAML revision bump, or stage the prompt body to a sibling temp file and only atomically rename it after validation plus a successful revision commit with rollback/cleanup on failures. `patch_prompt_template` writes the target prompt file with `std::fs::write` before validation and before `write_preset_yaml` bumps `revision:`. If validation or YAML serialization/write/rename fails afterward, the prompt file has already changed but the strategy revision remains old; the next client can reuse the stale `base_revision`, and the UI cannot reliably detect or recover from the partial write.
  - Evidence: `crates/nexus-daemon-runtime/src/api/handlers/strategy.rs:754-774` writes the prompt file, then validates, then bumps YAML. The prompt file write is not temp+rename+fsync and has no rollback path.
  - Proposed residual id: `R-V171-P0-QC3-C2` (`severity: critical`) — Prompt-template body write is non-atomic relative to Strategy revision persistence.

- **C3 — A single UI Save can self-induce stale-revision conflicts after partially committing earlier fields** -> Either split the UI into one patch operation per Save affordance, or update the local `baseRevision` from each successful `StrategyPatchResponse.new_revision` before sending the next mutation and await the canonical refetch before leaving edit mode. `handleSave` can issue up to three mutations in sequence (state, transition, prompt) but passes the same `baseRevision` to each. If the first mutation succeeds, the daemon increments the revision, so the second mutation from the same user is stale and returns 409. That leaves a partially applied edit and presents it as a conflict, reducing reliability of multi-field inspector edits and making recovery ambiguous.
  - Evidence: `apps/web/src/components/canvas/strategy-canvas.tsx:153-198` builds state/transition/prompt mutations from one `baseRevision`; `apps/web/src/lib/canvas/use-strategy-data.ts:270-279`, `315-324`, and `362-370` invalidate the query asynchronously but do not feed `new_revision` back into subsequent mutations.
  - Proposed residual id: `R-V171-P0-QC3-C3` (`severity: critical`) — Client multi-field Save uses stale base_revision after its own first successful patch.

### 🟡 Warning

- **W1 — YAML rename durability does not fsync the parent directory and uses a fixed temp filename** -> After `std::fs::rename`, fsync the bundle directory so the rename metadata survives a power loss on POSIX filesystems, and use a request-unique temp filename so concurrent writers cannot clobber the same `preset.yaml.tmp`. The current implementation fsyncs only the temp file, then renames it to `preset.yaml`. This is close to the requested temp+rename+fsync pattern but not fully crash-durable, and the fixed temp path amplifies the concurrent-writer race from C1.
  - Evidence: `crates/nexus-daemon-runtime/src/api/handlers/strategy.rs:339-368` writes `preset.yaml.tmp`, `file.sync_all()`, then `rename`, with no directory fsync and no unique temp path.
  - Proposed residual id: `R-V171-P0-QC3-W1` (`severity: medium`) — Atomic YAML write lacks directory fsync and unique temp path.

- **W2 — Conflict recovery does not automatically refetch canonical data on 409** -> On a `strategy_conflict`, keep the draft, immediately refetch the canonical preset, and render the modal from both canonical and draft values. The current code sets conflict state and waits for the user to click `Refetch graph`. That defers the required recovery path and gives the modal no canonical data to compare against the draft, so reliability of conflict resolution depends on a manual second step.
  - Evidence: `apps/web/src/components/canvas/strategy-canvas.tsx:200-208` only sets `conflict`; `295-303` passes a manual `onRefetch`; `478-522` renders no canonical/draft comparison.
  - Proposed residual id: `R-V171-P0-QC3-W2` (`severity: medium`) — Client conflict path does not refetch canonical Strategy immediately.

- **W3 — The first required web build command fails in a clean dist-less workspace unless contracts are built first** -> Either update the documented/gated command to `pnpm --filter @42ch/nexus-contracts run build && pnpm --filter web build`, or make the web package build depend on the workspace contracts build. The assignment requires `pnpm --filter web build`; that command failed because TypeScript read `@42ch/nexus-contracts` from `dist` and the new generated exports were not present until the contracts package was built. The rerun passed after building contracts, but the required command is not deterministic in a fresh workspace.
  - Evidence: initial `pnpm --filter web build` failed with TS2305 missing exports for `StrategyPatch*` from `@42ch/nexus-contracts`; after `pnpm --filter @42ch/nexus-contracts run build`, `pnpm --filter web build` passed.
  - Proposed residual id: `R-V171-P0-QC3-W3` (`severity: medium`) — Web build depends on an unstated generated contracts dist prebuild.

### 🟢 Suggestion

- **S1 — Add a deterministic concurrent-writer regression test** -> Add a test that runs two Strategy patch requests against the same `base_revision` and asserts one succeeds and one returns `strategy_conflict` (or that a write lock serializes and rechecks revision before commit). This would turn C1 into a durable guardrail.

- **S2 — Add crash/partial-failure tests around prompt-template persistence** -> Inject failures around prompt template write, validation, and YAML rename, then assert prompt content and `revision:` do not diverge. This would protect the atomic write boundary after C2 is fixed.

- **S3 — Track bundle size explicitly for the Strategy route chunk** -> Route splitting is preserved (`App.tsx` lazy-loads `StrategyPage`), and the production build produced a separate `strategy-page` chunk. The chunk is about 314 kB minified / 99 kB gzip; record this as a baseline and consider manual chunking for React Flow if the route grows further. The existing Vite warning is for the bootstrap `index` chunk, not the Strategy chunk, but the canvas chunk is already sizable.

## Source Trace
- Finding C1: `crates/nexus-daemon-runtime/src/api/handlers/strategy.rs:389-459`, `597-663`, `688-774`; `cargo test -p nexus-daemon-runtime` output shows only stale single-request coverage.
- Finding C2: `crates/nexus-daemon-runtime/src/api/handlers/strategy.rs:754-774`.
- Finding C3: `apps/web/src/components/canvas/strategy-canvas.tsx:153-198`; `apps/web/src/lib/canvas/use-strategy-data.ts:270-279`, `315-324`, `362-370`.
- Finding W1: `crates/nexus-daemon-runtime/src/api/handlers/strategy.rs:339-368`.
- Finding W2: `apps/web/src/components/canvas/strategy-canvas.tsx:200-208`, `295-303`, `478-522`.
- Finding W3: `pnpm --filter web build` failure log; successful rerun after `pnpm --filter @42ch/nexus-contracts run build`.

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 3 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 3 |

**Verdict**: Request Changes

**Rationale**: The current implementation passes Rust tests, contract tests, web tests, and web build after a contracts prebuild, but it does not yet meet the reliability-critical write-boundary guarantees for concurrent revision checks, prompt-template atomicity, or multi-field client saves. These issues can cause silent lost updates, partial on-disk state, or self-inflicted conflicts under normal inspector usage.

## Residual Findings (proposed stable IDs)
- `R-V171-P0-QC3-C1` — Strategy patch revision check is TOCTOU under concurrent writers (`critical`).
- `R-V171-P0-QC3-C2` — Prompt-template body write is non-atomic relative to Strategy revision persistence (`critical`).
- `R-V171-P0-QC3-C3` — Client multi-field Save uses stale `base_revision` after its own first successful patch (`critical`).
- `R-V171-P0-QC3-W1` — Atomic YAML write lacks directory fsync and unique temp path (`medium`).
- `R-V171-P0-QC3-W2` — Client conflict path does not refetch canonical Strategy immediately (`medium`).
- `R-V171-P0-QC3-W3` — Web build depends on an unstated generated contracts dist prebuild (`medium`).
