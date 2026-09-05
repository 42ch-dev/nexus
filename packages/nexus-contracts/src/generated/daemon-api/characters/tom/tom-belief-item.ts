/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * One Character ToM belief row read from an authorized carrier's authoritative modules.belief array. Keyset order is (order, carrier_entry_id, row_ordinal): L1 (order 1) sorts before L2 (order 2). row_ordinal is the physical zero-based array index; malformed legacy elements are skipped without renumbering.
 */
export interface TomBeliefItem {
  /**
   * Carrier KnowledgeEntry id holding this row.
   */
  carrier_entry_id: string;
  /**
   * Physical zero-based position of the row inside the carrier's modules.belief array.
   */
  row_ordinal: number;
  /**
   * Epistemic subject (chr_*). L1 rows name the viewer; L2 rows name another Character.
   */
  holder: string;
  /**
   * Minimal content being represented.
   */
  proposition?: string;
  /**
   * Belief depth: 1 = L1, 2 = L2.
   */
  order: number;
  /**
   * Truth Status (handbook closed label).
   */
  truth?: "True" | "False" | "Unknown";
  /**
   * Knowledge Access (handbook closed label).
   */
  access?: "Private" | "Shared" | "Public";
  /**
   * Representation (handbook closed label).
   */
  representation?: "Explicit" | "Implicit";
  /**
   * Content Type (handbook closed label; slash-containing labels are literal).
   */
  content_type?:
    | "Location"
    | "Contents/Physical State"
    | "Identity/Relation"
    | "Epistemic"
    | "Desire/Intention"
    | "Emotion"
    | "Trait/Value"
    | "Action/Event";
  /**
   * Mental Source (handbook closed label).
   */
  source?: "Narration" | "Perception" | "Memory" | "Testimony" | "Inference" | "Imagination" | "Unknown";
  /**
   * Context (handbook closed label).
   */
  context?: "Deceptive" | "Temporal" | "Counterfactual" | "Neutral";
  /**
   * Latest derivative MindState timestamp recorded against this carrier, when any. MindState is never treated as a second authority.
   */
  carrier_recorded_at?: string;
}
