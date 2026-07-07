---
report_kind: qa
plan_id: 2026-07-07-v1.96-implement-rework
verdict: Pass with notes
generated_at: 2026-07-07T13:48:13+0000
---

# QA Report — 2026-07-07-v1.96-implement-rework

## Scope (text-identical to QC tri)

- **plan_id**: `2026-07-07-v1.96-implement-rework`
- **Working branch**: `feature/v1.96-implement-rework`
- **Review cwd**: `/Users/bibi/workspace/organizations/42ch/nexus`
- **Review range / Diff basis**: `merge-base: f9b73d27` + `tip: HEAD (42af8737)`

**Compass**: `.mstar/iterations/v1.96-setup-wizard-rework-and-daemon-diagnostic-delivery-compass-v1.md` §5 (8 acceptance criteria).

**QC consolidated reference**: `.mstar/plans/reports/2026-07-07-v1.96-implement-rework/qc-consolidated.md` (Approve after fix-wave 1; 5 low/nit residuals registered).

## Automated verification suite (run once)

| Command | Result | Evidence |
|---------|--------|----------|
| `pnpm --filter web run test` | 546 passed (75 files) | 12.41s run; all 75 test files green |
| `pnpm --filter web run typecheck` | clean | `tsc --noEmit` exit 0 |
| `pnpm --filter web run build` | clean | `✓ built in 3.35s`; no errors |
| `pnpm -w run sidecar` (prereq) | success | binaries staged for aarch64 + x86_64 |
| `cd apps/desktop/src-tauri && cargo test` | 36 passed | `test result: ok. 36 passed` (incl. `sidecar::tests::*` stderr/daemon paths) |
| `cd apps/desktop/src-tauri && cargo clippy -- -D warnings` | clean | exit 0; no warnings |
| `cargo +nightly-2026-06-26 fmt --all --check` | clean | exit 0 (no output) |

All commands executed from repo root on `feature/v1.96-implement-rework` @ 42af8737.

## Branch & alignment verification

- `git branch --show-current` → `feature/v1.96-implement-rework`
- `git rev-parse HEAD` → `42af8737`
- `git merge-base f9b73d27 HEAD` → `f9b73d27` (matches assignment)

## Spot-checks against compass §5 criteria (key files)

1. **Desktop app opens without "The string did not match the expected pattern."** (V1.95 regression)
   - Verified via full test/build suite (ClientProvider immediate TauriClient path preserved from V1.95).
   - No regression in `apps/web/src/lib/nexus/tauri-client.test.ts` or desktop gate tests.

2. **Step 1 Browse no longer shows `[object Object]`**
   - `apps/web/src/lib/error-message.ts`:
     ```ts
     export function errorMessage(err: unknown): string {
       if (err && typeof err === 'object' && 'message' in err) { ... }
       if (err instanceof Error) return err.message;
       ...
     }
     ```
   - `setup-step-welcome.tsx:54-56`: `const message = errorMessage(err) || ...; toast(...)`
   - Same helper used in `setup-step-daemon.tsx`, `setup-gate.tsx`, `main-banner.tsx` (swept in T1/T2 + fix-wave 1 per QC).

3. **Step 1 layout: Browse inline; errors via toast; Continue wide bottom**
   - `setup-step-welcome.tsx:90-111`: single `workspace-location-row` (FolderOpen + label + path + Browse button inline).
   - Errors → `useToast()` (no inline `<p role="alert">`).
   - Continue: `<Button variant="primary" className="w-full max-w-...">` at bottom of card (line 114-121).
   - DESIGN tokens: `setup-wizard-surface-input-row-*`, `cta-primary-max-width`.

4. **Step 2 wizard never hangs in "Starting daemon…" (≤30s timeout)**
   - `setup-step-daemon.tsx:34-41`: explicit `status.state === 'starting'` branch (sets ready=false, error=null).
   - 25s timeout (line 77-103): probes status or surfaces "taking longer than expected" + Retry/Reset.
   - Mount-probe + subscription race fixed (T4).

5. **Step 2 surfaces real daemon error (verbatim stderr in detail)**
   - `sidecar.rs:257-274`: `drain_stderr` + `stderr_tail` retained (2 KiB cap).
   - `format_error_detail` (line 544-547): `"{message}\n\nDaemon output:\n{stderr}"`.
   - `setup-step-daemon.tsx:146-148`: `<p className="whitespace-pre-wrap ...">{error}</p>` renders verbatim.
   - Test: `setup-step-daemon.test.tsx` "renders error detail verbatim including stderr tail".
   - Closes `R-V195-ARCH-STRERR-GAP`.

6. **Layout centered + integrated; circle/label aligned**
   - `setup-wizard-page.tsx:63-67`: outer `items-center justify-center`; inner card `max-w-setup-wizard-step-wizard-max-width rounded-popover border ...`.
   - StepIndicator + content inside single chrome (border + bg + shadow).
   - `StepIndicator` rows: circle + label baseline-aligned (T7 fix + DESIGN tokens).

7. **DESIGN.md "Setup Wizard Surface" at Level 3 Production; components consume tokens**
   - `apps/web/DESIGN.md:889-969`: full "Setup Wizard Surface (V1.96 — Level 3 Production)" section (light + dark).
   - Tokens: `setup-wizard-surface:*` (step-panel-width, input-row-*, cta-*, card-*) + `setup-wizard-step`.
   - `apps/web/DESIGN.dark.md` mirrors names with dark values.
   - All wizard components (`setup-step-*.tsx`, `setup-wizard-page.tsx`) use `--color-setup-wizard-surface-*` / `setup-wizard-surface-*` Tailwind keys (no ad-hoc values).
   - Verified in `tailwind.config.ts` + `index.css`.

8. **Author wipe-and-relaunch smoke (`rm -rf ~/.nexus42/`)**
   - **DEFERRED** (explicit per compass §5 note and QC consolidated § "Acceptance criteria coverage").
   - "code-level; manual smoke deferred as human gate" — author will execute post-merge.
   - All code-level paths (Steps 1-2, error surfaces, timeout, stderr, layout) covered by automated + spot checks above.

## Residuals verification

```bash
python3 -c "..."
# V1.96 residuals count: 5
# V1.95 closed check: ['R-V195-ARCH-STRERR-GAP:resolved', 'R-V195QC3-S001:resolved']
```

- `status.json` root `residual_findings['2026-07-07-v1.96-implement-rework']`: exactly 5 low/nit (R-V196QC3-W001, R-V196QC1-S002, S004, R-V196QC3-S001, R-V196QC1-S001) — matches QC consolidated.
- V1.95 residuals `R-V195-ARCH-STRERR-GAP` and `R-V195QC3-S001` now `lifecycle: resolved` (closed by this plan).

## QC context (for traceability)

- Consolidated verdict: **Approve** (after 1 fix-wave).
- qc3 C-1 fixed in `acbbbe1a` (errorMessage usage in finish paths); revalidation passed.
- All other seats: Approve (0C remaining).
- 2 accepted design decisions (stderr verbatim in local trust boundary; errorMessage ordering).

## Not verified / caveats

- Criterion 8 (manual wipe-and-relaunch smoke): explicitly **DEFERRED** to author (post-merge human gate). Acceptable per compass §5.
- No desktop GUI launch in this CI-like env (sidecar + unit tests only); full end-to-end author smoke required for final human sign-off.
- No new residuals introduced by QA (only spot verification + test runs).

## Verdict

**Pass with notes**

- All 7 automated + code-level criteria (1-7) fully verified.
- Full test/lint/build suite green.
- Residuals tracked (5 open for V1.96; 2 V1.95 closures confirmed).
- Criterion 8 deferred per explicit compass guidance.
- Ready for plan Done + merge (author manual smoke is the final human gate).

**Recommended next**: author executes `rm -rf ~/.nexus42/` smoke on target desktop; then PM closes plan.
