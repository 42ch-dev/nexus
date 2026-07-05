---
report_kind: qc-consolidated
plan_id: "2026-06-25-v1.66-mid-meta-tracking"
consolidated_verdict: "Approve"
generated_at: "2026-06-26"
wave: "initial tri-review + fix-wave-1 + targeted re-review"
---

# QC Consolidated Decision — V1.66 P-mid (Approve after fix-wave-1)

## Final tri-review verdict (post fix-wave-1 + targeted re-review)

| Reviewer | Focus | Initial | Re-review (fix-wave-1) | Final |
|----------|-------|---------|------------------------|-------|
| qc1 (`@qc-specialist`) | Architecture coherence + maintainability | Request Changes (3W) | **Approve** (all 3W + S-3 resolved) | ✅ Approve |
| qc2 (`@qc-specialist-2`) | Security + correctness | Approve (2W accepted §5 #8 trade-offs) | *(excluded — already Approve)* | ✅ Approve |
| qc3 (`@qc-specialist-3`) | Performance + reliability | Request Changes (4W) | **Approve** (all 4W resolved) | ✅ Approve |

**Consolidated verdict: APPROVE** (all 3 seats Approve; zero Critical; zero unresolved Warning). fix-wave-1 (`b0a714c2`, merged to `iteration/v1.66` @ `1e595fb5`) resolved all 8 findings (F1–F8); targeted re-review confirmed each with verifiable evidence.

## Initial tri-review summary (Request Changes → fix-wave-1)

| Reviewer | Initial verdict | 🔴 Crit | 🟡 Warn | 🟢 Sugg |
|----------|-----------------|---------|---------|---------|
| qc1 | Request Changes | 0 | 3 | 5 |
| qc2 | Approve | 0 | 2 (accepted §5 #8 trade-offs) | 6 |
| qc3 | Request Changes | 0 | 4 | 5 |

Zero Critical. §5 LOCKED-decision conformance verified by qc1 (all 10 faithful, no drift). `wire_contracts_changed: false` confirmed.

## Alignment verification (PM)

- All 3 reviewers used text-identical `plan_id` (`2026-06-25-v1.66-mid-meta-tracking`), `Feature / scope label`, `Review range / Diff basis` (`6e1f18e0..c8d22976`), `Working branch` (`iteration/v1.66`), `Review cwd` (repo root). ✓
- §5 LOCKED-decision conformance: qc1 explicitly verified all 10 §5 items faithful (F-VALID-1) — **no drift from locked architect decisions**. The Warnings are implementation/doc gaps within the locked design, not re-opened decisions.
- `wire_contracts_changed: false` confirmed (qc1: zero diff under `schemas/` or `crates/nexus-contracts/src/generated/`).

## Consolidated fix list (fix-wave-1 → targeted re-review qc1 + qc3)

The Warnings cluster into 8 fixes, all in `apps/desktop` + CI (qc2 Approve; not in targeted re-review per `mstar-review-qc`).

| # | Source | Finding | Fix | Owner |
|---|--------|---------|-----|-------|
| F1 | qc3 W-1 | **Resolved daemon port NOT exposed to SPA** — `TauriClient` re-derives port via `process.env` (undefined in webview) → always 8420; if `NEXUS_DAEMON_PORT` overridden, sidecar runs on custom port but SPA hits 8420 → all Local API calls fail while status shows "running". Violates `daemon-runtime.md` §12.3. | Inject resolved port to webview (e.g., `window.__NEXUS_DAEMON_PORT__` via Tauri, or a `get_daemon_port` command); `TauriClient` uses authoritative port. **(correctness bug)** | @fullstack-dev |
| F2 | qc3 W-2 | **Attached (non-owned) daemon crash undetected** — status stays "running" indefinitely if external daemon crashes (no health-reprobe for attached daemons). | Health-reprobe loop for attached daemons, OR active probe in `getDaemonStatus` (short timeout); transition to error/stopped + offer Restart. | @fullstack-dev |
| F3 | qc1 W-1 | **`cargo build`/`test`/`clippy` fails on fresh checkout** — sidecar binary gitignored but required at compile time by `tauri-build`; opaque error. Intersects the `cargo clippy --all` CI gate. | "Development prerequisites" block in `apps/desktop/AGENTS.md` + a fail-fast pre-check script that points to `pnpm -w run sidecar`. | @fullstack-dev |
| F4 | qc1 W-2 | **Misleading `_p1_runtime_deps_note` in `apps/desktop/package.json`** — claims future JS deps that don't exist (plugin-shell is a Rust crate; §5 #7 uses `window.__TAURI__` global, no JS deps). | Rewrite note to reflect as-built design. | @fullstack-dev |
| F5 | qc1 W-3 | **Stale `apps/desktop/AGENTS.md` scope table** — lists bundled sidecar as "Out (V1.67+)" but P1 shipped it. | Move sidecar to "In"; note in-process lib link is the actual V1.67+ deferral. | @fullstack-dev |
| F6 | qc3 W-3 | **CI `desktop-build` has no rust/incremental cache** — 15–25 min cold builds every push. | Add `Swatinem/rust-cache@v2` keyed to desktop job + both targets; cache `apps/desktop/src-tauri/target` + repo-root `target`. | @fullstack-dev |
| F7 | qc3 W-4 | **CI path filter incomplete** — missing transitive crates (`nexus-local-db`, `nexus-orchestration`, etc.); PRs touching them won't trigger desktop build. | Add `crates/**` to path filter. | @fullstack-dev |
| F8 | qc1 S-3 (promoted to fix — one-line UX) | **`error` pill label hard-coded "Port unavailable"** even for generic boot failure (Rust produces 2 distinct detail strings). | Split label or use spec wording (`Daemon did not start` for generic) per `daemon-runtime.md` §12.2. | @fullstack-dev |

## Deferred to V1.67+ (Suggestions → residual_findings at P-last)

qc1 S-1 (event-driven status, replace dual poll loops), S-2 (robust unwrap sidecar.rs:198), S-4 (CI §5 #10 conditional fallback), S-5 (idiomatic Tauri exit hook); qc2 Suggestions (sidecar ownership restart messaging, TOCTOU comment, RuntimeLockGuard doc, etc.); qc3 S-1 (reset restart_count on manual start), S-2 (reuse reqwest::Client), S-3 (directory fsync after rename), S-4 (RuntimeLockGuard doc), S-5 (backoff jitter). All registered as `severity: low` deferred residuals at P-last.

## qc2 accepted Warnings (no fix — documented §5 #8 trade-offs)

- Broad opener scope `**` — accepted per §5 #8 LOCKED (runtime `guard_path` canonicalize is authoritative; opener scope is defense-in-depth only). qc2 verified no bypass.
- Chapter tests handler-direct only (not full HTTP E2E) — axum-test hyphenated-UUID limitation; guard logic covered. Note: residual `R-V165-QC1-W2` ("HTTP-level integration tests") was closed via handler-direct tests; qc2 flagged the gap as accepted for V1.66. PM accepts (the W-002 guard logic IS tested; full HTTP E2E is a future hardening).

## Next steps (PM)

1. Dispatch **fix-wave-1** (@fullstack-dev, single Assignment, fix branch `fix/v1.66-qc-fix-wave-1` from `iteration/v1.66`) covering F1–F8.
2. Merge fix-wave-1 → `iteration/v1.66`.
3. **Targeted re-review**: qc1 + qc3 only (N=2, the seats that raised blocking findings; qc2 already Approve → excluded per `mstar-review-qc` targeted re-review rule). Same alignment fields; each updates its own `qcN.md` with a `## Revalidation` section + verdict.
4. On Approve (qc1 + qc3): dispatch `@qa-engineer` verification (same alignment fields) — runtime behavior on macOS (the T8 GUI launch + sidecar autostart + port-override path that F1 fixes).
5. P-last: residual convergence (register deferred Suggestions) + spec promotion + Profile B compaction + PR `iteration/v1.66` → `main`.
