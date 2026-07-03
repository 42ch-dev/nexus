---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-07-03-v1.86-local-api-trust-hardening"
verdict: "Approve"
generated_at: "2026-07-03"
---

# QC Report — V1.86 Local API Trust-Boundary Hardening (Reviewer #3 — Performance & Reliability)

## Scope (verbatim)

- plan_id: `2026-07-03-v1.86-local-api-trust-hardening`
- Review range / Diff basis: `merge-base(main, iteration/v1.86)..iteration/v1.86` — `e795fff9..b2cdcfd6` (equivalent to `git diff main...iteration/v1.86`)
- Working branch (verified): `iteration/v1.86` (HEAD `b2cdcfd6`)
- Review cwd: `/Users/bibi/workspace/organizations/42ch/nexus`
- Reviewer focus: performance + reliability (T5 `spawn_blocking`, admission async correctness, Origin middleware cost, `DaemonApiConfig` clone footprint, resource lifecycle, CI/build regression)

## Context Gate

- `git rev-parse --show-toplevel` -> repo root (OK)
- `git branch --show-current` -> `iteration/v1.86` (OK)
- `git rev-parse HEAD` -> `b2cdcfd6f26c010dd6faee9997bf335c6abe0d7f` (OK)
- Diff basis reproducible from checkout; scope aligned with `mstar-review-qc` gate.

## Static / Test Evidence

| Check | Command | Result |
|---|---|---|
| Unit tests | `cargo test -p nexus-daemon-runtime --lib` | 387 passed / 0 failed / 0 ignored (30.00s) |
| Clippy | `cargo clippy -p nexus-daemon-runtime -- -D warnings` | clean (no warnings) |
| Tracked binaries in diff | `git diff main...iteration/v1.86 --stat` scan | none — only `apps/desktop/src-tauri/Cargo.lock` metadata refresh (adds `tracing` dep line) |
| Working tree | `git status` | clean; branch ahead of origin by 9 commits (expected) |

Release build not run (optional per assignment; time-boxed out). Changes are purely functional (no new build.rs, no proc-macros, no cfg-gated code); release regression risk very low.

## Findings

### F-1 — Double path resolution per fs/* call (admission + execute) — Suggestion

Evidence: `crates/nexus-daemon-runtime/src/api/handlers/host_tool_handlers.rs:97-98, 543-590, 596-655, 794-826`.

Every `fs/read_text_file` / `fs/write_text_file` call resolves the path twice via `resolve_guarded_path_async` (each schedules a `tokio::task::spawn_blocking` + executes `std::fs::canonicalize`):

1. `admission_pipeline` -> `validate_file_path` -> `resolve_guarded_path_async(..., must_exist=false)` (L98/L814).
2. `execute_read_file` / `execute_write_file` -> `resolve_guarded_path_async(..., must_exist=true|false)` (L563 / L624).

Perf impact: For local single-user (design target), 2x spawn_blocking + 2x canonicalize per fs/* call is negligible. Tokio's default blocking pool is 512 threads; exhaustion is not a realistic concern under expected daemon load. Not ship-blocking.

Reliability angle (informational only): Because `must_exist=false` in admission and `must_exist=true` in `execute_read_file`, the two `canonicalize` calls can observe different filesystem states (symlink swap between the two resolutions). This is the same TOCTOU class already documented in `path_guard.rs:24-30` as "racy-correct" per the V1.86 trust-boundary spec and residual `R-V166-QC2-TOCTOU`. No new hazard introduced; the second resolution marginally amplifies the race window.

Recommendation (optional, not required for approval): A future iteration could either (a) hoist the resolved `PathBuf` from admission through the executor via a small context object, or (b) skip `validate_file_path` in admission for fs/read/write and rely on the per-handler resolution (admission still guards `nexus.*` tools). Do not land now — design tweak, not a correctness fix.

### F-2 — DaemonApiConfig cloned into every request via axum State — Suggestion

Evidence: `crates/nexus-daemon-runtime/src/api/auth_middleware.rs:32-43` (config struct); `api/mod.rs:497, 533` and `auth_middleware.rs:462, 481` (installed via `axum::middleware::from_fn_with_state`).

`DaemonApiConfig` now holds three `Vec<String>` fields (`allowed_origins`, `allowed_origin_sources` pairs, plus `Option<String> api_key`). axum's `State` extractor clones the layer state per request for both `require_allowed_origin` and `require_api_key`. Each request performs ~2 x (Vec + String) heap clones just to pass config into the middleware.

Perf impact: For local single-user, tiny (a handful of allocations per request; microseconds). Not ship-blocking.

Recommendation (optional): Future micro-optimization — wrap `DaemonApiConfig` in `Arc<...>` and clone the Arc. Zero-cost per-request. Reliability-neutral.

Explicit confirmation for the PM checklist item: `DaemonApiConfig::clone()` at `api/mod.rs:497` and `auth_middleware.rs:462` is a one-time clone at router construction (one copy for `require_api_key` layer, one for `require_allowed_origin` layer). It is not called per-request in any handler body — axum's `State` extractor is the only per-request cloner, and that is unavoidable with `from_fn_with_state`.

### F-3 — Origin header allowlist scan — No finding (confirmed acceptable)

Evidence: `crates/nexus-daemon-runtime/src/api/auth_middleware.rs:220-256`.

`require_allowed_origin` on every non-OPTIONS request:
- Reads the `Origin` header (one map lookup).
- If present, calls `origin.to_str().unwrap_or_default()` (borrows a `&str` from the header; no allocation).
- Runs `.iter().any(|allowed| allowed == origin_str)` on `allowed_origins` (~4 entries default, small even with env override). `String == &str` via `PartialEq` — no clone/allocation on the hot path.

Perf impact: O(n) scan over ~4-8 strings per request. Effectively free.

Reliability: Invalid UTF-8 `Origin` -> `unwrap_or_default()` yields empty string, which will never match any allowlisted origin -> rejected 403. Correct fail-closed posture on malformed input.

### F-4 — spawn_blocking closures: capture-by-move correctness — No finding (confirmed correct)

Evidence: `host_tool_handlers.rs:573-585, 634-650, 773-786`.

All three `spawn_blocking` sites capture inputs by move:
- `execute_read_file` (L573-576): `resolved` is `.clone()`d into a `let` binding, then `move ||` captures the owned `PathBuf`. The outer `resolved` is a separate owner used for the error `display()` at L584 (no `&` reference crosses the `.await`).
- `execute_write_file` (L634-645): `resolved` and `content` are both moved into the closure via `move ||`. Not referenced again outside.
- `resolve_guarded_path_async` (L778-780): `workspace_root: PathBuf` and `rel_path: String` are owned parameters, moved into the closure.

No references to async-only state (`&WorkspaceState`, `&ToolExecuteRequest`) cross the blocking boundary. Send bounds satisfied by `PathBuf` / `String`. Correct pattern.

### F-5 — JoinError -> Internal panic mapping is the right reliability posture — No finding (positive observation)

Evidence: `host_tool_handlers.rs:577-581, 646-650, 781-785`.

All three `spawn_blocking` sites map `JoinError` (task panic or cancellation) to `NexusApiError::Internal { code: "FILE_READ_PANIC" | "FILE_WRITE_PANIC" | "PATH_GUARD_PANIC", ... }`. This:
- Does not swallow the panic — surfaced to the client as HTTP 500 with a distinct code.
- Does not poison the runtime — tokio catches `spawn_blocking` panics; the JoinError just propagates.
- Is greppable — three distinct codes ease triage.

Good practice.

### F-6 — admission_pipeline async: no check-then-await-then-act correctness gap — No finding

Evidence: `host_tool_handlers.rs:37-101`, `host_tool_executor.rs:232`.

Focusing on the async-yield reliability angle (qc2 owns the general TOCTOU lens): `admission_pipeline` is now `async`, but its only `.await` point (`validate_file_path(req, state).await` at L98) is the terminal step before returning. Gates 1 (allowlist), 2 (creator), 4 (permissions) are all synchronous and precede the `.await`. Gate 3 (path bounds) is the awaited call and directly returns its verdict.

Downstream `registry_dispatch` (`host_tool_executor.rs:232-256`) then re-resolves the path inside each fs/* handler — as noted in F-1, this is a re-check, not a check-then-await-then-act gap using stale admission data. Correct.

### F-7 — Cargo.lock refresh + tracked-binary sweep — No finding

Evidence: `apps/desktop/src-tauri/Cargo.lock` diff shows a single `+ "tracing",` line in the nexus-daemon-runtime dependency block (tracing is already used elsewhere; no version bump). No new binary tracked, no `target/` entries staged, no unexpected transitive-dep upgrade.

### F-8 — Resource lifecycle: spawn_blocking tasks + file handles — No finding

Evidence: All three `spawn_blocking` sites inspected.

- Every `spawn_blocking(...)` is directly `.await`ed on the same expression — no orphaned / detached tasks.
- File handles are opened + closed synchronously inside `std::fs::read_to_string` / `std::fs::write` (return-by-value / RAII). No fd leak surface.
- `PathBuf` / `String` inputs are dropped when the closure returns.

## Coverage of PM checklist (assignment)

| Assignment item | Result |
|---|---|
| T5 spawn_blocking captures-by-move (no `&` async state across boundary) | Pass (F-4) |
| Panic-safety: JoinError -> Internal { code: FILE_*_PANIC }, no swallow, no poison | Pass (F-5) |
| Unbounded / repeated spawn_blocking on hot path | Bounded to 2 per fs/* call (admission + execute); acceptable (F-1) |
| validate_file_path double resolution — perf material? | Negligible for local single-user; flagged only as optional simplification (F-1) |
| Origin middleware cost / per-request allocation | Effectively free; no per-request allocation on hot path (F-3) |
| DaemonApiConfig cheap to clone / not cloned per-request in handlers | Cloned only at router construction; per-request clone comes from axum `State` and is unavoidable, small (F-2) |
| async admission introduces check-await-act gap? | No (F-6) |
| CI/build impact — no unexpected tracked binary / build regression | Confirmed (F-7); test + clippy green |
| Resource lifecycle: spawn_blocking / file handles | No leak surface (F-8) |

## Verdict

Approve.

Rationale: All P0/P1 tasks (T1-T8) implement the intended perf/reliability posture correctly. `spawn_blocking` boundary is clean (move captures, JoinError -> distinct Internal codes). Async admission has no check-await-act gap. Origin middleware and config layout add negligible per-request cost for the local single-user design target. Test suite green (387/0), clippy clean, no tracked-binary regression. Two Suggestions (F-1 double resolution, F-2 Arc<Config>) are non-blocking optional future work; do not require changes for this iteration.

## Residuals to record

Recommended for PM to log in `.mstar/status.json` root `residual_findings` (severity: `low`, lifecycle: `open`, source: `qc3` per `mstar-plan-artifacts`):

1. `R-V186-QC3-PERF-DOUBLE-RESOLVE` (low) — fs/* admission + execute both call `resolve_guarded_path_async`, doubling the canonicalize/spawn_blocking cost. Optional simplification for a future perf iteration; not a correctness issue. See F-1.
2. `R-V186-QC3-PERF-ARC-CONFIG` (low) — `DaemonApiConfig` is cloned per-request by axum's `State` extractor; wrapping in `Arc` would make the clone a pointer bump. Micro-optimization; not a correctness issue. See F-2.

Both are Suggestions, not Warnings — PM discretion whether to formalize as residuals or defer as a compass note. No blocking action on V1.86.

## Reviewer identity + sign-off

- Reviewer: `qc-specialist-3` (index 3, focus: performance + reliability)
- Reviewed on: `iteration/v1.86` @ `b2cdcfd6`
- No subagent dispatched. Report written directly by this reviewer per QC NEVER rules.
