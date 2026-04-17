# ForkBranch contract alignment (TD-7)

**Source plan:** `v1-tech-debt-cleanup` (Batch D, Task 13 / TD-7)  
**Status:** Active — verification record for OSS handoff

## Scope

Align naming and semantics for fork provenance fields across:

- JSON Schema: `schemas/domain/fork-branch.schema.json` (`parent_branch_id`, `forked_from_event_id`)
- Generated wire type: `crates/nexus-contracts/src/generated/fork_branch.rs`
- Domain aggregate: `crates/nexus-domain/src/fork_branch.rs`

## Verification (2026-04-10)


| Layer                         | `parent_branch_id` | `forked_from_event_id` |
| ----------------------------- | ------------------ | ---------------------- |
| Schema                        | Required string    | Required string        |
| `nexus_contracts::ForkBranch` | `String`           | `String`               |
| `nexus_domain::ForkBranch`    | `String`           | `String`               |


`From` conversions in `fork_branch.rs` map both fields 1:1 in each direction (domain ↔ contracts).

## Automated guard

`crates/nexus-domain/src/contract_assertions.rs` includes `test_fork_branch_parent_branch_and_event_ids_roundtrip`, which asserts values survive domain → contract → domain roundtrip.

## Related

- Architecture alignment baseline: [architecture-alignment-review-v1.md](architecture-alignment-review-v1.md)