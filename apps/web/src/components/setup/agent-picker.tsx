/**
 * AgentPicker — presentational card grid for ACP agent selection.
 *
 * Placement (V1.101 locked): app-shared under `components/setup/`.
 * No wizard routing, no daemon client, no `@42ch/nexus-contracts` wire DTOs.
 * App hosts map scan results + static install/docs URLs into {@link AgentPickerItem}.
 *
 * Settings-reusable (DF-70): future Settings may mount the same component
 * without importing setup wizard pages.
 *
 * V1.102 chrome: soft Installed Badge, ArrowUpRight outbound icons at label
 * cap-height, hollow/lit selection dots, muted not-installed cards.
 */

import { ArrowUpRight, Loader2, Terminal } from 'lucide-react';
import type { ReactNode } from 'react';

import { cn } from '@/lib/utils';
import { Badge, Input, Label } from '@42ch/nexus-ui';

/** Local view-model for one agent card — owned by this module (not wire DTOs). */
export interface AgentPickerItem {
  /** Stable id used for selection (typically registry agent id or name). */
  id: string;
  name: string;
  version?: string | null;
  description?: string | null;
  /** When false, card is discoverability-only (not selectable as profile). */
  installed: boolean;
  /** Outbound install URL; omit/null → hide Install link. */
  installUrl?: string | null;
  /** Outbound docs URL; omit/null → hide Docs link. */
  docsUrl?: string | null;
}

export type AgentPickerStatus = 'loading' | 'ready' | 'empty' | 'error';

export type AgentPickerDensity = 'default' | 'compact';

export interface AgentPickerProps {
  status: AgentPickerStatus;
  agents?: AgentPickerItem[];
  /** Currently selected installed agent id (profile path). */
  selectedId?: string | null;
  onSelect?: (id: string) => void;
  /** Custom launch command value (escape hatch). */
  customLaunchValue?: string;
  onCustomLaunchChange?: (command: string) => void;
  /** When true, always show custom-launch row (default: empty/error, or when list non-empty). */
  showCustomLaunch?: boolean;
  errorTitle?: string;
  errorDescription?: string;
  onRetry?: () => void;
  /** Optional slot above the grid (product copy lives in the host). */
  header?: ReactNode;
  className?: string;
  loadingLabel?: string;
  emptyTitle?: string;
  emptyDescription?: string;
  /** Layout density. Omit or `'default'` for Settings; wizard may pass `'compact'`. */
  density?: AgentPickerDensity;
}

/**
 * Presentational AgentPicker: loading / ready grid / empty / error + custom launch.
 */
export function AgentPicker({
  status,
  agents = [],
  selectedId = null,
  onSelect,
  customLaunchValue = '',
  onCustomLaunchChange,
  showCustomLaunch,
  errorTitle = 'Could not scan for agents',
  errorDescription,
  onRetry,
  header,
  className,
  loadingLabel = 'Scanning for local ACP agents…',
  emptyTitle = 'No agents found on PATH',
  emptyDescription = 'Install an ACP-compatible agent, or continue with a custom launch command.',
  density = 'default',
}: AgentPickerProps) {
  const compact = density === 'compact';
  const customLaunchVisible =
    showCustomLaunch ??
    (status === 'empty' ||
      status === 'error' ||
      (status === 'ready' && agents.length > 0));

  return (
    <div className={cn('flex flex-col', compact ? 'gap-3' : 'gap-4', className)}>
      {header}

      <div
        className={cn(
          'flex flex-col rounded-card border border-gray-alpha-400 bg-background-200',
          compact
            ? 'min-h-[120px] gap-2 p-3'
            : 'min-h-[160px] gap-3 p-4',
        )}
        data-testid="agent-picker"
        data-status={status}
      >
        {status === 'loading' ? (
          <div
            className={cn(
              'flex flex-1 flex-col items-center justify-center gap-2',
              compact ? 'py-6' : 'py-10',
            )}
            role="status"
            aria-live="polite"
          >
            <Loader2 className="h-5 w-5 animate-spin text-blue-700" aria-hidden />
            <span className="text-copy-14 text-gray-900">{loadingLabel}</span>
          </div>
        ) : null}

        {status === 'error' ? (
          <div
            role="alert"
            className="flex flex-col gap-2 rounded-control border border-[color-mix(in_srgb,var(--color-red-700)_30%,transparent)] bg-[color-mix(in_srgb,var(--color-red-700)_6%,transparent)] p-3"
          >
            <p className="text-heading-16 font-heading text-red-1000">{errorTitle}</p>
            {errorDescription ? (
              <p className="text-copy-14 text-red-900">{errorDescription}</p>
            ) : null}
            {onRetry ? (
              <button
                type="button"
                onClick={onRetry}
                className="self-start text-label-14 font-medium text-blue-700 transition-colors duration-state ease-standard hover:text-blue-800"
              >
                Try again
              </button>
            ) : null}
          </div>
        ) : null}

        {status === 'empty' ? (
          <div className={cn('flex flex-col gap-2 text-center', compact ? 'py-2' : 'py-4')}>
            <p className="text-heading-16 font-heading text-gray-1000">{emptyTitle}</p>
            <p className="text-copy-14 text-gray-900">{emptyDescription}</p>
          </div>
        ) : null}

        {status === 'ready' && agents.length > 0 ? (
          <ul
            className={cn(
              'grid grid-cols-1 sm:grid-cols-2',
              compact ? 'gap-2' : 'gap-3',
            )}
            data-testid="agent-picker-grid"
          >
            {agents.map((agent) => (
              <li key={agent.id}>
                <AgentCard
                  agent={agent}
                  selected={selectedId === agent.id}
                  onSelect={onSelect}
                  compact={compact}
                />
              </li>
            ))}
          </ul>
        ) : null}

        {customLaunchVisible && onCustomLaunchChange ? (
          <div
            className={cn(
              'flex flex-col gap-2',
              status === 'ready' && agents.length > 0
                ? compact
                  ? 'mt-0 border-t border-gray-alpha-400 pt-2'
                  : 'mt-1 border-t border-gray-alpha-400 pt-3'
                : undefined,
            )}
          >
            <CustomLaunchField
              value={customLaunchValue}
              onChange={onCustomLaunchChange}
              compact={compact}
            />
          </div>
        ) : null}
      </div>
    </div>
  );
}

function AgentCard({
  agent,
  selected,
  onSelect,
  compact,
}: {
  agent: AgentPickerItem;
  selected: boolean;
  onSelect?: (id: string) => void;
  compact: boolean;
}) {
  const selectable = agent.installed;

  // Outbound links must NOT be descendants of the select <button> (QC B2 /
  // nested interactive content). Card chrome is a div; selection is a sibling
  // button that covers the identity region only.
  return (
    <div
      data-testid={`agent-card-${agent.id}`}
      data-installed={selectable ? 'true' : 'false'}
      className={cn(
        'flex w-full flex-col rounded-control border',
        compact ? 'p-2' : 'p-3',
        selectable
          ? selected
            ? 'border-blue-700 bg-blue-700/8'
            : 'border-gray-alpha-400 bg-background-100'
          : 'border-gray-alpha-400 bg-background-100 opacity-60',
      )}
    >
      {selectable ? (
        <button
          type="button"
          onClick={() => onSelect?.(agent.id)}
          aria-pressed={selected}
          data-testid={`agent-card-select-${agent.id}`}
          className={cn(
            'flex w-full items-start justify-between gap-2 rounded-sm text-left transition-colors duration-state ease-standard',
            'hover:bg-gray-alpha-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700',
          )}
        >
          <AgentCardIdentity agent={agent} selected={selected} />
        </button>
      ) : (
        <div className="flex w-full items-start justify-between gap-2">
          <AgentCardIdentity agent={agent} selected={selected} />
        </div>
      )}

      <div className="mt-3 flex flex-wrap items-center gap-x-3 gap-y-1">
        {agent.installed ? (
          <Badge variant="running" tone="soft" data-testid={`agent-card-installed-badge-${agent.id}`}>
            Installed
          </Badge>
        ) : (
          <span className="text-copy-13 text-gray-700">Not installed</span>
        )}
        {agent.installUrl ? (
          <OutboundLink href={agent.installUrl} label="Install" />
        ) : null}
        {agent.docsUrl ? (
          <OutboundLink href={agent.docsUrl} label="Docs" />
        ) : null}
      </div>
    </div>
  );
}

function AgentCardIdentity({
  agent,
  selected,
}: {
  agent: AgentPickerItem;
  selected: boolean;
}) {
  return (
    <>
      <div className="flex min-w-0 flex-col gap-0.5">
        <span className="truncate text-copy-14 font-medium text-gray-1000">
          {agent.name}
        </span>
        {agent.version ? (
          <span className="text-copy-13 text-gray-700">Version {agent.version}</span>
        ) : null}
        {agent.description ? (
          <span className="line-clamp-2 text-copy-13 text-gray-700">
            {agent.description}
          </span>
        ) : null}
      </div>
      <StatusDot installed={agent.installed} selected={selected} />
    </>
  );
}

/**
 * Selection affordance: hollow outline when installed-unselected; filled/lit
 * when selected; muted solid when not installed (non-selectable).
 */
function StatusDot({
  installed,
  selected,
}: {
  installed: boolean;
  selected: boolean;
}) {
  const label = installed
    ? selected
      ? 'Selected'
      : 'Installed'
    : 'Not installed';

  return (
    <span
      className="relative mt-0.5 inline-flex h-2.5 w-2.5 shrink-0"
      title={label}
      aria-hidden
      data-testid="agent-status-dot"
      data-dot={
        !installed ? 'muted' : selected ? 'lit' : 'hollow'
      }
    >
      <span
        className={cn(
          'absolute inset-0 rounded-full',
          !installed && 'bg-gray-500',
          installed &&
            selected &&
            'bg-green-700 ring-2 ring-blue-700 ring-offset-2 ring-offset-background-100',
          installed &&
            !selected &&
            'border-[1.5px] border-green-700 bg-transparent',
        )}
      />
    </span>
  );
}

function OutboundLink({ href, label }: { href: string; label: string }) {
  return (
    <a
      href={href}
      target="_blank"
      rel="noopener noreferrer"
      className="inline-flex items-center gap-1 text-label-14 font-medium leading-none text-blue-700 transition-colors hover:text-blue-800"
    >
      {label}
      <ArrowUpRight className="h-[1em] w-[1em] shrink-0" aria-hidden />
    </a>
  );
}

function CustomLaunchField({
  value,
  onChange,
  compact,
}: {
  value: string;
  onChange: (command: string) => void;
  compact: boolean;
}) {
  return (
    <div className={cn('flex flex-col', compact ? 'gap-1.5' : 'gap-2')} data-testid="agent-picker-custom-launch">
      <Label
        htmlFor="agent-picker-custom-launch"
        className="flex items-center gap-1.5 text-copy-14 text-gray-900"
      >
        <Terminal className="h-4 w-4 text-gray-700" aria-hidden />
        Use custom launch command
      </Label>
      <Input
        id="agent-picker-custom-launch"
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder="e.g. /usr/local/bin/my-agent"
      />
    </div>
  );
}
