# QA Report — V1.46 P2 novel-runtime-ux-edges

**plan_id**: `2026-06-14-v1.46-novel-runtime-ux-edges`  
**QA mode**: Default (full verification)  
**Working branch (verified)**: `iteration/v1.46`  
**Review cwd (verified)**: `/Users/bibi/workspace/organizations/42ch/nexus`  
**Review range / Diff basis**: `merge-base: 008e6bd8 (P2 merge, base of W-001 fix) → tip: 4e2a1362 (current iteration/v1.46 HEAD after P2 + fix + qc revalidation)` — equivalent `git diff 008e6bd8..4e2a1362`. Code-only delta: `008e6bd8..ce58d005` (4 P2 commits + 1 W-001 fix commit + 2 merges); `4e2a1362` is docs-only.

---

## Scope tested

Full V1.46 P2 scope per plan §4 + qc-consolidated fix round (W-001 only):

- **T1**: On-disk chapter path hints in `works status` (`crates/nexus42/src/commands/creator/works/mod.rs`) — including W-001 cap + tracing + summary line.
- **T2**: Manifest-driven clap `cli_args` help injection for 3 first-slice presets (`run.rs`, `main.rs`).
- **T3**: Tests (unit + binary e2e via `assert_cmd`).
- **T4**: Merge to `iteration/v1.46`.
- **W-001 fix** (qc3 Request Changes → targeted re-review): `CHAPTER_PATH_HINT_CAP = 50`, `tracing::info_span!("chapter_path_hints", ...)`, `+ N more (paths not checked)` summary, 3 new unit tests. Surgical: only `works/mod.rs` touched in fix round.

**Residuals left open** (per qc-consolidated + status.json): 5 P2 + 9 P1 + 4 P0 = 18 total this iteration. R-V139P5-N1 and R-V145B1-002 addressed in code; lifecycle closure deferred to P-last per plan §4.5.

---

## Checkout alignment (verified at dispatch)

```bash
$ git rev-parse --show-toplevel
/Users/bibi/workspace/organizations/42ch/nexus

$ git branch --show-current
iteration/v1.46

$ git log -1 --oneline
4e2a1362 qc(v1.46-p2): qc3 revalidation (targeted re-review)

$ git status --short
(no output)
```

All fields text-identical to Assignment. Working tree clean.

---

## CI gates (mandatory; all exit 0)

**`cargo +nightly fmt --all --check`** (per `crates/nexus42/AGENTS.md`):
```bash
$ cargo +nightly fmt --all --check
(no output)
exit code: 0
```
Silent = pass.

**`cargo clippy --all -- -D warnings`**:
```bash
$ cargo clippy --all -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.25s
```
Zero warnings. Exit 0.

**`cargo test -p nexus42`** (documented 843 passed in qc3 revalidation at `ce58d005`; verified on `4e2a1362`):
- 699 lib tests (core)
- 5 e2e (`creator_run_preset_help`)
- 6 chapter-hint unit tests (T1)
- 3 W-001 cap/summary unit tests (fix round)
- Multiple other test binaries (creator_works, integration, regression, etc.)
- **Grand total across all targets: 843 passed, 0 failed** (840 baseline + 3 new cap tests) — consistent with qc3 revalidation evidence.

Excerpt from relevant runs:
```
running 6 tests
test commands::creator::works::tests::chapter_path_missing_hint_exists_failure_is_silent ... ok
test commands::creator::works::tests::chapter_path_missing_hint_body_missing_on_disk ... ok
...
test result: ok. 6 passed; 0 failed; ...

running 5 tests
test novel_review_master_help_lists_manifest_cli_args ... ok
test novel_manuscript_audit_review_help_lists_manifest_cli_args ... ok
test novel_manuscript_audit_extract_help_lists_manifest_cli_args ... ok
test short_h_flag_also_triggers_enriched_help ... ok
test preset_without_cli_args_falls_through_to_clap_generic_help ... ok
test result: ok. 5 passed; 0 failed; ...

running 3 tests
test commands::creator::works::tests::chapter_path_hint_skipped_summary_format ... ok
test commands::creator::works::tests::chapter_path_hint_cap_only_first_50_chapters_get_hints ... ok
test commands::creator::works::tests::chapter_path_hint_cap_not_triggered_under_50 ... ok
test result: ok. 3 passed; 0 failed; ...
```

All gates green.

---

## Acceptance criteria evidence

### Original P2 ACs (plan §4)

**AC 1**: `works status` chapter table shows warning when configured path missing on disk.
- Unit test `chapter_path_missing_hint_body_missing_on_disk` → **ok**
- Unit test `chapter_path_missing_hint_exists_failure_is_silent` → **ok**
- Both still pass post W-001 fix (cap only affects >50 chapter works; these tests use small inputs).
- Implementation: `print_chapter_table` (works/mod.rs:1394) calls `chapter_path_missing_hint` for first N rows; emits `⚠ {reason} — run: nexus42 creator works reconcile-chapters {safe_work_id}`. `sanitize_for_terminal` applied to `work_id`.

**AC 2**: `nexus42 creator run novel-review-master --help` lists manifest `cli_args` flags.
- E2E test `novel_review_master_help_lists_manifest_cli_args` (tests/creator_run_preset_help.rs:34) → **ok**
- Asserts `PRESET_ARGS_HEADER`, `--finding-id`, `--auto-schedule`, and preset id in output.
- Binary-level via `assert_cmd`; proves full intercept path (`main()` → `maybe_print_preset_run_help_and_exit` → manifest load → `format_preset_run_help` → `exit(0)`).

**AC 3**: Same for both audit presets.
- E2E `novel_manuscript_audit_review_help_lists_manifest_cli_args` → **ok** (asserts `--chapter`, `--volume`, `required`)
- E2E `novel_manuscript_audit_extract_help_lists_manifest_cli_args` → **ok** (asserts `--chapter`)
- Both use same `run_preset_help_stdout` helper against real binary.

**AC 4**: Tests for hint rendering + clap help injection (subset).
- 6 T1 unit tests (chapter path hints, including body/outline/both-missing, no-paths, both-present, exists-failure-silent).
- 5 T3 binary e2e tests (3 positive for first-slice presets + 1 negative fall-through for `novel-writing` + short `-h` trigger).
- Plus 3 new W-001 cap tests (see fix AC below).
- All pass deterministically (no tempdir/network/timing deps for help tests; hermetic tempdir for hint tests).

**AC 5**: Residual R-V139P5-N1 closed; R-V145B1-002 closed or documented partial in P-last.
- **R-V139P5-N1** (chapter body_path hint): Addressed by T1 — `print_chapter_table` now renders `⚠` + reconcile hint when `body_path`/`outline_path` missing on disk. Code present and tested. Lifecycle closure deferred to P-last per plan §4.5.
- **R-V145B1-002** (cli_args in --help): Addressed by T2 — manifest-driven loader injects `cli_args` for `novel-review-master`, `novel-manuscript-audit-review`, `novel-manuscript-audit-extract`. E2E tests prove it. Lifecycle closure deferred to P-last per plan §4.5.
- Verified by code inspection + test execution (no premature residual mutation in this round).

### Fix-round acceptance criteria (qc-consolidated + W-001 Assignment)

**W-001**: Per-chapter `exists()` loop bounded by cap (50 chapters). When chapter count exceeds cap, single summary line `+ N more (paths not checked)` rendered instead of per-row hints for remaining chapters. `tracing::info_span!` (or debug/info) records chapter count + elapsed_ms. New unit test covers cap behavior. Spec-mandated `exists()` behavior preserved (Grill #9).

**Verification** (source read + test execution on `4e2a1362`):

- Const: `const CHAPTER_PATH_HINT_CAP: usize = 50;` (works/mod.rs:1379)
- Span: `let span = tracing::info_span!("chapter_path_hints", total_chapters, capped = hint_cap,);` (line 1418)
- Elapsed log: `if hint_loop_elapsed_ms > 100 { tracing::info!(elapsed_ms = ..., chapters_checked = hint_cap, ...); }` (lines 1454-1460)
- Summary emission:
  ```rust
  if ws_dir.is_some() {
      if let Some(summary) = chapter_path_hint_skipped_summary(total_chapters.saturating_sub(hint_cap)) {
          println!("  {summary}");
      }
  }
  ```
  And `chapter_path_hint_skipped_summary(skipped)` returns `Some(format!("+ {} more (paths not checked)", skipped))` for skipped > 0 (lines 1522-1530).
- 3 new unit tests (all pass):
  - `chapter_path_hint_skipped_summary_format` — exact string contract for `+ 1 more...` / `+ 10 more...` / None when 0.
  - `chapter_path_hint_cap_only_first_50_chapters_get_hints` — 60-chapter fixture; proves exactly 50 hints emitted for first 50, 10 would-have-hinted for tail, summary `+ 10 more (paths not checked)`.
  - `chapter_path_hint_cap_not_triggered_under_50` — 10-chapter work; no cap hit, no summary rendered, per-chapter behavior fully preserved.
- Grill #9 preserved: `chapter_path_missing_hint` still checks both `body_path` and `outline_path` for the first `hint_cap` rows; only the loop is bounded. Daemon reconcile remains authoritative.

**Diff scope discipline**: `git diff 008e6bd8..4e2a1362 --stat` shows W-001 fix touches only `works/mod.rs` (1 file in the surgical commit `eca490a2`); qc3 revalidation (`4e2a1362`) touches only `qc3.md`. No scope creep.

---

## Spec / scope discipline

- **plan_id / Working branch / Review cwd / Review range**: text-identical to Assignment.
- **Open residuals verified** (via `python3` extraction from `.mstar/status.json` on `4e2a1362`):
  ```
  P2 open residuals: 5
    R-V146P2-QC2-W: low - Manifest description text rendered verbatim without sanitize_for_...
    R-V146P2-QC1-S1: low - std::process::exit(0) in library module (maybe_print_preset_run_h...
    R-V146P2-QC1-S2: low - Config-resolution dedup opportunity (operational_workspace_dir_fr...
    R-V146P2-QC3-S1: low - T2 help-intercept exit path should flush stdout before std::proce...
    R-V146P2-QC3-S2: low - CapabilityRegistry::with_builtins() rebuilt on every help interce...
  P1 open residuals: 9
  P0 open residuals: 4
  Total open this iter (P2+P1+P0): 18
  ```
- **Diff basis confirmed**: `git diff 008e6bd8..4e2a1362 --stat` — 6 files total (qa report not yet committed at time of check); W-001 surgical change confined to `works/mod.rs`; all qc docs + status.json updates are metadata.
- **No premature residual closure**: 5 P2 residuals remain open in `residual_findings["2026-06-14-v1.46-novel-runtime-ux-edges"]`. R-V139P5-N1 / R-V145B1-002 addressed in code only; not mutated in this round.
- **No implementation edits by QA**: Only report authored + committed.

---

## Findings

**None blocking.**

All original P2 ACs met with passing tests and correct code paths. W-001 fix meets the exact acceptance criteria (cap=50, span, summary line, 3 new tests, Grill #9 preserved). CI gates clean. Scope discipline holds (only `works/mod.rs` in fix round; residuals left open per qc-consolidated disposition).

The 5 P2 residuals (all low) + 13 other open residuals from prior plans remain tracked and are out of scope for this QA sign-off.

---

## Recommended owners

- **Residuals** (5 P2 + 9 P1 + 4 P0): `@fullstack-dev` (per status.json owner fields); target V1.46+ or backlog per entries.
- **P-last hygiene** (R-V139P5-N1 / R-V145B1-002 lifecycle closure): `@fullstack-dev` or `@project-manager` when running `2026-06-14-v1.46-hygiene-and-closeout`.
- **Future cap tuning / observability**: `@fullstack-dev` if 50 proves too low/high in real author workloads (monitor via the new `chapter_path_hints` span).

---

## Reproduction steps

**Full verification (hermetic):**
```bash
# 1. Checkout alignment (must match Assignment)
git rev-parse --show-toplevel   # /Users/bibi/workspace/organizations/42ch/nexus
git branch --show-current       # iteration/v1.46
git log -1 --oneline            # 4e2a1362 ...
git status --short              # (empty)

# 2. CI gates
cargo +nightly fmt --all --check
cargo clippy --all -- -D warnings
cargo test -p nexus42

# 3. AC evidence (key subsets)
cargo test -p nexus42 chapter_path_missing_hint          # AC1 (6 tests)
cargo test -p nexus42 --test creator_run_preset_help     # AC2/AC3 (5 tests)
cargo test -p nexus42 chapter_path_hint_cap              # W-001 (3 tests)

# 4. W-001 source verification
grep -n 'CHAPTER_PATH_HINT_CAP\|info_span.*chapter_path_hints\|more (paths not checked)' \
  crates/nexus42/src/commands/creator/works/mod.rs

# 5. Residual count
python3 -c '
import json
d=json.load(open(".mstar/status.json"))
p2 = len(d["residual_findings"].get("2026-06-14-v1.46-novel-runtime-ux-edges", []))
p1 = len(d["residual_findings"].get("2026-06-14-v1.46-spec-cli-hygiene", []))
p0 = len(d["residual_findings"].get("2026-06-14-v1.46-author-desk-status-ux", []))
print(f"{p2} P2 + {p1} P1 + {p0} P0 = {p2+p1+p0} open")
'

# 6. Diff scope
git diff 008e6bd8..4e2a1362 --stat
```

All commands above were executed during this QA session and produced the evidence recorded.

---

## Not tested

- **Manual CLI smoke** (`nexus42 creator run ... --help` against a live daemon fixture): Not required for sign-off per Assignment. E2E binary tests via `assert_cmd` already prove the intercept + manifest load + formatting for the 3 presets (and fall-through for `novel-writing`). No daemon state needed for help text.
- **Large-chapter performance measurement** on real author SSD/network mounts: The cap + span + summary are in place; the 60-chapter unit test proves the logic. Real-world latency numbers are out of scope for this verification round (would require instrumented hardware matrix).
- **Full 18-residual closure**: Explicitly out of scope. This QA only verifies that the 5 P2 residuals are still open in status.json and were not closed by the fix round.
- **Other plans in v1.46** (P3, P4, P-last): Not in scope.

---

## QA Verdict

**PASS**

All acceptance criteria (original P2 + W-001 fix-round) are met with reproducible evidence:
- 5 original ACs verified via unit + binary e2E tests + code inspection.
- W-001 cap/span/summary/3-new-tests verified via source read + 3 dedicated passing unit tests.
- CI gates green (clippy 0 warnings, 843 tests passed, nightly fmt clean).
- Checkout alignment, diff scope, and residual discipline all hold (18 open residuals; no premature closure; surgical fix confined to 1 file).
- No blocking findings.

Ready for PM to mark plan `Done` (leaving the 5 P2 + 13 other residuals open per their documented targets).

---

**QA Agent**: `@qa-engineer`  
**Timestamp**: 2026-06-15 (session)  
**Evidence captured on HEAD**: `4e2a1362` (before qa.md commit)  
**Report file**: `.mstar/plans/reports/2026-06-14-v1.46-novel-runtime-ux-edges/qa.md`
