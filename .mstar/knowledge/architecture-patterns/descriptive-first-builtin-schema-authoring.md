---
module: nexus-daemon-runtime (capability registry, catalog route, MCP child)
date: 2026-08-26
problem_type: architecture_pattern
category: architecture-patterns
severity: medium
plan_id: 2026-08-25-v1.175-p0-mcp-catalog-schemas-listchanged
tags:
  - mcp
  - json-schema
  - catalog-descriptor
  - placeholder-ledger
  - capability-registry
  - descriptive-first
applies_when:
  - "Authoring or extending builtin tool rows in the capability registry"
  - "Publishing an MCP/catalog surface whose schemas must be usable without reading the handler"
  - "Adding a builtin tool and deciding whether the runtime path should validate against its schema"
---

# Descriptive-first builtin schema authoring (CatalogDescriptor substrate)

## Context

Before V1.175, the MCP catalog's builtin rows (`nexus.*`, `fs/*`) carried
**pseudo-schemas** (`AcpWire` refs like `{"work_id":"string"}` or
`"WorkApiDto"`) — not valid draft-2020-12 JSON Schema — plus a uniform
permissive `{"type":"object"}` placeholder on the wire. An MCP consumer
could not type a builtin `tools/call` from `tools/list` alone. DF-89
replaced this with real per-tool schemas on all 30 static registry rows.

The authoring discipline that made the replacement stick is the reusable
part: the schema lives on the registry row, never silent, and never gates
the runtime path.

## Guidance

### 1. Schemas live on the registry row

`CapabilityRow` gains `catalog: CatalogDescriptor { description,
input_schema, output_schema }` holding `&'static str` literals — real
draft-2020-12 JSON text (root `"type":"object"` — every builtin tool takes
object params). `&'static str` fits the `LazyLock` static-row design: no
parse at registry build, no new dependency. The authored JSON text is the
single source of truth: it flows registry → catalog route string → MCP
child parse. No parallel DTO, no schema-file duplication.

### 2. Authoring ground truth = the handler's actual reads

Author `input_schema` from the handler's real argument reads
(`host_tool_handlers.rs`), cross-checked against the removed pseudo-shape
and the capability `input_schema()` source. A field the handler does not
read must NOT be promised in the schema. Example: `manuscript.chapter.update`
description once mentioned "block overrides" — the handler reads only
`work_id`/`chapter`/`volume`/`content` (grep-verified), so the schema omits
it. Promising a no-op field teaches consumers a false call surface.

### 3. Named placeholder + remainder ledger — never silent

A row without an authored input schema is `input_schema: None`; the catalog
route emits the constant `NAMED_PLACEHOLDER_INPUT =
{"type":"object","$comment":"nexus42:schema-pending"}` — draft-2020-12-valid
and machine-distinguishable from a real schema. A registry-source const
`SCHEMA_REMAINDER_LEDGER` lists pending rows. **Lockstep pin**
(`placeholder_ledger_lockstep`): `input_schema.is_none() ⇔ id ∈ LEDGER`,
both directions, plus `silent_placeholder_gone` (no row emits the bare
`{"type":"object"}`). The target is an empty ledger; a non-empty ledger at
plan close needs a remainder re-registration (owner/trigger/done).

### 4. Descriptive-first — schemas never gate the runtime

The builtin dispatch path gains **no schema-validation gate**: `tools/call`
accept/reject semantics stay byte-identical. The schemas describe the call
surface for consumers; they are not an enforcement layer. A strictness
migration is future work with its own migration tests. Descriptive-first
also means closed-world **in the schema where the handler is closed**:
`work.patch` pins `additionalProperties: false` at root and inside
`stage_metadata` matching `PATCH_ALLOWED_FIELDS` /
`STAGE_METADATA_ALLOWED_KEYS` — descriptive, not enforced.

### 5. Description / schema / handler triple coherence (the drift lesson)

After the first full pass, QC found 3 of 30 rows where the three claims
diverged: a description promising a handler-unread field, a description
under-claiming a 4-action enum, and read-tool descriptions over-promising
output fields the pinned output schema omits. The fix is a **coherence
pin** (`touched_rows_description_schema_handler_coherence`) that
machine-checks: chapter-update property set == handler reads; pool-enum ==
handler match arm; output-schema presence == description claims. Any future
row edit must keep the triple coherent or the pin fails.

### 6. Output schemas: pin stable objects, omit honestly

Author `output_schema` when the success shape is a stable object (most
read tools + all write tools); omit when not worth pinning. Omission is
honest and rule-based — **no ledger entry for output omission** (the ledger
tracks missing input schemas only).

### 7. Schema-equality lockstep family (registry ⇄ catalog ⇄ tools/list)

- `builtin_catalog_schema_equality_registry_to_route`: registry descriptor
  text ⇄ `GET /v1/daemon/tools` emission, VERBATIM string equality for
  every builtin row, both directions (input always, output when pinned),
  plus route→registry (no invented ids, row-count equality).
- `mcp_serve_e2e` catalog fixture is **registry-derived**
  (`host_tool_registry().lookup(...)`) so it can never drift; the parsed
  `tools/list` schemas are compared against the parsed registry
  descriptors (catalog ⇄ tools/list leg).

## Why This Matters

The MCP child is stateless (see `stateless-mcp-bridge-child.md`), so the
registry row is the ONLY source a consumer's `tools/list` sees. If the
schema text drifts from the handler or the description, the catalog is
dishonest again and the DF-89 gain evaporates — the lockstep pins are what
make "honest by construction" machine-checked instead of by-review.
Descriptive-first keeps the schemas zero-risk: they can never change tool
behavior, so the cost of authoring is documentation, not regression
surface.

## When to Apply

- Adding a new builtin row or editing a `CatalogDescriptor` — keep the
  triple coherent and the ledger pin green.
- Choosing whether to gate a new tool's dispatch on its schema — the house
  stance is descriptive-first; gates are a separate, planned change.
- Publishing a catalog to external MCP consumers (ACP agents, native CLI
  providers) — schemas must be usable standalone.

## Examples

```rust
// Registry row (capability_registry.rs): schema text is the single source.
reg.register(CapabilityRow {
    id: "nexus.work.patch",
    catalog: CatalogDescriptor {
        description: "Patch a work's title, inspiration log, or stage metadata.",
        input_schema: Some(r#"{"type":"object","required":["work_id"],"properties":{...},"additionalProperties":false}"#),
        output_schema: Some(r#"{...}"#),
    },
    // ...
});
```

- Proof suites: `capability_registry` unit pins
  (`placeholder_ledger_lockstep`, `silent_placeholder_gone`,
  `builtin_input_schemas_parse_as_root_object`), `honesty_lockstep.rs`
  (registry ⇄ route, both directions), `mcp_serve_e2e.rs` (registry-derived
  fixture, schema-equality leg over all 30 ids), coherence pin
  (`touched_rows_description_schema_handler_coherence`).
- The old `AcpWire` struct was deleted in the same change as the migration
  (clean cutover, no shim); grep the workspace before removing a
  registry-only carrier — the catalog route was the only consumer.
- Related: `stateless-mcp-bridge-child.md` (the child that consumes the
  catalog), `conventions/graph-pin-honesty-discipline.md` (rmcp pin that
  keeps the child's server dependency single-version).
