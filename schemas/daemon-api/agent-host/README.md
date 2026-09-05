# Daemon API — Agent Host schemas

JSON Schemas for Agent Host session and operation contracts under `/v1/daemon/agent-host`.

## Endpoints

| Endpoint | Schema files |
|----------|-------------|
| `POST /v1/daemon/agent-host/scan` | `scan-request.schema.json`, `scan-response.schema.json`, `agent-scan-entry.schema.json` |
| `POST /v1/daemon/agent-host/sessions` | `create-session-request.schema.json`, `session-response.schema.json` |
| `GET /v1/daemon/agent-host/sessions` | `agent-host-list-sessions-query.schema.json`, `session-list-response.schema.json` |
| `GET /v1/daemon/agent-host/sessions/{session_id}` | `session-response.schema.json` |
| `DELETE /v1/daemon/agent-host/sessions/{session_id}` | `shutdown-session-response.schema.json` |
| `POST /v1/daemon/agent-host/sessions/{session_id}/operations` | `execute-operation-request.schema.json`, `operation-response.schema.json` |
| `POST /v1/daemon/agent-host/operations/{operation_id}:cancel` | `cancel-operation-response.schema.json` |

Paired Actor fields: `session-viewpoint.schema.json` plus domain `actor-ref.schema.json`. Both absent is the legacy path; both present is Actor mode.
