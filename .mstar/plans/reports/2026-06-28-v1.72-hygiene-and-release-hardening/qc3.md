---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: 2026-06-28-v1.72-hygiene-and-release-hardening
verdict: Request Changes
generated_at: 2026-06-28
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: volcengine-plan/ark-code-latest
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-06-28T09:17:14Z

## Scope
- plan_id: 2026-06-28-v1.72-hygiene-and-release-hardening
- Review range / Diff basis: `git diff 92a1c07f..HEAD -- .github/actions/setup-monorepo/ .github/workflows/ci.yml .github/workflows/desktop-build.yml .github/workflows/desktop-release.yml apps/desktop/SIGNING.md apps/web/src/components/canvas/strategy-canvas.tsx apps/web/src/components/canvas/strategy-canvas/ apps/web/src/lib/canvas/use-strategy-data.ts .mstar/status.json`
- Working branch verified: `iteration/v1.72`
- Review cwd verified: `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: assigned diff plus 16 directly read files
- Commit range: `92a1c07f..HEAD`
- Tools run:
  - `git rev-parse --show-toplevel`
  - `git branch --show-current`
  - assigned `git diff 92a1c07f..HEAD -- ...`
  - `cargo +nightly-2026-06-26 fmt --all --check`
  - `cargo clippy --all -- -D warnings`
  - `cargo test --all`
  - `pnpm --filter web typecheck`
  - `pnpm --filter web build`
  - `pnpm --filter web test`
  - `git log --oneline 73ed508b -1`, `git log --oneline b480a283 -1`, `git log --oneline 9a6591aa -1`
  - residual target parseability script over `.mstar/status.json`

## Findings

### 🔴 Critical
- None.

### 🟡 Warning

#### W-001 - Cmd/Ctrl+S save triggers can replay on later renders, duplicating inspector mutations
- Evidence: all three inspectors run `useEffect(() => { if (saveTrigger > 0) void handleSave(); }, [saveTrigger, handleSave]);` while `handleSave` is recreated every render (`state-inspector.tsx:85-117`, `edge-inspector.tsx:87-118`, `prompt-inspector.tsx:83-113`). The parent increments `saveTriggers[activeSection]` once and never acknowledges or resets it (`use-strategy-canvas.ts:85-94`).
- Trigger condition: focus a dirty inspector and press Cmd/Ctrl+S. After the first mutation starts or completes, mutation state and inline status updates cause re-renders; because `handleSave` has a new identity and `saveTrigger` remains greater than zero, the effect can invoke `handleSave` again for the same keyboard event while the form is still dirty against stale canonical data.
- Performance and reliability impact: a single shortcut can issue duplicate PATCH requests, extra revision bumps, or avoidable 409 conflicts. Prompt saves can duplicate file writes; transition saves can retry with the old `oldTarget` but a newer `baseRevision`. This undermines B1 independent-save reliability and can race against another inspector through the shared `workingRevisionRef`.
- Fix: make keyboard-trigger handling edge-triggered. Keep a `lastHandledTriggerRef` in each inspector and only process unseen trigger values, or reset and acknowledge the trigger after consumption. A stable `handleSave` via `useCallback` is also acceptable if the effect depends on the trigger edge rather than every render.

#### W-002 - Signing or notarization failures do not preserve/upload an unsigned fallback artifact, and job-level timeout can skip keychain cleanup
- Evidence: `desktop-release.yml` builds an unsigned DMG first, then runs keychain import, codesign, DMG recreation, notarytool `--wait`, and staple as ordinary fail-fast steps (`desktop-release.yml:93-157`). Packaging and upload run only after those steps succeed (`desktop-release.yml:159-177`). The unsigned DMG is removed during signed DMG recreation (`desktop-release.yml:135-137`). Cleanup is an `always()` step, but only after signing and notary steps and under the job-level `timeout-minutes: 120` (`desktop-release.yml:27,179-192`).
- Trigger condition: all five signing secrets are present, but certificate import, codesign timestamping, notarization, or stapling fails or hangs. Notarization can take minutes; a job-level timeout terminates the job rather than reliably running later cleanup.
- Performance and reliability impact: a release can end with no uploaded `.dmg` even though an unsigned artifact was successfully built, violating the requested degradation behavior. A timeout after keychain creation may also leave the temporary keychain/search-list mutation behind on the runner until VM teardown.
- Fix: copy the unsigned DMG to a fallback path before signing. Make packaging/upload run with an `always()` condition and upload the signed DMG when signing succeeds, otherwise upload the fallback unsigned DMG before failing the job with the signing error. Add step-level timeouts for keychain import, codesign, DMG recreation, notarization, and staple so the cleanup step can run before the overall job timeout.

### 🟢 Suggestion

#### S-001 - Desktop release should add Rust cache coverage or a composite cache input before real signed releases
- `setup-monorepo` caches pnpm via `actions/setup-node` (`action.yml:40-44`), and `desktop-build.yml` preserves separate `Swatinem/rust-cache` steps (`desktop-build.yml:53-59,95-100`). `desktop-release.yml` now uses the composite action but has no Rust cache step before its universal Tauri build (`desktop-release.yml:61-72`). This is not a correctness blocker, but signed release jobs are the most expensive path; adding `Swatinem/rust-cache` to `desktop-release.yml` or making the composite action optionally install Rust cache would reduce timeout pressure.

#### S-002 - Status residual closure prose overstates the composite action contents
- `.mstar/status.json` says the B4 composite action covers checkout and rust-cache, but `.github/actions/setup-monorepo/action.yml` currently covers pnpm setup, Node setup with pnpm cache, optional Rust toolchain, and `pnpm install`; checkout and Rust cache remain workflow-owned. This does not affect runtime behavior, but the lifecycle note should be corrected by PM/QA so future automation does not infer nonexistent composite-action cache inputs.

## Additional performance/reliability checks
- B1 per-inspector isolation: state, transition, and prompt now have separate `useMutation` instances and separate `saveStatuses`; UI buttons are independently disabled based on each section `patch.isPending`. Manual button clicks are isolated; the keyboard-trigger replay issue above is the remaining blocker.
- B2 split performance: the public orchestrator is 187 lines and sibling modules are each under 200 lines. The route remains lazy-loaded through `App.tsx`, so React Flow stays out of the bootstrap chunk. Import review found no circular dependencies among split modules. `pnpm --filter web build` produced `strategy-page-DtwB_V_A.js` at 320.19 kB minified and 100.53 kB gzip, essentially matching the prior V1.71 baseline.
- B3 signing flow: partial secrets are detected before signing and list missing secret names; no-secrets mode emits an unsigned-build notice. The fallback behavior for failures after all secrets are present is the blocking gap captured in W-002.
- B4 matrix/caching: CI matrix groups in `ci.yml` are preserved; `desktop-build.yml` keeps the desktop-build and served-UI-smoke jobs and their separate Rust cache steps. pnpm cache is handled by `setup-node cache: pnpm` in the composite action.
- Track A regression: Outline canvas still imports conflict behavior only via the public `@/components/canvas/outline-conflict-modal` facade, which reuses `conflict-modal-base`; P1 Strategy split does not require Outline to import private Strategy modules.
- T9/T10 lifecycle reliability: resolution commits `73ed508b`, `b480a283`, and `9a6591aa` exist in `git log`. The eight V1.73 deferred residual targets all include parseable `plan-id ...` markers. The `.mstar/status.json` diff is 68 insertions / 40 deletions and limited to lifecycle, target, and summary updates.

## Source Trace
- Finding ID: W-001
  - Source Type: manual-reasoning + git-diff
  - Source Reference: `apps/web/src/components/canvas/strategy-canvas/hooks/use-strategy-canvas.ts:85-94`; `apps/web/src/components/canvas/strategy-canvas/inspectors/state-inspector.tsx:85-117`; `edge-inspector.tsx:87-118`; `prompt-inspector.tsx:83-113`
  - Confidence: High
- Finding ID: W-002
  - Source Type: manual-reasoning + workflow review
  - Source Reference: `.github/workflows/desktop-release.yml:27,93-157,159-192`
  - Confidence: High
- Finding ID: S-001
  - Source Type: workflow review
  - Source Reference: `.github/actions/setup-monorepo/action.yml:40-55`; `.github/workflows/desktop-build.yml:53-59,95-100`; `.github/workflows/desktop-release.yml:61-72`
  - Confidence: Medium
- Finding ID: S-002
  - Source Type: status lifecycle review
  - Source Reference: `.mstar/status.json` lifecycle note for `R-V171-CI-WORKFLOW-SETUP-DEDUPE`; `.github/actions/setup-monorepo/action.yml`
  - Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 2 |

**Verdict**: Request Changes
