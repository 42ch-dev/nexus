---
report_kind: qc
plan_id: "2026-06-10-v1.40-architecture-hygiene"
reviewer: qc-specialist-3
reviewer_index: 3
focus: performance and reliability risk
verdict: Approve
generated_at: "2026-06-10"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: kimi-for-coding/k2p6
- Review Perspective: performance and reliability risk
- Report Timestamp: 2026-06-10T06:45:00Z

## Scope
- plan_id: 2026-06-10-v1.40-architecture-hygiene
- Review range / Diff basis: iteration/v1.40..feature/v1.40-architecture-hygiene (equivalently commits 3c90c18f..dc7f81e7)
- Working branch (verified): feature/v1.40-architecture-hygiene
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 6
- Commit range: 3c90c18f..dc7f81e7 (5 commits: T1 add module + relocate, T2 refactor reader, T4 remove old dir, T5 doc path corrections, plus qc2 report)
- Tools run: git diff, git show, git log, read (embedded_rules.rs, stage_gates.rs, lib.rs, AGENTS.md, world-kb-runtime-architecture.md), grep (read_embedded_template callers, embedded-presets/rules stale refs), cargo check -p nexus-orchestration, cargo test -p nexus-orchestration --lib, cargo clippy -p nexus-orchestration -- -D warnings, cargo +nightly fmt --all --check, cargo test -p nexus-orchestration --lib embedded_rules, cargo test -p nexus-orchestration --lib read_rules_layers, wc -c (file size comparison)

## Findings
### 🔴 Critical
- (none)

### 🟡 Warning
- **W-1**: `crates/nexus-orchestration/src/embedded_rules.rs:21-22` not formatted with nightly `cargo fmt`. The `pub const` declaration is split across two lines (`&str =` + newline + `include_str!(...)`), but nightly rustfmt collapses it to a single line. Per project `AGENTS.md`, `cargo fmt` must use the nightly toolchain (`cargo +nightly fmt --all`), and CI would flag this on `cargo +nightly fmt --check`. -> Run `cargo +nightly fmt -- crates/nexus-orchestration/src/embedded_rules.rs` and amend the commit.

### 🟢 Suggestion
- (none)

## Source Trace
- Finding ID: W-1
- Source Type: static-analysis (linter)
- Source Reference: `cargo +nightly fmt --all --check` output showing diff in `crates/nexus-orchestration/src/embedded_rules.rs:18-22`
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 0 |

**Verdict**: Request Changes

**Rationale (performance + reliability)**:
- `include_str!("../embedded-rules/writing-craft.md")` is a compile-time constant resolved by rustc. Zero runtime filesystem reads, zero runtime I/O, zero path traversal surface. Confirmed via source read and `grep` of `read_embedded_template` callers (function still used for preset templates, unaffected by this change).
- The `read_rules_layers()` refactor eliminates **three** sources of per-call overhead from the old implementation: (1) `format!("{preset_id}/{template_path}")` string allocation, (2) `EMBEDDED_PRESETS.get_file()` HashMap lookup, and (3) `contents_utf8().map(ToString::to_string)` UTF-8 conversion + String allocation. The new code reads a `&'static str` constant directly — the most efficient possible approach. Net result: fewer allocations, fewer indirections, same observable behavior.
- No new `LazyLock` / `OnceLock` / `RwLock` patterns introduced. `WRITING_CRAFT` is an immutable `&str` constant with no interior mutability; thread-safety is trivial.
- Binary size impact is zero: `writing-craft.md` is byte-identical (1578 bytes, SHA256 `6a9e6b196b5e951a06c3187e7abb1ba1a13cbd38485d9c62537695eae59431cd` per qc2 verification) between old `embedded-presets/rules/` and new `embedded-rules/` locations. The `.rodata` section embedding is identical.
- Incremental rebuild behavior: cargo fingerprinting tracks file content + mtime, not path. The move from `embedded-presets/rules/writing-craft.md` to `embedded-rules/writing-craft.md` is detected as a legitimate source change; no orphan artifacts or false-negative rebuilds observed. `cargo check --all` completed successfully in 13.84s.
- Test suite: 522 lib tests pass in 1.39s (no regression). The 3 `embedded_rules` unit tests and 3 `read_rules_layers` integration tests all pass in <0.01s. No tests perform real I/O for Layer 1 (hermetic by design via `include_str!`).
- `cargo clippy -p nexus-orchestration -- -D warnings` is clean.
- Doc changes in `.mstar/knowledge/world-kb-runtime-architecture.md` (T5) contain no misleading performance claims — only path corrections from `embedded-presets/rules/` to `embedded-rules/`.
- No pre-existing benchmarks in `nexus-orchestration` to re-run; the change is a pure relocation with strictly reduced runtime overhead.
- The single unresolved Warning (W-1) is a formatting standards violation that would fail CI under the project's mandated nightly `cargo fmt` toolchain.

## Revalidation

### Fix context
**W-1**: `crates/nexus-orchestration/src/embedded_rules.rs:21-22` was not formatted with nightly `cargo fmt`. The `pub const` declaration was split across two lines (`&str =` + newline + `include_str!(...)`), but nightly rustfmt collapses it to a single line. This was flagged as a Warning blocking approval.

### Diff since previous review
New commit added to `feature/v1.40-architecture-hygiene`:

```
1dd268ed style(orchestration): nightly fmt embedded_rules.rs (QC #3 fix)
```

This commit modifies exactly one file (`crates/nexus-orchestration/src/embedded_rules.rs`) with a whitespace-only change:

```diff
-pub const WRITING_CRAFT: &str =
-    include_str!("../embedded-rules/writing-craft.md");
+pub const WRITING_CRAFT: &str = include_str!("../embedded-rules/writing-craft.md");
```

**Semantic delta**: zero. No runtime behavior, API, or compile-time semantics changed. Only formatting.

### Re-verification evidence

1. **Format check (mandatory nightly)**:
   ```bash
   $ cargo +nightly fmt --all -- --check
   # (exit 0, no output — no formatting drift detected)
   ```

2. **Clippy (strict)**:
   ```bash
   $ cargo clippy -p nexus-orchestration -- -D warnings
   # Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.19s
   # (clean — zero warnings)
   ```

3. **Unit tests for `embedded_rules` module**:
   ```bash
   $ cargo test -p nexus-orchestration --lib embedded_rules
   # running 3 tests
   # test embedded_rules::tests::writing_craft_is_not_empty ... ok
   # test embedded_rules::tests::writing_craft_contains_expected_heading ... ok
   # test embedded_rules::tests::writing_craft_contains_five_question_gate ... ok
   # test result: ok. 3 passed; 0 failed; 0 ignored; 520 filtered out
   ```

4. **Diff verification**:
   ```bash
   $ git show 1dd268ed --stat
   # crates/nexus-orchestration/src/embedded_rules.rs | 3 +--
   # 1 file changed, 1 insertion(+), 2 deletions(-)
   ```

### Disposition of prior findings

| Finding | Status | Evidence |
|---------|--------|----------|
| W-1 Formatting drift in `embedded_rules.rs:21-22` | **Resolved** | `git show 1dd268ed` confirms single-line collapse; `cargo +nightly fmt --all -- --check` exits 0 |

### New findings (this pass)
- (none)

### Updated verdict
**Approve** — re-validation of QC #3. The blocking Warning (W-1) has been resolved with a whitespace-only formatting fix. No new findings. All static checks pass.
