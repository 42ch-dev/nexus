---
report_kind: qc-review
reviewer: qc-specialist
reviewer_index: 1
plan_id: 2026-06-12-v1.43-cli-first-run-copy
verdict: Request Changes
generated_at: 2026-06-12T19:15:00+08:00
---

# Code Review Report — P1 (CLI first-run remediation copy)

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: volcengine-plan/deepseek-v4-pro
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-12T19:15:00+08:00

## Scope
- plan_id: 2026-06-12-v1.43-cli-first-run-copy
- Review range / Diff basis: merge-base: cfdd71d3 + tip: 078d74eb
- Working branch (verified): feature/v1.43-cli-first-run-copy
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.43-p1
- Files reviewed: 8
- Commit range: cfdd71d3..078d74eb
- Tools run: git diff, git log, read (plan + specs + quickstart + code diffs), cargo +nightly fmt --all --check, cargo clippy -p nexus42 -p nexus-daemon-runtime -p nexus-orchestration -- -D warnings, cargo test -p nexus42 --lib -- errors::tests + run::tests, cargo test -p nexus-daemon-runtime --lib -- schedules::tests, cargo test -p nexus-orchestration --lib -- preset_gates::tests, rg (quickstart citations, emoji check, DaemonNotReachable call sites, § anchor verification), manual architecture + spec audit.

## Findings
### 🔴 Critical
- (none)

### 🟡 Warning
- **W-1 (architecture gap — dead-code constructor not wired)**: `daemon_not_reachable_quickstart()` in `crates/nexus42/src/errors.rs:247` is marked `#[allow(dead_code)]` and is only called from its own unit test (`errors.rs:579`). The 4 live `DaemonNotReachable` construction sites in `daemon_client.rs` (lines 585, 622, 659, 695) still use the older `daemon_not_reachable("Start the daemon with \`nexus42 daemon start\` and retry.")` — which does **not** cite `docs/novel-writing-quickstart.md §1`. This means the spec §3 "Daemon not reachable" remediation is **not delivered to users in production**. The constructor exists but is architecturally orphaned.
  - **Fix**: Wire `daemon_not_reachable_quickstart()` into the 4 call sites in `daemon_client.rs` (or into the shared `send_request`/error-handling helper those sites delegate to), replacing the old suggestion string. If the old constructor `daemon_not_reachable(suggestion)` is no longer needed after wiring, remove it (and its `#[allow(dead_code)]`). If it is still needed for non-creator paths, keep it but wire the quickstart variant for creator-facing paths.
  - **Confidence**: High

- **W-2 (spec stamp over-claiming)**: The `novel-author-experience.md` §3 table marks "Daemon not reachable" as `- [x] Shipped (V1.43 P1)`, and the `cli-spec.md` §7.1 V1.43 Implemented stamp (line 585) claims "daemon-not-reachable...errors all produce actionable one-line next steps citing docs/novel-writing-quickstart.md §1–§6." Both stamps are inaccurate for the daemon-not-reachable path, which does **not** cite the quickstart in production (see W-1).
  - **Fix**: Either wire the constructor (fix W-1) to make the stamps accurate, or downgrade the daemon-not-reachable checkbox to `- [ ]` (unshipped) and amend the cli-spec stamp to note the daemon path is deferred.
  - **Confidence**: High

### 🟢 Suggestion
- **S-1 (consistency — workspace_slug remediation lacks quickstart citation)**: In `preset_gates.rs:367`, `work_field_remediation("workspace_slug")` returns `"Ensure the workspace has a valid slug."` — the only gate remediation string in the diff that does **not** cite the quickstart. While `workspace_slug` is not novel-specific, the inconsistency is architecturally noticeable in a change whose purpose is to add quickstart citations to all remediation paths. Consider either citing a relevant quickstart section or adding a brief comment explaining why this gate is excluded.
  - **Confidence**: Low

## Source Trace
- Finding ID: W-1
- Source Type: code-review (static analysis of constructor + call-site audit)
- Source Reference: crates/nexus42/src/errors.rs:240-263 (constructor + `#[allow(dead_code)]`) + crates/nexus42/src/api/daemon_client.rs:585,622,659,695 (4 live call sites using old suggestion without quickstart citation)
- Confidence: High

- Finding ID: W-2
- Source Type: spec-audit (cross-reference spec stamps against production code paths)
- Source Reference: .mstar/knowledge/specs/novel-author-experience.md:52 (checkbox) + .mstar/knowledge/specs/cli-spec.md:585 (stamp text) vs crates/nexus42/src/api/daemon_client.rs:585-587 (actual production error string)
- Confidence: High

- Finding ID: S-1
- Source Type: code-review (consistency audit across all remediation functions)
- Source Reference: crates/nexus-orchestration/src/preset_gates.rs:367
- Confidence: Low

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 1 |

**Verdict**: Request Changes

## Spec §3 Alignment Table

| # | Condition | Remediation in code? | Quickstart § correct? | Status |
|---|-----------|---------------------|----------------------|--------|
| 1 | Daemon not reachable | Constructor exists (`errors.rs:247`) but **not wired** to production call sites | §1 (correct) | ⚠️ FAIL — dead code only |
| 2 | `preset_gates_failed` | `work_field_remediation()` + `previous_preset_remediation()` in `preset_gates.rs` | §2 or §3 (correct) | ✅ PASS |
| 3 | Missing scaffold / intake incomplete | `filesystem_remediation()` in `preset_gates.rs` | §2 (correct) | ✅ PASS |
| 4 | Work completed (auto-chain stopped) | `add_schedule` guard (`schedules.rs:183`), `reject_produce_when_novel_complete` (`run.rs:825`), `handle_status` completion block (`works/mod.rs:342`) | §6 (correct) | ✅ PASS |
| 5 | Open findings blocking progress | `handle_status` stale-findings block (`works/mod.rs:286`) | §5 (correct) | ✅ PASS |

## Code Organization Observations

### `daemon_not_reachable_quickstart()` constructor placement

- **Location**: `crates/nexus42/src/errors.rs` — architecturally correct. `CliError` is the nexus42 CLI's error type, and daemon-not-reachable is a CLI-level concern. Placing it in `nexus-daemon-runtime` would be wrong (the daemon runtime doesn't own CLI error presentation).
- **`#[allow(dead_code)]` marker**: This is the core architectural concern. A constructor added specifically for this plan's remediation copy, marked dead-code-allowed, and only exercised by its own test is a maintenance smell. The plan's acceptance criterion #1 states "Each §3 condition produces actionable one-line next step citing docs/novel-writing-quickstart.md section." The daemon-not-reachable condition does **not** meet this criterion in production.
- **Call-site analysis**: The 4 live `DaemonNotReachable` construction sites in `daemon_client.rs` all use `CliError::daemon_not_reachable("Start the daemon with \`nexus42 daemon start\` and retry.")` — the old suggestion without the quickstart citation. These are the actual user-facing error paths. The `daemon_not_reachable_quickstart()` constructor is never called outside its test.

### Help-text changes

- `creator run` help: "Start a new novel project, continue writing, advance chapters, or resume an interrupted work session. For a guided walkthrough, see docs/novel-writing-quickstart.md Part I §1–§3." — Accurate, actionable, points to specific sections covering first-run workflow. ✅
- `creator works` help: "List, inspect, and manage your Works and the selection pool. Shows progress, chapter status, open findings, and completion state. See docs/novel-writing-quickstart.md §4–§6 for usage patterns." — Accurate, matches quickstart structure (serial writing §4, quality §5, completion §6). ✅

### Test placement

All 7 new/updated tests are in the correct test modules:

| Test | Module | Correct? |
|------|--------|----------|
| `daemon_not_reachable_quickstart_cites_section_1` | `nexus42::errors::tests` | ✅ |
| `reject_produce_when_novel_complete_errors_on_none_next_chapter` (updated) | `nexus42::commands::creator::run::tests` | ✅ |
| `completion_guard_message_cites_quickstart_section_6` | `nexus-daemon-runtime::schedules::tests` | ✅ |
| `remediation_work_field_cites_quickstart` | `nexus-orchestration::preset_gates::tests` | ✅ |
| `remediation_filesystem_scaffold_cites_quickstart_section_2` | `nexus-orchestration::preset_gates::tests` | ✅ |
| `remediation_previous_preset_init_cites_quickstart_section_2` | `nexus-orchestration::preset_gates::tests` | ✅ |
| `remediation_previous_preset_writing_cites_quickstart_section_3` | `nexus-orchestration::preset_gates::tests` | ✅ |

Test names follow snake_case convention, are descriptive, and use `cites_quickstart` suffix pattern. ✅

### Error message style consistency

All remediation strings follow the pattern: actionable instruction + `See docs/novel-writing-quickstart.md §N`. Style is consistent across the 5 remediation classes. Multi-line variants (completion guard, works status) add contextual next-step commands before the citation — architecturally appropriate since these are terminal output blocks, not single-line API error fields. ✅

### Cross-link integrity

All cited quickstart § anchors verified present in `docs/novel-writing-quickstart.md`:
- §1 (line 20) — "Prerequisites & Bootstrap", step 5 is "Start the daemon runtime" ✅
- §2 (line 43) — "World & Project Init" ✅
- §3 (line 79) — "First Chapter" ✅
- §4 (line 108) — "Serial Writing with Auto-Chain" ✅
- §5 (line 143) — "Quality Loop" ✅
- §6 (line 172) — "Completion" ✅

## Spec Stamp Audit

### cli-spec.md §7.1 (line 585)

> **V1.43 Implemented (P1):** §7.1 UX principles are now enforced by CLI copy: daemon-not-reachable, `preset_gates_failed`, missing scaffold, work-completed, and open-findings errors all produce actionable one-line next steps citing `docs/novel-writing-quickstart.md` §1–§6 per `novel-author-experience.md` §3 remediation table. Help-text for `creator run` / `creator works` uses quickstart vocabulary per §7.1.

- **Location**: Correct — §7.1 is the UX principles section of the CLI spec Master.
- **Document class**: `cli-spec.md` is a Master spec; V1.43 Implemented stamps are appropriate for Master sections.
- **Accuracy**: The stamp claims daemon-not-reachable produces quickstart-citing next steps. This is **inaccurate** (see W-1, W-2). The other 4 conditions are accurate.
- **Annotation style**: Consistent with existing stamp conventions in the repo (blockquote `>` style, version-tagged).

### novel-author-experience.md §3 (5 checkboxes)

| Row | Checkbox | Accurate? |
|-----|----------|-----------|
| Daemon not reachable | `- [x] Shipped (V1.43 P1)` | ⚠️ Over-claiming — constructor exists but not wired |
| `preset_gates_failed` | `- [x] Shipped (V1.43 P1)` | ✅ |
| Missing scaffold / intake incomplete | `- [x] Shipped (V1.43 P1)` | ✅ |
| Work completed | `- [x] Shipped (V1.43 P1)` | ✅ |
| Open findings blocking progress | `- [x] Shipped (V1.43 P1)` | ✅ |

- **Document class**: `novel-author-experience.md` is a Draft overlay (V1.43); checkboxes are appropriate for tracking implementation status.
- **Annotation style**: Consistent with the spec's existing structure.

## Static Checks

- `cargo +nightly fmt --all --check`: ✅ clean
- `cargo clippy -p nexus42 -p nexus-daemon-runtime -p nexus-orchestration -- -D warnings`: ✅ clean
- Emoji check in crates/: pre-existing emojis only (✓/✗/⚠/🔒/⏰ in terminal output); no new emojis introduced by this change
- Quickstart citation count: 27 lines in crates/ cite `novel-writing-quickstart.md §` — covers all 5 spec §3 conditions + help-text + tests
- All § anchors verified present in `docs/novel-writing-quickstart.md`

## Test Results

- `cargo test -p nexus42 --lib -- errors::tests`: 37 passed (includes `daemon_not_reachable_quickstart_cites_section_1`) ✅
- `cargo test -p nexus42 --lib -- run::tests`: 7 passed (includes updated `reject_produce_when_novel_complete_errors_on_none_next_chapter`) ✅
- `cargo test -p nexus-daemon-runtime --lib -- schedules::tests`: 9 passed (includes `completion_guard_message_cites_quickstart_section_6`) ✅
- `cargo test -p nexus-orchestration --lib -- preset_gates::tests`: 22 passed (includes 4 new remediation citation tests) ✅

## Verdict Rationale

- 0 Critical findings.
- 2 Warning findings (W-1: dead-code constructor not wired to production; W-2: spec stamps over-claiming daemon-not-reachable remediation).
- Per `mstar-review-qc` gate rules: unresolved Warning → `Request Changes`.
- The fix is surgical: wire `daemon_not_reachable_quickstart()` into the 4 call sites in `daemon_client.rs` (or the shared error helper they delegate to), then update the spec stamps if needed. This is a single-crate, single-file change with no blast radius beyond the daemon_client error paths.
- The other 4 spec §3 conditions are fully and correctly implemented. Help-text changes are accurate. Tests are well-placed and meaningful. Code organization is otherwise sound.
