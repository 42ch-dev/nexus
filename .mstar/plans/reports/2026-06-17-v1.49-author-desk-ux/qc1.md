---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: 2026-06-17-v1.49-author-desk-ux
verdict: Approve
generated_at: 2026-06-16T17:10:00Z
review_range: 1475f1fa..0b98e194
working_branch: iteration/v1.49
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: zhipuai-coding-plan/glm-5.2
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-16T15:33:12Z

## Scope
- plan_id: 2026-06-17-v1.49-author-desk-ux
- Review range / Diff basis: `c993ad15..1fa8002` (verbatim from Assignment)
- Working branch (verified): `iteration/v1.49` @ `1fa80021a150cfc40d8a3badc1f35ad80fdc7a47`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 10 (7 implementation/test + 2 harness/doc + 1 status)
- Commit range (identical to Review range): `c993ad15..1fa8002`
  - `4ab9e0be` feat(v1.49-p2): reconcile-chapters --dry-run (R-V148P4-W2)
  - `0948cb87` feat(v1.49-p2): intake re-trigger + reconcile CLI flags (T1+T2)
  - `9aea7091` docs(v1.49-p2): overlay §8 shipped CLI + intake/reconcile surface tests (T3)
  - `7fe873f7` harness(v1.49-p2): mark plan InReview + completion report
  - `a3917063` harness(v1.49-p2): mark P2 InReview (pre-merge status update)
  - `1fa80021` merge(v1.49): P2 — intake re-trigger + reconcile preview
- Tools run: `git diff`/`git log`, `Read`, `Grep`, `cargo check -p nexus42 -p nexus-daemon-runtime -p nexus-local-db` (finished clean, 14.00s)

## Architecture & Maintainability Assessment

The change is well-scoped and architecturally coherent. Detailed responses to the eight
focus areas from the Assignment:

1. **CLI subcommand organization** — `intake` is correctly co-located in
   `works/mod.rs` as a `WorksCommand` variant, consistent with the V1.45 P2 / V1.48 P4
   baseline where every atomic Work operation (`inspire`, `reopen`, `resume-chain`,
   `reconcile-chapters`) lives in the same enum. The `rules_runtime.rs` split exists only
   for the heavier `Findings`/`Rules` subcommand families; `intake` is ~50 lines of logic,
   so a dedicated `intake.rs` module would be premature. **No finding.**
2. **`reconcile-chapters` flag handling** — the `--dry-run`/`--yes` policy mirrors
   `works rules reset` faithfully (TTY-prompt by default, error on non-TTY without
   `--yes`, `--dry-run` early-returns and takes precedence). Precedence is documented.
   See W-1 for the one over-promise in the flag help text, and S-3 for the silent
   `--dry-run --yes` combination.
3. **`reconcile_from_filesystem` signature change** — the `dry_run: bool` parameter is
   the classic boolean scalar, but it is **justified** here: the dry-run and mutate paths
   share the entire filesystem+DB walk and *must* stay in lockstep so the preview matches
   the apply. Gating writes with `if !dry_run` while leaving the `+= 1` counter increments
   ungated guarantees preview/apply fidelity, which the new test explicitly verifies.
   Splitting into "compute diff → apply" would be a larger refactor and risk drift. See
   S-2 for a future-proofing note.
4. **`handle_intake` design** — existence **and** ownership are validated: the GET
   `/v1/local/works/{id}` routes through the daemon `get_work` handler which calls
   `read_active_creator_id` + `works::get_work(pool, &creator_id, &work_id)` (creator-scoped).
   The error path cites §8.1 + `creator bootstrap`. Driver interaction is correct
   (independent schedule, no PATCH to `driver_schedule_id`). See S-1 on the hardcoded
   `preset_id`.
5. **`ReconcileReport` shape** — well-typed `struct` (not tuple) with four `u32` counters.
   `preserved` = no-op count. There is intentionally no `deleted` counter because the
   reconcile walk has no orphan-deletion path (it only inserts/updates/resyncs/preserves),
   so the report accurately reflects the operation's actual semantics. See S-4 for an
   optional enhancement.
6. **Tests organization** — the dry-run test belongs in `runtime_lock.rs` (it asserts
   *lock-not-acquired*, the file's theme) next to `test_reconcile_chapters_releases_lock_on_error`.
   CLI surface tests in `creator_works.rs`; wiremock intake contract tests as `#[cfg(test)]`
   in `works/mod.rs`. No split needed. **No finding.**
7. **Overlay §8 rewrite** — the new "Shipped (V1.49 P2)" tables are accurate to the
   implementation: §8.2 documents `?dry_run=true` threading, lock-skip, `confirm_reconcile_interactive`
   non-TTY error, and `--dry-run` precedence — all matching code. The §8.2 table does **not**
   over-promise a preview. The over-promise is isolated to the in-code clap `--yes` help
   text (W-1), not the overlay.
8. **Scope discipline** — clean. All edits map strictly to T1–T4. The `reconcile_from_filesystem`
   signature change is required by T2; the 8 call-site updates are mechanical (`false`).
   No piggyback refactors, no unrelated formatting churn. **No finding.**

The dry-run zero-mutation correctness property is the heart of R-V148P4-W2 and it is both
implemented correctly (every write — `insert_chapter`, `update_status`,
`sync_frontmatter_status` — is gated; counters are not) and proven by
`test_reconcile_chapters_dry_run_makes_zero_mutations`, which snapshots file bytes + DB
row count + lock holder before/after, then runs a follow-up mutating reconcile to prove
the report was accurate rather than a silent no-op. This is exemplary test design.

## Findings

### 🔴 Critical
- _(none)_

### 🟡 Warning
- **W-1: `--yes` clap help text promises an inline preview the default flow never prints.**
  In `crates/nexus42/src/commands/creator/works/mod.rs` the `ReconcileChapters.yes` field
  doc (shown verbatim in `creator works reconcile-chapters --help`) states:

  > "By default (when stderr/stdin is a TTY) the reconcile **prints a preview and asks
  > for confirmation** before mutating `work_chapters` and chapter frontmatter."

  `confirm_reconcile_interactive()` does **not** print any preview — it emits only the
  generic `dialoguer::Confirm` prompt ("Reconcile work_chapters from filesystem for Work
  {id}? This may create/update chapter rows and rewrite chapter frontmatter."). The
  `created`/`updated`/`resynced`/`preserved` counts are only shown *after* the mutate
  completes, never before. To actually see pending changes the user must run
  `--dry-run` as a separate invocation. Note the inconsistency: the `handle_reconcile_chapters`
  docstring and overlay §8.2 both correctly say only "prompt", so the over-promise is
  localized to the user-visible `--help` text. → **Fix**: either (a) in the default
  non-`--yes` interactive branch, call the dry-run report first and `print_reconcile_report(.., true)`
  before prompting (mirrors `rules_runtime::confirm_reset_interactive` which prints its diff
  first), or (b) correct the help text to remove "prints a preview" and instead point to
  `--dry-run` for the preview. (a) is the better UX and makes the "mirror rules reset"
  claim literally true; (b) is the minimal 1-line doc fix.

### 🟢 Suggestion
- **S-1: Centralize the `creative-brief-intake` preset_id constant.** The string literal
  `"creative-brief-intake"` is now hardcoded in two CLI call sites — the new
  `handle_intake` (`works/mod.rs:1062`) and the pre-existing `bootstrap.rs:303` — plus
  `nexus-orchestration` test tables. This is consistent with existing convention (so not
  a regression), but a shared `const CREATIVE_BRIEF_INTAKE_PRESET_ID: &str` would prevent
  typo drift as the preset name is referenced more widely. Low priority.
- **S-2: Future-proof `reconcile_from_filesystem` options.** The new `dry_run: bool` is
  acceptable today (justified by the shared walk; see assessment #3). If a later plan adds
  knobs (`--limit`, `--volume`, `--delete-orphans`), refactor the trailing bools into a
  `ReconcileOptions { dry_run: bool, .. }` struct so call sites stay self-documenting and
  the 8-site update does not recur per knob.
- **S-3: `--dry-run --yes` is silently resolved.** Passing both flags silently runs the
  dry-run path (`--dry-run` early-returns before `--yes` is consulted). This is documented
  ("Takes precedence over `--yes`") so it is acceptable, but emitting a one-line notice
  ("--yes ignored in --dry-run mode") or using clap `conflicts_with("dry_run")` would make
  the interaction explicit. Low priority.
- **S-4: Optional richer `ReconcileReport` for previews.** For a *preview* to be maximally
  actionable, the report could additionally carry the affected chapter numbers/slugs
  (currently only aggregate counters). Out of scope for V1.49 P2; consider for a future
  author-experience enhancement.

## Source Trace
- Finding W-1:
  - Source Type: manual-reasoning + git-diff
  - Source Reference: `crates/nexus42/src/commands/creator/works/mod.rs:134-141` (`--yes`
    field doc) vs `confirm_reconcile_interactive()` at `works/mod.rs:983-1010` (no preview
    print before prompt); contrast `rules_runtime.rs:399-419` which prints `diff` first.
  - Confidence: High
- Finding S-1:
  - Source Type: git-diff + grep
  - Source Reference: `grep '"creative-brief-intake"'` → `works/mod.rs:1062`,
    `bootstrap.rs:303`, `nexus-orchestration/.../validation.rs:1608`.
  - Confidence: High
- Finding S-2:
  - Source Type: manual-reasoning
  - Source Reference: `work_chapters.rs:524-530` signature; 8 call sites (2 handler +
    6 in-file tests + 1 `v148_serial_hardening.rs`).
  - Confidence: High
- Finding S-3:
  - Source Type: manual-reasoning + git-diff
  - Source Reference: `handle_reconcile_chapters` `if dry_run { … return Ok(()) }`
    precedes the `if !yes` block.
  - Confidence: High
- Finding S-4:
  - Source Type: manual-reasoning
  - Source Reference: `ReconcileReport` struct `work_chapters.rs:351-362`.
  - Confidence: Medium

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 4 |

**Verdict**: Request Changes

One unresolved Warning (W-1): user-facing `--help` text over-promises an inline preview
that the default interactive flow does not produce. The fix is small (either print a
dry-run report before the prompt, or correct one sentence of help text). No Critical
issues; architecture, scope discipline, test design (especially the dry-run zero-mutation
proof), and overlay §8 accuracy are all strong. S-1 through S-4 are non-blocking
maintainability notes for PM to triage (accept-as-residual or schedule).

## Revalidation

- Re-review kind: Targeted (Reviewer 1 of 1 — only qc1 raised blocking; qc2/qc3 stay approved)
- Re-review date: 2026-06-16T17:10:00Z
- Re-review range / Diff basis: `1475f1fa..0b98e194` (verbatim from Assignment — single fix commit `bdd646dc` + merge `0b98e194`; equivalent to `git diff 1475f1fa...0b98e194`)
- Original review range (cross-ref): `c993ad15..1fa8002` (wave 1 — preserved in `## Scope` above)
- Working branch (verified): `iteration/v1.49` @ `0b98e1942410ae93f71fc82883f1aa0fcd9f2753`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files re-reviewed: 2 (`crates/nexus42/src/commands/creator/works/mod.rs` doc comment ±9; `crates/nexus42/tests/creator_works.rs` +28 regression test)
- Tools run: `git diff 1475f1fa...0b98e194`, `Read`, `Grep`, `cargo +nightly fmt --all --check` (exit 0), `cargo clippy -p nexus42 -- -D warnings` (exit 0), `cargo test -p nexus42 --test creator_works` (11 passed / 0 failed)

### Per-finding disposition

| Original finding | Disposition | Evidence |
|---|---|---|
| W-1 (`--yes` help text over-promises an inline preview the default flow never prints) | **Resolved** | Doc comment rewritten to "asks for confirmation" + routes preview to `--dry-run`; matches `confirm_reconcile_interactive()` which emits only a `dialoguer::Confirm` prompt (no preview print). Regression test `works_reconcile_chapters_help_yes_does_not_promise_inline_preview` locks in the fix (passes). |

### Architecture / maintainability re-assessment (focus lens)

1. **Help text accuracy — ✅ resolved.** The new `--yes` doc comment (`works/mod.rs:134-141`) now states the default TTY flow "asks for confirmation before mutating `work_chapters` and chapter frontmatter" and explicitly directs the user to `--dry-run` to "preview the changes without writing". This is fully consistent with `confirm_reconcile_interactive()` (`works/mod.rs:994-1013`), which produces only the generic `dialoguer::Confirm` prompt — no `created`/`updated`/`resynced`/`preserved` counts are printed before the prompt. The over-promise is gone; the help text now mirrors reality. (Cross-check: the non-TTY error message at `mod.rs:1000` already correctly said "Pass --yes to proceed, or --dry-run to preview" — now consistent with the flag doc.)
2. **Behavior unchanged — ✅ surgical.** The only implementation-file edit is the `--yes` doc comment (diff hunk `@@ -133,10 +133,11 @@`). `handle_reconcile_chapters` (`mod.rs:892`) and `confirm_reconcile_interactive` (`mod.rs:994`) are byte-for-byte unchanged; no flag parsing, precedence (`--dry-run` still takes precedence over `--yes`), runtime-lock acquisition, or write-gating logic was touched. The `reconcile_from_filesystem(dry_run: bool)` signature and its 8 call sites are untouched. The dry-run zero-mutation correctness property (R-V148P4-W2) is unaffected.
3. **Regression test solidity — ✅ adequate.** `works_reconcile_chapters_help_yes_does_not_promise_inline_preview` (`creator_works.rs:247-273`) asserts (i) `!help_text.contains("prints a preview")` — the exact over-promising phrase from wave-1 W-1 is gone — and (ii) `help_text.contains("--dry-run") && help_text.contains("preview")` — the preview is routed to `--dry-run`. The first assertion is the strong guard against the specific regression; the second confirms the routing. Test passes (verified: 1 passed / 0 failed). Minor robustness note (not a finding): the assertions scan the entire `--help` output rather than the `--yes` section in isolation, and guard the literal phrase rather than the semantic property — acceptable for a regression lock-in.
4. **Scope discipline — ✅ clean.** Exactly 2 files, +33/-4. The doc rewrite is one logical change (re-wrapped across 5 lines); the test is the new regression guard. No piggyback refactors, no unrelated formatting churn. Implementer's "strict 1-line + test" claim is accurate.
5. **CI gates — ✅ green.** `cargo +nightly fmt --all --check` exit 0; `cargo clippy -p nexus42 -- -D warnings` exit 0; `cargo test -p nexus42 --test creator_works` → 11 passed / 0 failed (includes the new test).

### Choice of fix path

My original W-1 offered two acceptable options: (a) print a dry-run report before the prompt (mirrors `rules_runtime::confirm_reset_interactive`, better UX), or (b) correct the help text (minimal doc fix). The implementer chose (b) — the conservative, surgical, lowest-risk option that fully eliminates the over-promise and routes users to `--dry-run` for previews. This aligns with the "Surgical Changes" principle and is the right call for a fix wave; (a) remains a valid future UX enhancement (adjacent to S-4 "richer `ReconcileReport` for previews"), but is **not** required to resolve W-1.

### New findings in re-review scope

- 🔴 Critical: 0
- 🟡 Warning: 0
- 🟢 Suggestion: 0

### Updated verdict

**Approve** — W-1 is resolved. 0 Critical, 0 Warning in the re-review scope. Original S-1..S-4 remain non-blocking and out of scope for this fix wave (already deferred in the qc-consolidated roll-up).
