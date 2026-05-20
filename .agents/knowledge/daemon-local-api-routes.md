# Daemon Local API Routes (Implementation SSOT)

## Document metadata

| Field | Value |
| --- | --- |
| **Status** | Active |
| **Authority** | Implementation SSOT — derived from `crates/nexus-daemon-runtime/src/api/mod.rs` |
| **Normative upstream** | `nexus-platform` `.agents/designs/v1-spec/local/local-runtime-boundary-v1.md` (topology only); product boundaries in `daemon-runtime-v1.md`, `agent-host-v1.md` |
| **Supersedes** | Route tables in `daemon-api-workspace-write-architecture.md` §1 (pre-V1.20) |

> Reconcile this file whenever `api/mod.rs` changes. Unguarded vs protected routing is defined in the module header comment and `create_router()`.

---

## Auth model (V1.20+)

| Class | Routes | Notes |
| --- | --- | --- |
| **Unguarded** | `GET /v1/local/runtime/health`, `GET /v1/local/runtime/status`, `GET /v1/local/daemon/status` | No `X-API-Key` even in keyed-all mode |
| **Protected** | All other `/v1/local/*` | `require_api_key` middleware (`X-API-Key`) |

---

## Route inventory (as wired)

### Runtime (unguarded)

| Method | Path | Handler area |
| --- | --- | --- |
| GET | `/v1/local/runtime/health` | `handlers::runtime::health` |
| GET | `/v1/local/runtime/status` | `handlers::runtime::status` |
| GET | `/v1/local/daemon/status` | `handlers::runtime::daemon_status` |

### Monitoring

| Method | Path | Handler area |
| --- | --- | --- |
| GET | `/v1/local/monitoring/pool` | `handlers::monitoring::pool_status` |

### Workspace

| Method | Path | Handler area |
| --- | --- | --- |
| GET | `/v1/local/workspace` | `handlers::workspace::info` |
| POST | `/v1/local/workspace/init` | `handlers::workspace::init_workspace` |
| GET, POST | `/v1/local/workspaces` | `handlers::workspaces::list_workspaces`, `create_workspace` |
| GET, PUT | `/v1/local/workspaces/active` | `handlers::workspaces::get_active_workspace`, `set_active_workspace` |

### Creators (legacy list + registration)

| Method | Path | Handler area |
| --- | --- | --- |
| GET | `/v1/local/creators` | `handlers::creators::list` |
| POST | `/v1/local/creators/registrations` | `handlers::creator_registrations::initiate_registration` |
| POST | `/v1/local/creators/registrations/{code}:verify` | `handlers::creator_registrations::verify_registration` |
| GET | `/v1/local/creators/{creator_id}` | `handlers::creator_registrations::get_creator` |
| POST | `/v1/local/creators/{creator_id}:logout` | `handlers::creator_registrations::logout_creator` |
| GET, PUT | `/v1/local/creators/active` | `handlers::creator_registrations::get_active_creator`, `set_active_creator` |
| GET | `/v1/local/references` | `handlers::references::list` |

### Presets

| Method | Path | Handler area |
| --- | --- | --- |
| GET, POST | `/v1/local/presets` | `handlers::preset_management::list_presets`, `scaffold_preset` |
| POST | `/v1/local/presets:validate` | `handlers::preset_management::validate_preset` |
| POST | `/v1/local/presets/{id}:reload` | `handlers::preset_management::reload_preset` |

### KB

| Method | Path | Handler area |
| --- | --- | --- |
| GET, POST | `/v1/local/kb/entries` | `handlers::kb::list_entries`, `add_entry` |
| GET, DELETE | `/v1/local/kb/entries/{entry_id}` | `handlers::kb::get_entry`, `delete_entry` |

### Memory (pending review)

| Method | Path | Handler area |
| --- | --- | --- |
| POST, GET | `/v1/local/memory/pending-review` | `handlers::memory::create_pending_review`, `list_pending_reviews` |
| GET | `/v1/local/memory/pending-review/count` | `handlers::memory::count_pending_reviews` |
| DELETE | `/v1/local/memory/pending-review/{id}` | `handlers::memory::delete_pending_review` |

### ACP tool execution (internal)

| Method | Path | Handler area |
| --- | --- | --- |
| POST | `/v1/local/agent-host/internal/tool-executions` | `handlers::acp::tool_execute` |

Public `/v1/local/acp/*` session routes are **not** exposed; use agent-host routes below.

### Orchestration

| Method | Path | Handler area |
| --- | --- | --- |
| GET, POST | `/v1/local/orchestration/sessions` | `orchestration::sessions` |
| GET | `/v1/local/orchestration/sessions/{session_id}` | `orchestration::sessions` |
| POST | `/v1/local/orchestration/sessions/{session_id}/signal` | `orchestration::sessions` |
| GET | `/v1/local/orchestration/capabilities` | `orchestration::capabilities` |
| GET | `/v1/local/orchestration/presets` | `orchestration::presets` |
| POST | `/v1/local/orchestration/presets/{id}:reload` | `orchestration::presets` |
| POST, GET | `/v1/local/orchestration/schedules` | `orchestration::schedules` |
| GET, DELETE | `/v1/local/orchestration/schedules/{schedule_id}` | `orchestration::schedules` |
| PATCH, GET | `/v1/local/orchestration/schedules/{schedule_id}/core-context` | `orchestration::schedules` |
| GET | `/v1/local/orchestration/schedules/{schedule_id}/core-context-history` | `orchestration::schedules` |
| POST | `/v1/local/orchestration/schedules/{schedule_id}/signal` | `orchestration::schedules` |

### Agent host (V1.20+)

| Method | Path | Handler area |
| --- | --- | --- |
| GET | `/v1/local/agent-host/health` | `handlers::agent_host::health` |
| GET | `/v1/local/agent-host/providers` | `handlers::agent_host::list_providers` |
| POST, GET | `/v1/local/agent-host/sessions` | `handlers::agent_host::create_session`, `list_sessions` |
| GET, DELETE | `/v1/local/agent-host/sessions/{session_id}` | `handlers::agent_host::get_session`, `shutdown_session` |
| POST | `/v1/local/agent-host/sessions/{session_id}/operations` | `handlers::agent_host::execute_operation` |
| POST | `/v1/local/agent-host/operations/{operation_id}:cancel` | `handlers::agent_host::cancel_operation` |
| GET | `/v1/local/agent-host/sessions/{session_id}/events` | `handlers::agent_host::session_events` |

---

## Removed surfaces (do not document as current)

| Former path | Removal | Current direction |
| --- | --- | --- |
| `/v1/local/sync/*` | V1.21+ — no `handlers::sync` in router | Cloud sync via `nexus-cloud-sync` / CLI library path; see [cloud-sync-and-local-sync-status.md](./cloud-sync-and-local-sync-status.md) |
| `/v1/local/world/*`, `/v1/local/explore/*` | V1.20 | Platform HTTP via CLI `platform` group |
| `/v1/local/acp/tool/execute`, `/v1/local/acp/sessions` | V1.20 | `/v1/local/agent-host/internal/tool-executions` + agent-host session API |
| `/v1/local/context/assemble`, `/v1/local/research/*` | Not on daemon router | Platform context assembly |

---

## Related

- Workspace-write principles (still valid): [daemon-api-workspace-write-architecture.md](./daemon-api-workspace-write-architecture.md) §2+
- Agent host implementation: [agent-host-architecture.md](./agent-host-architecture.md)
- Delivery compass route narrative: `.agents/iterations/v1.20-delivery-compass-v1.md` §3
