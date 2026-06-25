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
