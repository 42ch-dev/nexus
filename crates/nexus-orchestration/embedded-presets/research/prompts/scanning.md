---
vars:
  references_dir: { type: string, required: true }
max_tokens: 2000
---

# Scanning Phase

Scan the references directory at **{{preset.input.references_dir}}** for all available reference sources.

Identify and list each file found, noting:
- File name and extension
- File type (PDF, Markdown, Text, URL, HTML)
- Approximate file size

For each source, provide a brief assessment of extractability:
- **Ready**: can be fully extracted
- **Partial**: may have extraction limitations
- **Unsupported**: cannot be processed

Format the scan results as a numbered list with one entry per source.
