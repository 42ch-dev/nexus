# Knowledge — AGENTS.md

Behavioral rules for the harness **knowledge** tree. **Do not** duplicate file indexes here — discover documents via [`README.md`](README.md) or directory listing.

> Project and crate rules: root [`AGENTS.md`](../../AGENTS.md). Harness layout: [`.mstar/AGENTS.md`](../AGENTS.md).

---

## What belongs where

**Principle:** separate **durable normative truth** (tracked `specs/`) from **cross-cutting policy** (tracked `knowledge/`). Local delivery state is gitignored and is **not** clone SSOT.

| Kind of content | Where | Must not |
| --- | --- | --- |
| CLI / daemon / ACP / orchestration **behavior contracts** | [`../specs/`](../specs/) | Live in knowledge root long-term |
| Schema ↔ contracts boundary, crate policy | `knowledge/` root | Restate normative command/API detail |
| Time-boxed delivery, plans, residuals, roadmaps | local process only | Be tracked here or treated as clone SSOT |

`.mstar/knowledge/` holds **distilled knowledge only**. Tracked files must not name, quote, or link ignored harness paths.

End-user docs stay in repo-root `docs/`.

---

## Specs

Normative OSS specs live in **[`../specs/`](../specs/)**. Rules: [`../specs/AGENTS.md`](../specs/AGENTS.md).

When implementing runtime behavior, read [`../specs/README.md`](../specs/README.md), then the cited spec bodies.

**Do not silently diverge** from a cited spec; record change via spec revision or ADR.

---

## Archiving superseded knowledge

When any knowledge or spec document is superseded:

1. Remove it from tracked `knowledge/` / `specs/` **entirely** — no archive-pointer stubs. Record supersession in the README index.
2. Fix all **tracked** in-repo links in the same change.
3. Update **README indexes only** — not AGENTS files.

Do not archive while a crate AGENTS or a shipped spec still treats the path as normative authority.

---

## OSS local normative SSOT

Platform `v1-spec/local/` was retired in favor of **`specs/` in this repo** (see platform ADR-029). Specs here are authoritative for OSS implementation; platform `v1-spec/` remains authoritative for cloud product and shared ADRs.

---

## AGENTS.md authoring rule (this tree)

Knowledge `AGENTS.md` files record **invariants, decision procedures, and anti-patterns** — not inventories of filenames, version lists, or tables that duplicate README / glob results. If content goes stale when a file is added or renamed, it belongs in README or in the spec header, not in AGENTS.
