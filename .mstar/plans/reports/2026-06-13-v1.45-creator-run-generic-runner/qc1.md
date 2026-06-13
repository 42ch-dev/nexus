---
report_kind: qc-review
reviewer: qc-specialist
reviewer_index: 1
plan_id: 2026-06-13-v1.45-creator-run-generic-runner
secondary_plan_ids:
  - 2026-06-13-v1.45-delete-bespoke-run-subcommands
  - 2026-06-13-v1.45-creator-bootstrap-and-works-migration
verdict: Request Changes
generated_at: 2026-06-14T00:30:00Z
review_range: "merge-base: 76a9eb79; tip: 79f540dc; equivalent: git diff 76a9eb79...79f540dc"
working_branch: iteration/v1.45
review_cwd: /Users/bibi/workspace/organizations/42ch/nexus
---

# Code Review Report — QC #1 (Architecture / Maintainability)

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-14T00:30:00Z

## Scope
- plan_id: `2026-06-13-v1.45-creator-run-generic-runner` (P0 primary; P1 + P2 in same atomic merge)
- Secondary plan_ids: `2026-06-13-v1.45-delete-bespoke-run-subcommands`, `2026-06-13-v1.45-creator-bootstrap-and-works-migration`
- Review range / Diff basis: merge-base: `76a9eb79` (origin/main V1.44) → tip: `79f540dc` (HEAD on `iteration/v1.45`); equivalent: `git diff 76a9eb79...79f540dc`
- Working branch (verified): `iteration/v1.45`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Files reviewed: 38 (4170 insertions, 2648 deletions)
- Commit range: `76a9eb79..79f540dc` (18 commits: 3 feature + 5 merge + 2 PM conflict + 8 harness/spec/docs)
- HEAD at review time: `ad7b5565` (2 harness-only commits beyond `79f540dc`; only `.mstar/status.json` changed between `79f540dc..ad7b5565`)
- Tools run: `cargo +nightly fmt --all -- --check`, `cargo clippy --all -- -D warnings`, `cargo test -p nexus42 --lib`, `cargo test -p nexus42 --test command_surface_contract`, `cargo test -p nexus-orchestration --lib preset`, `git diff` inspection, daemon-runtime handler cross-reference

## Findings

### 🔴 Critical

#### C-1: `work_id` not injected into `AddScheduleRequest.input` for non-FL-E presets — gated presets will fail with 422

**Location**: `crates/nexus42/src/commands/creator/run.rs`, `handle_run()` lines 128–161

**Issue**: The generic `creator run <preset_id>` runner resolves `work_id` (from the CLI positional arg or the pool active Work via `resolve_work_id`) at line 106, but in the non-FL-E dispatch path (lines 128–179), the resolved `work_id` is **only used for the display message** (`println!("Work: {resolved_work_id}")` at line 176). It is **never injected** into `AddScheduleRequest.input`.

The `input` field (line 158) contains only the parsed `cli_args` (e.g., `{"chapter": 5, "volume": 1}`). Since `AddScheduleRequest` has no top-level `work_id` field (verified in `crates/nexus-contracts/src/local/schedule/http.rs` line 23), the daemon must resolve `work_id` from `input["work_id"]` or `seed`.

The daemon handler (`crates/nexus-daemon-runtime/src/api/handlers/orchestration/schedules.rs` lines 197–212, 316–348) resolves `work_id` from `body.input["work_id"]` or `body.seed`. For **gated presets** (all three audit presets and `novel-review-master` declare gates), when `work_id` is absent the daemon returns **422 `preset_gates_failed`** with message "work_id must be provided for gated preset" (lines 322–348).

**Impact**: Every gated non-FL-E preset dispatched via the generic runner will fail:
- `creator run novel-manuscript-audit-review --chapter 5` → 422
- `creator run novel-manuscript-audit-extract --chapter 5` → 422
- `creator run novel-review-master` → 422

This breaks the core P0 deliverable. The generic runner is the sole entry point for preset dispatch after P1 deletes the bespoke handlers.

**Root cause**: Every old bespoke handler (`handle_audit_chapter`, `handle_review_master`, `handle_run_legacy::Start`) explicitly injected `"work_id": work_id` into the input JSON. The generic runner's `parse_preset_cli_args` only returns declared `cli_args` and does not add `work_id`.

**Fix**: After parsing cli_args, inject `work_id` into the input object before building the request:

```rust
let mut input = parse_preset_cli_args(&loaded.manifest.preset.cli_args, &extra)?;
// Inject resolved work_id so the daemon can evaluate gates and execute the preset.
if let serde_json::Value::Object(ref mut map) = input {
    map.entry("work_id").or_insert(serde_json::Value::String(resolved_work_id.clone()));
}
```

`work_id` is confirmed NOT in `RESERVED_INPUT_KEYS` (`["creator_id", "workspace_slug", "core_context", "preset"]` — schedules.rs line 72).

**Verification**: `cargo test -p nexus42 --lib` passes (665 tests), but no integration test exercises the full `creator run <preset_id>` → daemon schedule creation flow for non-FL-E gated presets. The 3 new V1.45 surface tests only check `--help` output.

### 🟡 Warning

#### W-1: ~1300 lines of dead legacy code retained in `run.rs` as `#[allow(dead_code)]` — P1 acceptance criterion #2 unmet

**Location**: `crates/nexus42/src/commands/creator/run.rs`, lines 298–1654 (14 `#[allow(dead_code)]` annotations in this one file)

**Issue**: The P1 plan (`2026-06-13-v1.45-delete-bespoke-run-subcommands.md`) §2 Goal #1 says "Delete `RunCommand` variants: `Start`, `Continue`, `Stage`, `Resume`, `ReconcileChapters`, `AuditChapter`, `ReviewMaster`" and §4 Acceptance Criterion #2 says "No references to `audit-chapter`, `review-master`, `stage advance` in `run.rs`." The file header comment (lines 13–14) states: "Legacy handler code is preserved as `#[allow(dead_code)]` for P1/P2 migration reference."

However, the file still contains **2356 lines**, of which approximately **1300 lines** (55%) are dead legacy code:
- `LegacyRunCommand` enum (7 variants, lines 302–462)
- `AuditMode` enum + `Display` impl (lines 466–481)
- `StageCommand` enum (lines 485–516)
- `handle_run_legacy()` (~510 lines, lines 528–1039)
- `fetch_work_context()` (lines 1051–1073)
- `handle_review_master()` (~270 lines, lines 1086–1358)
- `handle_audit_chapter()` (~145 lines, lines 1385–1529)
- `resolve_audit_body_path()` (lines 1542–1570)
- `validate_body_path()` (lines 1577–1595)
- `handle_stage()` (~50 lines, lines 1605–1654)
- `stage_list()` (~50 lines, lines 1661–1711)

`rg -n 'audit-chapter|review-master|stage advance|creator run continue|creator run start'` on `run.rs` returns **20+ matches** — all in dead code, but P1 acceptance criterion #2 is literally unmet.

The assignment's review criteria states: "Are legacy handler functions cleanly removed (no `#[allow(dead_code)]` leakage)?" — the answer is **no**.

**Impact**: Significant maintainability burden. The `bootstrap.rs` module (P2, 558 lines) is a near-exact extraction of the `LegacyRunCommand::Start` handler. With both present, the same logic exists in two places, and any future fix must be applied to both or the dead copy becomes drift documentation.

**Fix**: Delete all `#[allow(dead_code)]` legacy code from `run.rs` (lines 298–1654, excluding `stage_advance` and its live helpers `validate_produce_chapter_context`, `reject_produce_when_novel_complete`, `assemble_world_kb_block` which are still called by `handle_run`'s FL-E path). Move the retained tests for `validate_body_path` and `resolve_audit_body_path` into a dedicated test module or delete them if the tested functions are removed.

#### W-2: Preset CLI arg parser doesn't support `--flag=value` inline syntax

**Location**: `crates/nexus42/src/commands/creator/run.rs`, `parse_preset_cli_args()` lines 210–296

**Issue**: The parser only handles space-separated `--flag value` and standalone boolean `--flag`. It does not handle `--flag=value` (inline equals syntax), which is a standard convention in CLI tools including clap itself.

When a user runs `creator run novel-manuscript-audit-review --chapter=5`, clap captures `["--chapter=5"]` into `extra` (via `trailing_var_arg`). The parser then does `token.strip_prefix("--")` → `"chapter=5"`, looks up `"chapter=5"` in the cli_args map, fails, and returns: `"Unknown preset flag '--chapter=5'. This preset accepts: --chapter, --volume"`.

This is confusing because clap's own `--help` doesn't document the preset flags, so the user's natural instinct is to use the standard `--flag=value` syntax.

**Fix**: Split on `=` before lookup:

```rust
let (name, inline_value) = match token.strip_prefix("--") {
    Some(s) => match s.split_once('=') {
        Some((n, v)) => (n, Some(v.to_string())),
        None => (s, None),
    },
    None => return Err(/* ... */),
};
```

Then use `inline_value` as the value when present, falling back to consuming the next raw token.

#### W-3: `resolve_work_id` duplicated between `run.rs` and `works/mod.rs`

**Location**: `run.rs` lines 183–204 and `works/mod.rs` ~line 585

**Issue**: Both files contain an identical private `resolve_work_id` function that queries `/v1/local/works?limit=1&status=active` to resolve the pool active Work. The `works/mod.rs` copy was added as part of P2 migration.

**Fix**: Extract to a shared helper in `commands/creator/mod.rs` or a small `work_utils.rs` module.

### 🟢 Suggestion

#### S-1: `works_start_handler_returns_clear_error` test doesn't actually test the handler

**Location**: `crates/nexus42/src/commands/creator/works/mod.rs`, test at ~line 1810

**Issue**: The test constructs an async block that calls `handle_works(WorksCommand::Start { ... }, ...)` but never `.await`s it. It then compares a hardcoded string literal (`"creator works start" is not available"`) against another hardcoded string literal — a tautology that always passes regardless of handler behavior.

**Fix**: Either use a tokio runtime to actually invoke the handler and assert on the returned error, or remove the test and rely on the parsing tests (`works_start_is_intercepted`).

#### S-2: `parse_preset_cli_args` help text not surfaced in `--help` output

**Location**: `crates/nexus42/src/commands/creator/run.rs`

**Issue**: The `PresetCliArg.description` field is validated by `check_cli_args` in `validation.rs` but never surfaced to users in `creator run <preset_id> --help`. Users must know the preset's flags beforehand. Consider a `creator run <preset_id> --help-args` subcommand or dynamic `--help` generation that lists the preset's declared `cli_args`.

#### S-3: Consider splitting `RunCommand` into smaller Args structs

**Location**: `crates/nexus42/src/commands/creator/run.rs` lines 37–62

**Issue**: The `RunCommand` struct mixes positional args (`preset_id`, `work_id`), global flags (`--json`, `--force-gates`, `--reason`), and trailing var-args (`extra`). Splitting into `GlobalRunFlags` + positional `RunArgs` could improve readability and enable flag reuse in future commands.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| C-1 | manual-reasoning + daemon-runtime cross-ref | `run.rs:106,128-161`; `schedules.rs:197-212,316-348`; `http.rs:23-54`; `RESERVED_INPUT_KEYS` at `schedules.rs:72` | High |
| W-1 | git-diff + linter | `run.rs:298-1654`; P1 plan §2,#1 + §4,#2; `rg -n` matches | High |
| W-2 | manual-reasoning | `run.rs:210-296`; clap `trailing_var_arg` semantics | High |
| W-3 | git-diff | `run.rs:183-204`; `works/mod.rs:~585` | High |
| S-1 | manual-reasoning | `works/mod.rs:~1810` | High |
| S-2 | manual-reasoning | `run.rs`; `validation.rs:check_cli_args` | Medium |
| S-3 | manual-reasoning | `run.rs:37-62` | Low |

## Architecture Coherence Assessment

### Three-plane IA ✓
The implementation correctly matches the three-plane information architecture:
- **`creator bootstrap`** (composite): `BootstrapArgs` struct + `handle_bootstrap` in `bootstrap.rs` — sole composite entry for new Work creation
- **`creator works`** (atomic): `WorksCommand` enum with strictly single-purpose subcommands (`inspire`, `reopen`, `resume-chain`, `reconcile-chapters`) — each performs exactly one business function
- **`creator run <preset_id>`** (strategy): generic `RunCommand` struct with `#[command(flatten)]` — preset dispatch by ID

### Generic runner shape ✓ (with C-1 caveat)
`RunCommand` is now a struct (not enum) with `#[command(flatten)]` in `CreatorCommand::Run`. The legacy `RunCommand` enum is renamed to `LegacyRunCommand` and marked `#[allow(dead_code)]`.

### No deprecation aliases ✓
No `--deprecated` flag, no shim commands, no compatibility layer. The hidden `WorksCommand::Start`/`Create` variants are interception guards, not aliases — they produce clear errors directing users to `creator bootstrap`.

### Hint string consistency ✓ (in live code)
Live hint strings are correctly updated to V1.45 surface:
- `bootstrap.rs:428`: `creator run novel-writing {work_id}` (generic runner)
- `bootstrap.rs:439`: `creator works inspire {work_id} --note "..."`
- `works/mod.rs`: `creator run reflection-loop {safe_work_id}` (replaced `stage advance --stage review`)
- `works/mod.rs`: `creator works reconcile-chapters {work_id}` (replaced `run reconcile-chapters`)
- `works/mod.rs`: `creator works resume-chain` (replaced `run resume`)

### DEPRECATED preset dir ✓
`embedded-presets/novel-manuscript-audit/` fully deleted (86-line preset.yaml + 2 prompt files). Split presets (`novel-manuscript-audit-review`, `novel-manuscript-audit-extract`) correctly declare `cli_args` for the generic runner.

### BL-13 allowlist cleanup ✓
`memory-review` removed from `STAGE_PRESET_ALLOWLIST` persist stage — no matching preset existed. Test updated from `accepts_both_paths` to `accepts_kb_extract_only`.

### Contract schema ✓
`PresetCliArg` and `PresetCliArgType` correctly added to `PresetHeader` in contracts crate with serde defaults. `check_cli_args` validation in orchestration covers kebab-case naming, duplicate detection, and required+default conflict.

## CI Gates

| Gate | Result |
|------|--------|
| `cargo +nightly fmt --all -- --check` | **PASS** (no output — clean) |
| `cargo clippy --all -- -D warnings` | **PASS** (`Finished dev profile`) |
| `cargo test -p nexus42 --lib` | **PASS** (665 passed, 0 failed) |
| `cargo test -p nexus42 --test command_surface_contract` | **PASS** (37 passed, 0 failed) |
| `cargo test -p nexus-orchestration --lib preset` | **PASS** (207 passed, 0 failed) |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 1 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 3 |

**Verdict**: Request Changes

The Critical finding (C-1) must be resolved before approval: the generic runner's non-FL-E dispatch path does not inject `work_id` into the schedule request input, causing all gated non-FL-E presets to fail with 422. This breaks the core P0 deliverable. The Warning findings (W-1 through W-3) are strong maintainability concerns that should be addressed in the same fix round — W-1 in particular represents ~1300 lines of dead code that contradicts the P1 plan's stated "hard delete" approach.
