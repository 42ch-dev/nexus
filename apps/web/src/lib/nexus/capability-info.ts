/**
 * App-side TS mirror of the capability catalog wire DTO (V1.172 — AR-40/AR-42).
 *
 * Mirrors `crates/nexus-contracts/src/local/orchestration/http.rs`
 * `CapabilityInfo` 1:1 (camelCase on the wire). The capabilities list
 * endpoint is hand-coded local tier — NOT codegen'd — and the daemon serves
 * `inputSchema` / `outputSchema` (local DTO `#[serde(rename_all = "camelCase")]`);
 * the generated `@42ch/nexus-contracts` types are schema-facing (snake_case
 * `input_schema`/`output_schema`), not the served shape (same seam as
 * `apps/nexus42/src/api/models.rs` `CapabilityRow`).
 *
 * `origin` is always populated by the server (AR-40); the `builtin` default
 * tolerates a pre-AR-40 daemon (page checks `origin === 'user'`, so an absent
 * field renders the plain builtin row).
 */
import type { PaginationInfo } from '@42ch/nexus-contracts';

/** A single capability row on the wire (camelCase, per the local DTO). */
export interface CapabilityInfo {
  /** Dot-separated capability name, e.g. `"sync.pull"`. */
  name: string;
  /** JSON Schema (draft 2020-12) for valid inputs. */
  inputSchema: string;
  /** JSON Schema (draft 2020-12) for the output shape. */
  outputSchema: string;
  /** Provenance (AR-40): `"builtin"` (ships with the engine) or `"user"` (locally-installed developer capability). */
  origin?: 'builtin' | 'user';
}

/** Response body for `GET /v1/daemon/orchestration/capabilities` (cursor-paginated). */
export interface CapabilityListResponse {
  /** Registered capabilities with their schemas. */
  items: CapabilityInfo[];
  /** Cursor-based pagination envelope (shared generated shape). */
  pagination: PaginationInfo;
}
