---
max_tokens: 500
---

# Scanning Exit Check

Evaluate whether the scanning phase has identified sufficient reference sources to proceed with extraction.

Review the scan results. If at least 3 distinct reference sources have been identified with
extractability assessments, respond with "go" to proceed to extraction.

If no sources were found or all sources are marked as unsupported, respond with "wait" and
suggest alternative directories or file formats to check.
