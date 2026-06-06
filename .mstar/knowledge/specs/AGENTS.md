# Specs — AGENTS.md

Decision rules for **`.mstar/knowledge/specs/`** — functional and normative OSS specifications.

Parent index: [README.md](README.md). Harness conventions: [`.mstar/knowledge/AGENTS.md`](../AGENTS.md).

---

## Layout invariant

- **Flat directory only** — all spec files live directly under `specs/` (kebab-case, **no version suffix** in filenames).
- **Do not** introduce `specs/cli/`, `specs/orchestration/`, etc. without an ADR + bulk link migration (150+ in-repo references).
- **Exploration / draft** specs stay in this directory with explicit `Status: Exploration` or `Status: Draft` in the header — not in `knowledge/` root or `iterations/`.

---

## When to create a new spec vs extend an existing one

| Situation | Action |
| --- | --- |
| New **subsystem** with its own crate boundary and stable API | New spec (e.g. `agent-host.md`) |
| **Product line** with shipped iteration compass (FL-E, Work loop) | New feature spec; link from orchestration/cli as needed |
| **CLI top-level IA** or entry-model change | Extend `cli-command-ia.md` / `creator-centric-entry-model.md` during draft; merge into `cli-spec.md` at iteration close |
| **Preset engine** loader/runtime/validator change | Extend `orchestration-engine.md` |
| **Future engine feature** not yet implement-authorized | New exploration spec (e.g. `preset-conditional-routing-fl-d.md`) + link from orchestration-engine § |
| **Small OSS companion** to platform ADR (<80 lines, single concern) | Keep separate only if referenced independently (e.g. `canonical-hash.md`); else appendix in parent spec |
| **Iteration-scoped audit / evidence** | Compass appendix — **not** a spec |

**Anti-pattern:** parallel specs that restate the same normative rules (e.g. second CLI command list). Use cross-links and a single SSOT per concern.

---

## Status lifecycle (header field)

Every spec **should** declare `**Status**:` near the top.

| Status | Meaning | Edit rule |
| --- | --- | --- |
| **Normative** / **Active** / **Accepted** | Shipped or authoritative SSOT | Changes need plan or ADR for breaking behavior |
| **Shipped (V1.xx)** | Feature line delivered; still normative | Extend for forward-compatible additions only |
| **Draft (V1.xx)** | Locked in active compass; pre-ship | May change until iteration P5 hygiene |
| **Exploration** | Design only; **no implement authority** | Promote to Draft when a compass locks implement |
| **Active (legacy scope)** | Still cited; narrow domain frozen | Do not expand scope; prefer orchestration-engine for engine mechanics |

On promotion **Exploration → Draft → Shipped**: update [README.md](README.md) master index and `status.json` `spec_refs` if wave-0.

---

## Authority on overlap (conflict resolution)

Higher row wins when specs disagree without an active compass override:

1. Root [AGENTS.md](../../../AGENTS.md)
2. [local-cloud-crate-architecture.md](local-cloud-crate-architecture.md)
3. [entity-scope-model.md](entity-scope-model.md)
4. Active **iteration compass** (delivery batching only)
5. Domain SSOT — see [README.md § Authority matrix](README.md#authority-matrix-overlapping-topics)
6. [cli-spec.md](cli-spec.md) for per-command flags and behavior
7. Feature specs (`work-experience-model`, `creator-workflow-fl-e`, …)

**V1.35 CLI IA:** [cli-command-ia.md](cli-command-ia.md) supersedes `cli-spec.md` §6.0B until P5 merges IA into cli-spec.

---

## Merge / archive playbook

Execute at **iteration close (P5 spec hygiene)** or when a dedicated hygiene plan locks.

| Candidate | Trigger | Target |
| --- | --- | --- |
| `cli-command-ia.md` | V1.35 shipped | Merge into `cli-spec.md` §6.0B; stub or archive draft file |
| `creator-centric-entry-model.md` | V1.35 shipped | Merge into `cli-spec.md` §7; stub or archive |
| `preset-conditional-routing-fl-d.md` | FL-D shipped | Promote to normative; fold § into `orchestration-engine.md` §7.5 or keep if >200 lines |
| `skills-export-compatibility.md` | Next ACP hygiene | Optional appendix of `acp-client-tech-spec.md` |
| `novel-writing-sync-contract.md` | If sync module retired | Archive to `archived/knowledge/` |

**Archive steps:** `git mv` → `.mstar/archived/knowledge/`; pointer stub with `Superseded by:`; update README + grep fix links.

**Do not archive** while any open plan, compass, or crate `AGENTS.md` still cites the path as normative.

---

## Naming

- Pattern: `<domain>-<qualifier>.md` or `<product-line>-<feature>.md`
- Prefer **product line codes** in body text (FL-E, FL-D), not filenames (`fl-e` OK in feature spec names already shipped)
- Avoid version suffixes in filenames (`cli-command-ia-v1.35.md` is wrong)

---

## Related (not in `specs/`)

| Content | Location |
| --- | --- |
| Schema ↔ contracts boundary | [schemas-wire-platform-sync-boundary.md](../schemas-wire-platform-sync-boundary.md) |
| Deferred features | [deferred-features-cross-version-tracker.md](../deferred-features-cross-version-tracker.md) |
| Iteration delivery scope | [`.mstar/iterations/`](../../iterations/README.md) |
| Platform ADRs | `nexus-platform/.mstar/designs/v1-spec/adr/` |
