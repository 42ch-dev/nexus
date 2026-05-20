# Nexus Daemon Runtime Architecture

## 0. Document position

| Attribute | Value |
| --- | --- |
| **Normative scope** | Architecture boundaries, process model, subsystem responsibilities, pre-release constraints |
| **Related** | [cli-spec.md](./cli-spec.md), [local-runtime-boundary.md](./local-runtime-boundary.md), [agent-host.md](./agent-host.md) |

---

## 1. Objective

Converge on **one user-facing binary** (`nexus42`) with **daemon runtime** as an internal process mode — not a separate product binary (daemon runtime).

Pre-release posture: no compatibility migration layer required; local state may be wiped (see nexus-platform `v1-spec/adr/adr-023-pre-release-cli-breaking-refactor-v1.md` if needed).

---

## 2. Normative layering

```text
nexus42 (CLI — entry, routing, UX)
  ├─ nexus-daemon-runtime (library — lifecycle, subsystems, local API)
  │    ├─ local DB / workspace handles
  │    ├─ schedule / worker supervision
  │    ├─ loopback Local API (/v1/local/*) — local product only
  │    └─ AgentHostSubsystem → nexus-agent-host (see agent-host)
  └─ nexus-cloud-sync (CLI-only; platform HTTP + optional legacy-sync)
```

Platform sync and registration **must not** live in daemon-runtime. See [local-cloud-crate-architecture.md](./local-cloud-crate-architecture.md).

**Rules**:

1. Only **`nexus42`** is a user-facing executable artifact.
2. **Daemon** is started via CLI (`nexus42 daemon start`, foreground or background); background mode may use a hidden internal entry (implementation detail in knowledge SSOT).
3. **Local API** remains loopback HTTP and/or Unix socket; clients must not assume a separate daemon product binary.

---

## 3. Subsystem responsibilities

| Subsystem | Owns | Does not own |
| --- | --- | --- |
| CLI | Parsing, one-shot commands, spawning daemon mode, user errors | Long-lived agent protocol details |
| Daemon runtime | SQLite handles, Local API listener, orchestration/agent-host, graceful shutdown | Platform HTTP, sync outbox, creator registration |
| Agent host | Managed agent sessions (see agent-host) | Platform HTTP |
| Cloud sync (CLI) | Platform HTTP, legacy bundle sync (`nexus-cloud-sync`) | Daemon Local API |

---

## 4. Process model

### 4.1 Foreground

`nexus42 daemon start --foreground` runs the runtime in the current process until shutdown.

### 4.2 Background

Default `nexus42 daemon start`: preflight → spawn internal daemon-run mode → parent exits after startup gate. **Semantics** are normative; exact argv names are implementation SSOT.

### 4.3 Control plane

`status`, `stop`, `restart` coordinate via runtime health and process supervision (parity with prior daemon product behavior).

---

## 5. ACP role invariant

Daemon runtime is a **local supervisor**. It is **not** an ACP Agent or ACP Server and must **not** be advertised via ACP Registry as an agent. ACP Client role stays on the Nexus control plane path ([local-runtime-boundary](./local-runtime-boundary.md) §1).

---

## 6. Observability & errors

- User-facing logs refer to **Nexus daemon runtime**, not legacy daemon runtime product naming.
- Errors are owned by layer: CLI (misuse) → runtime (orchestration) → API handlers (request validation).

---

## 7. Acceptance criteria (architecture level)

1. Specs and docs do not **require** a standalone daemon runtime product binary.
2. Health endpoint reachable after foreground and background start.
3. Stop/restart leaves no orphan runtime without documented force path.
4. Agent-host subsystem can start under Managed-only rules ([agent-host](./agent-host.md)).

---

## 8. Verification matrix

1. `nexus42 daemon start --foreground` boots and serves health endpoint
2. Default background start returns and runtime stays alive
3. `status` sees running runtime
4. `stop` terminates runtime cleanly
5. `restart` replaces process and health returns
6. ACP-related runtime paths continue to function
7. Schedule supervisor boot and shutdown hooks remain valid

## 9. Implementation batches

### Batch 1: Runtime extraction

- Create `nexus-daemon-runtime`; migrate modules from legacy daemon runtime layout

### Batch 2: Single-binary wiring

- Wire `nexus42 daemon` to runtime / internal-run mode

### Batch 3: Remove old daemon crate

- Remove daemon runtime workspace member and references

### Batch 4: Naming and hardening

- Unify user-facing wording and logs; finalize reliability edge cases

