# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-2
- Runtime Agent ID: qc-specialist-2
- Runtime Model: xai/grok-build-0.1
- Review Perspective: Security and correctness risk (authorization preservation, ownership resolution, no silent scope downgrade, deprecation/audit visibility)
- Report Timestamp: 2026-06-19

## Scope
- plan_id: 2026-06-19-v1.52-cli-surface-consolidation-auto
- Review range / Diff basis: b97ec0d9..771f89e7
- Working branch (verified): feature/v1.52-cli-surface-consolidation-auto
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.52-ta-p1/
- Files reviewed: 3 (crates/nexus42/src/commands/creator/kb.rs, crates/nexus42/src/commands/creator/world/kb.rs [header context], crates/nexus42/tests/world_kb_alias.rs)
- Commit range: 771f89e7 (single commit in range for this plan)
- Tools run:
  - git diff b97ec0d9..771f89e7 (target files)
  - cargo test -p nexus42 --test world_kb_alias -- --nocapture (6/6 passed)
  - cargo test -p nexus42 --test creator_world_kb (3/3 regression passed)
  - cargo clippy --all -- -D warnings (clean)
  - Manual security/correctness review per assignment checklist

## Findings

### 🔴 Critical
(none)

### 🟡 Warning
(none)

### 🟢 Suggestion
- S-001: `search` and `add` (World scope) remain on legacy inline path (`open_world_kb_store`) and emit only the deprecation warning. This is explicitly documented in the plan (no canonical equivalent yet) and in cli-spec §6.2G.2. Consider adding a note in the deprecation message that these two subcommands have no 1:1 canonical yet (low priority; removal still V1.53).
- S-002: No dedicated audit event beyond the deprecation `tracing::warn!` + `eprintln!`. If PM needs to measure legacy surface usage volume separately from general deprecation noise, a distinct structured log field (e.g., `legacy_surface: true`) could be added before V1.53. Current observability via the warning text is sufficient for migration tracking.
- S-003: The alias test `creator_world_kb_adopt_help_is_reachable` prints a NOTE when `--auto` is absent. This is expected (T-A P0 pending merge into the iteration). Once T-A P0 lands, the test can be strengthened to assert `--auto` presence.

## Source Trace
- Finding ID: QC2-2026-06-19-R-V150KBED-01
- Source Type: git-diff + hermetic test + manual authorization walk
- Source Reference: crates/nexus42/src/commands/creator/kb.rs:448-797 (World branches + forwarding); world/kb.rs:352-383 (kb_delete + require_world_owner); tests/world_kb_alias.rs:140-170 (cross-author reject + hermetic parity)
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 3 |

**Verdict**: Approve

## Detailed Security & Correctness Review (per assignment)

### 1. Authorization preservation
- `creator kb --scope world remove` now forwards to `world::kb::kb_delete(&pool, &cid, &wid, entry_id, true)`.
- `kb_delete` (canonical) calls `require_world_owner(pool, world_id, creator_id)` which queries `narrative_worlds.owner_creator_id`.
- Legacy inline path had **no** owner gate. Forwarding **adds** the correct `403 WORLD_KB_FORBIDDEN` gate. This is a security improvement, not a regression.
- Cross-author test (`canonical_kb_delete_cross_author_rejects`) explicitly asserts the 403 path.
- Edge cases covered: `require_world_id` rejects missing `--world-id` before any DB access; `CreatorNotSelected` error is raised for missing active creator.

### 2. World ownership resolution
- Consistent with canonical surface: `narrative_worlds.owner_creator_id` (documented in world/kb.rs header lines 15-18).
- R-V150KBED-02 (world vs work ownership narrative) is acknowledged in plan as T-A P2 follow-up; current resolution is **correct per the canonical implementation**.

### 3. No silent scope downgrade
- `--scope world` is explicitly checked (`if scope == &KbScope::World`) before forwarding.
- Work-scope operations are untouched.
- The alias forces world scope when `--scope world` is supplied; no narrowing occurs.

### 4. Deprecation warning visibility
- `deprecation_notice_legacy_world_kb(subcmd)` emits:
  - `tracing::warn!("{}", msg);`
  - `eprintln!("nexus42: {msg}");`
- Message includes canonical form and planned removal V1.53.
- Unit test + integration test verify the string format.
- Both log and interactive channels are covered.

### 5. Audit trail
- Every legacy world invocation emits the structured warning via tracing. PM can filter on the deprecation text or add a dedicated field later.
- No other side-effect (no DB write of "legacy used").
- Sufficient for migration tracking per plan acceptance.

### 6. No behavior change (forwarding paths)
- `kb_list` → `world::kb::kb_list(&pool, &wid, false)` — identical output format.
- `kb_show` → `world::kb::kb_show(&pool, &wid, entry_id, false)` — identical output + fields.
- `kb_remove` → `world::kb::kb_delete(..., true)` — forces non-interactive (alias contract); prints "✓ Key block deleted" on success.
- `search`/`add` keep prior inline behavior + warning (documented as "no canonical equivalent").
- Exit codes, error messages, and JSON paths (where applicable) are preserved via the hermetic functions.

### 7. `--auto` flag interaction
- T-A P1 scope is alias wiring only. `--auto` is a T-A P0 deliverable on the canonical surface.
- Test `creator_world_kb_adopt_help_is_reachable` documents the forward-compatibility note.
- No alias for `--auto` is required in this plan; the canonical path will be used once T-A P0 merges.

### 8. Concurrency
- Both legacy and canonical now call the same hermetic functions (`kb_list`, `kb_show`, `kb_delete`) against the same `SqlitePool`.
- No new races introduced. Existing atomicity (e.g., adopt transaction + CAS in world/kb.rs) is unchanged.
- Simultaneous legacy + canonical calls see consistent data.

### 9. Spec overlay (cli-spec.md §6.2G.2)
- Diff review confirms:
  - Deprecation message text matches implementation.
  - Forwarding table (list/show/remove) matches code.
  - Explicit call-out that `remove` now adds the auth gate (correct behavior).
  - Planned removal V1.53.
  - Security model (world ownership via `require_world_owner`) is referenced by cross-link to entity-scope-model.
- Overlay is accurate and complete for this change.

## Verification Evidence
- `cargo test -p nexus42 --test world_kb_alias -- --nocapture`: 6/6 passed (CLI help docs + hermetic list/show/delete/cross-author).
- `cargo test -p nexus42 --test creator_world_kb`: 3/3 passed (regression on canonical surface).
- `cargo clippy --all -- -D warnings`: clean.
- Checkout alignment verified:
  ```
  /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.52-ta-p1
  feature/v1.52-cli-surface-consolidation-auto
  771f89e710d7a2d8c908d22e4fef252dc13a5d54
  ```
- Review range matches Assignment exactly.

## Residuals (for PM)
- None blocking. Suggestions S-001/S-002 are migration/observability hygiene for V1.53; S-003 is T-A P0 test strengthening.
- No new `Critical` or mandatory `Warning` to register in `status.json`.

---

**Verdict**: Approve
