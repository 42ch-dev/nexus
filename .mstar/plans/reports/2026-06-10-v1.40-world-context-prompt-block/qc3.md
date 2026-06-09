---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-10-v1.40-world-context-prompt-block"
verdict: "Request Changes"
generated_at: "2026-06-10"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: kimi-for-coding/k2p6
- Review Perspective: performance and reliability risk
- Report Timestamp: 2026-06-10T00:00:00Z

## Scope
- plan_id: 2026-06-10-v1.40-world-context-prompt-block
- Review range / Diff basis: iteration/v1.40..feature/v1.40-world-context-prompt-block (equivalently 9a795624..5ba65359)
- Working branch (verified): feature/v1.40-world-context-prompt-block
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 8 changed files (728-line new module + preset/template wiring + lib/moment refactor)
- Commit range: 9a795624..5ba65359
- Tools run:
  - `cargo +nightly fmt --all -- --check`
  - `cargo clippy -p nexus-moment-context-assembly -- -D warnings`
  - `cargo clippy -p nexus-orchestration -- -D warnings`
  - `cargo test -p nexus-moment-context-assembly --lib`
  - `cargo test -p nexus-moment-context-assembly` (integration test compile path)
  - `cargo test -p nexus-orchestration`
  - `cargo test -p nexus-orchestration --test e2e_novel_writing`
  - baseline regression check via `git worktree` on `iteration/v1.40`

## Findings

### 🔴 Critical

- **C-1: `preset.input.world_kb_block` is referenced by `novel-writing` v7 but never populated, causing deterministic strict-mode template failures at run time.**  
  The preset wiring added in `crates/nexus-orchestration/embedded-presets/novel-writing/preset.yaml:91` and `:113` passes `world_kb_block: "{{preset.input.world_kb_block}}"` to `creator.inject_prompt`. However, `crates/nexus-orchestration/src/stage_gates.rs:92-150` (`build_preset_input`) and `:240` (`build_schedule_for_stage`) do not insert a `world_kb_block` key, and no other call site in the diff calls `build_chapter_kb_block`.  
  Because `render_value_templates` in `crates/nexus-orchestration/src/tasks/mod.rs:1337-1347` uses strict Handlebars mode (`render_strict_template` at `:1319-1323`), the missing variable becomes a hard runtime error.  
  Evidence: `cargo test -p nexus-orchestration --test e2e_novel_writing` fails 4/11 tests on the feature branch with `Failed to access variable in strict mode Some("preset.input.world_kb_block")`; the same suite passes on baseline `iteration/v1.40` (`9a795624`).  
  -> {fix}: Close the loop in `stage_gates.rs` (or the daemon schedule admission path) so that `novel-writing` produce-stage schedules look up the Work's `world_id`, gather the current chapter's `world_refs`, call `build_chapter_kb_block`, and place the resulting `to_yaml()` string into `preset.input.world_kb_block` (defaulting to `""` for worldless Works). Add an orchestration-level test that renders a non-empty `## World Context` section into `outline-chapter` / `draft-chapter` prompts.

### 🟡 Warning

- **W-1: `runtime_compatibility` integration test does not compile without the `cloud-stage` feature, blocking the crate-level test gate.**  
  `crates/nexus-moment-context-assembly/tests/runtime_compatibility.rs:8` imports `nexus_moment_context_assembly::cloud_stage`, which is gated by `#[cfg(feature = "cloud-stage")]` in `src/lib.rs:33-34`. Running `cargo test -p nexus-moment-context-assembly` therefore fails at compile time on both the feature branch and baseline `iteration/v1.40`.  
  -> {fix}: Gate the integration test module with `#[cfg(feature = "cloud-stage")]` (or add a `required-features = ["cloud-stage"]` entry in `Cargo.toml` for the test target).

- **W-2: Per-prompt KB queries perform linear scans with no indexes, adding measurable latency on large worlds.**  
  `crates/nexus-kb/src/store.rs:276-316` (`InMemoryKbStore::query`) iterates all blocks for every query. `world_context.rs:226-320` issues at least three such scans per chapter (Characters, Scenes, active-rules `query_all`), and `resolve_items_by_refs` (`:287-298`) adds two additional scans per `world_ref`. `SqliteKbStore::query` (`crates/nexus-local-db/src/kb_store.rs:346-360`) similarly loads the entire world into memory and filters in Rust. There is no `(world_id, block_type)` or `canonical_name` index.  
  -> {fix}: Add a targeted index in `SqliteKbStore` (e.g., a SQL query with `WHERE world_id = ? AND block_type = ?`) and, for `InMemoryKbStore`, maintain auxiliary `HashMap<(world_id, block_type), ...>` indexes so that the per-prompt path is O(log n) or O(1) instead of O(world_size).

- **W-3: Token-budget truncation re-renders the full YAML after every popped item, giving O(n²) worst-case behavior and unreliable enforcement when the static header exceeds the budget.**  
  `crates/nexus-moment-context-assembly/src/world_context.rs:325-349` calls `block.to_yaml()` and `yaml.chars().count()` inside each `while` iteration. For a block with many items, each pop rebuilds and re-counts the entire string. Furthermore, the loops only truncate `locations_referenced`, `characters_in_chapter`, and `active_rules`; the static header (`world_id`, `world_name`, `current_timeline`) is never truncated. If the header itself is longer than `max_chars`, the function exits with `truncated = true` but the YAML still exceeds the budget.  
  -> {fix}: (1) Compute per-section character budgets up-front and truncate vectors in a single pass; (2) account for the header length before item truncation; and (3) consider capping or truncating `world_name` / `current_timeline` when the header alone exceeds the allowance.

- **W-4: `WorldContextBlock::to_yaml` output order depends on `HashMap` iteration, so `{{world_kb_block}}` rendering is not deterministic across runs when using `InMemoryKbStore`.**  
  `build_chapter_kb_block` (`world_context.rs:226-277`) collects items directly from `store.query(&query).await?.items`. `InMemoryKbStore::query` (`nexus-kb/src/store.rs:280-316`) collects from `HashMap::values()`, whose order is randomized by `RandomState`. Although `SqliteKbStore` orders by `created_at ASC`, the application-layer code never sorts the final `characters_in_chapter`, `locations_referenced`, or `active_rules` vectors, so prompt content can shift between runs.  
  -> {fix}: Sort each vector by a stable key (e.g., `canonical_name` or `key_block_id`) before rendering so that identical KB state yields bit-for-bit identical prompts.

### 🟢 Suggestion

- **S-1: Use a proper YAML serializer instead of Rust `Debug` string escaping for prompt safety and spec compliance.**  
  `world_context.rs:85-95` emits `world_name: {:?}` / `name: {:?}` / `descriptor: {:?}`. While deterministic, `Debug` escaping is not a robust YAML escaping strategy (e.g., control characters, non-ASCII, and backslashes produce Rust-specific escape sequences).  
  -> {improvement}: Serialize the block through `serde_yaml` or a small YAML quoting helper so the emitted block is guaranteed valid YAML.

- **S-2: Add observability around prompt-time KB block assembly.**  
  The new module contains no `tracing` spans or events. If chapter prompts become slow, there is no structured signal showing how much time is spent in KB queries versus YAML formatting.  
  -> {improvement}: Add a `tracing::debug!` or `#[tracing::instrument]` span on `build_chapter_kb_block` that records `world_id`, item counts, and token budget outcome.

- **S-3: Cap unbounded rule retrieval with a default `kb_limit`.**  
  `resolve_active_rules` (`world_context.rs:304-320`) calls `builder.query_all()` without a limit. A world with many foundation/rule entries can blow through the token budget before truncation even begins.  
  -> {improvement}: Pass a default limit (e.g., 50 or 100) to the active-rules query and sort by relevance or recency so the most important rules are included first.

- **S-4: Consider caching the rendered block per chapter/version to avoid repeated KB scans across retries.**  
  Because the block is assembled fresh for every prompt, repeated outline/draft cycles for the same chapter re-run the same KB scans.  
  -> {improvement}: Cache the rendered block keyed by `(world_id, chapter, world_refs_hash, kb_version)` with a short TTL or invalidation on `KeyBlock` write.

## Source Trace

- **F-001 (C-1)**: `git diff iteration/v1.40..feature/v1.40-world-context-prompt-block -- crates/nexus-orchestration/embedded-presets/novel-writing/preset.yaml` introduces `world_kb_block: "{{preset.input.world_kb_block}}"` at lines 91 and 113. `crates/nexus-orchestration/src/stage_gates.rs:92-150` (`build_preset_input`) does not emit that key. `cargo test -p nexus-orchestration --test e2e_novel_writing` fails with strict-mode variable error. Baseline `iteration/v1.40` passes the same suite.
- **F-002 (W-1)**: `cargo test -p nexus-moment-context-assembly` on both feature branch and baseline produces `error[E0432]: unresolved import nexus_moment_context_assembly::cloud_stage` at `tests/runtime_compatibility.rs:8`.
- **F-003 (W-2)**: `crates/nexus-kb/src/store.rs:276-316` and `crates/nexus-local-db/src/kb_store.rs:346-360` show scan-and-filter implementations; no `(world_id, block_type)` or `canonical_name` index is present.
- **F-004 (W-3)**: `crates/nexus-moment-context-assembly/src/world_context.rs:325-349` calls `block.to_yaml()` and `yaml.chars().count()` inside three nested `while` loops; static header is not truncated.
- **F-005 (W-4)**: `crates/nexus-kb/src/store.rs:133` stores blocks in `HashMap<String, KeyBlock>` with default `RandomState`; `query` returns items in HashMap iteration order. `world_context.rs:226-277` does not sort the vectors before rendering.
- **F-006 (S-1)**: `world_context.rs:85-95` uses `format!("... {:?}", ...)` for YAML string values.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 1 |
| 🟡 Warning | 4 |
| 🟢 Suggestion | 4 |

**Verdict**: Request Changes

Rationale: the new `world_context.rs` builder is well-factored and its unit tests pass quickly (40 lib tests in ~0.13s), but the orchestration integration is incomplete. The `novel-writing` preset now references `preset.input.world_kb_block` in strict mode without any producer of that key, which causes a deterministic runtime failure for outline/draft tasks and regresses four `e2e_novel_writing` tests that pass on the baseline. In addition, the per-prompt query path is O(n) with multiple linear scans, token-budget truncation is O(n²) and does not reliably bound the static header, and prompt output order is nondeterministic when backed by `InMemoryKbStore`. Formatting and clippy are clean; the crate-level test gate for `nexus-moment-context-assembly` is broken by a pre-existing `cloud-stage` feature mismatch that also needs resolution.
