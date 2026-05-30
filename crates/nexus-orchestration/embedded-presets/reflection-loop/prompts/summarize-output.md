---
vars:
  topic: { type: string, required: true }
max_tokens: 1000
---

# Summarize Output

Produce a concise summary of the refined work on the following topic:

**{{preset.input.topic}}**

The summary should:
1. Capture the key points and conclusions
2. Be significantly shorter than the full text
3. Preserve essential details and any critical nuance
4. Stand alone as a useful reference

Provide the summary now:
