/**
 * Assembly Inspector panel — P1 T1–T3 (DF-76). Read-only.
 *
 * Consumes the enriched inspector packet from `POST /v1/daemon/inspector/moment`
 * (`MomentInspectResponse`) and renders four sections: activation trace (T1),
 * slot map + budget + directive status (T2). Pure presentation — data comes
 * from the parent via the `packet` prop; this component never fetches and
 * never writes (AC-I4). The UI only observes the route — it changes no
 * assembled bytes (AC-I6).
 *
 * Batch B extension point: the Moment Directive set/clear form (T4) mounts via
 * `directiveActions`, forwarded to {@link DirectiveStatusBlock}.
 */
import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';

import type { MomentInspectResponse } from '@42ch/nexus-contracts';

import { ActivationTraceList } from './activation-trace-list';
import { BudgetBlock } from './budget-block';
import { DirectiveStatusBlock } from './directive-status-block';
import { SlotMapBlock } from './slot-map-block';

export interface AssemblyInspectorPanelProps {
  packet: MomentInspectResponse;
  /** Batch B (T4) extension point — the directive set/clear form. */
  directiveActions?: React.ReactNode;
}

export function AssemblyInspectorPanel({ packet, directiveActions }: AssemblyInspectorPanelProps) {
  const { t } = useTranslation('inspector');

  // entry_id → slot, joined once per packet so the trace rows and slot map
  // share one lookup.
  const slotByEntry = useMemo(
    () => new Map(packet.slot_map.map((entry) => [entry.entry_id, entry.slot])),
    [packet.slot_map],
  );

  return (
    <div className="flex flex-col gap-6" data-testid="assembly-inspector-panel">
      <section aria-labelledby="inspector-trace-title" data-testid="trace-block">
        <h2 id="inspector-trace-title" className="text-heading-16 font-heading text-gray-1000">
          {t('trace.title')}
        </h2>
        <p className="text-copy-13 text-gray-700">{t('trace.description')}</p>
        <div className="mt-2">
          <ActivationTraceList trace={packet.modules.activation_trace} slotByEntry={slotByEntry} />
        </div>
      </section>

      <SlotMapBlock slotMap={packet.slot_map} />
      <BudgetBlock budget={packet.budget} />
      <DirectiveStatusBlock directive={packet.moment_directive} actions={directiveActions} />

      <p className="border-t border-gray-alpha-400 pt-3 text-copy-13 text-gray-700" data-testid="inspector-readonly-note">
        {t('readonlyNote')}
      </p>
    </div>
  );
}
