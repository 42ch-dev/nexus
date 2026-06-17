# Novel Work Pool — Normative Specification v1

**Status**: Shipped (V1.41 — PR #53; post-merge `156e669d`)  
**Document class**: Feature line  
**Created**: 2026-06-10  
**Scope**: DF-61 selection pool + inspiration pool — **DB SSOT** with markdown inspiration files  
**Coordinates with**:

- [multi-work-lifecycle.md](multi-work-lifecycle.md) — completion clears active; `works use` sets default
- [work-experience-model.md](../work-experience-model.md) — pool is **not** a Work profile
- [cli-spec.md](../cli-spec.md) — `creator works` command group
- [local-db-schema.md](../local-db-schema.md) — table definitions

**Iteration compass**: [v1.41-multi-work-author-desk-delivery-compass-v1.md](../../iterations/v1.41-multi-work-author-desk-delivery-compass-v1.md) (Shipped)

---

## 1. Purpose

Authors track multiple novel ideas and one **default** writing target for CLI convenience. novels-system uses `选题库.md` + `灵感池/` as Obsidian SSOT. OSS uses **`state.db` as SSOT** with optional markdown files for human-readable inspiration notes.

**Invariant**: Pool entries are **creator-scoped**, not Work-scoped. Do not add `work_profile: novel-pool`.

### 1.1 `active` semantics (grill-me locked)

| | |
| --- | --- |
| **Means** | Default `work_id` when `creator run` omits `work_id` |
| **Does not mean** | Only one Work may run auto-chain or schedules globally |
| **Enforced** | At most **one** `novel_pool_entries.status = active` per `creator_id` |
| **Set by** | `creator works use`, `pool promote --set-default`, `creator bootstrap --set-default` |
| **Created on start** | `creator bootstrap` auto-inserts `queued` row unless `--no-pool-row` |
| **Prior active on change** | Demoted to **`queued`** (not `completed` unless Work already completed) |

---

## 2. Selection pool (选题库)

### 2.1 Status enum

| Status | Meaning | novels-system label |
| --- | --- | --- |
| `active` | CLI default Work | 当前在写 |
| `queued` | Waiting / non-default in-progress | 排队开坑 |
| `completed` | Finished/archived | 已完结 |

**Rule**: At most **one** `active` row per `creator_id` (partial unique index).

### 2.2 Table `novel_pool_entries` (intent)

| Column | Type | Notes |
| --- | --- | --- |
| `entry_id` | TEXT PK | `npe_` prefix |
| `creator_id` | TEXT FK | |
| `work_id` | TEXT FK nullable | Bound after scaffold; unique per creator when non-null |
| `title` | TEXT | Human title; default from `--idea` or Work metadata |
| `status` | TEXT | active \| queued \| completed |
| `created_at` | TEXT | ISO-8601 |
| `updated_at` | TEXT | ISO-8601 |

**Index**: `UNIQUE (creator_id, work_id) WHERE work_id IS NOT NULL`.

---

## 3. Inspiration pool (灵感池)

### 3.1 Model

Each inspiration item has:

1. **DB row** in `inspiration_items` (SSOT for listing, promotion, archive).
2. **Markdown file** at `{workspace_root}/Pool/Ideas/<slug>.md` where `workspace_root` is the operational workspace directory (per `nexus-home-layout::operational_workspace_dir`), referenced by `rel_path`.

**Not** per-Work `works.inspiration_log`.

### 3.4 Why `Pool/Ideas/` not `Works/_pool/`

Inspiration items are **creator-scoped**, not Work-scoped. An idea can outlive any single Work and may inspire multiple Works over time. The pool directory lives at the workspace root level (alongside `Works/`), not nested under any Work. See [`work-experience-model.md`](../work-experience-model.md) — pool is not a Work profile.

### 3.2 Table `inspiration_items` (intent)

| Column | Type | Notes |
| --- | --- | --- |
| `item_id` | TEXT PK | `npi_` prefix |
| `creator_id` | TEXT FK | |
| `rel_path` | TEXT | Relative to workspace root |
| `title` | TEXT | |
| `status` | TEXT | open \| promoted \| archived |
| `promoted_work_id` | TEXT nullable | Set on promote |
| `created_at` | TEXT | |

### 3.3 File scaffold

On `creator works pool inspiration add --title "..."`:

1. Insert DB row.
2. Create `{workspace_root}/Pool/Ideas/<slug>.md` with frontmatter `title`, `created`, empty body.

---

## 4. API authz (daemon)

### 4.1 `set_pool_active` creator binding (shipped `156e669d`)

Local API `set_pool_active` (and CLI `creator works use` / `pool promote --set-default`):

- Request body **`creator_id` must match** the authenticated/active creator context.
- Mismatch → **403 Forbidden** (not silent demote on wrong creator).
- Cross-reference [cli-spec.md](../cli-spec.md) §6.2D.

---

## 5. CLI surface (summary)

| Command | Effect |
| --- | --- |
| `creator works list` | List Works (from `works` table; migrated from `run list`) |
| `creator works status [<work_id>]` | Work detail; default = pool `active` |
| `creator works use <work_id>` | Upsert pool row if needed; demote prior `active` → `queued`; set target `active` |
| `creator works pool list` | List pool entries |
| `creator works pool promote <entry_id> [--set-default]` | `queued` → `active`; prior `active` → `queued`; bind/scaffold `work_id` if missing |
| `creator works pool inspiration promote <item_id> [--set-default] [--idea <text>]` | Read MD title/body → `run start --idea`; pool `queued` row; item → `promoted` |
| `creator works pool archive <entry_id>` | Entry → `completed` |
| `creator works pool inspiration add/list` | CRUD inspiration pool |
| `creator works completion-lock release <work_id>` | See lifecycle spec §3.1 |

---

## 6. Promote flow (no switch)

When `pool promote` targets `queued`:

1. Ensure `work_id` bound (scaffold via init preset if entry has none).
2. Demote current `active` row → `queued`.
3. Set promoted entry → `active`. `--set-default` is redundant when promote already sets `active`; flag retained for symmetry with `run start`.
4. **Does not** pause auto-chain on other Works.

CLI may **warn** when prior `active` Work still has running schedules (informational only).

### 6.1 Inspiration promote `--idea` semantics

When `pool inspiration promote <item_id>` is invoked:

- If `--idea <text>` is supplied, the new Work's `initial_idea` = `--idea` text.
- If `--idea` is omitted, the new Work's `initial_idea` = the inspiration item's `title`.
- The new Work's `title` always equals the inspiration item's `title` (not affected by `--idea`).

This makes the behavior explicit and CLI-help-testable.

---

## 7. Completion integration

On Work completion (lifecycle spec §2):

- Bound pool row → `completed`.
- If it was `active`, no pool `active` until `works use` / `promote --set-default` / `run start --set-default`.
- `resume --reopen` does **not** auto-restore pool `active`.

---

## 8. Implement split

| Phase | Deliverable |
| --- | --- |
| **P0** | `novel_pool_entries` migration; `works use`; completion → pool row update; `run start` auto row |
| **P1** | `inspiration_items`; `pool list/promote/archive/inspiration *` |

---

## 9. Acceptance (spec-level)

1. DB SSOT; markdown derivative for 选题库 export only.
2. One `active` per creator; unique `(creator_id, work_id)`.
3. `creator works list|status` replace `run list|status` (hard remove).
4. Inspiration promote = `run start` + pool row (grill-me A).

---

*Shipped V1.41. Pool promote bypassing completion-lock is by-design (grill-me §0.1 row 19).*
