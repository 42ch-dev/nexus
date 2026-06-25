/**
 * Tests for NexusClientError — the W-1 error-envelope parsing contract.
 *
 * The daemon wraps the canonical ErrorResponse under `{ success: false, error:
 * { code, message, details? } }` (ApiErrorResponse). `fromBody` must unwrap the
 * inner `error` object so the UI toast layer sees the stable `code` + actionable
 * `message` instead of a generic `http_<status>` fallback. This is the W-1 fix
 * (R-V164-QC1-S1 §3.2) and the foundation of the UI error surface.
 */
import { describe, expect, it } from 'vitest';

import { NexusClientError } from './errors';

describe('NexusClientError.fromBody', () => {
  it('unwraps the daemon envelope { success, error: { code, message } }', () => {
    const body = {
      success: false,
      error: { code: 'validation_failed', message: 'Title is required.' },
    };
    const error = NexusClientError.fromBody(400, body);
    expect(error.status).toBe(400);
    expect(error.code).toBe('validation_failed');
    expect(error.message).toBe('Title is required.');
  });

  it('preserves optional details from the inner error', () => {
    const body = {
      success: false,
      error: {
        code: 'validation_failed',
        message: 'Preset validation failed.',
        details: { path: '/presets/foo.yaml', line: 3 },
      },
    };
    const error = NexusClientError.fromBody(422, body);
    expect(error.details).toEqual({ path: '/presets/foo.yaml', line: 3 });
  });

  it('falls back to top-level code/message when no envelope is present', () => {
    // Some orchestration handlers still emit ad-hoc bodies (R-V164-FE1-ORCH).
    const body = { code: 'not_found', message: 'Work not found.' };
    const error = NexusClientError.fromBody(404, body);
    expect(error.code).toBe('not_found');
    expect(error.message).toBe('Work not found.');
  });

  it('falls back to http_<status> when the body has no parseable code', () => {
    const error = NexusClientError.fromBody(500, null);
    expect(error.code).toBe('http_500');
    expect(error.message).toBe('Request failed with status 500');
  });

  it('falls back to http_<status> for an unstructured string body', () => {
    const error = NexusClientError.fromBody(502, 'upstream timeout');
    expect(error.code).toBe('http_502');
    expect(error.message).toBe('Request failed with status 502');
  });

  it('uses the generic fallback when body is undefined', () => {
    const error = NexusClientError.fromBody(418, undefined);
    expect(error.code).toBe('http_418');
    expect(error.message).toBe('Request failed with status 418');
  });

  it('prefers inner envelope fields over stray top-level fields', () => {
    // Defensive: if both an envelope and top-level fields exist, the envelope wins.
    const body = {
      code: 'stale_top_level',
      message: 'ignored',
      error: { code: 'conflict', message: 'Work is locked by another session.' },
    };
    const error = NexusClientError.fromBody(409, body);
    expect(error.code).toBe('conflict');
    expect(error.message).toBe('Work is locked by another session.');
  });
});

describe('NexusClientError constructor', () => {
  it('carries status, code, message, and details', () => {
    const error = new NexusClientError(400, 'bad_request', 'bad', { field: 'title' });
    expect(error.status).toBe(400);
    expect(error.code).toBe('bad_request');
    expect(error.message).toBe('bad');
    expect(error.details).toEqual({ field: 'title' });
    expect(error.name).toBe('NexusClientError');
    expect(error).toBeInstanceOf(Error);
  });
});
