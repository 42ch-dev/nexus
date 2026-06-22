---
plan_id: 2026-06-22-v1.58-df44-reference-refresh-pipeline
reviewer: qc-specialist-2
reviewer_index: 2
focus: security-correctness
review_cwd: /Users/bibi/workspace/organizations/42ch/nexus
working_branch: iteration/v1.58
diff_basis: d443e855..af82ad39
reviewed_at: 2026-06-22T05:48:05Z
verdict: Request Changes
---

# QC2 — V1.58 P1 DF-44 Reference Refresh — Security/Correctness Review

## Summary

P1 delivers the core DF-44 refresh pipeline: DB migration adding `last_refreshed_at`/`refresh_policy`/`refresh_status` to `reference_sources`, the `nexus.reference.refresh` capability handler (with policy gating and blake3 hash-compare), DAO lifecycle methods, daemon-side `refresh_scheduler` hook, roster extension, `capability-registry.md` updates, and a new Draft `reference-knowledge.md`.

The implementation is surgically scoped (P1 = capability + migration + scheduler + spec; CLI + cross-cut E2E + on-disk body write deferred to P3). Most correctness properties hold for the declared scope, but one **High** security/correctness gap exists around network egress hygiene on the fetch path.

## Findings

### High severity

- **H-001 — Refresh fetch path lacks HTTPS-only and private-IP blocking (SSRF / unintended network access)**  
  `crates/nexus-orchestration/src/capability/builtins/reference_refresh.rs:166` does `HTTP_CLIENT.get(fetch_url).send()` using a plain `reqwest::Client` (only timeout + user-agent; no scheme or host validation).  
  Contrast with the V1.57 precedent in the same crate: `registry.rs:172` (`validate_cdn_url_static`) + `is_blocked_ip` (rejects non-https, private/loopback/link-local/169.254.0.0/16, and IPv6 equivalents).  
  Reference source `uri` values are user-controlled (registered via prior flows). An attacker or misconfigured source can point at `http://...` or internal hosts (e.g., `http://169.254.169.254/`, `http://10.0.0.1/`, `http://localhost:8080/`).  
  Impact: SSRF, internal service enumeration, potential data exfiltration or side-effect mutations inside the user's network.  
  This directly matches the QC scope item "Network security: ... does the refresh path use HTTPS-only? Are private IP ranges blocked (per V1.57 registry.refresh precedent)?" — the answer is no.  
  **Required**: Either reuse/adapt `validate_cdn_url_static` (or a shared `validate_reference_url`) before the GET, or document+enforce at registration time that refreshable URIs must be https + public. The former is stronger given P3 will expose a CLI.

### Medium severity

- **M-001 — On-disk body.md is not updated by the refresh handler (only DB metadata/hash)**  
  `reference_refresh.rs:199-207` explicitly comments out the file write: "file update is a follow-up concern (P3 wires file I/O through CLI)".  
  `mark_refreshed` updates `content_hash`, `last_refreshed_at`, and `refresh_status='fresh'`, but the canonical `body.md` on disk (written at `register` time via `nexus_home_layout`) remains the original bytes.  
  Consumers that read the body file directly (or via paths derived from `content_path`) will see stale content even when `refreshed=true` and `content_changed=true`.  
  This is within P1 scope per the plan ("core refresh pipeline"), but it materially weakens the observable "Reference Body Refreshable" contract for any code path that trusts the file rather than re-fetching the hash.  
  P3 must close this; if it does not, the capability output `refreshed` becomes misleading.

- **M-002 — Idempotency guard is best-effort within a single daemon process only**  
  `find_stale_sources` (reference_source.rs:475-476) excludes `refresh_status = 'refreshing'`, and the handler calls `mark_refreshing` before the GET.  
  The scheduler (refresh_scheduler.rs:148-180) dispatches sequentially in one tick. However, there is no cross-invocation or cross-process lock. An explicit CLI call (P3) arriving between the SELECT and the UPDATE, or two daemons against the same SQLite file, can both proceed to fetch.  
  Acceptable for the current local single-daemon model, but should be explicitly called out in `reference-knowledge.md` §3 or daemon-runtime.md as a limitation (and potentially hardened with a short advisory lock or row-level OCC on the refresh columns when P3 lands).

### Low severity

- **L-001 — Network-dependent success tests are `#[ignore]`**  
  `reference_refresh.rs:326-379`: `refresh_fetches_real_url_content_changed` and `refresh_not_modified_on_unchanged_content` require network to httpbin.org and are ignored by default.  
  The three failure tests (offline policy, nonexistent source, no-pool) run unconditionally and cover the main admission/error paths.  
  This is acceptable for CI hygiene but means the "≥1 success" vector in normal `cargo test -p nexus-orchestration` is only the policy/error cases. Consider a small hermetic mock (wiremock or a test server) for P3 or a follow-up so the happy path is exercised without external deps.

- **L-002 — sqlx cache hygiene regression during P1 development (process concern)**  
  Merge commit af82ad39 records that P1's `cargo sqlx prepare` run reduced `.sqlx/` from 138 → 1 entries. The restore used the exact protocol documented in P0 (T18) + `daemon-runtime.md`: `DATABASE_URL=... cargo sqlx prepare --workspace -- --tests`.  
  CI would have caught a broken build (`SQLX_OFFLINE=true cargo check --workspace --tests`). The incident itself is not a latent bug in the shipped code, but it confirms the value of the P0 protocol. No action required beyond ensuring future plans touching test queries follow the documented command.

- **L-003 — Migration uses unconstrained TEXT for timestamps and enum-like columns**  
  `202606220003_....sql`: `last_refreshed_at TEXT`, `refresh_policy TEXT NOT NULL DEFAULT 'offline'`, `refresh_status TEXT`.  
  Correct and portable for SQLite. Existing rows safely receive the DEFAULT. No CHECK constraints or application-level enum validation at the DDL layer (enforced in Rust). This is the established pattern in the crate; not a regression.

## Security/Correctness Properties Verified

- Migration preserves existing rows (ALTER ADD COLUMN with DEFAULT; no NOT NULL without default on new non-default columns).
- Refresh policy semantics are correctly implemented in the handler: `offline` → immediate `policy_blocked` (no network); `on_change`/`scheduled` proceed to fetch+compare (staleness for `scheduled` is decided upstream by the scheduler query).
- `find_stale_sources` excludes `refresh_policy='offline'` and `refresh_status='refreshing'` (idempotency guard present).
- `policy_blocked` is a stable output status (not a CapabilityError) and is documented in `reference-knowledge.md` §5 and the handler.
- Sibling capability IDs (`nexus.reference.refresh_policy.get`, `nexus.reference.refresh_status`) are explicitly marked deferred to P3 in the spec; no silent omission.
- `CapabilityRegistry` registration, input/output schemas, and cross-validation test (`registry_has_twenty_six_builtins`) are updated and passing.
- DAO methods (`set_refresh_policy`, `mark_refreshing`, `mark_refreshed`, `mark_refresh_error`, `find_stale_sources`) exist and are used by handler + scheduler.
- Spec §5 admission contracts match code (source must exist → invalid_input; offline → policy_blocked; empty URL → error status; network failure → TransientExternal or error status).

## Verdict Reasoning

One **High** finding (H-001) blocks approval: the refresh fetch path does not inherit the HTTPS-only + private-IP blocking hygiene that was established for `registry.refresh` (and exposed via `validate_cdn_url_static`). Because reference source URIs are user-supplied and the capability will be invocable (directly and via the scheduler), this is a correctness and security gap that must be closed before merge.

All other properties are either satisfied within the declared P1 scope or are explicitly deferred with documentation (body file write, sibling caps, CLI surface). Medium items are follow-ups that P3 must address; they do not independently block the capability + migration + scheduler core.

**Verdict: Request Changes**

## Cross-Plan Concerns

- **P3 must deliver on-disk body write + CLI** — otherwise the observable effect of a successful `nexus.reference.refresh` (with `content_changed=true`) is only a DB hash update. Any reader that materializes content from `content_path` will see stale data. Update `reference-knowledge.md` §2/5 and the capability output contract if the file write remains deferred.
- **Network hygiene should be shared** — once H-001 is fixed, consider extracting the URL validation (https + !blocked_ip) into a small shared helper in `nexus-orchestration` (or a new lightweight crate) so future "fetch from user-controlled URI" capabilities do not reintroduce the gap.
- **sqlx hygiene protocol** (from P0 T18) worked as intended here — the regression was caught in the integration merge commit. Ensure the CI step that runs `SQLX_OFFLINE=true cargo check --workspace --tests` (or equivalent) remains in the V1.58+ workflow.
- **Deferred-features tracker** — DF-44 row should be updated at P3 closeout to reflect that the body file materialization and CLI surface were the P3 increments.
