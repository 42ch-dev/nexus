# Completion Report v2 — P-last T1 (WL-A hygiene)

- plan_id: `2026-06-17-v1.49-hygiene-and-closeout`
- owner: `@fullstack-dev`
- Task: T1 — WL-A fix wave (V1.46 lows, compass §1.3)
- Working branch used: `fix/v1.49-wl-a-hygiene`
- Worktree path: `.worktrees/v1.49-wl-a-hygiene`
- Base: `iteration/v1.49` @ `dfc9d1782efac3e1ff1b3ca94fcf1d402a6b0f01`
- Scope: all 10 WL-A residuals fixed (no drops)

## Commits (one per residual + trailing fmt)

| # | Residual | Commit | Title |
|---|----------|--------|-------|
| 1 | R-V146P2-QC1-S1 | `d77a3627` | move process::exit out of library help-intercept |
| 2 | R-V146P2-QC3-S1 | `3e1c12af` | flush stdout before exit in help-intercept path |
| 3 | R-V146P2-QC3-S2 | `b88feaf6` | extract registry-injected help core (single build) |
| 4 | R-V146P2-QC1-S2 | `ed6a64ea` | dedup workspace-dir resolution in completion-lock hint |
| 5 | R-V146P0-QC1-S2 | `435973b5` | share findings-summary formatter between prod and test |
| 6 | R-V146P0-QC2-S1 | `7b92f317` | assert full finding element-shape fidelity in enrich |
| 7 | R-V146P0-QC3-S3 | `940efdc0` | route human stale banner through short-timeout fetch |
| 8 | R-V146P4-QC1-S1 | `2ccbaeed` | cover remaining 3 pool mutation trace paths |
| 9 | R-V146P4-QC3-S1 | `024bc961` | cover all 5 inspiration mutation trace paths |
| 10 | R-V146P3-QC1-S1 | `fec1d81a` | dedup research preset_version magic number |
| — | (fmt) | `fb52ea11` | nightly fmt — normalize formatting across WL-A touched files |

Tip: `git log --no-decorate --format='%h %s' iteration/v1.49..HEAD`

## Cargo verification

### `cargo +nightly fmt --all`
```
(produces no stdout on success)
fmt-exit:0
```

### `cargo +nightly fmt --all -- --check`
```
(produces no stdout when clean)
fmt-check-exit:0
```

### `cargo clippy --all -- -D warnings`
```
    Checking nexus-local-db v0.1.0 (.../crates/nexus-local-db)
    Checking nexus-daemon-runtime v0.1.0 (.../crates/nexus-daemon-runtime)
    Checking nexus42 v0.1.0 (.../crates/nexus42)
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.19s
clippy-exit:0
```

### Regression-test sweep on touched crates (final HEAD)
- `cargo test -p nexus42 --lib commands::creator` → **171 passed; 0 failed**
- `cargo test -p nexus-local-db --lib pool_entries` → **8 passed; 0 failed**
- `cargo test -p nexus-local-db --lib inspiration_items` → **10 passed; 0 failed**
- `cargo test -p nexus-orchestration --test research_supervisor_e2e` → **5 passed; 0 failed**

## Residuals fixed

- R-V146P0-QC1-S2 (author-desk-status-ux) — capture_findings_output no longer duplicates print_findings_summary formatting
- R-V146P0-QC2-S1 (author-desk-status-ux) — enrich now has full element-shape fidelity regression test
- R-V146P0-QC3-S3 (author-desk-status-ux) — human stale banner routed through short-timeout fetch (parity with JSON path)
- R-V146P2-QC1-S1 (novel-runtime-ux-edges) — `std::process::exit(0)` relocated from library to binary `main()`
- R-V146P2-QC1-S2 (novel-runtime-ux-edges) — `print_completion_lock_hint` deduped via `operational_workspace_dir_from_config`
- R-V146P2-QC3-S1 (novel-runtime-ux-edges) — stdout flushed before `exit(0)` in help-intercept path
- R-V146P2-QC3-S2 (novel-runtime-ux-edges) — `CapabilityRegistry::with_builtins()` extracted as explicit single-build dependency of the help core
- R-V146P4-QC1-S1 (pool-observability) — 3 remaining pool mutation trace paths covered (4/4)
- R-V146P4-QC3-S1 (pool-observability) — all 5 inspiration mutation trace paths covered (9/9 instrumented paths across both DAOs)
- R-V146P3-QC1-S1 (research-auto-chain-e2e) — `preset_version = 2` magic number deduped via `RESEARCH_PRESET_VERSION` const

## Residuals deferred (NOT in this wave; per compass §1.4 → V1.50)

- R-V146P4-QC1-S2 (low, defer V1.50)
- R-V146P4-QC3-S2 (low, defer V1.50)
- R-V146P3-QC3-S1 (low, defer V1.50)
- R-V146P3-QC3-S2 (low, defer V1.50)

## Status.json updates

**none — PM will update.** Per the plan hard rule, the implementer did not modify
`status.json` lifecycle fields. Each residual entry retains its current
`decision: defer` / `target: V1.49` / `note` shape. PM to mark the 10 fixed R#s
`lifecycle: resolved` in P-last T3, attributing each row to the commit SHA in
the table above; the 4 deferred rows (compass §1.4) move to `target: V1.50`.

The residual entries currently use `decision` / `target_date` / `target` /
`note` fields (no `lifecycle` / `closed_at` yet on these rows); PM to apply the
repo's resolved-lifecycle shape during T3.

## Risks / follow-ups

- **R-V146P0-QC3-S3** changed human-path behavior: the stale banner now uses the
  5s `STALE_FETCH_TIMEOUT` (was the default 30s). This is strictly an
  improvement (parity with the JSON path's qc3 F-002 fix) and the banner remains
  best-effort/None-on-failure, but it is a user-observable latency change on the
  `creator works status` human path. No wire-contract or output-shape change.
- **R-V146P2-QC1-S1** relocated `std::process::exit` from the library fn into
  `main()`. The public surface renamed from
  `maybe_print_preset_run_help_and_exit()` → `maybe_render_preset_run_help() ->
  Option<String>`. The only caller was `main.rs` (verified); no external
  consumers (binary-internal helper). Pre-1.0, so no deprecation needed.
- **R-V146P4-QC3-S1** `inspiration_promote_atomic` capture test relies on the
  trace being emitted at function entry; the subsequent works INSERT outcome is
  not asserted (consistent with the residual's scope = trace coverage, not
  function correctness which is covered elsewhere).
- No `#[allow(...)]` suppressions were added. Two clippy lints surfaced during
  the wave (`must_use` on the new `Option`-returning pub fn, and a
  `needless_borrow`) were fixed at the source, not suppressed.
- No wire-contract / JSON-Schema changes (all WL-A is internal hygiene).

## Ready for P-last T2/T3/T4/T5 (PM-driven): yes
