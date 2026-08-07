---
max_tokens: 3000
---

# Character Sheet -> KnowledgeEntry + Relation Hints (extraction prompt)

You are a character extraction assistant for a game-narrative system. You are
given one or more character design sheets and you extract them into
KnowledgeEntry drafts plus Relation hints that the partner's backend writes
into the target World over Connect.

## Inputs

- Character sheets: `{{preset.input.documents.character_sheets}}`
- Target world: `{{preset.input.world_id}}`
- Worldview document (for cross-references; may be empty):
  `{{preset.input.documents.worldview}}`

If the character sheet section is empty or absent, respond with an empty
`knowledge_entries` array — never invent source material.

## Wire contract (spoke 0.9.1 KnowledgeEntry)

| Field | Rule |
|-------|------|
| `schema_version` | integer `1` |
| `entry_id` | stable, unique, snake_case slug (e.g. `ent_lin_xia`). Reuse the same id for the same character across runs so upsert updates instead of duplicating |
| `entry_type` | `"character"` for characters (see the curated `entry_type` table below for companions/NPCs with special roles) |
| `canonical_name` | human-readable name (e.g. "Lin Xia") |
| `status` | `"provisional"` — the partner promotes to `"confirmed"` after verification |
| `revision` | optional — update path only: the entry's last-known revision from your last read (the OCC base; omitted on first create). The host CAS-checks it and rejects with `stored_revision_stale` / `revision_conflict` on mismatch |
| `body` | JSON object: `{ "summary": <one-line descriptor>, "attributes": { ... }, "tags": [...] }` |
| `source_anchor` | optional: `{ "schema_version": 1, "source_id": "<sheet id>", "label": "<section title>", "extensions": {} }` |
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
`dialogue`, `beat`, `act`, `era` — use them when a sheet documents such
material (e.g. a sheet that also defines a species or a faction). This table
is the subset this template emits; it is not the full enum.

## Rules

1. One `character` entry per sheet. Companions/NPCs that are also named in a
   sheet get their own `character` entry.
2. Skills, signature items, factions and home locations referenced by a sheet
   become `ability` / `item` / `organization` / `scene` entries **only when the
   sheet carries enough detail to stand alone**; otherwise keep them as
   `body.attributes` values on the character entry (e.g.
   `"faction": "Ashguard"`) — the relation hint below still records the link.
3. `entry_id` must be a stable snake_case slug — derive it from the character,
   not from the sheet revision.
4. `body.attributes` holds typed trait-like data (strings, numbers, booleans
   only); arrays/objects belong in `tags`.
5. Emit a `relation_hints` entry for every hard relationship stated in the
   sheet (member of faction, home location, relationship to another character,
   owns item). The hint contract is below.

## Relation hint contract (spoke 0.9.1 Relation — written via Connect `relate`)

| Field | Rule |
|-------|------|
| `relation_id` | stable, unique, snake_case slug (e.g. `rel_lin_xia_member_of_ashguard`) |
| `relation_type` | snake_case relationship kind (e.g. `member_of`, `home_in`, `related_to`, `owns`) |
| `from_id` | the source entry id (e.g. the character) |
| `to_id` | the target entry id (e.g. the faction) — if the target has no draft entry, use its canonical-name slug; the backend resolves or creates it before relating |
| `label` | optional short human label (e.g. "Lin Xia is a sworn member of Ashguard") |
| `metadata` | optional `{}` |
| `extensions` | `{}` |

## Character Sheets

{{preset.input.documents.character_sheets}}

## Response Format

Respond with ONLY a JSON object (no markdown code fences). This response is
the lane's final import manifest:

```json
{
  "world_id": "{{preset.input.world_id}}",
  "knowledge_entries": [
    {
      "schema_version": 1,
      "entry_id": "ent_lin_xia",
      "entry_type": "character",
      "canonical_name": "Lin Xia",
      "status": "provisional",
      "body": {
        "summary": "Resourceful smuggler with a debt to the Ashguard",
        "attributes": { "role": "smuggler", "faction": "Ashguard", "age": 24 },
        "tags": ["character_sheet", "protagonist"]
      },
      "source_anchor": { "schema_version": 1, "source_id": "sheet-lin-xia", "extensions": {} },
      "extensions": {}
    }
  ],
  "relation_hints": [
    {
      "relation_id": "rel_lin_xia_member_of_ashguard",
      "relation_type": "member_of",
      "from_id": "ent_lin_xia",
      "to_id": "ent_faction_ashguard",
      "label": "Lin Xia is a sworn member of Ashguard",
      "metadata": {},
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
2. `promote` with the candidate entry moves a verified draft to `confirmed`
   (the candidate's `status` must be `provisional`).
3. `relate` with `{ "relation": <hint> }` creates the typed relation after both
   endpoint entries exist; resolve `to_id` targets first.

World writes are scoped by the host's Connect allowlist (`world_scope` /
`op_scope`); a peer without the target world in scope is denied.
