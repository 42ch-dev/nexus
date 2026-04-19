# Local FS + workspace context — OSS pointer (non-normative)

**Normative definitions live only in v1-spec** (private `nexus-platform` design tree). Resolve `specs_root.v1-spec` via `.agents/local-paths.json` (see `.agents/local-paths.json.example` and repository `AGENTS.md` *External Design Specs*).

**Read in this order:**

1. `adr/adr-014-local-fs-creator-workspace-layout-v1.md`
2. `cli-sync/cli-spec-v1.md` §6.2B–§6.2C, §6.3 C2, §13.0–§13.2
3. `cli-sync/local-db-schema-v1.md` §0
4. `domain/data-model-v1.md` §5.14
5. `platform/auth-session-model-v1.md` §6 (CLI table)

Do **not** treat this file as a second SSOT. For disk layout, commands, or invariants, edit **v1-spec** only; keep this stub as a **handoff pointer** for clone-only workflows.

**Related (in-repo, module scope only):** [local-db-refactor-v2.md](local-db-refactor-v2.md)
