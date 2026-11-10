---
module: schemas + nexus-contracts + nexus-core + apps/web
date: 2026-09-29
problem_type: architecture_pattern
category: architecture-patterns
severity: high
applies_when:
  - "Two or more operations in one wire request need the same enum-typed field with operation-specific meaning"
  - "A generated request type collapses a shared optional member into one field consumed by several handler arms"
  - "An acceptance test only ever exercises the default value of a shared optional member"
tags:
  - "json-schema"
  - "codegen"
  - "wire-contract"
  - "shared-optional"
  - "per-operation-members"
  - "enum-collapse"
---

# Shared Optional Wire Members Collapse Across Operations: Name the Member Per Operation

## Context

Plan 001 (v1.200, DR-26 scene/beat carrier) added authoring operations
(`add_scene` / `add_beat`) to the existing `outline.patch_structure` wire. The
schema declared **one optional `status` member** (`enum: [drafted, completed]`)
shared by both operations, intending each op to set the status of the entity it
names. Codegen collapsed it into a single generated field
`pub status: Option<OutlinePatchStructureRequestStatus>` — one value, two
meanings.

## What Didn't Work

Three failures stacked on the same root cause:

1. **Wrong-entity application**: `scene_add` passed `req.status` to the scene
   mapper and `beat_add` passed the *same* `req.status` to the beat mapper — the
   operations could not carry independent statuses, and a mid-stream "fix" that
   split the mapper helpers did not touch the actual collapse.
2. **Untypable non-default value**: the decode boundary rejected `completed`
   (the generated enum accepted only values outside its variant set), so no
   client could author anything but the default. The HTTP round-trip test
   passed anyway — because it only ever asserted `drafted`.
3. **Unfalsifiable evidence**: the plan's Done claim "both declared statuses are
   authorable" was supported by a test that structurally could not produce the
   non-default value. The plan QC tri-review caught all three as one Critical
   finding (F-1/F-5) only by reading the *generated* contract instead of the
   reported error text.

## Solution

Split the member per operation at the schema — the SSOT — and regenerate:

```json
"scene_status": {"enum": ["drafted", "completed"]}   // add_scene only
"beat_status":  {"enum": ["drafted", "completed"]}   // add_beat only
```

The generated types become two distinct enums/fields; each handler arm consumes
only its own member; the old shared member is removed **without a backward-compat
alias** (`additionalProperties: false` rejects it — correct for pre-1.0). Tests
then author `completed` on both operations and pin persistence on the *named*
entity (a beat's `completed` must not leak onto its parent scene).

## Why This Works

A shared optional member has exactly one value slot for N semantic slots; any
handler disagreement becomes silent data routing, and any value-set restriction
becomes a cross-operation lockstep. Per-operation members make the type system
carry the distinction the handlers actually make.

## Prevention

- When adding a request field consumed by multiple operation arms, name it per
  operation (`<entity>_<field>`), even if the types are currently identical —
  merging later is additive; splitting later is a breaking wire change.
- Review generated artifacts, not error messages: the collapse was visible in
  the generated struct (one field) before it was visible in any behavior.
- An acceptance criterion of the form "both values are authorable" needs a test
  that produces the non-default value end to end; a default-only round trip
  proves nothing about the rejected branch.

Source: iteration v1.200 plan 001 (compound 2026-09-29).
