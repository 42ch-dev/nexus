---
max_tokens: 2000
---

# Scheduled Sweep — Inventory New/Changed Source Documents (scheduled lane)

You are the scheduled lane of a game-narrative import strategy. This run was
triggered by a timer (interval/cron-style) rather than a game event. Your job
is to inventory the source documents available for import and produce a sweep
brief for the extraction steps that follow.

The extraction steps (import-worldview.md, import-character-sheet.md) read the
source documents from `preset.input.documents`; this brief is the lane's trace
artifact and the sweep decision.

## Inputs

- Documents available this sweep: `{{preset.input.documents}}`
- Target world: `{{preset.input.world_id}}`
- Last-sweep watermark / prior state (from the schedule seed or core_context;
  empty on first run): `{{core_context}}`

## Task

1. Inventory the documents present in the payload:
   - `worldview` — a worldview document (worldbuilding doc, lore bible,
     setting notes)
   - `character_sheets` — one or more character design sheets
2. For each document, decide whether it is NEW or CHANGED since the last sweep
   (compare against the watermark when one is present). Only new/changed
   documents should proceed to extraction; unchanged ones are skipped.
3. Note a suggested watermark for the next run (e.g. latest document revision
   or timestamp seen).

## Response Format

Respond with ONLY a JSON object (no markdown code fences):

```json
{
  "sweep_summary": "<one or two sentences>",
  "to_extract": {
    "worldview": "<document text or null>",
    "character_sheets": ["<sheet text>"]
  },
  "skipped": ["<document ids already imported>"],
  "next_watermark": "<suggested watermark for the next run>"
}
```

Use `null` / `[]` for absent sections. If there is nothing new to import, the
exit judge (scheduled-sweep-exit.md) will keep this lane parked until the next
interval.
