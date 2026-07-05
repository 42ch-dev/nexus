---
report_kind: qa
plan_id: "2026-06-13-v1.44-multi-volume-hardening"
verdict: "Approve"
generated_at: "2026-06-13T03:56:15Z"
---

# QA Report — V1.44 P2 Multi-Volume Hardening

## Reviewer Metadata

- Reviewer: @qa-engineer
- Runtime Agent ID: qa-engineer
- Runtime Model: volcengine-plan/ark-code-latest
- Report Timestamp: 2026-06-13T03:56:15Z
- QA Mode: Report-only verification; no implementation code modified
- QC Baseline: qc1.md, qc2.md, qc3.md, and qc-consolidated.md all `Approve`

## Scope

- plan_id: `2026-06-13-v1.44-multi-volume-hardening`
- Review range / Diff basis: `c54b1aa6..9c53d8f6`
- Working branch (verified): `iteration/v1.44`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- Commit range verified:
  ```text
  9c53d8f6 merge(v1.44 P2): multi-volume completion + supervisor volume propagation
  b7d27aa7 style(P2): nightly fmt fixes after T4 regression tests
  233bc3f2 test(P2-T4): add multi-volume completion + volume propagation regression tests
  22324ddc fix(P2-T1/T2/T3): harden multi-volume completion + thread volume through supervisor chain
  ```
- Scope in: P2 only — commits `22324ddc..b7d27aa7` plus merge `9c53d8f6` on `iteration/v1.44`
- Scope out: P0/P1/P3/P-last implementation and QC report authorship

## Verification (per Acceptance Criteria)

### AC1 — 2-volume Work completes only when all volume rows finalized + correct progress predicate

**Verdict: Pass.**

Behavior verified from implementation and tests:

- `crates/nexus-local-db/src/work_chapters.rs::is_work_completed` now performs a volume-safe aggregate across `work_chapters` rows:
  - `COUNT(*) AS total_rows`
  - `SUM(CASE WHEN status = 'finalized' THEN 1 ELSE 0 END) AS finalized_rows`
  - completion returns true only when `total_rows == total_planned_chapters` and `finalized_rows == total_planned_chapters`, with `intake_status == 'complete'`.
- This removes the stale flat `current_chapter >= total_planned_chapters` predicate that is fragile when chapter numbers repeat per volume.
- Hermetic local-db tests cover:
  - all 6 rows across 2 volumes finalized → completed
  - vol 2 contains a draft row → not completed
  - total says 6 but only 3 rows exist → not completed
- Hermetic orchestration integration tests additionally cover the all-finalized and partial-volume cases through the cross-crate path.

Evidence:

```text
cargo test -p nexus-local-db
...
test work_chapters::tests::test_is_work_completed_multi_volume_all_finalized ... ok
test work_chapters::tests::test_is_work_completed_multi_volume_partial_vol2 ... ok
test work_chapters::tests::test_is_work_completed_multi_volume_missing_vol2_rows ... ok
...
test result: ok. 190 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 9.39s
...
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.07s
...
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
...
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.08s
...
test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.12s
...
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.29s
```

### AC2 — After volume boundary, enqueued `novel-writing` schedule `preset.input` includes correct `volume` + `chapter`

**Verdict: Pass.**

Behavior verified from implementation and regression tests:

- `WorkFields` includes `volume: Option<i32>`.
- `build_preset_input` injects `"volume": N` when `WorkFields.volume` is present.
- `build_auto_chain_schedule` and `enqueue_auto_chain_schedule` thread the volume value through to schedule input.
- The supervisor `ChainAction::NextChapter` path passes `Some(next_volume)` into `enqueue_auto_chain_step`, preserving cross-volume context at enqueue time.
- Regression test `f004_supervisor_enqueue_includes_volume_in_preset_input` verifies a volume-boundary schedule has `preset_id == "novel-writing"`, `input["chapter"] == 1`, and `input["volume"] == 2`.
- Negative test `f004_single_volume_enqueue_has_no_volume_in_input` verifies single-volume enqueue using `None` does not add a `volume` key.

Evidence:

```text
cargo test -p nexus-orchestration --test supervisor_cross_volume
...
test f004_supervisor_enqueue_includes_volume_in_preset_input ... ok
test f004_single_volume_enqueue_has_no_volume_in_input ... ok
...
test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.44s
```

### AC3 — Existing `supervisor_cross_volume.rs` tests remain green; failing-case regression covered

**Verdict: Pass.**

The full `supervisor_cross_volume.rs` test target ran successfully. It includes the existing F-001 cross-volume tests and the P2 F-002/F-004 regression cases, including negative cases for partial vol 2 completion and single-volume volume-key omission.

Evidence:

```text
cargo test -p nexus-orchestration --test supervisor_cross_volume
    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.16s
     Running tests/supervisor_cross_volume.rs (target/debug/deps/supervisor_cross_volume-d2796f32367d8b3d)

running 8 tests
test f002_multi_volume_work_not_completed_partial_vol2 ... ok
test f001_volume_aware_evaluator_work_complete ... ok
test f001_single_volume_all_finalized_marks_complete ... ok
test f004_single_volume_enqueue_has_no_volume_in_input ... ok
test f001_volume_aware_evaluator_picks_vol2_ch1 ... ok
test f002_multi_volume_work_completed_all_volumes_finalized ... ok
test f004_supervisor_enqueue_includes_volume_in_preset_input ... ok
test f001_cross_volume_supervisor_enqueues_vol2_chapter1 ... ok

test result: ok. 8 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.44s
```

### AC4 — Residual scopes in `status.json` corrected from stale `auto_chain.rs is_work_completed` to `work_chapters.rs`

**Verdict: Pass.**

`status.json` root `residual_findings["2026-06-11-v1.42-multi-volume"]` has the correct P2-relevant scope pointers:

- `R-V142P1-QC1-F-002` scope is `crates/nexus-local-db/src/work_chapters.rs::is_work_completed (flat current_chapter + list_chapters len vs total_planned_chapters)`.
- `R-V142P1-QC1-F-004` scope is `crates/nexus-orchestration/src/schedule/supervisor.rs NextChapter arm; build_auto_chain_schedule / WorkFields.volume`.

No stale `auto_chain.rs is_work_completed` pointer remains for F-002 in the verified residual entries.

### AC5 — F-002 + F-004 marked for resolution

**Verdict: Pass for P2 QA target; formal lifecycle closure remains P-last as assigned.**

Observed status:

- `R-V142P1-QC1-F-002` has `target: "V1.44 P2 (2026-06-13-v1.44-multi-volume-hardening)"`, correct scope, and behavior verified by AC1 tests.
- `R-V142P1-QC1-F-004` has `target: "V1.44 P2 (2026-06-13-v1.44-multi-volume-hardening)"`, correct scope, and behavior verified by AC2/AC3 tests.
- QC consolidated report records both original residuals as resolved by behavior, while noting formal `lifecycle: resolved` in `status.json` is P-last scope.

I did not modify `status.json` in this report-only QA. The entries remain traceably marked for V1.44 P2 resolution and are ready for P-last lifecycle closeout.

## Regression Behavior

- `cargo test -p nexus-orchestration --test supervisor_cross_volume`: green, 8/8 passed.
- `cargo test -p nexus-local-db`: green across unit tests, integration tests, and doc tests.
- `cargo test -p nexus42 --lib`: green, 640/640 passed.
- `cargo clippy --all -- -D warnings`: green.
- `cargo +nightly fmt --all --check`: green.

No behavior regression was observed in the P2 verification scope.

## Behavior Observation

The implementation behavior matches the P2 plan’s corrected multi-volume interpretation of novel completion: completion is now based on all seeded `work_chapters` rows across volumes being finalized and the planned row count being present. This is the behavior needed after the V1.42 `(work_id, volume, chapter)` PK migration where chapter numbers can repeat per volume.

The normative spec section `.mstar/knowledge/specs/novel-writing/workflow-profile.md` §6.1 still contains legacy wording that names `current_chapter >= total_planned_chapters`; P2 intentionally hardens beyond that stale flat predicate per the plan problem statement. The tested behavior aligns with the P2 plan and the multi-volume invariant under §4.5.4–§6.1.

## Evidence

### Checkout and review range

```text
git rev-parse --show-toplevel
/Users/bibi/workspace/organizations/42ch/nexus

git branch --show-current
iteration/v1.44

git log --oneline c54b1aa6..9c53d8f6
9c53d8f6 merge(v1.44 P2): multi-volume completion + supervisor volume propagation
b7d27aa7 style(P2): nightly fmt fixes after T4 regression tests
233bc3f2 test(P2-T4): add multi-volume completion + volume propagation regression tests
22324ddc fix(P2-T1/T2/T3): harden multi-volume completion + thread volume through supervisor chain
```

### Required commands

```text
cargo test -p nexus-orchestration --test supervisor_cross_volume
Result: ok. 8 passed; 0 failed; finished in 0.44s

cargo test -p nexus-local-db
Result: ok. Unit tests 190 passed; migrations_apply 2 passed; pool_smoke 1 passed; v142_migration_fixes 2 passed; versions_roundtrip 3 passed; doc-tests 2 passed.

cargo test -p nexus42 --lib
Result: ok. 640 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 30.21s
Note: command output was long and stored by OpenCode at /Users/bibi/.local/share/opencode/tool-output/tool_ebf1e5e35001uSfIRcJq9IfhMD.

cargo clippy --all -- -D warnings
Result: Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.25s

cargo +nightly fmt --all --check
Result: no output; exit 0
```

## Summary

| Item | Result |
| --- | --- |
| AC1 multi-volume completion predicate | Pass |
| AC2 volume + chapter schedule input | Pass |
| AC3 supervisor cross-volume regression suite | Pass |
| AC4 residual scope correction | Pass |
| AC5 F-002/F-004 marked for P2 resolution | Pass; formal status lifecycle closure remains P-last |
| Required tests | Pass |
| Clippy | Pass |
| Nightly fmt check | Pass |
| Behavior regression | None observed |

## Verdict

**Approve** — V1.44 P2 meets the five assigned acceptance criteria for QA verification, resolves the behavior behind `R-V142P1-QC1-F-002` and `R-V142P1-QC1-F-004`, and shows no regression in the required test/lint/format gates.
