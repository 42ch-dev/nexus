/**
 * Activation trace list — P1 T1 (DF-76). Read-only.
 *
 * Renders one row per `activation_trace` entry: canonical name, a fired/missed
 * badge (from `accepted`), the human-readable reason, the slot the entry
 * landed in (fired entries only, joined via `slot_map`), and hop depth/origin
 * **only when the entry carries hop fields**. The current wire does not emit
 * hop fields (spec §2 H4 — forward-compatible), mirroring the CLI `--inspect`
 * view (`render_inspector_readable`). The trace is observed, never modified
 * (AC-I6).
 */
import { Badge } from '@/components/ui/badge';
import { shortId } from '@/lib/format';
import type { MomentInspectResponse } from '@42ch/nexus-contracts';
import { useTranslation } from 'react-i18next';

/**
 * A trace entry as emitted by `build_inspector_packet` today, extended with
 * forward-compatible hop metadata. Hopped entries render their hop
 * depth/origin only when these fields are present.
 */
export type ActivationTraceEntryWithHop =
  MomentInspectResponse['modules']['activation_trace'][number] & {
    /** Relation-hop depth of a hopped entry (not emitted today, spec §2 H4). */
    hop_depth?: number;
    /** Entry the hopped entry originated from (not emitted today, spec §2 H4). */
    hop_origin_entry_id?: string;
  };

export interface ActivationTraceListProps {
  trace: ActivationTraceEntryWithHop[];
  /** `entry_id` → slot id, derived from `MomentInspectResponse.slot_map`. */
  slotByEntry: ReadonlyMap<string, string>;
}

export function ActivationTraceList({ trace, slotByEntry }: ActivationTraceListProps) {
  const { t } = useTranslation('inspector');

  if (trace.length === 0) {
    return (
      <p className="text-copy-13 text-gray-700" data-testid="trace-empty">
        {t('trace.empty')}
      </p>
    );
  }

  return (
    <ol className="flex flex-col gap-2" data-testid="activation-trace">
      {trace.map((entry) => {
        const fired = entry.accepted;
        // Slot is only meaningful for fired entries (the packet captures the
        // post stage-gate routing of accepted entries).
        const slot = fired ? slotByEntry.get(entry.entry_id) : undefined;
        return (
          <li
            key={entry.entry_id}
            className="flex flex-col gap-1 rounded-control border border-gray-alpha-400 bg-background-100 p-3"
            data-testid={`trace-entry-${entry.entry_id}`}
          >
            <div className="flex flex-wrap items-center gap-2">
              <Badge variant={fired ? 'running' : 'neutral'} tone="soft">
                <span aria-hidden>{fired ? '✓' : '✗'}</span>
                {fired ? t('trace.fired') : t('trace.missed')}
              </Badge>
              <span className="text-copy-13 font-semibold text-gray-1000">{entry.canonical_name}</span>
              <span className="text-copy-13-mono text-gray-700">{shortId(entry.entry_id)}</span>
              {slot ? (
                <Badge variant="neutral" tone="soft" data-testid={`trace-slot-${entry.entry_id}`}>
                  {t('trace.slotLabel')}: {slot}
                </Badge>
              ) : null}
            </div>
            <p className="text-copy-13 text-gray-900" data-testid={`trace-reason-${entry.entry_id}`}>
              <span className="text-gray-700">{t('trace.reasonLabel')}:</span> {entry.reason}
            </p>
            {entry.hop_depth !== undefined || entry.hop_origin_entry_id !== undefined ? (
              <p
                className="flex flex-wrap gap-x-4 gap-y-0.5 text-copy-13 text-gray-900"
                data-testid={`trace-hop-${entry.entry_id}`}
              >
                {entry.hop_depth !== undefined ? (
                  <span>
                    {t('trace.hopDepthLabel')}: {entry.hop_depth}
                  </span>
                ) : null}
                {entry.hop_origin_entry_id !== undefined ? (
                  <span>
                    {t('trace.hopOriginLabel')}: {entry.hop_origin_entry_id}
                  </span>
                ) : null}
              </p>
            ) : null}
          </li>
        );
      })}
    </ol>
  );
}
