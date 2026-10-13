# descriptive first builtin schema authoring — Historical Notes

**Provenance**: frozen from [`.mstar/knowledge/architecture-patterns/descriptive-first-builtin-schema-authoring.md`](../../../knowledge/architecture-patterns/descriptive-first-builtin-schema-authoring.md) on 2026-10-13 during the **v1.216** de-historicization pass (plan `2026-10-13-active-docs-dehistorize`).
**Evidenced supersession date**: 2026-10-13 (archive-move date; the removed text spans multiple iteration markers with no single uncontested calendar date, so the freeze date is used and stated here).
**Freeze date**: 2026-10-13

---

## Removed-line ledger (verbatim, in original order)

Every history unit removed from the live document by the v1.216 pass is reproduced below, verbatim and in original order, anchored to the line it occupied in the pre-freeze document. Still-true statements were reworded in place in the live doc; their superseded wording appears here. Nothing here is current authority.

### at pre-freeze line 26

```markdown
Before V1.175, the MCP catalog's builtin rows (`nexus.*`, `fs/*`) carried
```

### at pre-freeze line 28

```markdown
`"WorkApiDto"`) — not valid draft-2020-12 JSON Schema — plus a uniform
permissive `{"type":"object"}` placeholder on the wire. An MCP consumer
could not type a builtin `tools/call` from `tools/list` alone. DF-89
replaced this with real per-tool schemas on all 30 static registry rows.
```

### at pre-freeze line 33

```markdown
The authoring discipline that made the replacement stick is the reusable
part: the schema lives on the registry row, never silent, and never gates
the runtime path.
```

### at pre-freeze line 52

```markdown
(`host_tool_handlers.rs`), cross-checked against the removed pseudo-shape
and the capability `input_schema()` source. A field the handler does not
read must NOT be promised in the schema. Example: `manuscript.chapter.update`
description once mentioned "block overrides" — the handler reads only
`work_id`/`chapter`/`volume`/`content` (grep-verified), so the schema omits
it. Promising a no-op field teaches consumers a false call surface.
```

### at pre-freeze line 82

```markdown
### 5. Description / schema / handler triple coherence (the drift lesson)
```

### at pre-freeze line 84

```markdown
After the first full pass, QC found 3 of 30 rows where the three claims
diverged: a description promising a handler-unread field, a description
under-claiming a 4-action enum, and read-tool descriptions over-promising
output fields the pinned output schema omits. The fix is a **coherence
pin** (`touched_rows_description_schema_handler_coherence`) that
```

### at pre-freeze line 152

```markdown
- The old `AcpWire` struct was deleted in the same change as the migration
  (clean cutover, no shim); grep the workspace before removing a
  registry-only carrier — the catalog route was the only consumer.
```
