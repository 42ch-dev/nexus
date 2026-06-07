---
vars:
  work_ref: { type: string, required: true }
  work_id: { type: string, required: true }
  topic: { type: string, required: true }
  vibe: { type: string, default: "literary" }
  chapter: { type: integer, default: 1 }
max_tokens: 8000
---

# Chapter Draft

You are drafting the **body text** for chapter {{chapter}} of the novel
about **{{preset.input.topic}}** with a **{{preset.input.vibe}}** vibe.

**Work directory**: `Works/{{work_ref}}/`

**Output path**: Write the chapter body to:
`Works/{{work_ref}}/Stories/ch0{{chapter}}-<slug>.md`

Replace `<slug>` with a descriptive kebab-case slug derived from the chapter's
content (e.g., `ch01-the-awakening.md`). Create the `Stories/` directory if it
does not exist.

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

- `title`: human-readable chapter title
- `chapter`: chapter number (integer)
- `status`: must be `draft` on initial creation
- `word_count`: count of Chinese characters / words in the body text
- `world_refs`: list of World KB item ids referenced (leave `[]` if worldless)

## Content Guidelines

1. Read the chapter outline from `Works/{{work_ref}}/Outlines/chapters/ch0{{chapter}}-outline.md` first
2. Follow the outline's structure: opening scene, conflict, turning point, climax, ending hook
3. Honor any F### foreshadowing items listed in the outline
4. Write vivid, immersive prose that matches the **{{preset.input.vibe}}** style
5. Target 3000–5000 words for the body text
6. Do **not** include the title as a heading in the body — it's in frontmatter
