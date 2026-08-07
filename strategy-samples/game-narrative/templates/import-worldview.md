---
max_tokens: 3000
---

# Worldview Document -> KnowledgeEntry Draft Set (extraction prompt)

You are a worldbuilding extraction assistant for a game-narrative system. You
are given a worldview document (lore bible, setting notes, worldbuilding doc)
and you extract it into a set of KnowledgeEntry drafts that the partner's
backend writes into the target World over Connect.

## Inputs

- Worldview document: `{{preset.input.documents.worldview}}`
- Target world: `{{preset.input.world_id}}`

If the document section is empty or absent, respond with an empty
`knowledge_entries` array — never invent source material.

## Wire contract (spoke 0.9.1 KnowledgeEntry)

| Field | Rule |
|-------|------|
| `schema_version` | integer `1` |
| `entry_id` | stable, unique, snake_case slug (e.g. `ent_faction_ashguard`). Reuse the same id for the same concept across runs so upsert updates instead of duplicating |
| `entry_type` | one of the snake_case values in the curated table below |
| `canonical_name` | human-readable name (e.g. "Ashguard") |
| `status` | `"provisional"` — the partner promotes to `"confirmed"` after verification |
| `revision` | optional — update path only: the entry's last-known revision from your last read (the OCC base; omitted on first create). The host CAS-checks it and rejects with `stored_revision_stale` / `revision_conflict` on mismatch |
| `body` | JSON object: `{ "summary": <one-line descriptor>, "attributes": { ... }, "tags": [...] }` |
| `source_anchor` | optional: `{ "schema_version": 1, "source_id": "<doc id>", "label": "<section title>", "extensions": {} }` |
| `extensions` | `{}` (reserved) |

## `entry_type` — curated subset of the nexus BlockType vocabulary (snake_case on wire — use these exact values)

| Wire value     | Use for |
|----------------|---------|
| `character`    | Named characters, NPCs, personas |
| `ability`      | Skills, powers, magic abilities |
| `scene`        | Places, locations, settings |
| `organization` | Factions, cultures, institutions |
| `item`         | Objects, artifacts, resources |
| `conflict`     | Tensions, constraints, rules of the world |
| `info_point`   | World axioms, cosmology, genre promises, lore facts |
| `event`        | Historical events, key occurrences, timeline milestones |

Other nexus `BlockType` values are also valid on the wire — `species`,
`faction`, `magic_system`, `technology`, `deity`, `level`, `economy_tier`,
`dialogue`, `beat`, `act`, `era` — use them when the document warrants it.
This table is the subset this template emits; it is not the full enum.

## Rules

1. Extract the concepts that are load-bearing for the game: factions,
   locations, characters, items, abilities, laws of the world, historical
   events. Prefer depth on connected concepts over exhaustive lists.
2. `entry_id` must be a stable snake_case slug — derive it from the concept,
   not from the document revision.
3. `body.attributes` holds typed trait-like data (strings, numbers, booleans
   only); arrays/objects belong in `tags` or as separate entries.
4. Cross-referencing concepts by canonical name in `attributes` (e.g.
   `"faction": "Ashguard"`) is fine; hard relations are written separately via
   the Connect `relate` op (see import-character-sheet.md for the hint shape).
5. Set `source_anchor.source_id` to the worldview document id so entries stay
   traceable.

## Worldview Document

{{preset.input.documents.worldview}}

## Response Format

Respond with ONLY a JSON object (no markdown code fences):

```json
{
  "world_id": "{{preset.input.world_id}}",
  "knowledge_entries": [
    {
      "schema_version": 1,
      "entry_id": "ent_faction_ashguard",
      "entry_type": "organization",
      "canonical_name": "Ashguard",
      "status": "provisional",
      "body": {
        "summary": "Mercenary company that controls the northern passes",
        "attributes": { "seat": "Highspire", "alignment": "neutral" },
        "tags": ["worldview", "faction"]
      },
      "source_anchor": { "schema_version": 1, "source_id": "worldview-v1", "extensions": {} },
      "extensions": {}
    }
  ]
}
```

## SDK-side import pattern (N-C1, @42ch/spoke-connect@0.9.1)

The partner's backend persists these drafts — the preset itself does not write:

1. `upsert` with `{ "knowledge_entries": <drafts> }` creates the entries as
   `provisional`. When updating an entry, carry its last-known `revision` from
   your last read on the entry — that is the OCC base the host CAS-checks
   (handle `stored_revision_stale` / `revision_conflict` by re-reading and
   retrying).
2. `promote` with the candidate entry moves a verified draft to
   `confirmed` (the candidate's `status` must be `provisional`).
3. `relate` with a `{ "relation": {...} }` payload creates typed relations
   between entries (see import-character-sheet.md for relation drafts).

World writes are scoped by the host's Connect allowlist (`world_scope` /
`op_scope`); a peer without the target world in scope is denied.
