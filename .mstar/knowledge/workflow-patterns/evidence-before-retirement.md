---
module: repo-wide (obsolete host/composition retirement), CI/lockfile/docs
date: 2026-09-20
last_updated: 2026-10-11
problem_type: workflow_issue
category: workflow-patterns
severity: medium
plan_id: 2026-09-19-v1.192-p2-obsolete-host-retirement
applies_when:
  - retiring a host, composition, workflow, dependency tree or CLI family and answering "is this actually obsolete?"
  - a residual, roadmap item or task title claims a family is dead code — before acting on that claim
  - writing or reviewing a retirement closeout / documentation sweep
  - deleting a field, struct member or symbol that looks dead because nothing reads it
  - a deletion sweep over serde DTOs or schema-validated input types
related_components:
  - apps/desktop-electron
  - apps/nexus42
  - .github/workflows
  - docs / specs / knowledge surfaces
tags:
  - retirement
  - consumer-inventory
  - deferral
  - closeout-ledger
  - documentation-lag
  - dead-code
  - lockfile
  - validation-carrier
---

# Evidence before retirement — inventory consumers, adjudicate the keeps, ledger the closeout

## Context

Retiring a host, composition, workflow, dependency tree or CLI family is a claim about the current tree — and it is easy to get wrong in both directions. Families that *look* obsolete are often still wired (a default feature, a boot path, a contract-bound client); families that are genuinely dead carry tokens in dozens of files. The retirement round's most reusable output is the discipline that separates the two outcomes: a source-reverified consumer inventory, an adjudicated-keep record, and a closeout ledger that makes documentation lag a tracked quantity instead of a discovered surprise.

## Guidance

### 1. Inventory consumers against current HEAD sources before calling anything obsolete

A residual title, a roadmap sentence or a plan's intention is **not** evidence. For each candidate family, re-verify against current sources at a recorded baseline revision, per fact, with `file:line` evidence, in two passes:

1. **Is it still wired?** (default features, boot paths, release embedding, contract-bound clients, dev proxies, API call sites).
2. **Does anything still call it?** (CLI leaves, scripts, workflows, dependabot entries, docs-as-contract).

Concrete findings worth imitating: default features can still enable a legacy CLI + web-embed pair; a `start` leaf can still boot the integrated daemon; a release can still embed and serve an SPA; a client can be contract-bound to `/v1/daemon/*`. Note the trap: a residual's detail text can describe a *working tree ahead of `main`* — inventory against `main`, not against the plan branch that proposed the deletion.

### 2. Dormant ≠ dead: adjudicate callable surfaces row by row

Every "dormant" CLI row existed in code with a clap definition and a `run` path — hard-reject stubs (`creator workspace clone`), coming-soon prints (`workspace link|unlink|status`, `publish`), config-error/exit-2 stubs (`explore browse|search`, `platform context assemble`), and hidden-but-callable aliases (`sync`, `preset`, `capability`). One row can even be *visible* while the spec says hidden. Deletion of such rows requires a per-row disposition plus help/docs/parity proof — "no callers in tests" is not obsolescence.

### 3. Sequence deletion after the replacement passes acceptance

The native-composition deletion was correct only because its replacement (the Electron host) had passed its own gates; the retirement commit stayed separable and revertable on the integration branch. Deleting a shipped composition before the replacement is accepted trades a working product for an unverified one — this is a sequencing rule, not a style preference.

### 4. Deferrals carry a named blocker + trigger + tracking location

A deferral is valid only when it names: the **blocker** (residual id, missing proof), the **trigger** that unblocks a later attempt, and where it is tracked (roadmap slice / register row). "Later" in prose is not a deferral. The same rule applies per row in a plan's roadmap table and in the closeout.

### 5. Close the round with a ledger, not a narrative

The closeout is a first-class artifact with four sections:

| Section | Content |
| --- | --- |
| **Retired** | each path/package/workflow + the evidence report/commit that justifies it |
| **Verified retained** | family → why it stays → the same-iteration evidence (`file:line`) that says so |
| **Deferred** | item → blocker/trigger → tracking location (e.g. `R-…` residual, roadmap slice) |
| **Documentation lag** | path:line list of stale mentions + disposition + follow-up owner |

Documentation lag is expected: a host retirement invalidates prose across `README`s, `docs/`, specs, AGENTS files, `CONCEPTS.md`, `STRATEGY.md`, code comments and even proof harnesses. Listing it explicitly — each with disposition ("documentation lag, not a code defect; follow-up owner: PM, next docs-pass") — is what keeps it from being silently re-discovered later.

### 6. Sweep current docs to post-cutover truth in the same wave, with one writer and a scoped file list

- One writer owns the sweep's file list; classify every remaining mention as **N** (new retirement note), **H** (historical record, kept verbatim — "do not rewrite history"), **C** (code-accurate retained entry), **L** (durable roadmap label), **E** (pre-existing stale, left with rationale). Report the counts per file.
- Sweep the *inputs* too: CI filters, workflow jobs, dependabot ecosystem directories, lint overrides with dead globs. Verify each remaining dependabot directory exists on disk at HEAD (11 entries remained; one cargo entry pointed at the deleted crate).
- Repair the normative statements that still contradict the retirement (a spec header, a status table) — while preserving the explicitly retained families' descriptions. QC will find contradictory "still authoritative" text if the sweep stops at README-level files.
- Re-check links mechanically after the sweep (a scoped relative-link resolver): record how many checked, how many resolve, and classify any pre-existing dangling link (e.g. a gitignored iteration path) instead of silently ignoring it.

### 7. Run a debris check for live callers of deleted paths

Before declaring the closeout done, grep for executable callers of every deleted script/directory across owned scopes **and** outside them. A retirement can surface an out-of-scope live caller: a closed harness may still spawn a script the retirement just deleted, for a surface already non-functional at baseline. The disposition (remove the broken surface rather than restore the deleted script) belongs to PM; the *finding* belongs to the closeout. Stale comment-only citations and historical records are fine; live executable references are not.

### 8. Lockfile and workspace hygiene are part of the deletion evidence

For dependency retirements: regenerate with `--lockfile-only`, run it **twice** (idempotence: identical diff), confirm the removed content is exactly the retired family, confirm unrelated dependencies survive (the `sharp` entry was preserved for the *new* host while the removed host's plugin packages and the `desktop` importer disappeared), then prove `--frozen-lockfile` "Already up to date" and a **cold-state install** with the family's packages absent from `node_modules`. An export-surface diff on shared scripts (e.g. 42 → 39 exports, "shared unchanged: true") plus one smoke of the retained dispatch is cheap proof that the cut didn't take neighbors with it.

### 9. Gate external actions on the verified main merge

Closing dependabot PRs or any external cleanup executes only **after the retirement verifies merged to `main`** — never because a feature worktree deleted the files. The preconditions are explicit: confirm the PRs still target the retired tree, comment with the reason and the merge SHA, close, record final states/URLs as post-merge receipts. Related post-merge obligations (branch-protection required check names, CI green runs) are recorded at closeout even when they execute later.

### 10. "No callers" ≠ "no contract": an unread field can enforce a type check

The consumer inventory answers *who reads this*. It does not answer *what this
enforces*. A type check on a trust boundary is a contract even when the value is
never read, so a zero-caller symbol can still be load-bearing.

A dead-code sweep once deleted two serde fields from `nexus-orchestration` capability structs as "unread":

- `FinalizeDraftInput.word_count: Option<WordCount>` — an enum whose only variant is `Auto`
- `ExtractResponse.source_work_entry_id: String`

Both were **deserialization-time validators**. The declared input schemas admit
an integer `word_count` and an arbitrary-typed `source_work_entry_id`, so upstream
schema validation cannot reject those inputs — the Rust types are the only
rejection point. Deleting the fields silently **widened accepted input**, a
behaviour change inside a removal-only task, invisible to callers, tests and the
compiler.

The fix restored both fields (and the enum) with a one-line justified item-level
`#[allow(dead_code)]`, plus two rejection tests pinning the type-mismatch
behaviour (an integer `word_count` rejected; a `null` or numeric
`source_work_entry_id` rejected). Where a field is retained for exactly that
reason, the `allow(dead_code)` reason must say so — otherwise the next dead-code
sweep deletes it again.

Keep the rule honest: the same task correctly deleted `validate_slug` (a real
never-called helper; `validate_work_ref` calls the retained `validate_slug_inner`)
and `ScriptTemplate.filename` (private template metadata read only by tests). The
rule is "check what it enforces", not "never delete fields".

Checklist for a field or symbol with no readers:

| Question | Escalate when |
| --- | --- |
| Is the declared type stricter than the schema for the same input (single-variant enum, required `String`, custom `deserialize_with`)? | The value is never read, but its *type* rejects inputs the schema permits |
| If the item were absent, which malformed input becomes accepted? | Any — the deletion widens accepted input |
| Does a test pin that rejection? | No test covers it; the restore must add one |

## Why This Matters

- **"Obsolete" is a claim with a blast radius.** Each wrongly deleted family removes shipped capability (or a supported CLI name) that the keep-policy explicitly protects; each wrongly kept family leaves maintenance burden. Both errors come from acting on titles instead of current sources.
- **The ledger is the memory.** Six months later, the only way to know why a retained family still exists is the adjudicated-keep record with its blocker and trigger.
- **Documentation lag outlives the code.** Sweeping current docs in the same wave (and listing what remains) is what makes a retirement's boundary true for readers, not just for the build.

## When to Apply

- Any host/composition/dependency retirement.
- Before acting on a residual or roadmap claim that a subsystem is dead.
- When a deletion touches shared manifests, lockfiles, CI or docs — the same wave owns all four.

## Examples

### Consumer inventory row shape (verified retain)

```markdown
| Fact | Evidence |
|---|---|
| Default build still enables `legacy-cli` + `web-embed`, linking `nexus-daemon-runtime` | `apps/nexus42/Cargo.toml:236-237,260-261` |
| Release embeds `apps/web/dist` and serves the SPA at `/` | `crates/nexus-daemon-runtime/src/static_assets.rs:3-11,38-45` |
| Vite dev/preview still proxies `/v1/daemon` | `apps/web/vite.config.ts:10-16,26-30` |
```

### Closeout deferral row shape

```markdown
| Item | Blocker / trigger | Tracking location |
|---|---|---|
| <family> | `<residual-id>` (open, <severity>); trigger: <unblock condition> | <roadmap slice>; project register |
```

### External action precondition

```text
1. Confirm each PR is still open and still targets the retired tree.
2. Comment: reason + the retirement merge SHA on main.
3. Close the PRs; record final states and URLs.
Never close a PR merely because a feature worktree deleted files — verified main merge is the precondition.
```
