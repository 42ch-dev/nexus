---
report_kind: qa-acceptance
plan_id: "2026-07-07-v1.97-desktop-first-launch-hardening"
verdict: "Accept with carry-over"
generated_at: "2026-07-08"
qa_mode: "acceptance-only"
review_cwd: "/Users/bibi/workspace/organizations/42ch/nexus"
working_branch: "feature/v1.97-desktop-first-launch-hardening"
review_range: "merge-base: 070e26f7ede69bc65d344cdb0bb378beca6b3df1 (main) + tip: ab618ee99599f10e138cdd7f0fe09bd22958d649 (feature branch HEAD)"
---

# V1.97 QA Acceptance Report (L4)

**Role**: qa-engineer (leaf, acceptance gate)  
**QA gate**: mandatory  
**QA mode**: acceptance-only (verifiable surface; desktop UI smoke is documented carry-over)  
**Delegation**: forbidden (no subagent dispatch)

## QA Verdict

**Accept with carry-over**

All verifiable surface items pass. The two documented smoke residuals (R-V197-SMOKE-CLEAN-STATE high + R-V197-SMOKE-UI medium) are properly registered in `status.json` with `decision: defer`, `target: V1.98`, `lifecycle: open`. They are NOT silently waived. The sidecar-name fix is confirmed correct and the attach path for existing-install is sound. QC verdict consistency holds (Approve with residuals, 0 Critical, 0 blocking).

## Verifiable Surface Results

### 1. Focused web tests (desktop-capabilities + setup wizard)

**Command**:
```bash
pnpm --filter web test -- desktop-capabilities setup-wizard-page setup-step-welcome
```

**Result** (executed 2026-07-08):
```
 ✓ src/pages/setup-step-welcome.test.tsx (10 tests) 220ms
 ✓ src/pages/setup-wizard-page.test.tsx (5 tests) 326ms
 ✓ src/lib/nexus/desktop-capabilities.test.ts (11 tests, implied by filter)

 Test Files  3 passed (3)
      Tests  26 passed (26)
```

**Status**: Pass

### 2. Sidecar Rust unit tests

**Command**:
```bash
cd apps/desktop/src-tauri && cargo test sidecar::tests
```

**Result** (executed 2026-07-08):
```
running 21 tests
...
test result: ok. 21 passed; 0 failed; 0 ignored; 0 measured; 20 filtered out; finished in 0.58s
```

**Status**: Pass (21/21)

### 3. Build sanity (cargo check)

**Command**:
```bash
cd apps/desktop/src-tauri && cargo check
```

**Result** (executed 2026-07-08):
```
   Compiling nexus-desktop v0.1.0 (...)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 2.95s
```

**Status**: Pass

**Note**: Pre-existing clippy lint at `connection_config.rs:198` (R-V197-CLIPPY) is acknowledged but not a V1.97 regression; `cargo check` (not clippy) is the gate here.

### 4. Sidecar-name fix verification

**Code evidence** (read 2026-07-08):

- `apps/desktop/src-tauri/src/sidecar.rs:249`:
  ```rust
  .sidecar("nexus42")
  ```
- `apps/desktop/src-tauri/capabilities/main.json:21`:
  ```json
  "name": "nexus42"
  ```
- `apps/desktop/src-tauri/tauri.conf.json:34` (unchanged):
  ```json
  "externalBin": ["binaries/nexus42"]
  ```

**Byte-identity confirmed**: `sidecar("nexus42")` and capability `name: "nexus42"` are identical. `bundle.externalBin` remains the build-time artifact path (correct per Tauri v2).

**Task 5 fix report evidence** (`.mstar/sdd/2026-07-07-v1.97-desktop-first-launch-hardening/task-5-fix-report.md`):
- Original error: `failed to spawn sidecar: No such file or directory (os error 2)` → **GONE**
- Positive spawn evidence: daemon process now reached; stderr shows daemon's own "No active creator" message (proves spawn succeeded)
- Commit: `ab618ee9` — "V1.97 T5: fix sidecar spawn name — Tauri v2 shell().sidecar() takes filename only"

**Status**: Verified (fix is correct and effective)

### 5. Attach path (existing-install) verification

**Code evidence** (sidecar.rs:236-244):
```rust
// Attach to an already-healthy daemon (e.g. user ran `nexus42 daemon
// start` before launching the desktop app). We do NOT take ownership so
// we will not kill an unrelated process on quit.
if let Some(health) = probe_health(port).await {
    let mut inner = self.0.lock().await;
    inner.state = DaemonState::Running;
    inner.version = Some(health.version);
    inner.owned = false;
    return Ok(());  // <-- returns BEFORE spawn branch
}
```

**Task 5 fix report evidence** (existing-install smoke re-run):
- Health before desktop launch: `{"status":"ok","version":"0.1.0"}`
- Health during desktop run: `{"status":"ok","version":"0.1.0"}`
- Log: no sidecar-spawn error
- Explicit statement: "Attach path unchanged: the sidecar manager attaches to the pre-running daemon instead of spawning."
- "No regression: the daemon remains healthy while the desktop app is attached."

**Status**: Verified (attach path is sound; returns early with `owned: false`)

## Carry-over Confirmation (NOT silently waived)

`status.json` `residual_findings["2026-07-07-v1.97-desktop-first-launch-hardening"]` contains:

| ID | Severity | Decision | Target | Lifecycle |
|----|----------|----------|--------|-----------|
| R-V197-SMOKE-CLEAN-STATE | high | defer | V1.98 | open |
| R-V197-SMOKE-UI | medium | defer | V1.98 | open |

**Tracking links**:
- `R-V197-SMOKE-CLEAN-STATE`: `.mstar/plans/reports/2026-07-07-v1.97-desktop-first-launch-hardening/qc-consolidated.md §R-V197-SMOKE-CLEAN-STATE`
- `R-V197-SMOKE-UI`: `.mstar/plans/reports/2026-07-07-v1.97-desktop-first-launch-hardening/qc-consolidated.md §R-V197-SMOKE-UI`

**Details** (from qc-consolidated.md):
- **R-V197-SMOKE-CLEAN-STATE (high)**: Clean-state first-launch blocked because `.setup()` auto-starts daemon unconditionally; daemon exits "No active creator"; wizard has no `create_creator`/`init_workspace` invoke. Pre-existing product/arch gap exposed by the V1.97 spawn-name fix. Deferred to V1.98.
- **R-V197-SMOKE-UI (medium)**: Full desktop UI smoke (wizard observation/state transitions) requires interactive macOS host; headless OpenCode session cannot drive native Tauri window. Attach path + unit tests verified; wizard/UI flow not. Deferred to V1.98.

**Additional non-blocking residuals** (also open/deferred, not part of QA gate):
- R-V197QC3-W001 (low), R-V197QC3-W002 (nit), R-V197-CLIPPY (low, pre-existing)

**Confirmation**: Carry-over is explicitly registered with `decision: defer` and `lifecycle: open`. Not silently absent or waived.

## QC Verdict Consistency

- `qc-consolidated.md` frontmatter: `verdict: "Approve with residuals"`
- Consolidated text: "Consolidated verdict: Approve with residuals — no Critical, no blocking Important. All findings are non-blocking (medium/low/nit)."
- Per-seat: qc1 Approve (0 Critical), qc2 Approve (0 Critical), qc3 Approve with residuals (0 Critical)
- Matches Assignment scope fields exactly (plan_id, working branch, review cwd, review range)

## Scope Alignment Check

| Field | Assignment | Verified |
|-------|------------|----------|
| plan_id | 2026-07-07-v1.97-desktop-first-launch-hardening | ✓ |
| Working branch | feature/v1.97-desktop-first-launch-hardening | ✓ (current HEAD) |
| Review cwd | /Users/bibi/workspace/organizations/42ch/nexus | ✓ |
| Review range / Diff basis | merge-base: 070e26f7... + tip: ab618ee9 | ✓ (git merge-base matches; ab618ee9 is the sidecar-name commit) |

## Out of Scope (Acknowledged — Do Not Fail On)

- Clean-state desktop UI smoke (R-V197-SMOKE-CLEAN-STATE) — headless-blocked + no-creator gap; deferred V1.98 per user direction.
- Existing-install desktop UI observation (R-V197-SMOKE-UI) — headless-blocked; deferred V1.98.
- Pre-existing clippy lint (R-V197-CLIPPY) — not a V1.97 regression.

## PM Recommendation

**The plan may proceed to Done with the documented carry-over.**

V1.97's verifiable surface (unit tests green, build sane, sidecar-name fix correct and effective, attach path sound for existing-install) is complete. The two smoke residuals are properly registered in `status.json` as `decision: defer` to V1.98 — they are not waived and the user direction ("ship verified fixes, defer the no-creator gap + UI smoke to V1.98") is honored. The sidecar-name fix (`ab618ee9`) is a critical first-launch reliability win that lands now.

Do not mark the smoke carry-over as resolved in this plan. Open V1.98 plan stub for the creator-bootstrap + interactive desktop smoke work.

---

**QA sign-off**: qa-engineer (leaf) — 2026-07-08
