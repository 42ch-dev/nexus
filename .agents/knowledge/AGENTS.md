# Knowledge — AGENTS.md

Behavioral rules for the harness **knowledge** tree. **Do not** duplicate file indexes here — discover documents via [`README.md`](README.md) or directory listing.

> Project and crate rules: root [`AGENTS.md`](../../AGENTS.md). Harness layout: [`.mstar/AGENTS.md`](../../.mstar/AGENTS.md).

---

## What belongs where

**Principle:** separate current-state truth (`docs/specs/`), decision history (`.agents/adr/`), and reusable experience (`knowledge/`). Local delivery state is gitignored and is **not** clone SSOT.

| Kind of content | Where | Must not |
| --- | --- | --- |
| CLI / daemon / ACP / orchestration **behavior contracts** | [`docs/specs/`](../../docs/specs) | Live in knowledge root long-term |
| One decision among real alternatives, with its consequences | [`../adr/`](../adr) | Restate current state or plan text |
| Schema ↔ contracts boundary, crate policy | `knowledge/` root | Restate normative command/API detail |
| Time-boxed delivery, plans, residuals, roadmaps | local process only | Be tracked here or treated as clone SSOT |

`.agents/knowledge/` holds **distilled knowledge only**. Tracked files must not name, quote, or link ignored harness paths.

End-user docs stay in repo-root `docs/`.

---

## Specs and ADRs

Current-state specs live in **[`docs/specs/`](../../docs/specs)**, one page per subject at `<class>/<topic>.md`, latest-only. Rules and the navigation index: [`docs/specs/README.md`](../../docs/specs/README.md). The catalog (`mstar catalog list`) is the registry; it is process-local and not clone-shared.

Decisions live in **[`../adr/`](../adr)**. Contract: [`../adr/README.md`](../adr/README.md).

When implementing runtime behavior, read the cited spec body, then the relevant knowledge pattern. **Do not silently diverge** from a cited spec; record the change via spec revision or ADR.

---

## Retiring knowledge and specs

Both tracked layers describe the latest state; neither keeps version history.

**Knowledge:** remove a stale or superseded document from tracked `knowledge/` outright — no archive-pointer stubs and no versioned siblings. Fix tracked inbound links, remove its `README.md` rows, and reconcile its catalog row (`superseded`/`archived`, or remove). A local, gitignored comparison copy may be kept outside the tracked tree; it is not clone SSOT.

**Specs:** update in place. When a page no longer describes the current state, delete it or merge it into its successor and record the supersession in that successor's provenance line. Git and ADRs carry history; there is no `archived/` directory in the tracked specs tree. Fix every inbound link and reconcile the catalog row in the same change.

Do not retire while a crate `AGENTS.md` or a shipped spec still treats the path as normative authority. Published migration comments are the one exception: their bytes are checksum-frozen, so their references are redirected by the migration table in `crates/nexus-local-db/AGENTS.md` rather than edited in place.

---

## OSS local normative SSOT

Platform `v1-spec/local/` was retired in favor of **`docs/specs/` in this repo** (see platform ADR-029). Specs here are authoritative for OSS implementation; platform `v1-spec/` remains authoritative for cloud product and shared ADRs.

---

## AGENTS.md authoring rule (this tree)

Knowledge `AGENTS.md` files record **invariants, decision procedures, and anti-patterns** — not inventories of filenames, version lists, or tables that duplicate README / glob results. If content goes stale when a file is added or renamed, it belongs in README or in the spec header, not in AGENTS.
