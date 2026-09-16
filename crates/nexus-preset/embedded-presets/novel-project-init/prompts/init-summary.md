---
vars:
  initial_idea: { type: string, required: true }
max_tokens: 2000
---

# Novel Project Init — Summary & Confirm

Recap everything the creator has provided and ask for final confirmation before committing the scaffold:

**Summary to present:**

| Field | Value |
| --- | --- |
| Working Title | {{title}} |
| Genre/Tone | {{genre_tags}} |
| Total Chapters | {{total_planned_chapters}} |
| Work Reference | `Works/{{work_ref}}/` |
| World Binding | {{world_binding_summary}} |
| Worldless Setting Note | {{setting_note_or_none}} |

**What will happen on confirm:**

1. Create directory tree: `Works/{{work_ref}}/` with `Outlines/`, `Stories/`, `Logs/`, `README.md`, template stubs
2. Seed `{{total_planned_chapters}}` chapter rows in the database (all with `status: not_started`)
3. Set `work_profile: novel` on the Work record

**Idempotency note:** If this Work already has a scaffold, existing files will be **preserved** by default.

Ask:
- "Does everything look correct? (yes / no / adjust)"
- If re-init: "Overwrite existing template files? (default: no — preserve existing)"

Only proceed when they explicitly confirm.
