---
iteration_id: V1.129
start_date: 2026-07-21
status: locked
iteration_base_branch: main
target_branch: main
spec_integration_branch: iteration/v1.129
plans:
  - 2026-07-21-v1.129-p0-profile-create-reliability
  - 2026-07-21-v1.129-p1-transport-error-ux
  - 2026-07-21-v1.129-p2-dogfood-nit-closeout
---

# V1.129 Delivery Compass — Usability bug-sweep (Profile create + honest transport errors)

> **Direction lock mode: autonomous** (`/iteration-loop`, scale **M~L** — 3 business plans).
> Caller direction: "修复一切你发现的可用性问题" — concrete trigger: creating a Profile in the footer reports `Cannot reach the daemon at this address…` even though the user expects the daemon to be reachable. Sweep broader usability defects discovered during code-first research.
>
> **Phase 1 Review & Edit chain: COMPLETE — compass LOCKED.**
> - Seat 1 (product-manager): User-value + author-observable ACs strengthened on compass and all three specs; Non-Goals sharpened (full residual burn-down, visual redesign, retry/backoff, OS-keychain pinning); product concerns captured for Seat 2 (delete cascade copy, TLS fail-open, P1 promotion decision) and Seat 3 (dialog voice pass, zh-CN tone bar, i18n best-effort for R-P1-001).
> - Seat 2 (architect): All `pending-architect-lock` decisions resolved — zero placeholders remain. P0 `POST /v1/daemon/creators` (Tier-1, pool-attach per `patch_creator` pattern; `wire_contracts_changed: true`, reuse existing `CreatorDetail`). `TransportErrorKind` as instance field on `NexusClientError` with 6-step classification algorithm + TLS fail-open note. P1 `<TransportErrorBlock>` API + promote-in-iteration decision (no `@web-*` first). P2 hard delete + cascade (Work → Manuscripts/Outlines/Timelines/KB; World → KB/Timelines + `world_id NULL` on Works) + confirm-dialog contract; `wire_contracts_changed: true`. Dependency order P0 → P1 → P2 confirmed.
> - Seat 3 (writing-specialist): Three spec status lines normalized to `product-reviewed, architect-locked, writing-hygiene done (2026-07-21)`. Voice pass on P0 dialog copy (sentence-case headlines, verb-only CTAs, network headline reworded to avoid overlap with legacy blob). P2 confirm-dialog copy polished (Delete/Cancel). zh-CN tone guidance added. Cross-links verified. No pre-existing repo drift found.
>
> Direction is **locked** — do not re-question the usability bug-sweep scope.

## Autonomous direction lock record

### Caller direction mapping

| Caller signal | Plan |
|---------------|------|
| "创建 Profile 时还是报错 'Cannot reach the daemon at this address…'" | P0 (root cause) + P1 (error UX) |
| "修复一切你发现的可用性问题" (broader sweep) | P0 + P1 + P2 (visible-symptom nits) |

### Research evidence (code-first — explored before guessing)

| Finding | Source | Impact |
|---------|--------|--------|
| Web client `createCreator` calls `POST /v1/daemon/creators` (no id) | `apps/web/src/lib/nexus/browser-client.ts:168-170` | Triggers a request the daemon does not handle |
| Daemon router has **no** `POST /v1/daemon/creators` route — only `GET …/creators` (list), `GET/PATCH …/creators/:id`, `POST …/creators/:id:logout`, `GET/PUT …/creators/active` | `crates/nexus-daemon-runtime/src/api/mod.rs:140-157, 569` | The "Add creator" CTA in the footer hits an unrouted endpoint |
| In release mode the unmatched route falls through to the SPA fallback (`serve_embedded_app`) | `crates/nexus-daemon-runtime/src/api/mod.rs:598-599` | Browser sees an HTML response or 405 → fetch may throw or JSON parse fail |
| Transport-error message is a single generic blob ("URL/port wrong, daemon not running, or self-signed cert") regardless of cause | `apps/web/src/lib/nexus/browser-client.ts:614-623` | User cannot tell whether to fix URL, start daemon, switch to desktop, or open Connection settings |
| Toast just dumps the blob via `useErrorToast` (queries.ts:249) — no retry, no CTA | `apps/web/src/api/queries.ts:249-261`, `apps/web/src/components/layout/footer-profiles.tsx` | User sees the long blob and has no in-place recovery affordance |
| Prior similar bug: `fix: Setup Continue 404 on creator PATCH` (commit `e320e62d`) — `patch_creator` was missing pool-attach | git log; `.mstar/plans/2026-07-15-v1.119-setup-continue-unblock.md` | Confirms the pattern: creator endpoints need explicit daemon registration + pool-attach |
| 33 nits + 5 low-severity residuals open across V1.126/V1.127/V1.128 | `.mstar/status.json` `metadata.tech_debt_summary` | Subset has user-visible symptoms (missing DELETE in submenu R-V1126P0-T2-001; ~25 untranslated strings R-P1-001) |
| `DF-V1127-NIT-CLOSEOUT` deferred for V1.128+; not landed in V1.128 | `.mstar/knowledge/deferred-features-cross-version-tracker.md:69` | Pre-acknowledged scope; this iteration absorbs the dogfood-visible subset |

### Locked direction (single sentence)

Make Profile/Creator creation actually work end-to-end (daemon route + web flow), replace the generic `Cannot reach the daemon` blob with classified, actionable transport errors, and close the dogfood-visible subset of accumulated V1.126–V1.128 nits.

### Scale budget

- **M~L → 3 business plans** (caller `M~L`).
- Harness process (Review chain / QC / QA / compound / close / PR / merge-ready) does **not** count.

### Branch policy (autonomous resolve)

| Field | Value | Source |
|-------|-------|--------|
| `iteration_base_branch` | `main` | `status.json` root `metadata.iteration_base_branch` (V1.122→V1.128) |
| `target_branch` | `main` | `status.json` root `metadata.target_branch` |
| `spec_integration_branch` | `iteration/v1.129` | cut from `main` (matches prior iteration convention) |

## Scope

本迭代锁定的 spec 点（each is a **user-observable promise** a manual tester can verify without reading source）：

- **S-1 (P0):** An author with a running local (or reachable remote) daemon can open **Add creator**, enter a display name, click **Create**, and see the new profile in the footer avatar row — without a false "Cannot reach the daemon" error.
- **S-2 (P0):** When create fails for a real transport reason, the dialog names **what went wrong** (local daemon not running / wrong address / certificate rejected / request not recognized / timed out) and offers a **single primary next step** the author can take (Retry, Open Connection Settings, or Use Desktop App) — never the multi-cause generic blob.
- **S-3 (P1):** The same classified language and recovery CTAs appear on every other transport-failure surface the author hits before or after Profile create (resume fingerprint gate, daemon-launch gate, Connection settings, mutation toasts) — one mental model app-wide.
- **S-4 (P2):** Dogfood paths that today feel "broken in small ways" are fixed for the **visible-symptom subset** of open residuals — at minimum: DELETE present on the shell selection submenu for Work/World, and secondary pages readable in `zh-CN` (R-V1126P0-T2-001, R-P1-001). Pure code-quality nits stay deferred.

## Plans

| plan_id | Name | Status | Notes |
|---------|------|--------|-------|
| `2026-07-21-v1.129-p0-profile-create-reliability` | P0 — Profile/Creator create reliability + dialog error surface | Todo | Root-cause fix for the concrete caller bug |
| `2026-07-21-v1.129-p1-transport-error-ux` | P1 — Transport-error UX sweep across the app | Todo | Applies P0 classification to every transport surface |
| `2026-07-21-v1.129-p2-dogfood-nit-closeout` | P2 — Dogfood-visible nit closeout (V1.126–V1.128) | Todo | Subset of `metadata.tech_debt_summary` with user-visible symptoms |

Status values: `Todo` | `InProgress` | `InReview` | `Done` | `Blocked`

### Dependency graph (locked Seat 2)

```
P0 (Profile create + classification) ──► P1 (Apply classification app-wide)
P2 (Visible nits) — independent of P0/P1 but serial per iteration default
```

**Serial Phase 2 order locked: P0 → P1 → P2.** P1 depends on P0's `TransportErrorKind` + copy table. P2 is independent but serialized per `mstar-iteration` default (single implementer track; no parallelism benefit from interleaving). T3 (web submenu) may wire with a mock DELETE before T2 (daemon route) completes for parallel dev — but integration merge requires both.

## Milestones

| Milestone | Target date | Status |
|-----------|-------------|--------|
| Spec freeze (compass locked) | 2026-07-21 | pending |
| P0 dev complete | 2026-07-21 | pending |
| P1 dev complete | 2026-07-21 | pending |
| P2 dev complete | 2026-07-21 | pending |
| QC tri complete (all plans) | 2026-07-21 | pending |
| Iteration close + PR | 2026-07-21 | pending |

## Acceptance Criteria

### Iteration-level (author-observable)

Manual tester can verify each item with **pass/fail** without reading source. "Generic blob" = the multi-cause string starting with `Cannot reach the daemon at this address. This usually means the URL or port is wrong…`.

- **AC-V1129-1 (P0 — create works):** With the daemon running and reachable, open Web SPA (browser or desktop) → footer **Add creator** → enter a non-empty display name → **Create**. **Pass:** new profile appears in the footer avatar row within one refresh cycle; no error toast/dialog; profile still present after full page reload. **Fail:** generic blob, silent no-op, or profile vanishes on reload.
- **AC-V1129-2 (P0 — honest failure):** Reproduce at least two real failure modes from {daemon stopped, wrong port/URL, browser-rejected self-signed cert}. **Pass:** each mode shows a **distinct** headline (not the generic blob) and a **primary CTA** whose label matches the recovery path (Retry after start / Open Connection Settings / Use Desktop App). **Fail:** same blob for every mode, or CTA missing / dead.
- **AC-V1129-3 (P1 — same language everywhere):** Trigger a transport failure from **three** surfaces outside the create dialog: (1) resume FingerprintGate error view, (2) DaemonLaunchGate error view, (3) any mutation toast (e.g. create Work with daemon stopped). **Pass:** each surface uses the same kind-matched headline family + recovery CTA pattern; none show the generic blob. **Fail:** any of the three still dumps the blob or invents a one-off message.
- **AC-V1129-4 (P2 — dogfood nits gone):** Walk the anchored paths: (a) shell selection submenu on a Work **and** a World → **Delete** is present, confirm, row leaves the list; (b) switch locale to `zh-CN` and open each secondary page listed under R-P1-001 (Works / Schedule / Sessions / Strategies / Capabilities + their dialogs + content editor) → no English-only chrome for catalogued strings. **Pass:** both (a) and (b) symptoms gone; any extra visible nits from triage are either fixed or explicitly deferred with rationale. **Fail:** Delete still missing, or zh-CN still shows hardcoded English on a flagged page.

### Quality gates

- All plans: QC tri-review N=3 (`{SDD_DIR}/review/qc1.md`…`qc3.md` + consolidated).
- All plans: `QA gate: mandatory` (UI observable + runtime behavior change per `qa-trigger-matrix.md`).
- All plans: `wire_contracts_changed` verdict locked by architect (Seat 2): P0=`true` (new route, reuse existing type), P1=`false` (pure UI), P2=`true` (two new DELETE routes, 204 No Content).

## Non-Goals

- **Rewriting the transport layer** — classify and surface only; do not swap `fetch` or invent a new client stack.
- **OS-keychain / TOFU fingerprint pinning (`R-V192SEC-001`)** — desktop-Tauri transport hardening; remains deferred. "Use Desktop App" is the recovery path, not a pinning UI in this iteration.
- **Composite-endpoint performance (`DF-V1127-COMPOSITE-PERF`)** — scale/perf, not usability.
- **Full residual backlog / pure code-quality nits** (e.g. `R-V1126P0-QC-S-001` two-source-identity) — P2 is **dogfood-visible only**; quality-only rows stay open with deferral notes.
- **New Creator Controller business widgets** — V1.128 P2 stub stays a stub; no new Profile product surfaces beyond create reliability.
- **Visual redesign / design-system elevation** — closeout means the broken symptom goes away; not a restyle of submenu, gates, or toasts (`DF-V1122-V1121-RES` and design-system iterations own elevation).
- **Re-pinning remote fingerprints inside the create dialog** — Connection settings remains the single recovery path; dialog only deep-links.
- **Per-endpoint retry policy** (backoff, circuit breaker) — out of scope; authors get one clear Retry, not automated resilience engineering.

## Roadmap Position

- **Current iteration (V1.129):** Usability bug-sweep — Profile/Creator create works end-to-end, transport failures speak honestly with a next step, dogfood-visible nits stop accumulating "that's wrong" hits. Direct response to manual tester feedback after V1.128.
- **Next iteration (trigger):** After V1.129 ships a stable create + transport foundation, product picks the next **authoring** slice when author demand is clear — candidates: Canvas/Timeline work (`DF-V1123-WORLD-MOMENT`, `DF-V1123-WORK-BRIEF`, or `DF-V1122-COMPUTE-ON-TIMELINE`). **Do not start** that slice while create-still-broken or generic-blob still greets first open. Owner: product-manager (scope pick) + architect.
- **Final goal:** A Nexus build where a manual tester can go from "open the app" → create a creator → switch profiles → open a World → edit timeline **without** a generic transport blob, a missing endpoint, or an obviously unfinished chrome string.

## Delivery Branch Policy

> Mirror of frontmatter; keep in sync with `.mstar/status.json` `metadata`.

| Field | Value |
|-------|-------|
| `iteration_base_branch` | `main` |
| `spec_integration_branch` | `iteration/v1.129` |
| `target_branch` | `main` |

## Risk Register

> Updated by architect (Seat 2). Risks marked ~~struck~~ are resolved by locked architectural decisions; remaining risks are sharpened.

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| ~~New `POST /v1/daemon/creators` route reveals deeper pool-attach gap~~ | ~~Med~~ | — | Resolved: pool-attach pattern locked (mirrors `patch_creator` at `creators.rs:533-541`). T1 RCA will surface any gap before T2 code. |
| Transport classification mis-categorizes (e.g., CORS preflight vs TLS) | Med | Med | P1 includes a per-kind unit test matrix; `tls` is best-effort with explicit fail-open-to-`network`; `unknown` catch-all prevents wrong copy. Classification algorithm locked in P0 spec § Interfaces. |
| ~~Nit closeout balloons past visible subset~~ | ~~Med~~ | — | Resolved: P2 T1 triage produces explicit `visible`/`quality-only` table from `status.json` `residual_findings`. P2 plan enumerates scope — pure-quality nits stay deferred with `decision: defer` rationale. |
| Self-signed-cert error in Chrome/Firefox cannot be distinguished from generic network failure | Med | Low | **Sharpened:** TLS classification is best-effort via error-message substring matching. When ambiguous, fall back to `network`. Copy for `tls` (informational "This browser rejected…" + "Use Desktop App" CTA) does not over-claim. The real loss: authors using a remote daemon with self-signed cert in a browser will see `network` instead of `tls` → CTA mismatch. Acceptable trade-off for V1.129; OS-keychain pinning (`R-V192SEC-001`) is the proper fix. Documented in P0 spec as "TLS fail-open note." |
| DELETE cascade surprises author (hard delete removes child entities) | Low | High | Confirm dialog names cascaded items per Seat 2 cascade rules. Copy tells the author exactly what will be removed. Irreversibility is explicit ("This cannot be undone."). |

## Seat 1 — Product concern

> Product-manager Review & Edit (Seat 1). Not a fourth plan. Architect (Seat 2) / writing (Seat 3) should resolve or explicitly waive.

1. **Delete cascade is author-visible risk (P2):** ~~Hard-deleting a Work/World without a clear cascade story can strand manuscripts/timelines or surprise-delete author work.~~ **Resolved by Seat 2:** hard delete + cascade rules locked (P2 spec § Interfaces). Confirm dialog names what will be removed. **Resolved by Seat 3:** confirm dialog copy polished (names item, names cascaded items, irreversibility explicit).
2. **TLS classification honesty (P0/P1):** ~~Browsers often cannot distinguish cert failure from generic network failure.~~ **Resolved by Seat 2:** TLS fail-open note locked in P0 spec § Interfaces. `tls` is best-effort; fallback to `network` when ambiguous. Copy does not over-claim. **Resolved by Seat 3:** dialog table copy checked for honest tone; no unfair blame.
3. **i18n quality bar (P2 R-P1-001):** Best-effort zh-CN is acceptable to close the residual **if** keys are complete and readable; mark machine-translated strings for human review in residual note rather than blocking ship. **Resolved by Seat 3:** zh-CN tone guidance added to P0 spec; EN error table tone-checked for Voice & Content alignment. Machine-translated strings flagged with `// MT: needs review` convention.
4. **No fourth plan:** Anything outside create reliability, transport honesty, and visible-nit closeout stays deferred (perf, keychain pinning, Controller widgets, visual elevation).

## Iteration package

> Sibling paths under `.mstar/iterations/v1.129/` — not in `specs/` or `knowledge/`. Promoted to knowledge at iteration-close via `mstar-compound`.

| Path | Purpose |
|------|---------|
| `specs/profile-create-reliability.md` | P0 — create works + dialog classified errors |
| `specs/transport-error-ux.md` | P1 — app-wide transport language |
| `specs/dogfood-nit-closeout.md` | P2 — dogfood-visible nits only |
| `guides/` | Optional process/exploration notes |

## Quality Gate Summary

> Filled at iteration-close. Per-plan gate details in each main plan; open residual SSOT in `.mstar/status.json`.

| plan_id | QC decision | QA gate | Residuals | Durable summary |
|---------|-------------|---------|-----------|-----------------|
| `2026-07-21-v1.129-p0-profile-create-reliability` | pending | mandatory | pending | pending |
| `2026-07-21-v1.129-p1-transport-error-ux` | pending | mandatory | pending | pending |
| `2026-07-21-v1.129-p2-dogfood-nit-closeout` | pending | mandatory | pending | pending |

## Compound Round Summary

> Filled at iteration-close.

## Iteration Retrospective (minimal)

> Filled at iteration-close.
