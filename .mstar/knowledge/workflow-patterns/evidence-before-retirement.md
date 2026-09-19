---
module: repo-wide (obsolete host/composition retirement), CI/lockfile/docs
date: 2026-09-20
problem_type: workflow_issue
category: workflow-patterns
severity: medium
plan_id: 2026-09-19-v1.192-p2-obsolete-host-retirement
applies_when:
  - retiring a host, composition, workflow, dependency tree or CLI family and answering "is this actually obsolete?"
  - a residual, roadmap item or task title claims a family is dead code — before acting on that claim
  - writing or reviewing a retirement closeout / documentation sweep
related_components:
  - apps/desktop (retired)
  - apps/nexus42
  - .github/workflows
  - docs / specs / knowledge surfaces
tags:
  - retirement
  - consumer-inventory
  - obsolescence
  - deferral
  - closeout-ledger
  - documentation-lag
  - dead-code
  - lockfile
---

# Evidence before retirement — inventory consumers, adjudicate the keeps, ledger the closeout

## Context

v1.192 retired the replaced Tauri desktop composition (`apps/desktop/**`, `scripts/fetch-sidecar.sh`, the Tauri release workflow, `@tauri-apps/*` and the `desktop` workspace package) after the Electron host was accepted — one desktop host must remain. In the same iteration, two neighboring families were **not** deleted:

- the legacy integrated daemon + Daemon API + embedded SPA, and
- dormant/hidden CLI route rows,

both of which *look* obsolete but still have live consumers. The retirement round's most reusable output is the discipline that separates the two outcomes: a source-reverified consumer inventory, an adjudicated-keep record, and a closeout ledger that makes documentation lag a tracked quantity instead of a discovered surprise.

## Guidance

### 1. Inventory consumers against current HEAD sources before calling anything obsolete

A residual title, a roadmap sentence or a plan's intention is **not** evidence. For each candidate family, re-verify against current sources at a recorded baseline revision, per fact, with `file:line` evidence, in two passes:

1. **Is it still wired?** (default features, boot paths, release embedding, contract-bound clients, dev proxies, API call sites).
2. **Does anything still call it?** (CLI leaves, scripts, workflows, dependabot entries, docs-as-contract).

Concrete v1.192 findings worth imitating: default features were still `legacy-cli` + `web-embed`; `daemon start` still booted the integrated daemon; the release still embedded and served the SPA; `BrowserClient` was contract-bound to `/v1/daemon/*`. The open residual `R-V1190-P6T4-REMAINING-LEAVES` remained the named blocker. Note the trap: the residual's detail text partly described a *working tree ahead of `main`* — inventory against `main`, not against the plan branch that proposed the deletion.

### 2. Dormant ≠ dead: adjudicate callable surfaces row by row

Every "dormant" CLI row existed in code with a clap definition and a `run` path — hard-reject stubs (`creator workspace clone`), coming-soon prints (`workspace link|unlink|status`, `publish`), config-error/exit-2 stubs (`explore browse|search`, `platform context assemble`), and hidden-but-callable aliases (`sync`, `preset`, `capability`). One row was even *visible* while the spec said hidden. Deletion of such rows requires a per-row disposition plus help/docs/parity proof — "no callers in tests" is not obsolescence.

### 3. Sequence deletion after the replacement passes acceptance

The Tauri deletion was correct only because its replacement (the Electron host) had passed its own gates; the retirement commit stayed separable and revertable on the integration branch. Deleting a shipped composition before the replacement is accepted trades a working product for an unverified one — this is a sequencing rule, not a style preference.

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

Before declaring the closeout done, grep for executable callers of every deleted script/directory across owned scopes **and** outside them. v1.192's out-of-scope find: a closed measurement harness still spawned the deleted `scripts/fetch-sidecar.sh` for a surface that was already non-functional at baseline. The disposition (remove the broken surface rather than restore the deleted script) belongs to PM; the *finding* belongs to the closeout. Stale comment-only citations and historical records are fine; live executable references are not.

### 8. Lockfile and workspace hygiene are part of the deletion evidence

For dependency retirements: regenerate with `--lockfile-only`, run it **twice** (idempotence: identical diff), confirm the removed content is exactly the retired family, confirm unrelated dependencies survive (the `sharp` entry was preserved for the *new* host while `@tauri-apps/*` and the `desktop` importer disappeared), then prove `--frozen-lockfile` "Already up to date" and a **cold-state install** with the family's packages absent from `node_modules`. An export-surface diff on shared scripts (e.g. 42 → 39 exports, "shared unchanged: true") plus one smoke of the retained dispatch is cheap proof that the cut didn't take neighbors with it.

### 9. Gate external actions on the verified main merge

Closing dependabot PRs (#310/#312) or any external cleanup executes only **after the retirement verifies merged to `main`** — never because a feature worktree deleted the files. The preconditions are explicit: confirm the PRs still target the retired tree, comment with the reason and the merge SHA, close, record final states/URLs as post-merge receipts. Related post-merge obligations (branch-protection required check names, CI green runs) are recorded at closeout even when they execute later.

## Why This Matters

- **"Obsolete" is a claim with a blast radius.** Each wrongly deleted family removes shipped capability (or a supported CLI name) that the keep-policy explicitly protects; each wrongly kept family leaves maintenance burden. Both errors come from acting on titles instead of current sources.
- **The ledger is the memory.** Six months later, the only way to know why the daemon/SPA composition still exists is the adjudicated-keep record with its blocker and trigger.
- **Documentation lag outlives the code.** Sweeping current docs in the same wave (and listing what remains) is what makes "exactly one desktop host" true for readers, not just for the build.

## When to Apply

- Any host/composition/dependency retirement, including the future daemon/SPA and dormant-CLI families whose blockers are recorded.
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
| Legacy daemon/SPA composition deletion | `R-V1190-P6T4-REMAINING-LEAVES` (open, high); trigger: default-feature switch completed on main + consumers at zero + new inventory | roadmap RFT-11 slice; project register |
```

### External action precondition

```text
1. gh pr view 310 / 312 — confirm still open and targeting the retired tree.
2. Comment: reason + the retirement merge SHA on main.
3. gh pr close 310 / 312; record final states and URLs.
Never close a PR merely because a feature worktree deleted files — verified main merge is the precondition.
```

## Evidence

- Product-side retirement — deleted `apps/desktop/**` (21 tracked paths), `scripts/fetch-sidecar.sh`, `scripts/dev-backend-manifest.mjs` sidecar helpers; `.github/workflows/desktop-release.yml` removed and the Tauri `desktop-build` job stripped in `.github/workflows/desktop-build.yml` / `ci.yml`; lockfile 17 → 16 workspace projects with `sharp` preserved.
- Retained/adjudicated families — `apps/nexus42` default features, daemon boot path, embedded SPA serving, and the dormant CLI rows listed in `.mstar/specs/rust-core-service-boundary.md` §7.4.
- Policy texts — `.github/dependabot.yml` (all remaining directories verified at HEAD), `.oxlintrc.json` (dead override removed), `.mstar/specs/desktop-shell.md` header, `.mstar/specs/README.md` rows.
- Companion docs — [resolved-residual-verification.md](../architecture-patterns/resolved-residual-verification.md) (verify residual claims against current `main`), [surface-rename-hygiene-checklist.md](../conventions/surface-rename-hygiene-checklist.md) (sweep shape for cross-language renames), [capability-parity-receipt.md](capability-parity-receipt.md) (the acceptance-evidence counterpart for this cutover).
