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
import { memo, useCallback, useMemo, useRef, useState, type ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

import { cn } from '@/lib/utils';
import { Badge, Button, Input, Label } from '@42ch/nexus-ui';

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
  /**
   * Optional last-updated timestamp (forward-compat). Unsourced in V1.110 —
   * stays `undefined`; residual R-V110P2-001 covers the sort deferral.
   */
  lastUpdated?: string;
}

export type AgentPickerStatus = 'loading' | 'ready' | 'empty' | 'error';

export type AgentPickerDensity = 'default' | 'compact';

/**
 * Verify probe status for the custom-launch field.
 *
 * - `idle` — no probe run yet (default).
 * - `loading` — probe in flight; Verify button shows spinner + disabled.
 * - `success` — probe matched an installed agent; show success helper.
 * - `no-match` — scan reached the daemon but no installed agent matched the
 *   command; show the no-match helper.
 * - `error` — could not reach the daemon (transport failure); show the
 *   unreachable helper.
 */
export type AgentVerifyStatus = 'idle' | 'loading' | 'success' | 'no-match' | 'error';

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
  /** Verify a custom launch command probe. When omitted, the Verify button is hidden. */
  onVerify?: () => void;
  /** Status of the custom-launch verify probe (defaults to `idle`). */
  verifyStatus?: AgentVerifyStatus;
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
 * Common-agent priority — rendered first in the picker, in this order.
 *
 * Match keys are tried against `agent.id` (registry_agent_id, stable) first,
 * then case-insensitive `agent.name` (forward-compat for agents not yet in
 * the ACP CDN registry). The list reflects the user's locked priority order;
 * see `.mstar/iterations/v1.110/specs/agent-picker-ux-polish.md`.
 *
 * Order: Codex CLI, Claude Code, Cursor CLI, OpenCode, Hermes, Kimi Code,
 * Qoder, GitHub Copilot CLI, Pi, Kiro CLI. Do NOT reorder — this list is frozen.
 */
const COMMON_AGENT_PRIORITY: readonly string[] = [
  'codex-acp',           // Codex (user: "Codex CLI")
  'claude-acp',          // Claude Agent (user: "Claude Code")
  'cursor',              // Cursor (user: "Cursor CLI")
  'opencode',            // OpenCode
  'hermes',              // not in live registry — forward-compat (by id or name)
  'kimi',                // Kimi CLI (user: "Kimi Code")
  'qoder',               // Qoder CLI (user: "Qoder")
  'github-copilot-cli',  // GitHub Copilot (user: "GitHub Copilot CLI")
  'pi-acp',              // pi ACP (user: "Pi")
  'kiro',                // not in live registry — forward-compat (by id or name)
];

/**
 * Resolve the priority index for an agent. Tries `agent.id` (registry id,
 * stable) for an exact match first, then falls back to a case-insensitive
 * `agent.name` contains check (forward-compat for agents not yet in the ACP
 * CDN registry, e.g. Hermes/Kiro). Returns `-1` when no key matches.
 */
function findPriorityIndex(agent: AgentPickerItem): number {
  const lowerName = agent.name.toLowerCase();
  for (let i = 0; i < COMMON_AGENT_PRIORITY.length; i++) {
    const key = COMMON_AGENT_PRIORITY[i]!;
    const lowerKey = key.toLowerCase();
    if (agent.id === key || lowerName.includes(lowerKey)) {
      return i;
    }
  }
  return -1;
}

/**
 * Partition agents into `common` (matching {@link COMMON_AGENT_PRIORITY} by
 * id first then name, sorted by priority index) and `rest` (remaining agents
 * in stable registry order — the input array order). V1.110 does NOT sort
 * `rest` by `lastUpdated`; residual R-V110P2-001 covers that deferral.
 */
function partitionAgentsByPriority(agents: AgentPickerItem[]): {
  common: AgentPickerItem[];
  rest: AgentPickerItem[];
} {
  const rest: AgentPickerItem[] = [];
  const byPriority = new Map<number, AgentPickerItem[]>();
  for (const agent of agents) {
    const priorityIndex = findPriorityIndex(agent);
    if (priorityIndex === -1) {
      rest.push(agent);
    } else {
      const bucket = byPriority.get(priorityIndex);
      if (bucket) {
        bucket.push(agent);
      } else {
        byPriority.set(priorityIndex, [agent]);
      }
    }
  }
  const common: AgentPickerItem[] = [];
  for (let i = 0; i < COMMON_AGENT_PRIORITY.length; i++) {
    const bucket = byPriority.get(i);
    if (bucket) common.push(...bucket);
  }
  return { common, rest };
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
  onVerify,
  verifyStatus = 'idle',
  errorTitle,
  errorDescription,
  onRetry,
  header,
  className,
  loadingLabel,
  emptyTitle,
  emptyDescription,
  density = 'default',
}: AgentPickerProps) {
  const { t } = useTranslation('setup');
  const compact = density === 'compact';
  const [showRest, setShowRest] = useState(false);
  const { common: commonAgents, rest: restAgents } = useMemo(
    () => partitionAgentsByPriority(agents),
    [agents],
  );
  const gridClassName = cn(
    'grid grid-cols-1',
    !compact && 'sm:grid-cols-2',
    compact ? 'gap-2' : 'gap-3',
  );
  const customLaunchVisible =
    showCustomLaunch ??
    (status === 'empty' ||
      status === 'error' ||
      (status === 'ready' && agents.length > 0));
  const effectiveErrorTitle = errorTitle ?? t('agentPicker.error.title');
  const effectiveLoadingLabel = loadingLabel ?? t('agentPicker.loading');
  const effectiveEmptyTitle = emptyTitle ?? t('agentPicker.empty.title');
  const effectiveEmptyDescription = emptyDescription ?? t('agentPicker.empty.description');

  // Stabilise onSelect so that memoised AgentCards do not re-render when the
  // host re-renders for an unrelated reason (e.g. verifyStatus change). The ref
  // always points at the latest prop without changing callback identity.
  // R-V1108P1QC3-W001
  const onSelectRef = useRef(onSelect);
  onSelectRef.current = onSelect;
  const stableOnSelect = useCallback((id: string) => {
    onSelectRef.current?.(id);
  }, []);

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
            <span className="text-copy-14 text-gray-900">{effectiveLoadingLabel}</span>
          </div>
        ) : null}

        {status === 'error' ? (
          <div
            role="alert"
            className="flex flex-col gap-2 rounded-control border border-[color-mix(in_srgb,var(--color-red-700)_30%,transparent)] bg-[color-mix(in_srgb,var(--color-red-700)_6%,transparent)] p-3"
          >
            <p className="text-heading-16 font-heading text-red-1000">{effectiveErrorTitle}</p>
            {errorDescription ? (
              <p className="text-copy-14 text-red-900">{errorDescription}</p>
            ) : null}
            {onRetry ? (
              <button
                type="button"
                onClick={onRetry}
                className="self-start text-label-14 font-medium text-blue-700 transition-colors duration-state ease-standard hover:text-blue-800"
              >
                {t('agentPicker.tryAgain')}
              </button>
            ) : null}
          </div>
        ) : null}

        {status === 'empty' ? (
          <div className={cn('flex flex-col gap-2 text-center', compact ? 'py-2' : 'py-4')}>
            <p className="text-heading-16 font-heading text-gray-1000">{effectiveEmptyTitle}</p>
            <p className="text-copy-14 text-gray-900">{effectiveEmptyDescription}</p>
          </div>
        ) : null}

        {status === 'ready' && agents.length > 0 ? (
          <>
            <ul className={gridClassName} data-testid="agent-picker-grid">
              {commonAgents.map((agent) => (
                <li key={agent.id}>
                  <AgentCard
                    agent={agent}
                    selected={selectedId === agent.id}
                    onSelect={stableOnSelect}
                    compact={compact}
                  />
                </li>
              ))}
            </ul>
            {restAgents.length > 0 ? (
              <button
                type="button"
                onClick={() => setShowRest((expanded) => !expanded)}
                aria-expanded={showRest}
                aria-controls="agent-picker-rest"
                data-testid="agent-picker-more"
                className="self-start rounded-control text-label-14 font-medium text-blue-700 transition-colors duration-state ease-standard hover:text-blue-800 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700"
              >
                {showRest ? t('agentPicker.fewer') : t('agentPicker.more')}
              </button>
            ) : null}
            {restAgents.length > 0 && showRest ? (
              <ul
                id="agent-picker-rest"
                className={gridClassName}
                data-testid="agent-picker-grid-rest"
              >
                {restAgents.map((agent) => (
                  <li key={agent.id}>
                    <AgentCard
                      agent={agent}
                      selected={selectedId === agent.id}
                      onSelect={stableOnSelect}
                      compact={compact}
                    />
                  </li>
                ))}
              </ul>
            ) : null}
          </>
        ) : null}

        {customLaunchVisible && onCustomLaunchChange ? (
          <div
            className={cn(
              'flex flex-col gap-2',
              status === 'ready' && agents.length > 0
                ? compact
                  ? 'mt-2 border-t border-gray-alpha-400 pt-2'
                  : 'mt-3 border-t border-gray-alpha-400 pt-3'
                : undefined,
            )}
          >
            <CustomLaunchField
              value={customLaunchValue}
              onChange={onCustomLaunchChange}
              compact={compact}
              onVerify={onVerify}
              verifyStatus={verifyStatus}
            />
          </div>
        ) : null}
      </div>
    </div>
  );
}

const AgentCard = memo(function AgentCard({
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
  const { t } = useTranslation('setup');
  const selectable = agent.installed;

  // Outbound links must NOT be descendants of the select <button> (QC B2 /
  // nested interactive content). Card chrome is a div; selection is a sibling
  // button that covers the identity region only.
  return (
    <div
      data-testid={`agent-card-${agent.id}`}
      data-installed={selectable ? 'true' : 'false'}
      className={cn(
        'flex w-full flex-col rounded-control border border-gray-alpha-400 bg-background-100 transition-colors duration-state ease-standard',
        compact ? 'p-2' : 'p-3',
        // V1.108 FB-UI-007: hover paints the entire outer card surface.
        selectable && selected && 'border-blue-700 bg-blue-700/8',
        selectable && !selected && 'hover:bg-gray-alpha-100',
        !selectable && 'bg-background-200',
      )}
    >
      {selectable ? (
        <button
          type="button"
          onClick={() => onSelect?.(agent.id)}
          aria-pressed={selected}
          data-testid={`agent-card-select-${agent.id}`}
          className={cn(
            'flex w-full items-start justify-between gap-2 rounded-sm text-left',
            'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700',
          )}
        >
          <AgentCardIdentity agent={agent} selected={selected} />
        </button>
      ) : (
        <div className="flex w-full items-start justify-between gap-2">
          <AgentCardIdentity agent={agent} selected={selected} />
        </div>
      )}

      <div className="mt-3 flex flex-wrap items-center gap-3">
        {agent.installUrl ? (
          <OutboundLink href={agent.installUrl} label={t('agentPicker.install')} />
        ) : null}
        {agent.docsUrl ? (
          <OutboundLink href={agent.docsUrl} label={t('agentPicker.docs')} />
        ) : null}
      </div>
    </div>
  );
});

function AgentCardIdentity({
  agent,
  selected,
}: {
  agent: AgentPickerItem;
  selected: boolean;
}) {
  const { t } = useTranslation('setup');
  return (
    <>
      <div className="flex min-w-0 flex-1 flex-col gap-0.5">
        <div className="flex min-w-0 items-center gap-2">
          <span
            className={cn(
              'truncate text-copy-14 font-medium',
              agent.installed ? 'text-gray-1000' : 'text-gray-700',
            )}
          >
            {agent.name}
          </span>
          {agent.installed ? (
            <Badge
              variant="running"
              tone="soft"
              data-testid={`agent-card-installed-badge-${agent.id}`}
              className="shrink-0"
            >
              {t('agentPicker.installed')}
            </Badge>
          ) : (
            <Badge
              variant="neutral"
              tone="soft"
              data-testid={`agent-card-not-installed-badge-${agent.id}`}
              className="shrink-0"
            >
              {t('agentPicker.notInstalled')}
            </Badge>
          )}
        </div>
        {agent.version ? (
          <span className="text-copy-13 text-gray-700">{t('agentPicker.version', { version: agent.version })}</span>
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
 * Selection affordance: hollow gray outline when installed-unselected; filled
 * green when selected; muted solid gray when not installed (non-selectable).
 *
 * V1.108 FB-UI-006: unselected installed agents show hollow **gray** (not
 * green) so they do not imply validity before selection.
 */
function StatusDot({
  installed,
  selected,
}: {
  installed: boolean;
  selected: boolean;
}) {
  const { t } = useTranslation('setup');
  const label = installed
    ? selected
      ? t('agentPicker.status.selected')
      : t('agentPicker.status.installed')
    : t('agentPicker.status.notInstalled');

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
            'bg-green-700',
          installed &&
            !selected &&
            'border-[1.5px] border-gray-500 bg-transparent',
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
      aria-label={label}
      className="inline-flex items-center gap-1 text-label-14 font-medium leading-none text-blue-700 transition-colors hover:text-blue-800"
    >
      {label}
      <ArrowUpRight className="h-3 w-3" aria-hidden />
    </a>
  );
}

function CustomLaunchField({
  value,
  onChange,
  compact,
  onVerify,
  verifyStatus = 'idle',
}: {
  value: string;
  onChange: (command: string) => void;
  compact: boolean;
  onVerify?: () => void;
  verifyStatus?: AgentVerifyStatus;
}) {
  const { t } = useTranslation('setup');
  const canVerify = value.trim().length > 0 && verifyStatus !== 'loading';

  return (
    <div className={cn('flex flex-col', compact ? 'gap-1.5' : 'gap-2')} data-testid="agent-picker-custom-launch">
      <Label
        htmlFor="agent-picker-custom-launch"
        className="flex items-center gap-1.5 text-copy-14 text-gray-900"
      >
        <Terminal className="h-4 w-4 text-gray-700" aria-hidden />
        {t('agentPicker.customLaunch.label')}
      </Label>
      <div className="flex gap-2">
        <Input
          id="agent-picker-custom-launch"
          value={value}
          onChange={(e) => onChange(e.target.value)}
          placeholder={t('agentPicker.customLaunch.placeholder')}
          className="min-w-0 flex-1"
        />
        {onVerify ? (
          <Button
            type="button"
            variant="secondary"
            onClick={onVerify}
            disabled={!canVerify}
            data-testid="agent-picker-verify"
            className="shrink-0"
          >
            {verifyStatus === 'loading' ? (
              <>
                <Loader2 className="h-3.5 w-3.5 animate-spin" aria-hidden />
                {t('agentPicker.verifying')}
              </>
            ) : (
              t('agentPicker.verify')
            )}
          </Button>
        ) : null}
      </div>
      {verifyStatus === 'success' ? (
        <p
          className="text-copy-13 text-green-700"
          data-testid="agent-picker-verify-success"
          role="status"
        >
          {t('agentPicker.verifySuccess')}
        </p>
      ) : null}
      {verifyStatus === 'no-match' ? (
        <p
          className="text-copy-13 text-red-700"
          data-testid="agent-picker-verify-error"
          role="alert"
        >
          {t('agentPicker.verifyNoMatch')}
        </p>
      ) : null}
      {verifyStatus === 'error' ? (
        <p
          className="text-copy-13 text-red-700"
          data-testid="agent-picker-verify-error"
          role="alert"
        >
          {t('agentPicker.verifyError')}
        </p>
      ) : null}
    </div>
  );
}
