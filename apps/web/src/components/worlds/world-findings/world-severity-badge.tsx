/**
 *
 * The spoke severity vocabulary renders VERBATIM — `info | warning | error`
 * and any open string — and is NEVER remapped to the legacy work-findings
 * vocabulary (`minor / major / blocker`). Tone mapping goes through the
 * DESIGN.md `badge-status-pill` tokens with an explicit open-string fallback:
 * known spoke severities map to their semantic hue (info → teal informational,
 * warning → amber, error → red); anything else renders neutral while the
 * string itself stays untouched.
 */
import { Badge, type BadgeProps } from '@/components/ui/badge';

export function worldSeverityVariant(severity: string | undefined | null): BadgeProps['variant'] {
  switch (severity?.toLowerCase()) {
    case 'info':
      return 'queued'; // DESIGN.md semantic mapping: informational → teal
    case 'warning':
      return 'warning';
    case 'error':
      return 'error';
    default:
      // renders verbatim (never coerced to work `minor/major/blocker`).
      return 'neutral';
  }
}

export function WorldSeverityBadge({ severity }: { severity: string }) {
  return (
    <Badge variant={worldSeverityVariant(severity)} data-testid="world-finding-severity">
      {severity}
    </Badge>
  );
}
