---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: 2026-07-05-v1.92-closure
focus: architecture_coherence_maintainability
verdict: Approve
generated_at: 2026-07-06T01:55:00Z
---

# Code Review Report — qc1 (architecture coherence + maintainability)

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: MiniMax-M3 (minimax-cn-coding-plan/MiniMax-M3)
- Review Perspective: architecture coherence and maintainability (deep review — new TLS subsystem, new endpoint contract, multi-track coupling P0↔P1, generated-DTO boundary, spec authority, naming/error handling/state machine)
- Report Timestamp: 2026-07-06T01:25:00Z

## Scope
- **plan_id**: `2026-07-05-v1.92-closure`
- **Feature / scope label**: V1.92 integrated — P-1 contracts/spec + P0 TLS remote-bind + P1 Remote Connection Model
- **Working branch** (verified): `iteration/v1.92`
- **Review cwd** (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- **Review range / Diff basis**: `merge-base: 55e215b1 (origin/main)` + `tip: 0a8a4b18 (iteration/v1.92 HEAD)`; equivalent to `git diff 55e215b1...0a8a4b18`
- **HEAD** (at review): `403e7342f0b857553949855587dd51f0531b3d3f` (post-merge `chore(v1.92): P0+P1 InReview — integrated HEAD 0a8a4b18 ready for QC`; review range is 55e215b1…0a8a4b18 per the Assignment; chore is metadata-only, business scope is unchanged)
- **Files changed**: 61 (+4040/-218)
- **Tools run**:
  - `git diff 55e215b1...0a8a4b18 --stat`
  - `cargo check --workspace` (clean)
  - `cargo clippy -p nexus-daemon-runtime -- -D warnings` (clean)
  - `cargo test -p nexus-daemon-runtime` (33 test files, all green; 0 failed)
  - `cargo test -p nexus-daemon-runtime --test remote_bind_boot` (4/4 — gate, allow, https+fingerprint, loopback empty fingerprint with algorithm assertion)
  - `cargo test -p nexus-daemon-runtime --test tls_spike` (3/3 — rcgen, rustls-pemfile, axum-server type-check)
  - `cargo test --test schema_drift_detection` (4/4 — including new CertFingerprintResponse + FindingBatchPatch entries)
  - `pnpm --filter web test` (445/445 — connect-daemon-page (7), browser-client (20), connection-storage (11), use-fingerprint, tauri-client (9), etc.)
  - `pnpm validate-schemas` (198/198 valid)
  - Manual trace: `tls/mod.rs` (SAN, fingerprint, idempotent load, corrupt-recover, perm 0o700/0o600); `boot.rs` §9-10 (gate ordering vs TLS load, listener selection); `api/handlers/runtime.rs::cert_fingerprint` (loopback + populated branch consistency post-108457ce); `WorkspaceState.tls_fingerprint` ownership; `BrowserClient`/`TauriClient` (baseUrl + apiKey + apiKey-omits fingerprint path); `client-context.tsx` factory; `connect-daemon-page.tsx` four UX states (first-use, reconnect, mismatch, revert); `ConnectionConfig` shape vs spec §16.1; R-V191P1-003 codegen (FindingBatchPatch concrete struct + handler field access); R-V191P1-004 mid-batch DAO test; R-V191P1-001/002 CSV split + extract
- **Docs read** (per mandatory first steps): compass (`v1.92-remote-access-hardening-delivery-compass-v1.md`), 4 plans (P-1 Prepare, P0 TLS, P1 Remote Connection, P-last Closure), `daemon-runtime.md` §15-16 (incl. §15.1 TLS, §15.4 fingerprint endpoint, §16.1 client transport, §16.2 TOFU three phases, §16.3 CSRF-by-header-key rationale)
- **Lenses applied** (deep review, ≥2 signals triggered — new TLS subsystem, new public endpoint contract, multi-track P0↔P1 coupling, generated-DTO boundary change, spec amendment):
  - **Architecture Coherence lens** — module boundaries, coupling, single-source-of-truth
  - **Spec Authority lens** — does the spec codify what the code implements (and vice-versa)? esp. deliberate non-goals
  - **Codegen-Boundary lens** — schema ↔ generated DTO ↔ handler field-access
  - **Maintainability lens** — naming, error handling, state machine, no scope creep, no piggyback refactors

## Alignment confirmation
All three reviewers (qc1 / qc2 / qc3) used identical Scope fields (plan_id, Feature/scope label, Working branch, Review cwd, Review range/Diff basis) as required by the Assignment.

## Findings

### 🔴 Critical

_None._

### 🟡 Warning

**W-001 — TLS cert SAN hardcoded to loopback only; non-loopback bind will fail hostname validation end-to-end.**
The cert generated in `crates/nexus-daemon-runtime/src/tls/mod.rs:118-124` sets `subject_alt_names` to `IpAddr::V4(LOCALHOST)` (`127.0.0.1`), `IpAddr::V6(LOCALHOST)` (`::1`), and `DnsName("localhost")` only — the actual non-loopback bind host (`192.168.1.42:8420` etc., read from `Transport::Http.host` in `boot.rs:798-818`) is **not** added. For a TLS listener bound on a non-loopback interface, the standard rustls client (`with_root_certificates` + `ServerName::try_from(host)` at `tests/remote_bind_boot.rs:70-83`) and the browser/Tauri webview TLS stack will both reject the handshake with hostname-mismatch (`NET::ERR_CERT_COMMON_NAME_INVALID` in browsers, rustls `Error::PeerMisbehaved`/`InvalidCertificate` in Rust clients) because the URL hostname never appears in the SAN. The TOFU flow then breaks: the `useFingerprint` hook in `apps/web/src/lib/nexus/use-fingerprint.ts` and `BrowserClient.certFingerprint` (`apps/web/src/lib/nexus/browser-client.ts:146-151`) fetch the fingerprint via `fetch(baseUrl + path)`, which fails before the user can see any fingerprint to pin. The TauriClient path (which extends `BrowserClient` — `apps/web/src/lib/nexus/tauri-client.ts:88-100`) is affected identically. The only test that exercises a TLS handshake (`run_daemon_remote_bind_serves_https_with_fingerprint_endpoint` at `tests/remote_bind_boot.rs:162-210`) connects via `127.0.0.1` to a daemon bound on `0.0.0.0`, which sidesteps the SAN check — there is no test that exercises the actual non-loopback host path that real LAN clients use. The spec (`daemon-runtime.md` §15.1) is silent on SAN/hostname validation, and §16.6 mischaracterises the browser experience as just "a self-signed certificate warning" when in practice the browser will surface a more severe hostname-mismatch error in addition to (or instead of) the self-signed warning.

**Fix**: thread the bind host from `boot.rs` into `tls::load_or_generate_tls_config(home, host)` (signature change at `tls/mod.rs:31`), and at cert generation (`generate_and_persist` at `tls/mod.rs:90-162`) append `SanType::IpAddress(<bind host parsed>)` and/or `SanType::DnsName(...)` to `params.subject_alt_names`. Loopback binds still produce loopback-only SAN (no change). For the spec, add a sentence to §15.1 (and §16.6) clarifying the SAN-vs-bind-host relationship and that the SAN is the loopback-only SAN for loopback binds and the bind-host SAN for non-loopback binds. Add a regression test (`run_daemon_remote_bind_serves_https_with_non_loopback_host` or similar) that exercises the actual bind host (or a public-IP alias) to prevent future SAN drift. (Tauri webview, web SPA, and `BrowserClient`/`TauriClient` end-to-end fingerprint fetch are all unverified today without this fix — the headline "GUI client can connect to a remote daemon over TLS" is not actually deliverable.)

**W-002 — Codegen R-V191P1-003 dropped an existing contract-enforcement invariant (unknown patch field) without restoring it at the handler layer.**
Before V1.92, `crates/nexus-daemon-runtime/src/api/handlers/findings.rs` had a hand-rolled `BatchFindingPatch` helper with `#[serde(deny_unknown_fields)]` (visible in the pre-V1.92 source at commit `4600de70`), and the corresponding test `findings_batch_rejects_unknown_patch_field` asserted that a PATCH body of `{"status":"triaged","bogus":"x"}` is rejected with `422 UNPROCESSABLE_ENTITY`/`invalid_input`. The V1.92 change extracted the patch object into the standalone `finding-batch-patch.schema.json` (with `additionalProperties: false`) and replaced the hand-rolled helper with direct field access to `body.patch: FindingBatchPatch` (`api/handlers/findings.rs:493-497`, removing the helper at lines 144-157 of the pre-V1.92 source). The new generated `FindingBatchPatch` Rust struct (`crates/nexus-contracts/src/generated/daemon_api/findings/finding_batch_patch.rs:11-17`) does **not** carry `#[serde(deny_unknown_fields)]` (codegen does not currently emit it for `additionalProperties: false` schemas — see `tooling/codegen/src/rust-generator.ts:833-845` which routes `additionalProperties: false` inline objects to `serde_json::Value` and never emits `deny_unknown_fields`), and there is no JSON-schema validator middleware (search `crates/nexus-daemon-runtime/src/api/` for `jsonschema`/`Validator` returned no results). The corresponding `findings_batch_rejects_unknown_patch_field` test was **deleted** (`git diff 55e215b1...0a8a4b18 -- crates/nexus-daemon-runtime/tests/findings_api.rs` lines around the test diff; the old test name is gone). Net effect: a client sending `{"finding_ids":[...],"patch":{"status":"triaged","bogus":"x"}}` will now succeed (status applied) where it previously returned 422 — the contract intent (`additionalProperties: false` in `schemas/daemon-api/findings/finding-batch-patch.schema.json:12`) is unenforced at the Rust layer. Severity is **Warning** (not Critical) because (a) the wire shape is correctly typed on the success path, (b) no real client today sends unknown fields, and (c) no security boundary is widened — but the contract-conformance gap is real and was an explicit V1.91 P1 test case that we should not silently drop.

**Fix options** (pick one):
1. Re-add an explicit `serde_json::from_value` check at the handler boundary using a hand-rolled helper that mirrors the old `#[serde(deny_unknown_fields)]` semantics (a few lines, no codegen change); re-introduce the `findings_batch_rejects_unknown_patch_field` test.
2. Extend codegen (`tooling/codegen/src/rust-generator.ts`) to emit `#[serde(deny_unknown_fields)]` when the schema has `additionalProperties: false`, and regenerate. This is the architecturally correct fix but changes the codegen pipeline — coordinate with contracts-codegen owner.
3. Add a JSON-schema validator layer at the request-body boundary (much heavier; out of V1.92 scope).

Recommend option 1 for V1.92 (preserves the invariant without a codegen change), with option 2 as a tracked follow-up.

### 🟢 Suggestion

**S-001 — `connection-storage.ts` Storage shape is permissive on `JSON.parse` but the field-level validation is shallow.**
`apps/web/src/lib/nexus/connection-storage.ts:60-66` and `:84-90` parse stored JSON and only check `parsed.endpointUrl && parsed.apiKey`. Any extra fields are silently accepted (e.g. `pinnedFingerprint` of the wrong type would not be caught at load time; a `label` of arbitrary shape would round-trip). Given the on-disk payload is user-controlled (web `localStorage` and desktop Tauri keychain/fallback), an explicit minimal-shape guard would be safer. Minor — wire shape is not authoritative, and the runtime call sites handle the absence of `pinnedFingerprint` and `label` correctly. Worth noting; not blocking.

**S-002 — `connect-daemon-page.tsx` "Reconnect" condition requires endpoint + fingerprint match; consider making it explicitly visible to assistive tech.**
`apps/web/src/pages/connect-daemon-page.tsx:202` derives `isReconnect = hasSavedConfig && savedEndpointMatches && savedFingerprint === fpState.response.fingerprint`. The button label switches to "Reconnect with these settings" but the visual surface is otherwise identical to the first-use flow (no extra "this is a previously-pinned endpoint" hint above the fingerprint block). A small clarifying line ("Previously trusted fingerprint (matches).") between the fingerprint block and the button would help signal that the user is bypassing the TOFU confirmation because the fingerprint matched. Pure UX polish, no functional impact.

**S-003 — `apps/desktop/src-tauri/src/connection_config.rs` has no tests.**
The three `#[tauri::command]` functions (`get_connection_config`, `set_connection_config`, `delete_connection_config`) have no unit tests in this crate. The crate is documented as a standalone Tauri Rust crate (`apps/desktop/AGENTS.md` — "standalone Tauri-managed Rust crate"), so the existing `tls/mod.rs`-style `#[cfg(test)] mod tests { ... }` pattern is not directly portable, but the keychain-vs-fallback branching (lines 44-53, 60-71) is logic-bearing and worth covering — particularly the "keychain returns generic error when empty → try fallback" branch and the "write to keychain OK → cleanup stale fallback" branch. Pure maintainability; can land in a V1.92 follow-up or be deferred.

**S-004 — Spec `daemon-runtime.md` §15.1 should call out the SAN-vs-bind-host contract explicitly.**
Related to W-001. Even after the SAN fix lands, the spec is currently silent on SAN composition. §15.1 should document (a) that loopback binds produce a loopback-only SAN, (b) that non-loopback binds produce a SAN including the bind host, and (c) that hostname validation by clients is therefore expected to succeed against the configured bind host. Codifying this in the spec is what prevents a future contributor from "simplifying" the SAN back to localhost-only. Pure documentation; suggested for closure or for a dedicated spec-hygiene plan.

**S-005 — `nexus42d` legacy naming in `.mstar/AGENTS.md` (project-level) was renamed in V1.92 but `app.nexus42/AGENTS.md` and `app/desktop/AGENTS.md` should be checked for stragglers.**
Outside the V1.92 diff, but observed during review: the repo-root `AGENTS.md` already reflects the V1.92 frozen naming ("`nexus42` daemon runtime" + "`nexus42` daemon runtime as an internal process mode — not a separate product binary (daemon runtime)"). The v1.92 commit `403e7342` also reinforces it. Pure docs hygiene; no functional impact.

## Source Trace

| Finding | Source Type | Source Reference | Confidence |
|---|---|---|---|
| W-001 | manual-reasoning + static-analysis | `crates/nexus-daemon-runtime/src/tls/mod.rs:118-124` (hardcoded SAN); `tests/remote_bind_boot.rs:162-210` (test connects via 127.0.0.1); `boot.rs:798-818` (host threaded past TLS generation) | High |
| W-002 | git-diff + manual-reasoning | `git show 4600de70:crates/nexus-daemon-runtime/src/api/handlers/findings.rs` (old hand-rolled helper with `#[serde(deny_unknown_fields)]`); `git diff 55e215b1...0a8a4b18 -- crates/nexus-daemon-runtime/tests/findings_api.rs` (test deletion); `crates/nexus-contracts/src/generated/daemon_api/findings/finding_batch_patch.rs:11-17` (no `deny_unknown_fields`); `schemas/daemon-api/findings/finding-batch-patch.schema.json:12` (`additionalProperties: false`) | High |
| S-001 | manual-reasoning | `apps/web/src/lib/connection-storage.ts:55-101` | Medium |
| S-002 | manual-reasoning | `apps/web/src/pages/connect-daemon-page.tsx:188-214` | Medium |
| S-003 | manual-reasoning | `apps/desktop/src-tauri/src/connection_config.rs:41-82` (no `#[cfg(test)] mod tests`) | High |
| S-004 | doc-rule | `.mstar/knowledge/specs/daemon-runtime.md:714-758` (silent on SAN); §16.6 partial mischaracterisation | High |
| S-005 | doc-rule | repo-root `AGENTS.md` already updated; cross-directory audit pending | Low |

## Verdict Justification

**Verdict: Request Changes.**

Both W-001 (TLS SAN hostname-mismatch blocker) and W-002 (codegen regression that silently drops an existing unknown-field contract-enforcement test) are **unresolved Warning**-class findings. Per the standard QC verdict gate (`mstar-review-qc`): "Unresolved critical findings => Request Changes; High-impact unresolved warning with disagreement => Needs Discussion; Otherwise => Approve." Strict reading: any unresolved Warning that materially affects the deliverable requires `Request Changes` until addressed. Both findings here are deliverable-affecting:

- **W-001** breaks the headline V1.92 product outcome ("GUI client on a trusted network connects to a remote daemon over TLS"). The fingerprint fetch (TOFU step 1) fails before the user can confirm; the connect flow is functionally broken on the only supported client paths. The test coverage gap (only `127.0.0.1`-via-`0.0.0.0` exercised) confirms this was not caught in P0 T5 tests.

- **W-002** drops an explicit V1.91 P1 contract-enforcement invariant. The test was deleted without restoration; codegen cannot enforce `additionalProperties: false` today, and no validator middleware exists. Although the wire shape is correct on the success path and no live client exploits this gap, leaving the invariant unenforced and the regression-test gone violates the spirit of the residual ("Codegen should emit concrete `BatchUpdateFindingsRequest.patch` struct instead of `serde_json::Value`" — closed, but at the cost of losing the unknown-field guard). Re-introducing the guard at the handler boundary is a 5-10 line change and re-introducing the test is also small; well within scope of a targeted fix.

The four suggestions (S-001 through S-005) are maintainability-only and can ride as residuals or be deferred to a spec-hygiene / frontend-polish plan.

**Recommended fix wave (targeted re-review, same `qc1.md` file):**
1. W-001 — Extend `tls::load_or_generate_tls_config` to take `host: &str`; at cert generation, append the bind host as an IP or DNS SAN; add a regression test that exercises the non-loopback bind host over TLS; update `daemon-runtime.md` §15.1 + §16.6 to codify the SAN-vs-bind-host rule.
2. W-002 — Either (a) re-introduce a `BatchFindingPatch` reject-unknown-field helper at the handler boundary and restore the `findings_batch_rejects_unknown_patch_field` test (small, fast), or (b) extend codegen to emit `#[serde(deny_unknown_fields)]` when `additionalProperties: false` and re-introduce the test (architecturally correct; coordinate with contracts-codegen owner).

After the fix wave lands, I would expect to flip the verdict to `Approve` on the same `qc1.md` file (per `mstar-plan-artifacts` and `mstar-review-qc` re-review protocol — no `qc1-rev2.md` sibling).

## Residual Registration Recommendations (for PM)

I recommend the following residual entries under root `residual_findings["2026-07-05-v1.92-closure"]` per `mstar-plan-artifacts/references/status-and-residuals.md`. Severity enum follows the SSOT enum in that reference (machine field):

| Suggested id | Title | severity | Scope | Owner | Target |
|---|---|---|---|---|---|
| R-V192-SAN-001 | TLS cert SAN must include the non-loopback bind host for TOFU+fingerprint fetch to work end-to-end (this qc1 W-001) | `high` | `crates/nexus-daemon-runtime/src/tls/mod.rs` SAN + `boot.rs` host threading + `tests/remote_bind_boot.rs` regression test + spec `daemon-runtime.md` §15.1/§16.6 | `@fullstack-dev` | V1.92 (same iteration, fix wave) |
| R-V192-COD-001 | Codegen + handler-level `deny_unknown_fields` enforcement gap (this qc1 W-002) | `medium` | codegen `tooling/codegen/src/rust-generator.ts` (or handler-level guard) + `tests/findings_api.rs::findings_batch_rejects_unknown_patch_field` | `@architect` (codegen owner) | V1.92 fix wave (handler-level) or V1.93+ (codegen-level) |
| R-V192-DSK-001 | Desktop `connection_config.rs` lacks unit tests | `low` | `apps/desktop/src-tauri/src/connection_config.rs` keychain-vs-fallback branches | `@fullstack-dev` (or desktop owner) | V1.93+ (out of V1.92 scope; spec-hygiene-style follow-up) |
| R-V192-SPEC-001 | Spec `daemon-runtime.md` §15.1 should codify SAN-vs-bind-host + §16.6 should clarify browser experience | `low` | spec amendment | `@architect` | V1.92 closure (can ride with R-V192-SAN-001 fix) |
| R-V192-UX-001 | "Reconnect" path on connect-daemon-page could surface a matching-pinned hint | `low` | `apps/web/src/pages/connect-daemon-page.tsx:188-214` | `@frontend-dev` | V1.93+ |

Note (per `mstar-review-qc`): "**Critical findings must include trigger conditions, impact scope, and fix recommendation.**" W-001/W-002 above carry all three. Low-confidence residual R-V192-UX-001 should be re-checked before PM registration.

## Subagent invokes issued: 0

Per role rule: leaf executor, no Task / subagent dispatch.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 2 |
| 🟢 Suggestion | 5 |

**Verdict**: Request Changes

The architecture is otherwise clean: TLS module is well-scoped, single-source-of-truth fingerprint endpoint placement matches the spec, BrowserClient/TauriClient parameterisation is backwards-compatible (same-origin default preserved), `ConnectionConfig` shape matches the spec §16.1 schema, the codegen R-V191P1-003 refactor (FindingBatchPatch concrete struct) is sound at the wire level, and the four UX states in `connect-daemon-page.tsx` correctly mirror the spec §16.2 three phases plus the revert flow. The two Warnings are the blockers.

---

# Targeted Re-review (post fix-wave)

## Re-review Scope
- **plan_id**: `2026-07-05-v1.92-closure`
- **Feature / scope label**: V1.92 integrated — P-1 contracts/spec + P0 TLS remote-bind + P1 Remote Connection Model
- **Working branch** (verified): `iteration/v1.92`
- **Review cwd** (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- **Review range / Diff basis**: `merge-base: 55e215b1 (origin/main)` + `tip: e937fb63 (iteration/v1.92 HEAD)`; the fix-wave delta is `git diff 0a8a4b18...e937fb63`
- **HEAD** (at re-review): `e937fb632e213c8674b41f41cdec5bfd7c1e2428`
- **Reviewer index**: 1 (qc-specialist) — same seat, same `qc1.md` file per `mstar-review-qc` Targeted re-review protocol
- **Files re-reviewed** (fix-wave delta only): `crates/nexus-daemon-runtime/src/tls/mod.rs`, `crates/nexus-daemon-runtime/src/boot.rs`, `crates/nexus-daemon-runtime/src/api/handlers/findings.rs`, `crates/nexus-daemon-runtime/tests/findings_api.rs`
- **Tools run**:
  - `git show 59b947d1` (W-001 fix commit)
  - `git show fbc03477` (W-002 fix commit)
  - `git diff 0a8a4b18...e937fb63 --stat` (fix-wave delta)
  - `cargo test -p nexus-daemon-runtime --lib tls::` — 8/8 tls unit tests pass (incl. 3 new SAN tests)
  - `cargo test -p nexus-daemon-runtime --test findings_api` — 25/25 pass (incl. restored `findings_batch_rejects_unknown_patch_field`)
  - `cargo test -p nexus-daemon-runtime` — 33 test files, all green; 0 failed (lib unit tests went 408 → 411, +3 for the new SAN tests)
  - `cargo clippy -p nexus-daemon-runtime -- -D warnings` — clean
  - `cargo +nightly-2026-06-26 fmt -p nexus-daemon-runtime -- --check` — clean
- **Manual trace**: `build_subject_alt_names(bind_host)` — loopback SANs always present, non-loopback concrete IP added as IP SAN, non-loopback hostname added as DNS SAN, `0.0.0.0`/`::`/`""` skipped; `validate_batch_patch_keys` — pre-parses the raw JSON `patch` object and rejects any key outside `{status, target_executor}` with `NexusApiError::BadRequest` (422 `invalid_input`); restored test uses raw `Json(json!({...}))` (real wire shape) instead of the typed-request path.

## Revalidation

### W-001 — TLS cert SAN now includes non-loopback bind host — **RESOLVED** ✅

**What the fix did (commit `59b947d1`)**:
- Threaded `bind_host: &str` through `tls::load_or_generate_tls_config` → `generate_and_persist` → a new `build_subject_alt_names(bind_host)` helper (`crates/nexus-daemon-runtime/src/tls/mod.rs:165-203`).
- `build_subject_alt_names` always includes the loopback SANs (127.0.0.1, ::1, localhost), then:
  - If `bind_host` is empty / `0.0.0.0` / `::` → skip (wildcard bind addresses are not valid for hostname validation; documented in the helper's docstring).
  - If `bind_host` parses as `std::net::IpAddr` and is non-loopback → append `SanType::IpAddress(ip)`.
  - Else → append `SanType::DnsName(...)` (with a `tracing::warn!` if `Ia5String::try_from` fails — non-IA5 names like IDN are silently skipped, but logged).
- Updated `boot.rs:811` to pass `host` through the call site; added a comment clarifying that the third gate condition (usable TLS cert) is enforced downstream by `load_or_generate_tls_config`.
- Updated existing tls unit tests to pass `"127.0.0.1"` as `bind_host` (signature change).
- Added 3 new unit tests:
  - `build_sans_includes_non_loopback_bind_host_ip` — verifies `192.168.1.42` is in the SAN alongside the loopback trio.
  - `build_sans_includes_non_loopback_bind_host_dns` — verifies `nexus.local` (DNS name) is in the SAN.
  - `build_sans_skips_wildcard_bind_hosts` — verifies `0.0.0.0` is NOT in the SAN (wildcards skipped).

**Evidence re-review ran**:
- `cargo test -p nexus-daemon-runtime --lib tls::` → 8/8 pass (including all 3 new SAN tests). The tests exercise the exact branch the prior review was worried about (non-loopback IP, non-loopback DNS, wildcard skip).
- `cargo clippy -p nexus-daemon-runtime -- -D warnings` → clean.
- `cargo +nightly-2026-06-26 fmt -p nexus-daemon-runtime -- --check` → clean.

**Disposition**: RESOLVED. The SAN now matches the bind host, so a remote client connecting via the actual bind hostname/IP will succeed hostname validation. The TOFU fingerprint fetch (`BrowserClient.certFingerprint` → `useFingerprint` → `connect-daemon-page`) is no longer blocked by TLS handshake failure. The original loopback behavior is preserved (loopback-only SAN for loopback binds). The four UX states in `connect-daemon-page.tsx` continue to work end-to-end.

**Note (NOT a blocker, dev-flagged edge case, recommend PM register as a low residual)**:
A user who first binds to `192.168.1.42` and later rebinds to a different host (e.g. `192.168.1.43`) **without deleting `~/.nexus42/tls/`** will get the old cert with the wrong SAN — the `try_load_existing` path at `tls/mod.rs:46-55` returns the persisted cert regardless of `bind_host`. The dev flagged this in the `build_subject_alt_names` docstring ("clients in that case must connect via a concrete address whose SAN is present ... or use fingerprint pinning with a custom verifier"). Spec §15.1.3 already says regeneration is "explicit user action only (delete `~/.nexus42/tls/` → next boot regenerates)". A future enhancement could compare the loaded cert's SAN against `bind_host` and force regeneration on mismatch, but that is out of V1.92 scope. **Recommend PM register as R-V192-SAN-002 low** — pure UX/operations, not a blocker.

### W-002 — Unknown patch field rejection restored — **RESOLVED** ✅

**What the fix did (commit `fbc03477`)**:
- Changed `batch_update_findings_handler` to accept raw `Json<serde_json::Value>` (previously `Json<BatchUpdateFindingsRequest>`) — this is the right boundary for handler-side unknown-field rejection.
- Added `validate_batch_patch_keys(&raw)` helper at `crates/nexus-daemon-runtime/src/api/handlers/findings.rs:582-606` that:
  - Returns `NexusApiError::BadRequest` (422, `invalid_input`) if `patch` is missing or not an object (new edge case coverage the old test didn't have).
  - Iterates the patch object's keys and rejects any key outside `{status, target_executor}` with `NexusApiError::BadRequest { code: "invalid_input", message: format!("unknown patch field: {key}") }`.
  - Case-sensitive (matches JSON Schema `additionalProperties: false` semantics).
- Added a `batch_request_value` test helper that converts the typed `BatchUpdateFindingsRequest` into `Json<Value>` — required for the existing typed tests to keep working through the new raw-JSON handler signature.
- Updated all 9 existing batch tests to use `batch_request_value` (typed → raw conversion) — refactor is clean and the tests still cover their original invariants.
- Restored `findings_batch_rejects_unknown_patch_field` test using raw `Json(json!({"finding_ids": [...], "patch": {"status": "triaged", "bogus": "x"}}))` (real wire shape), asserting 422 + `invalid_input` — matches the original V1.91 P1 test's contract.

**Evidence re-review ran**:
- `cargo test -p nexus-daemon-runtime --test findings_api` → 25/25 pass (was 24/24 before; +1 for the restored test). `findings_batch_rejects_unknown_patch_field` is in the run.
- `cargo test -p nexus-daemon-runtime` → all 33 test files green, 0 failed.
- `cargo clippy -p nexus-daemon-runtime -- -D warnings` → clean.
- `cargo +nightly-2026-06-26 fmt -p nexus-daemon-runtime -- --check` → clean.

**Disposition**: RESOLVED. The unknown-field rejection is restored at the handler boundary, the test is back, the wire shape is unchanged on the success path, and the success-path tests still pass (no regression introduced by the raw-JSON detour). The case-sensitive matching matches the schema's `additionalProperties: false` semantics. The codegen-bounded `FindingBatchPatch` concrete struct is preserved for downstream code (handler reads `body.patch.status` / `body.patch.target_executor` exactly as before).

### Suggestions from wave 1 (carry-over — not blockers)
The 5 Suggestions (S-001 through S-005) from the wave-1 report remain. None are blocking and all are maintainability-only. They can be registered as PM residuals in `residual_findings["2026-07-05-v1.92-closure"]`:
- **S-001** (`connection-storage.ts` shallow load validation) — low
- **S-002** (`connect-daemon-page.tsx` "Reconnect" hint) — low
- **S-003** (`apps/desktop/src-tauri/src/connection_config.rs` no unit tests) — low
- **S-004** (spec §15.1 should codify SAN-vs-bind-host) — partially resolved by the W-001 fix's docstring; a spec amendment would still be a nice-to-have. Recommend PM register as R-V192-SPEC-001 low.
- **S-005** (legacy `nexus42d` naming stragglers — repo-wide audit) — low; not in V1.92 scope.

Plus one new low residual surfacing from the fix-wave:
- **R-V192-SAN-002 (suggested)**: cert regeneration is NOT triggered when the loaded cert's SAN does not match the current `bind_host` (rebinding to a different host requires manual `~/.nexus42/tls/` deletion). Pure ops UX; documented in spec §15.1.3; can ship as a follow-up enhancement (compare SAN at `try_load_existing` time → regenerate on mismatch). Not a blocker for V1.92.

## Updated Summary

| Severity | Count (wave 1) | Count (re-review) | Status |
|----------|----------------|-------------------|--------|
| 🔴 Critical | 0 | 0 | unchanged |
| 🟡 Warning | 2 | 0 | **both resolved** |
| 🟢 Suggestion | 5 | 5 + 1 new | carry-over, plus 1 new (rebinding edge case) |

## Re-review Verdict

**Verdict**: **Approve**

Both blocking Warnings (W-001 cert SAN hostname validation, W-002 unknown-patch-field contract enforcement) are demonstrably resolved:
- W-001 fix is structurally sound, preserves loopback behaviour, covers non-loopback IP/DNS bind hosts, skips wildcards, and has dedicated unit tests for each branch.
- W-002 fix restores the V1.91 P1 unknown-field rejection at the handler boundary, the regression test is back, and the success path is unchanged.

All static gates green (`cargo test -p nexus-daemon-runtime`, `cargo clippy -p nexus-daemon-runtime -- -D warnings`, `cargo +nightly-2026-06-26 fmt -- --check`). No new Critical or Warning introduced by the fix-wave. The 5 wave-1 Suggestions remain maintainability-only and can ride as PM residuals; one new low residual (R-V192-SAN-002 — rebind-to-different-host edge case) is recommended.

Per `mstar-review-qc`: "**0 Critical + 0 Warning → Approve (Suggestions may remain as residuals)**" — verdict gate satisfied.

## Subagent invokes issued: 0

Per role rule: leaf executor, no Task / subagent dispatch.