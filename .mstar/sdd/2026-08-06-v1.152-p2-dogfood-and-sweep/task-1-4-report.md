# Task 1-4 Report — Dogfood round-trip + sweep (micro-batch)

## Status

**COMPLETE** — T1–T4 delivered on `plan/v1.152-p2-dogfood-and-sweep`.

## Implemented

### T1 — Dogfood round-trip test

Added `dogfood_pack_round_trip_preserves_activation_and_relations` in `apps/nexus42/src/commands/creator/world/kb/pack.rs::tests`:

- Seeds World A with three `Character` entries carrying `modules.activation` (`alice`/`bob`/`carol` keys) plus Alice→Bob relation.
- Exports via `export_to_file_custom_world`, imports into fresh World B with `ConflictPolicy::Skip`.
- Asserts `entries.created == 3`, `relations.created >= 1`, `pack_import` provenance on all B entries, `modules` deep-equal A→B per canonical name.
- Re-import under `skip` asserts `entries.created == 0` and `relations.created == 0` (idempotency).

### T2 — Spec §11 verification

Verified `.mstar/specs/spoke-adapter-architecture.md` §11 against shipped code:

- Daemon routes match `world_kb_pack.rs`: `POST /v1/daemon/worlds/:world_id/kb/pack/export|import`.
- Conflict policies, ownership guard, provenance, and shared `import_pack` home confirmed.
- **Drift fixes applied:** rename suffix documented as ` imported` / ` imported N` (not parenthetical `(imported)`); added create-path `revision` normalization note (`prepare_create_entry` clears revision before create); status line → **shipped P0+P1; P2 dogfood-confirmed**; §11.8 references the actual dogfood test name.

### T3 — CONCEPTS + tracker + iterations README

- **CONCEPTS.md:** `### Knowledge Pack` entry under Creative Writing Domain (portable transport, provenance, consumer-only, conflict policies, cross-refs).
- **DF tracker:** DF-77 moved to shipped archive; quick status → V1.152 shipped; FL-L W4–W7 all delivered.
- **iterations README:** V1.152 row → `completed`.

### T4 — Residual assessment

**0 new P2 residuals.** P0 registered carry-forwards (R-V1152P0-001/002/003) remain in plan/status SSOT — no new deferrals introduced by this micro-batch. Dogfood green; no behavior changes.

## Test evidence

```text
cargo test -p nexus42 --lib pack
  → 35 passed (includes dogfood_pack_round_trip_preserves_activation_and_relations)

cargo check --all
  → ok

pnpm typecheck
  → ok
```

## Files changed

| File | Change |
|------|--------|
| `apps/nexus42/src/commands/creator/world/kb/pack.rs` | T1 dogfood test |
| `.mstar/specs/spoke-adapter-architecture.md` | T2 §11 verify + drift fixes |
| `CONCEPTS.md` | T3 Knowledge Pack entry |
| `.mstar/knowledge/deferred-features-cross-version-tracker.md` | T3 DF-77 delivered |
| `.mstar/knowledge/shipped-features-tracker.md` | T3 DF-77 archive row |
| `.mstar/iterations/README.md` | T3 V1.152 completed |
| `.mstar/sdd/2026-08-06-v1.152-p2-dogfood-and-sweep/task-1-4-report.md` | This report |

## Self-review

- Dogfood test combines cross-world import, activation preservation, relations, and skip-idempotency in one integration test — complements existing policy-specific tests without duplicating rename/overwrite matrices.
- Spec §11 now matches shipped rename suffix and revision normalization; no code behavior changed in this batch.
- Tracker/README/CONCEPTS aligned with P2 closeout; optional ST lorebook importer remains noted on DF-77 shipped row as backlog slice.
