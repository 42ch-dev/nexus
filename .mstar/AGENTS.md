# Nexus OSS — Harness Directory (`{HARNESS_DIR}`)

> For project-level rules, tech stack, and domain-specific conventions, see the root [`AGENTS.md`](../AGENTS.md).

## Concepts

| Symbol | Meaning | Path |
|--------|---------|------|
| `{HARNESS_DIR}` | Root of engineering harness (this tree) | `.mstar/` |
| `{PLAN_DIR}` | Plan documents and QC/QA reports | `plans/` |
| `{ITERATION_DIR}` | Iteration-level compass specs (version scope/acceptance/risk) | `iterations/` |
| `{KNOWLEDGE_DIR}` | Knowledge root + [`knowledge/specs/`](knowledge/specs/README.md) (functional/normative specs) | `knowledge/` |

## Upstream Harness

This repo follows the **[Morning Star (mstar-harness)](https://github.com/btspoony/mstar-harness)** framework. Default harness behavior lives in upstream `mstar-*` skills; this file records **project-specific deviations** only.

**Load order (harness work):** Read `mstar-harness-core`, then `mstar-plan-conventions` (+ `mstar-review-qc` when touching `InReview` or QC reports). State machine, QC triple-review timing, and multi-batch rules are **not** duplicated here.

## Reachability

Git-tracked docs and plans must be openable after a fresh `git clone`: no `.gitignore`-d paths, machine-specific absolute paths, or untracked sibling directories as sole authorities. Use repo-relative paths or stable public URLs.

## Content Boundary: `docs/` vs `{ITERATION_DIR}` vs `{KNOWLEDGE_DIR}`

- **`docs/`** (repo root): end-user and contributor documentation (installation, quickstart, architecture overview, contributing). **Do NOT** place architecture review reports, per-plan design decisions, or plan inputs/outputs here.
- **`{ITERATION_DIR}`**: iteration-level specs for a delivery version — including `*-delivery-compass-*.md` and legacy `v1.*` compass artifacts (overview, matrix, program notes). Indexed in [`iterations/README.md`](iterations/README.md).
- **`knowledge/specs/`**: functional and normative OSS specs (migrated from platform `v1-spec/local/`). Index: [`knowledge/specs/README.md`](knowledge/specs/README.md).
- **`{KNOWLEDGE_DIR}`** (root files): cross-cutting rules and trackers only — see [`knowledge/README.md`](knowledge/README.md). Layout: [`knowledge/AGENTS.md`](knowledge/AGENTS.md).

## Pre-merge Checklist (this repository)

1. Update `status.json` (plans, residuals, gates, timeline)
2. Run `pnpm run codegen` and commit regenerated output if `schemas/` changed
3. Update `roadmap.md` in `nexus-platform` if a plan is marked `Done`
4. Archive Done plan rows per `mstar-plan-conventions` (`references/done-compaction.md`, Profile B)

## Project-Specific Deviations

### Multi-plan iteration branches (harness convention)

When an active delivery compass has **two or more** locked implement plans in the **same repo**, this project uses a **two-tier branch model** (aligned with Morning Star `mstar-branch-worktree` — plan integration branch + per-plan topic branches):

| Tier | Field (`status.json`) | Purpose |
| --- | --- | --- |
| **Iteration integration** | `metadata.integration_branch` | Single line where all plan work lands before QC/QA; also the **`Working branch`** in QC/QA Assignments |
| **Final landing** | `metadata.integration_merge_target` | Usually `main`; integration branch merges here via PR after iteration sign-off |
| **Per plan** | `plans[].working_branch` | Topic branch for that plan’s commits only |
| **Per plan** | `plans[].merge_target` | Must equal `metadata.integration_branch` for the same iteration |

**Naming** (extends `metadata.branch_naming.feature`):

- Integration: `feature/<ver>-<iteration-slug>` (e.g. `feature/v1.27-local-authoring`)
- Topic: `feature/<ver>-<plan-slug>` where `<plan-slug>` is the plan title slug without date prefix (e.g. `feature/v1.27-narrative-world-writes`)

**PM / implement rules:**

1. Create the **integration branch** from `integration_merge_target` (typically `main`) before the first plan implement dispatch.
2. Each plan Assignment uses **`Working branch: create <topic-branch> from <integration-branch>`** (or `from` integration `HEAD` after prior plans merged).
3. On plan completion, merge topic branch into **`integration_branch`**; resolve conflicts on the integration branch, not on `main`.
4. Do **not** point QC/QA at a topic branch unless only that plan is in scope for a partial review (exception must be written in Assignment).
5. Same-repo **parallel** plans: one **git worktree** per topic branch; see `mstar-branch-worktree`.

**Single-plan iterations** may use one branch for both roles: set `working_branch` and `integration_branch` to the same name, and omit separate topic branches.

**SSOT:** active compass §Branch policy table + `status.json` for the iteration. If compass and `status.json` disagree, fix before dispatch.

### Plan compaction profile

**Profile B** — Morning Star `mstar-plan-conventions` → `references/done-compaction.md` (Template B). `status.json.plans[]` keeps **non-`Done`** plans only; historical `Done` plans live in the archive.

**Layout invariant** (enforce on every Profile B compaction):

| File | Schema | Content |
|---|---|---|
| `.mstar/status.json` → `plans[]` | array of plan objects | **non-`Done` plans only** (the SSOT for active work) |
| `.mstar/archived/plans-done.json` → `plans` | **array of `plan_id` strings** (e.g. `"2026-06-13-v1.45-harness-docs-prepare"`) | **index only** — every entry MUST be a string, not a dict |
| `.mstar/archived/plans/<plan-id>.json` | one full plan object per file | **single source of truth** for the Done plan's full data (status, qc_reports, merge_commits, completion_report, etc.) |

**Per-iteration closeout checklist** (P-last / Profile B step):

1. For each `Done` plan in `status.json.plans[]`:
   - Read the plan object (`status.json` row)
   - Write a copy to `.mstar/archived/plans/<plan-id>.json` (preserve all fields)
   - Append `"<plan-id>"` (string, **not** the object) to `plans-done.json`'s `plans` array
2. Remove the plan row from `status.json.plans[]` (only non-`Done` plans remain)
3. `iteration_summaries[<ver>]` block stays in `plans-done.json` (delivery snapshot; or move to `shipped-features-tracker.md` §2 — pick one and be consistent)
4. Drop verbose per-iteration `metadata.v1_*_ship` blocks from `status.json` after P-last (history lives in git, [shipped-features-tracker.md](archived/shipped-features-tracker.md) §2, and iteration compasses); keep `metadata.latest_ship` + branch/gate pointers only
5. Verify with `python3 -c "import json; d=json.load(open('.mstar/archived/plans-done.json')); assert all(isinstance(p, str) for p in d['plans'])"`

**Anti-patterns**:

- ❌ Appending the full plan object to `plans-done.json` (one occurrence in V1.45 P-last script; fixed 2026-06-14)
- ❌ Forgetting the per-file JSON (one occurrence in V1.45 P-1 closeout; backfilled 2026-06-14)
- ❌ Mixing strings and dicts in the same `plans` array
- ❌ Editing `archived/plans-done.json` directly when adding a single plan mid-iteration (use the same pattern even for one-off additions)

### Residual detail prose (`plans/residuals/`)

Optional Markdown under `plans/residuals/<plan-id>/`, named `<finding-id>-<short-label>.md`; supplements root `residual_findings` (see upstream `mstar-plan-conventions`). Archive prose with structured JSON to `archived/residuals/<plan-id>.json` when closed.

### Post-merge hotfix pattern (V1.42.1)

When a PR is merged to `main` and post-merge CI exposes a regression, the
canonical recovery flow is:

1. **Surface the regression as a `residual_findings` entry** at the
   `high` or `medium` severity, **before** opening the hotfix branch —
   the user's audit trail must see the regression first, not the fix.
2. Create a fix branch from `main` HEAD (not the integration branch, which
   is now retired). Use the `fix/<short-name>` naming convention (no
   `feature/<ver>-` prefix; hotfixes are version-pinned to current main).
3. Surgical fixes only — pattern-match the bug class, do not refactor
   unrelated code, do not piggyback V1.43 work.
4. Add at least one regression test per bug-class instance. Use
   `handler_state()` (fresh DB) for handler-level tests; pre/post
   `sqlx::query_scalar` to assert on lock state for paths where the
   subject row does not exist.
5. Verify: `cargo test -p <crate> --test <file>` (full file, not just
   one test) + `cargo clippy --all -- -D warnings` (CI command) +
   `cargo +nightly fmt --all --check`.
6. Open a PR; wait for all CI checks (default +1 hour budget).
7. Merge with `--merge` (merge commit, not squash) to preserve
   provenance for the regression audit.
8. Update `status.json`:
   - Add a plan entry with `type: "hotfix"`, the merge commit, the
     full file/function list, the regression tests, and the root_cause
     analysis.
   - Mark the regression `residual_findings` entry as `lifecycle: resolved`
     with `resolution.commit` + `resolution.plan_id`.
   - Add an architectural lesson residual (severity `low`) if the fix
     generalizes to a code class.
9. (Optional) Update the relevant crate's `AGENTS.md` with the rule that
   would have prevented the bug class from being introduced.

### "Pre-existing" claim verification protocol

When a PM-override cites a "pre-existing" failure to justify accepting a
test failure or a QC Request Changes verdict, the claim MUST be verified
against **current `main` HEAD**, not against a stale base commit:

| Step | Action |
|------|--------|
| 1 | Identify the failing test(s) and the failure mode |
| 2 | Run the test against `origin/main` (or `integration_merge_target`) |
| 3 | If the test **passes on current main** → the "pre-existing" claim is **FALSE**; the failure is attributable to the iteration under review |
| 4 | If the test **fails on current main** → the "pre-existing" claim is **TRUE**; document the failure base SHA + reproduce command, then proceed with the PM-override |
| 5 | If the test is **flaky** → use a fixed seed or document the flake rate, do not claim "pre-existing" without a deterministic reproduction |

This protocol was added after the V1.42 P-last PM-override incorrectly
cited `R-V141P0-W2` ("pre-existing works_api failures on c249c902 base")
when the same tests pass on `origin/main` (`e69d2a65`) post-V1.41 merge.
The actual cause was V1.42 P0 T2 wiring (`e8993870`), and the regression
was caught in V1.42 CI (`c0f6cd62`) → fixed in V1.42.1 (`279ec7b3`).
Residual: `R-V142-MERGE-CI-002`.

