# V1.110 P1 — Agent scan PATH reliability (FB-D3) — spec draft

> Iteration-scoped spec draft. Architect refines in §5.2 Review & Edit chain.

## 1. Problem

`crates/nexus-daemon-runtime/src/path_enrichment.rs`
`login_equivalent_bin_dirs()` omits the most common Node.js version-manager
global bin directories. Since many agent CLIs install via `npm install -g`,
and most macOS devs use nvm, the daemon's `which::which(binary)` probe
(`crates/nexus-acp-host/src/registry.rs:585`) cannot find them → all agents
report "Not installed". This is deferred residual `R-V1101P0-003`
materializing as a real user-facing bug.

## 2. Current covered dirs (evidence)

`login_equivalent_bin_dirs()` currently includes (existence-gated):

- `~/.local/bin`, `~/bin`, `~/.cargo/bin`, `~/.npm-global/bin`, `~/.bun/bin`
- `~/.asdf/shims`, `~/.local/share/mise/shims`
- macOS: `/opt/homebrew/bin`, `/opt/homebrew/sbin`, `/usr/local/bin`, `/usr/local/sbin`
- linux: `/usr/local/bin`, `/home/linuxbrew/.linuxbrew/bin`, `/snap/bin`

## 3. Missing dirs (target — locked resolution rules)

| Manager | Dir(s) | Resolution rule (locked) |
|---------|--------|--------------------------|
| **nvm** | `<nvm_root>/versions/node/<ver>/bin` | **Active-version-first.** `nvm_root = $NVM_DIR` if set else `~/.nvm`. Read `<nvm_root>/alias/default` (trim); resolve target (≤2 alias hops) → `<nvm_root>/versions/node/<target>/bin`, existence-gated. If alias absent or resolved dir missing: glob `<nvm_root>/versions/node/*/bin`, pick the **single highest-semver** match (sort desc, take first), existence-gated. **Never glob-all** (PATH bloat; `which` would resolve an arbitrary first match). |
| **volta** | `~/.volta/bin` | `$VOLTA_HOME` if set else `~/.volta` → `< volta_home >/bin`, existence-gated. |
| **fnm** | `~/Library/Application Support/fnm/aliases/default/bin` (macOS); `~/.local/share/fnm/aliases/default/bin` (linux) | platform path, existence-gated; fallback `~/.fnm/current/bin` if present. |
| **pnpm** | `~/Library/pnpm` (macOS); `~/.local/share/pnpm` (linux) | `$PNPM_HOME` if set else platform path, existence-gated. |
| **yarn** | `~/.yarn/bin` | `~/.yarn/bin`, existence-gated. |

**Env-var precedence (Q2 locked):** env-var **primary**, `~`-relative/platform
**fallback** — matches how each tool resolves its own home. On a GUI-launched
daemon the env vars are typically unset (GUI apps don't source the shell rc),
so the `~`/platform fallback is the primary code path in practice; honoring the
env var when present is correct for users who exported it inheritably.

## 4. Locked decisions (architect §5.2 review)

### Q1 — nvm: active-version-only (NOT glob-all) ✅ LOCKED

**Verdict:** resolve the **active** version dir via `<nvm_root>/alias/default`
(≤2 alias-hops to bound chained aliases like `default → lts/hydrogen → v20.10`),
existence-gated. Glob `<nvm_root>/versions/node/*/bin` **only as a fallback**
when the alias file is absent or the resolved dir does not exist — and then
take the **single highest-semver** match, not all matches.

**Rationale:** globbing all installed node versions would add 5–20 dirs for
heavy nvm users (PATH bloat), and `which::which` resolves the FIRST PATH match
(glob order is not semver-sorted), so the detected agent could come from an
arbitrary old node version — not what the user's login shell would use. The
correctness target is "login-shell-equivalent", and a login shell with nvm
loaded puts the **active** version's bin first. Single active dir = same agent
the user's shell resolves, minimal PATH growth.

### Q2 — Env-var precedence: env-var primary, `~`/platform fallback ✅ LOCKED

`$NVM_DIR`, `$VOLTA_HOME`, `$PNPM_HOME` honored when set (authoritative for
custom install locations); `~`-relative / platform paths are the fallback for
the default-install + GUI-launched case. All existence-gated. Do not invent
`$NVM_HOME` (not an nvm variable; nvm uses `$NVM_DIR`).

### Q3 — Diagnostics: extend the existing enrichment log ✅ LOCKED

Extend `apply_process_path_enrichment`'s existing `tracing::info!` (which
already logs `before`/`after`/`added` counts) with a `vm_managers` field listing
the resolved version-manager names (e.g. `["nvm","volta","pnpm"]`). Per-manager
visibility lets future false-negatives be debugged from logs without re-running
a scan. Keep at `info!` when ≥1 manager resolved; the existing `debug!` path
covers the "already enriched / none resolved" case.

## 5. Constraints (Global)

- **No shell-out** (keep the "No shell-out" design note in `path_enrichment.rs`).
- All added dirs **existence-gated** (no dead PATH entries).
- `merge_path` is already idempotent + de-duplicating — rely on it, do not
  re-implement de-dup.
- **No wire/schema change** (`wire_contracts_changed: false`) — no scan-response
  shape change; this is daemon-boot PATH enrichment only.
- **nvm alias resolution is bounded** (≤2 hops) to prevent alias-cycle infinite
  loops; a cycle or unresolvable target falls through to the highest-semver glob
  fallback.
- Highest-semver selection must handle `v`-prefixed and bare version dir names
  (`v20.11.0`, `20.11.0`); compare as semver, not lexicographic string.
