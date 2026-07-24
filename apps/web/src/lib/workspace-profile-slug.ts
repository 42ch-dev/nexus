/**
 * Workspace Profile path-segment slug (V1.119 P1 — AC-P1-3 / AC-P1-4).
 *
 * The Profile **display name** is kept exactly as the author types it. These
 * helpers only sanitize the name into a single filesystem-safe **path segment**
 * (the last folder of `workspaceRoot`) and apply it to a path. Display name ↛
 * slug is one-way and applies to the path last segment only.
 *
 * Spec (normative): `.mstar/iterations/v1.119/specs/setup-workspace-profile-path.md`
 * § Architecture contract → Slug helper.
 */

/** Characters disallowed in a single path segment across POSIX + Windows. */
const ILLEGAL_SEGMENT_CHARS = /[\\/:*?"<>|]/g;
/** NUL byte — stripped separately to avoid control-char regex lint noise. */
const NUL_CHAR = '\u0000';
/** Any run of whitespace (incl. NBSP / ideographic space after NFKC). */
const WHITESPACE_RUN = /\s+/g;
/** Repeated dashes introduced by whitespace/illegal removal. */
const REPEATED_DASH = /-+/g;
/** Leading/trailing dashes left over after stripping. */
const EDGE_DASH = /^-+|-+$/g;

/**
 * Windows reserved device names. A path segment matching one of these
 * (case-insensitive exact) cannot exist as a file/folder on Windows, so we
 * append `-profile` to disambiguate (e.g. `CON` → `CON-profile`).
 */
const WINDOWS_RESERVED = new Set<string>([
  'CON', 'PRN', 'AUX', 'NUL',
  'COM1', 'COM2', 'COM3', 'COM4', 'COM5', 'COM6', 'COM7', 'COM8', 'COM9',
  'LPT1', 'LPT2', 'LPT3', 'LPT4', 'LPT5', 'LPT6', 'LPT7', 'LPT8', 'LPT9',
]);

/**
 * Collapse repeated dashes and strip leading/trailing dashes from a segment.
 * Re-applied after reserved-name suffixing to stay idempotent.
 */
function trimDashes(segment: string): string {
  return segment.replace(REPEATED_DASH, '-').replace(EDGE_DASH, '');
}

/**
 * Slugify a Profile display name into a filesystem-safe path segment.
 *
 * Pipeline (spec § Slug helper):
 * 1. Trim whitespace
 * 2. NFKC normalize (preserve CJK + Latin letters/numbers; do not romanize)
 * 3. Replace internal whitespace runs with `-`
 * 4. Remove illegal segment chars: `/ \ : * ? " < > |` and `\0`
 * 5. Collapse repeated `-`; strip leading/trailing `-`
 * 6. Windows reserved name (case-insensitive exact) → append `-profile`, re-trim
 * 7. Lone `.` / `..` → empty (would otherwise resolve to current/parent dir)
 * 8. Empty → `default`
 *
 * @example slugProfileSegment('default') // 'default'
 * @example slugProfileSegment('Alice') // 'Alice'
 * @example slugProfileSegment('我的空间') // '我的空间'
 * @example slugProfileSegment('  foo  bar  ') // 'foo-bar'
 * @example slugProfileSegment('///') // 'default'
 * @example slugProfileSegment('CON') // 'CON-profile'
 * @example slugProfileSegment('.') // 'default'
 * @example slugProfileSegment('..') // 'default'
 */
export function slugProfileSegment(displayName: string): string {
  // 1. Trim, 2. NFKC.
  let segment = displayName.trim().normalize('NFKC');

  // 3. Internal whitespace runs → single `-`.
  segment = segment.replace(WHITESPACE_RUN, '-');
  // 4. Strip illegal segment characters.
  segment = segment.replace(ILLEGAL_SEGMENT_CHARS, '').split(NUL_CHAR).join('');
  // 5. Collapse + strip dashes.
  segment = trimDashes(segment);

  // 6. Windows reserved device name → append `-profile` and re-trim dashes.
  if (segment.length > 0 && WINDOWS_RESERVED.has(segment.toUpperCase())) {
    segment = trimDashes(`${segment}-profile`);
  }

  // Reject dot-segments: a slug of '.' or '..' would make
  // `replaceLastPathSegment` resolve the workspace root to the current or
  // parent directory. Treat them as empty so they fall through to `default`.
  if (segment === '.' || segment === '..') {
    segment = '';
  }

  // 8. Empty → default.
  return segment.length === 0 ? 'default' : segment;
}

/** Index of the last path separator (`/` or `\`) in `path`, or `-1`. */
function lastSeparatorIndex(path: string): number {
  return Math.max(path.lastIndexOf('/'), path.lastIndexOf('\\'));
}

/**
 * Return the last segment of a POSIX or Windows path.
 *
 * @example lastPathSegment('~/Documents/nexus/default') // 'default'
 * @example lastPathSegment('C:\\Users\\bibi\\nexus\\default') // 'default'
 * @example lastPathSegment('default') // 'default'
 */
export function lastPathSegment(path: string): string {
  if (!path) return '';
  const sep = lastSeparatorIndex(path);
  return sep === -1 ? path : path.slice(sep + 1);
}

/**
 * Replace the last segment of a path with `lastSegment`, preserving the parent
 * path and the original separator. With no separator present, returns
 * `lastSegment` verbatim.
 *
 * @example replaceLastPathSegment('~/Documents/nexus/old', 'alice') // '~/Documents/nexus/alice'
 * @example replaceLastPathSegment('C:\\Users\\bibi\\nexus\\old', 'alice') // 'C:\\Users\\bibi\\nexus\\alice'
 */
export function replaceLastPathSegment(path: string, lastSegment: string): string {
  if (!path) return lastSegment;
  const sep = lastSeparatorIndex(path);
  if (sep === -1) return lastSegment;
  return `${path.slice(0, sep + 1)}${lastSegment}`;
}
