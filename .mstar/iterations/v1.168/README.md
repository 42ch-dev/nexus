# V1.168 package

Iteration package — `delivery-compass.md` + specs/guides. Not `{KNOWLEDGE_DIR}/`. Worthy content is **promoted** at iteration-close via `mstar-compound`.

Native host series: authors keep `claude-native` / `codex-native` without Nexus-owned wire parsers; add bring-your-own `dsh-native`. ACP-first unchanged.

## Documents

| Document | Kind | Description | Status |
|----------|------|-------------|--------|
| [delivery-compass.md](delivery-compass.md) | compass | Iteration product SSOT — Scope / AC / Non-Goals / Roadmap | active |
| [specs/v1.168-native-host-locks.md](specs/v1.168-native-host-locks.md) | spec | Grill-me locks PD-1..PD-7 + architect resolutions AR-1..AR-7 (decision SSOT) | locked |

`guides/` is empty at start — exploration notes land there during execution.

## Promotion log (filled at iteration-close)

| Source | Promoted to | Date | Notes |
|--------|-------------|------|-------|
| [specs/v1.168-native-host-locks.md](specs/v1.168-native-host-locks.md) | `.mstar/knowledge/architecture-patterns/native-cli-provider-adapter-pattern.md` (updated) | 2026-08-17 | Essence promoted: decode-drift contract (PD-3/AR-1/AR-7), per-session client locks, no frame-gap timeouts, turn-id filtering, `dsh_limited` honest descriptor, mock-stub testing, discovery routes. Locks kept as iteration snapshot. |
| `.mstar/sdd/*/fix-wave-1-report.md` (P2) | `.mstar/knowledge/workflow-patterns/process-env-lock-fixture-spawn-serialization.md` (new) | 2026-08-17 | PROCESS_ENV_LOCK flake root-cause. |
