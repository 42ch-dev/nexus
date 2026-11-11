# ADR: Knowledge IR — Spoke KnowledgeEntry as SSOT with Local Read/Write Adapters

Status: implemented — every durable `KnowledgeEntry` write commits through the Spoke `KnowledgeEntryPort` plus `KbStore` / `SqliteKbStore` stack, and no parallel IR, parallel port trait, or `kb/index.json` staging face remains
Date: 2026-11-10
Scope: Write-path ownership for narrative / World (and Actor) knowledge. Spoke `KnowledgeEntry` is the content SSOT; `nexus-knowledge` holds the in-process projection, `nexus-spoke-adapter` the sole conversion seam and the `KnowledgeEntryPort` implementation, `nexus-local-db` the SQLite persistence adapter. The normative rule lives in [world-kb-runtime-architecture.md §2.2](../../../../docs/specs/architecture/world-kb-runtime-architecture.md#22-single-durable-writer-normative) and [spoke-adapter-architecture.md §2.3](../../../../docs/specs/architecture/spoke-adapter-architecture.md#23-sqlite-storage-shape); this record keeps the reasoning.
Source: migrated from the retired tracked specs corpus

## Problem

Three scatter surfaces sat beside the World KB stack — the User `KnowledgeStore` / UserScope (`knowledge_entries`), reference sources (`reference_sources` plus `references/units/<id>/body.md`), and the `creator kb` work-scope file index (`kb/index.json` plus `kb/entries/<id>.md`). Each had live consumers (MCA, Host assembly, CLI groups, HTTP routes, the demo seed), so the family read as four stores and every one of them invited "one more store" thinking. They are retired by [2026-11-10-retire-user-scope-and-references.md](2026-11-10-retire-user-scope-and-references.md).

What survived the retirement still had two unstated properties that review could not check:

1. **No stated write owner.** Extract-apply and several CLI / demo paths could look like direct `kb_key_blocks` writes. Nothing said whether SQLite or the Spoke protocol was authoritative after a round trip, so a second writer, a second mapper, or a "fix" that trusted the row over the protocol had no written rule to violate.
2. **Dry-run had no structural barrier.** Simulate, extract *preview*, and Workflow Simulate paths that merely intend mutations could persist `kb_key_blocks` rows, which turns a preview into a durable state change.

The cost of leaving both unstated is not stylistic: a stray writer reintroduces the multi-store shape the retirement just removed, and a persisting dry-run writes data an operator never authorized.

## Decision

**Spoke `KnowledgeEntry` is the content SSOT; local Nexus code reaches it only through adapters.** Present behavior:

- **Projection, not authority.** `KnowledgeEntryRecord` (`nexus-knowledge`) is an in-process projection of the protocol, never a second authoring authority. If the record and a Spoke `KnowledgeEntry` disagree after a round trip, the conversion seam is buggy; the SQLite row is not promoted to arbiter over the protocol.
- **One durable write path.** Every durable create/update/delete commits through Spoke `KnowledgeEntryPort` — implemented by `NexusAdapter` at `crates/nexus-spoke-adapter/src/adapter/knowledge_entry_port.rs:170`, reusing the sole conversion seam `knowledge_record_to_spoke` / `spoke_to_knowledge_record` (`crates/nexus-spoke-adapter/src/conversion/knowledge_entry.rs:94,219`) plus CAS — onto the `KbStore` trait (`crates/nexus-knowledge/src/world_kb/store.rs:107`) and its `SqliteKbStore` implementation (`crates/nexus-local-db/src/kb_store.rs:365`), which persists `body_json` and the indexed columns in `state.db` `kb_key_blocks`. `SqliteKbStore` is the only production writer of `kb_key_blocks`: no CLI group, daemon handler, preset, or demo seed keeps its own insert/update statement. Test fixtures and `#[cfg(test)]` modules are the stated exception.
- **Sibling ports stay siblings.** Relationship rows (`kb_relationships`) and extract-job apply (`kb_extract_jobs`) keep their own adapter layer in `nexus-local-db`, called from `nexus-core`. They are not folded into `KnowledgeEntryPort` or `KbStore`, and they carry the same non-apply stub rule for their own paths.
- **Non-apply paths are stubbed.** Simulate, dry-run, extract preview, and any Workflow Simulate that would touch knowledge bind a stub implementation of `KnowledgeEntryPort`. The stub may record intended mutations for inspection; it does not insert or update `kb_key_blocks` or auto-apply extract-job rows.
- **No third name.** There is no parallel `KnowledgePort` trait and no second knowledge document, JSON schema, or crate duplicating protocol fields. New local fields ride the existing `extensions.nexus` contract or do not exist.
- **Adapters on one stack.** HTTP `/kb/` and `/kb/key-blocks/`, `nexus42 creator world kb …`, and extract-apply after the author gate are adapters over the same port stack; route stability stays with the adapter Master. MCA's World KB slot is a read adapter and does not call the retired User `KnowledgeStore`.
- **Retired faces are not adapters.** `kb/index.json` and `kb/entries/<id>.md` are not import, staging, or compatibility formats for this IR; UserScope and reference sources have no API on the port stack.
- **Authority split.** Holder/governance, the promotion state machine (manual / pending / confirmed / rejected), relationship taxonomy, OCC, and `/kb/` paths remain owned by their Masters. This record and the two §2 subsections state write-path ownership only; the rule is normative in [world-kb-runtime-architecture.md §2.2](../../../../docs/specs/architecture/world-kb-runtime-architecture.md#22-single-durable-writer-normative) and [spoke-adapter-architecture.md §2.3](../../../../docs/specs/architecture/spoke-adapter-architecture.md#23-sqlite-storage-shape), with the reasoning here.

## Alternatives considered

1. **A new Knowledge IR schema, document type, or crate beside `KnowledgeEntry`.** Rejected. The protocol already carries the content fields and `extensions.nexus` already carries the local extras; a second document would need its own mapper, its own disclosure path, and a reconciliation rule for a disagreement that the conversion seam should never have produced. It would also recreate the "four stores" shape the scatter-surface retirement removed.
2. **A parallel `KnowledgePort` trait to host the Simulate / preview stub.** Rejected. Two ports over one record drift: the stub would prove something about the wrong surface, and every new entry operation would need implementing twice. The cost accepted instead is that the stub implements the existing port's full method set and must be kept in step with it.
3. **Keep the retired scatter surfaces, including the withdrawn "file index as staging adapter" default.** Rejected. Each surface was an unfinished placeholder duplicating World-KB ownership, and a file index used as staging would reintroduce a second authoring format next to `body_json` with no owner. Withdrawing the default means workspace `kb/` trees have no reader; the code that produced them is gone.
4. **Fold relationship and extract-job writes into the same trait surface.** Rejected. They have different lifecycles — `needs_review` suggestions, job `version` CAS, a separate apply gate — so one trait would either grow unrelated methods or hide two state machines behind one name. They keep dedicated ports with the same stub policy.

## Consequences

**What it buys.** One auditable write path with one conversion seam, so "is there a stray writer?" is a repository grep rather than a review judgement. A dry-run cannot change durable state. The protocol stays the single content authority, which keeps HTTP, CLI, MCA reads, and extract-apply projecting the same record shape.

**What it costs, and how the cost is contained.** A stub must implement the port's full method set and is updated whenever the port grows — a review duty on port changes, not a runtime check. The sibling ports need their own non-apply stubs, which duplicates the stubbing pattern across three small surfaces. Any future persistence adapter (a test fixture store, a remote Spoke-backed store) must implement `KbStore` and must not invent a different entry shape; the port is the seam that makes such an adapter possible without touching callers.

**How it landed.** P0 recognized `KnowledgeEntryPort` + the conversion seam + `KbStore` as the port stack, with round-trip and stub tests. P1 closed the remaining direct writers onto `KbStore`. P2 bound the `KnowledgeEntryPort` stub in Simulate / extract-preview / Workflow Simulate. P3 dropped the leftover compile references and amended the two Masters in the same window. The P3 check, `rg -n "KnowledgeStore|reference_sources|kb/index\.json" crates apps --glob '*.rs' --glob '*.ts'`, now returns only the migration-history comment at `crates/nexus-local-db/src/version.rs:32`, test assertions in `crates/nexus-local-db/tests/migrations_apply.rs:48-55`, and a CLI test comment at `apps/nexus42/tests/kb_rescan_cli.rs:402` — no production reader survives, and nothing reads `kb/index.json`.

**Risks carried as consequences.** Stub drift is contained by review on port-surface changes; a persisting preview would now be a visible violation of §2.2 rather than an undefined case. Reconsider this record if Spoke knowledge content becomes remote-authoritative and local SQLite degrades to a cache — the write-path owner moves with the SSOT, and the two Masters move with it.

## References

- [2026-11-10-retire-user-scope-and-references.md](2026-11-10-retire-user-scope-and-references.md) — the retirement this design builds on (K1)
- [world-kb-runtime-architecture.md](../../../../docs/specs/architecture/world-kb-runtime-architecture.md) §2.2 (single durable writer), §5.5.7 (extract persistence through `KbStore`), §6 (OCC)
- [spoke-adapter-architecture.md](../../../../docs/specs/architecture/spoke-adapter-architecture.md) §2.3 (storage shape and write path), §7.1 (conversion-seam enforcement)
- [entity-scope-model.md](../../../../docs/specs/architecture/entity-scope-model.md) §5.1, §5.5–§5.6; [holder-governance.md](../../../../docs/specs/architecture/holder-governance.md); [llm-extract.md](../../../../docs/specs/orchestration/llm-extract.md)
- Spoke memory records: `implemented/architecture/2026-10-28-spoke-memory-protocol.md`, `proposed/architecture/2026-10-28-spoke-memory-adoption-plan.md` (a separate family; no `KnowledgeEntry` in memory)
- Sibling IRs: [PR #483](https://github.com/42ch-dev/nexus/pull/483) (Workflow IR), [PR #485](https://github.com/42ch-dev/nexus/pull/485) (Work Artifact IR) — they share the ports-and-stubs practice, not the document, types, or version
- Implementation evidence: `crates/nexus-spoke-adapter/src/conversion/knowledge_entry.rs`, `crates/nexus-spoke-adapter/src/adapter/knowledge_entry_port.rs`, `crates/nexus-knowledge/src/world_kb/store.rs`, `crates/nexus-local-db/src/kb_store.rs`, `crates/nexus-moment-context-assembly` World KB slot
