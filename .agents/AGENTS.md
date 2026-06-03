# `.agents/` — Code agents (Cursor, Codex, etc.)

This directory is the **workspace skill root** for coding agents and ACP sessions. Tools read skills from `.agents/skills/<slug>/` (optional; presets or setup may create them).

| Path | Role |
|------|------|
| `.agents/skills/` | Project-local skill trees |

## Rules

- Keep agent-facing skills here; do not use this tree for plans, specs, or other project documentation.
- Global skills may live under `~/.nexus42/skills/` and be linked into `.agents/skills/` when a preset requires it.
