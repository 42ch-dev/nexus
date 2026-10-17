---
title: Ignore-aware lookups hide committed gitignored paths
category: best-practices
track: knowledge
last_updated: 2026-10-18
created: 2026-10-18
status: active
problem_type: qa_false_negative
module: harness-qa
severity: low
symptoms:
  - "QA/verification reports a required file as missing even though `git log` shows it committed and `ls` finds it on disk"
  - "directory inventory tools that respect .gitignore list a tracked-under-force-add path as absent"
root_cause: "Verification helpers (and some listing/glob surfaces) apply .gitignore filtering; a path under an ignored subtree that was committed with `git add -f` (e.g. .mstar/iterations/** in this repo) exists in Git and on disk but is invisible to ignore-aware inventory, producing a false-negative acceptance check."
resolution_type: workflow_improvement
---

# Ignore-Aware Lookups Hide Committed Gitignored Paths

## Context

In this repo `.gitignore` ignores `.mstar/**` with negations only for `.mstar/knowledge/`, `.mstar/specs/`, and `.mstar/AGENTS.md`. Iteration-close artifacts that must travel on the integration branch (e.g. `iterations/<id>/specs/*.md`) are therefore committed with `git add -f`. Any later check that enumerates via an ignore-aware tool reports the file as missing — v1.217 QA round 1 REJECTED criterion 3 on exactly this false negative.

## Guidance

When verifying the existence of a file that may live under an ignored subtree:

1. Prefer Git as the source of truth: `git ls-files --error-unmatch <path>` (run inside the checkout under test).
2. Or use an absolute-path `cat`/`test -f` — never an ignore-aware directory listing — before declaring a file missing.
3. In QA reports, distinguish "absent" from "not listed by the inventory tool"; the second is not evidence.
4. When staging such artifacts, expect `git status` to stay silent for them; confirm with `git diff --cached --name-only` after `git add -f`.

## When to Apply

- Any acceptance check against `iterations/<id>/**` artifacts on an integration branch.
- Any QA/tooling step that enumerates directories to prove presence or absence.
