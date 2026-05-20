# Cloud Sync and Local Sync Status (Implementation SSOT)

## Document metadata

| Field | Value |
| --- | --- |
| **Status** | Active |
| **Wire normative** | `nexus-platform` `.agents/designs/v1-spec/shared/sync-contract-v1.md`, `shared/schema/*` |
| **Scope** | OSS crate wiring, daemon HTTP surface, CLI `sync` commands |

---

## Summary

| Layer | State (2026-05) |
| --- | --- |
| **Wire contract** | Stable in `schemas/` + `nexus-contracts`; platform consumption unchanged |
| **Daemon HTTP** | **`/v1/local/sync/*` removed** (V1.21+). `handlers/mod.rs` has no `sync` module; `handlers/sync.rs` is empty placeholder |
| **Library** | `nexus-cloud-sync` in workspace (includes `legacy/` modules). **`nexus-sync`** crate still on disk as path dependency for `nexus42` and `nexus-daemon-runtime` but **not** a workspace member |
| **CLI** | `nexus42 sync push|pull` still POSTs `/v1/local/sync/push` and `/v1/local/sync/pull` via `DaemonClient` — **known gap** until CLI calls library API or routes are restored |

---

## Crate map

```text
nexus-cloud-sync/     # workspace member; target home for cloud push/pull/apply
  src/legacy/         # migrated sync pipeline (conflict, outbox, pull_apply, …)
nexus-sync/           # legacy path dep only (not in root Cargo.toml members)
nexus-local-db/       # outbox tables (20260420_outbox_tables.sql)
nexus42/commands/sync.rs
nexus-daemon-runtime  # depends on nexus-sync path; no sync HTTP handlers
```

---

## Implementation checklist (agents)

1. Prefer **`nexus-cloud-sync`** for new sync logic; do not extend empty `handlers/sync.rs` without an ADR.
2. When fixing CLI sync: either wire `sync` commands to `nexus-cloud-sync` in-process, or re-expose guarded HTTP routes — pick one in plan/ADR; do not leave CLI calling removed paths.
3. Novel-writing / workspace artifact rules: [novel-writing-sync-contract.md](./novel-writing-sync-contract.md) (update push path when CLI wiring lands).

---

## Related

- Daemon routes: [daemon-local-api-routes.md](./daemon-local-api-routes.md)
- Schema boundary: [schemas-wire-platform-sync-boundary.md](./schemas-wire-platform-sync-boundary.md)
- Archived dual-outbox design: `.agents/archived/knowledge/dual-outbox-architecture.md`
