/**
 * Status badge for the world-scoped rules section (V1.166 P2, PD-1/PD-2).
 *
 * The spoke rule-status vocabulary renders VERBATIM — `draft | active |
 * deprecated` and any open string — so authors see exactly what auto-include
 * skips (only `active` rules auto-include; draft/deprecated stay visible).
 * Tone mapping goes through the DESIGN.md `badge-status-pill` tokens with an
 * explicit open-string fallback: `active` → green (live, auto-included),
 * `deprecated` → amber (stale — the same semantic family as the generic
 * `statusVariant` `stale` keyword), `draft` and everything else → neutral
 * while the string itself stays untouched. Deliberately does NOT reuse the
 * work `statusVariant` keyword matcher (T1 precedent: the world path must
 * stay strictly spoke-vocabulary).
 */
import { Badge, type BadgeProps } from '@/components/ui/badge';

export function worldRuleStatusVariant(
  status: string | undefined | null,
): BadgeProps['variant'] {
  switch (status?.toLowerCase()) {
    case 'active':
      return 'running'; // DESIGN.md semantic mapping: running/healthy → green
    case 'deprecated':
      return 'warning'; // DESIGN.md: stale/needs-review → amber
    case 'draft':
      return 'neutral'; // staged, not evaluated — muted
    default:
      // Open-string fallback — unknown status keeps the neutral tone and
      // renders verbatim (never coerced to a different vocabulary).
      return 'neutral';
  }
}

export function WorldRuleStatusBadge({ status }: { status: string }) {
  return (
    <Badge variant={worldRuleStatusVariant(status)} data-testid="world-rule-status">
      {status}
    </Badge>
  );
}
