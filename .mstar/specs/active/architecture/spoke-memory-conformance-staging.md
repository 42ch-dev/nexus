# Spoke Memory Conformance Staging

> **Status:** Active architecture and staged-conformance contract.
> **Scope:** The independent durable provider used for cross-provider conformance, the exact 95-scenario disposition, and the mandatory N4 reflection/transfer remainders.
> **Related:** [Spoke Memory Provider Binding](spoke-memory-provider-binding.md).

## 1. Conformance model

Nexus conformance is capability-staged. The local provider and an independently implemented durable provider execute every applicable `spoke-memory` base scenario in its stated provider scope. An unavailable optional capability has explicit refusal coverage; refusal is not successful reflection or transfer behavior and does not complete an N4 success remainder.

A scenario id is never treated as evidence by itself. An execution record reports the scenario id, concrete selector, provider scope, observed result, and evidence location. The only complete N3-stage success statement is:

> capability-staged N3 matrix passed; full historical matrix not claimed

That statement is valid only when every N3, N3-refusal, N3-base, and reuse-plus-addition obligation in §5 has observed evidence. N4-only rows remain open work, not skips or implied passes.

## 2. Independent durable provider

### 2.1 Ownership and dependency boundary

`crates/nexus-memory-conformance` is a workspace test crate, not a production server. `FixtureMemoryProvider` is independently authored from the published SPOKE protocol and carries the workspace Apache-2.0 license. It does not copy the upstream fixture engine or SQLite-provider source. Any later copied upstream material separately requires preservation of upstream copyright/license notices and an amendment to this contract.

The fixture owns its file-backed SQLite database, migrations, policy, and bootstrap state. It does not depend on `LocalMemoryProvider`, `nexus-core` provider-private modules, `spoke-fixture-memory-engine`, a sibling checkout, an absolute path, an unpublished package, an echo service, or an in-memory substitute. Its integration tests may dev-depend on public `nexus-core` and `nexus42`; `nexus-core` never depends on the fixture crate.

The fixture directly consumes published `spoke-schemas` (through `spoke-operations`), `spoke-operations`, and `spoke-connect`. The current published SPOKE lockstep is `0.15.2` with `libp2p 0.57.1`; workspace manifests own the exact versions. No source gate is waived for an unpublished or assumed interface.

**Ablation: the repository-owned fixture implementation** — removal would leave no reproducible independent durable leg because the upstream policy engine is unpublished; it is retained for genuine cross-provider conformance.

### 2.2 Storage, restart, and bootstrap

Each scenario uses a fresh temporary directory and a fixture-owned SQLite file. The fixture schema owns spaces, subjects, grants, candidates, records, profiles, receipts, cursors, and idempotency state. Restart closes the harness-owned pool, responder, and transports and then opens the same file. Records, grants, candidate shells, receipts, and `authority_generation` survive that restart.

Operator bootstrap creates one space and its initial admin through fixture provisioning before serving. `MemoryAdmission` maps each authenticated peer id to a stable principal through an explicit peer-to-principal directory. A second peer receives only grants minted through `memory.grants.grant`; payload principal or creator values are ignored. Reconnect with the same identity seed maps to the same principal and deduplication namespace.

### 2.3 Protocol and server composition

The fixture implements the 25 base `MemoryPort` operations under `spoke-memory` with independently written deterministic admission, CAS, idempotency, candidate, recall, and profile policy. Published pure validators, digest functions, and grant helpers may be reused; Nexus local policy and storage code may not.

`memory.describe` advertises only `spoke-memory` and a fixture-distinct `engine_policy_version`. Reflection and transfer methods return explicit `op_unsupported`; Connect dispatch returns `capability_missing` when the negotiated sub-capability is absent. Neither family is silently emulated or advertised.

The published `MemoryPortService` owns the real 33-operation mapping, typed decode, unknown-operation refusal, and authenticated `MemoryAdmission`. `connect_responder` and `RemoteAdapter` own signed hello, negotiated-capability dispatch, and wire envelopes over the published loopback transport. Nexus `WsTransport` covers the separate WebSocket client seam. No Nexus memory-serving advertisement is added.

### 2.4 Exact received-request digest bridge

The fixture's `RemoteMemoryService` decorator delegates every invocation to the unchanged `MemoryPortService`. For a mutating operation it computes the RFC 8785 JCS SHA-256 digest directly from the received `serde_json::Value` before typed decode, then moves that same object into the inner service. Only the digest plus matching operation and provider identity travel in the per-invoke task-local scope.

The typed port consumes that scope through the private admission seam. A missing or mismatched scope fails closed before any effect. The implementation never reconstructs a digest from a normalized typed request and never clones, caches, logs, or persists the raw body in a request map.

The scope is established inside `invoke`, within the responder's per-request spawned task. Concurrent invocations therefore have isolated scopes. A detached invocation that outlives its response deadline retains its own digest until completion, and task destruction drops the scope. A future SDK upgrade that changes the same-task `MemoryPortService → MemoryPort` call chain requires renewed compatibility review.

Receipts and tombstones carry ids and digests, not raw request bodies. Explicit `"source_context_ids": []` and omission remain distinct digest inputs even though typed serialization can normalize them to the same request.

### 2.5 Receipts, authorization, and uncertain outcomes

A mutation's current authorization, replay resolution, effect, content-free receipt, deduplication row, and receipt target share one transaction. Receipt targets retain the content-free capability, subject, placement, selection, and visibility requirements of the original operation for both keyed and keyless receipts. A different-target grant or scoped admin cannot authorize disclosure. Cursor bindings retain effective authorization at the supplied instant so grant expiry does not silently re-filter a continuing page.

Only replay with the same provider-scoped idempotency key and identical payload, or `memory.receipts.get`, resolves an uncertain outcome. A changed payload under the same key produces `IDEMPOTENCY_CONFLICT`. Post-commit response loss is `OUTCOME_UNCERTAIN`; it is not reported as a clean no-effect error. Current authorization is rechecked before replay and before receipt reads.

### 2.6 Fault and clock control

`FixtureFaults` supplies bounded `BeforeCommit`, `AfterCommitBeforeResponse`, `AdmissionPause`, and `ResponseDrop` points plus a controllable clock. Fault state is test control, never durable provider authority. Crash/restart scenarios use bounded joins and timeouts. Assertions use protocol-visible behavior except for the explicitly local-physical L-family scenarios.

## 3. Scenario naming and scope

For a stem such as `c_020_capture_receipt_is_content_free`, provider scope `both` means two concrete libtest selectors:

```text
c_020_capture_receipt_is_content_free_local
c_020_capture_receipt_is_content_free_remote
```

A `local` or `remote` row uses only the named suffix. A selector without a suffix intentionally selects both provider tests:

```text
cargo test -p nexus-memory-conformance c_020_capture_receipt_is_content_free -- --nocapture
```

`N3 refusal` is a real negative behavioral scenario with zero effects. `N3 + N4` means the named base/refusal portion belongs to N3 and the explicitly named capability-success remainder belongs to N4; it is not a full-row success claim. `Reuse` identifies retained behavioral coverage and any separately named missing addition. These selectors define the conformance surface; they do not assert an observed result.

## 4. Durable N4 roadmap

The N4 remainders below are mandatory durable work. This staging contract does not authorize their implementation. Base operations already implemented under `spoke-memory`—including freeze/fence, CAS, idempotency, grants, receipts, and privacy—are not deferred merely because an older stage label names a later milestone.

### N4-R — reflection capability

- **Owner:** `fullstack-dev` implements local-provider policy; `fullstack-dev-2` implements independent-fixture policy; `qa-engineer` owns acceptance. PM coordinates and does not reassign fixture construction to QA.
- **Dependency:** explicit `spoke-memory-reflection` support on both providers; authorized selection/filter/grant policy; pinned basis and staleness; pending-receipt generation; deterministic failure and racing hooks. N4-R also supplies the long-running operation required by C-015.
- **Trigger:** explicit N4 reflection-slice authorization after N3, with a plan citing this specification.
- **Completion:** C-042, C-070, C-071, C-072, successful C-073, C-075, C-076, C-077 list/get, C-078, and C-079 pass on both providers; C-090 and C-100 reflection remainders pass; C-003 continues to pass on a host where reflection is deliberately unadvertised. C-015 is jointly completed with N4-T after reflection supplies the admitted long-running operation. Full reflection content equality is not required for FREE rows.

### N4-T — transfer, drain, and cutover

- **Owner:** `fullstack-dev` implements NXM-12 tooling and local transfer policy; `fullstack-dev-2` implements independent-fixture transfer policy; `qa-engineer` owns acceptance.
- **Dependency:** explicit `spoke-memory-transfer` on both providers; quiescent drain barrier; chunked export/import; grant adoption; rollback and authority-generation transitions. C-015, C-077 export, and the C-080/C-082 reflection slices additionally depend on N4-R.
- **Trigger:** explicit N4 transfer/lifecycle authorization after N3, with a plan citing this specification.
- **Completion:** C-015 jointly with N4-R, C-077 export jointly with N4-R, C-080 through C-089, C-094, and C-119 pass in their stated provider scopes; C-011, C-090, and C-100 transfer remainders pass; C-004 continues to pass where transfer is deliberately unadvertised. Rollback never deletes target-written history or creates dual authority.

## 5. Complete 95-scenario disposition

### A. Capability admission and discovery

| ID | Stage | Exact selector and provider scope | N4 remainder |
|---|---|---|---|
| C-001 | N3 | `c_001_describe_contract`; both | None |
| C-002 | N3 refusal | `c_002_known_and_unknown_ops_fail_closed`; both | None |
| C-003 | N3 refusal | `c_003_reflection_requires_subcapability` across state/list/generate; both | N4-R successful reflection rows |
| C-004 | N3 refusal | `c_004_transfer_requires_subcapability` across exports.create/get and imports.stage/activate/discard; both | N4-T successful transfer rows |
| C-005 | N3 | `c_005_describe_has_no_space_catalogue`; both | None |

### B. Space and subject lifecycle fencing

| ID | Stage | Exact selector and provider scope | N4 remainder |
|---|---|---|---|
| C-010 | N3 | `c_010_space_identity_and_generation_are_stable`; both | None |
| C-011 | N3 + N4 | `c_011_freeze_allowlist_base_operations`; both | N4-T export/import allowlist success |
| C-012 | N3 | `c_012_write_racing_freeze_has_one_admission_outcome`; both | None |
| C-013 | N3 | `c_013_subject_fence_applies_across_clients`; both | None |
| C-014 | N3 | `c_014_reversible_archive_fence_survives_restart`; both | None |
| C-015 | N4 | No N3 success selector; C-003/C-004 prove unavailable long-running families refuse; both | N4-R + N4-T: reflection supplies a pending receipt; transfer/drain proves export/cutover barrier |
| C-016 | N3 | `c_016_context_fence_does_not_collateral_fence`; both | None |
| C-017 | N3 | `c_017_stale_fence_revision_cannot_lost_update`; both | None |

### C. Capture, idempotency, and receipts

| ID | Stage | Exact selector and provider scope | N4 remainder |
|---|---|---|---|
| C-020 | N3 | `c_020_capture_receipt_is_content_free`; both | None |
| C-021 | N3 | `c_021_identical_capture_retry_replays`; both | None |
| C-022 | N3 | `c_022_changed_payload_idempotency_conflict`; both | None |
| C-023 | N3 | `c_023_response_loss_replays_one_effect`; both | None |
| C-024 | N3 | `c_024_midcommit_crash_resolves_uncertain_receipt`; both | None |
| C-025 | N3 | `c_025_foreign_capture_persists_nothing`; both | None |
| C-026 | N3 | `c_026_capture_to_fenced_target_has_no_effect`; both | None |
| C-027 | N3 | `c_027_receipts_never_leak_deleted_content`; both | None |
| C-028 | N3 | `c_028_receipt_and_dedup_retention_windows`; both | None |
| C-029 | N3 | `c_029_reconnect_preserves_principal_and_receipt_remote`; remote | None |

### D. Candidate lifecycle

| ID | Stage | Exact selector and provider scope | N4 remainder |
|---|---|---|---|
| C-030 | N3 | `c_030_candidate_pages_are_bounded_and_scoped`; both | None |
| C-031 | N3 | `c_031_review_consumes_and_creates_atomically`; both | None |
| C-032 | N3 | `c_032_dismiss_replay_never_resurrects`; both; canonical-bearer regression remains in `nexus-core` | None |
| C-033 | N3 | `c_033_concurrent_review_has_exactly_one_effect`; both | None |
| C-034 | N3 | `c_034_review_inherits_scope_and_requires_dual_promote_grant`; both | None |
| C-035 | N3 refusal | `c_035_merge_review_action_is_explicitly_unsupported`; both | None |
| C-036 | N3 | `c_036_replay_does_not_recheck_current_fence_or_cas`; both | None |
| C-116 | N3 | `c_116_binding_local_review_cannot_widen_visibility`; both | None |

### E. Retained records

| ID | Stage | Exact selector and provider scope | N4 remainder |
|---|---|---|---|
| C-040 | N3 | `c_040_record_crud_and_stale_cas`; both | None |
| C-041 | N3 | `c_041_revision_changes_once_per_real_update`; both | None |
| C-042 | N4 | No reflection-basis selector; both | N4-R all basis-affecting changes invalidate state |
| C-043 | N3 | `c_043_record_pages_are_bounded_and_stable`; both | None |
| C-044 | N3 | `c_044_record_promote_checks_grant_and_freeze`; both | None |
| C-045 | N3 | `c_045_advertised_content_bounds_are_enforced`; both | None |
| C-046 | N3 | `c_046_base_read_operations_do_not_mutate`; both | Successful reflection reads remain C-073/N4-R |
| C-047 | N3 | `c_047_by_id_reads_recheck_current_authorization`; both | None |

### F. Recall and read selection

| ID | Stage | Exact selector and provider scope | N4 remainder |
|---|---|---|---|
| C-050 | N3 | `c_050_subject_all_requires_unrestricted_grant`; both | None |
| C-051 | N3 | `c_051_shared_only_excludes_context_records`; both | None |
| C-052 | N3 | `c_052_selected_contexts_can_include_shared`; both | None |
| C-053 | N3 | `c_053_selected_contexts_can_exclude_shared`; both | None |
| C-054 | N3 | `c_054_recall_never_returns_candidates`; both | None |
| C-055 | N3 | `c_055_recall_enforces_utf8_budget_and_overflow`; both | None |
| C-056 | N3 | `c_056_cursor_invalid_after_grant_or_state_change`; both | None |
| C-057 | N3 | `c_057_missing_or_null_selection_is_invalid_input`; both | None |
| C-057A | N3 | `c_057a_scoped_grant_cannot_widen_recall`; both | None |
| C-058 | N3 | `c_058_ranking_is_deterministic_per_provider`; both | None |
| C-059 | N3 | `c_059_provenance_filter_is_anded_with_placement`; both | None |

### G. Profile documents

| ID | Stage | Exact selector and provider scope | N4 remainder |
|---|---|---|---|
| C-060 | N3 | `c_060_profile_content_is_byte_preserved`; both | None |
| C-061 | N3 | `c_061_profile_update_enforces_cas`; both | None |
| C-062 | N3 | `c_062_profile_and_capability_admission_are_distinct`; both | None |
| C-063 | N3 | `c_063_capture_never_mutates_profile`; both | None |
| C-064 | N3 | `c_064_selection_requires_no_fake_profile_kinds`; both | None |

### H. Reflections

| ID | Stage | Exact selector and provider scope | N4 remainder |
|---|---|---|---|
| C-070 | N4 | C-003 proves unavailable generation refuses; both | N4-R reflect grant and committed/pending generation |
| C-071 | N4 | No generated-basis selector; both | N4-R terminal receipt and pinned-basis listing |
| C-072 | N4 | No generated-envelope selector; both | N4-R valid envelope with provider-variable content |
| C-073 | N3 refusal + N4 | `c_073_reflection_reads_refuse_without_capability`; both | N4-R successful state/list, sufficiency, and no mutation |
| C-075 | N4 | C-057A and C-090 base scopes run in N3; both | N4-R derived-text subject-wide refusal |
| C-076 | N4 | No racing-generation selector; both | N4-R publish-pinned basis and honest staleness |
| C-077 | N4 | No derived-content-deletion selector; both | N4-R tombstoned bases excluded from list/get; N4-T same exclusion for export |
| C-078 | N4 | No generation-cost selector; both | N4-R failure honesty and no double charge |
| C-079 | N4 | No generated-sufficiency selector; both | N4-R insufficient-data policy outcomes |

### I. Export, import, and recovery

| ID | Stage | Exact selector and provider scope | N4 remainder |
|---|---|---|---|
| C-080 | N4 | C-004 proves unavailable export refuses; both | N4-T bounded chunks, digests, slices, and privacy |
| C-081 | N4 | No staging selector; both | N4-T ordered invisible staging and id map |
| C-082 | N4 | No activation selector; both | N4-T atomic activation, import refs, and stale reflections |
| C-083 | N4 | No discard selector; both | N4-T discard leaves no residue |
| C-084 | N4 | No cutover-rollback selector; both | N4-T both rollback cases without history loss or dual authority |
| C-085 | N4 | No discarded-activation selector; both | N4-T terminal import conflict without partial activation |
| C-086 | N4 | No export-expiry selector; both | N4-T expired chunks refuse |
| C-087 | N4 | No transfer-crash selector; both | N4-T frozen source, replayable receipts, and explicit resume |
| C-088 | N4 | No imported-grant selector; both | N4-T grants activate only by explicit adoption |
| C-089 | N4 | N3 pins generation; no cutover-success selector; both | N4-T old-authority and stale-generation writes refuse |
| C-094 | N4 | No import-identity-map selector; both | N4-T same-id strings merge only through explicit staged maps |

### J. Grants and concurrency

| ID | Stage | Exact selector and provider scope | N4 remainder |
|---|---|---|---|
| C-090 | N3 + N4 | `c_090_grant_scope_covers_base_surfaces`; both | N4-R reflection surfaces; N4-T export surfaces |
| C-091 | N3 | `c_091_revoke_applies_at_admission_boundary`; both | None |
| C-092 | N3 | `c_092_concurrent_record_update_has_one_winner`; both | None |
| C-093 | N3 | `c_093_revoke_or_fence_race_has_one_outcome`; both | None |
| C-095 | N3 | `c_095_scoped_admin_cannot_amplify_grant`; both | None |
| C-096 | N3 | `c_096_operator_bootstrap_is_callable_and_visible`; both | None |

### K. Error envelopes and retry classes

| ID | Stage | Exact selector and provider scope | N4 remainder |
|---|---|---|---|
| C-100 | N3 + N4 | `c_100_supported_error_envelopes_and_retry_classes`; both | N4-R and N4-T capability-specific error classes |
| C-101 | N3 refusal | `c_101_unknown_op_and_missing_capability_are_distinct`; both | None |
| C-102 | N3 | `c_102_foreign_and_missing_ids_have_no_existence_oracle`; both | None |
| C-103 | N3 | `c_103_internal_error_reports_effect_honesty`; both | None |
| C-104 | N3 | `c_104_receipt_read_rechecks_revoked_authorization`; both | None |

### L. Nexus local-provider specifics

| ID | Stage | Exact selector and provider scope | N4 remainder |
|---|---|---|---|
| C-110 | Reuse N3 | Existing `cargo test -p nexus-core --test local_memory_provider out_of_guard_writes_are_writer_fenced`; local | None |
| C-111 | N3 behavioral | `c_111_creator_cli_uses_pinned_provider_local`; local, and `c_111_creator_cli_uses_pinned_provider_remote`; remote; no source-grep substitute | None |
| C-112 | N3 | `c_112_local_layout_and_kind_frontmatter_preserved_local`; local | None |
| C-113 | Reuse + add N3 | Existing `cargo test -p nexus-core --test memory_reconcile crash_before_journal_commit_leaves_no_effect_and_frees_the_key` and `cargo test -p nexus-core --test memory_reconcile crash_after_journal_commit_is_withheld_then_settled_by_a_replay`; add `c_113_recovering_objects_excluded_until_resolved_local`; local | None |
| C-114 | Reuse N3 | Existing `cargo test -p nexus-core --test local_memory_provider bearer_path_validation_is_unchanged`; local | None |
| C-115 | N3 | `c_115_remember_true_refusal_unchanged_local`, covering Character and non-Character arms; local | None |
| C-117 | Reuse N3 | Existing `cargo test -p nexus-core --test local_memory_provider control_metadata_migration_is_idempotent_and_writer_registered`; local | None |
| C-118 | Reuse N3 | Existing `cargo test -p nexus-core --test creator_capture creator_capture_review_promote_round_trip`; local | None |
| C-119 | N4 | C-004 proves transfer is unavailable in N3; local | N4-T quiescent SQL+file snapshot and verified restore |

## 6. Accounting and review rules

N3-stage acceptance accounts for every N3, N3-refusal, N3 + N4 base portion, and reuse-plus-addition row in its exact provider scope. N4-R and N4-T rows remain explicitly open with their owners, dependencies, triggers, and completion definitions intact.

Reviewers must not:

- collapse N4 obligations into `not applicable`, `skipped`, or an implied pass;
- count unavailable-capability refusal as successful reflection or transfer;
- replace a behavioral selector with a source-grep assertion;
- infer a complete-matrix result from a scenario name, selector existence, private unit test, or technical design;
- defer an implemented base operation solely because a historical stage column names N4.

The table is the durable scenario SSOT. Execution evidence remains in the applicable tracked acceptance report rather than being embedded as historical claims in this architecture specification.
