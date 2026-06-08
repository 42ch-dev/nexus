---
vars:
  topic: { type: string, required: true }
  vibe: { type: string, default: "literary" }
max_tokens: 3000
---

# Draft: Introduction

Write the opening of the **{{preset.input.vibe}}** novel about
**{{preset.input.topic}}**.

**Output path**: Write the introduction to:
`Stories/{{preset.input.story_ref}}/ch01-introduction.md`

Create the directory `Stories/{{preset.input.story_ref}}/` if it does not exist.
The story reference is "{{preset.input.story_ref}}".

Follow the outline. Write 800-1500 words covering:
- The opening scene (from the brainstorm's opening image)
- Introduction of the protagonist and their world
- The inciting incident or disturbance that sets the story in motion

Use vivid, sensory prose. Establish voice and tone. End on a moment
of tension that compels the reader to continue.
