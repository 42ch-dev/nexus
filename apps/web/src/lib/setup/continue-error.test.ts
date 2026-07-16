/**
 * Classifier matrix tests for `classifySetupContinueError`.
 *
 * Verifies the spec §Architecture contract classification table (AD-P0):
 * each phase + error-input combination maps to the expected
 * `SetupContinueErrorClass`, which in turn drives Reset visibility and
 * inline-helper copy in the Workspace step.
 *
 * Migration detection relies on the message regex `/migration/i` (T1 finding:
 * structured migration codes are effectively dead — the daemon emits the
 * public code `internal` for DB errors). Both the message path and the
 * structured-code path are covered here for completeness.
 */
import { describe, expect, it } from 'vitest';

import { NexusClientError } from '../nexus/errors';
import { classifySetupContinueError } from './continue-error';

describe('classifySetupContinueError', () => {
  // ── Phase-level (non-bootstrap) ──────────────────────────────────────────

  it('1. display_name phase → always soft_display_name', () => {
    const result = classifySetupContinueError('display_name', new Error('anything'));
    expect(result.class).toBe('soft_display_name');
  });

  it('2. workspace_path phase → always soft_workspace_path', () => {
    const result = classifySetupContinueError('workspace_path', new Error('anything'));
    expect(result.class).toBe('soft_workspace_path');
  });

  // ── Bootstrap phase: migration-class detection ──────────────────────────

  it('3. bootstrap + message contains "migration" → migration_db', () => {
    const result = classifySetupContinueError(
      'bootstrap',
      new Error('database migration needed'),
    );
    expect(result.class).toBe('migration_db');
  });

  it('4. bootstrap + realistic production migration message → migration_db', () => {
    const result = classifySetupContinueError(
      'bootstrap',
      new Error('Failed to open creator database: Failed to run database migrations: schema mismatch'),
    );
    expect(result.class).toBe('migration_db');
  });

  it('5. bootstrap + structured code migration_failed → migration_db', () => {
    const error = new NexusClientError(500, 'migration_failed', 'internal');
    const result = classifySetupContinueError('bootstrap', error);
    expect(result.class).toBe('migration_db');
  });

  it('6. bootstrap + generic "config write failed" → soft_bootstrap', () => {
    const result = classifySetupContinueError(
      'bootstrap',
      new Error('config write failed'),
    );
    expect(result.class).toBe('soft_bootstrap');
  });

  it('7. bootstrap + code database_error → migration_db', () => {
    const error = new NexusClientError(500, 'database_error', 'internal');
    const result = classifySetupContinueError('bootstrap', error);
    expect(result.class).toBe('migration_db');
  });

  it('8. bootstrap + code uninitialized → soft_bootstrap', () => {
    const error = new NexusClientError(409, 'uninitialized', 'not bootstrapped');
    const result = classifySetupContinueError('bootstrap', error);
    expect(result.class).toBe('soft_bootstrap');
  });

  it('8b. bootstrap + production wire code internal (no migration in message) → soft_bootstrap', () => {
    const error = new NexusClientError(500, 'internal', 'database unavailable');
    const result = classifySetupContinueError('bootstrap', error);
    expect(result.class).toBe('soft_bootstrap');
  });

  it('8c. bootstrap + uppercase DATABASE_ERROR is not the public wire code → soft_bootstrap', () => {
    const error = new NexusClientError(500, 'DATABASE_ERROR', 'pool open failed');
    const result = classifySetupContinueError('bootstrap', error);
    expect(result.class).toBe('soft_bootstrap');
  });

  // ── Message extraction (callers apply phase fallback when empty) ─────────

  it('extracts the message from the error for display', () => {
    const result = classifySetupContinueError('bootstrap', new Error('disk full'));
    expect(result.message).toBe('disk full');
  });

  it('returns an empty message when the error has none', () => {
    const result = classifySetupContinueError('workspace_path', {});
    expect(result.message).toBe('');
    expect(result.class).toBe('soft_workspace_path');
  });
});
