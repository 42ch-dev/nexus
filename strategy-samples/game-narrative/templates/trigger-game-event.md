---
max_tokens: 2000
---

# Game Event -> Extraction/Assembly Task (trigger lane)

You are the trigger lane of a game-narrative import strategy. A game event has
arrived from the game runtime. Your job is to turn it into an
extraction/assembly task: decide which source documents the event carries or
references, and emit a task brief the extraction steps will follow.

The downstream extraction steps (import-worldview.md, import-character-sheet.md)
read the source documents from `preset.input.documents`. This brief is the
lane's trace artifact and the routing decision.

## Inputs

- Event payload: `{{preset.input.event}}`
- Target world: `{{preset.input.world_id}}`
- Documents carried by the event (may be empty): `{{preset.input.documents}}`

## Task

1. Summarize the event in one or two sentences.
2. Identify which imports the event warrants:
   - A worldview document (worldbuilding doc, lore bible, setting notes) -> `worldview`
   - One or more character sheets (design docs for characters/NPCs) -> `character_sheets`
   - Neither (the event only references existing entries) -> `none`
3. For each warranted import, quote the document text (or note its reference if
   it must be fetched by the backend before the extraction step).
4. Flag any entries the event references by a canonical name that likely
   already exist (the backend should re-read before upserting to avoid
   revision conflicts).

## Response Format

Respond with ONLY a JSON object (no markdown code fences):

```json
{
  "event_summary": "<one or two sentences>",
  "imports": ["worldview", "character_sheets"],
  "documents": {
    "worldview": "<quoted document text or null>",
    "character_sheets": ["<quoted sheet text>"]
  },
  "existing_entry_hints": ["<canonical names likely already imported>"]
}
```

Use `null` / `[]` for absent sections. Do not invent documents that are not in
the payload.
