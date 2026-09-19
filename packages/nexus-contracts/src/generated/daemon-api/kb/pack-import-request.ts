/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * POST /v1/daemon/worlds/:world_id/kb/pack/import — import a Narrative Knowledge Pack into a World (V1.152 P0 DF-77), or review one import batch's quarantined atoms (v1.191 P1 T10). Two mutually exclusive arms: the IMPORT arm requires `pack` + `conflict` and may carry `holder_map`/`include_anchors`; the REVIEW arm carries `review_import` alone (a read-only, bounded, owner-only review of one batch). Sending `review_import` together with `pack`, `holder_map` or `include_anchors` is refused as invalid input.
 */
export interface PackImportRequest {
  /**
   * Opaque Narrative Knowledge Pack (spoke handbook shape: modules.pack + entries + relations + optional source_anchors). Parsed server-side via nexus_spoke_adapter::pack::parse_pack. Required on the import arm; absent on the review arm.
   */
  pack?: {
    [k: string]: unknown | undefined;
  };
  /**
   * Collision policy: skip (keep existing, default), rename (disambiguate and create), overwrite (replace one entry body, behind confirmation). Required semantic state on the import arm; absent on the review arm.
   */
  conflict?: "skip" | "rename" | "overwrite";
  /**
   * Whether to import source_anchors from the pack. Accepted on the wire; no-op until anchor persistence ships.
   */
  include_anchors?: boolean;
  /**
   * Explicit foreign→local holder adoptions for this import (v1.191 P1 T10, holder-governance.md §6). Each mapping validates through authoring admission; an inadmissible mapping refuses the whole import with no atom written. Without a mapping for a foreign-governed atom, that atom is quarantined — a foreign id is never adopted by string equality, and unknown disclosure stays quarantined even when its owner is mapped.
   */
  holder_map?: HolderMapping[];
  /**
   * Import batch id to review. Selects the read-only review arm: it returns the batch's bounded quarantined atoms (immutable original KE JSON, original owner/disclosure, reason) authorized to the stored controlling Creator that ran the batch, and never a model-facing knowledge view. Mutually exclusive with pack/ST input and holder mappings.
   */
  review_import?: string;
}
/**
 * This interface was referenced by `PackImportRequest`'s JSON-Schema
 * via the `definition` "holder_mapping".
 */
export interface HolderMapping {
  /**
   * The foreign holder id exactly as the pack carries it. Matched by exact key only; equality with a local hld_ digest adopts nothing.
   */
  foreign_id: string;
  /**
   * The permitted local identity the atom is adopted into: "author-only" (the admitted controlling Creator) or "character-private:<character_id>" (that owned, active Character, which must hold an active binding to the owned World).
   */
  selector: string;
}
