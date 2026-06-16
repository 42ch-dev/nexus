---
vars:
  work_ref: { type: string, required: true }
  work_id: { type: string, required: true }
  topic: { type: string, required: true }
  vibe: { type: string, default: "literary" }
  chapter: { type: integer, default: 1 }
  chapter_label: { type: string, default: "01" }
  outline_path: { type: string, required: true }
  body_path: { type: string, required: true }
  slug: { type: string, default: "ch01" }
  world_kb_block: { type: string, default: "" }
  open_findings_block: { type: string, default: "" }
max_tokens: 8000
---

# Chapter Draft

You are drafting the **body text** for chapter {{chapter}} of the novel
about **{{preset.input.topic}}** with a **{{preset.input.vibe}}** vibe.

**Work directory**: `Works/{{work_ref}}/`

**Output path**: Write the chapter body to:
`{{body_path}}`

Create the `Stories/` directory if it does not exist.

{{#if world_kb_block}}
## World Context

The following World context block provides characters, locations, and active rules from the World KB. Stay consistent with these when writing:

```yaml
{{world_kb_block}}
```
{{/if}}

{{#if open_findings_block}}
## Open Findings to Address

The following open quality findings were surfaced by prior review passes for this chapter (and Work-level findings that affect every chapter). When drafting the body, **actively address** each item — either by writing prose that resolves it, or by deliberately writing around it with a documented reason in the frontmatter `notes`. Do not silently ignore them:

{{open_findings_block}}
{{/if}}

## Frontmatter (REQUIRED)

The chapter file **must** start with YAML frontmatter:

```yaml
---
title: "<chapter title>"
chapter: {{chapter}}
status: draft
word_count: <auto-calculated from body length>
world_refs: []
---
```

**Frontmatter fields**:
- `title` — chapter title (plain text, no markdown)
- `chapter` — chapter number (integer, matches outline)
- `status` — `draft` (first write), `revised`, or `finalized`
- `word_count` — auto-calculated from body length after writing
- `world_refs` — list of World KB anchor IDs referenced in this chapter (e.g. `["wka_char_alice"]`)

## Content Guidelines

1. Read the chapter outline from `{{outline_path}}` first
2. Follow the outline's structure: opening scene, conflict, turning point, climax, ending hook
3. Honor any F### foreshadowing items listed in the outline
4. Write vivid, immersive prose that matches the **{{preset.input.vibe}}** style
5. Target 3000–5000 words for the body text
6. Do **not** include the title as a heading in the body — it's in frontmatter
