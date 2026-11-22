/**
 * View-preference helpers for the World KB canvas (V1.73 P0 A6/A8).
 *
 * Detects whether the user prefers reduced motion (WCAG 2.3.3) so the
 * non-spatial alternate view can default to list mode for keyboard-only /
 * screen-reader users, and re-evaluates when the OS preference changes.
 */
import { useEffect, useState } from 'react';

const QUERY = '(prefers-reduced-motion: reduce)';

/** True when the user has requested reduced motion at the OS level. */
export function useReducedMotionPreference(): boolean {
  const [prefers, setPrefers] = useState<boolean>(() => readPrefersReducedMotion());

  useEffect(() => {
    if (typeof window === 'undefined' || !window.matchMedia) return;
    const mql = window.matchMedia(QUERY);
    const onChange = (event: MediaQueryListEvent) => setPrefers(event.matches);
    mql.addEventListener('change', onChange);
    return () => mql.removeEventListener('change', onChange);
  }, []);

  return prefers;
}

function readPrefersReducedMotion(): boolean {
  if (typeof window === 'undefined' || !window.matchMedia) return false;
  return window.matchMedia(QUERY).matches;
}
