---
iteration_id: V1.93
start_date: 2026-07-06
end_date: 2026-07-06
status: completed
iteration_base_branch: main
target_branch: main
plans:
  - 2026-07-06-v1.93-prepare-spec-codification
  - 2026-07-06-v1.93-tls-robustness
  - 2026-07-06-v1.93-remote-connection-polish
  - 2026-07-06-v1.93-closure
---

# V1.93 — Remote-Access Polish & Residual Convergence — Delivery Compass v1

**Status**: completed (Phase 3 §3.1–§3.4 done; PR delivery Phase 4 next). `wire_contracts_changed: false` (polish iteration — no new DTOs, no schema changes). QC 3/3 Approve; QA Pass. PR to `main` after Phase 3.

## 0. Context

V1.92 (Remote-Access Hardening) shipped TLS for remote-bind + the Remote Client Connection Model (PR #120). QC tri-review approved with a fix-wave, and 8 low/nit residuals were registered against V1.93 — the natural convergence follow-on. The V1.92 retrospective explicitly endorses these as V1.93 work, calling R-V192P0-002 (TLS graceful-shutdown grace cap) and R-V192P1-003 (desktop connection-config tests) "cheap V1.93 sweeps" and R-V192P0-001 (cert SAN invalidation on re-bind) a named candidate.

This is a **stabilization / residual-convergence iteration** — the same pattern the repo has followed after every major feature/security iteration (V1.30, V1.44, V1.80, V1.92's own companion sweep). It completes the V1.92 remote-access story coherently before opening any new feature axis. **No new product surface.** User-visible surface delta is near-zero: V1.92 flows and visuals behave identically; changes are limited to robustness fixes (TLS cert regeneration on re-bind, graceful-shutdown grace cap), shape-validation guards on load, error-copy guidance (honest browser limitation + desktop hint), one optional low-surface reassurance hint on an existing reconnect happy-path state, and added test coverage. `wire_contracts_changed: false`.

## 0.1 Terminology conventions

Inherited from V1.92 (see `v1.92/delivery-compass.md` §0.1). Key terms reused unchanged: **Daemon**, **Daemon API** (NOT "Local API" in new prose — R-V192HYG-001 sweeps residual stragglers), **Remote bind**, **Certificate fingerprint**, **TOFU**, **Trust anchor**, **Connection config**, **Setup screen**. No new V1.93 vocabulary met the CONCEPTS.md bar.

## 1. Locked Decisions (grill-me output)

| Decision | Resolution |
|---|---|
| Iteration direction | **A (remote-access polish & residual convergence).** Sweep all 8 V1.93-targeted V1.92 residuals. No new product surface. |
| Headline 1 — TLS robustness | **Backend TLS hardening.** (a) R-V192P0-001: detect SAN-vs-bind-host mismatch at cert load and regenerate the cert (today the cert is reused from `~/.nexus42/tls/` without re-validating SAN against the current `bind_host`). (b) R-V192P0-002: thread `shutdown_grace_ms` into the TLS `graceful_shutdown()` call (today the TLS path passes `None`, ignoring the grace cap the plain-HTTP path honours). (c) R-V192SPEC-001: tighten the SAN generation rule + rebinding behaviour in `specs/daemon-runtime.md` §15.1 (the rule is already codified normatively by the V1.92 fix-wave; P-1 closes the remaining precision gaps — `bind_host` resolution semantics, IPv6 literal example, hostname IA5 fallback, full "always covered" address catalogue, and §16.2 cross-references). |
| Headline 2 — Remote-connection UX polish + desktop test coverage | **Frontend polish on the V1.92 connection surface.** (a) R-V192P1-001: add minimal shape validation to `connection-storage.ts` load so a corrupt `localStorage` entry fails gracefully. (b) R-V192P1-002: when fingerprint fetch fails, surface honest guidance that the Nexus desktop app is the supported remote-connection surface for self-signed certs/TOFU (browser cannot distinguish cert-rejection from unreachable; copy does not promise browser-remote will work for untrusted certs). (c) R-V192P1-003: add unit tests for the desktop `connection_config.rs` (keychain vs app-data fallback). (d) R-V192UX-001: low-surface "pinned fingerprint matches" reassurance hint on reconnect to a same-endpoint pinned connection (on already-detected happy-path state; may be omitted if copy review finds it noisy). |
| Headline 3 — Naming hygiene | **Cross-directory sweep** (R-V192HYG-001): residual "Local API" / legacy naming stragglers in non-historical prose. |
| Plan structure | **Option 1 (V1.92-style dual-track).** P-1 Prepare (spec precision tightening, locks the SAN/rebinding rule ahead of P0) → **P0 TLS backend** (`@fullstack-dev`, `crates/`) ‖ **P1 Frontend+desktop polish** (`@frontend-dev`, `apps/`) on parallel worktrees → **P-last closure** (naming sweep + QC + QA + compound + PR). P0 ‖ P1 have disjoint file sets → worktree isolation is safe. Desktop tests (R-V192P1-003) fold into P1 (same `@frontend-dev` owner). |
| Branch policy | `iteration_base_branch=main` (HEAD post-V1.92, PR #120; integration branch `iteration/v1.92` retired); `spec_integration_branch=iteration/v1.93`; `target_branch=main`. Matches documented project convention (`.mstar/AGENTS.md` two-tier branch model) and unbroken V1.39–V1.92 history. |
| Contract impact | **NONE.** `wire_contracts_changed: false`. No new endpoints, no new DTOs, no schema changes, no `schema_version` bump. `@42ch/nexus-contracts` stays at `0.20.0`. This is a polish iteration by design. |
| Residual posture | All 8 V1.93-targeted residuals close to `lifecycle: resolved` (with `resolution.commit` + `resolution.plan_id`). R-V191P1-005 (FindingsPage memoisation, nit) stays deferred to "when list virtualisation lands" — unchanged. |

## 2. Scope

This iteration locks four delivery spec points plus closure:

- **SP-1: Prepare — Spec Precision Tightening (P-1).** Tighten the TLS SAN generation rule + the detect-and-regenerate-on-bind-host-mismatch behaviour in `specs/daemon-runtime.md` §15.1. The rule itself is already codified normatively by the V1.92 fix-wave (loopback always + non-loopback `bind_host` + skip wildcard; regenerate on load when the persisted cert's SAN does not cover the current `bind_host`). P-1 closes the remaining precision gaps: `bind_host` is the literal `--host` value (no DNS resolution), a non-loopback IPv6 literal example, the hostname IA5 fallback, the full "always covered" address catalogue, and §16.2 Phase 3 cross-references (so the security story is traceable in one reading pass). Architect-owned spec amendment; writing-specialist terminology pass. No DESIGN.md change (no new UI surface — P1 polish is on existing surfaces).
- **SP-2: TLS Robustness — P0 (security-transport headline).** R-V192P0-001: at cert load, validate the persisted cert's SAN against the current `bind_host`; regenerate (and re-pin via the fingerprint endpoint value) when mismatched — today `try_load_existing` reuses blindly. R-V192P0-002: thread the configured `shutdown_grace_ms` into the TLS listener's `graceful_shutdown()` (today passes `None`). Regression tests: SAN mismatch triggers regeneration; SAN match reuses; TLS graceful shutdown respects the grace cap. Loopback plain-HTTP path unchanged.
- **SP-3: Remote-Connection Polish + Desktop Tests — P1 (product-access headline).** R-V192P1-001: shape-validate the parsed `connection-storage.ts` config on load (fail gracefully on corrupt `localStorage`). R-V192P1-002: improve fingerprint-fetch error copy — when fetch fails, surface guidance that the Nexus desktop app is the supported remote-connection surface for self-signed certs / TOFU (browser cannot distinguish cert-rejection from unreachable; copy is honest about this limitation). R-V192P1-003: unit tests for `apps/desktop/src-tauri/src/connection_config.rs` (keychain vs app-data fallback paths). R-V192UX-001: low-surface "pinned fingerprint matches" reassurance hint on reconnect to a same-endpoint pinned connection (on an already-detected happy-path state; may be omitted during copy review if noisy).
- **SP-4: Closure.** R-V192HYG-001 (naming sweep, `@writing-specialist`) + QC tri-review (qc2 security lens on the P0 TLS changes) + QA + compound + Profile B compaction + PR to `main`.

## 2.1 Architecture Hierarchy and Ownership

- **P0 (TLS robustness) lives in `crates/nexus-daemon-runtime/src/`**: `tls/mod.rs` (`try_load_existing` + `load_or_generate_tls_config` gain SAN-vs-bind-host validation + regenerate-on-mismatch), `boot.rs` (TLS `graceful_shutdown` threads `shutdown_grace_ms`; ~line 862), `tests/` (SAN regeneration + graceful-shutdown regression tests). Out of bounds: `apps/**` runtime code (the fingerprint endpoint contract from V1.92 is unchanged — `wire_contracts_changed: false`).
- **P1 (connection polish + desktop tests) lives in `apps/`**: `apps/web/src/lib/nexus/connection-storage.ts` (shape validation), `apps/web/src/lib/nexus/browser-client.ts` + `apps/web/src/pages/connect-daemon-page.tsx` (fingerprint error copy + reconnect hint), `apps/desktop/src-tauri/src/connection_config.rs` + its `tests/` (unit tests). Out of bounds: `crates/**` (no contract change).
- **P-1 (Prepare) spans `specs/daemon-runtime.md`** (§15.1 SAN-vs-bind-host normative section). No schema changes, no DESIGN.md changes, no codegen.
- **Single owner per track, parallel worktrees for P0 ‖ P1.** P-1 must land on `iteration/v1.93` before P0/P1 topic branches are cut (the codified SAN rule is the P0 implementation contract). P0 and P1 touch disjoint trees (`crates/` vs `apps/`) → no merge conflicts expected.

## 2.2 Product Success Criteria

- **TLS certs stay correct under rebinding.** A daemon rebind to a different host is detected at cert load: the SAN is re-validated against the current `bind_host`, and the cert is regenerated when mismatched (no manual `rm -rf ~/.nexus42/tls/` recovery needed). SAN match reuses the existing cert (idempotent, no needless regeneration).
- **TLS graceful shutdown respects the configured grace cap.** The TLS listener honours `shutdown_grace_ms` identically to the plain-HTTP path — no mid-response drop from an unbounded wait.
- **The remote-connection surface degrades gracefully on bad client state and gives clearer errors.** A corrupt `localStorage` connection entry fails with a clear reset path rather than feeding malformed config into the client. A fingerprint-fetch failure surfaces guidance that the Nexus desktop app is the supported remote-connection surface for self-signed certs / TOFU (honest about browser limitation). A reconnect to a pinned-matching endpoint shows a low-surface reassurance hint on the happy-path state (may be trimmed if copy review finds it noisy).
- **Desktop connection-config has test coverage.** The keychain vs app-data fallback paths in `connection_config.rs` are unit-tested.
- **No regression** in local / CLI / existing desktop / web-dev flows (`cargo test -p nexus-daemon-runtime`, `pnpm --filter web test`, `cargo test -p nexus42-desktop` / the desktop tauri tests, Vite dev proxy).
- `cargo clippy --all -- -D warnings` and `cargo +nightly-2026-06-26 fmt --all --check` pass (CI gate).
- `specs/daemon-runtime.md` §15.1 carries the codified SAN-vs-bind-host policy as normative.
- QC tri-review consolidated Approve (qc2 security lens: SAN regeneration does not widen the remote attack surface; qc3 transport lens: graceful-shutdown + cert lifecycle sound); QA verifies the regression tests reproduce the pre-fix behaviour as failing-without-the-fix.

## 3. Plans

| plan_id | Name | Status | Notes |
|---------|------|--------|-------|
| `2026-07-06-v1.93-prepare-spec-codification` | Prepare — daemon-runtime.md §15.1 SAN-vs-bind-host precision tightening (R-V192SPEC-001) | Done | Architect-owned spec amendment; tightens existing §15.1 rule + adds §16.2 cross-references. Locks P0 implementation contract. Spec edits landed in iteration-start `ed6993e9` (Phase-1 review chain 5.2/5.3); P-1 execute verified all 5 precision gaps closed. QC skipped (prepare/spec; consolidated at P-last). R-V192SPEC-001 resolved. |
| `2026-07-06-v1.93-tls-robustness` | TLS Robustness (P0 — SAN invalidation on re-bind + graceful-shutdown grace cap; R-V192P0-001/002) | Done | `@fullstack-dev`, `crates/nexus-daemon-runtime/`. Verified V1.92 shipped the production code; added 2 regression tests (`962d5847`); merged `ad049c97`. QC 3/3 Approve; QA Pass `eb6f5393`. R-V192P0-001/002 resolved. |
| `2026-07-06-v1.93-remote-connection-polish` | Remote-Connection Polish + Desktop Tests (P1 — storage validation + fingerprint error copy + reconnect hint + desktop tests; R-V192P1-001/002/003 + R-V192UX-001) | Done | `@frontend-dev`, `apps/web/` + `apps/desktop/`. Implemented (`b6c38e05`); merged `341cffa0`. QC 3/3 Approve; QA Pass `eb6f5393`. R-V192P1-001/002/003 + R-V192UX-001 resolved. |
| `2026-07-06-v1.93-closure` | Closure — naming sweep (R-V192HYG-001) + QC tri-review + QA + compound + Profile B + PR to `main` | Done | Naming sweep `@writing-specialist` (`09d0720b` + fix-wave `ef1b4efa`); QC 3/3 Approve; QA Pass `eb6f5393`; compound 2 docs updated; compass `status: completed`. PR Phase 4. R-V192HYG-001 resolved. |

Status values: `Todo` | `InProgress` | `InReview` | `Done` | `Blocked`

## 4. Milestones

| Milestone | Target date | Status |
|-----------|-------------|--------|
| Compass + plans locked (Phase 1 Review & Edit chain done) | 2026-07-06 | in progress |
| P-1 Prepare complete (§15.1 spec precision tightening landed) | 2026-07-06 | pending |
| P0 (TLS) + P1 (connection polish) implemented on parallel worktrees | 2026-07-07 | pending |
| QC tri-review Approve | 2026-07-07 | pending |
| QA + iteration-close + PR to `main` | 2026-07-07 | pending |

## 5. Acceptance Criteria

- All 8 V1.93-targeted residuals close to `lifecycle: resolved` with `resolution.commit` + `resolution.plan_id`, OR are explicitly re-deferred with a recorded reason:
  - R-V192P0-001 (SAN invalidation on re-bind) — resolved by P0.
  - R-V192P0-002 (TLS graceful_shutdown grace cap) — resolved by P0.
  - R-V192P1-001 (connection-storage load shape validation) — resolved by P1.
  - R-V192P1-002 (fingerprint fetch error copy) — resolved by P1.
  - R-V192P1-003 (desktop connection_config unit tests) — resolved by P1.
  - R-V192UX-001 (reconnect pinned-fingerprint hint) — resolved by P1.
  - R-V192SPEC-001 (spec §15.1 SAN policy precision tightening) — resolved by P-1.
  - R-V192HYG-001 (legacy naming sweep) — resolved by P-last.
- R-V191P1-005 (FindingsPage memoisation) stays deferred to "when list virtualisation lands" — unchanged, recorded in §6 Non-Goals and the deferred tracker.
- TLS cert SAN is re-validated against the current `bind_host` on load; mismatch triggers regeneration (tested); match reuses (idempotent, tested).
- TLS `graceful_shutdown` honours `shutdown_grace_ms` (tested); plain-HTTP path unchanged.
- Connection storage load validates shape and fails gracefully on corrupt entries; fingerprint-fetch error copy acknowledges the browser limitation and guides users to the supported desktop app surface for self-signed certs/TOFU (honest copy, no browser-remote promise for untrusted certs); reconnect shows a low-surface pinned-match reassurance hint on the already-detected happy path (may be trimmed during copy review if noisy — it is reassurance only, not a new flow or navigation); desktop `connection_config.rs` keychain/app-data paths are unit-tested.
- `specs/daemon-runtime.md` §15.1 codifies the SAN generation rule (loopback always + bind host + wildcard skip) + the regenerate-on-mismatch behaviour as normative.
- No regression in local / CLI / desktop / web-dev flows; `cargo clippy --all -D warnings` + nightly-2026-06-26 fmt + `pnpm --filter web test/build` + desktop tests green.
- `wire_contracts_changed: false` (no new DTOs, no schema changes, `@42ch/nexus-contracts` stays `0.20.0`).
- QC tri-review consolidated Approve; QA Pass; compass `status: completed` at Phase 3.

## 6. Non-Goals

- **Remote-endpoint manager UI** (multi-endpoint list/add/edit/delete, header switcher, endpoint switching) — YAGNI; explicitly deferred by V1.92 retrospective and grill-me lock. Trigger: ≥3 distinct remote endpoints in real usage **or** explicit author requests. The single-active-endpoint model (V1.92) is the complete product surface for this iteration; no partial UI scaffolding is introduced.
- **BL-09 standalone maturation dashboard** — deferred since V1.79; belongs to a future feature iteration, not a convergence iteration.
- **Any new wire contract / schema / endpoint** — this is a polish iteration; `wire_contracts_changed: false` is a hard non-goal. The V1.92 fingerprint endpoint contract is unchanged (additive only in V1.92). No new DTOs, no schema changes, no `schema_version` bump. This constraint is enforced in compass §1, §2.2, §5, and all plan "Out of scope" sections.
- **R-V191P1-005 (FindingsPage selection memoisation)** — stays deferred to "when list virtualisation lands"; not user-visible at the 100-item cap.
- **ACME / Let's Encrypt / network-CA integration** — non-goal unless the threat model expands beyond trusted-LAN; the V1.92 self-signed + TOFU model is the deliberate local-first choice.
- **New product surface beyond finishing V1.92's existing remote-access surface.**
- **Key rotation / per-token scoping / `~/.nexus42/auth.json` hardening** — V1.86-deferred; separate auth-mode work.
- **Unrelated residuals** — only the 8 named V1.93-targeted residuals are swept; no other open items pulled in.

## 7. Roadmap Position / Next Iteration Transition

- **Current iteration (V1.93)** — **delivered**: the V1.92 remote-access surface's deferred polish landed coherently in one convergence iteration. All 8 V1.93-targeted residuals resolved: TLS certs stay correct under rebinding (R-V192P0-001); graceful shutdown respects the grace cap (R-V192P0-002); the SAN-vs-bind-host policy is normative + precision-tightened in the spec (R-V192SPEC-001); the remote-connection UX degrades gracefully and gives clearer errors (R-V192P1-001/002 + R-V192UX-001); desktop connection-config has test coverage (R-V192P1-003); legacy naming stragglers swept (R-V192HYG-001). Tech-debt board cleared to zero-in-V1.93-scope. QC 3/3 Approve (after a fix-wave on naming-sweep stutters/anchors); QA Pass. `wire_contracts_changed: false`. 3 residuals open going forward (all V1.94): R-V192SEC-001 (medium, TOFU transport-binding), R-V193PL-001 (low, /v1/local path-literal spec hygiene), R-V191P1-005 (nit, FindingsPage memoisation — unchanged, when list virtualisation lands).
- **Next iteration (V1.94) transition criteria**:
  - Trigger: V1.93 merged to `main`; integration branch retired.
  - Selection input: PM reviews backlog against a now-polished remote-access product. Candidates:
    - **R-V192SEC-001 — TOFU transport-binding (medium security)**: the resume FingerprintGate is in-band over the same MITM-able TLS fetch; the real fix is desktop (Tauri) reqwest+rustls transport-layer cert pinning against `pinnedFingerprint`. The strongest V1.94 candidate by severity. Browser-side TOFU weakness is a documented V1.92 trade-off (raw-browser remote = non-goal).
    - **R-V193PL-001 — /v1/local path-literal spec hygiene (low)**: ~12 specs still reference `/v1/local/*` for resources now at `/v1/daemon/*` (V1.90 stragglers); needs per-path verification, not a blind find-replace.
    - **Remote-endpoint manager UI** — only after the V1.92 retrospective trigger fires (≥3 distinct remote endpoints in usage or explicit author requests).
    - **BL-09 standalone maturation dashboard** — the oldest deferred backlog item (since V1.79); SOUL data infrastructure is mature; a feature candidate once the security/path-hygiene items settle.
    - Deeper reading annotations (BL-11) if reading-surface usage data surfaces requests.
  - Output of V1.93 for V1.94: a V1.92 remote-access surface that is robust under rebinding, degrades gracefully on bad client state, and has the test coverage to safely build the next feature axis on top.
- **Long-term goal**: Nexus gives authors a local-first creative workspace that scales safely to a trusted network without forcing cloud accounts — V1.90 (remote-bind) → V1.92 (TLS + connection model) → V1.93 (robustness + polish) makes the LAN-remote-author workflow production-grade. STRATEGY Principle #1 ("local-first privacy") is preserved end-to-end.

## 8. Delivery Branch Policy

> Mirror of frontmatter; keep in sync with `{HARNESS_DIR}/status.json` `metadata`.

| Field | Value |
|-------|-------|
| `iteration_base_branch` | `main` |
| `spec_integration_branch` | `iteration/v1.93` |
| `target_branch` | `main` |

Per-plan topic branches:

| Plan | Working branch | Merge target |
|---|---|---|
| P-1 | `feature/v1.93-prepare` | `iteration/v1.93` |
| P0 | `feature/v1.93-tls-robustness` | `iteration/v1.93` |
| P1 | `feature/v1.93-remote-connection-polish` | `iteration/v1.93` |
| P-last | `feature/v1.93-closure` | `iteration/v1.93` |

**Worktree isolation**: required for P0 ‖ P1 (same-repo parallel writers: `crates/nexus-daemon-runtime/` vs `apps/web/` + `apps/desktop/`). P-1 must land on `iteration/v1.93` before P0/P1 topic branches are cut (the §15.1 SAN rule is the P0 implementation contract). P-last runs after both merge.

## 9. Risk Register

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| SAN regeneration on mismatch deletes a cert the user manually trusted (TOFU pin mismatch storm) | Medium | Medium | Regeneration produces a new fingerprint; the existing V1.92 FingerprintGate already surfaces a blocking re-pin warning on fingerprint change. Spec §15.1 records this as expected behaviour, not a bug. QA verifies the re-pin warning fires. |
| SAN validation logic misidentifies loopback / wildcard / IP-vs-hostname cases | Medium | Medium | P-1 locks the exact SAN rule (loopback always + bind host + skip wildcard) before P0 implement; P0 regression tests cover loopback, hostname, and IPv4/IPv6 bind cases. |
| `graceful_shutdown(Some(dur))` changes TLS teardown timing vs current `None` | Low | Low | One-line fix threading the existing configured duration; the plain-HTTP path already behaves this way. QA verifies no connection drop under the configured grace. |
| Desktop `connection_config.rs` tests require keychain mocking that is fragile on CI | Low | Low | Tests target the pure fallback logic paths; keychain-gated code paths use feature/env flags to stay CI-runnable. Architect confirms in P-1 if needed. |
| Naming sweep (R-V192HYG-001) touches historical prose / git-archived copy | Low | Low | Sweep scope is non-historical prose only; historical references in iteration compasses / archived docs are out of bounds. |
| Residuals proliferate if QC finds new issues during the fix-wave | Medium | Low | Accepted: this is a convergence iteration; any new V1.93 residuals are registered against V1.94 with clear severity. Low-severity by nature of the polish scope. |

## Compound Round Summary

- Knowledge docs **updated** (no new docs — both candidates had high overlap with existing compound output per `mstar-compound` Q5):
  - `conventions/surface-rename-hygiene-checklist.md` — added the anchor-link verification gate (W-002 lesson: renamed heading → broken TOC slug) + a V1.93 lesson reinforcing that the §3 stutter/anchor greps are a **mandatory pre-commit self-check** for the sweep executor (V1.93 skipped them; stutters reached QC). Also tightened the "renaming from X to Y" rule (the *X* side is historical and must stay).
  - `architecture-patterns/resolved-residual-verification.md` — added the **symmetric case: deferred-but-already-satisfied** (V1.93 architect found 3/8 `target: V1.93` residuals already code-complete on `main` from the V1.92 fix-wave; plans re-scoped from "implement" to "verify + test + paperwork"). Generalized the doc from "resolved is a claim" to "residual lifecycle is a claim (both directions)." Fixed a V1.86 discoverability gap (the doc was missing from `knowledge/README.md` index — added).
- New CONCEPTS.md entries: **0** (V1.93 introduced no new domain vocabulary; all terms inherit from the V1.92 glossary).
- compound-refresh triggered: **no** (both updates extend existing docs; no older doc contradicted or superseded).

## Iteration Retrospective (minimal)

- **Went well**: the Phase-1 review chain (5.2 architect) discovered mid-iteration that 3 of 8 residuals were already code-complete on `main` from V1.92's fix-wave — letting P0/P1 re-scope to "verify + tests + paperwork" instead of duplicating shipped code. The P0 ‖ P1 dual-track on disjoint worktrees (`crates/` vs `apps/`) merged with zero conflicts. The grill-me direction-lock (Candidate A convergence) kept scope honest — no temptation to pull R-V192SEC-001 or BL-09 into a polish iteration. Recording R-V192SEC-001 (the PR-#120 merge-close Cursor finding) as a tracked V1.94 residual closed a real audit gap.
- **Could improve**: the naming sweep (@writing-specialist) had **two consecutive empty Task returns** before succeeding on the third dispatch (after a model change) — a recurring flaky-subagent failure mode (V1.92 saw the same with qc3). More importantly, the sweep executor skipped the existing `surface-rename-hygiene-checklist.md` §3 verification greps, so stutter/anchor defects (W-001/W-002) reached QC instead of being caught pre-commit — the checklist is now updated to make those greps a mandatory self-check. QC1 was the only seat that caught the doc defects (QC2/QC3 approved) — for doc-heavy convergence iterations, a writing-specialist-leaning QC lens or a pre-QC doc-self-check would catch rename regressions earlier.
- **Next-iteration suggestion**: V1.94 headline should be **R-V192SEC-001 (TOFU transport-binding)** — the one medium-severity open item, with a clear fix path (desktop Tauri reqwest+rustls cert pinning). R-V193PL-001 (path-literal spec hygiene) is a cheap companion sweep. BL-09 (maturation dashboard) remains the feature candidate once security/hygiene settle.
