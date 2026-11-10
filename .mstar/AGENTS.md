# `.mstar/`

Harness-layer **process** directory for this repo. Project identity and tech stack: root [`AGENTS.md`](../AGENTS.md). Runtime lifecycle: upstream `mstar-*` skills — do not copy them here.

## Source priority

1. Current user instruction
2. Root [`AGENTS.md`](../AGENTS.md)
3. This file
4. Upstream `mstar-*` skills

## Invariants

- **Process stays local. Results are shared.** This directory is gitignored except this file. Plans, iterations, SDD, reviews, workflows, projects, payloads, and `store.db` never leave the control checkout. Do not `git add -f` process paths unless the user overrides. Do not name, quote, or link process paths from tracked docs.
- **Results live outside `.mstar/`.** The tracked document corpora are declared in the repo-root [`.mstarc`](../.mstarc):
  - [`../.agents/knowledge/`](../.agents/knowledge/AGENTS.md) — distilled engineering experience and methodology.
  - [`../.agents/adr/`](../.agents/adr/README.md) — one decision among real alternatives, its rationale and consequences.
  - [`../docs/specs/`](../docs/specs/README.md) — the project's current state: layers, boundaries, contracts, data flow, current paths and commands.
- **Catalog is the registry.** Document identity, path, kind, relations, and lifecycle live in [`store.db`](store.db) catalog rows (`mstar catalog list` / `show`). README tables are prose navigation, not registries.
- **`docs/`** is human contributor and integrator documentation. Wire/schema **code** SSOT is repo-root `schemas/`.
- Do not put plan progress, residual prose, or QC narratives in root `AGENTS.md`.
- Waiving a test/QC finding as “pre-existing” requires reproducing it against current `origin/<target_branch>` HEAD.

## Layout

| Path | Tracked | Holds |
| --- | --- | --- |
| `.mstarc` (repo root) | yes | Harness directory declarations: `harness_dir`, `knowledge_dir`, `specs_dir`, `adr_dir` |
| `.mstar/AGENTS.md` | yes | This file |
| `.mstar/plans/` | no | Main plans and durable gate summaries |
| `.mstar/iterations/` | no | Iteration packages (compass, guides, prototypes) |
| `.mstar/sdd/` | no | SDD scratch and review bundles |
| `.mstar/reviews/` | no | Review run directories and evidence |
| `.mstar/workflows/` | no | Workflow notes and reports |
| `.mstar/projects/` | no | Project references and roadmap transport |
| `.mstar/store.db` | no | Issue, catalog, roadmap, milestone and execution authority |

## Anti-patterns

- Treating ignored process files as clone-shared SSOT or team handoff
- Dumping unfinished specs into `knowledge/` or decisions into `docs/specs/`
- Duplicating wire contracts under `docs/specs/` that belong in `schemas/`
- Pointing tracked docs at ignored local files
