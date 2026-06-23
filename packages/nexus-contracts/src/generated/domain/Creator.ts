import type { CreatorStatus, RegistrationSource, SchemaVersion } from '../common/CommonTypes';
/**
 * Nexus Creator Entity
 *
 * Creator entity - a first-class creative agent that can be user-owned or agent-registered. Aligned with data-model-v1.md §5.2.
 *
 * @schema_version 1
 * @source creator.schema.json
 */
/** Creator entity - a first-class creative agent that can be user-owned or agent-registered. Aligned with data-model-v1.md §5.2. */
export interface Creator {
  schema_version: number;
  creator_id: string;
  user_id?: string;
  display_name: string;
  status: CreatorStatus;
  is_platform_owned?: boolean;
  api_key_ref?: string;
  registration_source: RegistrationSource;
  persona_summary?: string;
  style_profile?: { tone?: string[]; narrative_preferences?: string[]; forbidden_patterns?: string[] };
  experience_revision?: number;
  created_at: string;
  updated_at?: string;
}
