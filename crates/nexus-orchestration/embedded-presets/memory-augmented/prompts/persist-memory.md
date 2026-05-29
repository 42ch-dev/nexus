---
vars:
  keyword: { type: string, required: true }
  topic: { type: string, required: true }
max_tokens: 500
---

# Persist Memory Fragment

The generated content on "{{preset.input.topic}}" (keyword: "{{preset.input.keyword}}")
is being persisted as a new memory fragment for future recall.

This step stores the output so that subsequent runs of this or other
memory-augmented workflows can recall it when the keyword matches.
