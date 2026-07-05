/**
 * Reading-chrome profile mapping.
 *
 * The backend `Work.work_profile` stores `game_bible` with an underscore, but
 * the V1.91 DESIGN.md reading-chrome tokens use hyphenated profile keys. This
 * module provides the canonical chrome-profile union and the wire-to-token
 * mapping so components consume DESIGN.md token names only.
 */

import { isWorkProfile } from '@/lib/work-profiles';

/**
 * Chrome profile keys — match the hyphenated suffixes in DESIGN.md
 * `reading-chrome-<profile>` token names.
 */
export type ReadingChromeProfile = 'novel' | 'essay' | 'game-bible' | 'script';

/** Ordered list of chrome profiles for iteration/display. */
export const READING_CHROME_PROFILES: readonly ReadingChromeProfile[] = [
  'novel',
  'essay',
  'game-bible',
  'script',
];

/**
 * Map a backend `work_profile` value (or any string) to a chrome profile.
 * Unknown values fall back to `novel`, matching the V1.91 acceptance bar.
 */
export function toReadingChromeProfile(
  value: string | undefined | null,
): ReadingChromeProfile {
  if (!value) return 'novel';
  if (value === 'game_bible') return 'game-bible';
  if (isWorkProfile(value)) return value as ReadingChromeProfile;
  return 'novel';
}

/**
 * Type guard: narrows an arbitrary string to a {@link ReadingChromeProfile}.
 */
export function isReadingChromeProfile(
  value: string,
): value is ReadingChromeProfile {
  return (READING_CHROME_PROFILES as readonly string[]).includes(value);
}
