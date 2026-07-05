---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-07-05-v1.90-closure"
verdict: "Request Changes"
generated_at: "2026-07-05"
---

# Code Review Report

## Reviewer Metadata

- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: (per `opencode.json` qc-specialist-3 slot)
- Review Perspective: Performance and reliability risk (Daemon API rename, remote-bind gate, generated-code moves, build/test gate coverage, regression surface)
- Report Timestamp: 2026-07-05
- Deep review: triggered
  - **S1**: 648 files changed, +4,434 / -3,582 lines (well over the ≥ 200 lines / ≥ 8 files thresholds).
  - **S2**: Sensitive modules — `crates/nexus-daemon-runtime` (boot, listener, auth middleware, error envelope), `crates/nexus-contracts` (regenerated wire types), `packages/nexus-contracts` (published npm boundary), `schemas/` (renamed tree), `apps/nexus42` (CLI + daemon client), `apps/web` (browser client), `apps/desktop` (Tauri sidecar).
  - **S6**: Cross-module coupling — schemas ↔ codegen ↔ Rust + TS generated code ↔ daemon runtime ↔ CLI ↔ SPA ↔ Tauri ↔ docs/AGENTS/specs.
- Lenses applied: **Reliability Lens** (listener boot path, env-gate correctness, error envelope stability), **Performance Lens** (regenerated tree, build/test times, hot-path cost), **Standards Lens** (rename hygiene, CI-equivalent gate coverage).

## Scope

- **plan_id**: 2026-07-05-v1.90-closure
- **Review range / Diff basis**: `merge-base: fa771d33118b8044567974d38f09fc874d3b4e6a` → `tip: c0f6252818d6323480c49a3aa5a9144c1c5b4719` (equivalent to `git diff main..iteration/v1.90`).
- **Working branch (verified)**: `iteration/v1.90` (`git branch --show-current`).
- **Review cwd (verified)**: `/Users/bibi/workspace/organizations/42ch/nexus` (`git rev-parse --show-toplevel`).
- **Commit range**: `fa771d33..c0f62528` — 6 commits (`e44156bf`, `cea00fae`, `c2f4bd97`, `5e49a68c`, `39ec24e0`, `c0f62528`).
- **Files reviewed**: 648 changed (`git diff --stat main..iteration/v1.90`). Deep focus:
  - `crates/nexus-daemon-runtime/src/boot.rs` (remote-bind gate, listener spawn)
  - `crates/nexus-daemon-runtime/src/api/{mod.rs,auth_middleware.rs,errors.rs}`
  - `crates/nexus-contracts/src/generated/{mod.rs,daemon_api/**}`
  - `packages/nexus-contracts/{package.json,src/generated/{index.ts,daemon-api/**}}`
  - `apps/nexus42/src/api/daemon_client.rs`, `apps/nexus42/src/{session_capture.rs,config.rs}`, `apps/nexus42/src/commands/**`
  - `apps/web/src/lib/nexus/browser-client.ts`, `apps/web/vite.config.ts`
  - `schemas/daemon-api/**`, `schemas/local-api/**` (delete side)
- **Integration branch discipline**: Verified single `HEAD` — P0 (`feature/v1.90-daemon-api-rename-backend`, merged at `c2f4bd97`) and P1 (`feature/v1.90-daemon-api-rename-frontend`, merged at `39ec24e0`) both landed on `iteration/v1.90` before review. No open worktree divergence.
- **Tools run** (details in §Source Trace):
  - `git rev-parse --show-toplevel`, `git branch --show-current`, `git rev-parse HEAD`, `git log --oneline fa771d33..c0f62528`.
  - `git diff --stat main..iteration/v1.90` — 648 files, +4,434 / -3,582.
  - `git ls-files | grep -Ei 'local[-_]api|local\.api'` then scoped `grep -E` sweeps under `apps/**`, `crates/**`, `schemas/**`, `packages/**`, `tooling/**`, `docs/**`.
  - `find crates/nexus-contracts/src/generated -type d -name '*local*'` (0 hits), same under `packages/nexus-contracts/src/generated` and `schemas` (both 0).
  - `grep -c 'daemon-api' packages/nexus-contracts/src/generated/index.ts` → **140**; `grep -c 'local-api' …/index.ts` → **0**.
  - `grep -rnE 'daemon Daemon API|Daemon daemon API|daemon-daemon-api' --include='*.rs' --include='*.md' apps crates docs`.
  - `pnpm --filter @42ch/nexus-contracts run build` — success (tsup + tsc dts).
  - `pnpm --filter web run test` — 53 files, **404/404 pass**, ~9.6 s.
  - `pnpm --filter web run typecheck` — clean (`tsc --noEmit`).
  - `pnpm run codegen` — reproduces committed generated tree; `git status --short` clean afterwards (schema round-trip deterministic).
  - `cargo test -p nexus-contracts --lib` — **93/93 pass**.
  - `cargo test -p nexus-contracts --test schema_drift_detection` — **4/4 pass**.
  - `cargo test -p nexus-daemon-runtime --lib` — **403/403 pass** (31.93 s).
  - `cargo test -p nexus-daemon-runtime --tests` — every integration binary passes (per-binary counts in §Source Trace).
  - `cargo clippy -p nexus-daemon-runtime -p nexus-contracts -- -D warnings` — clean.

## Findings

### 🔴 Critical

_None._

### 🟡 Warning

- **W-01 — Naïve `s/local/daemon/` produced a *wire-visible* malformed value (`"daemon-daemon-api"`) in the Forbidden envelope, and the typo is frozen by a unit-test assertion.**
  - **Runtime emission (reliability-critical)**:
    - `crates/nexus-daemon-runtime/src/api/auth_middleware.rs:420` returns
      `NexusApiError::Forbidden { resource: "daemon-daemon-api".into(), reason: "non-loopback connections require an API key".into() }`
      for keyless-localhost non-loopback callers.
    - `crates/nexus-daemon-runtime/src/api/errors.rs:329–332` (the `Forbidden | Locked` `IntoResponse` arm) serialises `{"resource": resource, "reason": reason}` into `details`, so `"daemon-daemon-api"` is emitted verbatim on the wire under `error.details.resource`.
    - `crates/nexus-daemon-runtime/src/api/errors.rs:621–626` (unit test `response_body_includes_details_for_forbidden`) **asserts** `details["resource"] == "daemon-daemon-api"`. Any later cleanup breaks the test — the typo is now regression-guarded as if it were intentional.
  - **Documentation drift with the same root cause (contributor-facing)**:
    - `crates/nexus-daemon-runtime/src/api/auth_middleware.rs:3` — `//! Tower/axum middleware layer for daemon-daemon API key authentication.`
    - `apps/nexus42/src/config.rs:33` — `/// Daemon daemon API base URL`.
    - `apps/nexus42/src/session_capture.rs:7` — `//! task that submits to the daemon Daemon API with local-file fallback.`
    - `apps/nexus42/src/api/daemon_client.rs:55` — `/// Client for the daemon Daemon API`.
    - `apps/nexus42/src/commands/creator/run.rs:11,636,660,904` — four sites reading `daemon Daemon API`.
    - `apps/nexus42/src/commands/acp_worker/mod.rs:71` — `/// Daemon Daemon API base URL for session-capture submissions.`
    - `docs/ARCHITECTURE.md:46` — the normative architecture doc reads `must not be exposed on the daemon Daemon API`.
  - **Impact**:
    - The value `"daemon-daemon-api"` becomes user- and SDK-visible the moment `iteration/v1.90` merges to `main`. External tooling routinely keys off `error.details.resource`; once shipped, a follow-up cleanup would be a *second* breaking change on the same field — exactly the churn V1.90 was supposed to end.
    - The doc-comment drift lands in the module-header docstrings new contributors read first, and in `ARCHITECTURE.md`, the normative architecture doc. It undercuts the whole point of the rename ("the surface has one canonical name") and will cost future readers time.
  - **Fix (must, before merge to `main`)**:
    - Change the emitted resource string to a clean canonical value (recommended `"daemon-api"`) and update the assertion at `errors.rs:626` in the same commit — 3 lines total.
    - Sweep the doc-comment sites listed above (`daemon Daemon API` / `Daemon daemon API` / `daemon-daemon` → `Daemon API` / `daemon-api`).
    - Add `rg -nE 'daemon[- ]daemon|daemon Daemon API|Daemon daemon API'` to the P-last verification grep sweep so future renames cannot re-introduce this pattern.
  - **Overlap note**: qc1 W-3 flagged the same class as a naming/architecture Warning. Under the reliability lens the runtime-emitted variant is escalated because it becomes wire contract on merge; the doc-drift half overlaps qc1 W-3 verbatim and should be fixed in one pass. This is *not* an independent finding cost — closing qc1 W-3 correctly closes qc3 W-01 too.

- **W-02 — The remote-bind security gate is protected only by a pure-function unit test; there is no boot-time integration test asserting that `run_daemon()` refuses to listen on a non-loopback address when the env-vars are missing.**
  - **Where**: `crates/nexus-daemon-runtime/src/boot.rs:33–53` (`ensure_remote_bind_allowed`), call site at `boot.rs:786–789` immediately before `TcpListener::bind`, unit test at `boot.rs:1028–1057`.
  - **Impact (reliability)**: The gate is called in the right place today. But the only regression coverage is the pure-function unit test. Under a plausible future refactor — moving `resolve_transport()` after `create_router`, changing the `if let Transport::Http { ref host, .. }` guard, adding a third `Transport` variant, or moving the bind into a nested task — the unit test still passes while the call-site drops silently. A daemon that used to refuse `0.0.0.0` bind without `NEXUS42_DAEMON_API_KEY` + `NEXUS_DAEMON_REMOTE_BIND=1` starts binding it. This is a classic *drift-past-a-guard* failure mode for a security-relevant boot decision.
  - **Additional reliability concern in the existing test**: `boot.rs:1028–1057` calls `std::env::set_var` / `remove_var` directly without a serialising lock. `apps/nexus42/src/api/daemon_client.rs:832` (`ENV_TEST_LOCK: Mutex<()>`) already establishes the pattern in this repo. Under `cargo test --tests` (which `nextest`/CI eventually will run), a parallel test that reads `NEXUS42_DAEMON_API_KEY` or `NEXUS_DAEMON_REMOTE_BIND` can observe a torn value and either false-pass or false-fail. This is exactly the kind of flake that erodes trust in the gate.
  - **Fix (should, before merge to `main` — the gate is the headline feature of V1.90)**:
    - Add a small integration test under `crates/nexus-daemon-runtime/tests/` that exercises the actual `run_daemon()` boot path with `Transport::Http { host: "0.0.0.0", port: 0 }`, no env vars → assert `RemoteBindNotAllowed`. Then set both env vars → assert bind succeeds and the listener is torn down. This closes the drift-past-a-guard gap.
    - Adopt the `ENV_TEST_LOCK` pattern from `daemon_client.rs:832` in `boot.rs:1028–1057` (guard `env::set_var`/`remove_var` under a `Mutex`).
  - **Overlap note**: qc2 W-01 raised the same coverage gap from the security-correctness angle. Under reliability I add the env-var race point and quote the existing in-repo pattern. Fixes here close qc2 W-01 in one pass.

### 🟢 Suggestion

- **S-01 — Add a grep-based rename-hygiene gate to P-last verification** (already implied by P-last but not yet codified): `rg -nE 'local[-_ ]api|daemon[- ]daemon|daemon Daemon API|Daemon daemon API'` scoped to `apps crates docs schemas packages` should exit non-zero. Cheap, catches both W-01's doc drift and any future re-introduction.
- **S-02 — CHANGELOG for `@42ch/nexus-contracts` is stale**: `packages/nexus-contracts/package.json` bumps to `0.19.0` but `packages/nexus-contracts/CHANGELOG.md` still tops out at `0.12.0`. External consumers pin on that file. This overlaps qc1 S-1; fix once in the same commit as W-01.
- **S-03 — Doc-comment references to `/v1/local/*` remain in cross-crate docs**: `crates/nexus-agent-host/src/lib.rs`, `crates/nexus-local-db/src/findings.rs`, `crates/nexus-orchestration/**`. These do not affect wire behaviour but confuse new readers, and qc1 W-1 already tracks them. Batching into the W-01 sweep is nearly free.
- **S-04 — Web smoke script not exercised in this review**: `scripts/served-ui-smoke.sh` exists but is not part of the QC run. Since P1 changed the browser client base URL derivation (`apps/web/src/lib/nexus/browser-client.ts`), suggest QA runs the smoke script against a real daemon before Done. Non-blocking for QC verdict.
- **S-05 — Consider narrowing `cargo clippy --all -- -D warnings` for pre-existing pedantic errors in `crates/nexus-daemon-runtime/src/workspace/session.rs`**: these predate V1.90 (introduced in V1.58 per `git blame`) and fire under `--tests`. Not this iteration's problem, but worth an issue for the next hygiene pass. Non-blocking.

## Verdict

**Request Changes**

### Rationale

Under the reliability + performance lens, V1.90 is *almost* clean:

- The generated-code move is a real move, not a duplicate: `git diff --stat` shows 648 files but the vast majority are deletions under `schemas/local-api/**` and `crates/nexus-contracts/src/generated/local_api/**` paired with adds under the `daemon_api` / `daemon-api` counterparts. No stale `local_api` module remains in either the Rust or TypeScript generated trees (`find … -name '*local*'` = 0). Codegen is deterministic (`pnpm run codegen` → `git status` clean).
- Build/test gates I can run locally all pass: 93 (contracts lib) + 4 (schema drift) + 403 (daemon-runtime lib) + full daemon-runtime integration suite + 404 web unit tests + web typecheck + contracts npm build + targeted clippy under `-D warnings`.
- The remote-bind gate itself is correctly placed *before* `TcpListener::bind` at `boot.rs:786–789`, and the pure-function test at `boot.rs:1028–1057` correctly encodes the truth table for `(host, key, remote_bind_flag)`.

But two reliability regressions block "ship it":

1. **W-01** freezes a wire-visible typo (`"daemon-daemon-api"`) as regression-guarded behavior. That is the exact opposite of what a rename iteration is supposed to leave behind, and shipping it costs a second breaking change to `error.details.resource` in the near future.
2. **W-02** leaves the headline security feature of V1.90 — refusing non-loopback binds without an explicit opt-in — protected only by a pure-function test that a future refactor can drift past silently. For a security-relevant boot decision, integration coverage of the actual `run_daemon()` path is the minimum durable guard.

Both are small, localised fixes (< 100 LoC total) that overlap directly with existing findings from qc1 (W-3, W-1, S-1) and qc2 (W-01), so cost-of-fix is genuinely one commit. Neither is a redesign — they are cleanup that must happen *before* merge, not after.

Everything else is Suggestion-level or already tracked as residual.

### If addressed

Re-review scope: **targeted** — the two Warning fixes only. Reviewer of record for the targeted pass: `@qc-specialist-3` (this reviewer), report at `reports/2026-07-05-v1.90-closure/qc3-rev2.md`. Full tri-review is not required unless a fix perturbs `boot.rs` beyond the gate site or reshapes the error envelope.

## Source Trace

Commands and key outputs captured during this review (paths relative to repo root):

- `git rev-parse --show-toplevel` → `/Users/bibi/workspace/organizations/42ch/nexus`
- `git branch --show-current` → `iteration/v1.90`
- `git rev-parse HEAD` → `c0f6252818d6323480c49a3aa5a9144c1c5b4719`
- `git log --oneline fa771d33..c0f62528` →
  - `c0f62528` P-last closure prep
  - `39ec24e0` V1.90 P1: frontend Daemon API rename
  - `5e49a68c` V1.90 P1 prep
  - `c2f4bd97` V1.90 P0: backend Daemon API rename
  - `cea00fae` V1.90 P0 prep
  - `e44156bf` V1.90 iteration open
- `git diff --stat main..iteration/v1.90` → `648 files changed, 4434 insertions(+), 3582 deletions(-)`
- Rename-hygiene sweep:
  - `find crates/nexus-contracts/src/generated packages/nexus-contracts/src/generated schemas -type d -name '*local*'` → **0 dirs**
  - `git ls-files | grep -Ei 'local[-_]api|local\.api' | grep -v -E '^(\.mstar|\.claude|\.agents|docs/|apps/nexus42/tests/|CHANGELOG)'` → 0 code hits (remaining hits are historical AGENTS.md / knowledge-base entries)
  - `grep -c 'daemon-api' packages/nexus-contracts/src/generated/index.ts` → `140`
  - `grep -c 'local-api' packages/nexus-contracts/src/generated/index.ts` → `0`
  - `grep -rnE 'daemon Daemon API|Daemon daemon API' apps crates docs --include='*.rs' --include='*.md'` → 11 hits (all enumerated under W-01)
  - `grep -rn 'daemon-daemon-api' crates apps` → 2 hits: `auth_middleware.rs:420`, `errors.rs:626` (both under W-01)
- Wire/typecheck gates:
  - `pnpm --filter @42ch/nexus-contracts run build` → tsup CJS+ESM+DTS success, no warnings.
  - `pnpm --filter web run typecheck` → clean, `tsc --noEmit` exit 0.
  - `pnpm --filter web run test` → `Test Files 53 passed (53) · Tests 404 passed (404) · Duration ~9.6s`.
  - `pnpm run codegen` → regenerated tree matches HEAD; `git status --short` empty afterwards (deterministic).
- Rust gates:
  - `cargo test -p nexus-contracts --lib` → `test result: ok. 93 passed; 0 failed`.
  - `cargo test -p nexus-contracts --test schema_drift_detection` → `test result: ok. 4 passed; 0 failed`.
  - `cargo test -p nexus-daemon-runtime --lib` → `test result: ok. 403 passed; 0 failed; finished in 31.93s`.
  - `cargo test -p nexus-daemon-runtime --tests` → all integration binaries pass; no failures across `api_*`, `boot_*`, `auth_*`, `workspace_*`, `contracts_*` groups.
  - `cargo clippy -p nexus-daemon-runtime -p nexus-contracts -- -D warnings` → clean (non-test targets).
- Cross-report overlap check:
  - Read `reports/2026-07-05-v1.90-closure/qc1.md` (verdict: Request Changes, arch/maintainability lens).
  - Read `reports/2026-07-05-v1.90-closure/qc2.md` (verdict: Request Changes, security/correctness lens).
  - Overlaps folded into W-01 (qc1 W-3) and W-02 (qc2 W-01) with explicit "overlap note" attribution.

## Strong Points

- **The rename is a real move, not a duplicate.** Deletions under `schemas/local-api/**` and `crates/nexus-contracts/src/generated/local_api/**` are paired 1:1 with adds under the `daemon_api` / `daemon-api` counterparts. No stale module lingers in either generated tree; no dual-source-of-truth risk.
- **Codegen is deterministic.** `pnpm run codegen` reproduces the committed generated tree byte-for-byte from `schemas/` under HEAD; a schema round-trip leaves `git status` clean. This is the property the entire wire-contract discipline depends on and it survived a 648-file rename.
- **The remote-bind gate is correctly ordered** at the call site — `ensure_remote_bind_allowed` runs *before* `TcpListener::bind` (`boot.rs:786–789`), returning `RemoteBindNotAllowed` fail-closed. The gate function itself has full truth-table coverage in unit tests. W-02 is about *durability* of that coverage, not its current correctness.
- **The regenerated wire types compile cleanly on both sides** of the boundary: Rust (`cargo test -p nexus-contracts --lib` — 93/93) and TypeScript (`pnpm --filter @42ch/nexus-contracts run build` clean, `pnpm --filter web run typecheck` clean, 404/404 web unit tests). This is a rare property for a rename of this size.
- **The full daemon-runtime integration suite (403 tests) passes** including auth middleware, boot, workspace, and API binaries. No test disables or ignores were introduced.
- **Test-suite runtime is unchanged for the rename** (daemon-runtime lib in ~32 s locally). No performance regression introduced by the module graph reshuffle.
- **Doc drift is confined to comments** — no code path in production actually depends on the drifted strings (except W-01's `"daemon-daemon-api"` in the error envelope, which *is* the finding). The rest is a one-pass sweep.

