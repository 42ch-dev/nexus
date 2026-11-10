# `.agents/`

Workspace root for agent-facing shared results and skills (ACP workspace skill root).

Project and crate rules: root [`AGENTS.md`](../AGENTS.md). Harness: [`.mstar/AGENTS.md`](../.mstar/AGENTS.md).

## Contents

| Path | Holds |
| --- | --- |
| [`skills/`](skills/) | Agent skills (`<slug>/SKILL.md`) |
| [`knowledge/`](knowledge/) | Distilled, reusable engineering knowledge — patterns, lessons, how-to; **not** current-state descriptions |
| [`adr/`](adr/) | Architecture decision records — one decision, its real alternatives and consequences, filed by fate and domain class |

## Invariants

- `knowledge/` and `adr/` are **tracked results** shared across clones; `skills/` is the agent skill root.
- Current-state specifications live in [`docs/specs/`](../docs/specs/), not here.
- This tree is **not** harness process state or plans — those stay in the gitignored `.mstar/` control directory.
- Do not duplicate `mstar-*` runtime rules or repo identity here.

Skills may be absent on a fresh clone; recreate from presets or setup as needed.
