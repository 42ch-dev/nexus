---
module: iteration process (host/platform cutover acceptance evidence)
date: 2026-09-20
problem_type: workflow_issue
category: workflow-patterns
severity: medium
plan_id: 2026-09-19-v1.192-p0-electron-desktop-cutover
applies_when:
  - accepting a host, platform, service or runtime migration whose claim is "same product, new substrate"
  - writing or reviewing a capability-parity claim ("everything works like before")
  - reconciling task reports into one acceptance receipt, or consuming such a receipt as QC/QA
related_components:
  - apps/desktop-electron
  - apps/web
  - apps/nexus-service
tags:
  - parity
  - evidence
  - receipt
  - verdict-vocabulary
  - unverified
  - capability-inventory
  - cutover
  - acceptance
---

# Capability-parity receipt — layer-bound verdicts, revision-bound evidence, honest unverifieds

## Context

When a migration's acceptance criterion is "same product, new substrate", the rule is explicit: **"parity implied" is a fail** — every capability of the old substrate either has an explicit evidence-backed verdict or a registered deviation. The plan freezes a stable list of capability IDs (source-backed inventory: current source, target behavior, required behavior, owning task), and the cutover's reconciliation pass produces a receipt mapping each row to recorded evidence: report → implementing/fix revision → command → observed result → exercised boundary.

The receipt is where the iteration's honesty conventions concentrate. Its reusable discipline matters beyond a single cutover because every future retirement or migration (each with its own recorded blocker and unblocking trigger) must make the same kind of claim without inflating it.

## Guidance

### 1. Freeze stable capability IDs before implementation

The row list is part of the plan, not of the receipt: current-state evidence (old host source `file:line`), target behavior, deviations declared up front, and the owning task per row. Stable IDs are then shared across plans (implementation rows, packaging rows, retirement rows), so evidence can accumulate against a fixed vocabulary instead of being re-derived per report.

### 2. Every row gets an explicit verdict — "parity implied" is a fail

Rows are not prose; each is `verdict / remaining limit`. If a row cannot be demonstrated, it gets a registered deviation, an explicit non-addition, or an `[UNVERIFIED]` marker — never an omission.

### 3. Keep an evidence registry; the reconciliation pass is read-only

Structure each evidence key as: source report + implementing revision + **recorded command and observed result** (transcribed, not rerun) + the exercised boundary. The reconciliation pass maps and verdicts — it does not launch the app, rebuild, rerun tests, run signing tools, or rewrite earlier evidence. Test counts belong to their individual runs and **must never be summed** into an invented final-suite result; later fixes supersede earlier descriptions where noted.

### 4. Use a layered verdict vocabulary, and name the layer

- **"Demonstrated"** means *only* the named exercised layer (e.g. "path/adapter layer", "config layer", "controller/host seams", "policy/headless composition"). A passing adapter test proves the adapter contract, not the OS behavior.
- **`[UNVERIFIED]` marks a missing observation, not a passing test.** Real-environment behaviors (GUI launch, keychain/safeStorage runtime, LaunchServices, installed-app flows, actual window chrome) stay unverified unless actually exercised; test doubles, virtual clocks and source inspection are labelled as such.
- **Partial rows remain partial** even where a narrower behavior passed; a package-build receipt does not become a GUI qualification.
- Distinguish **deviations** (already-selected differences from the old host, each with rationale and evidence — e.g. "never auto-kill a PID-only listener") from **explicit non-additions** (intentionally not added, e.g. auto-update) and from **non-goals**.

### 5. Bind evidence to revisions, and never substitute one identifier for another

- The receipt baseline (the revision the receipt describes) is not the implementation SHA of any single task, and a receipt's own `git_revision`/`dirty`/input hashes are its provenance — if those fields are missing or unverified, that fact stays `[UNVERIFIED]`; "the implementation SHA" is not a substitute for "the receipt field".
- Report-transcribed observations are not the reconciler's recalculations; say which one you have. A checksum recorded by the builder is not independent verification.
- Where a task's output was cleaned up after smoke (generated receipts removed), the missing artifact is the finding — extraction/identity verification cannot be claimed from source inspection.

### 6. Closures are authoritative over row text

Fix-now findings close rows via a new commit plus a targeted re-review recorded as Approved; the row's prose keeps its fix-time wording and the receipt states the closure explicitly. Without that ordering rule, a closed row still *reads* open (or vice versa) and downstream consumers cannot tell which is current.

### 7. Separate authorization from evidence — never backfill old rows

A development GO may rest on an explicit decision record (e.g. a compass decision selecting an earlier authorization plus a CI run) while a historical evaluator file remains `blocked` with missing rows. The correct handling: record the authorization basis, keep the contradiction visible, and **do not regenerate or edit the historical evaluation** to resolve it. New acceptance evidence is bound to the new revision; it never retroactively fills old rows. If a reason remains unknown (why the old JSON was never regenerated), write `[UNVERIFIED]` rather than inventing one.

### 8. End with qualification limits and an ownership handoff

An explicit section enumerates what the receipt does **not** prove (e.g. all actual Electron/OS behavior; macOS floor metadata is not macOS-floor execution; a native CI matrix is not GUI qualification). Then name the owners of the remaining acts: fix acceptance, package-verifier/CI reconciliation, plan/QC/QA decisions, residual registration, and any separately requested real-environment qualification stay with PM/QA — the receipt records, it does not waive.

## Why This Matters

- **Cutovers are accepted on claims; the receipt is where claims become checkable.** Without layer-bound verdicts, "the host was cut over" silently downgrades into "the composition tests pass".
- **The unverified markers are the value.** The next party (QA, a future migration, a user asking about macOS 13) needs to know exactly which real-environment rows were never observed — and must not be able to confuse them with passes.
- **Revision binding prevents evidence laundering.** The most common failure shape is quoting an implementation SHA as if it certified a receipt, or a task report as if it certified the branch.

## When to Apply

- Any migration whose acceptance is "same product, new substrate" — host cutover, runtime swap, service extraction.
- Producing/consuming a reconciliation pass over multiple task reports with fix rounds.
- Any "we verified X" statement that mixes headless composition evidence with real-environment claims.

## Examples

### Capability row shape

```markdown
| ID | Capability | Implementing revision(s) | Row → evidence and observed behavior | Verdict / remaining limit |
|---|---|---|---|---|
| <id> | <capability> | `<sha>` + `<sha>` | `<evidence key>`: versioned closed IPC, size/admission checks, sandbox/isolation, protocol-before-window | **Demonstrated — <layer>.** Actual runtime enforcement [UNVERIFIED] |
```

### Evidence registry row shape

```markdown
| Key | Source and implementing revision | Recorded command and observed result | Exercised boundary |
|---|---|---|---|
| `<key>` | <task> report, fixes 1–2; `<sha>` → `<sha>` | `<command>` → exit 0; N pass / 0 fail | Compiled request/response parsers, sender/admission checks, policy path; not a real runtime renderer |
```
