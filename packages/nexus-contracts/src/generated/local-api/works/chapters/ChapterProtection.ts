import type { SchemaVersion } from '../../../common/CommonTypes';
/**
 * Nexus ChapterProtection
 *
 * Protection level describing what UI actions are allowed for a chapter (V1.65 P0).
 *
 * @schema_version 1
 * @source chapter-protection.schema.json
 */

/** Inline enum type */
export type ChapterProtectionLevel = 'none' | 'confirm_structure_edit' | 'hard_block_delete';

/** Protection level describing what UI actions are allowed for a chapter (V1.65 P0). */
export interface ChapterProtection {
  level: ChapterProtectionLevel;
  reason: string;
}
