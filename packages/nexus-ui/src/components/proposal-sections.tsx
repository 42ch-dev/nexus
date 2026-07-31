import type { ReactNode } from 'react';

import { cn } from '../lib/cn';

import { Badge } from './badge';

/**
 * Structural mirror of the compute proposals envelope (state_delta,
 * timeline_events, new_key_blocks, battle_report). All-optional/subset
 * fields keep the primitive compatible with the generated DTOs via
 * structural typing — the package never imports wire contracts.
 */
export interface StateDeltaProposal {
  op: 'add' | 'sub' | 'set';
  path: string;
  target_key_block_id?: string;
  value?: unknown;
}

export interface TimelineEventProposal {
  title?: string;
  summary?: string;
  affected_key_block_ids?: string[];
}

export interface BattleReportProposal {
  kind?: string;
  [key: string]: unknown;
}

export interface ComputeProposals {
  schema_version?: number;
  state_delta: StateDeltaProposal[];
  timeline_events: TimelineEventProposal[];
  new_key_blocks: Record<string, unknown>[];
  battle_report: BattleReportProposal;
}

/** Caller-owned copy for the proposal inspector (i18n lives in the app). */
export interface ProposalSectionsCopy {
  reportTitle: string;
  knowledgeUpdatesTitle: string;
  timelineEventsTitle: string;
  newKnowledgeTitle: string;
  /** Note shown when the run response was truncated (full detail via the Run). */
  truncatedNote: string;
  /** Fallback for a timeline event proposal without a title. */
  untitledEventLabel: string;
  /** Label formatting affected entries per event — caller-owned for i18n plurals. */
  affectedEntriesLabel: (count: number) => string;
  /** Fallback caption for one new-knowledge entry without a recognizable title. */
  newEntryLabel: string;
}

export interface ProposalSectionsProps {
  proposals: ComputeProposals;
  /** True when the response payload was truncated — renders the truncated note. */
  truncated?: boolean;
  copy: ProposalSectionsCopy;
  /**
   * Optional per-event selection (behavior spec §2 opt-in): when
   * `onToggleEvent` is supplied, each timeline event renders a checkbox
   * keyed by its stable id `evt_<index>`; `selectedEventIds` drives state.
   */
  selectedEventIds?: string[];
  onToggleEvent?: (eventId: string) => void;
  className?: string;
}

function formatValue(value: unknown): string {
  if (value === undefined) return '—';
  if (typeof value === 'string') return value;
  try {
    return JSON.stringify(value);
  } catch {
    // Circular structures only — module output is JSON by contract.
    return '[unserializable value]';
  }
}

function Section({
  title,
  testId,
  children,
}: {
  title: string;
  testId: string;
  children: ReactNode;
}) {
  return (
    <section data-testid={testId} className="flex flex-col gap-2">
      <h4 className="text-label-14 font-medium text-gray-1000">{title}</h4>
      {children}
    </section>
  );
}

/** Picks a display title for a new-knowledge entry proposal. */
function newEntryTitle(entry: Record<string, unknown>, fallback: string): string {
  const candidate = entry.title ?? entry.name;
  return typeof candidate === 'string' && candidate.length > 0 ? candidate : fallback;
}

/**
 * ProposalSections — review inspector for a succeeded Run's proposals
 * (V1.147 P1). Groups the 4-part envelope per behavior spec §2: Report /
 * Knowledge updates / Timeline events / New knowledge; empty sections are
 * hidden. Pure presentational: copy and callbacks are caller-owned.
 */
export function ProposalSections({
  proposals,
  truncated = false,
  copy,
  selectedEventIds,
  onToggleEvent,
  className,
}: ProposalSectionsProps) {
  const reportEntries = Object.entries(proposals.battle_report).filter(
    ([key]) => key !== 'kind',
  );
  const showReport = proposals.battle_report.kind !== undefined || reportEntries.length > 0;

  return (
    <div className={cn('flex flex-col gap-6', className)} data-testid="proposal-sections">
      {truncated && (
        <p
          data-testid="proposal-truncated-note"
          className="rounded-control border border-warning-surface-border bg-warning-surface px-3 py-2 text-copy-13 text-gray-1000"
        >
          {copy.truncatedNote}
        </p>
      )}

      {showReport && (
        <Section title={copy.reportTitle} testId="proposal-section-report">
          <div className="flex flex-col gap-1.5">
            {proposals.battle_report.kind && (
              <div>
                <Badge variant="queued" data-testid="proposal-report-kind">
                  {proposals.battle_report.kind}
                </Badge>
              </div>
            )}
            {reportEntries.map(([key, value]) => (
              <div key={key} className="flex flex-wrap items-baseline gap-2">
                <span className="text-copy-13 text-gray-700">{key}</span>
                <span className="text-copy-13-mono text-gray-1000">{formatValue(value)}</span>
              </div>
            ))}
          </div>
        </Section>
      )}

      {proposals.state_delta.length > 0 && (
        <Section title={copy.knowledgeUpdatesTitle} testId="proposal-section-knowledge-updates">
          <ul className="flex flex-col gap-1.5">
            {proposals.state_delta.map((delta, index) => (
              <li
                key={`${delta.path}-${index}`}
                className="flex flex-wrap items-baseline gap-2"
                data-testid={`proposal-delta-${index}`}
              >
                <Badge variant="neutral" className="uppercase">
                  {delta.op}
                </Badge>
                {delta.target_key_block_id && (
                  <span className="text-copy-13-mono text-gray-1000">
                    {delta.target_key_block_id}
                  </span>
                )}
                <span className="text-copy-13-mono text-gray-700">{delta.path}</span>
                <span className="text-copy-13-mono text-gray-1000">
                  {formatValue(delta.value)}
                </span>
              </li>
            ))}
          </ul>
        </Section>
      )}

      {proposals.timeline_events.length > 0 && (
        <Section title={copy.timelineEventsTitle} testId="proposal-section-timeline-events">
          <ul className="flex flex-col gap-2">
            {proposals.timeline_events.map((event, index) => {
              const eventId = `evt_${index}`;
              const affected = event.affected_key_block_ids?.length ?? 0;
              return (
                <li
                  key={eventId}
                  data-testid={`proposal-event-${eventId}`}
                  className="rounded-control border border-gray-alpha-300 bg-background-100 p-3"
                >
                  <div className="flex items-start gap-2">
                    {onToggleEvent && (
                      <input
                        type="checkbox"
                        className="mt-1 h-4 w-4 accent-blue-700"
                        checked={selectedEventIds?.includes(eventId) ?? true}
                        onChange={() => onToggleEvent(eventId)}
                        aria-label={event.title ?? copy.untitledEventLabel}
                        data-testid={`proposal-event-toggle-${eventId}`}
                      />
                    )}
                    <div className="flex flex-col gap-1">
                      <p className="text-label-14 font-medium text-gray-1000">
                        {event.title ?? copy.untitledEventLabel}
                      </p>
                      {event.summary && (
                        <p className="text-copy-13 text-gray-700">{event.summary}</p>
                      )}
                      {affected > 0 && (
                        <p className="text-copy-13 text-gray-700">
                          {copy.affectedEntriesLabel(affected)}
                        </p>
                      )}
                    </div>
                  </div>
                </li>
              );
            })}
          </ul>
        </Section>
      )}

      {proposals.new_key_blocks.length > 0 && (
        <Section title={copy.newKnowledgeTitle} testId="proposal-section-new-knowledge">
          <ul className="flex flex-col gap-2">
            {proposals.new_key_blocks.map((entry, index) => (
              <li
                key={index}
                data-testid={`proposal-new-entry-${index}`}
                className="rounded-control border border-gray-alpha-300 bg-background-100 p-3"
              >
                <p className="text-label-14 font-medium text-gray-1000">
                  {newEntryTitle(entry, copy.newEntryLabel)}
                </p>
                <p className="mt-1 text-copy-13-mono text-gray-700 break-all">
                  {formatValue(entry)}
                </p>
              </li>
            ))}
          </ul>
        </Section>
      )}
    </div>
  );
}
