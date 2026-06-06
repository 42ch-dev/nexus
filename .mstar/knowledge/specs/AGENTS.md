# Specs — AGENTS.md

Decision rules for normative documents in **`knowledge/specs/`**. **Do not** maintain file lists or domain indexes here — use [`README.md`](README.md) and each spec's header (`Status`, `Document class`, `Coordinates with`).

Parent rules: [`knowledge/AGENTS.md`](../AGENTS.md). Repo root: [`AGENTS.md`](../../../AGENTS.md).

---

## Layout invariant

- **Flat directory only** — one spec file per kebab-case basename; **no version suffix** in filenames.
- **No subdirectories** unless an ADR authorizes bulk link migration.
- **Exploration and draft overlays** live here with explicit header status — not under `knowledge/` root or `iterations/`.

---

## Document classes

Every spec declares **`Document class`** in its header (with **`Status`**).

| Class | Purpose | Implement authority |
| --- | --- | --- |
| **Master** | Long-lived SSOT for a subsystem or surface (CLI detail, engine, runtime) | Yes, when Status is normative/shipped |
| **Draft overlay** | Iteration-scoped revision of part of a Master; avoids editing Master mid-flight | Only while active compass + Status: Draft |
| **Feature line** | Shipped product line (Work loop, creator workflow, tool bridge) | Yes |
| **Exploration** | Future product line or engine capability pre-compass | **No** |
| **Companion** | OSS notes for a platform ADR or narrow module contract | Yes for OSS scope only |
| **Legacy scope** | Frozen subdomain still cited; do not expand | Yes for cited scope only |

**Anti-pattern:** two Masters restating the same normative surface (e.g. parallel command trees). Extend the Master or add a Draft overlay until merge.

---

## When to create vs extend

| Trigger | Action |
| --- | --- |
| New **crate boundary** with stable public API | New Master spec |
| **Product line** locked in a shipped compass | New Feature line spec |
| **Top-level IA or entry model** during active iteration | Draft overlay; merge into CLI Master at P5 hygiene |
| **Preset engine** loader, validator, runtime | Extend orchestration Master |
| **Engine capability** not yet compass-authorized | New Exploration spec; link from Master § stub |
| **Iteration audit / evidence** | Compass appendix — never a spec |
| **Small platform ADR companion** | Companion spec only if independently cited; else Master appendix |

---

## Status lifecycle

| Status | Meaning | Edit rule |
| --- | --- | --- |
| **Normative** / **Active** / **Accepted** | Shipped SSOT | Breaking behavior needs plan or ADR |
| **Shipped (Vx.xx)** | Feature line delivered | Forward-compatible extensions only |
| **Draft (Vx.xx)** | Overlay locked in active compass | Free until iteration P5 |
| **Exploration** | Design only | Promote to Draft when compass locks implement |

On **Exploration → Draft → Shipped**: update README index and `status.json` spec_refs if wave-0.

---

## Authority when specs overlap

Resolve conflicts top-down (higher wins unless active compass explicitly overrides **delivery batching only**):

1. Repo root **AGENTS.md**
2. **Architecture Masters** — crate graph, entity scope (foundation layer)
3. **Active iteration compass** — schedule and scope lock only; does not override shipped normative text except Draft overlays
4. **Draft overlay** over conflicting **legacy section** in the same-domain Master until P5 merge
5. **Domain Master** for that subsystem
6. **Feature line** spec for product behavior built on the Master
7. **Exploration** — input only; never overrides 5–6

**Single concern, single SSOT:** per-command flags live in CLI Master; top-level groups in IA overlay until merged; Work entity in Work feature spec; preset grammar in orchestration Master.

---

## Merge and retire (P5 hygiene)

Execute at **iteration close** or a dedicated spec-hygiene plan.

| Class transition | Rule |
| --- | --- |
| **Draft overlay → Master** | Fold overlay sections into Master; archive overlay with `Superseded by:` stub |
| **Exploration → normative** | Promote Status; fold into Master § if small, else keep Feature/Exploration Master with normative Status |
| **Companion obsolete** | Archive when platform ADR + code drop the OSS hook |
| **Legacy scope** | Do not grow; new engine behavior goes to orchestration Master |

After any retire: fix links, update README, never leave duplicate normative paragraphs.

---

## Naming

- Pattern: `<domain>-<qualifier>.md` or `<product-line>-<feature>.md`
- Product line codes (**FL-E**, **FL-D**, etc.) belong in **body text** or deferred tracker — **never** in spec filenames or document titles
- Never encode iteration version in filename (`*-v1.35.md`)

---

## AGENTS.md authoring rule (this tree)

Specs `AGENTS.md` holds **classes, lifecycle, and conflict rules** only. Filenames, domain tables, authority matrices, and consolidation schedules belong in **README.md** or spec headers — they change every iteration.
