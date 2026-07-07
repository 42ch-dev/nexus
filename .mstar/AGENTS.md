# Nexus OSS — Harness Directory (`{HARNESS_DIR}`)

> For project-level rules, tech stack, and domain-specific conventions, see the root [`AGENTS.md`](../AGENTS.md).

## Source priority

1. Current user instruction
2. Root [`AGENTS.md`](../AGENTS.md)
3. This file
4. Upstream `mstar-*` skills (runtime SSOT for harness behavior)

## Concepts

| Symbol | Meaning | Path (this repo) |
|--------|---------|------------------|
| `{HARNESS_DIR}` | Root of engineering harness (this tree) | `.mstar/` |
| `{PLAN_DIR}` | Plan documents and QC/QA reports | `plans/` |
| `{SDD_DIR}` | SDD runtime scratch (gitignored) | `sdd/<plan-id>/` |
| `{ITERATION_DIR}` | Iteration-level compass specs (version scope/acceptance/risk) | `iterations/` |
| `{KNOWLEDGE_DIR}` | Knowledge root (cross-cutting policy + trackers) | `knowledge/` |
| `{SPECS_DIR}` | Frozen functional/normative OSS specs | `knowledge/specs/` (see deviation below) |

## Upstream Harness

This repo follows the **[Morning Star (mstar-harness)](https://github.com/btspoony/mstar-harness)** framework. Default harness behavior lives in upstream `mstar-*` skills; this file records **project-specific deviations** only.

**Load order (harness work):** Read `mstar-harness-core`, then load topic skills **only as needed** (do not read all `mstar-*` skills by default):

| Role / task | After core, typically also read |
|-------------|----------------------------------|
| `@project-manager` | `mstar-dispatch-gates`, `mstar-phase-gates`, `mstar-plan-conventions`, `mstar-plan-artifacts`; iteration work → `mstar-iteration`; parallel Git → `mstar-branch-worktree`; QC dispatch → `mstar-review-qc` |
| Implement / QC / QA / ops | `mstar-coding-behavior` + role ref; Git writes → `mstar-branch-worktree`; plan paths → `mstar-plan-conventions`; status/residual → `mstar-plan-artifacts`; QC/QA → `mstar-review-qc` |
| Leaf executor | Above + **`mstar-dispatch-gates`** (anti-recursion) |

State machine, QC triple-review timing, SDD serial rules, and iteration Phase 1–5 are **not** duplicated here — see upstream skills.

### Editing this file

This file defines **rules and invariants** — it is not a changelog, incident postmortem, or audit trail. When adding or editing a section:

- **Use generic placeholders** in examples (e.g. `<plan-id>`, `{ver}`) — not specific version numbers, plan IDs, or commit SHAs.
- **State the rule**, not the story of how we learned it. Git history preserves provenance; `notes.json` preserves incident narrative.
- **Never record** "first observed in Vx.yz", "fixed on YYYY-MM-DD", "occurred after PR #N", or residual IDs as inline justification. If a section's rationale is non-obvious without context, write a concise one-line rationale in the section body — not an embedded incident report.
- **Anti-patterns** describe the mistake generically — not "one occurrence in Vx.yz P-last script".

## Plans & Reports Layout Invariant

Each plan is a **single `.md` file** under `plans/` — **never** a directory. QC/QA reports live under `plans/reports/<plan-id>/`, **never** as a side-by-side `reports/` subdirectory of a plan-named directory.

| ✅ Correct | ❌ Wrong — never do this |
|---|---|
| `plans/<plan-id>-<name>.md` | `plans/<plan-id>/…` (plan as a directory) |
| `plans/reports/<plan-id>/qc1.md` … `qc3.md` | `plans/<plan-id>/reports/qc1.md` … `qc3.md` |

**Rule**: `plans/reports/` is the **single** reports root. A `plans/<plan-id>/` directory must not exist — the plan itself is the `.md` file.

### Who writes where (implement vs QC)

| Path | Typical writers | Content |
|------|-----------------|--------|
| **`{SDD_DIR}`** (`sdd/<plan-id>/`, gitignored) | **Implementers** (SDD default), PM via `mstar-sdd` scripts, task reviewers (ledger) | `task-N-brief.md`, `task-N-report.md`, `progress.md`, branch review diffs, `implementer-session.json` |
| **`plans/<plan-id>-<name>.md`** | Implementers (checkboxes only), PM, architect, product-manager | Main plan tasks, decisions — **not** per-task SDD bodies |
| **`plans/reports/<plan-id>/`** | **`qc-specialist*`** (QC tri), PM (`qc-consolidated.md`), QA | `qc1.md`…`qc3.md`, `qc-consolidated.md`, `qc.md` (inline/hotfix) — **plan-level L3 audit** |

**Default (`Execution mode: sdd`):** implementors **do not** write implementation artifacts under `plans/reports/`. Per-task handoff lives in **`{SDD_DIR}`** only; dispatch prompts reference file paths there (`mstar-sdd` · `references/file-handoffs.md`). Plan QC runs **after** all tasks pass L2 review, then QC writes to `plans/reports/<plan-id>/`.

**Exceptions:** `Execution mode: inline` / hotfix — no `{SDD_DIR}` scratch; a single `qc.md` may land in `plans/reports/` after delivery. PM may point QC at a review-package diff under `{SDD_DIR}` without copying SDD bodies into reports.

## Reachability

Git-tracked docs and plans must be openable after a fresh `git clone`: no `.gitignore`-d paths, machine-specific absolute paths, or untracked sibling directories as sole authorities. Use repo-relative paths or stable public URLs.

## Content Boundary: `docs/` vs harness subtrees

| Area | Content | Must not |
|------|---------|----------|
| **`docs/`** (repo root) | End-user and contributor documentation (installation, quickstart, architecture overview, contributing) | Architecture review reports, per-plan design decisions, plan I/O |
| **`{ITERATION_DIR}`** | Iteration compass snapshots (`*-delivery-compass-*.md`, legacy `v1.*` artifacts). Index: [`iterations/README.md`](iterations/README.md) | Become permanent spec without P5 merge |
| **`{SPECS_DIR}`** (`knowledge/specs/`) | Frozen functional/normative OSS specs (migrated from platform `v1-spec/local/`). Index: [`knowledge/specs/README.md`](knowledge/specs/README.md) | Runtime knowledge, iteration audit evidence |
| **`{KNOWLEDGE_DIR}`** (root files) | Cross-cutting rules and trackers only — see [`knowledge/README.md`](knowledge/README.md). Layout: [`knowledge/AGENTS.md`](knowledge/AGENTS.md) | Restate normative command/API detail from specs |
| **`{PLAN_DIR}/`** | Main plans + `reports/` (QC/QA audit chain only) | SDD task briefs/reports, implementor scratch |
| **`{SDD_DIR}`** | SDD per-plan scratch (gitignored) — **default implementor write target** | Committed handoff artifacts, QC reports |

**`{SPECS_DIR}` deviation:** Morning Star default resolves repo-root `specs/` (else `designs/`). In this repo, **`{SPECS_DIR}` = `{KNOWLEDGE_DIR}/specs/`** — wire JSON Schema contracts remain in repo-root `schemas/` (see root [`AGENTS.md`](../AGENTS.md)).

## Pre-merge Checklist (this repository)

1. Update `status.json` (plans, residuals, gates, timeline)
2. Run `pnpm run codegen` and commit regenerated output if `schemas/` changed
3. Update `roadmap.md` in `nexus-platform` if a plan is marked `Done`
4. Archive Done plan rows per `mstar-plan-artifacts` (`references/done-compaction.md`, Profile B)
5. **Size gate:** before P-last, verify `wc -c .mstar/status.json` < 20_000; archive all `lifecycle: resolved` findings to `archived/residuals/`. Full clone/worktree/commit discipline: root [`AGENTS.md`](../AGENTS.md) § Git & repository hygiene.

## Project-Specific Deviations

### `status.json` field discipline (narrative vs structured)

`status.json` is **machine-readable structured state only** — the SSOT for active plans, residuals, gates, and iteration pointers. **Narrative belongs in `notes.json`** (append-only timeline), git commit messages, or plan/compass docs — not in `metadata` prose fields.

**Forbidden in `metadata`** (narrative — write to `notes.json` instead):

- ❌ `metadata.<iter>_plan_registration_note` — plan-registration facts live in `plans[]` rows.
- ❌ `metadata.<iter>_carry_forward_index` — residual lifecycle lives in `residual_findings` (and `archived/residuals/` when closed).
- ❌ `metadata.tech_debt_summary.<iter>_ship_note` — narrative ship summaries live in `notes.json` or `archived/shipped-features-tracker.md`.
- ❌ Any new `*_note`, `*_index`, `*_narrative` field whose value is a paragraph of prose.

**Test before adding a field**: if the value is a sentence/paragraph rather than an ID, count, enum, date, or path, it goes in `notes.json`. If the facts it expresses are already derivable from `plans[]`, `residual_findings`, or `archived/plans/<id>.json`, the field is redundant and forbidden.

**Audit trail preservation**: removing a forbidden narrative field never loses information — the underlying facts remain in `plans[]` (structured), `residual_findings` (per-finding lifecycle), `archived/plans/` (per-plan snapshots), and `notes.json` (timeline). Record the removal in `notes.json` for traceability.

### Iteration branch model (Spec integration + per-plan topics)

Formal iterations use Morning Star **`mstar-iteration`** + **`mstar-plan-conventions`** Spec-driven branch metadata. This project’s **two-tier** model (Spec integration line + per-plan topic branches) maps to upstream field names as follows:

| Tier | Field (`status.json`) | Purpose |
| --- | --- | --- |
| **Iteration base** | `metadata.iteration_base_branch` | Ancestor ref used to **create** the Spec integration branch — **not** assumed `main`/`master`; must be explicit in compass + metadata |
| **Spec integration** | `metadata.spec_integration_branch` (plan mirror: `plans[].metadata.spec_integration_branch`) | Single line where all plan work lands before iteration-close; QC/QA **`Working branch`** when reviewing integrated iteration scope |
| **Final PR target** | `metadata.target_branch` | Branch the iteration PR merges into after sign-off (usually `main`; must be explicit) |
| **Per plan** | `plans[].working_branch` | Topic branch for that plan’s commits only |
| **Per plan** | `plans[].merge_target` | Must equal `spec_integration_branch` for the same iteration |

**Legacy aliases (read-only migration):** older rows may still carry `metadata.integration_branch` or `metadata.integration_merge_target`. Treat them as mirrors of `spec_integration_branch` and `target_branch` respectively; **new writes** use the upstream names only.

**Naming (this repo):**

- Spec integration: `iteration/{ver}` (e.g. `iteration/v1.51`)
- Topic: `feature/{ver}-{plan-slug}` where `<plan-slug>` is the plan title slug without date prefix
- Hotfix: `fix/{short-name}`

**PM / implement rules:**

1. **Branch metadata gate:** before first implement dispatch, root `metadata.iteration_base_branch`, `metadata.target_branch`, and plan `metadata.spec_integration_branch` must be recorded (compass frontmatter mirrors the same names). **Do not** silently default to `main` — see `mstar-iteration` §2.3 resolution chain.
2. Create **`spec_integration_branch`** with `git checkout -b <spec_integration_branch> <iteration_base_branch>` (or checkout if it already exists).
3. Each plan Assignment uses **`Working branch: create <topic-branch> from <spec_integration_branch>`** (or `from` integration `HEAD` after prior plans merged).
4. On plan completion, merge topic branch into **`spec_integration_branch`**; resolve conflicts on the integration branch, not on `target_branch`.
5. Do **not** point QC/QA at a topic branch unless only that plan is in scope for a partial review (exception must be written in Assignment). **`Review range` / `Diff basis`** merge-base uses `target_branch` or PM-specified ref — not assumed `origin/main`.
6. Same-repo **parallel** plans: one **git worktree** per topic branch; see `mstar-branch-worktree`.

**Single-plan iterations** may collapse roles: set `working_branch` and `spec_integration_branch` to the same name, and omit separate topic branches.

**SSOT:** active compass §Branch policy table + `status.json` for the iteration. Resolution order: `status.json` metadata → compass frontmatter → ask user. If compass and `status.json` disagree, fix before dispatch.

### Plan compaction profile

**Profile B** — Morning Star `mstar-plan-artifacts` → `references/done-compaction.md` (Template B). `status.json.plans[]` keeps **non-`Done`** plans only; historical `Done` plans live in the archive.

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

- ❌ Appending the full plan object to `plans-done.json` (must be plain `plan_id` strings only)
- ❌ Forgetting the per-file JSON in `archived/plans/<plan-id>.json`
- ❌ Mixing strings and dicts in the same `plans` array
- ❌ Editing `archived/plans-done.json` directly when adding a single plan mid-iteration

### Residual detail prose (`plans/residuals/`)

Optional Markdown under `plans/residuals/<plan-id>/`, named `<finding-id>-<short-label>.md`; supplements root `residual_findings` (canonical schema: `mstar-plan-artifacts/references/status-and-residuals.md`). Archive prose with structured JSON to `archived/residuals/<plan-id>.json` when closed.

### Post-merge hotfix pattern

When a PR is merged to `main` and post-merge CI exposes a regression, the
canonical recovery flow is:

1. **Surface the regression as a `residual_findings` entry** at the
   `high` or `medium` severity, **before** opening the hotfix branch —
   the user's audit trail must see the regression first, not the fix.
2. Create a fix branch from `main` HEAD (not the integration branch, which
   is now retired). Use the `fix/<short-name>` naming convention (no
   `feature/<ver>-` prefix; hotfixes are version-pinned to current main).
3. Surgical fixes only — pattern-match the bug class, do not refactor
   unrelated code, do not piggyback other in-flight work.
4. Add at least one regression test per bug-class instance.
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
| 2 | Run the test against `origin/main` (or `metadata.target_branch`) |
| 3 | If the test **passes on current main** → the "pre-existing" claim is **FALSE**; the failure is attributable to the iteration under review |
| 4 | If the test **fails on current main** → the "pre-existing" claim is **TRUE**; document the failure base SHA + reproduce command, then proceed with the PM-override |
| 5 | If the test is **flaky** → use a fixed seed or document the flake rate, do not claim "pre-existing" without a deterministic reproduction |

