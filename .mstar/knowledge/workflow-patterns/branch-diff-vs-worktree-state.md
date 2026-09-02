---
module: process, review
date: 2026-09-02
problem_type: workflow_issue
category: workflow-patterns
severity: medium
plan_id: 2026-09-02-v1.180-p2-core-reliability-sweep
tags:
  - review
  - qc
  - evidence-channel
  - worktree
  - branch-diff
applies_when:
  - "Running plan QC tri-review against a feature worktree"
  - "Generating or consuming mstar sdd review-package diffs"
  - "Verifying claimed gates (cargo check/test/clippy/fmt) from review evidence"
  - "Reconstructing what a subagent actually left in a shared checkout"
---

# Review-package diffs do not see working-tree corruption

## Context

During v1.180 p2 QC, seat qc1 flagged `boot.rs` containing literal `+`
line artifacts and a duplicated ~40-line resume block — while the
`review/branch-diff.patch` (generated via `mstar sdd review-package`) was
byte-identical to `git diff <base>..<head>` and showed clean hunks. Seats
qc2/qc3, reviewing only the patch file, saw no corruption.

Root cause: the patch describes **commits**; the working tree can drift
from HEAD via uncommitted edits (here: paste artifacts from an earlier
in-place fix attempt that were left uncommitted in the feature worktree).
`cargo check` still passed because the duplicated block happened to be
syntactically valid — corruption invisible to both the patch and the
compiler.

## What Didn't Work

- Reviewing only `branch-diff.patch` (commit-derived) — cannot see
  uncommitted working-tree drift.
- Trusting "worktree clean" claims without `git status` at review time —
  the two seats that checked the patch never ran `git status` in the
  worktree.

## Solution

1. **Reviewer protocol**: a QC seat that reviews a *worktree* (not just a
   patch) must run `git status --short` in that worktree first, and state
   the observed HEAD + cleanliness in its report. Any drift → flag as
   evidence-channel mismatch, do not adjudicate correctness from the
   patch alone.
2. **PM adjudication when seats disagree**: when one seat claims
   corruption that others cannot see in the patch, PM runs the cheap
   probes (`git status`, `git diff --stat`, `grep -n '^+' <file>`,
   `cargo check <crate> --all-targets`) in the worktree and decides —
   do not re-dispatch more reviewers to break the tie.
3. **Fix verification**: after repair, the fixer commits, and the
   re-review re-reads the **worktree state** (single recover block +
   single spawn; `grep '^+'` zero) — not just the fix diff.

## Why This Works

The patch is a projection of the commit graph; the worktree is the thing
tests actually compile against. Both are needed: patch = what is
proposed, worktree = what is real. The disagreement between the two IS
the finding.

## Prevention

- When dispatching worktree-scoped review assignments, explicitly require
  `git status --short` + HEAD echo in the report template.
- Treat "patch clean but worktree dirty" as a first-class evidence
  failure (Unconfirmed verdict), not a reviewer error.

## Examples

- v1.180 p2: qc1.md F-001 (corruption), qc2/qc3 patch-only reviews,
  PM adjudication via `grep -n '^+'` + `sed -n` reads, fix commit
  d2743bd3, three-seat re-verification at worktree HEAD.