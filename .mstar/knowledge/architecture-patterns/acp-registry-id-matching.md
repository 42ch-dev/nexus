---
module: apps/web (agent-picker) + crates/nexus-acp-host (registry) + apps/web/src/pages (hosts)
date: 2026-07-11
problem_type: architecture-pattern
category: architecture-patterns
severity: medium
plan_id: 2026-07-11-v1.110-agent-picker-ux-polish
tags: [agent-picker, acp-registry, registry_agent_id, matching, display-name, priority, common-first, partition, false-negative]
applies_when: matching UI elements against the ACP CDN registry; any "priority" or "pinned" list of agents; ordering agents by user preference
---

# ACP Registry Matching: id vs Display Name

**Track**: Knowledge (durable guidance from V1.110 FB-D2 C1 — priority list didn't match).

## Context

V1.110 FB-D2 added a common-first priority list to `AgentPicker`. The first implementation matched by `agent.name` using the user's mental-model names (`"Codex CLI"`, `"Claude Code"`, etc.). **This produced an empty common grid in production** because the live ACP CDN registry emits different `name` values:

| User's mental model | Registry `id` (stable) | Registry `name` (actual) |
|---------------------|------------------------|--------------------------|
| Codex CLI | `codex-acp` | **Codex** |
| Claude Code | `claude-acp` | **Claude Agent** |
| Cursor CLI | `cursor` | **Cursor** |
| OpenCode | `opencode` | OpenCode ✓ |
| Kimi Code | `kimi` | **Kimi CLI** |
| Qoder | `qoder` | **Qoder CLI** |
| GitHub Copilot CLI | `github-copilot-cli` | **GitHub Copilot** |
| Pi | `pi-acp` | **pi ACP** |

The registry `name` is a **display label that can change**; the registry `id` is the **stable key**.

## Guidance

| Rule | Reason |
|------|--------|
| Match priority/pinning lists by `registry_agent_id` (the `id`), not by `name` | `name` is a mutable display label; `id` is the stable identifier. The ACP CDN may rename an agent without changing its id. |
| `AgentPickerItem.id` IS `registry_agent_id` (via `agentPickerId()`) | For registered agents, the host maps `id = registry_agent_id || name`. So `agent.id` is the reliable match key downstream. |
| Add a **case-insensitive name `includes` fallback** for agents not yet in the registry | Forward-compat: agents like Hermes/Kiro that the user named but aren't in the CDN yet will match by name when they appear. Accept the low false-positive risk (e.g. "cursor-tool" matching "cursor"). |
| When a user reports "my pinned agent isn't showing first" | Verify the priority list uses the registry `id` (check the live CDN: `curl https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json | jq '.agents[] | {id, name}'`). |

## The matching algorithm

```typescript
function findPriorityIndex(agent: AgentPickerItem): number {
  const lowerName = agent.name.toLowerCase();
  for (let i = 0; i < COMMON_AGENT_PRIORITY.length; i++) {
    const key = COMMON_AGENT_PRIORITY[i]!;
    if (agent.id === key || lowerName.includes(key.toLowerCase())) {
      return i;
    }
  }
  return -1; // → rest partition
}
```

Priority keys are registry ids (`codex-acp`, `claude-acp`, `cursor`, ...) + forward-compat name tokens (`hermes`, `kiro`).

## What did NOT work

- **Verbatim user-name matching** (`"Codex CLI"` === `agent.name`): produced zero matches because the registry name is `"Codex"`. The user's mental model of agent names ≠ the CDN's display labels.
- **Match by `name` only**: brittle to CDN renames.

## Prevention

When implementing any registry-dependent priority/pinning/matching: **always verify against the live CDN** (`curl ... | jq '.agents[] | {id, name}'`) before locking the match keys. The user's naming intuition is a starting point, not the contract.
