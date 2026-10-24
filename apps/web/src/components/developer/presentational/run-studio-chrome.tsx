/**
 * Presentational body of the Run Studio (plan v221-modules-rail T2).
 *
 * The product studio (`components/compute/run-studio.tsx`) owns every hook,
 * query, mutation, toast and i18n lookup; this module owns the markup so the
 * Design Studio fixture can render the run-pending / run-success / run-error
 * variants from static view models with literal copy.
 *
 * Boundary: no transport, no router, no i18n, no QueryClient. Every string
 * arrives through `copy`, data as view models, behavior as callbacks. The two
 * destructive confirm dialogs stay app-owned (they are mounted as siblings of
 * this tree); the chrome only renders their trigger buttons.
 */
import { useId } from 'react';

import { Play } from 'lucide-react';

import {
  ProposalSections,
  RunFormFields,
  RunStatusBadge,
  RunsTable,
  type ComputeProposals,
  type EntityPickerEntry,
  type InvocationSchema,
  type ProposalSectionsCopy,
  type RunFormCopy,
  type RunStatus,
  type RunsTableCopy,
  type RunTableRow,
} from '@42ch/nexus-ui';

import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import { Select } from '@/components/ui/select';
import { ErrorState, LoadingState } from '@/components/ui/states';

export interface RunStudioWorldView {
  options: readonly { world_id: string; title: string }[];
  value: string;
  onChange: (worldId: string) => void;
  isLoading: boolean;
  isError: boolean;
  onRetry: () => void;
}

export interface RunStudioFormView {
  schema: InvocationSchema | null | undefined;
  requiredKeyBlockTypes?: string[];
  values: Record<string, unknown>;
  onChange: (name: string, value: unknown) => void;
  entityEntries?: Record<string, EntityPickerEntry[]>;
  copy: RunFormCopy;
  /**
   * World-scoped KB graph feed (the entity pickers' source). `null` while no
   * World is selected — the form then renders without gating on that feed.
   */
  graph: { isLoading: boolean; isError: boolean; onRetry: () => void } | null;
}

export interface RunStudioAdvancedView {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  json: string;
  onJsonChange: (value: string) => void;
  /** Parse error shown under the textarea (caller-owned copy). */
  error: string | null;
}

export interface RunStudioRunView {
  pending: boolean;
  canRun: boolean;
  onRun: () => void;
  /** Deep-linked run-detail read: own loading / error states above the inspector. */
  detailLoading: boolean;
  detailError: boolean;
  onDetailRetry: () => void;
}

export interface RunStudioProposalView {
  /** Inspected run (fresh POST response or detail read); `null` → no inspector. */
  run: {
    /** Short correlation id, caller-shortened for display. */
    runIdShort: string;
    status: RunStatus;
    proposals?: ComputeProposals;
    error?: { code: string; message?: string };
  } | null;
  truncated: boolean;
  acceptPending: boolean;
  discardPending: boolean;
  onAccept: () => void;
  onDiscard: () => void;
}

export interface RunStudioRunsView {
  rows: RunTableRow[];
  copy: RunsTableCopy;
  isLoading: boolean;
  isError: boolean;
  onRetry: () => void;
  statusFilter: '' | RunStatus;
  onStatusFilterChange: (status: '' | RunStatus) => void;
  worldFilter: string;
  onWorldFilterChange: (worldId: string) => void;
  /** Clear history is World-scoped; `null` disables the affordance. */
  clearScopeWorldId: string | null;
  clearPending: boolean;
  onClear: () => void;
  onOpenRun: (runId: string) => void;
  hasNextPage: boolean;
  isFetchingNextPage: boolean;
  onLoadMore: () => void;
}

export interface RunStudioCopy {
  title: string;
  worldLabel: string;
  worldPlaceholder: string;
  branchNote: string;
  noWorldHint: string;
  worldsErrorTitle: string;
  worldsErrorDescription: string;
  graphErrorTitle: string;
  graphErrorDescription: string;
  graphLoading: string;
  advancedJson: string;
  advancedJsonWarning: string;
  runButton: string;
  runPending: string;
  runLoadingDetail: string;
  runErrorTitle: string;
  runErrorDescription: string;
  runsTitle: string;
  statusFilter: string;
  allStatuses: string;
  worldFilter: string;
  allWorlds: string;
  clearHistory: string;
  clearDisabledHint: string;
  runsLoading: string;
  runsErrorTitle: string;
  runsErrorDescription: string;
  loadMore: string;
  loadingMore: string;
  /** Product status options for the Runs filter (labels caller-owned). */
  statusOptions: readonly { value: RunStatus; label: string }[];
  /** Product status label per wire status (Needs review = `succeeded`). */
  statusLabel: (status: RunStatus) => string;
  accept: string;
  discard: string;
  acceptAria: string;
  discardAria: string;
  acceptNote: string;
  stoppedTitle: string;
  failedTitle: string;
  stoppedDescription: string;
  failedDescription: (reason: string) => string;
  failedCodeLabel: string;
  appliedNote: string;
  discardedNote: string;
  runningNote: string;
  proposalsCopy: ProposalSectionsCopy;
}

export interface RunStudioChromeProps {
  copy: RunStudioCopy;
  world: RunStudioWorldView;
  form: RunStudioFormView;
  advanced: RunStudioAdvancedView;
  run: RunStudioRunView;
  proposal: RunStudioProposalView;
  runs: RunStudioRunsView;
}

/** Guided form → Run → proposal inspector → Runs history. */
export function RunStudioChrome({
  copy,
  world,
  form,
  advanced,
  run,
  proposal,
  runs,
}: RunStudioChromeProps) {
  // `useId` — the gallery mounts this chrome three times; static ids would
  // collide and every label after the first would point at frame 1's control.
  const worldSelectId = useId();
  const statusFilterId = useId();
  const worldFilterId = useId();

  return (
    <div className="flex flex-col gap-4 border-t border-gray-alpha-300 pt-4" data-testid="run-studio">
      <p className="text-label-12 uppercase tracking-wide text-gray-700">{copy.title}</p>

      {/* Product chrome: World selector (always present, spec §3). A data-feed
          failure here must not collapse into a silent empty select (qc3 W-3
          fix) — surface the ErrorState with a retry instead. */}
      {world.isError ? (
        <ErrorState
          title={copy.worldsErrorTitle}
          description={copy.worldsErrorDescription}
          onRetry={world.onRetry}
        />
      ) : (
        <div className="flex max-w-xs flex-col gap-1.5">
          <Label htmlFor={worldSelectId}>{copy.worldLabel}</Label>
          <Select
            id={worldSelectId}
            value={world.value}
            onChange={(event) => world.onChange(event.target.value)}
            disabled={world.isLoading}
            data-testid="run-studio-world"
          >
            <option value="" disabled>
              {copy.worldPlaceholder}
            </option>
            {world.options.map((w) => (
              <option key={w.world_id} value={w.world_id}>
                {w.title}
              </option>
            ))}
          </Select>
          <p className="text-copy-13 text-gray-700">
            {world.value ? copy.branchNote : copy.noWorldHint}
          </p>
        </div>
      )}

      {form.graph?.isError ? (
        <ErrorState
          title={copy.graphErrorTitle}
          description={copy.graphErrorDescription}
          onRetry={form.graph.onRetry}
        />
      ) : form.graph?.isLoading ? (
        <LoadingState label={copy.graphLoading} />
      ) : (
        <RunFormFields
          schema={form.schema}
          requiredKeyBlockTypes={form.requiredKeyBlockTypes}
          values={form.values}
          onChange={form.onChange}
          entityEntries={form.entityEntries}
          copy={form.copy}
          idPrefix="run-form"
        />
      )}

      {/* Advanced JSON escape hatch — collapsed by default; form syncs best-effort. */}
      <details
        open={advanced.open}
        onToggle={(event) => advanced.onOpenChange(event.currentTarget.open)}
        data-testid="run-studio-advanced-json"
        className="rounded-control border border-gray-alpha-300 bg-background-100 p-3"
      >
        <summary className="cursor-pointer text-label-14 text-gray-1000">
          {copy.advancedJson}
        </summary>
        <p className="mt-2 text-copy-13 text-gray-700">{copy.advancedJsonWarning}</p>
        <textarea
          aria-label={copy.advancedJson}
          value={advanced.json}
          onChange={(event) => advanced.onJsonChange(event.target.value)}
          spellCheck={false}
          className="mt-2 h-40 w-full rounded-control border border-gray-alpha-400 bg-background-200 p-3 font-mono text-copy-13 text-gray-1000 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-1000 dark:focus-visible:ring-blue-700"
          data-testid="run-studio-json-textarea"
        />
        {advanced.error && (
          <p
            className="mt-2 text-copy-13 text-red-700"
            role="alert"
            data-testid="run-studio-json-error"
          >
            {advanced.error}
          </p>
        )}
      </details>

      <div className="flex items-center gap-3">
        <Button
          type="button"
          variant="primary"
          onClick={run.onRun}
          disabled={!run.canRun}
          data-testid="run-studio-run"
        >
          <Play className="h-4 w-4" aria-hidden />
          {run.pending ? copy.runPending : copy.runButton}
        </Button>
      </div>

      {run.detailLoading ? (
        <LoadingState label={copy.runLoadingDetail} />
      ) : run.detailError ? (
        // A deep-linked Run whose detail read fails (cleared by Clear history,
        // deleted, or transport down) must SAY so — rendering nothing hides the
        // failure behind an empty space where the inspector should be.
        <ErrorState
          title={copy.runErrorTitle}
          description={copy.runErrorDescription}
          onRetry={run.onDetailRetry}
        />
      ) : (
        proposal.run && (
          <RunInspector
            run={proposal.run}
            truncated={proposal.truncated}
            acceptPending={proposal.acceptPending}
            discardPending={proposal.discardPending}
            onAccept={proposal.onAccept}
            onDiscard={proposal.onDiscard}
            copy={copy}
          />
        )
      )}

      <div className="flex flex-col gap-2">
        <div className="flex flex-wrap items-end justify-between gap-2">
          <p className="text-label-12 uppercase tracking-wide text-gray-700">{copy.runsTitle}</p>
          {/* Filter chrome (spec §4 + plan Global Constraints). Status + World
              narrow the module-scoped history; Clear history is World-scoped
              because the daemon DELETE requires a World. */}
          <div className="flex flex-wrap items-end gap-2">
            <div className="flex flex-col gap-1">
              <Label htmlFor={statusFilterId} className="text-label-12 text-gray-700">
                {copy.statusFilter}
              </Label>
              <Select
                id={statusFilterId}
                value={runs.statusFilter}
                onChange={(event) =>
                  runs.onStatusFilterChange(event.target.value as '' | RunStatus)
                }
                className="w-44"
                data-testid="run-runs-status-filter"
              >
                <option value="">{copy.allStatuses}</option>
                {copy.statusOptions.map((option) => (
                  <option key={option.value} value={option.value}>
                    {option.label}
                  </option>
                ))}
              </Select>
            </div>
            <div className="flex flex-col gap-1">
              <Label htmlFor={worldFilterId} className="text-label-12 text-gray-700">
                {copy.worldFilter}
              </Label>
              <Select
                id={worldFilterId}
                value={runs.worldFilter}
                onChange={(event) => runs.onWorldFilterChange(event.target.value)}
                disabled={world.isLoading || world.isError}
                className="w-44"
                data-testid="run-runs-world-filter"
              >
                <option value="">{copy.allWorlds}</option>
                {world.options.map((w) => (
                  <option key={w.world_id} value={w.world_id}>
                    {w.title}
                  </option>
                ))}
              </Select>
            </div>
            {/* Clear history (V1.147 P3 T2) — server-backed DELETE. The daemon
                REQUIRES `world_id`, so the button is gated on the World filter
                (never a world-wide purge); running / needs-review rows are
                always kept. */}
            <div className="flex items-end">
              <Button
                type="button"
                variant="secondary"
                size="small"
                onClick={runs.onClear}
                disabled={runs.clearScopeWorldId === null || runs.clearPending}
                title={runs.clearScopeWorldId === null ? copy.clearDisabledHint : undefined}
                aria-label={copy.clearHistory}
                data-testid="run-runs-clear"
              >
                {copy.clearHistory}
              </Button>
            </div>
          </div>
        </div>

        {/* Data-feed errors must not collapse into "No runs yet" (qc3 W-3
            fix): a failed runs query renders ErrorState, and a loading list
            renders a placeholder instead of flashing the empty state. */}
        {runs.isLoading ? (
          <LoadingState label={copy.runsLoading} />
        ) : runs.isError ? (
          <ErrorState
            title={copy.runsErrorTitle}
            description={copy.runsErrorDescription}
            onRetry={runs.onRetry}
          />
        ) : (
          <RunsTable rows={runs.rows} copy={runs.copy} onOpenRun={runs.onOpenRun} />
        )}

        {/* Cursor pagination (qc1 W-002 / qc3 W-2 fix): runs beyond the first
            page — including older Needs-review rows — must stay reachable. */}
        {runs.hasNextPage && (
          <Button
            type="button"
            variant="tertiary"
            size="small"
            onClick={runs.onLoadMore}
            disabled={runs.isFetchingNextPage}
            data-testid="run-runs-load-more"
          >
            {runs.isFetchingNextPage ? copy.loadingMore : copy.loadMore}
          </Button>
        )}
      </div>
    </div>
  );
}

/** Result inspector for one Run — succeeded (proposals + Accept/Discard), failed, or terminal. */
function RunInspector({
  run,
  truncated,
  acceptPending,
  discardPending,
  onAccept,
  onDiscard,
  copy,
}: {
  run: NonNullable<RunStudioProposalView['run']>;
  truncated: boolean;
  acceptPending: boolean;
  discardPending: boolean;
  onAccept: () => void;
  onDiscard: () => void;
  copy: RunStudioCopy;
}) {
  const isLimit =
    run.error !== undefined &&
    ['compute_fuel_exhausted', 'compute_wall_time_exceeded', 'compute_memory_cap_exceeded'].includes(
      run.error.code,
    );

  return (
    <div
      className="flex flex-col gap-4 rounded-card border border-gray-alpha-300 bg-background-100 p-4"
      data-testid="run-inspector"
    >
      <div className="flex flex-wrap items-center gap-3">
        <RunStatusBadge status={run.status} label={copy.statusLabel(run.status)} />
        <span className="text-copy-13-mono text-gray-700">{run.runIdShort}</span>
      </div>

      {run.status === 'succeeded' && run.proposals && (
        <>
          <ProposalSections
            proposals={run.proposals}
            truncated={truncated}
            copy={copy.proposalsCopy}
          />
          <div className="flex flex-wrap items-center gap-3">
            <Button
              type="button"
              variant="primary"
              onClick={onAccept}
              disabled={acceptPending}
              aria-label={copy.acceptAria}
              data-testid="run-inspector-accept"
            >
              {copy.accept}
            </Button>
            <Button
              type="button"
              variant="secondary"
              onClick={onDiscard}
              disabled={discardPending}
              aria-label={copy.discardAria}
              data-testid="run-inspector-discard"
            >
              {copy.discard}
            </Button>
            <p className="text-copy-13 text-gray-700">{copy.acceptNote}</p>
          </div>
        </>
      )}

      {run.status === 'failed' && run.error && (
        <div
          data-testid="run-inspector-failed"
          className="rounded-card border border-error-surface-border bg-error-surface p-4"
        >
          <p className="text-label-14 font-medium text-gray-1000">
            {isLimit ? copy.stoppedTitle : copy.failedTitle}
          </p>
          <p className="mt-1 text-copy-13 text-gray-700">
            {isLimit
              ? copy.stoppedDescription
              : copy.failedDescription(run.error.message ?? '')}
          </p>
          <p className="mt-2 text-copy-13-mono text-gray-700">
            {copy.failedCodeLabel}: {run.error.code}
          </p>
        </div>
      )}

      {run.status === 'applied' && (
        <p className="text-copy-13 text-gray-700" data-testid="run-inspector-applied-note">
          {copy.appliedNote}
        </p>
      )}
      {run.status === 'discarded' && (
        <p className="text-copy-13 text-gray-700" data-testid="run-inspector-discarded-note">
          {copy.discardedNote}
        </p>
      )}
      {run.status === 'running' && (
        <p className="text-copy-13 text-gray-700" data-testid="run-inspector-running-note">
          {copy.runningNote}
        </p>
      )}
    </div>
  );
}
