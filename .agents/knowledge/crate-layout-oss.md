# OSS Crate Layout (Implementation SSOT)

## Document metadata

| Field | Value |
| --- | --- |
| **Status** | Active |
| **Source** | Root `Cargo.toml` `[workspace.members]` + path dependencies |
| **Normative** | `nexus-platform` `v1-spec/local/daemon-runtime-v1.md`, ADR-026/027 |

---

## Workspace members

| Crate | Role |
| --- | --- |
| `nexus42` | Sole user-facing binary (CLI + hidden daemon-run entry) |
| `nexus-daemon-runtime` | Daemon lifecycle, local HTTP API, subsystem composition (library only) |
| `nexus-agent-host` | Managed agent sessions (Hybrid providers) |
| `nexus-acp-host` | ACP client, registry client, skills export, worker IPC |
| `nexus-orchestration` | Presets, schedules, capability registry, worker supervision |
| `nexus-local-db` | SQLite + sqlx migrations (`state.db`) |
| `nexus-cloud-sync` | Platform sync client/apply pipeline (`legacy/` subtree in flux) |
| `nexus-domain` | Domain types; re-exports KB/memory modules during split |
| `nexus-kb` | KB domain extraction (WIP — member listed; flesh out with crate `Cargo.toml`) |
| `nexus-memory` | Memory domain extraction (WIP — same) |
| `nexus-contracts` | Generated + `local/` hand-written wire types |
| `nexus-home-layout` | `~/.nexus42/` paths |

---

## Path dependencies outside workspace members

| Crate | Consumers | Note |
| --- | --- | --- |
| `nexus-sync` | `nexus42`, `nexus-daemon-runtime` | Legacy; migrate callers to `nexus-cloud-sync` |

---

## Dependency edges (simplified)

```text
nexus42 → nexus-daemon-runtime, nexus-sync, nexus-acp-host, …
nexus-daemon-runtime → nexus-local-db, nexus-orchestration, nexus-agent-host, nexus-sync, …
nexus-orchestration → nexus-local-db, nexus-acp-host, …
nexus-agent-host → nexus-acp-host, …
```

---

## Related

- Runtime architecture: [nexus42-single-binary-daemon-runtime-architecture.md](./nexus42-single-binary-daemon-runtime-architecture.md)
- Local DB tables: [local-db-schema-implementation.md](./local-db-schema-implementation.md)
