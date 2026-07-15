/**
 * Work-shell route detection — shared by RootLayout main wrapper and tests.
 *
 * `/works/:workId/*` uses the canvas-first shell (full-width main). The sibling
 * list route `/works/chapters` is reserved and must keep the standard 1200px
 * main column.
 *
 * **I1 — workId `chapters` collision (V1.118 P2 T4):** The reserved segment
 * `chapters` excludes every `/works/chapters…` path from work-shell layout
 * detection — including `/works/chapters/outline` that would belong to a Work
 * whose `work_id` is literally `chapters`. React Router also matches the
 * static `works/chapters` list route before `works/:workId` for the bare path.
 * Product mitigation is deferred: disallow reserved ids at create time or
 * relocate the global chapters list path.
 */

/** First path segment under `/works/*` that is not a work id. */
const WORKS_RESERVED_SEGMENTS = new Set(['chapters']);

/** True when the pathname is a canvas-first work shell (`/works/:workId/*`). */
export function isWorkShellRoute(pathname: string): boolean {
  const match = pathname.match(/^\/works\/([^/]+)(?:\/|$)/);
  if (!match) return false;
  return !WORKS_RESERVED_SEGMENTS.has(match[1]);
}
