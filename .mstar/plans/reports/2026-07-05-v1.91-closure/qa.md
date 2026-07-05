## Completion Report v2

**Agent**: qa-engineer
**Task**: Final QA verification for V1.91 on iteration/v1.91 (P0 profile-specific reading chrome + P1 findings batch triage)
**Status**: Done
**Scope Delivered**:
- Full gate suite execution on combined diff `main..iteration/v1.91`
- Explicit verification of every Acceptance Criterion in `.mstar/iterations/v1.91-reading-chrome-and-findings-batch-delivery-compass-v1.md` §5
- Confirmation that the 5 open residuals registered under `residual_findings["2026-07-05-v1.91-findings-batch"]` are non-blocking (QC "Approve with residuals" + defer/accept decisions)

**Artifacts**:
- This report: `.mstar/plans/reports/2026-07-05-v1.91-closure/qa.md`
- Supporting evidence in: compass §5, QC reports (qc1/qc2/qc3 under `2026-07-05-v1.91-reading-chrome`), status.json residual_findings, git diff main..iteration/v1.91, test files (reading-prose.test.tsx, findings-page.test.tsx, findings_api.rs), DESIGN.md (Reading Chrome section), schemas/daemon-api/findings/*

**Validation**:

### 1. Gate Suite Results (all PASS)

| Gate | Command | Result | Evidence |
|------|---------|--------|----------|
| Rust fmt | `cargo +nightly-2026-06-26 fmt --all --check` | ✅ PASS | EXIT 0, no output |
| Schema validation | `pnpm run validate-schemas` | ✅ PASS | 196 valid, 0 invalid |
| Rust tests | `cargo test --all` | ✅ PASS | All crates "test result: ok"; doc-tests and integration tests passed (full run completed with 0 failures in final output) |
| Rust clippy | `cargo clippy --all -- -D warnings` | ✅ PASS | EXIT 0, "Finished dev profile" with no warnings treated as errors |
| Web typecheck | `pnpm --filter web run typecheck` | ✅ PASS | tsc --noEmit clean after contracts build; "DTS Build success" |
| Contracts build | `pnpm --filter @42ch/nexus-contracts run build` | ✅ PASS | tsup success for both CJS/ESM + DTS |
| Web tests | `pnpm --filter web run test` | ✅ PASS | 55 test files, 420 tests passed |

### 2. Compass §5 Acceptance Criteria — Item-by-Item Verification

**P0 — Per-profile chrome (4 profiles)**:
- ✅ Each of `novel`, `essay`, `game-bible`, `script` renders with ≥2 distinct visual markers sourced exclusively from new `apps/web/DESIGN.md` `## Reading Chrome` token section.
- Evidence: DESIGN.md lines 425-539 (reading-chrome-novel: chapter-title/scene-separator/epigraph; reading-chrome-essay: section-heading/blockquote/footnote-marker; reading-chrome-game-bible: term-link/definition-callout/category-badge; reading-chrome-script: character-name/parenthetical/scene-heading). All additive, no renames.
- Component tests: `apps/web/src/components/reading/reading-prose.test.tsx` asserts `data-chrome-profile="novel"` (and fallbacks), `reading-chrome-novel-chapter-title`, and equivalent per-profile classes for the other three profiles.
- QC confirmation (qc1/qc2/qc3): "Token names frozen verbatim", "data-chrome-profile + data-chrome-element attributes", "no ad-hoc CSS for differentiation".

**P0 — Token contract**:
- ✅ `apps/web/DESIGN.md` contains `## Reading Chrome` section. All chrome differences token-driven.
- Evidence: DESIGN.md 425-539 + DESIGN.dark.md parallel section; index.css consumes `--reading-chrome-*` and `.reading-chrome-*` classes; reading-chrome-renderers.tsx uses only token-derived classes.
- No ad-hoc Tailwind or raw values for profile differentiation.

**P0 — Read-only invariant**:
- ✅ Reading chrome implementation and tests exercise only GET paths for Work/body. No PUT/PATCH/POST to `body_path`, outline, or timeline.
- Evidence: git diff main..iteration/v1.91 touches only `apps/web/src/components/reading/*`, `reading-prose.tsx`, `reading-chrome*.ts*`, chapter-page.tsx (read surface). Zero mutations. QC explicit check: "Reading chrome read-only", "no writes, no new mutations".

**P1 — Batch status**:
- ✅ Findings list supports multi-select; bulk status transition updates exactly the selected IDs via new helper endpoint and returns `{updated: N}` (or error shape).
- Evidence: `apps/web/src/api/queries.ts` `useBatchUpdateFindings`; `apps/web/src/pages/findings-page.tsx` bulk bar + handlers; integration tests `crates/nexus-daemon-runtime/tests/findings_api.rs` (findings_batch_update_status_happy_path, not_found_collected, conflict_collected, cap_rejected_with_422).
- Response shape per generated `BatchUpdateFindingsResponse`: `updated`, `not_found[]`, `conflict[]`.

**P1 — Batch assignment**:
- ✅ Multi-select + "assign target_executor" updates the field for exactly the selected findings.
- Evidence: Same batch handler + `findings_batch_update_assign_executor` test; UI wires `target_executor` into the PATCH body when selected.

**P1 — Export**:
- ✅ Client-side CSV export of the current filtered list emits a file whose header matches the documented columns and whose row count matches the visible filtered set (UI test).
- Evidence: `apps/web/src/pages/findings-page.test.tsx` "exports filtered findings as CSV" — asserts header `id,title,status,kind,severity,target_executor,created_at,rule_suggestion`, 3 lines for 2 rows + header, and filename pattern.

**P1 — Backend contract, schema & test**:
- ✅ `PATCH /v1/daemon/findings/batch` exists, additive only, enforces ≤100 IDs, reuses `update_finding` DAO per-ID, ≥1 Rust integration test asserting counts/not-found/conflict/cap.
- Evidence: Handler `crates/nexus-daemon-runtime/src/api/handlers/findings.rs:464`; route in `api/mod.rs`; 8 dedicated tests in `findings_api.rs` (happy, assign, not_found, conflict, cap, empty, etc.).
- New schemas: `schemas/daemon-api/findings/batch-update-findings-request.schema.json` + `-response.schema.json`.
- Codegen: `pnpm run codegen` run; `crates/nexus-contracts/src/generated/daemon_api/findings/` and `packages/nexus-contracts/.../BatchUpdateFindings*` present; `@42ch/nexus-contracts` bumped to 0.19.1 in package.json.
- `schema_drift_detection.rs` registers both with `CheckMode::Strict`.
- `validate-schemas` clean (196/196).

**Regression**:
- ✅ All pre-existing single-finding edit flows, reading-body display, and list behaviors unchanged.
- Evidence: `cargo test -p nexus-daemon-runtime` (findings) passes; web tests covering list/reading surface (420 total) pass; QC: "single-finding edit path and existing list behavior are unchanged"; git diff shows no mutation of existing PATCH /findings/:id or reading body paths.

**CI & hygiene**:
- ✅ All gates green (see table above). No wire-contract breaking changes (additive schemas only, schema_version unchanged).

**Scope fidelity**:
- ✅ Diff touches only reading chrome (P0, apps/web) and findings batch (P1, one handler + UI). No TLS, no canvas, no auto-fix, no new profiles, no backend export jobs.
- Evidence: `git diff --stat main..iteration/v1.91` (40 files, focused); QC cross-checks confirm P0/P1 separation ("P1 never touches reading chrome", "P0 adds zero backend routes").

### 3. Residuals (5 open, non-blocking)

All 5 registered under `residual_findings["2026-07-05-v1.91-findings-batch"]` in `.mstar/status.json` (lines 6876-6941):

1. **R-V191P1-001** (medium): CSV toast conflates not_found vs conflict — partial-success UX lacks distinction. Decision: defer (V1.92). Source: qc1 W-003 / qc2 W-003. Non-blocking for V1.91 (feature works; UX polish tracked).
2. **R-V191P1-002** (low): Extract CSV utilities from findings-page.tsx (code health). Decision: accept. Non-blocking.
3. **R-V191P1-003** (low): Codegen should emit concrete BatchUpdateFindingsRequest.patch struct. Decision: defer (V1.92+). Non-blocking (runtime helper is correct).
4. **R-V191P1-004** (low): Add test for mid-batch internal error path (partial apply visibility). Decision: accept. Non-blocking (matches documented "loop, not transaction" design).
5. **R-V191P1-005** (info): FindingsPage recomputes allSelected/someSelected on every render without memoisation. Decision: defer (when list virtualisation lands). Non-blocking (100-item cap, not user-visible).

QC verdict was "Approve with residuals" (no Critical/Warning blocking). All residuals are explicitly open, owned, targeted, and non-blocking per compass + QC.

**Issues/Risks**: None blocking. All 5 residuals are acknowledged, categorized, and deferred/ accepted per QC decisions. No Critical or unresolved Warning findings. Read-only invariant and scope fidelity hold strictly.

**Plan Update**: N/A (leaf QA; no code changes). Compass §5 AC all verified with evidence. 5 residuals confirmed non-blocking.

**Handoff**: Ready for PM to proceed with P-last closure (tracker update, compound, Profile B compaction, PR to main). All evidence reproducible via:
- `git diff main..iteration/v1.91`
- The gate commands listed above
- Specific test files + DESIGN.md sections cited

**Git**: (report-only QA — no commits authored by this agent)

---

**Verdict**: **Pass**

All gates green. Every compass §5 AC has concrete, reproducible evidence. The 5 open residuals are non-blocking (QC "Approve with residuals", explicit defer/accept decisions, no Critical/Warning open). Scope fidelity, read-only invariant, and contract hygiene hold. V1.91 is QA-verified for closure.
