---
module: Release process governance (decoupled releases + enforced pre-trigger verification)
date: 2026-10-10
problem_type: workflow_issue
category: workflow-patterns
severity: high
plan_id: 2026-10-10-v1.210-p1-v011-release-readiness
applies_when:
  - designing or reviewing a release runbook or release pipeline
  - reconciling a "verify before publish" acceptance with an automatic publisher
  - deciding whether a release action belongs inside an iteration or a standalone process
  - writing acceptance criteria that name a verification the pipeline cannot enforce
related_components:
  - AGENTS.md (Release Policy — decoupled from development)
  - the release runbook (kept as an iteration-local snapshot; process paths stay out of tracked docs)
  - docs/release.md
  - .github/workflows/release.yml
tags:
  - release
  - decoupling
  - pre-trigger-gate
  - draft-first
  - verification-gate
  - acceptance-reconciliation
---

# Release decoupling and the mandatory pre-trigger gate

## Context

Across v1.208–v1.210 the release story produced three layered lessons. First, a shipped artifact can be broken in ways only a real consumer launch reveals (absolute staging symlinks; then a bootstrap resource path) — each fix exposed the next defect layer, and only a produced-artifact launch probe in CI closed the class. Second, executing a release **inside** an active iteration ties bump PRs, tags and producer runs to a branch that is about to be rewritten by the merge — v1.208's direct mid-iteration release left exactly this mess. Third, when the pipeline was finally asked to "verify the draft, then publish", the reviewer found the pipeline creates the draft, uploads, runs its count/name guards and clears `draft` in one uninterrupted step: the acceptance criterion named a human verification boundary that **does not exist**.

v1.210 turned all three into policy: releases are a standalone process fully decoupled from development (`AGENTS.md` → Release Policy), and the verification obligation is enforced as a **mandatory pre-trigger operator gate** that is executable with the shipped tooling.

## Guidance

### 1. A release is never part of a development flow

- No release action — version-bump PR, tag, producer runs, GitHub Release publish — runs inside an iteration, plan, task, or any phase/post-phase step. An iteration's Done never depends on a release; at most it delivers **readiness** (pipeline fixes, runbooks, gates).
- The trigger is independent: the shipped development work merges to the target branch first, then the release process runs as its own authorized operation. This keeps the tag anchored on a settled tree and keeps iteration PRs free of release side effects.

### 2. When the publisher is automatic, put enforcement **before the trigger**

A "draft-first" pipeline that auto-clears the draft has no operator pause. The correct reconciliation is not to soften the acceptance criterion into a post-publication audit (reviewers will — correctly — refuse), and not to invent a pause. It is to move the verification to the moment the operator actually controls: **before the release run is triggered**.

The v0.1.1 release runbook's gate (§2 of the iteration-local runbook snapshot) does this in five steps, all runnable with shipped tooling:

1. Dispatch the producer workflows manually as **canary runs** on the release branch (e.g. `release/0.1.1`) — before merging the bump PR.
2. Require the packaged launch probe to PASS for every architecture (the consumer-facing artifact gate).
3. Download the producer artifacts and check the receipts (`dirty: false` on a clean tag checkout) and family checksum contents.
4. Build a **local aggregate canary** with the frozen generator from the downloaded sidecars and verify it (`shasum -a 256 -c`) — proving the exact manifest the publisher will later assemble.
5. Only if every check passes, trigger the release (the merge/trigger is the approval point). Any failure = **do not merge / do not trigger**.

### 3. Keep the layers distinct when you describe the release

- **Enforced automatically** (source-backed): the publish job's staged/remote count and asset-name guards run *before* the draft-clearing PATCH; failure leaves the run un-published with the draft retained for repair-and-rerun.
- **Enforced by process** (operator, mandatory): the pre-trigger gate above.
- **Additional** (never the gate): post-publication download/hash re-verification; on a post-publication failure the remediation path is re-upload/re-run/notes — the release cannot be made "unpublished" again.
- **Not present**: a post-upload/pre-PATCH approval pause. If one is ever demanded, it must be built (e.g. environment protection) and recorded as an owner-bearing prerequisite — never implied to exist.

### 4. Write down what each gate actually proves

A consumer-launch probe proves launch; a receipt proves provenance of the build inputs; a socket-write completion proves the bytes left the writer — none of these proves downstream host processing. State the proven layer precisely in the runbook and in QC/QA dispositions; when a literal acceptance exceeds demonstrable evidence (v1.210: "cursor-advancing" without a production publisher), disposition it durably (plan amendment + register entry with owner/trigger/done) instead of letting prose drift from evidence.

## Why This Matters

A release runbook written against an imagined pipeline fails at the worst moment — during the release itself — or worse, silently publishes with less verification than the acceptance promised. Three QC rounds in v1.210 converged on this exact family of findings (one seat even re-raised it after a docs-only fix). Encoding the decoupling rule plus an executable pre-trigger gate aligns the contract, the pipeline, and the operator's real control points.

## When to Apply

- Authoring or reviewing any release runbook, release pipeline change, or release-related acceptance criterion.
- Whenever an acceptance criterion names verification that happens "before publish" while the pipeline publishes automatically.
- Deciding where a release action lives (iteration vs standalone) — default: standalone.

## Examples

```sh
# Pre-trigger gate (runbook §2 shape): canary producers on the release branch BEFORE the bump PR merges
gh workflow run runtime-build.yml --ref release/0.1.1
gh workflow run desktop-electron-build.yml --ref release/0.1.1
# ... download artifacts, check receipts dirty:false + family checksums + dual-arch launch PASS ...
mkdir -p ./v011-pretrigger/canary
node tooling/release/assemble-sha256sums.mjs --version 0.1.1 \
  --artifacts-dir ./v011-pretrigger/artifacts --output ./v011-pretrigger/canary/Nexus-0.1.1-SHA256SUMS
# Stage the seven binaries NEXT TO the manifest (the manifest carries bare names) before verifying:
cp ./v011-pretrigger/artifacts/nexus-runtime-*/nexus-runtime-*.zip ./v011-pretrigger/canary/
cp ./v011-pretrigger/artifacts/nexus-desktop-unsigned-darwin-*/Nexus-0.1.1-darwin-*.dmg ./v011-pretrigger/canary/
cp ./v011-pretrigger/artifacts/nexus-desktop-unsigned-darwin-*/Nexus-0.1.1-darwin-*.app.zip ./v011-pretrigger/canary/
(cd ./v011-pretrigger/canary && shasum -a 256 -c Nexus-0.1.1-SHA256SUMS)   # local aggregate canary must pass
# Only now merge the bump PR / trigger the release — the merge is the approval point.

# Automatic guard band (inside the publish job, before the draft-clearing PATCH):
# staged count == 15 && remote count == 15 && every staged asset name present remotely — failure leaves the draft unpublished.
```

Source: v1.210 P1 (plan `2026-10-10-v1.210-p1-v011-release-readiness`, QC1-F001/QC3-001 revalidation; the iteration-local release runbook snapshot); policy rule `AGENTS.md` → Release Policy (introduced by v1.210).
