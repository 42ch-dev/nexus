---
module: tooling
date: 2026-08-15
problem_type: knowledge
category: conventions
severity: low
plan_id: 2026-08-14-v1.165-p1-entry-scoped-findings-alignment
tags: [subagent-crash, evidence-preservation, report-skeleton, qa-dispatch, redispatch]
last_updated: 2026-08-15
applies_when: Dispatching long-running verification/review subagents (QA gates, large test suites); any subagent that crashed mid-run before writing its output artifact
---

# Crash-Resilient Subagent Report Dispatch (Skeleton-First)

## Context

Across V1.164–V1.165, four long-running leaf agents (three QA gates, one PM review pass) crashed mid-run (host exit 1, no output). Each crash cost a full re-dispatch and, for the QA seats, lost runtime evidence (test results already produced but never written). One PM-pass crash additionally left **half-finished file edits** on disk that the re-dispatch had to detect and repair.

## Guidance

1. **Skeleton-first**: instruct every dispatch that writes a report file to CREATE the file (with headers/status placeholders) as its FIRST action, then append results incrementally. A crash then preserves partial evidence instead of nothing.
2. **Re-dispatch with damage survey**: after any mid-edit crash, assume partial writes. The re-dispatch prompt must say "files may contain partial edits — re-read, repair half-finished sentences, then apply your full work."
3. **Cheap triage before re-dispatch**: check for the report file (absent = clean re-run), `git status` (untouched tracked files = no repo damage), and the transcript tail (last actions indicate what completed).

## Why This Matters

Crashes cluster on the longest-running dispatches (full cargo test suites) — exactly the ones where lost evidence is most expensive to regenerate. And mid-edit crashes on shared artifacts produce inconsistent state that a naive re-dispatch will treat as intentional.

## When to Apply

- Any dispatch expected to run >5 minutes or write an artifact.
- Any re-dispatch after a `failed (exit 1)` with truncated output.

## Examples

V1.165 P1 QA: first attempt crashed after ~9 min with no report; re-dispatch ran skeleton-first and completed in 10 min with full evidence. V1.165 PM pass 1: crashed mid-edit; re-dispatch repaired two truncated sentences + a hashline-dump corruption in the compass.
