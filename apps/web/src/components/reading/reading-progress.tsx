/**
 * ReadingProgress — V1.79 Author Reflection (Track A / P0).
 *
 * Session-only scroll/position indicator. Tracks document scroll progress
 * through the reading surface and renders a thin progress bar. Per the P-1
 * clarify gate 3 decision, progress is NEVER persisted — no localStorage,
 * no session DB, no write route. The component is keyed on chapter by the
 * parent so the bar resets to zero when the reader navigates to a new chapter.
 *
 * DESIGN.md §reading-progress-indicator tokens map to the existing background /
 * blue / gray primitives (track, fill, label). The bar is a progressbar role
 * with text + value (status is never color-only).
 */
import { useEffect, useState } from 'react';

/** Debounce floor so the bar doesn't thrash on every scroll event tick. */
const RAF_GUARD_MS = 16;

export function ReadingProgress() {
  const [pct, setPct] = useState(0);

  useEffect(() => {
    let frame = 0;
    let last = 0;

    function onScroll() {
      const now = performance.now();
      if (now - last < RAF_GUARD_MS) {
        if (!frame) frame = window.requestAnimationFrame(flush);
        return;
      }
      last = now;
      flush();
    }

    function flush() {
      frame = 0;
      const scrollable = document.documentElement.scrollHeight - window.innerHeight;
      // A page shorter than the viewport has nothing to track; keep the bar at
      // zero rather than dividing by zero.
      const ratio = scrollable > 0 ? window.scrollY / scrollable : 0;
      setPct(Math.max(0, Math.min(100, Math.round(ratio * 100))));
    }

    window.addEventListener('scroll', onScroll, { passive: true });
    window.addEventListener('resize', onScroll);
    flush();

    return () => {
      if (frame) window.cancelAnimationFrame(frame);
      window.removeEventListener('scroll', onScroll);
      window.removeEventListener('resize', onScroll);
    };
  }, []);

  return (
    <div
      className="flex items-center gap-2"
      role="progressbar"
      aria-label="Reading progress"
      aria-valuenow={pct}
      aria-valuemin={0}
      aria-valuemax={100}
    >
      <div className="h-1.5 flex-1 overflow-hidden rounded-pill bg-gray-alpha-200">
        <div
          className="h-full rounded-pill bg-blue-700 transition-[width] duration-state ease-standard motion-reduce:transition-none"
          style={{ width: `${pct}%` }}
        />
      </div>
      <span className="tabular-nums text-label-12 text-gray-700">{pct}%</span>
    </div>
  );
}
