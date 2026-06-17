---
report_kind: qc-review
reviewer: qc-specialist
reviewer_index: 1
plan_id: 2026-06-12-v1.43-hygiene-and-residuals
verdict: Approve
generated_at: 2026-06-12T22:15:00+08:00
---

# Code Review Report — P-last (hygiene and residuals)

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: deepseek-v4-pro (volcengine-plan/deepseek-v4-pro)
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-12T23:45:00+08:00

## Scope
- plan_id: 2026-06-12-v1.43-hygiene-and-residuals
- Review range / Diff basis: merge-base: a693752b + tip: 283d61e4
- Working branch (verified): feature/v1.43-hygiene-and-residuals
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.43-p-last
- Files reviewed: 11
- Commit range: a693752b..283d61e4 (3 commits: 445e0f1d, 2c13c2c6, 283d61e4)
- Tools run: cargo +nightly fmt --all --check, cargo clippy -p nexus-orchestration -p nexus-local-db -- -D warnings, cargo test -p nexus-orchestration --lib -- warn_unknown, cargo test -p nexus-local-db --lib -- volume_aware, rg TODO/FIXME/XXX, rg Shipped stamps, python3 -m json.tool status.json, manual diff review

## Findings
### 🔴 Critical
- **C-01: Clippy `too_long_first_doc_paragraph` on new code blocks compilation with `-D warnings`**
  → The doc comment on `warn_unknown_top_level_keys` (loader.rs:1056–1063) exceeds the first-paragraph length limit enforced by `clippy::too-long-first-doc-paragraph` (pedantic, promoted to deny via `-D warnings`). This is new code introduced in commit `445e0f1d`. The lint cleanup commit `283d61e4` addressed `too_many_lines` on `reconcile_from_filesystem` but did not fix this error. `cargo clippy -p nexus-orchestration -- -D warnings` fails with this single error. Per project AGENTS.md: "Do not suppress with `#[allow(...)]` without a brief justification comment." Fix: either shorten the first doc paragraph to fit the limit, or add a justified `#[allow(clippy::too_long_first_doc_paragraph)]` on the function.

### 🟡 Warning
- **W-01: `KNOWN_TOP_LEVEL_KEYS` is a manual mirror of `PresetManifest` struct fields — maintenance trap**
  → The constant at loader.rs:1054 hardcodes `["preset", "states", "inner_graphs", "signals", "roles"]`. These correspond to the five `#[serde]` fields of `PresetManifest` in `nexus-contracts/src/local/orchestration/preset.rs:37–52`. If a future iteration adds a new top-level field to `PresetManifest` (e.g. a `templates` key), this constant must be manually updated — there is no compile-time check tying the two together. The divergence would cause the new field to trigger `tracing::warn!` on every preset load, creating noise without a real problem. Fix: add a comment cross-referencing `PresetManifest` struct location, or consider a compile-time assertion (e.g. a test that deserializes a YAML with all known keys and verifies no warnings are emitted).

### 🟢 Suggestion
- **S-01: The `#[allow(clippy::too_many_lines)]` justification on `reconcile_from_filesystem` is well-written but the function is now 130+ lines**
  → The justification (work_chapters.rs:412–416) is clear and references the specific residual (R-V142P1-F-003). However, the function grew from ~95 to ~130 lines in this change. The new insert-path code (lines 503–515) is a candidate for extraction: the "apply frontmatter status after insert" logic could be a small helper. Not blocking — the current state is acceptable — but worth noting for future maintainability.

- **S-02: `warn_unknown_top_level_keys` is `pub` but only called from `load_preset_from_str_with_limits`**
  → The function is declared `pub` (loader.rs:1064). It is only called once, from within the same module (line 200). Making it `pub(crate)` or private would better signal intent and reduce API surface. Low priority.

## Source Trace
- Finding ID: C-01
- Source Type: clippy (static analysis)
- Source Reference: `cargo clippy -p nexus-orchestration -- -D warnings` → `loader.rs:1056:1`
- Confidence: High

- Finding ID: W-01
- Source Type: manual-reasoning (architecture audit)
- Source Reference: `loader.rs:1054` vs `nexus-contracts/src/local/orchestration/preset.rs:37–52`
- Confidence: Medium

- Finding ID: S-01
- Source Type: manual-reasoning (code review)
- Source Reference: `work_chapters.rs:412–534`
- Confidence: Low

- Finding ID: S-02
- Source Type: manual-reasoning (code review)
- Source Reference: `loader.rs:1064` (pub fn) vs `loader.rs:200` (sole call site)
- Confidence: Low

## Per-§2-Row Architecture Audit

| ID | Disposition | Architecture Check | Result |
| --- | --- | --- | --- |
| R-V137P0-01 | fix | Strict loader: `warn_unknown_top_level_keys` placed correctly in `load_preset_from_str_with_limits` after size/depth checks, before deserialization. `KNOWN_TOP_LEVEL_KEYS` matches `PresetManifest` fields. Warn-vs-fail distinction (non-fatal `tracing::warn!`) is appropriate for P-last — existing embedded presets must not break. **However, clippy error C-01 blocks this.** | **FAIL (C-01)** |
| R-V142P1-F-003 | fix | Volume-aware reconcile: frontmatter `volume` parsing is in the correct module (`work_chapters.rs`). Does NOT change persistence schema — `volume` column already exists (V1.42 migration `202606110001_v142_multi_volume_pk.sql`). The change only makes the reconcile path use the frontmatter value instead of hardcoding `1`. Backward-compatible. | **PASS** |
| R-V138 chain-engine completion UX | triage ("already-fixed") | `reject_produce_when_novel_complete` guard exists at `crates/nexus42/src/commands/creator/run.rs:817`. It checks `target_stage == "produce" && next_chapter.is_none()` and returns a clear error referencing quickstart §6. Adequately handles the chain-engine completion case. | **PASS** |
| R-V142P0-QC-W-01 | waive | Waiver rationale: V1.42.1 hotfix (279ec7b3) resolved the 5-site release leak class; TTL-based cleanup sufficient for local-only single-user model. Rationale is correct — the hotfix commit exists on main, the architectural lesson is tracked in R-V142.1-ARCH-LESSON. | **PASS** |
| R-V142P0-QC-W-001 | waive | Waiver rationale: local-only single-writer invariant documented; race only under concurrent multi-terminal invocation which is out of scope. Rationale is correct — matches the product's local-first model and the documented invariant. | **PASS** |
| R-V143P0-001 | fix | §2 row 4 amendment: changed `creator run status` → `creator works status`. The wording change is minimal and matches V1.41 cli-spec.md §6.2H. Quickstart already uses `creator works status` (verified at docs/novel-writing-quickstart.md lines 88, 103, 120, 159, 190). | **PASS** |
| R-V143P0-002 | defer to V1.44+ | Deferral rationale: keep both spec + CLI with explicit notes. Quickstart line 174 has inline note explaining the gap. Spec documents future `review-master` surface; CLI provides `creator run stage advance --stage review` as available remediation. Sound deferral — the gap is documented, non-blocking, and the convergence path is clear. | **PASS** |

## Spec Promotion Coherence

| Spec | Claim | Verification | Result |
| --- | --- | --- | --- |
| `preset-conditional-routing.md` | Shipped V1.42 P2 | `llm_judge` GO/NOGO is implemented in `embedded-presets/novel-writing/preset.yaml` (lines 127–129). DF-56 minimal slice shipped in V1.42 P2. Spec stamp is accurate. | **PASS** |
| `novel-writing/workflow-profile.md` | V1.42 Shipped stamps | Multi-volume PK migration (§4.5.4), volume outline scaffold (§4.5.5), migration tests (§4.5.7) all shipped via plan `2026-06-11-v1.42-multi-volume`. Spec stamp is accurate. | **PASS** |
| `novel-writing/author-experience.md` | Shipped V1.43 | P0 (BL-10), P1 (CLI copy), P2 (author visibility) all Done. §6 disposition: kept as Feature line supplement (not archived). Correct — the document maps quickstart sections to CLI surfaces and serves ongoing reference value. Promotion to Shipped Feature line is appropriate. | **PASS** |

## Iteration Closeout Completeness

| Item | Verification | Result |
| --- | --- | --- |
| `iterations/README.md` V1.43 → Shipped | Line 57: `**Shipped** (2026-06-12)` with correct scope summary. | **PASS** |
| Deferred tracker V1.43 quick-status | Quick-status line updated: `**V1.43 Shipped**`; `**Status**: Shipped (V1.43 — 2026-06-12)`. Footer updated. | **PASS** |
| `status.json` residual closures | R-V137P0-01, R-V142P1-F-003, R-V143P0-001 → `lifecycle: resolved` with closure notes and evidence. R-V142P0-QC-W-01, R-V142P0-QC-W-001 → `lifecycle: waived` with rationale. R-V143P0-002 → `lifecycle: defer` with updated target. All entries have `closed_at` or updated notes. JSON valid. | **PASS** |
| `tech_debt_summary` refreshed | `total_open`: 91→86; `by_severity.high`: 1→0; `by_severity.medium`: 10→7; `by_target.v1.43`: 5→2; `by_target.v1.42`: 14→9; new `by_target.v1.44`: 3. Notes updated with P-last closure summary. | **PASS** |

## Code Organization

- **Strict loader placement**: `warn_unknown_top_level_keys` is in `preset/loader.rs` (existing module). The function is placed after `yaml_value_depth` and before the test module — logical grouping with other validation helpers. Appropriate.
- **Volume-aware reconcile placement**: Changes are in `work_chapters.rs` (existing module). The function `reconcile_from_filesystem` was already there; the change modifies it in-place. Correct placement.
- **No new modules introduced**: Both changes extend existing modules. No architectural concerns.

## Lint Discipline Audit

| Check | Result |
| --- | --- |
| `#[allow(clippy::too_many_lines)]` justification on `reconcile_from_filesystem` | Present (work_chapters.rs:412–416). References R-V142P1-F-003. Adequate. |
| `#[allow(clippy::too_long_first_doc_paragraph)]` on `warn_unknown_top_level_keys` | **MISSING** — this is C-01. |
| `cargo +nightly fmt --all --check` | **PASS** — no output (clean). |
| No TODO/FIXME/XXX in changed Rust files | **PASS** — `rg` returned "no TODOs: OK". |

## Test Quality

| Test | Assessment |
| --- | --- |
| `warn_unknown_top_level_keys_detects_misplaced_gates` | **Meaningful**: Loads a YAML with a misplaced `gates:` key at root, verifies the preset still loads (non-fatal), then directly verifies the `KNOWN_TOP_LEVEL_KEYS` filter detects "gates" as a stray key. Tests both the non-breaking behavior and the detection logic. |
| `test_reconcile_volume_aware_from_frontmatter` | **Meaningful**: Creates two chapter files — one without volume field (defaults to 1), one with explicit `volume: 2`. Verifies both are created, then asserts on `get_chapter` results for each volume. Tests the full reconcile path with volume awareness and the default-to-1 fallback. |

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 1 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 2 |

**Verdict**: **Request Changes** — C-01 (clippy `too_long_first_doc_paragraph` on new code) is a blocking Critical. The lint cleanup commit `283d61e4` was supposed to handle all clippy issues but missed this one. Once fixed, the remaining Warning (W-01, maintenance trap) is non-blocking for P-last but should be addressed in a follow-up.

## Revalidation (post-fix wave, fix commit 016832f1)

**Re-review mode**: Targeted — qc-specialist only (raised 1 blocking Critical in initial wave)
**Fix range reviewed**: 283d61e4..016832f1
**Files in fix wave**: crates/nexus-orchestration/src/preset/loader.rs (+90/-22), Cargo.toml (+1 dev-dep), crates/nexus-local-db/src/work_chapters.rs (+50/-X), Cargo.lock

### Previously raised blocking findings — re-check
| Finding ID | Summary | Status | Evidence |
|------------|---------|--------|----------|
| qc1-C-01 | clippy too_long_first_doc_paragraph | **PASS** | `#[allow(clippy::too_long_first_doc_paragraph)]` present at loader.rs:1066 with justification comment (lines 1064–1065: "first paragraph intentionally lists the purpose in full for single-reading callers of this helper; splitting would reduce clarity"). `cargo clippy -p nexus-orchestration -p nexus-local-db -- -D warnings` exits clean (0 errors). |
| qc1-W-01 | KNOWN_TOP_LEVEL_KEYS maintenance trap | **NOT IN SCOPE** (deferred to V1.44+) | Prior recommendation explicitly deferred this to V1.44+; fix wave did not address it, which is expected. |

### Static checks (re-run on full P-last feature scope a693752b..016832f1)
- `cargo +nightly fmt --all --check`: **PASS** (no output)
- `cargo clippy -p nexus-orchestration -p nexus-local-db -- -D warnings`: **PASS** (0 errors)
- Test counts: orchestration 560 passed, 0 failed; local-db 187 passed, 0 failed

### Updated verdict
**Verdict**: **Approve**
**Rationale**: The sole blocking Critical (C-01) is resolved — the `#[allow(clippy::too_long_first_doc_paragraph)]` annotation is present with an adequate justification comment, and clippy is clean on both touched crates. W-01 remains deferred per the prior recommendation and is not in scope for this fix wave. All static checks pass (fmt, clippy, tests). No new findings from the fix wave diff.
