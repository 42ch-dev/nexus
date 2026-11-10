---
module: crates/nexus-spoke-adapter/src/pack/st_lorebook.rs
date: 2026-09-02
problem_type: best_practice
category: best-practices
severity: medium
plan_id: 2026-09-02-v1.181-p1-lore-hygiene-and-pack-import
tags:
  - clean-room
  - importers
  - stable-ids
  - overwrite-anchor
  - conflict-policy
  - diagnostics
  - external-formats
applies_when:
  - "Converting third-party/external format records into store rows with ids used as conflict/overwrite anchors"
  - "Building a clean-room importer for an external tool's file format"
  - "Choosing between positional vs content-derived ids for re-importable artifacts"
  - "Deciding what a converter should do with foreign optional fields and enum-shaped values"
---

# Clean-Room Importer: Stable IDs, Honest Diagnostics, No Silent Drops

**Track**: Knowledge (durable guidance, distilled from v1.181 P1 Task 2 + QC fix rounds).

## Context

DF-80 imported SillyTavern lorebook JSON into nexus Knowledge Packs (clean-room: format knowledge from public docs, never ST source). The converter inserts before `parse_pack`, so converted entries ride the existing `import_pack` conflict machinery (skip/rename/overwrite), where **entry_id is the overwrite anchor**. Three classes of defect surfaced in QC, all sharing one root: *the converter made silent choices on the user's behalf*.

1. **Positional ids** (`kb_st_{idx}`) are unstable across lorebook edits — insert an entry mid-file and every later id shifts, so a re-import under Overwrite cascades wrong-entry overwrites and under Skip silently drops edits.
2. **Foreign semantics dropped silently**: ST `enabled: false` entries imported as live `confirmed` content; the documented `key` array form yielded no activation keys — both without diagnostics.
3. **Collision-blind fallbacks**: deriving fallback ids without checking a claimed-id set could emit duplicate ids (two fallback branches discarded the `seen.insert` result), resurrecting the overwrite-anchor hazard even on first import.

## Guidance

1. **Derive ids from format-intrinsic stable fields** (ST `uid`/`id`) before any positional fallback; sanitize into the id namespace. Positional fallback is allowed only as last resort, and **every id claim must go through one unique-claim helper** (check the seen-set insert result; suffix `_1/_2/…` on collision). Uniqueness-by-construction at a single derivation point beats post-hoc duplicate detection.
2. **A converter never silently maps, drops, or invents**: every unmappable/documented-but-unhandled shape (array-form `key`, `enabled: false`, wrong JSON types, unknown extra fields) emits a typed `ConversionDiagnostic`; the CLI prints diagnostics BEFORE the import summary (also under `--dry-run`). Foreign semantics need an explicit mapping decision with a documented rationale (e.g. no disabled state exists in the entry lifecycle → diagnostic-only, content preserved).
3. **File-level malformation aborts before any write** (no partial import); per-entry problems continue with diagnostics — the two-tier error taxonomy keeps single bad entries from blocking a bulk import without letting corruption through.
4. **Test the id-stability property, not just the mapping**: assert the id set of *unchanged* entries is identical when the lorebook gains an entry in the middle; and run the exact collision repro as a regression test.
5. **Clean-room discipline**: fixtures hand-written from the public documentation; never fetch/read the external tool's source; tolerant parsing is for documented variance, not for undocumented reverse-engineered behavior.

## Why This Matters

The overwrite anchor makes id choice a *data-integrity* decision, not a naming detail: an unstable id silently corrupts user content on a routine re-import, and a silent semantic drop (enabled→confirmed) publishes content the author explicitly disabled. Both failures are invisible until a user loses data — the converter is the only place to catch them.

## When to Apply

- Any importer/adapter converting external formats into stores with conflict/overwrite semantics (pack imports, sync bundles, external reference refresh).
- Any id-derivation scheme with positional or fallback components.
- Review checklists for converters: "which foreign fields have no mapping, and does each produce a diagnostic?" / "are ids stable across plausible edits of the source file?" / "does the fallback claim path check the seen set?"