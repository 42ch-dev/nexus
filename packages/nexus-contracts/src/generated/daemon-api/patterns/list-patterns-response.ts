/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response for GET /v1/daemon/patterns (cursor-based pagination). Every scanned package directory appears as its own item; an unavailable package carries its reason and never suppresses the valid ones.
 */
export interface ListPatternsResponse {
  items: NexusPatternSummary[];
  pagination: NexusPaginationInfo;
}
/**
 * One pattern package as reported by the read-only pattern registry listing. `available` and `reason` are runtime observations made while reading the package directory; they are never publication, provenance or trust signals, and a listed package is not thereby endorsed.
 */
export interface NexusPatternSummary {
  /**
   * Cursor identity of the package directory: its basename, never an absolute path, kept as valid UTF-8 with `%` and every byte outside valid UTF-8 percent-escaped, so two distinct directories never share one identity. The listing is ordered by this value compared as UTF-8 bytes, and `pagination.next_cursor` carries it.
   */
  package_dir: string;
  /**
   * Declared package id; null when the manifest is unreadable or invalid.
   */
  id: string | null;
  /**
   * Declared version label; null when the manifest is unreadable or invalid.
   */
  version: string | null;
  /**
   * Declared human label; null when the manifest is unreadable or invalid.
   */
  description: string | null;
  /**
   * Canonical content hash of the package's IR fragment; null when it could not be computed.
   */
  content_hash: string | null;
  /**
   * Declared effect class, which must equal the reader's rollup of the fragment. The vocabulary is the IR document's node `effect_class` set, owned by `nexus_workflow_ir::effect` (that rollup), so a vocabulary change there is a change here. Null only when the manifest itself was unreadable or invalid: a package whose manifest parsed keeps its declared class even when the package is unavailable for another reason.
   */
  effect_class: "pure" | "read" | "write_candidate" | "external" | null;
  /**
   * Declared slots. Empty only when the manifest itself was unreadable or invalid; a package whose manifest parsed keeps its declarations even when the package is unavailable for another reason.
   */
  slots: NexusPatternSlot[];
  /**
   * Declared capability ids. Empty only when the manifest itself was unreadable or invalid; a package whose manifest parsed keeps its declarations even when the package is unavailable for another reason.
   */
  requires_capabilities: string[];
  /**
   * True only when the manifest parsed, every enforced field was valid, the member files were regular and within the reader's size ceiling, the fragment hash was computed and the declared effect class agreed with the rollup. It never means published or trusted. A false item still reports whatever its manifest declared when it parsed; only an unreadable or invalid manifest leaves those fields null or empty.
   */
  available: boolean;
  /**
   * Null when available; otherwise an actionable reason naming what made the package unavailable.
   */
  reason: string | null;
}
/**
 * One named slot a pattern package declares for compose-time binding. The declaration is data: binding a slot, and checking that every required slot is bound, are runtime compose steps.
 */
export interface NexusPatternSlot {
  /**
   * Slot name. Non-empty; unique within one package manifest.
   */
  name: string;
  /**
   * What a binding supplies for this slot: a string value, an asset id, or a capability id.
   */
  kind: "string" | "asset" | "capability";
  /**
   * Whether compose must bind this slot before the package can be used.
   */
  required: boolean;
}
/**
 * Cursor-based pagination metadata.
 */
export interface NexusPaginationInfo {
  limit: number;
  /**
   * Opaque cursor returned by the previous page. Clients MUST NOT parse it. Non-null only when another page exists.
   */
  next_cursor?: string;
  /**
   * True when the client may request another page (equivalent to `next_cursor` being non-null).
   */
  has_more: boolean;
}
