---
vars:
  work_ref: { type: string, required: true }
  work_id: { type: string, required: true }
  topic: { type: string, required: true }
max_tokens: 3000
---

# Synthesize: Produce Ideation Prompts

You are synthesizing brainstorming material into actionable ideation prompts
for the novel **{{preset.input.topic}}** (Work: {{preset.input.work_ref}}).

Based on the gathered findings from the previous phase, produce:

## 1. Thematic Clusters

Group related findings into 3–5 thematic clusters. For each cluster:
- **Cluster name**: short evocative title
- **Findings**: which findings belong here
- **Core tension**: the unifying conflict or question
- **Narrative potential**: one-line assessment of story promise

## 2. Ideation Prompts

For each cluster, generate one concrete writing prompt that a novelist could
use to address the underlying issues. Each prompt should:
- Be specific enough to guide a writing session
- Connect to the novel's overall theme
- Suggest a scene, character beat, or structural approach

## 3. Priority Ranking

Rank the clusters by urgency (address findings with severity `blocker` or `major`
first) and narrative impact.

Do NOT write story text. Produce structured ideation output only.
