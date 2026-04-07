import type { SchemaVersion } from './CommonTypes';

/**
 * Nexus Creator Entity
 *
 * Creator entity - a first-class creative agent that can be user-owned or agent-registered. Aligned with data-model-v1.md §5.2.
 *
 * @schema_version 1
 * @source creator.schema.json
 */

/** Inline enum type */
export type CreatorStatus = 'active' | 'archived' | 'locked';

/** Inline enum type */
export type CreatorRegistrationSource = 'cli' | 'web_agent' | 'platform';

/** Creator entity - a first-class creative agent that can be user-owned or agent-registered. Aligned with data-model-v1.md §5.2. */
export interface Creator {
  schema_version: number;
  creator_id: string;
  user_id?: string;
  display_name: string;
  status: CreatorStatus;
  is_platform_owned?: boolean;
  api_key_ref?: string;
  registration_source: CreatorRegistrationSource;
  persona_summary?: string;
  style_profile?: { tone?: string[]; narrative_preferences?: string[]; forbidden_patterns?: string[] };
  experience_revision?: number;
  created_at: string;
  updated_at?: string;
}
