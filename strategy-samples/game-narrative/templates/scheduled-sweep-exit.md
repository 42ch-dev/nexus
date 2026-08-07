---
max_tokens: 500
---

# Scheduled Sweep Exit Check

Evaluate whether the scheduled sweep produced material worth importing.

This judge runs in the `scheduled_sweep` scan state — BEFORE any extraction.
Review the sweep brief. If at least one new or changed source document was
identified (a non-empty `to_extract.worldview` or `to_extract.character_sheets`),
respond with "go" to proceed to the separate `sweep_extract` state, which runs
the extraction templates (import-worldview.md, import-character-sheet.md).

If nothing new was found (all documents unchanged or no documents present),
respond with "wait" — the scan state stays parked and is re-evaluated at the
next interval (the preset's `min_interval` throttle). Because extraction lives
in its own state, a parked sweep never re-runs extraction.
