---
module: process, review
date: 2026-09-02
problem_type: workflow_issue
category: workflow-patterns
severity: medium
plan_id: 2026-09-02-v1.180-p2-core-reliability-sweep
last_updated: 2026-10-11
tags:
  - review
  - qc
  - evidence-channel
  - worktree
  - branch-diff
  - path-resolution
  - control-root
  - checkout-alignment
applies_when:
  - "Running plan QC tri-review against a feature worktree"
  - "Generating or consuming mstar sdd review-package diffs"
  - "Verifying claimed gates (cargo check/test/clippy/fmt) from review evidence"
  - "Reconstructing what a subagent actually left in a shared checkout"
  - "Dispatching work into a feature worktree, or reading tracked harness files from one"
  - "A verdict or an edit contradicts what the branch is supposed to contain"
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

## Cross-checkout path resolution (v1.212)

A second way for review or edit evidence to describe the wrong tree, with no
working-tree drift involved: **path resolution**.

Two facts make it possible:

- `.mstar/specs/**` and `.mstar/knowledge/**` are tracked (`.gitignore` ignores
  `.mstar/**` and re-includes only those two subtrees plus `.mstar/AGENTS.md`), so
  **every checkout carries its own copy of the same relative path at its own
  revision**. A branch that amends a spec leaves the control root's `main` copy
  untouched until the PR merges. The rest of `.mstar/**` (plans, sdd, workflows,
  iterations) is ignored and exists only in the control root.
- The control root (the primary checkout, where the harness process runs) and a
  feature worktree are two such checkouts, and a session's tools can disagree
  about their base directory: shell commands inherit the working directory,
  while file reads and edits can resolve against the session's control root.

Observed in v1.212:

- **A false REJECTED verdict.** A p2 QA seat ran its `bash`/`rg` commands inside
  the feature worktree (command results correct) but read two tracked specs
  through `.mstar/specs/...` relative paths. It therefore judged the unamended
  `main` copies and failed a spec-truth criterion the branch had in fact
  satisfied. Re-read by absolute worktree path, the criterion passed and the
  verdict was corrected to ACCEPTED WITH NOTES.
- **Edits landing in the wrong tree.** A p3 implementer's first edit pass wrote
  to the control root through relative `crates/...` paths; the feature worktree
  stayed clean while the control root showed the modifications in `git status`.
  The pass was fully restored (`git checkout --`) before any commit, and both
  trees were verified clean afterwards.

The two directions are not equally detectable. A wrong-tree *write* shows up as
stray modifications in `git status` on the tree that should have stayed clean. A
wrong-tree *read* leaves no trace and can travel all the way into a verdict: when
a claim contradicts the branch (e.g. "spec X still names symbol Y" while the
branch removed it), first ask **which checkout was read** instead of adjudicating
the claim.

Remedy: an Assignment that touches tracked harness results names the checkout root
absolutely and requires absolute paths for **both** reads and writes —
relative-path resolution is per tool, not per task, so "run the shell in the
worktree" is not sufficient. Have the seat observe `pwd` + the checked-out branch
at start (mismatch → stop) and report `git -C <control-root> status --porcelain`
at start and end, expecting empty.

Unlike the drift failure above, nothing here is uncommitted: both checkouts can
be clean while the evidence still describes the wrong one.

## Examples

- v1.180 p2: qc1.md F-001 (corruption), qc2/qc3 patch-only reviews,
  PM adjudication via `grep -n '^+'` + `sed -n` reads, fix commit
  d2743bd3, three-seat re-verification at worktree HEAD.
- v1.212 p2: QA spec-truth read via relative paths → false REJECTED, corrected
  after re-reading the two specs at absolute worktree paths.
- v1.212 p3: first edit pass written to the control root via relative paths,
  detected with `git status`, restored before commit.
