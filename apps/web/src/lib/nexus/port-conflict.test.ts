import { describe, expect, it } from 'vitest';

import { isPortConflictError } from './port-conflict';

describe('isPortConflictError', () => {
  it('returns true when the message mentions port + in use', () => {
    expect(
      isPortConflictError(
        'Nexus couldn\'t start its background service — port 8420 is already in use.',
      ),
    ).toBe(true);
  });

  it('is case-insensitive', () => {
    expect(isPortConflictError('PORT 8420 IN USE')).toBe(true);
    expect(isPortConflictError('Port already In Use')).toBe(true);
  });

  it('returns false when only one marker is present', () => {
    expect(isPortConflictError('port 8420 is reserved')).toBe(false);
    expect(isPortConflictError('the address is in use')).toBe(false);
  });

  it('returns false for unrelated or empty input', () => {
    expect(isPortConflictError('Daemon did not start.')).toBe(false);
    expect(isPortConflictError('')).toBe(false);
  });
});
