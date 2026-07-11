---
module: crates/nexus-daemon-runtime (path_enrichment) + crates/nexus-acp-host (registry scan) + apps/desktop (sidecar)
date: 2026-07-11
problem_type: architecture-pattern
category: architecture-patterns
severity: high
plan_id: 2026-07-11-v1.110-agent-scan-path-reliability
tags: [path-enrichment, agent-scan, gui-process, version-managers, nvm, volta, fnm, pnpm, yarn, which, login-shell, detection, false-negative]
applies_when: any feature that probes PATH for binaries from a GUI-launched process; any "agents/tools show as not installed despite being installed" report; extending login_equivalent_bin_dirs()
---

# GUI-Process PATH Enrichment for Agent CLI Discovery

**Track**: Knowledge (durable guidance from V1.110 FB-D3 — agents showed "Not installed" despite being installed).

## Context

macOS GUI apps (including the Tauri desktop shell) inherit a **minimal PATH** such as `/usr/bin:/bin:/usr/sbin:/sbin` — not the user's login-shell PATH. Homebrew, npm-global, cargo, and especially **version-manager** bin directories (`~/.nvm/versions/node/*/bin`, `~/.volta/bin`, etc.) are invisible to `which::which()` during `POST /v1/daemon/agent-host/scan`. Result: **every** agent reports `installed: false`, forcing the user into the custom-launch escape hatch — even though they installed several agent CLIs.

V1.101 introduced `login_equivalent_bin_dirs()` in `crates/nexus-daemon-runtime/src/path_enrichment.rs` to merge common user bin dirs into the process PATH at daemon boot. V1.110 (FB-D3) closed the largest remaining gaps: **nvm, volta, fnm, pnpm, yarn**.

## Guidance

| Rule | Reason |
|------|--------|
| Enrich PATH **once at daemon boot**, before the Tokio runtime starts | `setenv(3)` is not thread-safe against concurrent `getenv(3)` on a live multi-threaded runtime. Call `apply_process_path_enrichment()` from sync `main` before `tokio::runtime::Builder`. |
| **No shell-out** (no `sh -c "echo $PATH"`) | Shell-out is higher-risk (parse fragility, injection surface, login-shell cost). A static, existence-gated dir list is deterministic and fast. |
| **Existence-gate every dir** (`is_dir()`) | Dead PATH entries pollute `which` lookups and slow `--version` probes. Only include dirs that currently exist. |
| Resolve **version-manager active versions**, not all versions | nvm: read `alias/default` (≤2 hops) → `versions/node/<target>/bin`; fall back to **single highest-semver** glob. **Never glob-all** (PATH bloat + arbitrary `which` match). |
| Compare as **semver**, not lexicographic | `v20.11.0` vs bare `20.11.0` vs `20.11.0-rc.1` — parse numeric tuples, `take_while` digits on patch for pre-release suffixes. |
| Honor env vars primary, `~`/platform fallback | `$NVM_DIR`, `$VOLTA_HOME`, `$PNPM_HOME` take precedence; fall back to standard `~`-relative / platform paths. |
| Log resolved manager names (`vm_managers` field) | Future false-negative reports are debuggable from the enrichment log without re-running. |

## The nvm resolution algorithm (bounded, safe)

```
nvm_root = $NVM_DIR || ~/.nvm
1. Read <nvm_root>/alias/default → target string
2. resolve_nvm_alias_target(nvm_root, target, depth=0):
   - if depth > 2: return None  (cycle/chain bound)
   - <nvm_root>/versions/node/<target>/bin exists? → return it
   - else: <target> may itself be an alias → read <nvm_root>/alias/<target>, recurse depth+1
3. If alias resolution fails: glob <nvm_root>/versions/node/*/bin → single highest-semver
```

## Covered dirs (V1.110)

`~/.local/bin`, `~/bin`, `~/.cargo/bin`, `~/.npm-global/bin`, `~/.bun/bin`, `~/.asdf/shims`, `~/.local/share/mise/shims`, `/opt/homebrew/{bin,sbin}`, `/usr/local/{bin,sbin}` (existing) + **nvm** (active + highest-semver), **volta** (`$VOLTA_HOME`/`~/.volta/bin`), **fnm** (platform aliases/default + `~/.fnm/current/bin`), **pnpm** (`$PNPM_HOME`/platform), **yarn** (`~/.yarn/bin`).

## What did NOT work

- **Login-shell `echo $PATH` shell-out**: considered, rejected — parse-fragile, slow (spawns a shell), and a security review concern.
- **Glob-all nvm versions**: bloats PATH; `which` may match an unintended Node version's binary.
- **Lexicographic semver sort**: `"v9.0.0" > "v20.11.0"` lexicographically — must parse numeric tuples.

## Prevention

When a user reports "agent shows Not installed but I installed it": check the daemon enrichment log (`vm_managers` field) → if the manager is missing, add its dir resolution to `login_equivalent_bin_dirs()` (existence-gated, env-var-honoring). The custom-launch escape hatch exists for exotic paths, not for common managers.
