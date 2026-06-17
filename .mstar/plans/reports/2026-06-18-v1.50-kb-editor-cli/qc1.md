---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-18-v1.50-kb-editor-cli"
working_branch: "feature/v1.50-kb-editor-cli"
review_cwd: "/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-kb-editor-cli"
review_range: "merge-base c38fbe1f264b9574b25355d872d20138c1c04e77..3132f80e740297ef6f79009f1c804fb68dcb95ea"
verdict: "Approve"
generated_at: "2026-06-17T05:56:10Z"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: zhipuai-coding-plan/glm-5.2
- Review Perspective: Architecture coherence and maintainability risk (Reviewer #1)
- Report Timestamp: 2026-06-17T05:56:10Z

## Scope
- plan_id: `2026-06-18-v1.50-kb-editor-cli`
- Review range / Diff basis: `merge-base c38fbe1f264b9574b25355d872d20138c1c04e77..3132f80e740297ef6f79009f1c804fb68dcb95ea`
- Working branch (verified): `feature/v1.50-kb-editor-cli`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v150-kb-editor-cli` (from `git rev-parse --show-toplevel`)
- Files reviewed: 5 (1 new module, 1 rename, 2 new test files, 1 plan markdown)
- Commit range: `c38fbe1f..3132f80e` — `ec201b40` (T1/T2), `de0c9d29` (T3/T4/T5), `429b7101` (T6 tests), `3132f80e` (plan done + completion report)
- Tools run: `git diff`/`git show`/rename detection, `cargo build -p nexus42 -p nexus-kb`, `cargo clippy -p nexus42 -p nexus-kb -- -D warnings`, `cargo test -p nexus42 --test world_kb_cli`, `cargo test -p nexus42 --test world_kb_authz`

## Reviewer Perspective Notes (architecture coherence + maintainability)

### Module restructure `world.rs` → `world/mod.rs` (R-V150KBED-01 — accept, verified)
The split is **not scope creep**; it is the minimal mechanical prerequisite for
adding a `kb` submodule as a sibling of the existing flat `world.rs`. Git rename
detection reports **93% similarity** (`world.rs` → `world/mod.rs`, history
preserved). The only changes to the pre-existing file are:

1. Module-doc update (added World KB paragraph + `pub mod kb;`).
2. New `WorldCommand::Kb { command }` enum variant + dispatch arm
   `WorldCommand::Kb { command } => kb::run(command, config).await`.

The existing `Create` / `EventAdd` / `List` / `Show` command bodies, helpers
(`slug_from_title`, `run_create`, `run_event_add`, `run_list`, `run_show`), and
the raw `narrative_worlds` SELECT in `event-add` (with its SAFETY comment) are
**byte-identical** to the pre-split file. This is the canonical way to grow a
clap subcommand group for a command that was a flat file, and it keeps the
existing call sites untouched. Surgical.

The helper functions `open_workspace_pool` and `active_creator_id` were
**reused** (not duplicated) by `kb.rs` via `super::…` and deliberately kept at
their original private `fn` visibility — Rust child-module visibility permits
this, so no signature/visibility change was needed. Good.

### Reuse of V1.40 KB API surface — no parallel handwritten DTOs (verified)
Confirmed single-sourced in `nexus-kb`:

| Type | Source | Reused in `kb.rs` |
|------|--------|-------------------|
| `KeyBlock`, `KeyBlockBody` | `nexus-kb::key_block` | ✅ |
| `ValidationMode` | `nexus-kb::validation` | ✅ (`ValidationMode::Novel`) |
| `KbStore` trait, `KbStoreError` | `nexus-kb::store` / `nexus-kb` | ✅ |
| `SqliteKbStore` impl | `nexus-local-db::kb_store` | ✅ (`new`, `with_validation_mode`, `list_by_world`, `get_key_block`, `update_key_block`, `delete_key_block`) |

No parallel DTOs introduced in `nexus42`. The `--json` serialization reuses
`serde_json::to_value(&KeyBlock)` directly. The `KeyBlockBody` parse from
`--body` JSON deserializes straight into the shared type, which is why
`ValidationMode::Novel` validation then runs through the same store path on
`update_key_block`. Clean.

### `creator world kb` subcommand group fit (verified)
`WorldCommand::Kb` slots into the existing enum alongside `Create` / `EventAdd`
/ `List` / `Show` and follows the identical dispatch shape
(`Variant { .. } => path::run(...).await`). The `creator world` parent already
establishes the "world-scoped operation" grouping, so `kb` as a nested
subcommand group is a natural, precedent-consistent fit. No surprising
top-level surface.

### Author identity gate — `owner_creator_id` vs `works.creator_id` (R-V150KBED-02 — accept, verified)
The plan/assignment phrased the author check as "`works.creator_id` match".
`KeyBlock`s are **World-scoped** (entity-scope-model §1.2 / §5.1 / §5.5) and
carry **no** `works` FK on `kb_key_blocks`. The only coherent ownership path is
`narrative_worlds.owner_creator_id`. The implementation gates edit/delete on
**world ownership** and documents the reconciliation in both the module doc and
the Completion Report with explicit spec citations.

The resolution is sound: the raw `SELECT owner_creator_id FROM narrative_worlds
WHERE world_id = ?` in `require_world_owner` mirrors the existing `event-add`
pattern (raw `narrative_worlds` SELECT + SAFETY comment), so the new code is
consistent with the established local-query convention in this file. The
forward-looking note (revisit if a Work→KeyBlock provenance linkage is
introduced) is appropriate and tracked in R-V150KBED-02.

### Surgical changes
Only the expected paths are touched: `crates/nexus42/src/commands/creator/world/`
(new `kb.rs` + renamed `mod.rs`), two new hermetic test files, and the plan
markdown. No piggybacked refactors, no unrelated formatting churn, no behavior
change to existing commands.

## Findings
### 🔴 Critical
- _(none)_

### 🟡 Warning
- _(none)_

### 🟢 Suggestion
- **S-V150KBED-QC1-01** (low) — **Plan "Code touch" references a non-existent
  file.** The plan's §"Code touch" lists `crates/nexus-kb/src/api.rs` as the
  read API surface to reuse, but `nexus-kb/src/api.rs` does **not exist** in
  this repo. The implementation correctly reuses the public re-exports from
  `nexus-kb/src/lib.rs` (`KbStore`, `KeyBlock`, `KeyBlockBody`, `ValidationMode`,
  `KbStoreError`) and the `SqliteKbStore` impl from `nexus-local-db::kb_store`.
  → Amend the plan's "Code touch" line to reference `crates/nexus-kb/src/lib.rs`
  (re-exports) + `crates/nexus-local-db/src/kb_store.rs` (`SqliteKbStore`) so
  the plan matches the shipped code. Code is correct; this is a doc/plan
  reconciliation only.

- **S-V150KBED-QC1-02** (low) — **`world_ref` positional arg name implies
  flexible resolution but only world IDs are accepted.** The new `list`/`show`/
  `edit`/`delete` commands take a positional `world_ref: String`, and the
  arg doc reads "World reference — the world ID (e.g. `wld_abc123`)". There is
  no ref-resolution logic: the value is passed straight through as `world_id`
  to the store and to `require_world_owner`. The legacy `creator kb --scope
  world` uses `--world-id`, and `creator world show` uses positional
  `world_id`. The `_ref` suffix is aspirational but no slug/handle resolution
  exists. → Either (a) rename to `world_id` to match `creator world show` /
  legacy `--world-id` semantics, or (b) document that only IDs are accepted
  for now and track slug/handle resolution as a follow-up. Pre-release (<1.0)
  CLI, so a rename is low-cost. Non-blocking.

- **S-V150KBED-QC1-03** (nit) — **`map_kb_store_error` verb is `"show"` inside
  `kb_edit` / `kb_delete` (copy-paste).** In `kb_edit` the initial
  `get_key_block` load (kb.rs:178) and in `kb_delete` the pre-check
  `get_key_block` load (kb.rs:237) both call
  `map_kb_store_error("show", block_id, world_id, e)`, so a load failure during
  `edit`/`delete` surfaces as "Failed to **show** key block …" — slightly
  misleading. (The `update` / `delete` calls at kb.rs:253 / kb.rs:301 correctly
  use `"update"` / `"delete"`; kb.rs:290 in `kb_show` is correct.) → Pass a
  load-appropriate verb like `"load"` for these two call sites. Cosmetic; error
  still surfaces correctly.

## Source Trace
- Finding ID: S-V150KBED-QC1-01
  - Source Type: manual-reasoning / doc-rule
  - Source Reference: `git diff … -- crates/nexus-kb/src/` (no `api.rs`); plan
    `.mstar/plans/2026-06-18-v1.50-kb-editor-cli.md` "Code touch" bullet;
    `rg "^pub" crates/nexus-kb/src/lib.rs` (re-exports).
  - Confidence: High
- Finding ID: S-V150KBED-QC1-02
  - Source Type: manual-reasoning
  - Source Reference: `crates/nexus42/src/commands/creator/world/kb.rs`
    `WorldKbCommand::List { world_ref, .. }` + doc; legacy
    `crates/nexus42/src/commands/creator/kb.rs` `--world-id`.
  - Confidence: High
- Finding ID: S-V150KBED-QC1-03
  - Source Type: git-diff / manual-reasoning
  - Source Reference: `crates/nexus42/src/commands/creator/world/kb.rs:178,237`
    (verb `"show"` inside `kb_edit`/`kb_delete` load step).
  - Confidence: High

## Validation Evidence
```
# Build (scoped)
$ cargo build -p nexus42 -p nexus-kb
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.23s

# Clippy (scoped, workspace pedantic+nursery inherited)
$ cargo clippy -p nexus42 -p nexus-kb -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.26s   # 0 warnings

# AC4 + AC5 + round-trip
$ cargo test -p nexus42 --test world_kb_cli
    test result: ok. 9 passed; 0 failed; 0 ignored

# AC2 (cross-author 403 + stable code + no mutation)
$ cargo test -p nexus42 --test world_kb_authz
    test result: ok. 4 passed; 0 failed; 0 ignored
```

Rename provenance (surgical split, history preserved):
```
$ git diff c38fbe1f..3132f80e --diff-filter=R -M -- 'crates/nexus42/src/commands/creator/'
    similarity index 93%
    rename from crates/nexus42/src/commands/creator/world.rs
    rename to crates/nexus42/src/commands/creator/world/mod.rs
```

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 3 (2 low, 1 nit) |

**Verdict**: **Approve**

No Critical or Warning findings. The three Suggestions are low/nit
non-blocking items (one plan-doc reconciliation, two minor CLI/error-message
polish) appropriate for residual tracking or a small follow-up. The module
restructure is justified and surgical (git rename preserved, 93% similarity,
existing commands byte-identical); the V1.40 KB API surface is correctly reused
with no parallel DTOs; the `creator world kb` group fits the existing
`creator world` precedent; and the `owner_creator_id` author-gate
reconciliation (R-V150KBED-02) is spec-correct and well-documented. All build /
clippy / test gates are green.

### Pre-existing accepted residuals (verified sound, no change)
- **R-V150KBED-01** (low, accept) — `creator world kb` (canonical author
  surface) coexists with legacy `creator kb --scope world` (ingest path). Both
  reach `SqliteKbStore`; consolidation deferred. Verified: the two surfaces are
  intentionally separate and documented; non-blocking.
- **R-V150KBED-02** (low, accept) — Author identity uses world ownership
  (`owner_creator_id`) because `KeyBlock`s are World-scoped with no `works` FK.
  Verified: the implementation + doc + completion report cite
  entity-scope-model §1.2/§5.1 and the resolution is the only coherent path.

### New residuals suggested for PM registration
- **S-V150KBED-QC1-01** (severity `low`) — plan "Code touch" references
  non-existent `nexus-kb/src/api.rs`; amend to `lib.rs` + `nexus-local-db::kb_store`.
- **S-V150KBED-QC1-02** (severity `low`) — `world_ref` arg name vs ID-only
  semantics; rename or document.
- **S-V150KBED-QC1-03** (severity `nit`) — `map_kb_store_error("show", …)`
  verb in edit/delete load steps; cosmetic.
