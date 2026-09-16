---
vars:
  work_ref: { type: string, required: true }
  work_id: { type: string, required: true }
  topic: { type: string, required: true }
  vibe: { type: string, default: "literary" }
  chapter: { type: integer, default: 1 }
  chapter_label: { type: string, default: "01" }
  outline_path: { type: string, required: true }
  slug: { type: string, default: "ch01" }
  world_kb_block: { type: string, default: "" }
  open_findings_block: { type: string, default: "" }
  foreshadowing_summary: { type: string, default: "" }
max_tokens: 3000
---

# Chapter Outline Generation

You are generating a **chapter outline** for chapter {{chapter}} of the novel
about **{{preset.input.topic}}** with a **{{preset.input.vibe}}** vibe.

**Work directory**: `Works/{{work_ref}}/`

**Output path**: Write the outline to:
`{{outline_path}}`

If the directory `Works/{{work_ref}}/Outlines/chapters/` does not exist, create it first.

{{#if world_kb_block}}
## World Context

The following World context block provides characters, locations, and active rules from the World KB. Honor these when planning the chapter:

```yaml
{{world_kb_block}}
```
{{/if}}

{{#if open_findings_block}}
## Open Findings to Address

The following open quality findings were surfaced by prior review passes for this chapter (and Work-level findings that affect every chapter). When planning the outline, **actively address** each item — either by structuring a scene/beat that resolves it, or by deliberately writing around it with a documented reason. Do not silently ignore them:

{{open_findings_block}}
{{/if}}

## Required Sections

The outline **must** contain every section below. Missing sections are a validation error:

1. **Opening Scene** — setting, mood, character entrance
2. **Core Conflict** — central tension or problem in this chapter
3. **Turning Point** — what changes direction or raises stakes
4. **Climax** — peak moment of tension or decision
5. **Ending Hook** — what compels the reader to turn to the next chapter
6. **Character State Change** — how the protagonist's situation or understanding shifts
7. **Foreshadowing Touched (F###)** — see below (REQUIRED even if empty)

## Foreshadowing Section (REQUIRED)

{{#if foreshadowing_summary}}
The current foreshadowing index (`Works/{{work_ref}}/Outlines/foreshadowing.md`) lists these active items — reuse their `F###` ids when you touch them, and stay consistent with their planted/paid-off status:

{{foreshadowing_summary}}
{{/if}}

The outline **must** include a `## Foreshadowing Touched (F###)` section. For each
foreshadowing item touched (buried or paid-off) in this chapter:

- Reference the F### id from `Works/{{work_ref}}/Outlines/foreshadowing.md` if it exists
- If `Outlines/foreshadowing.md` does not exist or this is a new foreshadowing item,
  declare a new F### inline (e.g., `F001: <description>`); the next outline pass will
  promote it to the index

If **no** foreshadowing is touched in this chapter, the section must still be present
with the note: "No foreshadowing items touched in this chapter."

## Format

Use the embedded `chapter-outline.md` template structure. Fill in each section with
concrete, actionable prose — not placeholders.
