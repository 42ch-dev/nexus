/**
 * Task-kind badge — V1.78 Creator Memory review-loop (web-ui.md §24 + DESIGN.md
 * `memory-task-kind-*`).
 *
 * `task_kind` is a free-form string on the wire (the contract does not constrain
 * it), so the badge maps the five known values (`brainstorm` / `outline` /
 * `chapter` / `research` / `unknown`) to distinct color accents via the
 * DESIGN.md frontmatter `components.memory-task-kind-*` tokens (projected as
 * `--color-memory-task-kind-*` CSS vars via @nexus/design-tokens), and falls
 * back to the neutral `unknown` chip for any unrecognized value (rendered
 * verbatim so authors are not misled). Tokens stay correct in both light and
 * dark themes.
 */
import { humanizeStatus } from '@/lib/format';
import { cn } from '@/lib/utils';
import { useTranslation } from 'react-i18next';

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
 * `memory-task-kind-*` via the projected token classes.
 */
function taskKindClasses(taskKind: string | undefined | null): string {
  switch (taskKind as KnownTaskKind) {
    case 'brainstorm':
      // amber — ideation / creative.
      return 'bg-memory-task-kind-brainstorm-bg text-memory-task-kind-brainstorm-text border-memory-task-kind-brainstorm-border';
    case 'outline':
      // blue — planning / structure.
      return 'bg-memory-task-kind-outline-bg text-memory-task-kind-outline-text border-memory-task-kind-outline-border';
    case 'chapter':
      // teal — writing / content.
      return 'bg-memory-task-kind-chapter-bg text-memory-task-kind-chapter-text border-memory-task-kind-chapter-border';
    case 'research':
      // purple — inquiry / knowledge.
      return 'bg-memory-task-kind-research-bg text-memory-task-kind-research-text border-memory-task-kind-research-border';
    case 'unknown':
    default:
      // neutral gray — unrecognized values render verbatim (humanized).
      return 'bg-memory-task-kind-unknown-bg text-memory-task-kind-unknown-text border-memory-task-kind-unknown-border';
  }
}

interface TaskKindBadgeProps {
  taskKind?: string | null;
  className?: string;
}

/** Task-kind pill with the DESIGN.md `memory-task-kind-*` mapping. */
export function TaskKindBadge({ taskKind, className }: TaskKindBadgeProps) {
  const { t } = useTranslation('memory');
  const normalized = (taskKind ?? 'unknown') as KnownTaskKind;
  const label = KNOWN_TASK_KINDS.includes(normalized)
    ? t(`taskKind.${normalized}`)
    : humanizeStatus(taskKind);
  return (
    <span
      className={cn(
        'inline-flex h-6 items-center rounded-pill border px-2 text-label-12',
        taskKindClasses(taskKind),
        className,
      )}
    >
      {label}
    </span>
  );
}
