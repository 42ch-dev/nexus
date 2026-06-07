---
vars:
  work_ref: { type: string, required: true }
  work_id: { type: string, required: true }
  topic: { type: string, required: true }
  vibe: { type: string, default: "literary" }
  chapter: { type: integer, default: 1 }
max_tokens: 3000
---

# Chapter Outline Generation

You are generating a **chapter outline** for chapter {{chapter}} of the novel
about **{{preset.input.topic}}** with a **{{preset.input.vibe}}** vibe.

**Work directory**: `Works/{{work_ref}}/`

**Output path**: Write the outline to:
`Works/{{work_ref}}/Outlines/chapters/ch0{{chapter}}-outline.md`

If the directory `Works/{{work_ref}}/Outlines/chapters/` does not exist, create it first.

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
