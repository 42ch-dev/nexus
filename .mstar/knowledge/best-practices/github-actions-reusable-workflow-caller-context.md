---
module: .github/workflows/release.yml, .github/workflows/new-release.yml, .github/workflows/runtime-build.yml, .github/workflows/desktop-electron-build.yml
date: 2026-09-30
problem_type: best_practice
category: best-practices
severity: medium
plan_id: 2026-09-30-v1.202-p3-release-governance
tags:
  - github-actions
  - reusable-workflows
  - caller-context
  - concurrency
  - release-pipeline
  - input-based-identity
---

# GitHub Actions reusable workflows: the `github` context belongs to the caller

## Context

v1.202 P3 built the release pipeline (`release.yml`) on two `workflow_call` producers (`runtime-build.yml`, `desktop-electron-build.yml`). QC3-004 required release-invoked producer runs to be cancel-isolated from ordinary builds. The first fix keyed the producers' `concurrency` groups on `github.event_name == 'workflow_call'` — and never fired.

**Root cause**: inside a reusable (called) workflow, the entire `github` context is associated with the **caller** workflow, not the called one. `github.event_name` reflects the caller's event (`pull_request` / `push`), `github.ref` the caller's ref, `github.workflow` the caller's workflow name. There is no in-band "am I being called?" signal — an expression testing `== 'workflow_call'` is dead code.

## Guidance

1. **Never detect reusable invocation via `github.event_name`.** It cannot work; the condition is statically false for every merged-PR/push caller.
2. **Pass an explicit identity input from the caller.** Declare an optional `workflow_call` input (e.g. `release_run_identity`, default empty) on the producer; the caller supplies it (`with: release_run_identity: ${{ github.run_id }}`).
3. **Branch the concurrency contract on that input.** Workflow-level `concurrency` supports `github`/`inputs`/`vars` contexts:

```yaml
concurrency:
  group: ${{ inputs.release_run_identity != '' && format('release-{0}', inputs.release_run_identity) || format('{0}-{1}', github.workflow, github.ref) }}
  cancel-in-progress: ${{ inputs.release_run_identity == '' }}
```

   Standalone runs keep today's ref-based group with deliberate cancellation; each release invocation gets a unique `release-<caller-run-id>` group with cancellation disabled — ordinary builds cannot cancel a release, and two simultaneous releases don't cancel each other.

## Why This Matters

The trap is doubly expensive: the wrong mechanism compiles, parses, and its scenario walkthrough reads plausibly — only a caller-context-aware review (or a live run) exposes it. A release cancelled by an unrelated main push fails in a way that looks like flakiness, not like a workflow-authoring bug.

## When to Apply

Any `workflow_call` producer whose runtime posture must differ between standalone and called execution: concurrency isolation, permission-sensitive steps, artifact retention, notification routing.

## Examples

- Fixed mechanism: `17fe2e056` (v1.202 P3) — `release_run_identity` inputs on both producers, `github.run_id` wired from both `release.yml` calls.
- Rejected mechanism (kept as the negative example): `7ebefd33f`, QC3-004 revalidation in `.mstar/sdd/2026-09-30-v1.202-p3-release-governance/review/qc3.md`.

## Related

- [GitHub Docs — Reusing workflows (`using`)](https://docs.github.com/en/actions/how-tos/reuse-automations/reuse-workflows)
- [GitHub Docs — Contexts availability (`concurrency` supports `github`/`inputs`/`vars`)](https://docs.github.com/en/actions/reference/workflows-and-actions/contexts#context-availability)
