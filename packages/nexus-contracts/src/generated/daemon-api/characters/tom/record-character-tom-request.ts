/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for POST /v1/daemon/characters/{character_id}/tom. Closed DTO: the server admits viewer, carrier owner, selected binding, and L2 subject from stored rows before any mutation; payload claims never establish scope. holder is the epistemic subject (modules.belief[*].holder), never the MindState carrier FK. Paper aliases (actor, knowledge_access, mental_source) are not fields.
 */
export interface RecordCharacterTomRequest {
  /**
   * Selected owned active World. The viewer's binding and any L2 subject binding must be active in this World.
   */
  world_id: string;
  /**
   * The viewer Character's selected active binding in world_id.
   */
  binding_id: string;
  /**
   * Carrier KnowledgeEntry id. Must be non-deleted and canonically owned by the viewer Character or by the selected binding.
   */
  carrier_entry_id: string;
  /**
   * OCC precondition: the carrier revision observed on read (NULL normalizes to 0).
   */
  expected_revision: number;
  /**
   * Epistemic subject. L1 (order 1): must equal the viewer Character id. L2 (order 2): a different Character with its own active binding to world_id.
   */
  holder: string;
  /**
   * Minimal content being represented.
   */
  proposition: string;
  /**
   * Recursive belief depth on the Character API: 1 = L1 self-belief, 2 = L2 model of another Character. 0 and >2 reject.
   */
  order: number;
  /**
   * Truth Status (handbook closed labels).
   */
  truth?: "True" | "False" | "Unknown";
  /**
   * Knowledge Access (handbook closed labels). Exact field name is access, not knowledge_access.
   */
  access?: "Private" | "Shared" | "Public";
  /**
   * Representation (handbook closed labels).
   */
  representation?: "Explicit" | "Implicit";
  /**
   * Content Type (handbook closed labels; slash-containing labels are literal).
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
   * Mental Source (handbook closed labels). Exact field name is source, not mental_source.
   */
  source?: "Narration" | "Perception" | "Memory" | "Testimony" | "Inference" | "Imagination" | "Unknown";
  /**
   * Context (handbook closed labels).
   */
  context?: "Deceptive" | "Temporal" | "Counterfactual" | "Neutral";
  /**
   * Optional when-axis: occurrence time copied onto the derivative MindState row.
   */
  occurred_at?: string;
  /**
   * Optional when-axis: caller ordering key copied onto the derivative MindState row.
   */
  sort_key?: string;
  /**
   * Optional when-axis: grounding TimelineEvent id recorded as the derivative MindState source_anchor. Never an inference trigger.
   */
  event_id?: string;
}
