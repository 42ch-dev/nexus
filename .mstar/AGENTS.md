# `.mstar/`

Harness-layer SSOT for this repo. Project identity and tech stack: root [`AGENTS.md`](../AGENTS.md). Runtime lifecycle: upstream `mstar-*` skills — do not copy them here.

## Source priority

1. Current user instruction
2. Root [`AGENTS.md`](../AGENTS.md)
3. This file
4. Upstream `mstar-*` skills

## Invariants

- **Process stays local. Results are shared.** Tracked here: this file, [`specs/`](specs/), [`knowledge/`](knowledge/). Everything else under `.mstar/` is process (gitignored). Do not `git add -f` process paths unless the user overrides. Do not name, quote, or link process paths from tracked docs.
- **`specs/`** is normative OSS behavior. **`knowledge/`** is distilled cross-iteration policy — not a second specs tree. Layout: [`specs/AGENTS.md`](specs/AGENTS.md), [`knowledge/AGENTS.md`](knowledge/AGENTS.md).
- **`docs/`** is human contributor docs. Wire/schema **code** SSOT is repo-root `schemas/`.
- Do not put plan progress, residual prose, or QC narratives in root `AGENTS.md`.
- Waiving a test/QC finding as “pre-existing” requires reproducing it against current `origin/<target_branch>` HEAD.

## Anti-patterns

- Treating ignored process files as clone-shared SSOT or team handoff
- Dumping unfinished specs into `knowledge/`
- Duplicating wire contracts under `specs/` that belong in `schemas/`
- Pointing tracked docs at ignored local files
