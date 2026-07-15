import { useEffect, useState } from 'react';

const LG_MIN_WIDTH_QUERY = '(min-width: 961px)';

function readMinLgViewport(): boolean {
  if (typeof window === 'undefined' || !window.matchMedia) return false;
  return window.matchMedia(LG_MIN_WIDTH_QUERY).matches;
}

/** True when the viewport is at or above the app `lg` breakpoint (961px). */
export function useMinLgViewport(): boolean {
  const [matches, setMatches] = useState<boolean>(() => readMinLgViewport());

  useEffect(() => {
    if (typeof window === 'undefined' || !window.matchMedia) return;
    const mql = window.matchMedia(LG_MIN_WIDTH_QUERY);
    const onChange = (event: MediaQueryListEvent) => setMatches(event.matches);
    mql.addEventListener('change', onChange);
    return () => mql.removeEventListener('change', onChange);
  }, []);

  return matches;
}
