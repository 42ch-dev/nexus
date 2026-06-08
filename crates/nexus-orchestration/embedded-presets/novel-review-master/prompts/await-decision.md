---
vars:
  work_ref: { type: string, required: true }
  work_id: { type: string, required: true }
  open_findings: { type: string, required: true }
max_tokens: 2000
---

# Review Master: Record Decisions

**Work:** {{preset.input.work_ref}}

You are recording the master's decisions for the following findings:

```
{{preset.input.open_findings}}
```

## Instructions

For each finding, record the master's decision:

1. **finding_id**: The unique identifier
2. **decision**: One of `approve`, `wont_fix`, or `defer`
3. **note** (optional): Brief rationale for the decision

Present the decisions as a structured list. The daemon will write these back
to the findings API (PATCH /v1/local/findings/{finding_id}).

### Example Output

- Finding `01JXY...` (continuity blocker): **approve** — "Must fix timeline inconsistency"
- Finding `01JXZ...` (craft minor): **wont_fix** — "Stylistic choice, not a bug"
- Finding `01JXQ...` (plot_hole major): **defer** — "Need to review chapter 7 first"

Await your decisions.
