/**
 * AUTO-GENERATED FROM JSON SCHEMA — DO NOT MODIFY MANUALLY
 * Source: schemas/ (JSON Schema wire contracts)
 * Generator: json-schema-to-typescript (tooling/codegen/src/ts-gen.ts)
 */

/**
 * Creator entity - a first-class creative agent that can be user-owned or agent-registered. Aligned with data-model-v1.md §5.2.
 */
export interface Creator {
  /**
   * Schema version as integer (e.g., 1)
   */
  schema_version: number;
  /**
   * Unique creator identifier
   */
  creator_id: string;
  /**
   * Default paired User ID (null if unpaired)
   */
  user_id?: string;
  /**
   * Creator display name
   */
  display_name: string;
  /**
   * Creator status
   */
  status: "active" | "archived" | "locked";
  /**
   * Whether this is a platform-hosted creator
   */
  is_platform_owned?: boolean;
  /**
   * Reference to platform-stored ACP/agent credential
   */
  api_key_ref?: string;
  /**
   * How this creator was registered
   */
  registration_source: "cli" | "web_agent" | "platform";
  /**
   * Optional creator persona summary
   */
  persona_summary?: string;
  /**
   * Optional style profile
   */
  style_profile?: {
    /**
     * Style tone tags
     */
    tone?: string[];
    /**
     * Narrative preference tags
     */
    narrative_preferences?: string[];
    /**
     * Forbidden pattern tags
     */
    forbidden_patterns?: string[];
    [k: string]: unknown | undefined;
  };
  /**
   * Current experience revision (0 = template-only)
   */
  experience_revision?: number;
  /**
   * Creator registration timestamp
   */
  created_at: string;
  /**
   * Last update timestamp
   */
  updated_at?: string;
}
