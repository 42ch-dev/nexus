/**
 * Inline mono Nexus mark — hand-authored JSX derived from `logo-mono.svg`.
 *
 * This component does not import any asset file; it inlines the path data so
 * the package stays bundler-agnostic. Color is inherited via `currentColor`.
 */

import { useId } from 'react';

import { logoMinSizePx } from '../tokens';

export interface NexusMarkProps {
  /** Accessible label for the mark. Defaults to the product name. */
  label?: string;
  className?: string;
  /** Rendered size in px (width and height). Defaults to `logoMinSizePx`. */
  size?: number;
}

export function NexusMark({
  label = 'Nexus',
  className,
  size = logoMinSizePx,
}: NexusMarkProps) {
  const titleId = useId();
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 100 100"
      role="img"
      width={size}
      height={size}
      className={className}
      aria-labelledby={titleId}
    >
      <title id={titleId}>{label}</title>
      <g
        fill="none"
        stroke="currentColor"
        strokeWidth="6.6"
        strokeLinecap="round"
        strokeLinejoin="round"
      >
        <rect x="8" y="8" width="84" height="84" rx="20" />
        <polygon points="50,8 92,50 50,92 8,50" />
        <path d="M30 28 L30 72" />
        <path d="M30 28 L70 72" />
        <path d="M70 28 L70 72" />
      </g>
      <g fill="currentColor" stroke="none">
        <circle cx="50" cy="8" r="7.4" />
        <circle cx="92" cy="50" r="7.4" />
        <circle cx="50" cy="92" r="7.4" />
        <circle cx="8" cy="50" r="7.4" />
        <circle cx="30" cy="28" r="7.4" />
        <circle cx="70" cy="28" r="7.4" />
        <circle cx="30" cy="72" r="7.4" />
        <circle cx="70" cy="72" r="7.4" />
      </g>
    </svg>
  );
}
