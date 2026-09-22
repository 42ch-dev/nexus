---
module: oxlint type-aware lint toolchain (root pnpm workspace, .oxlintrc.json, .github/workflows/ci.yml)
date: 2026-09-21
problem_type: tooling_decision
category: tooling-decisions
severity: medium
plan_id: 2026-09-21-v1.194-p0-lint-gate
applies_when:
  - Sizing a warning/lint clean-up against a count someone observed earlier
  - A lint or type-check run reports type-dependent families (or `error`-type findings) that a re-read of the source does not explain
  - Deciding between repairing a lint row, adding an override, or tracking a residual
  - Choosing the invocation scope for the CI lint gate, or reading a file-scoped lint count
tags:
  - oxlint
  - tsgolint
  - lint-baseline
  - degraded-environment
  - types-node
  - tsconfig-error
  - build-order
  - evidence-capture
---

# A lint/warning baseline is only a baseline if it was measured in a healthy checkout

## Context

A zero-warning lint goal needs a number to aim at. One was available: an earlier research run of `node_modules/.bin/oxlint .` on the baseline revision reported **88 warnings**, against a historical residual that said 59. Nothing in that report looked wrong — node `v22.23.1`, pnpm `11.25.0`, the same root script (`lint` = `oxlint .`), the same configuration.

Re-measuring the same revision in a fresh worktree changed the target by a third. In a CI-equivalent environment (`pnpm install --frozen-lockfile` plus the contracts build the workflow runs before its Lint step) the same command reported **56 diagnostics, all warnings, 0 errors, 24 files, exit 0** — reproduced with an identical diagnostic row set on re-runs. The 88 was real, but it was a property of the checkout it was taken in, not of the repository.

The decomposition was complete and additive, which is exactly why the original number was believable:

| Bucket | Count | Nature |
|---|---|---|
| Present in both captures | 55 warnings | environment-independent findings — the actual work |
| Present only in the main checkout | 33 warnings (27 `no-redundant-type-constituents` in 16 files, 5 `require-array-sort-compare` in web tests, 1 `no-implied-eval`) | environment artifacts |
| Error-severity rows present only in the main checkout | 68 (65 `prefer-nullish-coalescing`, 3 `tsconfig-error`) | environment artifacts; `oxlint .` exits 1 there |
| Present only in the healthy worktree | 1 warning (`apps/desktop-electron/src/utility-host.ts:41`) | build-order artifact |

`55 + 33 = 88` reproduces the observed baseline exactly.

Root cause of the artifacts (observed chain, with one labelled inference): the main checkout's per-package `@types/node` links **dangle** — `test -e` exits 1 for all three and they still point at the pruned pnpm entry for the pre-bump version — while the freshly installed worktree resolves them inside its own `node_modules`. With byte-identical `tsconfig.json` (md5 equal across checkouts, so configuration is not the variable), `tsc -p <tsconfig> --noEmit` reports `error TS2688: Cannot find type definition file for 'node'` for `apps/desktop-electron`, `packages/nexus-native` and `apps/nexus-service` (which `extends` the native one), and the type-aware engine reports `error creating TS program` for exactly those three projects. *Inferred (mechanism, not directly observed):* a type-aware pass that cannot resolve a module reports that module's symbols as error types, which fires the families that depend on them. What the retained captures establish directly is the correlation at one revision — the 27 `no-redundant-type-constituents` rows and the 65 `prefer-nullish-coalescing` errors exist only in the main capture. The web project isolates it further: in main, `apps/web`'s lint program *is* created (exit 0) even though `tsc -p apps/web` exits 1 with 7205 `error TS` diagnostics starting at `Cannot find module 'react' / 'react-i18next' / 'react-router' / '@testing-library/react' / '@vitejs/plugin-react'` — main's web program has no React types at all — and that same run reports 22 NRTC rows plus 59 `prefer-nullish-coalescing` occurrences; in the worktree the same two commands report 0 and `tsc` is silent.

The single healthy-environment row had a different cause again: `apps/desktop-electron/src/utility-host.ts:41` imports a type from `@42ch/nexus-service`, whose `types` point at `dist/index.d.ts`; in a CI-equivalent checkout that package's declarations are not built, so the import resolves to an error type. Building the workspace type dependencies in dependency order removes the row — no rule and no file has to be disabled for it.

Decisions taken from the measurement (and the reason this is a *decision* record, not a war story): the React/ReactFlow `error`-type family is **not** a tool false positive on valid input and gets no override (`apps/web/src/components/ui/table.tsx`, the representative file, has five rows in the degraded capture and zero in the healthy one); the close-out baseline is the measured 56, not the 88; the single build-order row is repaired at the build step; the 28 `no-console` rows are the existing bounded tooling/CLI stdout policy extended **per exact file** (a 14-path `overrides[].files` list under the single `no-console` rule — no directory or workspace glob); the main checkout's `node_modules` needs a reinstall, which is an environment repair outside the lint work's write scope and is reported, not edited.

Landed gate (`typescript-checks` job): build workspace type dependencies in dependency order, then `pnpm run lint` (root `oxlint .`) with `options.denyWarnings: true`, so any warning fails the job; the path filter was widened until 797/797 linted files are covered.

## Guidance

1. **Anchor a reported count to (command, revision, install precondition, toolchain pins) before it becomes a target.** The healthy capture is only comparable because it records `pnpm exec oxlint --format json .`, the revision, `pnpm install --frozen-lockfile`, the pre-lint build step, oxlint `1.83.0`, oxlint-tsgolint `7.0.2001`, typescript `7.0.2`, node `v22.23.1`, pnpm `11.25.0`, and the tool's own scope line (797 files, 86 rules). A number without that tuple is an anecdote.
2. **Reproduce in a CI-equivalent environment before sizing work.** A fresh `--frozen-lockfile` install plus whatever build the workflow runs before the step. In this repo the pre-lint step is a workspace package build; skipping it changes both what resolves and what is reported.
3. **Detect the degraded state before trusting the number.** The tell is *error-severity* or *program-level* failure, not a warning count: type-dependent families firing on files that read fine, plus `tsconfig-error` rows. Recipe per affected project: `tsc -p <tsconfig> --noEmit` (`error TS2688` = missing type library), `tsgolint --tsconfig <tsconfig> --list-files` (`error creating TS program`), `readlink -f` on the package's `node_modules/@types/node` (a dangling link prints nothing / exits non-zero), and md5 of the `tsconfig` across checkouts to rule configuration out.
4. **Attribute the delta bucket by bucket, and only accept the arithmetic if it closes.** "55 environment-independent + 33 main-only = 88" is the form that turns a suspicious number into a diagnosis. A bucket you cannot name is a bucket you have not explained.
5. **Classify each row before choosing a disposition.** Environment artifact → no edit, no override, no residual (repair the environment). Build-order artifact → repair the build step. Real finding → repair. Override only when the root cause cannot be repaired, scoped to the smallest item that works, with a tracked residual. Here, the per-file `no-console` list is the existing bounded policy applied to individually evidenced files; the four pre-existing `no-redundant-type-constituents` file exceptions stayed untouched; no `apps/web/**` override was created.
6. **Keep the gate on the root invocation.** `oxlint .` is what CI runs and what the baseline describes. Narrower scopes are not comparable: on pristine, identical content and configuration, `require-array-sort-compare` fired on `apps/desktop-electron/scripts/proof-contract.mjs:469` under directory-, file-list- and single-file scope but was absent from the root run (`:264` only). The mechanism is type-aware scope: that path lies outside every tsconfig `include`, so the type context differs by invocation shape (which exact program each mode builds is internal to the tool and was not observable). Size work from the root run; treat a file-scoped count as a different measurement.
7. **Do not claim byte-for-byte reproducibility you did not get.** Independent re-runs produced identical diagnostic row *sets* (equal multiset, compared programmatically) with different capture *bytes* — diagnostic row order and the wrapper `start_time` differ. Say "identical diagnostic row sets"; a hash of a capture is provenance, not a reproducibility claim.
8. **Retain the causal evidence as raw, hash-pinned captures.** Every capture records checkout, `HEAD`, `git status --short`, an environment fingerprint (here: the `@types/node` link targets and the presence of the gitignored workspace `dist` outputs), the verbatim command, its exit code, stdout and stderr; hashes live in the machine-readable inventory. This was not a nicety: the first round's artifact conclusion rested on prose and was raised as a review finding, and after the captures existed, re-review narrowed three prose claims that the captures did not support (e.g. the worktree `tsc` runs are *not* uniformly silent — `nexus-native` exits 0, while `desktop-electron` exits 1 with four `TS2307` rows for unbuilt workspace packages, a different root cause from the dangling types).

## Why This Matters

- **Sizing against an unreproducible baseline wastes rounds and invites mass suppression.** An 88-warning target with 16 unexplained React files is how a whole directory gets an override; the same target measured properly is 55 real rows, one build order, one policy extension.
- **A degraded environment produces *plausible* findings, not obviously broken ones.** The React/ReactFlow `error`-type family looked like a textbook tool false positive — 21 rows of the shape `'X' is an 'error' type …` — and was a missing type library. Declaring a false positive there would have suppressed real diagnostics for every project whose program could not be created.
- **Scope honesty cuts both ways.** The healthy-environment claim ("affected sources resolve cleanly") holds only for the environment that was measured; in the degraded checkout the same compiler invocation emits 7205 errors. State which environment a verdict describes.
- **A wrong disposition outlives the iteration.** An override added for an environment artifact is invisible debt: it keeps suppressing after the environment is repaired, and nothing points back at the mistake.

## When to Apply

- Before writing a "clean up N warnings" task, a gate threshold, or a CI `denyWarnings` flip.
- When a lint/type run reports families concentrated in files that read correctly, or any `tsconfig-error` / "cannot create program" row.
- When deciding override vs repair vs residual for a family, and when auditing an existing override that has no recorded cause.
- When CI and a developer machine disagree on lint or type-check results (compare `pnpm --version`, install freshness, and the type-lib link targets first).

## Examples

### Healthy measurement (the shape to reuse)

```bash
pnpm install --frozen-lockfile
pnpm --filter @42ch/nexus-contracts run build        # exactly the step CI runs before Lint
pnpm exec oxlint --format json . > evidence.json    # authoritative: 56 warnings, 0 errors, exit 0
pnpm run lint                                       # same run, human-readable
```

### Degraded-state probe (read-only; three commands name the cause)

```bash
node_modules/.bin/tsc -p apps/desktop-electron/tsconfig.json --noEmit
#   degraded: error TS2688: Cannot find type definition file for 'node'.
#   healthy:  no TS2688
node node_modules/oxlint-tsgolint/bin/tsgolint.js --tsconfig apps/desktop-electron/tsconfig.json --list-files
#   degraded: error creating TS program      healthy: programs created, findings printed
ls -l apps/desktop-electron/node_modules/@types/node packages/nexus-native/node_modules/@types/node
#   degraded: link target under a pruned .pnpm entry (test -e exits 1 → dangling)
#   healthy:  link resolves inside the checkout's node_modules
```

### Invocation-scope control (why the gate is the root run)

```bash
oxlint --format json .                                             # proof-contract.mjs: :264 only
oxlint --format json apps/desktop-electron/scripts/                # :264 and :469
oxlint --format json apps/desktop-electron/scripts/proof-contract.mjs   # :264 and :469
```

### Build-order row, repaired at the build step

```yaml
# .github/workflows/ci.yml — typescript-checks, before the Lint step
- name: Build workspace type dependencies
  run: |
    pnpm --filter @42ch/nexus-native run build
    pnpm --filter @42ch/nexus-provider-acp run build
    pnpm --filter @42ch/nexus-service run build
```

## Evidence

- Measurement: `pnpm exec oxlint --format json .` at `c74e0274e91d65a131def0fbb0e39a742dad7a0b` — healthy 56 warnings / 0 errors / 24 files / exit 0; main checkout 156 rows (88 warnings / 68 errors) / exit 1; toolchain pins and the tool's 797-file scope recorded with each capture.
- Root-cause captures: `apps/{desktop-electron,nexus-service}/node_modules/@types/node` + `packages/nexus-native/node_modules/@types/node` link targets (`test -e` / `readlink -f`), `tsc -p` and `tsgolint --list-files` per project in both checkouts, `md5` of the three `tsconfig.json` across checkouts, dependency-ordered build isolation of the build-order row.
- Landed state: `.oxlintrc.json` (`options.typeAware: true`, `options.denyWarnings: true`, the 14-file per-file `no-console` exception list, the four pre-existing NRTC file exceptions unchanged, no `apps/web/**` override); `.github/workflows/ci.yml` `typescript-checks` (dependency-ordered type build → Lint; path filter covering 797/797 linted files). Final `pnpm run lint`: zero warnings.
- Sibling discipline: [deliberate-guard-scope-and-clippy-allow.md](../conventions/deliberate-guard-scope-and-clippy-allow.md) (the Rust-side rule for classifying a lint finding as repair-vs-suppress and for scoping an `#[allow]`), [pnpm-toolchain-pin-and-supply-chain-age.md](../conventions/pnpm-toolchain-pin-and-supply-chain-age.md) (install/toolchain preconditions that change what a local run measures), [capability-parity-receipt.md](../workflow-patterns/capability-parity-receipt.md) (`[UNVERIFIED]` discipline for claims whose evidence was never produced — the same rule that keeps "CI fails on any warning" unverified until a workflow run exists).
