import type { PublishStoryOutcome, SchemaVersion } from './CommonTypes';
/**
 * Nexus PublishHistoryEntry
 *
 * Single publish history row (platform Publish API).
 *
 * @schema_version 1
 * @source publish-history-entry.schema.json
 */
/** Single publish history row (platform Publish API). */
export interface PublishHistoryEntry {
  occurred_at: string;
  outcome: PublishStoryOutcome;
  story_manifest_id?: string;
  published_artifact_id?: string;
  message?: string;
}
