# Legacy-Surface Cutover Pattern (provider seam with read bridge)

Promoted from: `v1.228-spoke-memory-n2` (`v228-n2-legacy-content-migration` + `v228-n2-nxm5-coreservice-reroute`); source iterations retain the full evidence under their SDD trees.

## Problem

A codebase owns an entrenched legacy storage surface (tables + bearer files) that many entry points address directly. A new provider owns a better surface (its own tables + projections) but starts with zero callers. Cutting over requires serving old content through the new provider, migrating it, and retiring the legacy read/write paths — without ever losing content or breaking the old callers mid-flight.

## Pattern (as landed in v1.227 N1 + v1.228 N2)

1. **Additive provider surface first** (N1): the provider owns its own tables and file projections; the legacy surface stays byte-identical and untouched. The provider implements the port/trait the callers will eventually use.
2. **Provider-seam read bridge**: a single module projects legacy rows/files into provider-native record views, so ONE read path serves migrated and unmigrated content. Rules that made it sound:
   - Deterministic external ids derived from legacy row identity (`lgc:<family>:<key>`), reversible and stable — the migration later writes objects under the SAME ids, so provider rows win on id collision and tombstones stay deleted.
   - One canonical bearer→subject rule (e.g. the smallest `subject_id` naming the bearer), applied identically in list and by-id paths; non-canonical subjects project nothing (list/get can never disagree).
   - A per-space completion marker (`legacy_content_migration:<space_id>`) gates the bridge: absent ⇒ merged reads; present ⇒ provider-only reads. Checked inside the caller's read transaction, including by-ID and profile fallbacks.
3. **Entry-point reroute** (N2): handlers move to the port with DTOs and error mapping preserved (D5-style). Mutations that target legacy-only objects perform **mutation-time JIT adoption**: adopt the projected row under its deterministic id inside the SAME guarded transaction as the mutation. Read-time adoption is deliberately excluded while a legacy drain still consumes the source rows (it would orphan them).
4. **Authority alignment during the window**: any legacy consumer that can decide the same content (e.g. a review drain classifying dismissed rows) re-checks the provider state under a shared per-bearer exclusion section before persisting its decision. Dismissal/migration and the legacy decision must be mutually exclusive per bearer.
5. **Migration as one guarded transaction**: all provider objects, journal intents, the receipt, the completion manifest, and the marker commit together; file effects ride the existing journal/publish protocol with the intent fencing EVERY affected subject (store the full fence set on the intent row — source-context lists are not placement). Re-runs verify against the manifest and no-op; source-snapshot changes refuse.
6. **Byte-exact preservation acceptance**: "migration ran" is not acceptance. Assert before/after equivalence: per-family row counts, per-record content equality (exact bodies, including whitespace — beware parsers that `trim_start()`), raw frontmatter preservation (fixed-structure parsers drop fields), file byte identity, and provider-only reads after the marker with legacy rows gone.
7. **Legacy selection semantics are TEXT semantics**: legacy SQL orders by raw timestamp TEXT (`ORDER BY created_at DESC`), which differs from parsed-instant order for mixed formats (`.` vs `Z`); same-instant tie groups straddle page boundaries. A faithful cutover orders, caps, and filters on the legacy keys over complete reads — bounded instant-ordered windows silently drop legacy-visible rows.
8. **Retention closes the journal**: after cutover, applied journal shells prune on a fixed age; unsettled intents (pending/deferred/quarantined) are never reclaimed — they are the recovery authority.

## Non-obvious pitfalls (all found by review loops)

- Pairwise parse-fallback comparators violate transitivity for mixed timestamp formats (a comparator must assign its total-order class per value, never per pair).
- Projection snapshots that hash only (id, revision) miss content changes of always-revision-1 projections — hash the served values.
- Canonical-subject re-selection (a smaller subject ensured later) can resurrect consumed candidates; stabilize ownership at ensure time.
- Disclosure ordering: resolve-and-refuse paths can leak existence/ownership before authorization checks; authorize first, then compose owner-specific refusals.
- Idempotency keys must name the mutation and include the discriminators that make the payload stable across true retries (freeze generated timestamps into the payload).

## Why this ordering

Each step is independently reviewable and reversible: the bridge can land before any caller moves; each entry point reroutes separately; the migration lands last, when the unified read path already exists to prove preservation; the marker flips reads atomically; physical removal stays a later, separately-gated decision.
