# `.agents/` — Code agents (Cursor, Codex, etc.)

This directory is the **workspace skill root** for coding agents and ACP sessions. Tools read skills from `.agents/skills/<slug>/` (optional; presets or setup may create them).

| Path | Role |
|------|------|
| `.agents/skills/` | Project-local skill trees |

## Rules

- Keep agent-facing skills here; do not use this tree for plans, specs, or other project documentation.
- Global skills may live under `~/.nexus42/skills/` and be linked into `.agents/skills/` when a preset requires it.
- **`docs/` MUST NOT contain pinned third-party dependency version text** (e.g. `=0.9.2`, `@0.30.0`, `pinned vX`, a hard-coded release). Versions are the user's/integrator's choice: when a doc references a dependency (spoke-connect, nexus-contracts, …), state that the version is selected by the user to match their runtime/build — never hard-code a specific release. Internal toolchain pins (e.g. the CI rustfmt nightly) are exempt: they are repo-internal consistency constraints, not user-selectable versions.
