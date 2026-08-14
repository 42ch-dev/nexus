/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Request body for POST /v1/daemon/worlds/{world_id}/kb/patch-entity (V1.73). Edits an entity (KnowledgeEntry) title/body/aliases/block_type with per-row OCC on kb_key_blocks.revision.
 */
export interface WorldKbPatchEntityRequest {
  /**
   * KnowledgeEntry id from the URL world scope. Authoritative identifier.
   */
  entity_id: string;
  /**
   * Per-row version observed by the client on the last canonical read (kb_key_blocks.revision, NULL normalized to 0).
   */
  expected_version: number;
  patch: NexusWorldKbEntityPatch;
}
/**
 * Fields to update. At least one property must be provided.
 */
export interface NexusWorldKbEntityPatch {
  /**
   * New canonical_name (display title).
   */
  title?: string;
  /**
   * Replacement KnowledgeEntry body JSON (summary/attributes/tags/state/computable).
   */
  body?: {
    [k: string]: unknown | undefined;
  };
  /**
   * Replacement alias list.
   */
  aliases?: string[];
  /**
   * Re-classify the entity. Must be a valid BlockType (entity-scope-model §5.1.1).
   */
  block_type?:
    | "character"
    | "ability"
    | "scene"
    | "organization"
    | "item"
    | "conflict"
    | "info_point"
    | "event"
    | "species"
    | "faction"
    | "magic_system"
    | "technology"
    | "deity"
    | "level"
    | "economy_tier"
    | "dialogue"
    | "beat"
    | "act"
    | "era";
  /**
   * Per-entry functional-dialect modules (modules.mental, modules.belief, modules.observation, etc.) merged into the entry's stored modules_json. Mirrors the spoke ModuleMap shape: keys are functional-dialect ids matching ^[a-z][a-z0-9_-]*$, values are objects or arrays. First-level key upsert (AR-4/PD-12): provided keys replace the whole first-level value; unspecified sibling keys are preserved; `{}` is a no-op; omitted inherits the stored modules.
   */
  modules?: {
    [k: string]:
      | (
          | {
              [k: string]: unknown | undefined;
            }
          | unknown[]
        )
      | undefined;
  };
}
