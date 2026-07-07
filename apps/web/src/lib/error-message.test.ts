import { describe, expect, it } from 'vitest';

import { errorMessage } from './error-message';

describe('errorMessage', () => {
  it('returns message from an Error instance', () => {
    const err = new Error('something failed');
    expect(errorMessage(err)).toBe('something failed');
  });

  it('returns message from a plain object with a string message field', () => {
    expect(errorMessage({ message: 'tauri invoke error' })).toBe('tauri invoke error');
  });

  it('falls through when the message field is not a string', () => {
    expect(errorMessage({ message: 42 })).toBe('');
  });

  it('returns string primitives as-is', () => {
    expect(errorMessage('plain string error')).toBe('plain string error');
  });

  it('returns empty string for undefined', () => {
    expect(errorMessage(undefined)).toBe('');
  });

  it('returns empty string for null', () => {
    expect(errorMessage(null)).toBe('');
  });

  it('returns empty string for a plain object without a message field', () => {
    expect(errorMessage({})).toBe('');
  });
});
