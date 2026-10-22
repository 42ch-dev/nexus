/**
 * Activation store (plan v1219 § Shared interfaces 1) — per-creator local
 * unlock state; only `brief-saved` → `work` is writable this iteration (D26).
 */
import { renderHook } from '@testing-library/react';
import { act } from 'react';
import { beforeEach, describe, expect, it } from 'vitest';

import { isUnlocked, unlock, useActivation } from '@/lib/activation-store';

const KEY = 'nexus-activation:creator-a';

beforeEach(() => {
  window.localStorage.clear();
});

describe('activation store', () => {
  it('locks every app for a creator with no persisted state', () => {
    expect(isUnlocked('creator-a', 'work')).toBe(false);
    expect(isUnlocked('creator-a', 'session')).toBe(false);
    expect(isUnlocked(null, 'work')).toBe(false);
  });

  it('persists the brief-saved unlock under the per-creator key', () => {
    unlock('creator-a', 'work');
    expect(window.localStorage.getItem(KEY)).toBe(JSON.stringify({ work: true }));
    expect(isUnlocked('creator-a', 'work')).toBe(true);
    // Other creators are unaffected.
    expect(isUnlocked('creator-b', 'work')).toBe(false);
  });

  it('accepts a persisted map written before mount', () => {
    window.localStorage.setItem(KEY, JSON.stringify({ work: true }));
    expect(isUnlocked('creator-a', 'work')).toBe(true);
  });

  it('refuses to write unlocks with no unlock path this iteration (D17/D26)', () => {
    unlock('creator-a', 'session');
    unlock('creator-a', 'memory');
    unlock('creator-a', 'world');
    expect(window.localStorage.getItem(KEY)).toBeNull();
    expect(isUnlocked('creator-a', 'session')).toBe(false);
    expect(isUnlocked('creator-a', 'memory')).toBe(false);
  });

  it('useActivation re-renders when the same tab unlocks', () => {
    const { result } = renderHook(() => useActivation('creator-a'));
    expect(result.current.isUnlocked('work')).toBe(false);
    act(() => unlock('creator-a', 'work'));
    expect(result.current.isUnlocked('work')).toBe(true);
  });
});
