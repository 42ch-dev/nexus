---
vars:
  output_dir: { type: string, required: true }
max_tokens: 4000
---

# Synthesizing Phase

Synthesize the validated extraction results into a structured research report.

Write the report to **{{preset.input.output_dir}}** with the following structure:

1. **Executive Summary**: overview of the reference corpus
2. **Source Inventory**: catalog of all processed sources with metadata
3. **Thematic Analysis**: cross-source themes and connections
4. **Key Findings**: notable content and insights from the corpus
5. **Gaps & Recommendations**: areas needing further research

Produce the report in Markdown format.
