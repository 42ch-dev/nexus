---
module: github-actions, ci
date: 2026-08-07
problem_type: workflow_issue
category: workflow-patterns
severity: medium
applies_when: [adding a new GitHub Actions workflow file on a non-default branch, iterating on trigger configuration for a workflow that does not yet exist on the default branch, writing a plan whose verification gate relies on a brand-new CI workflow]
tags: [github-actions, ci, workflow-registration, default-branch, runtime-build, headless-runtime]
---

# GitHub Actions Workflow Registration Is Default-Branch-Only

## Context

V1.153 P2 added a brand-new workflow file, `.github/workflows/runtime-build.yml` (headless `nexus-runtime` bin + Windows x64 export matrix), on a plan branch. The plan assumed the normal CI loop: push the branch → workflow runs; open a PR → checks appear; push a matching tag → release build runs. **None of that happened — silently.** The workflow file was not evaluated at all until it was mirrored onto the default branch.

## Guidance

### A new workflow file is registered from the default branch only

When a workflow file **does not yet exist on the default branch**, GitHub does not evaluate it for any trigger — `push`, `pull_request`, `workflow_dispatch`, or tag pushes. The workflow registry is derived from the default branch; a file that only exists on a feature/plan branch is invisible to the runner scheduler.

### Empirical verification (V1.153 P2 T2, all on the plan branch)

1. A temporary `push`-on-branch trigger on the new file → **no run**, no check suite, API 404 on the workflow ID.
2. A minimal probe workflow with `push` + `pull_request` triggers → **no run**.
3. A PR opened with a `pull_request` trigger in the head branch → **no run**, zero check suites on head or merge refs.
4. A matching tag push → **no run**.

The only fix that worked: **mirror the workflow file onto the default branch** (`main`, commit `9773cea1`). After that, `gh workflow list` showed the workflow as active and the PR-triggered matrix ran green.

### Mitigation pattern for plan-branch CI iteration

1. **Mirror first**: commit the (single-file) workflow to the default branch before iterating on it from the plan branch — even a stub with `workflow_dispatch` only.
2. Iterate on the plan branch afterward; trigger via `workflow_dispatch` or the PR now that the file is registered.
3. **Clean up temporary triggers before merge** — a leftover debug trigger on the final workflow shape is a classic merge-review finding.
4. Treat "CI will verify X" in a plan as **not verifiable** until the workflow file exists on the default branch.

## Why This Matters

- The failure mode is **invisible**: no error, no warning, no skipped run — just silence. Cost of not knowing: ~30+ minutes of silent no-run debugging in V1.153 P2.
- The documented rule ("`workflow_dispatch` requires the file on the default branch") is only part of the story. The empirical finding is that **push / pull_request / tag triggers are also default-branch-gated** for workflow files that do not yet exist there.
- A plan gate that says "CI runs on the branch" is false for brand-new workflow files; the plan must budget the mirror step.

## When to Apply

- Any plan that adds a new `.github/workflows/*.yml` file and wants to iterate on it from a feature/plan branch.
- Any review of a plan whose verification/QA gate relies on a CI workflow that has never run on `main` yet.
- Debugging a "workflow never runs" mystery where the file is new and the branch is not the default.

## Examples

### Before (the assumption)

```yaml
# .github/workflows/runtime-build.yml — pushed on the plan branch only
name: runtime-build
on:
  push:            # expected to run on branch push — never fires
  pull_request:    # expected to run on PR — never fires
  workflow_dispatch:   # not even listed by gh workflow list
  # tag pushes (runtime-v*) were also expected to fire — never did
```

Result: no runs, no check suites, API 404 — for hours.

### After (the fix)

1. Mirror the file to `main` (single-file commit `9773cea1`) → `gh workflow list` shows `runtime-build` active.
2. Continue iterating on the plan branch; PR-triggered matrix runs green.
3. Remove any temporary debug triggers before the merge.
