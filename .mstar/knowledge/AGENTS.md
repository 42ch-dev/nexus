# Knowledge — AGENTS.md

Behavioral rules for the harness **knowledge** tree. **Do not** duplicate file indexes here — discover documents via [`README.md`](README.md), [`specs/README.md`](specs/README.md), or directory listing.

> Project and crate rules: root [`AGENTS.md`](../../AGENTS.md). Harness layout: [`.mstar/AGENTS.md`](../AGENTS.md).

---

## What belongs where

**Principle:** separate **durable normative truth** (specs) from **cross-cutting policy** (knowledge root) from **time-boxed delivery** (iterations) from **machine state** (`status.json`).

| Kind of content | Where | Must not |
| --- | --- | --- |
| CLI / daemon / ACP / orchestration **behavior contracts** | `specs/` | Live in knowledge root or compass long-term |
| Schema ↔ contracts boundary, crate policy, trackers | `knowledge/` root | Restate normative command/API detail |
| Iteration scope, grill decisions, audit evidence | `iterations/` | Become permanent spec without P5 merge |
| Open plans, residuals, branch names | `status.json` | Drift from compass without explicit update |

End-user docs stay in repo-root `docs/`.

---

## Specs subtree

All normative OSS specs are **flat** under `specs/`. Rules for creating, merging, and retiring specs: [`specs/AGENTS.md`](specs/AGENTS.md).

When implementing runtime behavior, read the active iteration compass (or `metadata.latest_ship.compass` between iterations), [specs/README.md](specs/README.md), then the cited spec bodies. Platform ADRs live in **`nexus-platform`** when this repo points outward.

**Do not silently diverge** from a cited spec; record change via spec revision, plan residual, or ADR.

---

## Deferred-feature trackers (two-document model)

**Principle:** one **active** tracker holds open/backlog rows only; one **append-only archive** holds closed history and per-iteration snapshots. Never merge them.

### Active tracker — maintenance discipline

1. **Open only** — no long-lived “shipped” strikethrough rows in open tables.
2. **Closing** — remove row from active; append same id to archive with version, plan, and brief note.
3. **Iteration close** — add delivery snapshot to archive; refresh active quick-status line; hygiene plan merges **last**.
4. **Re-defer** — keep row active; update target and history; archive only on ship or cancel.
5. **Conflicts** — active delivery compass wins on scope; `status.json` `residual_findings` wins over tracker mirror for machine-state residuals.

### Shipped archive — maintenance discipline

1. **Append-only** — never delete closed rows or snapshots.
2. **No open backlog** — new deferrals go to active tracker only.

Spec supersession uses the archiving rules below — independent from feature-tracker lifecycle.

---

## Archiving superseded knowledge

When any knowledge or spec document is superseded:

1. Move to `.mstar/archived/knowledge/` (or appropriate archived subtree).
2. Leave a **pointer stub** at the old path or fix all in-repo links in the same change.
3. Update **README indexes only** — not AGENTS files.

Do not archive while an active plan, compass, or crate AGENTS still treats the path as normative authority.

---

## OSS local normative SSOT

Platform `v1-spec/local/` was retired in favor of **`specs/` in this repo** (see platform ADR-029). Specs here are authoritative for OSS implementation; platform `v1-spec/` remains authoritative for cloud product and shared ADRs.

---

## AGENTS.md authoring rule (this tree)

Knowledge `AGENTS.md` files record **invariants, decision procedures, and anti-patterns** — not inventories of filenames, version lists, or tables that duplicate README / `status.json` / glob results. If content goes stale when a file is added or renamed, it belongs in README or in the spec header, not in AGENTS.
