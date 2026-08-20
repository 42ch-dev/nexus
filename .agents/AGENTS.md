# `.agents/`

Workspace skill root for coding agents and ACP sessions.

Project and crate rules: root [`AGENTS.md`](../AGENTS.md). Harness: [`.mstar/AGENTS.md`](../.mstar/AGENTS.md).

## Invariants

- This tree holds **agent-facing skills** only (`skills/<slug>/`).
- It is **not** harness SSOT, plans, specs, knowledge, or contributor docs.
- Do not duplicate `mstar-*` runtime rules or repo identity here.

Skills may be absent on a fresh clone; recreate from presets or setup as needed.
