---
max_tokens: 500
---

# Scheduled Sweep Exit Check

Evaluate whether the scheduled sweep produced material worth importing.

Review the sweep result. If at least one new or changed source document was
identified (a non-empty `to_extract.worldview` or `to_extract.character_sheets`),
respond with "go" to proceed to extraction and import.

If nothing new was found (all documents unchanged or no documents present),
respond with "wait" — the lane stays parked and is re-evaluated at the next
interval (the preset's `min_interval` throttle).
