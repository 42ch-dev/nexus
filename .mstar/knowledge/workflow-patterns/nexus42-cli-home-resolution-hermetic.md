---
module: nexus42-cli
date: 2026-08-08
last_updated: 2026-08-16
problem_type: developer_experience
category: workflow-patterns
severity: low
applies_when:
  - "Writing hermetic/sandboxed walkthroughs that run both the nexus42 CLI and nexus-runtime"
  - "Debugging why CLI writes land in the real ~/.nexus42 during a temp-home test"
tags: [nexus42, nexus-runtime, home-resolution, hermetic-testing, quickstart]
---

# nexus42 CLI resolves home from $HOME; nexus-runtime reads NEXUS42_HOME — hermetic blocks need HOME

## Context

V1.154 P3's integrator quickstart aimed for a hermetic first-run with
`export NEXUS42_HOME=/tmp/nexus-e2`, then ran `nexus42 creator ...` CLI steps
and `nexus-runtime`. QC caught the split: the CLI silently wrote to the real
`~/.nexus42` while the runtime booted against the empty temp home (fail-closed)
— a cascade into every later step.

## Guidance

- `nexus42` CLI home resolution: `dirs::home_dir().join(".nexus42")` — `$HOME`
  ONLY (`config.rs::nexus_home`). No `--home`, no `NEXUS42_HOME` reader in the
  CLI.
- `nexus-runtime` home resolution: `--home` > `NEXUS42_HOME` > `$HOME`
  (`bin/nexus-runtime.rs::resolve_home`), where the value is the *parent* of
  `.nexus42`.
- Hermetic blocks that drive BOTH binaries must export `HOME=/tmp/...` (both
  resolve it), or pair CLI `HOME=` with runtime `--home`/`NEXUS42_HOME`.

```bash
export HOME=/tmp/nexus-e2          # CLI steps (nexus42 resolves $HOME only)
export NEXUS42_HOME=/tmp/nexus-e2  # optional: runtime ALSO resolves it
nexus42 creator workspace init ... # writes $HOME/.nexus42/... = temp home
nexus-runtime ...                  # boots against the same temp home
```

## Why This Matters

A walkthrough that "works" on the author's machine (real home present) silently
pollutes the real `~/.nexus42` for anyone following it, and fails closed for
anyone with an empty home. Same-class trap (updated V1.167): `nexus42 creator
register` **without flags** is a platform operation (auth token + network;
writes `auth.json`, not `config.toml`) — hermetic flows use `creator register
--local --name <n>` (V1.167 P2: mints persistent `ctr_local*`, sets active,
and materializes the workspace `creators` row so `world create` works — see
[creator-bootstrap-two-store-materialization.md](../../architecture-patterns/creator-bootstrap-two-store-materialization.md))
or `workspace init`'s FS fallback (`active_creator_id=local`).

## When to Apply

- Authoring quickstarts/scripts that exercise both binaries.
- Writing sandboxed tests that must not touch the real home.

## Examples

### Before
`export NEXUS42_HOME=/tmp/nexus-e2` then CLI steps → real `~/.nexus42`
polluted; runtime fails against empty temp home.

### After
`export HOME=/tmp/nexus-e2` for CLI + runtime (same home), documented as the
CLI's binding; hermetic block genuinely hermetic.
