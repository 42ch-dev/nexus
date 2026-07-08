---
report_kind: qc-consolidated
plan_id: "2026-07-07-v1.97-desktop-first-launch-hardening"
verdict: "Approve with residuals"
generated_at: "2026-07-08"
---

# V1.97 Plan QC Consolidated (tri-review)

## Scope reviewed

- plan_id: `2026-07-07-v1.97-desktop-first-launch-hardening`
- Review range / Diff basis: `merge-base: 070e26f7 (main, iteration base) + tip: ab618ee9 (feature/v1.97-desktop-first-launch-hardening HEAD)`; equivalent to `git diff 070e26f7...ab618ee9`
- 5 commits, 16 files, +564/-18
- Tri-review dispatched same message, identical scope fields (qc1/qc2/qc3).

## Per-seat verdicts

| Seat | Focus | Verdict | Critical | Warning | Suggestion |
|------|-------|---------|----------|---------|------------|
| qc1 | Architecture + maintainability | Approve | 0 | 0 | 3 |
| qc2 | Security + correctness | Approve | 0 | 0 | 1 |
| qc3 | Performance + reliability | Approve with residuals | 0 | 2 | 3 |

**Consolidated verdict: Approve with residuals** — no Critical, no blocking Important. All findings are non-blocking (medium/low/nit).

## What V1.97 delivers (verified)

1. **Sidecar FSM correctness** — `SidecarManager::new` starts `Stopped`; `start_with_budget` short-circuits `Starting` only when `inner.child.is_some()`; attach-to-healthy-daemon preserves `owned: false` (no fabricated child); `Stopped`/`Error` retryable; stop/quit terminates only owned children. All 5 Architecture Invariants hold (qc1+qc3).
2. **Sidecar spawn-name fix** (`ab618ee9`) — `app.shell().sidecar("binaries/nexus42")` → `sidecar("nexus42")` per Tauri v2 docs ("expects only the filename, not its full path"); capability-scope `name` coordinated to match; `bundle.externalBin` build-time path preserved. Verified: original `os error 2` gone, daemon process now reached at spawn. This was a latent first-launch blocker (the desktop app could never spawn its bundled daemon before V1.97).
3. **Folder-picker IPC** — `pickDirectory` sends `{ defaultPath }` camelCase (Tauri v2 default camelCase→snake_case); regression test pins the key (proven red on `default_path`).
4. **Setup-wizard layout containment** — card `overflow-hidden`, content `min-w-0`, path `min-w-0`+`truncate`, button `flex-shrink-0`; pure utility composition, no DESIGN.md token drift (T1 rejected the `index.css` token-value drift).
5. **Path hygiene** — no `/Users/bibi` or machine-local paths in `apps/` code/tests; generic fixtures only.
6. **Scope discipline** — no schema / `@42ch/nexus-contracts` / daemon-API / daemon-runtime changes; QC1 verified `crates/nexus-daemon-runtime/`, `apps/nexus42/`, `schemas/` untouched in the diff.

## Residuals registered (status.json `residual_findings[2026-07-07-v1.97-desktop-first-launch-hardening]`)

| ID | Severity | Decision | Target | Source |
|----|----------|----------|--------|--------|
| R-V197-SMOKE-CLEAN-STATE | high | defer | V1.98 | smoke + qc1 S-1 |
| R-V197-SMOKE-UI | medium | defer | V1.98 | smoke env limitation |
| R-V197QC3-W001 | low | defer | V1.98+ | qc3 W-001 |
| R-V197QC3-W002 | nit | defer | V1.98 | qc3 W-002 |
| R-V197-CLIPPY | low | defer | V1.98 hygiene | qc1 S-3 (pre-existing `connection_config.rs:198`) |

### Residual detail

- **R-V197-SMOKE-CLEAN-STATE (high)** — Clean-state desktop first-launch cannot reach a working state: `lib.rs:534` `.setup()` auto-starts the daemon unconditionally on app launch; the daemon exits `No active creator configured`; the desktop wizard has no `create_creator`/`init_workspace` invoke to bootstrap the creator before the daemon starts. This is a pre-existing product/architecture gap newly exposed by the V1.97 smoke (the spawn-name fix made the daemon reachable, which surfaced this next gate). Deferred to V1.98 (architect design: gate daemon auto-start behind `setup_completed` + add creator-bootstrap to the wizard). The V1.97 sidecar-name fix is itself a major reliability win regardless.
- **R-V197-SMOKE-UI (medium)** — Full desktop UI smoke (clean-state + existing-install wizard observation / state transitions) requires an interactive macOS host; the headless OpenCode session cannot drive the native Tauri window. The attach path + unit tests are verified; the wizard/UI flow is not. Deferred to V1.98 manual/automated desktop smoke.
- **R-V197QC3-W001 (low)** — Stderr drain dual-storage (`Arc<Mutex<String>>` + `inner.stderr_tail`) is confusing but not buggy; preserves the V1.96 R-V196QC3-W001 sub-ms race, neither worsened nor fixed.
- **R-V197QC3-W002 (nit)** — Missing test for `start()` (auto, no budget reset) from `Error` state; same code path as `start_daemon`, low risk.
- **R-V197-CLIPPY (low)** — Pre-existing clippy `io-other-error` lint at `apps/desktop/src-tauri/src/connection_config.rs:198`; not introduced by V1.97. V1.98 hygiene.

### Folded (minor, not SSOT-registered)

- qc1 S-2 (nit): `start_with_budget` doc polish.
- qc2 S-1 (nit): naming-distinction comment between `externalBin` artifact path and runtime `sidecar()` name.

## Cannot verify (carried to QA gate)

- Clean-state desktop smoke — blocked by R-V197-SMOKE-CLEAN-STATE (deferred V1.98).
- Existing-install desktop UI smoke — blocked by R-V197-SMOKE-UI (headless; deferred V1.98).

## PM decision

User direction (this iteration-drive): **ship verified fixes, defer the no-creator gap + UI smoke to V1.98.** V1.97 resolves as *delivered with documented hard-gate carry-over* rather than full-smoke-satisfied Done. The sidecar-name fix (`ab618ee9`) is a critical first-launch reliability fix that lands now. Mandatory QA gate runs acceptance on the verifiable surface (unit tests, build, spawn-name fix, attach path) with the smoke carry-over explicitly registered, not waived.

## Next

- PM registers residuals in `status.json`.
- Dispatch `qa-engineer` (QA gate: mandatory, QA mode: acceptance-only on verifiable surface + smoke carry-over documented).
- On QA acceptance: merge `feature/v1.97-desktop-first-launch-hardening` → `iteration/v1.97`, mark plan Done, open V1.98 plan stub, proceed to Phase 3 (iteration-close).
