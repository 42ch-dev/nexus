# Specs — AGENTS.md

Decision rules for normative documents in **`specs/`**. **Do not** maintain file lists or domain indexes here — use [`README.md`](README.md) and each spec's header (`Status`, `Document class`, `Coordinates with`).

Parent rules: [`../knowledge/AGENTS.md`](../knowledge/AGENTS.md). Repo root: [`../../AGENTS.md`](../../AGENTS.md).

---

## Layout invariant

- **Lifecycle tree** — every spec lives at **`specs/{active|proposed|archived}/{class}/<file>.md`**. The **class axis** is the domain directory that owns the spec: `agents/`, `architecture/`, `cli/`, `compute/`, `contracts/`, `creator/`, `novel-writing/`, `orchestration/`, `runtime/`, `surfaces/`. One spec file per kebab-case basename; no version suffix in filenames.
- **Lifecycle states** — the lifecycle directory states the spec's implement authority:
  - **`active/`** — **current authority**: normative/shipped Masters, Feature lines, and delivered overlays; the citation target for implement and review.
  - **`proposed/`** — **not yet implement authority**: Draft overlays, Exploration, target contracts that are not implement-GO, and Input briefs. **Classification test:** a Draft overlay (or amendment) whose defined behavior is **already implemented and owned by that spec** stays in `active/`; `proposed/` is for overlays and contracts **not yet in effect**. Promoted only through the lifecycle-change rule below.
  - **`archived/`** — **frozen history**, never current authority (see the `archived/` bullet below).
- **`novel-writing/` class** — all `work_profile: novel` Feature line specs live under `specs/{active,proposed}/novel-writing/`; the class index stays at [novel-writing/README.md](novel-writing/README.md).
- **`archived/`** — retired, deprecated, and historical records only (retired host records, historical rationale supplements, legacy acceptance records, redirect stubs), stored as `archived/{class}/yyyy-mm-dd-<topic-title>.md` (**Archived naming** below). Files under archived/ are frozen historical snapshots: never edit them or treat them as current authority. Archived records are **evidence and history, not implement authority**: any still-binding contract an archived record names must be owned by an **active** spec, and a current document that needs that contract links the active owner, naming the archived record only as history. Archived records are cited for history and do not grow new normative sections. `specs/archived/**` is **git-tracked** (`.gitignore` re-includes `.mstar/specs/**`), so the snapshots persist with the repository and are the archive of record; the harness-local archive directory is gitignored process material and is **not** an archive of record — only `specs/archived/**` is.
- **Root** keeps only [README.md](README.md) (index) and this file (rules); the one class-level index outside the lifecycle tree is [novel-writing/README.md](novel-writing/README.md).
- **New specs** land in the matching class dir under `active/` or `proposed/` per their `Status`; a spec that fits no class goes to the closest one and the README index flags it. Retired or deprecated records move to `archived/`.
- **Moves require the full link-migration sweep**: every spec-to-spec relative link, `knowledge/**` spec-links, root `AGENTS.md`/`DESIGN.md`/`CONCEPTS.md`/`STRATEGY.md`, `docs/**`, `schemas/**`, source-tree comments, and per-directory `AGENTS.md` files are retargeted in the same change; the README two-way index/disk diff and a 100% relative-link check must pass before commit.
- **Lifecycle change = `git mv` + `Status`/index sync + sweep rerun**: moving a spec between `active/`, `proposed/`, and `archived/` is a `git mv` **in the same change** as its `Status` header update and the [README.md](README.md) index update, and **must** rerun the link-migration sweep above — a stale pre-lifecycle path is a dangling reference.
- **Exploration and draft overlays** live in `proposed/<class>/` with explicit header status — not under `knowledge/` root; a Draft overlay whose defined behavior is already implemented and owned by its spec stays in `active/` (classification test above).
- *Authorization:* the maintainer authorized replacing the former flat-layout invariant with the domain-subdirectory taxonomy on **2026-09-29**, then that layout with the **lifecycle tree** (`active/` / `proposed/` / `archived/`) on **2026-10-12** (each executed as one bulk `git mv` + scripted link rewrite).

---

## Archived naming

Archived paths are **`archived/{class}/yyyy-mm-dd-<topic-title>.md`** — never flat in the `archived/` root:

- **`{class}`** — the domain directory the record originally belonged to: `architecture/`, `runtime/`, `cli/`, `orchestration/`, `agents/`, `creator/`, `surfaces/`, `contracts/`, `compute/`, `novel-writing/`.
- **`yyyy-mm-dd`** — the record's **evidenced** retirement/supersession date, taken from its own header (`Status`, iteration, or version record) and resolved against the repository's iteration history when the header names a version but no date. When no date can be evidenced, use the repository archival-move date and state that basis in the README row.
- **`<topic-title>`** — the original topic in kebab-case, with **no version suffix**.

An archiving change is the `git mv` **plus** the full link-migration sweep above **plus** the README two-way index/disk check (archived-table rows ↔ `archived/**` files); a frozen record gains no new normative sections afterwards.

---

## Document classes

Every spec declares **`Document class`** in its header (with **`Status`**).

| Class | Purpose | Implement authority |
| --- | --- | --- |
| **Master** | Long-lived SSOT for a subsystem or surface (CLI detail, engine, runtime) | Yes, when Status is normative/shipped |
| **Draft overlay** | Iteration-scoped revision of part of a Master; avoids editing Master mid-flight | Only while Status: Draft |
| **Feature line** | Shipped product line (Work loop, creator workflow, tool bridge) | Yes |
| **Exploration** | Future product line or engine capability not yet implement-locked | **No** |
| **Companion** | OSS notes for a platform ADR or narrow module contract | Yes for OSS scope only |
| **Legacy scope** | Frozen subdomain still cited; do not expand | Yes for cited scope only |

**Anti-pattern:** two Masters restating the same normative surface (e.g. parallel command trees). Extend the Master or add a Draft overlay until merge.

---

## When to create vs extend

| Trigger | Action |
| --- | --- |
| New **crate boundary** with stable public API | New Master spec |
| **Product line** locked and shipped | New Feature line spec |
| **Top-level IA or entry model** during active iteration | Draft overlay; merge into CLI Master at P5 hygiene |
| **Preset engine** loader, validator, runtime | Extend orchestration Master |
| **Engine capability** not yet implement-locked | New Exploration spec; link from Master § stub |
| **Iteration audit / evidence** | Local process only — never a spec |
| **Small platform ADR companion** | Companion spec only if independently cited; else Master appendix |

---

## Status lifecycle

| Status | Meaning | Edit rule |
| --- | --- | --- |
| **Normative** / **Active** / **Accepted** | Shipped SSOT | Breaking behavior needs plan or ADR |
| **Shipped (Vx.xx)** | Feature line delivered | Forward-compatible extensions only |
| **Draft (Vx.xx)** | Overlay locked for an in-flight iteration | Free until iteration P5 |
| **Exploration** | Design only | Promote to Draft when implement is locked |

On **Exploration → Draft → Shipped**: update the README index and move the file `proposed/` → `active/` (**Layout invariant**).

---

## Authority when specs overlap

Resolve conflicts top-down (higher wins):

1. Repo root **AGENTS.md**
2. **Architecture Masters** — crate graph, entity scope (foundation layer)
3. **Draft overlay** over conflicting **legacy section** in the same-domain Master until P5 merge
4. **Domain Master** for that subsystem
5. **Feature line** spec for product behavior built on the Master
6. **Exploration** — input only; never overrides 4–5

**Single concern, single SSOT:** per-command flags live in CLI Master; top-level groups in IA overlay until merged; Work entity in Work feature spec; preset grammar in orchestration Master.

---

## Merge and retire (P5 hygiene)

Execute at **iteration close** or a dedicated spec-hygiene change.

| Class transition | Rule |
| --- | --- |
| **Draft overlay → Master** | Fold overlay sections into Master; remove the overlay file from tracked `specs/` entirely (record supersession in the README index) |
| **Exploration → normative** | Promote Status; fold into Master § if small, else keep Feature/Exploration Master with normative Status |
| **Companion obsolete** | Remove from tracked `specs/` when platform ADR + code drop the OSS hook (record in README index) |
| **Legacy scope** | Do not grow; new engine behavior goes to orchestration Master |

After any retire: fix links, update README, never leave duplicate normative paragraphs.

---

## Naming

- Pattern: `<domain>-<qualifier>.md` or `<product-line>-<feature>.md`
- Product line codes (**FL-E**, **FL-D**, etc.) belong in **body text** — **never** in spec filenames or document titles
- Never encode iteration version in filename (`*-v1.35.md`)

---

## AGENTS.md authoring rule (this tree)

Specs `AGENTS.md` holds **classes, lifecycle, and conflict rules** only. Filenames, domain tables, authority matrices, and consolidation schedules belong in **README.md** or spec headers — they change every iteration.
