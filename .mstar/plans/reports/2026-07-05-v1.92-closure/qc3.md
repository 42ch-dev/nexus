---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: 2026-07-05-v1.92-closure
focus: performance_reliability_transport
verdict: Approve
generated_at: 2026-07-06
---

# Code Review Report — qc3 (performance + reliability + transport)

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Review Perspective: performance, reliability, and transport security (deep review — new TLS subsystem, listener change, crypto provider install, cert lifecycle, graceful shutdown, fingerprint endpoint reliability, P1 client connection model reliability)
- Report Timestamp: 2026-07-06

## Scope
- **plan_id**: `2026-07-05-v1.92-closure`
- **Feature / scope label**: V1.92 integrated — P-1 contracts/spec + P0 TLS remote-bind + P1 Remote Connection Model
- **Working branch** (verified): `iteration/v1.92`
- **Review cwd** (verified): `/Users/bibi/workspace/organizations/42ch/nexus`
- **Review range / Diff basis**: `merge-base: 55e215b1 (origin/main)` → `tip: e937fb63 (post-fix-wave code state; qc1 re-review at f91b29d7 only adds report file)`; equivalent to `git diff 55e215b1...e937fb63`
- **HEAD verified**: `f91b29d7 qc(v1.92): qc1 targeted re-review after fix-wave` (parent of code tip `e937fb63 Merge qc1 fix-wave into iteration/v1.92`)
- **Files changed** (code diff `55e215b1...e937fb63`): 63 (+4457/-221) — includes the qc1/qc2 fix-wave (`59b947d1` SAN fix + `fbc03477` unknown-field guard restore)
- **Tools run**:
  - `git rev-parse --show-toplevel` + `git branch --show-current` + `git log -1 --oneline` (branch + HEAD verified)
  - `git diff 55e215b1...e937fb63 --stat`
  - `git log 55e215b1..e937fb63 --oneline` (fix-wave commit chain traced)
  - `git show 59b947d1 --stat` + `git show fbc03477 --stat` (fix-wave scope)
  - `cargo test -p nexus-daemon-runtime --test tls_spike --test remote_bind_boot` — 7/7 passed (rcgen + rustls-pemfile + axum_server type-check; gate rejects/allows; TLS + fingerprint over TLS to a `0.0.0.0`-bound daemon; loopback empty-fingerprint with `algorithm:"sha256"`)
  - `cargo test -p nexus-daemon-runtime --lib tls::` — 8/8 passed (generate+reuse, corrupt regen, perms 0o700/0o600, fingerprint format, `system_time_to_rfc3339`, non-loopback IP SAN, non-loopback DNS SAN, wildcard-skip)
  - `cargo test -p nexus-daemon-runtime --test findings_api` — 25/25 passed (incl. restored `findings_batch_rejects_unknown_patch_field`)
  - `cargo clippy -p nexus-daemon-runtime -- -D warnings` — clean
  - `cargo +nightly-2026-06-26 fmt --all -- --check` — clean
  - `pnpm --filter web build` — clean; 2502 modules built
  - Manual trace: `tls/mod.rs` (SAN builder, load/regen, corrupt recovery, perms, fingerprint DER-SHA256, cert reuse); `boot.rs` §1/§9/§10 (crypto-provider install-once at 169–173; gate call vs TLS load ordering at 798–823; `bind_rustls` graceful-shutdown at 851–863 vs plain `axum::serve` at 871–878; listener parity); `api/handlers/runtime.rs::cert_fingerprint` (loopback empty-string branch + populated branch); `WorkspaceState.tls_fingerprint` ownership (`Arc<Option<CertFingerprintResponse>>`); `api/mod.rs::create_router` (runtime_routes unguarded vs protected_routes middleware layering; CORS + Origin gate applied at outer router — identical for TLS and plain-HTTP paths); `browser-client.ts::certFingerprint` + `use-fingerprint.ts` (throwaway client, no key sent); `connect-daemon-page.tsx` (4 UX states; mismatch is blocking).
- **Docs read** (per mandatory first steps): compass, 4 plans (P-1 Prepare, P0 TLS remote-bind, P1 Remote Connection Model, P-last Closure), `daemon-runtime.md` §15.1–§15.4 (Transport Security) + §16.1–§16.6 (Client Connection Model), `tls_spike.rs`.
- **Lenses applied** (deep review — new TLS subsystem, listener change): Transport Soundness · Cert Lifecycle Reliability · Listener Parity · Startup + Shutdown Reliability · Performance / Blocking.

## Alignment confirmation
All three reviewers share identical Scope fields (plan_id, Feature/scope label, Working branch, Review cwd, Review range / Diff basis) per PM Assignment. qc1 (initial) + qc2 first-wave cited pre-fix `tip: 0a8a4b18`; the qc1 targeted re-review (at f91b29d7) verifies the fix-wave at post-fix code tip `e937fb63`. This qc3 fresh review is at the same post-fix code tip `e937fb63` (HEAD `f91b29d7` differs only by the qc1 report file addition).

## Findings

### 🔴 Critical

_None._

### 🟡 Warning

_None._

### 🟢 Suggestion

**S-001 — Re-bind-to-different-host edge case: persisted cert SAN is not invalidated on host change.**

`tls::load_or_generate_tls_config(home, bind_host)` (`crates/nexus-daemon-runtime/src/tls/mod.rs:36-59`) always calls `try_load_existing` first and, if the persisted `cert.pem` + `key.pem` parse into a usable `RustlsConfig`, returns them unconditionally — **without** verifying that the persisted cert's SAN list still covers the new `bind_host` argument. The fix-wave (`59b947d1`) correctly threads `bind_host` into `generate_and_persist` on the fresh-generation path, but the load path is host-oblivious.

Concrete scenario:

1. Operator first boots the daemon with `NEXUS_DAEMON_REMOTE_BIND=1` and `--host 192.168.1.42`. Cert is generated with SANs `{127.0.0.1, ::1, localhost, 192.168.1.42}` and persisted.
2. Operator later moves the host to a new LAN and reboots with `--host 10.0.0.7`. The persisted cert is reloaded with the **old** SAN list — rustls hostname validation on a remote client connecting to `10.0.0.7` will fail (`Error::PeerMisbehaved` / `InvalidCertificate`).

Recovery is manual: operator deletes `~/.nexus42/tls/cert.pem` + `key.pem`, next boot regenerates. This is consistent with §15.1 ("regeneration is explicit user action only"), but the failure mode (silent post-move handshake failure with a mismatched-hostname error) is not obvious without spec familiarity.

**Severity: `low`.** Rationale:

- Local-first single-machine operators (the overwhelming majority) never trip this.
- The pre-release policy (repo root AGENTS.md § Pre-release Development) explicitly allows "local persistence may be wiped rather than migrated" — deleting `~/.nexus42/tls/` is a legitimate remediation.
- The daemon-side symptom (rustls handshake fails) is externally visible and does not corrupt anything.
- No security regression: the pinned fingerprint remains stable across the daemon reboot, so the TOFU-mismatch UX at the client end is arguably the correct outcome (a fingerprint that no longer matches over the new hostname is behaviourally equivalent to a rotated cert).

Options for follow-up (register as residual; do not block V1.92):

1. **Detect + regenerate** in `load_or_generate_tls_config`: after successful load, parse the DER for SANs and compare with the new `bind_host`; on mismatch, `warn!` and fall through to `generate_and_persist` (overwriting the persisted PEMs). Trade-off: fingerprint changes silently on host move — clients see a TOFU re-pin prompt. Probably the right behaviour, but needs spec §15.1 clarification.
2. **Detect + refuse to boot** with a clear error pointing at `~/.nexus42/tls/` deletion as the remediation. Preserves the fingerprint-stability guarantee but adds friction.
3. **Document only** in §15.1 that host changes require deleting `~/.nexus42/tls/`. Cheapest; matches current implementation exactly; leaves the operator to debug the handshake error the first time they hit it.

Recommend option 1 for V1.93+ (see Residuals below).

**S-002 — TLS graceful shutdown ignores the `shutdown_grace_ms` configuration parameter.**

`boot.rs:695-698` passes `config.shutdown_grace_ms` to `StatigLifecycle::new_with_subsystems`, and the plain-HTTP path uses `axum::serve(...).with_graceful_shutdown(...)` (line 871–878) which drains connections until the notify fires. The TLS path (line 862) calls `handle.graceful_shutdown(None)` — passing `None` means `axum_server` waits **indefinitely** for in-flight requests to complete. This is *safer* than the plain-HTTP path (won't drop responses mid-stream), but it does not respect the `shutdown_grace_ms` ceiling the operator configured.

In practice unlikely to bite: the daemon has no long-lived streaming endpoints in scope for V1.92, and the lifecycle HSM's own `shutdown_grace_ms` budget will terminate the process independently. But if a large batch findings PATCH or a slow schedule query is in flight when SIGTERM arrives, the TLS path can block past the configured budget.

**Severity: `low`; reliability polish.** Fix is one line: `handle.graceful_shutdown(Some(Duration::from_millis(config.shutdown_grace_ms)))`. Register as residual; not blocking.

**S-003 — Fingerprint fetch error surface could distinguish "self-signed cert not yet trusted" from "wrong URL / port".**

`BrowserClient::certFingerprint()` (`apps/web/src/lib/nexus/browser-client.ts:146-151`) issues a `fetch` against the entered endpoint URL. If the daemon isn't reachable, or (for the raw-browser-tab flow — a documented non-goal per §16.6) the browser blocks the self-signed cert, `fetch` throws and `request()` (`browser-client.ts:562-573`) surfaces the generic `transport_unreachable` message. A network-layer distinction between "browser rejected self-signed cert" and "wrong URL / port" is not reliably available through the `fetch` API in the browser, so the current message is defensible — but the Tauri-shell path (§16.6 primary supported client) can and does bypass the browser trust store, so the fingerprint fetch should succeed there and the ambiguity is confined to the browser fallback path.

**Severity: `low`; UX polish, not reliability.** Consider adding a UI hint that the browser-tab fallback may be blocked by the self-signed cert and that the Nexus desktop app is the supported remote-connection surface. Register as residual; not blocking.

## Verdict Details

- **Verdict**: **Approve**
- Rationale:
  - **Transport soundness — clean.** The rustls default crypto provider (`aws_lc_rs`) is installed exactly once at boot (`boot.rs:169-173`, before any `RustlsConfig` is constructed) — this addresses the exact class of runtime panics that a naïve `install_default()` call would introduce. Cert files use 0o600, tls dir 0o700, key material persisted only to `~/.nexus42/tls/` (see `nexus-home-layout::tls_dir` at `crates/nexus-home-layout/src/lib.rs:146-166`). Fingerprint is DER-SHA256 formatted as `SHA256:<colon-hex>` matching §15.4.
  - **Cert lifecycle — sound after the fix-wave.** `59b947d1` correctly threads `bind_host` into `build_subject_alt_names`, which always includes loopback SANs, adds the non-loopback bind host as IP-SAN or DNS-SAN, and skips wildcards (`0.0.0.0` / `::`). Corrupt-file recovery works (test `regenerates_when_certificate_files_are_corrupt`). Reuse-when-valid works (test `reuses_existing_material_when_valid`).
  - **Listener parity — verified.** `api/mod.rs::create_router` composes runtime_routes (unguarded — health, gate, cert-fingerprint) and protected_routes (behind `require_api_key`) into a single router; CORS + Origin gate are applied at the **outer** router. Both `bind_rustls(...).serve(router.into_make_service())` (line 862) and `axum::serve(listener, router)` (line 871) hand off the same fully-layered router — no middleware drift.
  - **Startup ordering — correct.** Crypto provider installed → home layout initialised → tls_config loaded/generated *before* fingerprint is stashed into `WorkspaceState` (`boot.rs:798-823`); remote-bind gate is checked before both, and if the gate rejects, TLS material is never touched. `remote_bind_boot::rejects_non_loopback_without_flag` verifies the reject-early behaviour.
  - **Shutdown reliability — acceptable, with S-002 polish deferred.** TLS `handle.graceful_shutdown(None)` is *safer* than a bounded shutdown (never drops in-flight responses); the `shutdown_grace_ms` mismatch is a residual reliability item, not a regression.
  - **Performance / blocking — no concerns.** Cert generation is one-shot at boot; `Sha256::digest(cert.der())` is negligible; `WorkspaceState.tls_fingerprint` is `Arc<Option<...>>` — cheap clone per handler invocation; no I/O per fingerprint GET (returns the cached struct or the loopback empty-string sentinel). rcgen and rustls-pemfile parse costs are bounded and only paid on boot or regen.
  - **Fingerprint endpoint semantics — spec-correct.** Loopback bind returns `{ algorithm: "sha256", value: "" }` (empty string sentinel, not `null`) per the fix in `108457ce`; TLS bind returns the populated `SHA256:<colon-hex>`. Handler is unguarded (in runtime_routes), consistent with §16.4 ("client fetches fingerprint before it has an API key").
  - **P1 client model reliability — clean.** `BrowserClient` normalises endpoints, isolates the fingerprint probe with a throwaway client (no API key sent to a not-yet-trusted daemon), pins the fingerprint at first pair, and rejects mismatches. `useFingerprint` reacts to endpoint changes with debounced re-probe and clear loading / error / mismatch states. UX blocks pairing on mismatch (`connect-daemon-page.tsx`).
  - **Test coverage — appropriate.** New `tls_spike.rs` covers rcgen + pemfile + axum_server bindings; `remote_bind_boot.rs` covers gate + TLS + fingerprint over TLS + loopback empty-fingerprint; TLS lib tests cover regen, perms, SAN builder branches; findings_api regression restored.
  - **Fix-wave — surgical.** `59b947d1` +49/-3 loc (SAN builder + threading + 3 tests); `fbc03477` +14 loc (unknown-field guard restored, one regression test). No collateral changes; both scoped to the qc1/qc2 findings.

- Blocking findings: none.

- **Follow-ups suggested for future iterations** (residuals; PM to register):
  - **R-V192-P0-SAN-invalidation** (from S-001; severity `low`, category `reliability`): re-bind to a different non-loopback host reloads the old-SAN cert. Recommend option 1 (detect + regenerate) for V1.93+.
  - **R-V192-P0-shutdown-grace** (from S-002; severity `low`, category `reliability`): TLS `handle.graceful_shutdown(None)` ignores `config.shutdown_grace_ms`. One-line fix for V1.93+.
  - **R-V192-P1-fingerprint-error-hint** (from S-003; severity `low`, category `ux`): fingerprint fetch error message doesn't distinguish self-signed-cert rejection from unreachable-daemon. Copy-only tweak for V1.93+.

## Notes for PM / QA

- All three tri-review reports (qc1 initial, qc1 targeted re-review after fix-wave, qc2 first-wave, this qc3 fresh) collectively cover architecture-coherence, security-correctness, and performance-reliability-transport perspectives at the post-fix-wave code state.
- qc1 initial (777d996b) flagged the SAN gap as W-001; qc2 (ca70279e) flagged the unknown-field regression as W-002; both were addressed by the fix-wave (59b947d1, fbc03477) and re-verified by qc1's targeted re-review (f91b29d7).
- No blocking findings from qc3. All three residuals proposed here are `low` severity and appropriate for V1.93+.
- QA hand-off: post-merge smoke should exercise (a) loopback boot + fingerprint GET returns empty-string with `algorithm:"sha256"`, (b) `NEXUS_DAEMON_REMOTE_BIND=1` + `--host 0.0.0.0` boots and TLS fingerprint GET returns `SHA256:<colon-hex>`, (c) protected route without `x-api-key` returns 401 over both listener paths, (d) findings batch PATCH with unknown patch field returns 400.
