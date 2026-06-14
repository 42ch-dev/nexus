# QA Report — V1.46 P0 author-desk-status-ux

## Scope tested
- **plan_id**: `2026-06-14-v1.46-author-desk-status-ux`
- **Working branch (verified)**: `iteration/v1.46`
- **Review cwd (verified)**: `/Users/bibi/workspace/organizations/42ch/nexus`
- **Review range / Diff basis**: `merge-base: c9fb1abb (original P0 integrated HEAD before qc-consolidated) → tip: bb0deae9 (current iteration/v1.46 HEAD after fix + qc revalidations)` — equivalent to `git diff c9fb1abb..bb0deae9`. The full code delta is `c9fb1abb..52a7330d` (P0 + fix); the `f54c928d` and `bb0deae9` commits are qc report docs only.
- **Files reviewed**: `crates/nexus42/src/commands/creator/works/mod.rs` (primary), `.mstar/knowledge/specs/novel-author-experience.md`, `.mstar/plans/2026-06-14-v1.46-author-desk-status-ux.md`, all three qcN.md + qc-consolidated.md, `.mstar/status.json` residual section.
- **Commits covered**: Original P0 (26a09085–a134a98f + c9fb1abb docs), fix round (36b96205–04bd7aca + 52a7330d merge), qc revalidations (f54c928d, bb0deae9).

## Acceptance criteria evidence
**CI gates (mandatory, executed on `iteration/v1.46` at `bb0deae9`):**
- `cargo clippy --all -- -D warnings` → exit 0, zero warnings.
  ```
  Blocking waiting for file lock on package cache
  Blocking waiting for file lock on package cache
  Blocking waiting for file lock on build directory
  Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.41s
  ```
- `cargo test -p nexus42 --lib -- 'works::tests'` → **47 passed, 0 failed, 0 ignored**.
  ```
  test commands::creator::works::tests::enrich_novel_stale_inserts_findings_stale ... ok
  ...
  test commands::creator::works::tests::fetch_novel_findings_and_stale_runs_concurrently ... ok
  test commands::creator::works::tests::fetch_novel_findings_and_stale_degrades_when_findings_fail ... ok
  test commands::creator::works::tests::fetch_stale_findings_returns_none_on_endpoint_error ... ok
  test commands::creator::works::tests::stale_fetch_timeout_matches_findings_fetch_timeout ... ok
  test commands::creator::works::tests::enrich_findings_truncated_marker_set_when_at_limit ... ok
  test commands::creator::works::tests::enrich_findings_truncated_omitted_when_below_limit ... ok
  test commands::creator::works::tests::enrich_findings_truncated_omitted_when_empty ... ok

  test result: ok. 47 passed; 0 failed; 0 ignored; 0 measured; 632 filtered out; finished in 0.68s
  ```
- `cargo +nightly fmt --all --check` → silent (exit 0, pass).

**Per-AC test mapping + result lines (all 47 under the exact filter; grouped by original P0 vs fix-round ACs):**

**Original P0 acceptance criteria (from plan §4):**
- AC1 (`creator works status <novel_work_id> --json` includes non-empty `findings[]` when open findings exist; matches list API shape verbatim; daemon fields preserved; degradation behavior):
  - `enrich_novel_with_findings_inserts_array` ... ok
  - `enrich_preserves_daemon_work_fields` ... ok
  - `enrich_novel_unavailable_findings_omits_key` ... ok (extended in fix round to also cover `findings_truncated` absent on unavailable)
  - `enrich_novel_stale_inserts_findings_stale` ... ok
- AC2 (Human output uses per-finding hints; no blanket `reflection-loop` footer; empty → `novel-review-master`):
  - `display_findings_with_severity_summary` ... ok
  - `display_no_open_findings` ... ok
  - `display_findings_completed_work_shows_summary` ... ok
  - `display_truncated_findings_shows_plus_indicator` ... ok
- AC3 (Generic work `works status` does not fetch findings (human + json)):
  - `enrich_generic_work_omits_findings_gate` ... ok
- AC4 (Unit/integration tests cover JSON shape + novel-only gate): All 47 tests in `works::tests` (40 baseline + 7 new) + clippy/fmt clean.
- AC5 (Spec §4.1 documents JSON contract + novel-only gate): Verified (see Spec / scope discipline below).

**Fix-round acceptance criteria (from qc-consolidated.md + fix Assignment):**
- F-001 (`handle_status` JSON branch uses `tokio::join!` for findings + stale (avoid ~35 s stacked worst-case); degradation when findings fail):
  - `fetch_novel_findings_and_stale_runs_concurrently` ... ok (timing assertion: < 700ms vs ~800ms sequential)
  - `fetch_novel_findings_and_stale_degrades_when_findings_fail` ... ok
- F-002 (stale fetch uses short timeout (~5 s) matching `FINDINGS_FETCH_TIMEOUT`):
  - `stale_fetch_timeout_matches_findings_fetch_timeout` ... ok
  - `fetch_stale_findings_returns_none_on_endpoint_error` ... ok
- F-003 (`findings_truncated` (or equivalent marker) present in JSON output when `findings.len() == FINDINGS_FETCH_LIMIT` (50); omitted below/empty/unavailable):
  - `enrich_findings_truncated_marker_set_when_at_limit` ... ok
  - `enrich_findings_truncated_omitted_when_below_limit` ... ok
  - `enrich_findings_truncated_omitted_when_empty` ... ok
  - (plus extension of `enrich_novel_unavailable_findings_omits_key` asserting marker absent on unavailable)
- S-1 (dead `let _ = work_id;` and its comment removed): Confirmed in qc1 revalidation (commit `e07d4538`); `print_findings_summary` now ends cleanly after findings for-loop; clippy clean; `work_id` legitimately used in empty-findings branch.
- S-001 (plan §6 Verification command corrected to `cargo test -p nexus42 --lib -- 'works::tests'`): Confirmed in plan file + qc1/qc3 revals; the exact filter now matches 47 tests (previously listed command matched zero).

All tests are present in the test binary and pass. No CI failures attributable to scope.

## Spec / scope discipline
- `git diff c9fb1abb..52a7330d -- .mstar/knowledge/specs/novel-author-experience.md` (excerpt):
  ```diff
  @@ -144,15 +144,20 @@ For **`work_profile=novel`** only, `creator works status <work_id> --json` **ext
   | --- | --- | --- | --- |
   | *(work fields)* | object | yes | Unchanged from daemon GET `/v1/local/works/{id}` |
   | `findings` | array | yes | Same element shape as findings list API; empty array if none |
  +| `findings_truncated` | boolean | no | Present (and `true`) only when `findings[]` hit the fetch cap (`FINDINGS_FETCH_LIMIT = 50`); signals more open findings may exist beyond the fetched page. Omitted otherwise (qc3 F-003) |
   | `findings_stale` | object | no | Present when 96h master-review stale banner would show (human parity) |

   ...

   **Best-effort degradation**: `findings` is fetched via the daemon findings endpoint
  -with a short timeout. When that endpoint is unreachable, `findings` is **omitted**
  -(rather than fabricated as an empty array) so a JSON consumer can distinguish a
  -genuinely findings-free Work from a transient daemon fault. `findings_stale`
  -follows the same novel-only, best-effort contract.
  +with a short timeout (`FINDINGS_FETCH_TIMEOUT`, 5 s). When that endpoint is
  +unreachable, `findings` is **omitted** (rather than fabricated as an empty array)
  +so a JSON consumer can distinguish a genuinely findings-free Work from a
  +transient daemon fault. `findings_stale` follows the same novel-only,
  +best-effort contract and uses a matching short timeout (`STALE_FETCH_TIMEOUT`,
  +5 s; qc3 F-002) so neither subcall can block the JSON status command beyond ~5 s.
  +The two subcalls run concurrently (`tokio::join!`; qc3 F-001), bounding the
  +JSON-path fetch by the slower of the two rather than their sum.
  ```
  Only F-003 row addition + best-effort paragraph update (F-001/F-002 notes). **W-1** ("Required: yes" vs omission-on-unreachable) and **W-2** (`findings_stale` creator-global scope) are untouched — explicitly deferred to P1 per `qc-consolidated.md`.
- Pre-existing human stale banner quickstart refs (`docs/novel-writing-quickstart.md §5` etc.) remain untouched (P1 scope).
- `creator run` command surface NOT modified (P1 scope).
- `status.json` residuals (verified):
  ```
  4 open residuals:
    R-V146P0-QC1-S2: low - capture_findings_output test helper duplicates print_findings_summary ...
    R-V146P0-QC2-S1: low - enrich_status_json tests use minimal finding_json helper; full element...
    R-V146P0-QC3-S2: low - JSON-path findings/stale degradation paths lack tracing observability...
    R-V146P0-QC3-S3: low - Consider skipping stale fetch when findings fetch already failed...
  ```
  Exactly the 4 low-severity items listed in `qc-consolidated.md`; **NOT closed** by this round (per PM disposition).

## Findings
- **Critical**: 0
- **Warning**: 0 (all initial Warnings either fixed in scope or deferred to P1 per consolidated disposition)
- **Suggestion**: 0 new (prior S items resolved or carried as the 4 open low residuals above)

All three QCs (qc1/qc2/qc3) revalidated to **Approve** on the final basis (`bb0deae9` / `f54c928d` targeted re-reviews). No new blocking findings introduced by fix round.

## Recommended owners
- **PM**: Mark P0 `Done` (after this QA sign-off + commit of qa.md). Update `status.json` plan row to `Done`. Leave the 4 low residuals open (they have explicit V1.46+ targets per consolidated).
- **P1 plan** (`2026-06-14-v1.46-spec-cli-hygiene`): W-1, W-2, pre-existing quickstart refs, runtime remediation chain sweep.
- Residual owners per `qc-consolidated.md` table (deferred to V1.46+).

## Reproduction steps
All commands run from project root on `iteration/v1.46` at `bb0deae9` (working tree clean):
```bash
git rev-parse --show-toplevel   # /Users/bibi/workspace/organizations/42ch/nexus
git branch --show-current       # iteration/v1.46
git log -1 --oneline            # bb0deae9 qc(v1.46-p0): qc1 revalidation (targeted re-review)
git status --short              # (empty)

cargo clippy --all -- -D warnings
cargo test -p nexus42 --lib -- 'works::tests'
cargo +nightly fmt --all --check

git diff c9fb1abb..52a7330d -- .mstar/knowledge/specs/novel-author-experience.md
python3 -c 'import json,sys; d=json.load(sys.stdin); r=d["residual_findings"]["2026-06-14-v1.46-author-desk-status-ux"]; print(len(r)); [print(f"  {x[\"id\"]}: {x[\"severity\"]}") for x in r]' < .mstar/status.json
```
All outputs match the excerpts above.

## Not tested
- Manual CLI smoke (`nexus42 creator works status <novel_id> --json | jq '.findings'`) — hermetic test fixtures cover the contract; daemon fixture not required for this QA scope (per assignment).
- Full workspace `cargo test --all` (time-permitting only; the 47 tests under the exact filter + clippy/fmt are the mandated gates for this feature scope).
- W-1/W-2 remediation or P1 items (explicitly out of scope).
- Pre-existing human-path quickstart references (P1).

## QA Verdict
**PASS**

All original P0 + fix-round acceptance criteria verified end-to-end with reproducible evidence. CI gates green. Spec/scope discipline holds (only F-003 + best-effort updates; W-1/W-2 and 4 low residuals untouched per PM disposition). Checkout alignment, review range, and plan_id text-identical to QC re-review basis. No implementation changes made by QA. Ready for PM to mark P0 `Done`.
