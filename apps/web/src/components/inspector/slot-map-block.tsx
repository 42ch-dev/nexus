/**
 * Slot map block — P1 T2 (DF-76). Read-only.
 *
 * Renders `slot_map` (`entry_id` → slot id) grouped in prompt emit order:
 * `world.before` → `default` → `world.after` → `kb.outlet.<name>` →
 * `style.post_history` → `moment.directive` (moment.directive top-level when
 * an active directive renders). Within the same slot, the packet's capture
 * order is preserved (stable sort). The slot map is observed, never modified
 * (AC-I6).
 */
import { Badge } from '@/components/ui/badge';
import { shortId } from '@/lib/format';
import type { MomentInspectResponse } from '@42ch/nexus-contracts';
import { useId } from 'react';
import { useTranslation } from 'react-i18next';

/** Emit-order rank for named slots (spec §2 H2). `kb.outlet.<name>` is a prefix family. */
const SLOT_EMIT_RANK: Record<string, number> = {
  'world.before': 0,
  default: 1,
  'world.after': 2,
  'style.post_history': 4,
  'moment.directive': 5,
};

function slotRank(slot: string): number {
  if (slot.startsWith('kb.outlet.')) return 3;
  return SLOT_EMIT_RANK[slot] ?? 6;
}

export interface SlotMapBlockProps {
  slotMap: MomentInspectResponse['slot_map'];
}

export function SlotMapBlock({ slotMap }: SlotMapBlockProps) {
  const { t } = useTranslation('inspector');
  const titleId = useId();
  const ordered = [...slotMap].sort((a, b) => slotRank(a.slot) - slotRank(b.slot));

  return (
    <section aria-labelledby={titleId} data-testid="slot-map-block">
      <h3 id={titleId} className="text-heading-16 font-heading text-gray-1000">
        {t('slotMap.title')}
      </h3>
      <p className="text-copy-13 text-gray-700">{t('slotMap.description')}</p>
      {ordered.length === 0 ? (
        <p className="mt-2 text-copy-13 text-gray-700" data-testid="slot-map-empty">
          {t('slotMap.empty')}
        </p>
      ) : (
        <ul className="mt-2 flex flex-col gap-1" data-testid="slot-map">
          {ordered.map(({ entry_id, slot }) => (
            <li key={entry_id} className="flex flex-wrap items-center gap-2" data-testid={`slot-row-${entry_id}`}>
              <span className="text-copy-13-mono text-gray-900">{shortId(entry_id)}</span>
              <Badge variant="neutral" tone="soft" data-testid={`slot-name-${entry_id}`}>
                {slot}
              </Badge>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
