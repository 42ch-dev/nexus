/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Response body for POST /v1/daemon/characters/{character_id}/soul/reflect. Reports the Character SOUL narrative cache state, stale metadata, current scope counts, and insufficient-data thresholds. Mirrors the Creator SoulNarrativeResponse with `character_id` in place of `creator_id`.
 */
export interface CharacterSoulNarrativeResponse {
  /**
   * Character ID (lowercase prefix chr_ and exactly 32 hex characters)
   */
  character_id: string;
  /**
   * Narrative cache state. `insufficient_data` means the endpoint did not invoke synthesis because one or more minimum thresholds is not met.
   */
  state: "ungenerated" | "current" | "stale" | "insufficient_data";
  /**
   * Cached reflective narrative. Present only for `current` or `stale` rows; omitted for `ungenerated` and `insufficient_data`.
   */
  narrative?: string;
  /**
   * RFC 3339 generation timestamp. Present only when a cached narrative exists.
   */
  generated_at?: string;
  /**
   * True only when a cached narrative exists and current fragment stats differ from the generation snapshot.
   */
  stale: boolean;
  /**
   * Fragment-count snapshot persisted with the cached narrative. Present only when a cached narrative exists.
   */
  fragment_count_at_generation?: number;
  /**
   * RFC 3339 timestamp snapshot of the newest fragment included in generation. Present only when a cached narrative exists and at least one fragment existed at generation time.
   */
  max_fragment_created_at_at_generation?: string;
  /**
   * Current fragment count in the requested Character scope used for stale and insufficient-data decisions.
   */
  current_fragment_count: number;
  /**
   * Current distinct keyword count across the requested Character scope.
   */
  current_distinct_keyword_count: number;
  /**
   * Minimum fragment count required before synthesis is allowed.
   */
  min_fragment_count: 10;
  /**
   * Minimum distinct keyword count required before synthesis is allowed.
   */
  min_distinct_keyword_count: 20;
}
