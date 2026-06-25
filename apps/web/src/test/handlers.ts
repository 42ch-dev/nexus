/**
 * Consolidated msw handler registry for the Local API surface.
 *
 * Per-test handlers still belong to the test that declares them (see
 * `useHandlers` in msw-server.ts). This module collects the **reusable**
 * fixtures every screen/integration test re-shapes: a healthy daemon, an empty
 * Works list, a canonical error envelope. P2 screen tests compose these instead
 * of re-typing the daemon contract shapes inline, which keeps the wire surface
 * in one place (R-V164-QC1-S1-P1 baseline).
 *
 * All handlers match the same-origin relative paths the BrowserClient emits
 * (default `baseUrl: ''`), so they work unchanged against msw's `http.get`/
 * `http.post` path matchers.
 */
import type { ErrorResponse, PaginationInfo } from '@42ch/nexus-contracts';
import { http, HttpResponse, type RequestHandler } from 'msw';

/** Shared cursor-pagination block the daemon emits on every list endpoint. */
export function pagination(over: Partial<PaginationInfo> = {}): PaginationInfo {
  return { limit: 20, has_more: false, ...over };
}

/**
 * Canonical daemon error envelope (F-E1 / `ApiErrorResponse`):
 * `{ success: false, error: ErrorResponse }`. Screens parse this via
 * `NexusClientError.fromBody`, so every error fixture must round-trip through
 * this shape rather than a bare `{ code, message }`.
 */
export function errorEnvelope(status: number, error: ErrorResponse) {
  return HttpResponse.json({ success: false, error }, { status });
}

// ── Daemon health ────────────────────────────────────────────────────────────

/** `GET /v1/local/runtime/health` → 200 `{ status, version }`. */
export function healthOk(version = '0.1.0'): RequestHandler {
  return http.get('/v1/local/runtime/health', () =>
    HttpResponse.json({ status: 'ok', version }),
  );
}

// ── Works ────────────────────────────────────────────────────────────────────

/** `GET /v1/local/works` → 200 `{ works, pagination }` (F-P3 `works` key). */
export function worksList(
  rows: unknown[],
  over: Partial<PaginationInfo> = {},
): RequestHandler {
  return http.get('/v1/local/works', () =>
    HttpResponse.json({ works: rows, pagination: pagination(over) }),
  );
}

/**
 * `GET /v1/local/works/:workId` — echoes the captured id back as `work_id` so
 * detail-screen tests can assert the path param threaded correctly.
 */
export function workDetail(workId: string, over: Record<string, unknown> = {}): RequestHandler {
  return http.get('/v1/local/works/:workId', ({ params }) =>
    HttpResponse.json({ work_id: params.workId ?? workId, title: 'Work', ...over }),
  );
}

/** `POST /v1/local/works` → 201 with the created Work summary. */
export function createWorkCreated(): RequestHandler {
  return http.post('/v1/local/works', async ({ request }) => {
    const body = (await request.json().catch(() => ({}))) as { title?: string };
    // CreateWorkResponse is `{ work_id, status }`; `title` is echoed only for
    // the test's benefit via a separate field if needed by callers.
    return HttpResponse.json(
      { work_id: 'w-new', status: 'draft', _title: body.title ?? 'New Work' },
      { status: 201 },
    );
  });
}

// ── Chapters ─────────────────────────────────────────────────────────────────

/** Canonical chapter summary fixture builders for P2 screen tests. */
export function chapterSummary(
  chapter: number,
  over: Record<string, unknown> = {},
): Record<string, unknown> {
  return {
    work_id: 'w-123',
    chapter,
    volume: 1,
    title: null,
    slug: `ch${String(chapter).padStart(2, '0')}`,
    planned_word_count: 4000,
    actual_word_count: null,
    status: 'not_started',
    outline_path: `Works/WRK/Outlines/chapters/ch${String(chapter).padStart(2, '0')}-outline.md`,
    body_path: `Works/WRK/Stories/ch${String(chapter).padStart(2, '0')}-ch${String(chapter).padStart(2, '0')}.md`,
    created_at: '2026-06-25T00:00:00Z',
    updated_at: '2026-06-25T00:00:00Z',
    ...over,
  };
}

/** `GET /v1/local/works/:workId/chapters` → 200 `{ items, pagination }`. */
export function chaptersList(
  rows: Record<string, unknown>[],
  over: Partial<PaginationInfo> = {},
): RequestHandler {
  return http.get('/v1/local/works/:workId/chapters', () =>
    HttpResponse.json({ items: rows, pagination: pagination(over) }),
  );
}

/** `GET /v1/local/works/:workId/chapters/:n` → 200 `ChapterDetail`. */
export function chapterDetail(
  _chapter: number,
  over: Record<string, unknown> = {},
): RequestHandler {
  return http.get('/v1/local/works/:workId/chapters/:n', ({ params }) => {
    const n = Number(params.n);
    return HttpResponse.json({
      ...chapterSummary(n),
      can_edit_outline: true,
      can_edit_structure: true,
      body_read_only: true,
      protection: { level: 'none', reason: '' },
      ...over,
    });
  });
}

/** `GET /v1/local/works/:workId/chapters/:n/outline` → 200 `ChapterOutline`. */
export function chapterOutline(
  chapter: number,
  content: string,
  over: Record<string, unknown> = {},
): RequestHandler {
  return http.get('/v1/local/works/:workId/chapters/:n/outline', ({ params }) =>
    HttpResponse.json({
      work_id: 'w-123',
      chapter: Number(params.n),
      volume: 1,
      outline_path: `Works/WRK/Outlines/chapters/ch${String(chapter).padStart(2, '0')}-outline.md`,
      content,
      updated_at: '2026-06-25T00:00:00Z',
      ...over,
    }),
  );
}

/** `PUT /v1/local/works/:workId/chapters/:n/outline` → 200 `ChapterOutline`. */
export function chapterOutlineUpdated(): RequestHandler {
  return http.put('/v1/local/works/:workId/chapters/:n/outline', async ({ params, request }) => {
    const body = (await request.json().catch(() => ({}))) as { content?: string };
    return HttpResponse.json({
      work_id: params.workId ?? 'w-123',
      chapter: Number(params.n),
      volume: 1,
      outline_path: `Works/WRK/Outlines/chapters/ch${String(params.n).padStart(2, '0')}-outline.md`,
      content: body.content ?? '',
      updated_at: '2026-06-25T00:00:00Z',
    });
  });
}

/** `PATCH /v1/local/works/:workId/chapters/:n` → 200 `ChapterDetail`. */
export function chapterPatched(): RequestHandler {
  return http.patch('/v1/local/works/:workId/chapters/:n', async ({ params, request }) => {
    const body = (await request.json().catch(() => ({}))) as Record<string, unknown>;
    return HttpResponse.json({
      ...chapterSummary(Number(params.n)),
      ...body,
      can_edit_outline: true,
      can_edit_structure: true,
      body_read_only: true,
      protection: { level: 'none', reason: '' },
    });
  });
}

/** `GET /v1/local/works/:workId/chapters/:n/body` → 200 `ChapterBody`. */
export function chapterBody(
  chapter: number,
  content: string,
  over: Record<string, unknown> = {},
): RequestHandler {
  return http.get('/v1/local/works/:workId/chapters/:n/body', ({ params }) =>
    HttpResponse.json({
      work_id: 'w-123',
      chapter: Number(params.n),
      volume: 1,
      body_path: `Works/WRK/Stories/ch${String(chapter).padStart(2, '0')}-ch${String(chapter).padStart(2, '0')}.md`,
      content,
      frontmatter: { status: 'draft' },
      read_only: true,
      updated_at: '2026-06-25T00:00:00Z',
      ...over,
    }),
  );
}
