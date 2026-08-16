---
module: creator-identity
date: 2026-08-16
problem_type: architecture
category: architecture-patterns
severity: high
applies_when:
  - "Adding any CLI or daemon entry point that creates/activates a creator identity"
  - "Debugging 'referenced creator not found' on world-create or other workspace writes"
  - "Designing local-only (no-platform) bootstrap flows"
tags: [creator, identity, bootstrap, state-db, workspace-db, local-only, ctr_local]
---

# Creator bootstrap materializes TWO stores — minting an identity is not enough

## Context

V1.167's dogfood (P0) hit `Authentication required` on `creator register` in an
isolated `HOME` and worked around it via an undocumented HTTP
`PATCH /v1/daemon/creators/{id}` upsert. The first fix attempt (`register
--local` delegating to `system identity create`) passed its unit tests but
**still failed** at PM live acceptance: `creator world create` → `referenced
creator 'ctr_local…' not found`.

Root cause: creator bootstrap spans **three** cooperating stores, and each
existing entry point materialized only a subset:

| Store | Path | Written by | Read by |
|-------|------|-----------|---------|
| Active-creator config | `~/.nexus42` CLI config `active_creator_id` | `system identity use`, daemon setup bootstrap | `require_active_creator` middleware, all CLI commands |
| Global identity store | `~/.nexus42/state.db` → `local_identities` | `system identity create` (persistent `ctr_local*`) | `resolve_active_identity`, identity subcommands |
| Workspace creators row | per-creator+workspace SQLite (`resolve_state_db_path`) → `creators` table | daemon `PATCH /v1/daemon/creators/{id}` (private `upsert_creator_display_name`), **now** `creator register --local` via `ensure_creator_row` | `create_world` FK precheck (`SELECT EXISTS(SELECT 1 FROM creators WHERE creator_id = ?)`), `list_creators` |

## Guidance

- **Any bootstrap entry point that intends a usable creator must materialize
  all three stores.** Minting an identity + setting active is insufficient:
  the first workspace write FK-prechecks the `creators` row in the
  per-creator+workspace db.
- The canonical helper is `nexus_local_db::ensure_creator_row(pool,
  creator_id, display_name)` (V1.167 P2 T2): UPDATE-else-INSERT mirroring the
  daemon's private `upsert_creator_display_name` SQL verbatim
  (`status='active'`, `data='{}'`, RFC3339 `cached_at`).
- `creator register --local --name <n>` is the complete product-path
  bootstrap (mint + active + workspace row); the platform form requires
  platform auth and hints at `--local`.
- Known **remaining gap** (tracked as a tracker candidate, V1.167 close):
  `system identity create --persistent` still writes only the first two
  stores — bootstrapping via that entry can still hit the FK dead-end. A
  shared bootstrap helper across both entry points is the candidate fix.
- Bootstrap must be error-honest: if the workspace row fails after identity
  mint, surface the failure (partial state = minted identity without row);
  do not silently skip.

## Why This Matters

The two-store split is intentional (ADR-014 lineage: global identity vs
workspace data), but it makes "identity exists" ≠ "workspace writes work".
Unit tests that stop at config readback + `resolve_active_identity` will pass
while the first real product write fails. Live end-to-end acceptance
(register → world create) is the only trustworthy check for bootstrap
changes.

## When to Apply

- New CLI/daemon flows that create or activate creators (check all three
  stores).
- Debugging `referenced creator … not found` (check which store is missing
  the row; remember which db the failing path resolves via
  `resolve_state_db_path`).
- Writing bootstrap tests: assert the workspace `creators` row exists, not
  just `active_creator_id`.

## Examples

- V1.167 P2: T1 (`510b6797`) = flag + delegation (insufficient alone); T2
  (`743060e4`) = `ensure_creator_row` + wiring; live acceptance evidence in
  `.mstar/iterations/v1.167/guides/dogfood-findings-register.md` (DF-A-02).
- SQL duplication note: daemon keeps its private copy until it refactors
  onto `nexus-local-db` for creator upserts (durable roadmap row in the P2
  plan); the two SQL strings are intentionally byte-identical so the `.sqlx`
  offline cache keys collide harmlessly.

Source: `iteration:v1.167/plans/2026-08-16-v1.167-p2-creator-register-local.md`
