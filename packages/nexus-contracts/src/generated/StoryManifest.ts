import type { ManifestType, ManuscriptStorage, SchemaVersion, StoryManifestStatus } from './CommonTypes';
/**
 * Nexus StoryManifest
 *
 * StoryManifest entity for platform-side chapter/arc manifest and summary. Aligned with data-model-v1.md §5.9.
 *
 * @schema_version 1
 * @source story-manifest.schema.json
 */
/** StoryManifest entity for platform-side chapter/arc manifest and summary. Aligned with data-model-v1.md §5.9. */
export interface StoryManifest {
  schema_version: number;
  story_manifest_id: string;
  world_id: string;
  creator_id: string;
  manifest_type: ManifestType;
  status: StoryManifestStatus;
  title: string;
  summary_unit_id: string;
  summary_text?: string;
  output_manuscript?: boolean;
  manuscript_storage?: ManuscriptStorage;
  local_path?: string;
  sandbox_path?: string | null;
  content_hash?: string | null;
  published_artifact_id?: string | null;
  created_at: string;
  updated_at?: string;
}
