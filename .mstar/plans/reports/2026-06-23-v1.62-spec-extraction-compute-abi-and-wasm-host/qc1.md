---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-23-v1.62-spec-extraction-compute-abi-and-wasm-host"
verdict: "Approve"
generated_at: "2026-06-24"
---

# Code Review Report

## Reviewer Metadata

- **Reviewer**: @qc-specialist
- **Runtime Agent ID**: qc-specialist
- **Runtime Model**: MiniMax-M3 (minimax-cn-coding-plan/MiniMax-M3)
- **Review Perspective**: Architecture + maintainability (spec-structure specialization) — docs-only plan
- **Report Timestamp**: 2026-06-24 (ISO-8601)

## Scope

- **plan_id**: `2026-06-23-v1.62-spec-extraction-compute-abi-and-wasm-host`
- **Review range / Diff basis**: `merge-base iteration/v1.62 @ f77b3de8 → feature/v1.62-spec-extraction @ 2424c760` (1 commit)
- **Working branch (verified)**: `feature/v1.62-spec-extraction`
- **Review cwd (verified)**: `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.62-p2-specs`
- **HEAD verified**: `2424c760` (matches Assignment)
- **Files reviewed**: 6 (2 NEW Master specs + 4 amended specs, including `README.md` index)
- **Commit range**: `f77b3de8..HEAD` = `2424c760` (1 commit, message: `docs(specs): V1.62 P2 — 2 NEW Master specs + 4 amendments`)
- **Tools run**: `git rev-parse --show-toplevel`, `git branch --show-current`, `git rev-parse HEAD`, `git log --oneline f77b3de8..HEAD`, `git diff --stat f77b3de8..HEAD`, `rg` (cross-ref enumeration, stale-ref scan, section counts, V1.62 marker scan, status.json + compass + plan check), `head` (header inspection)

### Diff overview (`f77b3de8..HEAD`)

```
.mstar/knowledge/specs/README.md                   |  15 +-
.mstar/knowledge/specs/compute-module-abi.md       | 463 +++++++++++++++++++++
.mstar/knowledge/specs/entity-scope-model.md       | 110 ++++-
.mstar/knowledge/specs/orchestration-engine.md     | 103 ++++-
.mstar/knowledge/specs/schemas-directory-layout.md |  11 +-
.mstar/knowledge/specs/wasm-host.md                | 365 ++++++++++++++++
6 files changed, 1053 insertions(+), 14 deletions(-)
```

**Coverage note**: Only `.mstar/knowledge/specs/**/*.md` and the README index changed. No `schemas/`, `crates/`, or other code paths were touched — code-only territories of P0/P1 remain untouched. This is the expected docs-only scope for P2.

## Findings

### 🔴 Critical

_None._

### 🟡 Warning

_None._

### 🟢 Suggestion

#### S-1: New §5.5.9 in `entity-scope-model.md` perpetuates a pre-existing §5.5.x placement quirk (non-blocking)

The new `#### 5.5.9 Computable-flag semantics and structured validation mode (V1.62 P2)` amendment was placed at line 146, between `## 4` (Crate ownership map) and `## 5` (Naming clarifications). The natural location for a §5.5.x amendment is under `### 5.5 World KB promotion state machine` at line 422, alongside siblings §5.5.1–§5.5.7. However, **this placement pattern is pre-existing** — `#### 5.5.8 Conditional routing branch input visibility` (added V1.56 P3) is in the same out-of-place location (line 132, also between `## 4` and `## 5`); the V1.62 P2 amendment correctly followed the existing convention rather than introducing a new structural anomaly.

**Source**: `git show f77b3de8:.mstar/knowledge/specs/entity-scope-model.md | rg -n '^#### 5\.5\.[0-9]'` confirms §5.5.8 was already at line 132 in the pre-P2 base.

**Why non-blocking**:
- This is **pre-existing** structure from V1.56 P3, explicitly out of scope per Assignment ("NEVER raise pre-existing items").
- The new amendment follows the established (if suboptimal) convention.
- Consolidating §5.5.8 and §5.5.9 under the §5.5 promotion state machine heading is a P5 hygiene item, not a P2 deliverable.

**Recommendation**: Track for V1.62 P5 spec-hygiene (or follow-up iteration) to consolidate §5.5.8 + §5.5.9 under `### 5.5`. Flag as `low` severity residual if the consolidation is not done before P-last.

#### S-2: Status line for `entity-scope-model.md` dropped minor descriptive clauses (V1.50 / V1.51 detail)

The pre-V1.62 Status line included parenthetical clarifications about **how** each version's § became normative:

- V1.50: "§5.5 World KB promotion state machine (T-B P1); promotion row promoted Draft → Normative at V1.50 P-last when the `kb_extract_jobs` migration landed and review-time extraction is verified end-to-end."
- V1.51: "§5.5.6 LLM pathway subsection (T-A P0) — `nexus.llm.extract` wire `BlockType` → `novel_category` mapping documented as SSOT."

The new Status line abbreviates both to "§5.5 World KB promotion state machine" and "§5.5.6 LLM pathway subsection" respectively. The dropped content is **provenance/audit-detail, not normative text** — the §5.5 and §5.5.6 sections themselves are unchanged, and the V1.50/V1.51 milestone history remains preserved in `git log` and the V1.50/V1.51 iteration compasses.

**Source**: `git diff f77b3de8..HEAD -- .mstar/knowledge/specs/entity-scope-model.md` (line 1-5 of diff).

**Why non-blocking**:
- No normative content was lost.
- The shorter Status line is arguably better (less repetition between header and body).
- The audit trail lives in `git log`, `iterations/v1.5*-...-compass-v1.md`, and `status.json` (per `.mstar/AGENTS.md` "Audit trail preservation").

**Recommendation**: Optional — restore the parenthetical audit-detail if a reader is likely to need it from the spec alone. Otherwise no action.

## Source Trace

- **F-001 / S-1**: `git show f77b3de8:.mstar/knowledge/specs/entity-scope-model.md | rg -n '^#### 5\.5\.[0-9]'` (pre-existing §5.5.8 placement) + `rg -n '^#### 5\.5\.[0-9]' .mstar/knowledge/specs/entity-scope-model.md` (post-P2 §5.5.8 + §5.5.9 placement). Confidence: **High**.
- **F-002 / S-2**: `git diff f77b3de8..HEAD -- .mstar/knowledge/specs/entity-scope-model.md` (Status + Last updated + Related header rows changed; in-body §5.5 + §5.5.6 unchanged). Confidence: **High**.

## Detailed Verification (per QC checklist)

### 1. Doc-class compliance (header structure)

Both new specs open with `## 0. Document position` and declare the full required attribute set:

| Field | compute-module-abi.md | wasm-host.md |
| --- | --- | --- |
| **Status** | Normative — V1.62 Shipped ✓ | Normative — V1.62 Shipped ✓ |
| **Document class** | Master ✓ | Master ✓ |
| **Scope** | declarative one-liner listing the 7 covered concerns ✓ | declarative one-liner listing the 9 covered concerns ✓ |
| **Last updated** | 2026-06-23 — V1.62 P2 ✓ | 2026-06-23 — V1.62 P2 ✓ |
| **Related** | 4 in-repo spec links + section anchors ✓ | 4 in-repo spec links + section anchors ✓ |

**Format matches the established Master spec pattern** (compared against `llm-extract.md` lines 3-11, which is the canonical closeout-status Master from V1.51). The table is well-formed Markdown, the Status is `Normative — V1.62 Shipped` matching the `Master` document class authority rule (from `knowledge/specs/AGENTS.md` lines 22-29).

**Result**: ✅ Doc-class compliance for both NEW specs.

### 2. Section completeness (per plan T1/T2)

| Spec | `##` sections | Plan requirement | `###` subsections | Substantive content |
| --- | --- | --- | --- | --- |
| `compute-module-abi.md` | 10 (§0–§9) | 9 substantive sections per T1 ✓ | 17 | Yes — every section has prose, tables, code, or worked examples |
| `wasm-host.md` | 10 (§0–§9) | 9 substantive sections per T2 ✓ | 18 | Yes — every section has prose, tables, or step lists |

**T1 section list per plan** (verify in `compute-module-abi.md`):

- §1 V1 envelope ABI overview ✓ (line 22)
- §2 Module exports table (with 2.1 `init` semantics) ✓ (line 50)
- §3 Host import ABI table ✓ (line 78)
- §4 `ComputeInput` envelope structure ✓ (line 103)
- §5 `ComputeOutput` 4-part envelope (with 5.1-5.5 sub-sections) ✓ (line 150)
- §6 Marshalling convention (with 6.1-6.4 sub-sections) ✓ (line 227)
- §7 `manifest.json` contract (with 7.1-7.4 sub-sections) ✓ (line 288)
- §8 Sandbox model cross-ref ✓ (line 406)
- §9 Versioning (with 9.1-9.3 sub-sections) ✓ (line 423)

**T2 section list per plan** (verify in `wasm-host.md`):

- §1 Runtime overview ✓ (line 27)
- §2 Engine lifecycle (with 2.1-2.2 sub-sections) ✓ (line 48)
- §3 Per-invocation sandbox ✓ (line 83)
- §4 Sandbox limits ✓ (line 113)
- §5 Wall-time watchdog mechanism (with 5.1-5.2 sub-sections) ✓ (line 145)
- §6 Embedded module loading (with 6.1-6.3 sub-sections) ✓ (line 177)
- §7 User module discovery (with 7.1-7.2 sub-sections) ✓ (line 220)
- §8 Error taxonomy (with 8.1-8.5 sub-sections) ✓ (line 244)
- §9 Host function implementation (with 9.1-9.3 sub-sections) ✓ (line 304)

**Result**: ✅ All plan-required sections present and substantive in both NEW specs.

### 3. Information architecture (separation of concerns)

The two NEW specs cleanly partition the compute subsystem:

| Spec | Concern | Examples |
| --- | --- | --- |
| `compute-module-abi.md` | **Module-side contract** — what the module author must do, what the host promises to provide | Exports, imports, envelopes, marshalling, manifest contract, versioning |
| `wasm-host.md` | **Host runtime** — how the daemon implements the host side | Engine, sandbox, limits, watchdog, module loading, error taxonomy, host function impls |

**No redundant content**: the two specs cross-reference rather than restate. `compute-module-abi.md` §8 says "Full details are in [wasm-host.md] §3-§5" (sandbox). `wasm-host.md` §1 + §9.3 say "Read [compute-module-abi.md] for the module-side contract" (memory ABI). The error sentinels are defined once in `compute-module-abi.md` §6.3 and referenced by name from `wasm-host.md` §8.4. The `ComputeError` variants are defined once in `wasm-host.md` §8 and referenced by name from `compute-module-abi.md` §2.1, §6.3, §7.3, §8.

**Result**: ✅ Clean separation, no duplication, bidirectional cross-references.

### 4. Cross-reference integrity (every cited path resolves)

Enumerated `./<spec>.md` references in both NEW specs:

| Cited path | Target | Resolves? |
| --- | --- | --- |
| `./wasm-host.md` | `.mstar/knowledge/specs/wasm-host.md` | ✓ |
| `./schemas-directory-layout.md` | `.mstar/knowledge/specs/schemas-directory-layout.md` | ✓ |
| `./orchestration-engine.md` | `.mstar/knowledge/specs/orchestration-engine.md` | ✓ |
| `./entity-scope-model.md` | `.mstar/knowledge/specs/entity-scope-model.md` | ✓ |

External references also resolve:

| Cited path | Target | Resolves? |
| --- | --- | --- |
| `../../../crates/nexus-wasm-host/AGENTS.md` (in wasm-host.md Related) | exists, 59 lines, matches V1.62 architecture decisions | ✓ |
| `../../../schemas/local-api/compute/compute-input.schema.json` (in compute-module-abi.md §4) | exists, 65 lines, V1.62 envelope shape | ✓ |
| `../../../schemas/local-api/compute/compute-output.schema.json` (in compute-module-abi.md §5) | exists, 4-part envelope | ✓ |
| `../../../modules/README.md` (in compute-module-abi.md §7) | exists, 223 lines, matches ABI documentation | ✓ |

Section anchors used in cross-references (spot-checked):

- `[wasm-host.md](./wasm-host.md) §3–§5` — §3, §4, §5 all exist ✓
- `[wasm-host.md](./wasm-host.md) §8.5` — §8.5 "Manifest validation" exists ✓
- `[schemas-directory-layout.md](./schemas-directory-layout.md) §3.5` — §3.5 "local-api/compute/" exists ✓
- `[orchestration-engine.md](./orchestration-engine.md) §8 (narrative.compute)` — §8.4 exists ✓
- `[entity-scope-model.md](./entity-scope-model.md) §5.5.9` — §5.5.9 exists ✓

**Stale-reference scan** (per Assignment: zero matches expected):

```bash
rg 'schemas/compute/|schemas/cloud-sync/|entity-attributes\.schema\.json|entity-state\.schema\.json' \
   .mstar/knowledge/specs/compute-module-abi.md \
   .mstar/knowledge/specs/wasm-host.md
# Result: (no output)
```

No NEW specs reference deleted paths. The deleted-path documentation lives in `entity-scope-model.md` §5.5.9.3 + §5.5.9.4 (which is intentional and correct — that subsection exists to record the supersession), and in `schemas-directory-layout.md` §1 + §5 (pre-existing, also intentional).

**Result**: ✅ All cross-references resolve. No stale paths in NEW specs.

### 5. Style consistency (matches existing Master voice/density)

Compared against `llm-extract.md` (canonical V1.51 closeout Master) and the existing `entity-scope-model.md` / `orchestration-engine.md` amendment style:

| Voice marker | compute-module-abi.md | wasm-host.md | Reference (llm-extract.md) |
| --- | --- | --- | --- |
| `## 0. Document position` table format | ✓ | ✓ | ✓ (line 3) |
| Self-positioning opening paragraph | ✓ | ✓ | ✓ (line 13) |
| "This Master is normative for …" | ✓ | ✓ | ✓ |
| Cross-references with `[text](./file.md) §N` syntax | ✓ | ✓ | ✓ |
| "Coordinates with" / "Related" links via Markdown | ✓ | ✓ | ✓ |
| Compiled crate/file path references (e.g., `crates/nexus-wasm-host/src/sandbox.rs`) | ✓ (§4) | ✓ (§4, §6, §8, §9) | n/a (different domain) |
| ASCII flow diagrams in fenced `text` blocks | ✓ (§1, §6.4) | ✓ (§3, §5) | ✓ (§3, §4) |
| Footer line `*Normative Master. Vx.xx Py (date). Source material: …*` | ✓ | ✓ | ✓ (line ~245) |
| Table-driven prose (vs. long paragraphs) | ✓ (most sections) | ✓ (most sections) | ✓ |

**Result**: ✅ Voice and density match the established Master-spec convention.

### 6. Findability (V1.63+ reader can understand the compute subsystem from these specs alone)

A V1.63+ reader landing in the compute subsystem will find:

1. **README index → "Compute and WASM" domain section** lists both new specs with Status ✓
2. **compute-module-abi.md** is self-contained for module authors (exports, imports, envelopes, manifest, versioning)
3. **wasm-host.md** is self-contained for host implementors (engine, sandbox, lifecycle, errors, host function impls)
4. **schemas-directory-layout.md** §3.5 explains the new `local-api/compute/` location and points to the 2 NEW specs
5. **entity-scope-model.md** §5.5.9 explains the `computable` + `state` semantics that govern which KeyBlocks are eligible for compute
6. **orchestration-engine.md** §8.4 documents the `narrative.compute` capability + `combat-engine` preset that calls into the host
7. **authority matrix** (in README) shows the compute ABI → wasm-host, schemas-directory-layout §3.5, orchestration-engine §8.4, entity-scope-model §5.5.9 dependency graph
8. **`crates/nexus-wasm-host/AGENTS.md`** is referenced from wasm-host.md Related and remains the implementation-side rulebook

The whole V1.61 compass grill-decision chain (Q1/Q3/Q4/Q5/Q6/Q7/Q8/Q9/Q10/Q11) is now decomposed into:

- Q1 (runtime) → `wasm-host.md` §1
- Q3 (V1 envelope scope) → `compute-module-abi.md` §1
- Q4 (KB structured layer) → `entity-scope-model.md` §5.5.9.1 + §5.5.9.2
- Q5 (state granularity / `state.character.current_hp` nesting) → `compute-module-abi.md` §4 (KeyBlock state in body) + §5.1 (state_delta) + `entity-scope-model.md` §5.5.9.2
- Q6 (per-invocation sandbox) → `compute-module-abi.md` §1 + `wasm-host.md` §3 + §4
- Q7 (combat-engine preset) → `orchestration-engine.md` §8.4.2
- Q8 (4-part output envelope) → `compute-module-abi.md` §5
- Q9 (host functions) → `compute-module-abi.md` §3 + `wasm-host.md` §9
- Q10 (repo structure / `modules/` + embedded) → `wasm-host.md` §6
- Q11 (wire change impact) → `schemas-directory-layout.md` §3.5 (local-api/compute) + §5 (historical renames)

A reader does **not** need to load the V1.61 compass to follow the compute architecture from these 2 NEW + 4 amended specs.

**Result**: ✅ V1.63+ reader can self-serve on the compute subsystem from these specs.

### 7. Amendment quality (4 amendments clearly marked + integrate cleanly)

| Amended spec | V1.62 marker (Status) | V1.62 marker (Last updated) | In-body marker | Integration |
| --- | --- | --- | --- | --- |
| `schemas-directory-layout.md` | "Normative — V1.62 Shipped (consumer-scope reorganization)" | "2026-06-23 — V1.62 P2 (spec-seal polish)" | Footer: "V1.62 P0 consumer-scope reorganization (2026-06-23); V1.62 P2 spec-seal polish" | Clean — §3.5 cross-refs to both NEW specs (was a relative-path string before, now a real Markdown link) |
| `entity-scope-model.md` | "V1.62 Shipped: §5.5.9 computable-flag semantics + structured validation mode (closes R-V161P1-LOW-001)" | "2026-06-23 — V1.62 P2 §5.5.9 computable-flag semantics + structured validation mode" | "Status: Normative — V1.62 Shipped (closes R-V161P1-LOW-001)" inline at §5.5.9 | Clean — new §5.5.9 with 4 sub-subsections (§5.5.9.1–§5.5.9.4) integrates new content; §5.5.9.3 explicitly documents the structured validation mode; §5.5.9.4 documents the supersession of deleted `entity-attributes`/`entity-state` schemas |
| `orchestration-engine.md` | "V1.62 Shipped: §5.2 narrative.compute capability + §8.4 combat-engine preset (deferred from V1.61 P3)" | "Date: 2026-04-17; Last updated: 2026-06-23 — V1.62 P2 §5.2 + §8.4" | "Status: Normative — V1.62 Shipped (deferred from V1.61 P3)" inline at §8.4 | Clean — row added to §5.2 capability table (line 352); §8.4 added with §8.4.1 (`narrative.compute` capability) + §8.4.2 (`combat-engine` preset); cross-references to both NEW specs |
| `README.md` | Updated domain table; new "Compute and WASM" section; new authority matrix rows; 3 amended-spec Status rows | n/a | n/a | Clean — both NEW specs in the right domain; cross-refs in authority matrix |

**No orphan sections**: every new `#### ` or `### ` heading has substantive body content.

**No normative content was deleted** by any amendment (verified by reading both pre- and post-P2 versions of each amended spec).

**Result**: ✅ Amendments are well-marked, integrate cleanly, and preserve all pre-existing normative content.

### 8. README index correctness

| Check | Result |
| --- | --- |
| `compute-module-abi.md` listed in Master index | ✓ "Compute and WASM" section, line 83 |
| `wasm-host.md` listed in Master index | ✓ "Compute and WASM" section, line 84 |
| Status fields updated for `entity-scope-model.md` | ✓ line 64 (now shows V1.62 §5.5.9) |
| Status fields updated for `schemas-directory-layout.md` | ✓ line 66 (now shows V1.62 Shipped) |
| Status fields updated for `orchestration-engine.md` | ✓ line 100 (now shows V1.62 §5.2 + §8.4) |
| Authority matrix includes Compute module ABI | ✓ line 168 |
| Authority matrix includes WASM compute host runtime | ✓ line 169 |
| Status fields mention new V1.62 sections | ✓ |

**Result**: ✅ README index is correctly updated.

## Summary

| Severity | Count |
| --- | --- |
| 🔴 Critical | 0 |
| 🟡 Warning | 0 |
| 🟢 Suggestion | 2 |

**Verdict**: **Approve**

This is a **docs-only** plan that delivers 2 NEW Master specs and 4 amendments with high quality:

- Both NEW specs are well-formed (doc-class compliant headers, 10 substantive sections each, comprehensive cross-references, 463 + 365 lines of dense, table-driven prose).
- All cross-references resolve; no stale paths.
- The 4 amendments are clearly V1.62-marked and integrate cleanly without orphan content or duplicate normative paragraphs.
- The README index is updated with a new "Compute and WASM" domain section, status updates, and 2 new authority matrix rows.
- The 2 suggestions are non-blocking: one is a pre-existing structural quirk from V1.56 P3 that the new amendment follows, and one is a minor Status-line abbreviation with no normative loss.

This plan is ready to merge once P0 (the code-driven schemas reorg) has landed on `iteration/v1.62` and the P1 manifest-validation work has been integrated — both of which are documented as `dependency: P0 merged to integration` in the plan and are out of scope for this QC review.

## QC Process Notes

- **Reviewer scope boundary**: This is a docs-only plan with no code changes in the diff. The QC focus is "architecture + maintainability (spec-structure specialization)" per the Assignment. Code-level concerns (clippy, tests, runtime behavior) are explicitly excluded.
- **Coordination context (read-only)**: This QC review was performed in parallel with qc2 and qc3 per `mstar-dispatch-gates` tri-review rules. The diff under review contains only `.mstar/knowledge/specs/**/*.md` and the README index. No `schemas/`, `crates/`, or other code paths were touched.
- **Out-of-scope pre-existing items not raised**:
  - `R-V161P0-LOW-001` (P-last T5 clippy) — Assignment explicitly excludes.
  - §5.5.8 placement in `entity-scope-model.md` — pre-existing V1.56 P3 structure; S-1 above notes this is the pattern §5.5.9 followed.
  - Compass prose count drift — already corrected per Assignment context.
  - P0/P1 code decisions — different plans' territories.
  - qc3 W-001 fix-wave on a different worktree — separate plan, separate worktree.
