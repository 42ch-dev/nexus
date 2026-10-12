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

When implementing runtime behavior, read the rules in [`../specs/AGENTS.md`](../specs/AGENTS.md) plus the lifecycle tree (`specs/active/<class>/`), then the cited spec bodies.

**Do not silently diverge** from a cited spec; record change via spec revision or ADR.

---

## Archiving superseded knowledge

The two tracked trees retire differently — the rules split by directory.

**In `knowledge/`** (indexed tree):

1. Remove the superseded document from tracked `knowledge/` **entirely** — no archive-pointer stubs. Record the supersession in [`README.md`](README.md): its index rows are validated, so a retired document's row is updated in the same change.
2. Fix all **tracked** in-repo links in the same change.
3. Update that README index only — not AGENTS files.

**In `specs/`** (no index — the path is the state): no README row is added or updated. A superseded spec records its supersession in the **merge-target / successor spec's `Supersedes:` provenance line**; a retired record is **frozen** at `archived/{class}/<date>-<topic>.md` and never edited afterwards; a rename or retirement is discovered from **git history**, not from a maintained list.

Do not archive while a crate AGENTS or a shipped spec still treats the path as normative authority.

---

## OSS local normative SSOT

Platform `v1-spec/local/` was retired in favor of **`specs/` in this repo** (see platform ADR-029). Specs here are authoritative for OSS implementation; platform `v1-spec/` remains authoritative for cloud product and shared ADRs.

---

## AGENTS.md authoring rule (this tree)

Knowledge `AGENTS.md` files record **invariants, decision procedures, and anti-patterns** — not inventories of filenames, version lists, or tables that duplicate README / glob results. If content goes stale when a file is added or renamed, it belongs in README or in the spec header, not in AGENTS.
