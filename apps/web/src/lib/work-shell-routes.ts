/**
 * Work-shell route detection — shared by RootLayout main wrapper and tests.
 *
 * `/works/:workId/*` uses the canvas-first shell (full-width main). The sibling
 * list route `/works/chapters` is reserved and must keep the standard 1200px
 * main column.
 */

/** First path segment under `/works/*` that is not a work id. */
const WORKS_RESERVED_SEGMENTS = new Set(['chapters']);

/** True when the pathname is a canvas-first work shell (`/works/:workId/*`). */
export function isWorkShellRoute(pathname: string): boolean {
  const match = pathname.match(/^\/works\/([^/]+)(?:\/|$)/);
  if (!match) return false;
  return !WORKS_RESERVED_SEGMENTS.has(match[1]);
}
