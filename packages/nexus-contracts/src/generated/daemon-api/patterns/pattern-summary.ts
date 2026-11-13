/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * One pattern package as reported by the read-only pattern registry listing. `available` and `reason` are runtime observations made while reading the package directory; they are never publication, provenance or trust signals, and a listed package is not thereby endorsed.
 */
export interface PatternSummary {
  /**
   * Package directory basename, never an absolute path.
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
   * Declared effect class, which must equal the reader's rollup of the fragment; null when unavailable. The vocabulary is the IR document's node `effect_class` set, owned by `nexus_workflow_ir::effect` (that rollup), so a vocabulary change there is a change here.
   */
  effect_class: "pure" | "read" | "write_candidate" | "external" | null;
  /**
   * Declared slots; empty when the package is unavailable.
   */
  slots: NexusPatternSlot[];
  /**
   * Declared capability ids; empty when the package is unavailable.
   */
  requires_capabilities: string[];
  /**
   * True only when the manifest parsed, every enforced field was valid, the fragment hash was computed and the declared effect class agreed with the rollup. It never means published or trusted.
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
