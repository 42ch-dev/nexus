/**
 * Reading API shim — V1.89 Deeper Manuscript Reading (BL-11 MVP slice).
 *
 * P0 owns promoting these methods onto `NexusClient`
 * (`src/lib/nexus/types.ts` + `src/lib/nexus/browser-client.ts`). Until that
 * lands, this temporary shim exposes the same async signatures so the reading
 * UI can typecheck and test against MSW-mocked Local API routes. The shim uses
 * same-origin `fetch`, matching the `BrowserClient` transport conventions.
 *
 * TODO(P0-merge): replace imports from this file with `NexusClient` methods
 * (`client.getReadingProgress`, `client.saveReadingProgress`, etc.).
 */

export const ANNOTATION_COLORS = ['yellow', 'blue', 'green', 'pink'] as const;
export type AnnotationColor = (typeof ANNOTATION_COLORS)[number];

export interface ReadingProgress {
  work_id: string;
  chapter: number;
  scroll_progress: number;
  updated_at: string;
}

export interface ReadingAnnotation {
  annotation_id: string;
  work_id: string;
  chapter: number;
  start_offset: number;
  end_offset: number;
  selected_text: string;
  color: AnnotationColor;
  note?: string;
  created_at: string;
  updated_at: string;
}

export interface CreateAnnotationRequest {
  work_id: string;
  chapter: number;
  start_offset: number;
  end_offset: number;
  selected_text: string;
  color: AnnotationColor;
  note?: string;
}

export interface PatchAnnotationRequest {
  color?: AnnotationColor;
  note?: string;
}

function toQueryString(params: Record<string, string | number>): string {
  const qs = new URLSearchParams();
  for (const [key, value] of Object.entries(params)) {
    qs.set(key, String(value));
  }
  const s = qs.toString();
  return s ? `?${s}` : '';
}

async function expectOk(response: Response): Promise<void> {
  if (!response.ok) {
    let body: unknown = null;
    try {
      body = await response.json();
    } catch {
      // ignore non-JSON error bodies
    }
    const message =
      typeof body === 'object' &&
      body !== null &&
      'error' in body &&
      typeof (body as { error?: { message?: string } }).error?.message === 'string'
        ? (body as { error: { message: string } }).error.message
        : `HTTP ${response.status}`;
    throw new Error(message);
  }
}

export async function getReadingProgress(workId: string, chapter: number): Promise<ReadingProgress> {
  const response = await fetch(
    `/v1/local/reading/progress${toQueryString({ work_id: workId, chapter })}`,
  );
  await expectOk(response);
  return (await response.json()) as ReadingProgress;
}

export async function saveReadingProgress(
  workId: string,
  chapter: number,
  scrollProgress: number,
): Promise<ReadingProgress> {
  const response = await fetch('/v1/local/reading/progress', {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ work_id: workId, chapter, scroll_progress: scrollProgress }),
  });
  await expectOk(response);
  return (await response.json()) as ReadingProgress;
}

export async function listAnnotations(workId: string, chapter: number): Promise<ReadingAnnotation[]> {
  const response = await fetch(
    `/v1/local/reading/annotations${toQueryString({ work_id: workId, chapter })}`,
  );
  await expectOk(response);
  const data = (await response.json()) as { items: ReadingAnnotation[] };
  return data.items;
}

export async function createAnnotation(request: CreateAnnotationRequest): Promise<ReadingAnnotation> {
  const response = await fetch('/v1/local/reading/annotations', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(request),
  });
  await expectOk(response);
  return (await response.json()) as ReadingAnnotation;
}

export async function updateAnnotation(
  annotationId: string,
  patch: PatchAnnotationRequest,
): Promise<ReadingAnnotation> {
  const response = await fetch(`/v1/local/reading/annotations/${encodeURIComponent(annotationId)}`, {
    method: 'PATCH',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(patch),
  });
  await expectOk(response);
  return (await response.json()) as ReadingAnnotation;
}

export async function deleteAnnotation(annotationId: string): Promise<void> {
  const response = await fetch(`/v1/local/reading/annotations/${encodeURIComponent(annotationId)}`, {
    method: 'DELETE',
  });
  await expectOk(response);
}
