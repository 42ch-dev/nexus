/**
 * Chapter inspector — shared metadata field primitives (V1.76 B1 /
 * `R-V175QC1-S001`).
 *
 * Extracted from `chapter-inspector.tsx` to keep that file ≤240 lines
 * (V1.73 split cap). Pure presentational — label/control wrapper + the shared
 * DESIGN.md input class. No behavior.
 */
import type { ReactNode } from 'react';

/** Shared form-control class for the metadata inputs (DESIGN.md tokens). */
export const INPUT_CLASS =
  'rounded-control border border-gray-alpha-400 bg-background-100 px-3 py-2 text-gray-1000 focus:border-blue-700 disabled:bg-gray-100 disabled:text-gray-700';

/** Label + control wrapper for the metadata fields. */
export function MetaField({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="flex flex-col gap-1 text-copy-13">
      <span className="text-gray-700">{label}</span>
      {children}
    </label>
  );
}
