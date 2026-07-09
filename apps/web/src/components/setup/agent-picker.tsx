/**
 * AgentPicker — presentational card grid for ACP agent selection.
 *
 * Placement (V1.101 locked): app-shared under `components/setup/`.
 * No wizard routing, no daemon client, no `@42ch/nexus-contracts` wire DTOs.
 * App hosts map scan results + static install/docs URLs into {@link AgentPickerItem}.
 *
 * Settings-reusable (DF-70): future Settings may mount the same component
 * without importing setup wizard pages.
 */

import { ExternalLink, Loader2, Terminal } from 'lucide-react';
import type { ReactNode } from 'react';

import { cn } from '@/lib/utils';
import { Input, Label } from '@42ch/nexus-ui';

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
}: AgentPickerProps) {
  const customLaunchVisible =
    showCustomLaunch ??
    (status === 'empty' ||
      status === 'error' ||
      (status === 'ready' && agents.length > 0));

  return (
    <div className={cn('flex flex-col gap-4', className)}>
      {header}

      <div
        className="flex min-h-[160px] flex-col gap-3 rounded-card border border-gray-alpha-400 bg-background-200 p-4"
        data-testid="agent-picker"
        data-status={status}
      >
        {status === 'loading' ? (
          <div
            className="flex flex-1 flex-col items-center justify-center gap-2 py-10"
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
          <div className="flex flex-col gap-2 py-4 text-center">
            <p className="text-heading-16 font-heading text-gray-1000">{emptyTitle}</p>
            <p className="text-copy-14 text-gray-900">{emptyDescription}</p>
          </div>
        ) : null}

        {status === 'ready' && agents.length > 0 ? (
          <ul
            className="grid grid-cols-1 gap-3 sm:grid-cols-2"
            data-testid="agent-picker-grid"
          >
            {agents.map((agent) => (
              <li key={agent.id}>
                <AgentCard
                  agent={agent}
                  selected={selectedId === agent.id}
                  onSelect={onSelect}
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
                ? 'mt-1 border-t border-gray-alpha-400 pt-3'
                : undefined,
            )}
          >
            <CustomLaunchField
              value={customLaunchValue}
              onChange={onCustomLaunchChange}
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
}: {
  agent: AgentPickerItem;
  selected: boolean;
  onSelect?: (id: string) => void;
}) {
  const selectable = agent.installed;
  const statusLabel = agent.installed ? 'Installed' : 'Not installed';

  const body = (
    <>
      <div className="flex items-start justify-between gap-2">
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
        <StatusDot installed={agent.installed} selected={selected} label={statusLabel} />
      </div>

      <div className="mt-3 flex flex-wrap items-center gap-x-3 gap-y-1">
        <span className="text-copy-13 text-gray-700">{statusLabel}</span>
        {agent.installUrl ? (
          <OutboundLink href={agent.installUrl} label="Install" />
        ) : null}
        {agent.docsUrl ? (
          <OutboundLink href={agent.docsUrl} label="Docs" />
        ) : null}
      </div>
    </>
  );

  if (selectable) {
    return (
      <button
        type="button"
        onClick={() => onSelect?.(agent.id)}
        aria-pressed={selected}
        data-testid={`agent-card-${agent.id}`}
        data-installed="true"
        className={cn(
          'flex w-full flex-col rounded-control border p-3 text-left transition-colors duration-state ease-standard',
          selected
            ? 'border-blue-700 bg-blue-700/8'
            : 'border-gray-alpha-400 bg-background-100 hover:bg-gray-alpha-100',
        )}
      >
        {body}
      </button>
    );
  }

  return (
    <div
      data-testid={`agent-card-${agent.id}`}
      data-installed="false"
      className="flex w-full flex-col rounded-control border border-gray-alpha-400 bg-background-100 p-3 text-left opacity-90"
    >
      {body}
    </div>
  );
}

function StatusDot({
  installed,
  selected,
  label,
}: {
  installed: boolean;
  selected: boolean;
  label: string;
}) {
  return (
    <span
      className="relative mt-0.5 inline-flex h-2.5 w-2.5 shrink-0"
      title={label}
      aria-hidden
    >
      <span
        className={cn(
          'absolute inset-0 rounded-full',
          installed ? 'bg-green-700' : 'bg-gray-500',
          selected && 'ring-2 ring-blue-700 ring-offset-2 ring-offset-background-100',
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
      className="inline-flex items-center gap-1 text-label-14 font-medium text-blue-700 transition-colors hover:text-blue-800"
      onClick={(e) => e.stopPropagation()}
    >
      {label}
      <ExternalLink className="h-3 w-3" aria-hidden />
    </a>
  );
}

function CustomLaunchField({
  value,
  onChange,
}: {
  value: string;
  onChange: (command: string) => void;
}) {
  return (
    <div className="flex flex-col gap-2" data-testid="agent-picker-custom-launch">
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
