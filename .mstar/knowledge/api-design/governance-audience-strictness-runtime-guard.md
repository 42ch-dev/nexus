# Governance-Audience Strictness Needs a Runtime Guard

**Source:** v1.199 medium-residual convergence (`R4-audience-oneOf`, plan `2026-09-28-medium-residual-convergence`). SSOT for the audience contract: [holder-governance.md](../../specs/architecture/holder-governance.md).

## Failure shape

A wire schema can look strict while every runtime layer ignores that strictness:

- `typify`-generated adjacent-tag `oneOf` arms carried arm-level `additionalProperties: false`, but serde's internally-tagged deserialization **ignores arm-internal unknown members** (probed on serde 1.0.229) — the schema promise is not enforced at the Rust boundary.
- The generated TypeScript side is a discriminated union with **no runtime validator**, so the TS service forwarded unknown members verbatim.
- Net effect: an arm-internal unknown field on the actor-knowledge create surface returned **201 Created** while both the schema and the hand-written route looked correct. The same gap hid behind a **false-positive 400**: `WORLD_KB_ENTITY_PATCH_SHAPE` (native validator) declared no `audience` field, so patches were rejected as *patch-level unknown field* — a test asserting 400 passed without any audience-arm guard existing.

## Pattern

1. **Enforce closed arms at a real runtime guard, not in the schema.** A closed oneOf shape in the native validator (`validate.ts`: per-arm `additionalProperties: false`, required `kind`, arm-specific required fields such as `character_private → character_id`) turns the schema promise into behavior.
2. **Key allowmaps by the wire spelling.** The audience vocabulary is hyphenated (`author-only`, `character-private`); an allowmap keyed `author_only` silently skips every governed arm. Prefer switching on the generated union so the compiler ties keys to wire values; otherwise pin the exact strings.
3. **Own-key lookups only.** A plain object allowmap inherits `toString`/`constructor`; `Object.hasOwn` (or a null-prototype map) keeps unknown kinds — including prototype names — flowing to downstream validation instead of throwing.
4. **Guard every call site of the family.** Create and patch verbs share the audience schema; fixing one verb leaves the other accepting unknown members. Factor one guard helper and apply it at both.
5. **Pin the guard, not the status code.** A regression that asserts any 400 passes with or without the fix. Assert the guard's distinguishing message (`audience contains unknown fields`) on a **known** arm, and add a valid-audience-passes probe so the shape is proven permissive-exactly-where-intended.

## Scope note

`packages/nexus-native/src/validate.ts` shapes and `apps/nexus-service/src/actors.ts` are service-boundary enforcement — no wire schema or generated contract changed.
