/**
 * Task-kind badge — V1.78 Creator Memory review-loop (web-ui.md §24 + DESIGN.md
 * `memory-task-kind-*`).
 *
 * `task_kind` is a free-form string on the wire (the contract does not constrain
 * it), so the badge maps the five known values (`brainstorm` / `outline` /
 * `chapter` / `research` / `unknown`) to distinct color accents via the same
 * `color-mix` pattern the V1.77 findings-status badges use, and falls back to a
 * neutral chip for any unrecognized value (rendered verbatim so authors are not
 * misled). Colors reuse the established semantic palette and stay correct in
 * both light and dark themes.
 */
import { humanizeStatus } from '@/lib/format';
import { cn } from '@/lib/utils';

/** Known `task_kind` values the daemon defaults and capture pipeline emit. */
export type KnownTaskKind = 'brainstorm' | 'outline' | 'chapter' | 'research' | 'unknown';

export const KNOWN_TASK_KINDS: readonly KnownTaskKind[] = [
  'brainstorm',
  'outline',
  'chapter',
  'research',
  'unknown',
];

/**
 * Tailwind classes for each task-kind chip. Mirrors DESIGN.md frontmatter
 * `memory-task-kind-*` (verbatim token names; values filled in A5).
 */
function taskKindClasses(taskKind: string | undefined | null): string {
  switch (taskKind as KnownTaskKind) {
    case 'brainstorm':
      // amber — ideation / creative.
      return 'bg-[color-mix(in_srgb,var(--color-amber-700)_12%,transparent)] text-amber-1000 border-[color-mix(in_srgb,var(--color-amber-700)_30%,transparent)]';
    case 'outline':
      // blue — planning / structure.
      return 'bg-[color-mix(in_srgb,var(--color-blue-700)_10%,transparent)] text-blue-1000 border-[color-mix(in_srgb,var(--color-blue-700)_30%,transparent)]';
    case 'chapter':
      // teal — writing / content.
      return 'bg-[color-mix(in_srgb,var(--color-teal-700)_10%,transparent)] text-teal-1000 border-[color-mix(in_srgb,var(--color-teal-700)_30%,transparent)]';
    case 'research':
      // purple — inquiry / knowledge.
      return 'bg-[color-mix(in_srgb,var(--color-purple-700)_10%,transparent)] text-purple-1000 border-[color-mix(in_srgb,var(--color-purple-700)_30%,transparent)]';
    case 'unknown':
    default:
      // neutral gray — unrecognized values render verbatim (humanized).
      return 'bg-gray-alpha-100 text-gray-900 border-gray-alpha-300';
  }
}

interface TaskKindBadgeProps {
  taskKind?: string | null;
  className?: string;
}

/** Task-kind pill with the DESIGN.md `memory-task-kind-*` mapping. */
export function TaskKindBadge({ taskKind, className }: TaskKindBadgeProps) {
  return (
    <span
      className={cn(
        'inline-flex h-6 items-center rounded-pill border px-2 text-label-12',
        taskKindClasses(taskKind),
        className,
      )}
    >
      {humanizeStatus(taskKind)}
    </span>
  );
}
