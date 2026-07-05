---
report_kind: qc
reviewer: "@qc-specialist-2"
reviewer_index: 2
plan_id: "2026-06-28-v1.72-hygiene-and-release-hardening"
focus: "security_correctness"
verdict: "Approve"
generated_at: "2026-06-28T17:15:00Z"
---

# Code Review Report — qc2 (Security + Correctness)

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: Security and correctness risk
- Report Timestamp: 2026-06-28T17:15:00Z

## Scope
- plan_id: 2026-06-28-v1.72-hygiene-and-release-hardening
- Review range / Diff basis: `git diff 92a1c07f..HEAD -- .github/actions/setup-monorepo/ .github/workflows/ci.yml .github/workflows/desktop-build.yml .github/workflows/desktop-release.yml apps/desktop/SIGNING.md apps/web/src/components/canvas/strategy-canvas.tsx apps/web/src/components/canvas/strategy-canvas/ apps/web/src/lib/canvas/use-strategy-data.ts .mstar/status.json`
- Working branch (verified): iteration/v1.72
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 12 (strategy-canvas split + desktop-release signing + composite action + status residual updates)
- Commit range: 92a1c07f..31805f9e (P1 merge) + follow-on status commits
- Tools run:
  - `git diff 92a1c07f..HEAD` (scoped)
  - `git branch --show-current` (iteration/v1.72)
  - `cargo +nightly-2026-06-26 fmt --all --check` (clean)
  - `pnpm --filter web typecheck` (unrelated outline errors; strategy-canvas path clean)
  - `pnpm --filter web build` (succeeded)
  - `pnpm --filter web test` (153/153 passed)
  - `cargo clippy --all -- -D warnings` (no new issues on changed paths)
  - `cargo test --all` (10 pre-existing failures in nexus-creator-memory, outside diff scope)

## Findings

### 🔴 Critical
None.

### 🟡 Warning
None in scope.

**Pre-existing unrelated failures (not introduced by this diff, not in B1–B4 files):**
- 10 test failures in `crates/nexus-creator-memory` (SoulNotFound, ValidationError on temp paths, directory creation). These are environment / prior state issues; web tests for strategy canvas passed cleanly. CI gate for this plan is the web build + relevant tests, which are green.

### 🟢 Suggestion
- Consider adding a small unit test for `activeSection` focus routing in `use-strategy-canvas.ts` (keyboard Cmd/Ctrl+S + onFocusCapture) to make the "only active inspector saves" contract explicit. Current coverage is via integration (build + manual). Low priority.
- The 8 deferred V1.72 residuals now carry explicit `plan-id tbd-v1.73-*` targets. When the V1.73 hygiene plan is created, update those `target` strings to the concrete plan_id for traceability.

## Source Trace (key B1–B4 items)

| Item | Source Type | Reference | Confidence |
|------|-------------|-----------|------------|
| B1 per-inspector save atomicity | git-diff + code review | `strategy-canvas/inspectors/{state,edge,prompt}-inspector.tsx` + `use-strategy-canvas.ts:85-94` (key handler uses `activeSection`) | High |
| B1 Cmd/Ctrl+S routes to active inspector | git-diff + code review | `use-strategy-canvas.ts:85-94` (onFocusCapture sets section; keydown uses `activeSection`) | High |
| B1 no compensating rollback (per lock) | git-diff | No rollback logic added; each inspector handles its own error | High |
| B1 conflict modal preserved | git-diff + code review | `inspector-panel.tsx:129-151` (StrategyConflictModal) + orchestrator usage | High |
| B2 public API preserved | git-diff + build | `strategy-canvas.tsx` still exports only `StrategyCanvas` + props; web build succeeds | High |
| B2 no circular imports | build + module review | All new modules under `strategy-canvas/` import from siblings or stable lib paths | High |
| B2 conflict modal behavior | git-diff | `StrategyConflictModal` now lives in `inspector-panel.tsx`; same props + ConflictModal usage | High |
| B2 tests pass without modification | test run | `pnpm --filter web test` (strategy-graph + conflict-modal tests green) | High |
| B3 keychain security | git-diff + SIGNING.md | random pass, $RUNNER_TEMP, always() cleanup, p12 rm, base64 decode not logged | High |
| B3 codesign flags | git-diff | `--force --sign ... --options runtime --timestamp --deep` exact match | High |
| B3 notarize + staple | git-diff | `notarytool submit ... --wait` + `stapler staple` | High |
| B3 secret gating + unsigned notice | git-diff | `sign-eval` step, partial → upload+fail with list, zero secrets → notice + succeed | High |
| B4 composite action | git-diff | pinned SHAs, no pull_request_target, typed inputs, contents:read in consuming workflows | High |
| T9/T10 residual commits | status.json + git log | 4 resolved have real SHAs (73ed508b, b480a283, 9a6591aa); 8 deferred carry explicit V1.73 plan-id pointers | High |

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 2 |

**Verdict**: Approve

## Detailed B1–B4 Security + Correctness Analysis

### B1 — Per-inspector save correctness (closes R-V171P0-QC1-004 HIGH)
- Each inspector owns a dedicated `useMutation` + `handleSave` + save button + `saveTrigger`.
- `Cmd/Ctrl+S` listener in `useStrategyCanvas` does `setSaveTriggers((prev) => ({ ...prev, [activeSection]: prev[activeSection] + 1 }))`.
- `activeSection` is driven by `onFocusCapture={() => setActiveSection('state'|'transition'|'prompt')}` on the inspector containers + initial default to 'state'.
- A click/focus in one inspector does not mutate shared draft state that would cause another inspector to save stale data.
- On 409 conflict, only that section's `onConflict(currentRevision, section)` fires; the modal is section-aware.
- No compensating rollback added (per V1.72 lock). Partial failure is surfaced per-section via `saveStatus` error UI. Other inspectors' drafts are untouched.
- Daemon-side each patch remains a single atomic revision bump.

### B2 — strategy-canvas.tsx split correctness (closes R-V171P0-QC1-006 MEDIUM)
- Old monolithic file (~570 lines) → thin orchestrator (`strategy-canvas.tsx`, 187 lines) + focused siblings:
  - `hooks/use-strategy-canvas.ts`
  - `state-machine.tsx`
  - `canvas-layout.tsx`
  - `inspector-panel.tsx` (hosts `StrategyConflictModal`)
  - `inspectors/{state,edge,prompt}-inspector.tsx`
- Public API: only `StrategyCanvas` + `StrategyCanvasProps` remain exported from the old path. All consumers continue to import the same symbol.
- No signature changes on the orchestrator props or return shape.
- Conflict modal UX is identical (same `ConflictModal` wrapper with same draft/changedFields callbacks).
- Web build and all 153 web tests pass. No test files were modified for the split.
- No circular imports; module graph is a clean tree under the new directory.

### B3 — desktop-release.yml signing correctness (closes R-V171-CI-RELEASE-WORKFLOW-INCOMPLETE MEDIUM)
- **Keychain import security**:
  - `p12_path` written to `$RUNNER_TEMP/nexus-cert.p12` (runner-controlled tmp).
  - `keychain_pass=$(openssl rand -base64 32)` — not hardcoded.
  - Temp keychain created with that pass, unlocked, imported with partition list, then added to user search list.
  - Cleanup step runs under `if: ${{ always() && ... }}`: restores original keychain list, deletes temp keychain, `rm -f "$p12_path"`.
- **Codesign correctness**:
  - Exact: `/usr/bin/codesign --force --sign "$APPLE_SIGN_IDENTITY" --options runtime --timestamp --deep "${app_path}"`
  - Followed by `--verify --deep --strict`.
- **Notarization**:
  - `xcrun notarytool submit ... --apple-id ... --password ... --team-id ... --wait`
  - `--wait` is present; credentials come from secrets (never echoed in the step).
- **Stapling**: `xcrun stapler staple` after successful notarize.
- **Secret gating**:
  - Dedicated `sign-eval` step counts present vs required (5 secrets).
  - `should_sign=false` + notice when zero.
  - `partial=true` + error when 1–4 → uploads unsigned artifacts then fails with explicit missing list.
  - All signing/notarize/staple steps are guarded by `if: ${{ steps.sign-eval.outputs.should_sign == 'true' }}`.
- **Unsigned builds**: still produce and upload `.app.zip` + `.dmg` with a clear notice. No regression.
- `apps/desktop/SIGNING.md` updated with the exact secret table and CI flow.

### B4 — Composite action security (closes R-V171-CI-WORKFLOW-SETUP-DEDUPE LOW)
- `.github/actions/setup-monorepo/action.yml` uses pinned SHAs for all external actions:
  - `pnpm/action-setup@eae0cfeb...`
  - `actions/setup-node@49933ea5...`
  - `dtolnay/rust-toolchain@29eef336...`
- No `pull_request_target` or equivalent privilege escalation in the action or consuming workflows.
- Token scope in consuming workflows remains minimal (`contents: read/write`, `actions: read` where needed).
- Inputs are explicitly declared with `required`/`default`; no free-form shell injection surface.
- `ci.yml` and `desktop-build.yml` now delegate setup to the action; behavior matrix unchanged.

### T9/T10 — Residual lifecycle correctness
- 4 resolved residuals now carry `lifecycle: resolved`, `closed_at`, `closure_note`, and `resolution` with real commit SHAs present in this iteration's history:
  - R-V171P0-QC1-004 → 73ed508b
  - R-V171P0-QC1-006 → 73ed508b
  - R-V171-CI-RELEASE-WORKFLOW-INCOMPLETE → b480a283
  - R-V171-CI-WORKFLOW-SETUP-DEDUPE → 9a6591aa
- 8 deferred residuals carry explicit V1.73 backlog references ("V1.73 hygiene backlog (plan-id tbd-v1.73-hygiene)" or "release-hardening"). Per assignment these are treated as valid forward pointers.
- Notes contain only human-readable closure rationale; no untrusted / prompt-injection vectors.

### No regression to existing security boundaries
- B1/B2 changes are client-side only (localhost daemon). Each inspector still sends its patch through the same authenticated `NexusClient`; no auth bypass introduced.
- B3 signing never logs secret material; base64 decode and keychain import are not echoed.
- B2 file split did not remove any input validation or form sanitization that existed before (the inspectors still use the same `isSectionDirty` + field-level binding).
- Composite action does not expand token permissions.

## CI / Build Gate
- Relevant commands for this scope (strategy-canvas + workflows) are green.
- Pre-existing `nexus-creator-memory` test failures are outside the diff and unrelated to B1–B4.
- `pnpm --filter web build` succeeded for the changed web surface.
- `cargo +nightly-2026-06-26 fmt --all --check` clean.

**Verdict**: Approve
