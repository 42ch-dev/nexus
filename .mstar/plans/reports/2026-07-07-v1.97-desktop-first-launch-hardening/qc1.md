---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-07-07-v1.97-desktop-first-launch-hardening"
verdict: "Approve"
generated_at: "2026-07-08"
---

# Code Review Report

## Reviewer Metadata

- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: minimax-cn-coding-plan/MiniMax-M3
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-07-08 (Asia/Shanghai)

## Scope

- plan_id: `2026-07-07-v1.97-desktop-first-launch-hardening`
- Review range / Diff basis: `merge-base: 070e26f7ede69bc65d344cdb0bb378beca6b3df1 (main, iteration base) + tip: ab618ee99599f10e138cdd7f0fe09bd22958d649 (feature branch HEAD)`; equivalent to `git diff 070e26f7...ab618ee9`
- Working branch (verified): `feature/v1.97-desktop-first-launch-hardening`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 16 (564 insertions, 18 deletions; `git diff --stat 070e26f7...ab618ee9`)
- Commit range (identical to Review range): `070e26f7..ab618ee9`
  - `265e062e` V1.97 — Desktop First-Launch Reliability Hardening
  - `66cd3be8` T1: prototype intake — owned baseline, path hygiene, intake ledger
  - `c4b365da` T3: setup wizard layout containment verification
  - `b3d06b48` T4: sidecar startup state machine verification
  - `ab618ee9` T5: fix sidecar spawn name — Tauri v2 `shell().sidecar()` takes filename only
- Tools run: `git rev-parse`, `git log`, `git diff --stat`, `git log --pretty`, `rg`, `grep`, `wc -c .mstar/status.json`, read tool on changed files, Tauri v2 docs lookup (Context7).
- **Deep review: triggered (S1: 564 lines / 16 files, S6: ≥3 module boundaries — `apps/desktop/src-tauri/`, `apps/web/src/`, `.mstar/`)**
- **Lenses applied:** Modularity Lens + Contract Lens (QC1 default); Contract Lens re-applied specifically to the `sidecar()` / `bundle.externalBin` / `capabilities/main.json` triple and the `defaultPath` casing fix.

## Tri-Alignment Cross-Check

| Field | PM Assignment | This report |
|-------|---------------|-------------|
| `plan_id` | `2026-07-07-v1.97-desktop-first-launch-hardening` | matches |
| `Review range / Diff basis` | `merge-base: 070e26f7 + tip: ab618ee9 (= git diff 070e26f7...ab618ee9)` | matches verbatim |
| `Working branch` | `feature/v1.97-desktop-first-launch-hardening` | matches `git branch --show-current` |
| `Review cwd` | `/Users/bibi/workspace/organizations/42ch/nexus` | matches `git rev-parse --show-toplevel` |
| `Reviewer focus` | architecture coherence + maintainability | applied (default Modularity + Contract) |

## Architecture Review — Sidecar FSM (V1.97 five invariants)

Each invariant is mapped to its evidence site and test coverage.

### Invariant 1 — `SidecarManager::new` initial state = `Stopped`

- Evidence: `apps/desktop/src-tauri/src/sidecar.rs:111` `state: DaemonState::Stopped` (was `Starting` in the pre-fix prototype).
- Existing init also sets `owned: false`, `child: None`, `restart_count: 0` (`sidecar.rs:114-119`) — the "no owned child" half of the invariant.
- Test: `new_manager_starts_from_stopped_state` (`sidecar.rs:710-714`) asserts `manager.status().await.state == Stopped`.
- Architectural call chain: `lib.rs:507` constructs the manager before `setup()` (which then calls `start_with_budget`). Between `new()` and the `setup()` closure dispatching `start()`, the wizard probes via `get_daemon_status`. With the old `state=Starting`, that brief window could be observed as "starting" before any work was attempted — now the manager transparently reports `Stopped` until `start_with_budget` flips it to `Starting`. Clean.
- Verdict: ✅ invariant holds.

### Invariant 2 — `Starting` only suppresses when `inner.child.is_some()`

- Evidence: `start_with_budget` early-return at `sidecar.rs:219-224`:
  ```rust
  if inner.state == DaemonState::Running
      || (inner.state == DaemonState::Starting && inner.child.is_some())
  {
      return Ok(());
  }
  ```
  This rewrites the prior guard (`state == Running || state == Starting`) into the child-handle-conditional form.
- Test: `starting_without_child_does_not_suppress_attach` (`sidecar.rs:900-925`) directly preconditions `state = Starting; owned = false; child = None;` then asserts attach proceeds (`state == Running; !owned; child.is_none()`).
- Test: `new_manager_start_attaches_when_health_ready` (`sidecar.rs:880-897`) covers the natural entry path (Stopped → attach).
- Architectural observation: there is a small redundancy at `sidecar.rs:225` — when we fall through from the early-return because `Starting && child.is_none()`, the next line reassigns `inner.state = DaemonState::Starting;` to the same value. That is intentional but worth flagging as a minor cleanliness point; I'd rather the fall-through be expressed as "reset diagnostic + transit to a fresh Starting" rather than "re-assign Starting". See Suggestion S-2 below.
- Verdict: ✅ invariant holds.

### Invariant 3 — Attach vs own: no fabricated ownership

- Evidence: attach-only branch at `sidecar.rs:239-245` sets `state = Running`, `owned = false`, and returns without ever touching `inner.child`. The variable `child` is established solely by the spawn path (`sidecar.rs:259-282`) where `inner.child = Some(child); inner.owned = true;`.
- Test coverage:
  - `new_manager_start_attaches_when_health_ready` (`sidecar.rs:880-897`) asserts `!inner.owned` after attach.
  - `starting_without_child_does_not_suppress_attach` (`sidecar.rs:900-925`) re-asserts the same property on the no-child-fallthrough path.
  - `stop_is_noop_for_unowned_manager` (`sidecar.rs:745-758`) — preexisting — confirms `stop()` returns early if `!owned`, never reaching `kill`.
- Architectural observation: the `set_app_handle()` -> `start_with_budget()` -> probe -> attach sequence is race-safe because everything happens under the same async `Mutex`; only one transition can occur per lock acquisition. The `status()` method has its own compensating probe for `Running && !owned` (line 158-187) that demotes to `Error` on probe failure — this is the bounded-recovery half of the invariant. Well composed.
- Verdict: ✅ invariant holds.

### Invariant 4 — Retryability of `Stopped` and `Error`

- Evidence: `start_with_budget` does NOT include `Stopped` or `Error` in its early-return set. `reset_budget=true` (the manual `start_daemon` entry) resets `restart_count` (`sidecar.rs:229-231`).
- Test: `start_daemon_resets_crash_budget` (`sidecar.rs:854-877`) — preexisting — pins `Stopped + restart_count=3 → start_daemon → Running; restart_count=0`.
- Test: `error_state_retries_and_attaches_when_health_ready` (`sidecar.rs:928-954`) — newly added in T4 — covers the `Error` half: `state=Error; restart_count=MAX_RESTART_ATTEMPTS; → start_daemon → Running; restart_count=0; detail=None`.
- Architecturally the retryability depends on the absence of any "I'm giving up" flag in `start_with_budget` itself. The crash-budget exhaustion is enforced only inside `handle_crash` (`sidecar.rs:402-466`) — i.e. retrying from an `Error` daemon through a manual click is intentionally always allowed regardless of `restart_count`. This is correctly capped at the wrong layer (process supervisor, not the manager) so the manager's own surface stays linear.
- Verdict: ✅ invariant holds.

### Invariant 5 — Wizard observation via existing wire only; no schema/contract changes

- Evidence:
  - State surfacing still goes through `DaemonStatus { state, version, port, detail }` (`sidecar.rs:71-76`) and `nexus://daemon-status-changed` event (`sidecar.rs:38`).
  - No new fields, no new event names, no `@42ch/nexus-contracts` consumer nor producer added.
  - `git log 070e26f7..ab618ee9 -- crates/nexus-daemon-runtime/ apps/nexus42/ schemas/` returns empty — confirming no daemon-runtime, no CLI, no schema touched.
- Test: `daemon_state_serializes_to_lowercase` (`sidecar.rs:841-851`) pins the existing wire format.
- Verdict: ✅ invariant holds.

### FSM coherence across the lifecycle

The four transitions are well-composed:

| Pre-condition (state, child) | Action | Post-condition |
|------------------------------|--------|----------------|
| `Running` | early-return `Ok` | unchanged |
| `Starting + child` | early-return `Ok` | unchanged |
| `Starting + !child` (or any other state) | fall through → probe → attach or spawn | fresh `Starting` → `Running` (attach: `!owned`) or `Error` |
| `Error` after backoff exhaustion | `handle_crash` lands `Stopped` | idle, ready for manual restart |

The FSM does not admit an "I'm-giving-up" terminal that would freeze the manager; the deepest dead state (`Error`) is retryable by construction. Failures are bounded by the `MAX_RESTART_ATTEMPTS` counter in the monitor, not in the manager — the right division of labor for a sidecar supervisor.

## Architecture Review — Spawn-name fix (T5)

### `sidecar()` arg + capability scope `name` coordination

- `apps/desktop/src-tauri/src/sidecar.rs:249` `app.shell().sidecar("nexus42")` — runtime sidecar name is the **filename only** in Tauri v2 (verified against Context7 doc snippet for `calling-rust.mdx`; Tauri v2 builds the target-triple-suffixed artifact from `bundle.externalBin` and the runtime API resolves by base name).
- `apps/desktop/src-tauri/capabilities/main.json:21` `name: "nexus42"` matches `sidecar("nexus42")`. The `shell:allow-execute` scope authorization matches the runtime lookup, so the permission gate is not the source of failure.
- `apps/desktop/src-tauri/tauri.conf.json:34` `bundle.externalBin: ["binaries/nexus42"]` is the **build-time** path; the build script (`build.rs:11-13`) joins `binaries/nexus42-<target-triple>` and fails fast if absent. This was correctly preserved by the fix — the runtime API only takes the base name; the build-time path is untouched.
- Confirmation evidence: task-5-fix-report.md shows the original `failed to spawn sidecar: No such file or directory (os error 2)` is gone and the daemon process is now reached at spawn time (stderr captured from the spawned child). The fix is correct for the documented Tauri v2 contract.

### Casing fix on the `pickDirectory` IPC

- `apps/web/src/lib/nexus/desktop-capabilities.ts:207` now invokes `pick_directory` with `{ defaultPath }`.
- `apps/desktop/src-tauri/src/lib.rs:448` `pick_directory(app: AppHandle, default_path: String)` — Rust param is snake_case.
- **No `#[tauri::command(rename_all = "snake_case")]` attribute is used anywhere in `apps/desktop/src-tauri/src/`** (verified by exhaustive `grep -B1`). Therefore Tauri v2's **default** automatic camelCase→snake_case conversion applies: JS camelCase `defaultPath` maps to Rust snake_case `default_path`.
- This is the conventional Tauri v2 contract and the fix is correct.
- Test: `pickDirectory invokes pick_directory with Tauri camelCase args` (`desktop-capabilities.test.ts:113-123`) asserts `expect(invoke).toHaveBeenCalledWith('pick_directory', { defaultPath })`. Test uses `/Users/example/...` (generic example, not a real machine-local path) — path hygiene preserved.
- Architectural observation: this also matches what every other command in `lib.rs` does (e.g. `set_workspace_path(path: String)` → JS `set_workspace_path` with `{ path }`; `set_setup_completed(value: bool)` → JS `{ value }`; `set_agent_profile(name, launch_command)` → JS `{ name, launch_command }`). The former prototype code was the lone outlier, introduced as a side-effect of the inherited pre-V1.66 code, and the V1.97 fix realigns with the rest.

## Architecture Review — Layout containment (T3)

Three production lines, all using **pre-existing Tailwind utilities** (no new tokens invented, no `DESIGN.md` / `DESIGN.dark.md` patch needed, no `index.css` change):

| File | Change | Architectural role |
|------|--------|---------------------|
| `setup-wizard-page.tsx:64` | add `overflow-hidden` on the integrated card root | prevent inner content from spilling past the card's `rounded-popover` corners |
| `setup-wizard-page.tsx:68` | add `min-w-0` on `<main>` | let the right-side content panel shrink in the flex row so the step indicator panel reserves its fixed width |
| `setup-step-welcome.tsx:95` | add `min-w-0` on the inner `div` parent of the path text | let the path row's flex-1 child actually shrink so the long path truncates rather than pushing the Browse button off-screen |
| `setup-step-welcome.tsx:106` | add `flex-shrink-0` on the Browse button | preserve intrinsic width of the affordance; flex-1 sibling absorbs the slack |

Tests verify the containment chain:

- `setup-wizard-page.test.tsx:71` asserts the card root has `overflow-hidden`.
- `setup-wizard-page.test.tsx:86` asserts `<main>` has `min-w-0`.
- `setup-step-welcome.test.tsx:95-103` asserts the path's parent has both `min-w-0` and `flex-1`, and that the Browse button has `flex-shrink-0`. **Test additionally asserts the Browse button keeps its intrinsic width so the path container absorbs all available horizontal space in the row** — this is the actual semantic of the fix (the visual behaviour the comment author intended).
- Test fixture: `'/very/long/path/'.repeat(10)` (`setup-step-welcome.test.tsx:82`) — synthetic, not machine-local. Path hygiene preserved.

Architecturally: the fix is at exactly the right layer — pure utility composition, no token drift, no CSS rules added. The DESIGN.md "Setup Wizard Surface" Level 3 Production spec is preserved verbatim.

## Cross-Task Coherence

| Task | Surface | Composition |
|------|---------|-------------|
| T1 prototype intake | baseline | already satisfies accepted baseline; index.css token-drift hunks explicitly rejected/reverted per progress.md |
| T2 folder picker IPC | `desktop-capabilities.ts` | one-line + test (no Rust rename required; the Rust command already used the snake_case default) |
| T3 layout containment | `setup-wizard-page.tsx` + `setup-step-welcome.tsx` | utilities only; no overlap with T1/T2/T4/T5 files |
| T4 sidecar FSM | `sidecar.rs` initial state + `start_with_budget` guard + 2 new tests | additive; no overlap with T1-T3 |
| T5 spawn-name fix | `sidecar.rs:249` + `capabilities/main.json:21` | two-line surgical; builds on T4 invariants |

No conflict between any pair. The 2-line T5 fix lives inside the same `sidecar.rs:219-249` block that T4 just verified; it does not disturb the FSM. The `capabilities/main.json` change does not change any `sidecar.rs` symbol. The T2 fix is in a separate file and the Rust command name is unchanged.

## Scope Discipline

- `git diff --stat` shows file count = 16. Mapping by domain:
  - `apps/desktop/src-tauri/` (production): `capabilities/main.json`, `src/sidecar.rs` (2 files)
  - `apps/web/src/` (production): `lib/nexus/desktop-capabilities.{ts,test.ts}`, `pages/setup-{wizard-page,step-welcome}.{tsx,test.tsx}` (6 files)
  - `.mstar/` (harness docs): iterations/README.md, compasses, plan, status.json, iteration workspace README, iteration guides, knowledge/specs/* (8 files)
- **Verified empty diffs in protected scope:**
  - `git log 070e26f7..ab618ee9 -- crates/nexus-daemon-runtime/` → empty (daemon-runtime untouched)
  - `git log 070e26f7..ab618ee9 -- apps/nexus42/` → empty (CLI untouched)
  - `git log 070e26f7..ab618ee9 -- schemas/` → empty (wire contracts untouched)
- Path hygiene: `rg -n '/Users/bibi' apps/desktop/src-tauri apps/web/src` → empty. No local-machine path leakage in app code or app tests.
- DESIGN.md token SSOT: `index.css` is not modified, no fabricated utility tokens. `apps/web/DESIGN.md` and `apps/web/DESIGN.dark.md` files are unchanged. The `tailwind-merge` / `truncate` / `min-w-0` / `flex-1` / `flex-shrink-0` / `overflow-hidden` utilities are all pre-existing Tailwind primitive utilities, not DESIGN.md tokens.
- `apps/desktop/src-tauri/src/lib.rs` — `apps/desktop/AGENTS.md` flags the desktop-side Rust crate as a standalone Tauri-managed crate, not a root workspace member. The diff touches only `sidecar.rs` inside this crate (the rest of `lib.rs` is read-only context); the `setup` hook (`lib.rs:529-538`) is unchanged and remains the trigger point for `manager.start()`. No coordinate edit needed.
- Generated code (`crates/nexus-contracts/src/generated/`) untouched.
- Tests: 21 sidecar Rust tests pass (per task-5-fix-report.md), 26 web tests pass (per task-2-report / task-4 coverage). `cargo +nightly-2026-06-26 fmt --all --check` clean per task reports.
- `wc -c .mstar/status.json` = 16306 bytes (under 20000-byte ceiling).

## Findings

### 🔴 Critical
*(none)*

### 🟡 Warning
*(none)*

### 🟢 Suggestion

**S-1 (low/nit) — Pre-existing clean-state no-creator gap, surfaced by the smoke gate.** `apps/desktop/src-tauri/src/lib.rs:529-538` calls `manager.start(&handle)` unconditionally from the `setup` closure on every app launch. On a clean `~/.nexus42` (no creator in `config.toml`), the spawned daemon exits with `No active creator in ~/.nexus42/config.toml` (verified in task-5-fix-report.md), the health endpoint never responds, and the sidecar manager lands in `Error`. This is a **pre-existing** autos-start design that V1.97's tighter smoke gate made visible. The fix is architecturally outside this plan's scope (it touches the daemon no-creator guard and the wizard bootstrap flow), and the plan correctly defers it. Track as V1.98+ scope with a brief architectural-decision note: should the desktop-side `start()` gate on `setup_completed` (so the wizard owns the creator-bootstrap step), or should the daemon tolerate a no-creator state and return a typed error the wizard can render? The plan's "deeper than V1.97 scope" framing is accurate.

**S-2 (low/nit) — Small redundancy in `start_with_budget` fallthrough.** `apps/desktop/src-tauri/src/sidecar.rs:225` unconditionally reassigns `inner.state = DaemonState::Starting;` even when the early-return was bypassed precisely because `state == Starting && child.is_none()`. The reassignment to the same value is intentional (resets the cosmetic transition clock for UI), but a comment such as `// fresh diagnostic window for retry/attach` would make the intent obvious to a future reader. Not blocking; just a doc polish. The architecture is sound.

**S-3 (low/nit) — Pre-existing clippy lint surfaced by the strict run.** `apps/desktop/src-tauri/src/connection_config.rs:198` fails `cargo clippy --all-targets -- -D warnings` with `clippy::io-other-error` (`std::io::Error::new(std::io::ErrorKind::Other, "keychain unavailable")` should be `std::io::Error::other(_)`). This is unrelated to the V1.97 diff (the changed files are clippy-clean) and was already a flag in the task-5-fix-report.md "self-review notes" + "Concerns" section. Track as a separate lint-rotation cleanup, not as a V1.97 residual.

## Source Trace

- S-1: `apps/desktop/src-tauri/src/lib.rs:529-538` — `setup` closure spawns `manager.start(&handle).await` unconditionally. Confirmed by task-5-fix-report.md §"Clean-state desktop smoke re-run" (stderr captured: `No active creator in ~/.nexus42/config.toml`).
  - Source Type: manual-reasoning + lint
  - Confidence: High
- S-2: `apps/desktop/src-tauri/src/sidecar.rs:219-232` — early-return block followed by unconditional `state = Starting` reassignment.
  - Source Type: manual-reasoning
  - Confidence: High
- S-3: `apps/desktop/src-tauri/src/connection_config.rs:198` — strict clippy failure on `std::io::Error::new(std::io::ErrorKind::Other, ...)`.
  - Source Type: linter (pre-existing in diff scope because strict mode tests it; not introduced by V1.97)
  - Confidence: High

## Residual Candidates (non-blocking leftovers)

| Residual ID | Severity | Title | Owner | Target plan |
|-------------|----------|-------|-------|-------------|
| R-V197QC1-S001 | medium | Clean-state no-creator gap: `lib.rs:534` `manager.start()` runs before wizard creator-bootstrap. Daemon exits `No active creator` on first launch. | `@architect` + `@fullstack-dev` | V1.98 (architectural decision: gate start on setup_completed OR daemon tolerates no-creator) |
| R-V197QC1-S002 | nit | `start_with_budget` reassigns `state=Starting` on the `Starting + !child` fallthrough path; doc comment would help future readers. | `@fullstack-dev` | opportunistic; or next sidecar.rs pass |
| R-V197QC1-S003 | low | Pre-existing `clippy::io-other-error` at `connection_config.rs:198` (unrelated to V1.97 diff; surfaces under strict `--all-targets`). | `@fullstack-dev` or `@ops-engineer` | lint-rotation cleanup |

## Cannot Verify (PM/QA follow-up)

1. **Clean-state interactive desktop smoke.** Cannot be observed in a headless terminal-only session. The Tauri window cannot be driven; the wizard UI cannot be probed. The only signal available is daemon stderr, which now reaches the spawned child (T5 spawn-name fix verified) but immediately exits on the no-creator guard (S-1). **Recommendation:** `qa-engineer` or PM to perform the smoke on an interactive macOS Aqua host, or document this as an unrecoverable hard-gate blocker per plan §"Task 5" final checkbox.
2. **Existing-install desktop smoke UI behavior.** The T5-fix-report shows the **daemon stays healthy** while the desktop app attaches, which is the strongest non-interactive proxy. UI behavior (wizard auto-skip via `setup_completed`, workspace path preservation, etc.) still requires an interactive host to confirm end-to-end.

Both blockers are PM-acknowledged in the Assignment and not introduced by this diff.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 3 |

**Verdict:** **Approve**

The five V1.97 sidecar architecture invariants are cleanly satisfied (one-line `state` change in `SidecarManager::new`; one-line guard rewrite in `start_with_budget`; new regression tests for `Starting+!child` and `Error` retry). The `sidecar()` / capability-scope `name` triple coordinates correctly per Tauri v2 docs, with `bundle.externalBin` (build-time path) intentionally preserved. The `defaultPath` casing fix matches the Tauri v2 default camelCase→snake_case conversion policy; the prior `default_path` was the lone outlier in this crate. The layout containment is pure utility composition — no DESIGN.md drift, no `index.css` churn, no fabricated tokens. Cross-task composition has no conflicts, no scope creep (daemon-runtime / CLI / schemas / generated contracts all empty in this diff). Path hygiene is preserved (no `/Users/bibi` literals under `apps/`). The 3 suggestions are all out-of-scope leftovers (a pre-existing no-creator design gap surfaced by the V1.97 smoke gate; a doc-comment polish on `start_with_budget`; a pre-existing clippy lint) — none are blockers on this fix.

All three QC reviewers and the QA gate are not part of QC1's verdict scope. The two known smoke carry-over items (no-creator clean-state gap + headless UI smoke) are correctly tracked separately per PM-acknowledged assignment, do not penalize this diff's quality, and have been routed to the appropriate owners above.
