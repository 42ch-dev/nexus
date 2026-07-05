---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-14-v1.46-novel-runtime-ux-edges"
verdict: "Approve"
generated_at: "2026-06-15"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: glm-5.2 (zhipuai-coding-plan/glm-5.2)
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-15T01:35:00Z

## Scope
- plan_id: `2026-06-14-v1.46-novel-runtime-ux-edges`
- Review range / Diff basis: `merge-base: ab3312e2 (P1 Done commit, base of P2 work) → tip: 008e6bd8 (P2 merge + plan checkboxes) (5 commits + 1 --no-ff merge + 1 plan-doc = 7 total)` — equivalent `git diff ab3312e2..008e6bd8`
- Working branch (verified): `iteration/v1.46`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus` (project root, `git rev-parse --show-toplevel`)
- Files reviewed: 5 (4 impl + plan doc)
- Commit range: `ab3312e2..008e6bd8` (identical to Review range)
- Tools run: `git diff --stat`, `git show --stat`, `cargo clippy -p nexus42 -- -D warnings`, `cargo test -p nexus42` (840 passed), `cargo +nightly fmt --all --check`, manual source review

## Scope Confirmation (checkout alignment)
- `git branch --show-current` = `iteration/v1.46` ✓
- `git log -1 --oneline` = `008e6bd8 chore(v1.46-p2): mark plan tasks T1-T4 complete` ✓
- Three-way alignment with QC#2/QC#3 holds: same `plan_id`, same `Review range / Diff basis`, same `Working branch`.

## Change Context (5 files, +689/-7)

| File | Theme | New lines |
| --- | --- | --- |
| `crates/nexus42/src/commands/creator/works/mod.rs` | T1 chapter on-disk hints | +207 |
| `crates/nexus42/src/commands/creator/run.rs` | T2 manifest-driven clap help | +359 |
| `crates/nexus42/src/main.rs` | T2 intercept call site | +5 |
| `crates/nexus42/tests/creator_run_preset_help.rs` | T3 e2e tests (new file) | +117 |
| `.mstar/plans/2026-06-14-v1.46-novel-runtime-ux-edges.md` | plan checkboxes | +4/-4 |

Commit topology (7 commits, `--no-ff` merge confirmed):
```
008e6bd8 chore(v1.46-p2): mark plan tasks T1-T4 complete
c4f5d265 merge(v1.46-p2): novel-runtime-ux-edges into iteration/v1.46
a1790699 test(v1.46-p2): T3 end-to-end help-intercept smoke tests
6b645ba0 feat(v1.46-p2): T2 manifest-driven clap cli_args help injection
9ce99c63 feat(v1.46-p2): T1 on-disk chapter path hints in works status
```

## AC Verification Matrix

| AC | Status | Evidence |
| --- | --- | --- |
| AC1: `works status` chapter table ⚠ + `works reconcile-chapters` hint when path missing | ✅ Met | `chapter_path_missing_hint` (6 unit tests); `print_chapter_table` wiring (works/mod.rs:530-532); best-effort contract pinned by `chapter_path_missing_hint_exists_failure_is_silent` |
| AC2: `creator run novel-review-master --help` lists manifest `cli_args` | ✅ Met | e2e `novel_review_master_help_lists_manifest_cli_args`; manifest declares `finding-id` + `auto-schedule` (verified in `embedded-presets/novel-review-master/preset.yaml:67-76`) |
| AC3: Same for both audit presets | ✅ Met | e2e `novel_manuscript_audit_review_help_lists_manifest_cli_args` + `novel_manuscript_audit_extract_help_lists_manifest_cli_args`; manifests declare `chapter` + `volume` |
| AC4: Tests for hint rendering + clap help injection | ✅ Met | 22 new tests: 6 unit (chapter hint) + 8 unit (argv parser) + 3 unit (formatter) + 5 e2e (help intercept) |
| AC5: R-V139P5-N1 + R-V145B1-002 addressed (closure deferred to P-last) | ✅ Met (code) | Plan §4.5 + residuals: code addresses both; residual lifecycle closure deferred to P-last per plan (not QC scope) |

## Architecture Review (focus: coherence + maintainability)

### T1 — On-disk chapter path hints (works/mod.rs)

**Layering (good):** The change cleanly separates pure logic from I/O:

| Layer | Function | I/O? | Tested? |
| --- | --- | --- | --- |
| Caller (I/O) | `print_chapter_table(chapters, work_id)` | Yes (resolves ws_dir, prints) | Via integration path |
| Config resolver (I/O) | `operational_workspace_dir_from_config()` | Yes (reads CliConfig, home_dir) | Returns `Option` (best-effort) |
| **Pure hint logic** | `chapter_path_missing_hint(ch, ws_dir)` | **No** | 6 hermetic unit tests with `tempfile::tempdir()` |

The pure helper takes `(chapter JSON, ws_dir)` and returns `Option<String>` — hermetically testable without daemon or config dependencies. This is the correct factoring for a CLI-only best-effort surfacing hint.

**Best-effort contract (good):** `Path::exists()` semantics swallow permission/IO errors as `false`, which is the documented behavior (a missing file and an unreadable file both warrant reconcile). Pinned by `chapter_path_missing_hint_exists_failure_is_silent`.

**Remediation copy consistency (good):** The `⚠ {reason} — run: nexus42 creator works reconcile-chapters {safe_work_id}` copy is consistent with the existing V1.42 P-last completion-lock hint (line 1483) and the V1.45 P2 migrated subcommand grammar. The `work_id` is sanitized via `sanitize_for_terminal` before interpolation (line 1410), preventing terminal injection from daemon-sourced data.

**Scope discipline (good):** The `print_chapter_table` signature change (`+work_id: &str`) is the minimal surface area needed to render the per-Work remediation command. No unrelated refactoring.

### T2 — Manifest-driven clap help injection (run.rs + main.rs)

**Data-driven design (verified, good):** The loader is genuinely data-driven — no per-preset hardcode:

- `maybe_print_preset_run_help_and_exit()` resolves the preset via `nexus_orchestration::preset::lookup_preset_by_id` / `resolve_preset` (same APIs as `handle_run`), then reads `loaded.manifest.preset.cli_args` generically.
- Confirmed all 3 first-slice presets declare `cli_args` in their YAML manifests:
  - `novel-review-master`: `finding-id` (string), `auto-schedule` (boolean, default false)
  - `novel-manuscript-audit-review`: `chapter` (integer, required), `volume` (integer, default 1)
  - `novel-manuscript-audit-extract`: `chapter` (integer, required), `volume` (integer, default 1)
- The e2e tests assert the manifest-declared flags appear in `--help` output — they would fail if the loader were hardcoded or the manifests drifted.
- Future presets with `cli_args` get enriched help without Rust code changes (the `format_preset_run_help` formatter is generic over `&[PresetCliArg]`).

**Layering (good):**

| Layer | Function | I/O? | Tested? |
| --- | --- | --- | --- |
| Intercept entry (I/O + exit) | `maybe_print_preset_run_help_and_exit()` | Yes (args, manifest load, exit) | 5 e2e via `assert_cmd` |
| **Pure argv parser** | `extract_run_help_target(argv)` | **No** | 8 unit tests |
| **Pure help formatter** | `format_preset_run_help(preset_id, desc, cli_args)` | **No** | 3 unit tests |

The pure helpers are extracted and hermetically tested. The I/O+exit boundary is isolated to the entry function and covered by binary-level e2e tests.

**Intercept placement (good):** Called at the top of `main()` before `Cli::parse()`. Fast-path: `extract_run_help_target` returns `None` for any non-`creator run <preset_id> --help` invocation, so the manifest load (the expensive part) only runs when the user actually requests preset help. No latency added to other CLI commands.

**Argv parser correctness (good):** Handles `-h`/`--help`, preset-before-help ordering, work_id-before-help, and correctly returns `None` for `--help` before preset_id (mirrors clap's own ordering). The `!tok.starts_with('-')` guard correctly skips dashed tokens when scanning for the preset_id positional.

**Spec consistency (good):** The `PresetCliArg` / `PresetCliArgType` contract matches `creator-run-preset-entry.md` §3.3 (`name`, `type: string|integer|boolean`, `required`, `default`, `description`). The formatter renders type placeholders (`<STRING>`, `<INTEGER>`, none for boolean), required markers, and defaults — consistent with the manifest schema.

### T3 — Tests

Thorough coverage: 22 new tests across 3 layers (pure unit, integration unit with tempdir, binary e2e). The e2e tests use `assert_cmd` to invoke the real `nexus42` binary, proving the full intercept path (argv → manifest load → enriched help → exit 0). The negative test (`preset_without_cli_args_falls_through_to_clap_generic_help`) confirms the fall-through contract for presets without `cli_args`.

### T4 — Merge

`--no-ff` merge commit `c4f5d265` present. Integration HEAD `008e6bd8` includes T1-T3 + merge + plan checkboxes. Branch topology matches the compass dispatch order (P2 → merge into `iteration/v1.46`).

## CI Gates

| Gate | Command | Result |
| --- | --- | --- |
| Clippy | `cargo clippy -p nexus42 -- -D warnings` | ✅ 0 warnings |
| Tests | `cargo test -p nexus42` | ✅ 840 passed, 0 failed (696 lib + 4 + 20 + 37 + 8 + 5 + 7 + 47 + 15 + 1 doctest) |
| New e2e suite | `cargo test -p nexus42 --test creator_run_preset_help` | ✅ 5 passed |
| Fmt | `cargo +nightly fmt --all --check` | ✅ clean |

## Findings
### 🔴 Critical
_(none)_

### 🟡 Warning
_(none)_

### 🟢 Suggestion

- **S-1: `std::process::exit(0)` in library module** (`run.rs:235`) — `maybe_print_preset_run_help_and_exit()` lives in a library module (`nexus42::commands::creator::run`) but calls `std::process::exit(0)` on the happy path. This is a known Rust library anti-pattern (untestable in-process; skips destructors). The mitigation is strong: the function name signals the side effect (`_and_exit`), the doc comment documents it, the two pure helpers (`extract_run_help_target`, `format_preset_run_help`) are extracted and hermetically tested, and the full exit path is covered by 5 `assert_cmd` e2e tests. The alternative (return `enum HelpHandled(String) | NoAction` and let `main()` print + exit) would be marginally cleaner for in-process testing but adds boilerplate for no behavioral gain. **Low priority** — current design is acceptable given the mitigations; revisit only if the intercept grows non-trivial branching that would benefit from in-process unit tests.

- **S-2: Config-resolution dedup opportunity** (`works/mod.rs:1422-1430` vs `1466-1488`) — The new `operational_workspace_dir_from_config()` helper (T1) duplicates the inlined config-resolution logic in the pre-existing `print_completion_lock_hint()` (V1.42 P-last). The new helper is better factored (returns `Option`, uses `?` chaining). The pre-existing function additionally has a latent `dirs::home_dir().unwrap_or_default()` (line 1473) that would produce a nonsense path if `home_dir()` returns `None` — the new helper handles this correctly via `?`. **Scope note:** both the duplication and the `unwrap_or_default` are pre-existing (not introduced by this diff); the new code does not worsen either. Refactoring `print_completion_lock_hint()` to call the new helper would reduce duplication and fix the latent issue, but is out of scope for P2. Defer to a future hygiene pass.

## Source Trace
- Finding S-1: `crates/nexus42/src/commands/creator/run.rs:205-236` (`maybe_print_preset_run_help_and_exit`)
- Finding S-2: `crates/nexus42/src/commands/creator/works/mod.rs:1422-1430` (new helper) vs `1466-1488` (pre-existing `print_completion_lock_hint`)

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 2 |

**Verdict**: Approve

Rationale: The P2 change is a well-factored, narrowly-scoped pair of runtime UX improvements. T1 cleanly separates pure hint logic from I/O. T2 is genuinely data-driven (no per-preset hardcode), extensible (future presets get enriched help via YAML only), and correctly intercepts before `Cli::parse()`. All 5 ACs are met with 22 new tests across 3 layers. CI gates green (clippy clean, 840 tests pass, fmt clean). The 2 Suggestions are non-blocking architectural notes (exit-in-library pattern, pre-existing config-resolution dedup) — neither warrants a Warning given the strong mitigations and scope discipline.
