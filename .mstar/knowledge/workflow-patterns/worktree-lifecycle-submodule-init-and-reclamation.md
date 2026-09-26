---
module: developer worktree lifecycle (submodule metadata, concurrency budget, reclamation)
date: 2026-09-25
problem_type: workflow_issue
category: workflow-patterns
severity: high
plan_id: 2026-09-25-v1.197-p1-worktree-submodule-gitdir
applies_when:
  - creating or automating a linked worktree in a repository that carries submodules
  - deciding how many development tracks may run concurrently
  - reclaiming a finished track's worktree, cargo cache or temporary build footprint
  - diagnosing a cleanup probe that refuses a worktree, or a submodule that resolves in the main checkout but not in a linked one
tags:
  - git-worktree
  - submodule
  - resource-budget
  - reclamation
  - convergence
  - envrc
  - cargo-target-dir
---

# Worktree lifecycle: native submodule initialization, resource-budget concurrency, guarded reclamation

## Context

Nexus runs development work in linked worktrees under `.worktrees/<name>/`, each with its own scoped `CARGO_TARGET_DIR` (see the scoped-cargo-target-dir doc). Two things quietly broke that lifecycle and are now handled by checked-in tooling:

1. A **linked worktree's submodule metadata** can be absent or wrong, so `git status` inside it aborts and every cleanup probe refuses the checkout (fail-closed, so the track sticks around forever).
2. **Nothing reclaimed a finished slice.** An iteration that finished weeks earlier still held tens of gigabytes of target caches; six concurrent targets once consumed 98 GiB of `/tmp` in a single iteration and degraded the host.

The rule that replaced them is a **resource budget plus immediate reclamation**, not a cap on worktree count. The repository now owns two scripts and a published checklist; this doc records the invariant behind them.

## Guidance

### Initialize linked-worktree submodules with native Git, never by transplanting metadata

After `git worktree add`, run the checked-in initializer:

```sh
node scripts/init-worktree-submodules.mjs --worktree <absolute-checkout>
```

It discovers administrative directories through Git itself (`rev-parse --absolute-git-dir`, `git worktree list`, `git config`), so each checkout gets **its own** submodule gitdir and index; it initializes only submodules that are still missing; and it validates already-initialized ones. The raw `git submodule update --init --recursive` is what the initializer runs internally for missing paths — it is **not** an equivalent step to run instead, because it does not validate metadata, does not refuse a copied pointer, an out-of-subtree gitdir or an unmerged index, and would reset a deliberately different submodule HEAD.

Repeated calls are validated no-ops that preserve an intentionally different HEAD, and failures fail closed with a JSON refusal on stdout. The refusal distinguishes two states: `init.refuse.preflight` (this run attempted no mutation — either it refused before touching anything with `initialized_paths: null`, or the native command completed zero submodules, which reports `initialized_paths: []`) and `init.refuse.partial` (at least one submodule was initialized by this run and is **not** rolled back — `initialized_paths` names them). Re-running validates that subset and completes the rest.

### Decide concurrency from resources and ready work, not from a worktree count

```
K = min(ready independent tasks, floor(disk budget / per-track target estimate), max(1, cores / 2))
```

Round down, and re-measure every iteration and at each rescheduling checkpoint — the number is not a standing limit. Before opening another track, require **root free ≥ 90 GiB** and **total feature targets ≤ 120 GiB**; if either fails, reclaim first and re-measure rather than refusing to open the track. On this host the measured fields (10 cores, 32 GiB RAM, 20 GiB per track, ~120 GiB budget) gave `K = 2` with two ready plans and `K = 4` once four independent tasks were ready — the binding term was ready work, never disk.

Parallelism is not only for development: pass each task's review, merge incrementally into the plan's integration branch (merges stay serial but are cheap), then let the tri-QC seats and QA run on one frozen HEAD in parallel. The integration merge is the only hard serial point; it must never be widened into "plans run one at a time".

### Reclaim inside the slice, and prove it

Whoever creates a footprint reclaims it, in the **same slice** in which its owner completes: the feature worktree, its scoped target dir, and the temporary build products it produced under `/tmp` and `~/.cache` (not only `.worktrees/` and per-track targets). Evidence has two scopes:

- **Per slice** — the slice's own worktree, target and temporary paths are gone, `git worktree list` no longer lists it, and a per-track completion check passes.
- **Convergence** — with no peer feature in flight, only the main checkout and the integration worktree remain. Asserting this while a peer is still active is wrong evidence.

The repository sweeper turns both scopes into machine checks: `scripts/worktree-sweep.mjs --repo <root> --harness <dir> --workflow <id> --inventory <json>` (dry-run by default, `--check-exit <track-id>` and `--check-convergence` read-only, explicit `--apply`). Run its dry-run at rescheduling checkpoints; a dry-run proposal is **never** authorization. Its capacity block also reports `computed_k` and the watermark verdicts, so scheduling and reclamation evidence come from one place.

### The two obstacles that a non-forced removal must survive

Both were measured on this repository and both look like failures until you know them:

1. **A submodule-carrying worktree cannot be removed by `git worktree remove`** — `fatal: working trees containing submodules cannot be moved or removed`, and the refusal survives `git submodule deinit --all` and even dropping the gitlink from that worktree's index. Worse, `deinit` rewrites the **shared** superproject config, unregistering the submodule clone-wide. The documented non-force route is: prove merged + clean + released → `rm -rf <exact worktree path>` → `git worktree prune` → re-observe.
2. **An ignored-only build footprint also reads as dirty.** A checkout whose tracked tree is clean but which holds ignored outputs (a `node_modules/` or `dist/` left by a test hook) is refused as dirty. Enumerate the exact ignored paths and sizes as evidence, then take the same exact-path non-force route. Never `--force`, never a wildcard, never a guessed path.

A third, subtler one: a file attributed `filter=lfs` in `.gitattributes` while the committed blob is the raw binary reports as modified in **every fresh checkout** (the LFS clean filter turns it into a pointer for comparison). The bytes equal the committed blob, so it is an artifact, not an edit — never `git add -A` on such a tree, and never "fix" it by committing the pointer form.

### Branch cleanup belongs to the workflow-level checkpoint

After the non-force worktree route, the engine can no longer match the pruned path, so that track's branch legitimately stays behind with a `retained` verdict. Branch deletion is the workflow-level cleanup's job (the installed engine deletes merged branches once the owning plan row is `Done`), not the sweeper's — do not widen the tool into a workflow-wide scan to "finish the job".

## Why This Matters

- **The guard is the reclamation gate, not a quota.** A finished slice leaves nothing behind, so concurrency is safe exactly when reclamation is immediate and provable; capping worktree count would have hidden the real defect (unfinished slices) while still leaking 76 GiB after two completed iterations.
- **Fail-closed ownership beats convenient deletion.** A branch claim is not evidence of path ownership; the sweeper refuses to delete unless the snapshot's retained row proves the path, and it re-reads lease/merge/producer facts immediately before every mutation. That is why the plan-QC tri-review accepted the tool only after two corroborated Criticals were fixed.
- **Evidence must come from the artefact, not from prose.** Convergence for this iteration was signed off by running the sweeper's own `--check-convergence` (exit 0, no reasons) after the engine had deleted every merged branch — not by a PM assertion.

## When to Apply

- Any plan that creates, uses or reclaims a worktree in a repository with submodules.
- Any scheduling decision about how many tracks to run, or any disk-pressure symptom during a multi-track iteration.
- Any cleanup refusal on a worktree (`dirty-worktree`, `non-terminal`, `blocked` submodules) — check the two obstacles above before reaching for `--force`.
- Before declaring an iteration or plan formally complete: run the per-slice check, then the convergence check once no peer feature is in flight.

## Examples

```sh
# initialize a fresh worktree (required immediately after git worktree add)
node scripts/init-worktree-submodules.mjs --worktree "$PWD"

# checkpoint sweep (read-only): capacity, watermarks, per-track verdicts
node scripts/worktree-sweep.mjs --repo <main-root> --harness <control-harness> \
  --workflow <iteration-id> --inventory <inventory.json>

# per-slice and convergence completion checks
node scripts/worktree-sweep.mjs … --check-exit <track-id>
node scripts/worktree-sweep.mjs … --check-convergence

# reclaim an owned slice (only after merged + clean + released)
rm -rf ~/.cache/nexus-target-<track>            # scoped cache
rm -rf "$WT" && git worktree prune              # documented non-force route
```

Policy text and the full checklist live in the root `AGENTS.md` and `docs/CONTRIBUTING.md`.
