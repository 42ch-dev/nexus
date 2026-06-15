---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-06-16-v1.48-serial-hardening"
verdict: "Approve"
generated_at: "2026-06-16"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: grok-build-0.1 (xai/grok-build-0.1)
- Review Perspective: Security and correctness risk (DB status SSOT, path safety, atomicity of FS writes, CLI mass-mutation safety, test coverage of conflict rules)
- Report Timestamp: 2026-06-16

## Scope
- plan_id: 2026-06-16-v1.48-serial-hardening
- Review range / Diff basis: merge-base: 975899e7895cacc34f4966c1e872c93cac670ace (origin/main pre-V1.48) + tip: 1c70b7c2 (iteration/v1.48 HEAD); for P4 scope, focus on commits `2b75aa81..bfc1f332`
- Working branch (verified): iteration/v1.48
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 2 (P4 delta) + supporting call sites for context
  - crates/nexus-local-db/src/work_chapters.rs (core `reconcile_from_filesystem`, `sync_frontmatter_status`, `verify_stories_dir_in_workspace`)
  - crates/nexus-local-db/tests/v148_serial_hardening.rs (new hermetic tests #4/#5)
  - crates/nexus42/src/commands/creator/works/mod.rs (CLI `handle_reconcile_chapters` — read for confirmation surface)
  - crates/nexus-daemon-runtime/src/api/handlers/works.rs (daemon handler — read for delegation surface)
- Commit range (P4 focus): 7093ba2d (T2: resume draft row test), a61fc6a9 (T3: reconcile DB-status-SSOT + test #5)
- Tools run:
  - `git rev-parse --show-toplevel`, `git branch --show-current`
  - `git diff 975899e7..HEAD --stat` + P4 filter
  - `git log --oneline 2b75aa81..bfc1f332 -- <P4 files>`
  - Full file reads of scoped sources + relevant spec sections (§4.5.3, §4.5.7)
  - `cargo clippy --all -- -D warnings` (clean)
  - `cargo test -p nexus-local-db -- v148_serial` (both tests pass)
  - `cargo test -p nexus42 -- reconcile` (0 unit matches — expected; integration surface not named "reconcile")

## Findings

### 🔴 Critical
None.

### 🟡 Warning
- **Non-atomic file write in `sync_frontmatter_status`** (correctness / durability risk)
  - Location: `crates/nexus-local-db/src/work_chapters.rs:462` — `std::fs::write(path, new_content)` after in-memory rewrite of frontmatter.
  - The function reads the full file, rebuilds lines, then does a direct overwrite. No temp-file + atomic rename (`rename` on Unix is atomic for same-filesystem).
  - On crash, power loss, or OOM during the write, the chapter `.md` can be left truncated or with partial frontmatter, while the DB row already reflects the "correct" status. Subsequent runs see a malformed file.
  - This is the only path that mutates chapter files during reconcile (the DB-status-SSOT fix). The risk is narrow (local single-user author machine) but real for a "source of truth" sync tool.
  - Evidence: code inspection of `sync_frontmatter_status` (lines 404-468); no `tempfile`/`NamedTempFile` or `fs::rename` in the function or callers in P4 delta.
  - Fix direction: introduce a small atomic-write helper (write to `.tmp` next to target, `fs::rename`, best-effort cleanup) and use it here. Can be a shared utility in `nexus-local-db` or `nexus-home-layout`.

- **No confirmation / dry-run surface on `creator works reconcile-chapters`** (mass FS mutation without explicit guard)
  - Location: `crates/nexus42/src/commands/creator/works/mod.rs:744` (`handle_reconcile_chapters`) and the daemon POST handler it calls.
  - The command unconditionally walks `Stories/`, rewrites frontmatter on any status conflict, and creates missing rows. It prints counts but never prompts "This will modify N chapter files. Continue? [y/N]" or offers `--dry-run` / `--yes`.
  - Per spec §4.5.3 the re-sync is intentional and "the next run must re-sync", but a user who has hand-edited many frontmatters (or has a large legacy workspace) can lose visible state in one command with no undo except git.
  - This is not a security issue (no traversal, no injection), but it is a correctness/usability risk for the "mass-sync" operation the plan added coverage for.
  - Evidence: CLI handler does a plain POST with `{}` body and renders the report; no `Confirm` or `dialoguer` usage in the P4-scoped handler or the `works` module for this subcommand.
  - Note: the plan is narrowly about §4.5.7 #4/#5 hermetic tests and the DB-wins rule; adding UX guardrails may be out of slice, but the surface exists and is now exercised by the new test.

### 🟢 Suggestion
- **Test coverage of "create missing chapter" and "DB wins" paths is now explicit and good**
  - `v148_serial_reconcile_preserves_db_status_and_creates_missing` (new in a61fc6a9) directly exercises:
    - File present, no DB row → created (ch02).
    - File status differs from DB → DB preserved, file frontmatter rewritten to DB value, body content untouched (ch01).
  - The update to the pre-existing `test_reconcile_update_and_idempotent` is a genuine semantic correction (it now asserts the §4.5.3 direction instead of the old "FS wins" behavior). The test name is slightly stale ("update_and_idempotent" still fits the word_count path), but the assertions are now aligned with the spec and the plan's intent.
  - Resume test (#4) is a clean hermetic check that a single draft row is resumed and no duplicate is created.

- **Path safety is appropriately defended in the P4 delta**
  - `verify_stories_dir_in_workspace` (called early in `reconcile_from_filesystem`) does canonicalize + prefix check on the parent of Stories/. The comment explicitly calls it "Defense in depth".
  - No new traversal vectors were introduced in the P4 changes. `work_ref` is resolved via the normal work-id machinery before reaching the DB layer.

- **No SQL injection or unsanitized path construction in the changed code**
  - All chapter queries continue to use parameterized `sqlx::query` / `bind`.
  - File paths for reconcile are produced by `read_dir` on a verified directory or by formatting known templates; the write path only touches files that came from that same `read_dir` iteration.

- **CLI `reconcile-chapters` handler is a thin delegation (correct for architecture)**
  - The handler resolves the active work, POSTs to the daemon, and renders the `ReconcileReport` counts. No business logic duplication. The real correctness surface is in the `work_chapters` routine (reviewed above).

## Source Trace
- Finding ID: W-001 (non-atomic write)
  - Source Type: manual code review
  - Source Reference: `crates/nexus-local-db/src/work_chapters.rs:404-468` (`sync_frontmatter_status` + `std::fs::write`)
  - Confidence: High

- Finding ID: W-002 (no confirmation on mass FS mutation)
  - Source Type: manual code review + CLI surface scan
  - Source Reference: `crates/nexus42/src/commands/creator/works/mod.rs:744-780` (`handle_reconcile_chapters`)
  - Confidence: High

- Finding ID: S-001 (test coverage quality)
  - Source Type: test inspection + spec cross-check
  - Source Reference: `tests/v148_serial_hardening.rs:104-231` + plan T3 + novel-workflow-profile.md §4.5.3 + §4.5.7 #5
  - Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 4 |

**Verdict**: Approve

## Additional Notes (for PM / subsequent waves)
- The two Warnings are real engineering concerns but do not block the stated acceptance criteria of this plan (hermetic tests #4/#5 pass, DB-as-SSOT rule is implemented and tested, R-V147P1-01 is explicitly deferred).
- If a follow-up hygiene slice touches the reconcile path, the atomic-write helper and a minimal confirmation/dry-run flag on the CLI command would be natural cleanups.
- Lint and the two new serial tests are clean. No pre-existing CI failures were introduced by the P4 delta in the reviewed surface.
- No changes were made to `status.json`, plan files, or implementation code during this review session (per QC constraints).
