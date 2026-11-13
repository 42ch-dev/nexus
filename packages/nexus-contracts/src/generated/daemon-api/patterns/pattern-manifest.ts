/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Declarative manifest of one pattern package directory (`pattern.toml`). A manifest is data, never authority: it records what the package declares. Readability, IR compatibility, effect-rollup agreement, trust and loadability are runtime facts owned by the registry reader, and none of them is claimed here.
 */
export interface PatternManifest {
  /**
   * Package id. It MUST equal the package directory name; a mismatch is an unavailable package, never an alias.
   */
  id: string;
  /**
   * SemVer 2.0.0 version label. This pattern is the single authority over which labels are accepted: the registry reader does not re-parse it, so a value the pattern admits is a valid label even when its numeric identifiers exceed a machine integer, exactly as SemVer 2.0.0 specifies. It is a human version selector, never the content identity: the pin is the canonical content hash of the package's IR fragment, so two packages may declare the same version with different bytes.
   */
  version: string;
  ir_compat: PatternIrCompat;
  /**
   * Non-empty human label shown wherever the package is listed.
   */
  description: string;
  /**
   * Worst-case effect this package declares for its fragment. The vocabulary is the IR document's node `effect_class` set, owned by `nexus_workflow_ir::effect` (the rollup the reader compares against), so a vocabulary change there is a change here. The reader compares the declaration with its own rollup of the fragment; a disagreement makes the package unavailable and neither value is rewritten.
   */
  effect_class: "pure" | "read" | "write_candidate" | "external";
  /**
   * Named slot declarations, possibly empty. Slot names are unique within the manifest.
   */
  slots: NexusPatternSlot[];
  /**
   * Non-empty unique capability ids the package requires; instantiation unions them into the parent. Capability presence is checked by shared semantic validation, not by this manifest.
   */
  requires_capabilities: string[];
}
/**
 * Inclusive range of workflow IR versions this package supports. A package whose range excludes the daemon's current `ir_version` is unavailable.
 */
export interface PatternIrCompat {
  /**
   * Lowest supported IR version (inclusive).
   */
  min: number;
  /**
   * Highest supported IR version (inclusive). `min > max` is an invalid range and makes the package unavailable.
   */
  max: number;
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
