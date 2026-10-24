import { useState, type ReactNode } from 'react';

import {
  type ComputeProposals,
  type EntityPickerEntry,
  type ProposalSectionsCopy,
  type RunFormCopy,
  type RunStatus,
  type RunsTableCopy,
  type RunTableRow,
} from '@42ch/nexus-ui';

import {
  ModuleDetailChrome,
  ModulesPageChrome,
  type ModuleDetailCopy,
  type ModuleDetailView,
  type ModuleManifestView,
  type ModulesListView,
  type ModulesPageCopy,
  type ModuleSummaryView,
} from '@web-developer/modules-page-chrome';
import {
  RunStudioChrome,
  type RunStudioChromeProps,
  type RunStudioCopy,
} from '@web-developer/run-studio-chrome';

/**
 * Studio fixture for the `/modules` surface (plan v221-modules-rail T3).
 *
 * Renders the three chrome roots from the developer generation —
 * `ModulesPageChrome`, `ModuleDetailChrome`, `RunStudioChrome` — from static
 * view models and literal English copy:
 *
 *   - list (ready, one selected)
 *   - detail (manifest fields + the `selectTitle` prompt)
 *   - run-pending / run-success (proposal visible) / run-error (detail read error)
 *
 * The chrome is pure presentation: no transport, no router, no i18n, no
 * QueryClient. Every string below is literal (Studio is developer-auxiliary
 * and excluded from i18n catalogs); every value is fake. Toggle the Studio
 * theme to check light + dark — there is no per-fixture theme prop because
 * every class is token-backed.
 */

/* ------------------------------------------------------------------ */
/*  Caller-owned copy (mirrors locales/en/modules.json)                 */
/* ------------------------------------------------------------------ */

const PAGE_COPY: ModulesPageCopy = {
  title: 'Compute Modules',
  description: 'Installed compute modules that power the local runtime.',
  refreshAria: 'Refresh modules',
  refresh: 'Refresh',
  loading: 'Loading modules…',
  listTitle: 'Installed modules',
  listAriaLabel: 'Installed modules',
  errorTitle: 'Could not load modules',
  errorDescription: 'Could not load compute modules.',
  engineUnavailableTitle: 'Orchestration engine not running',
  engineUnavailableDescription: 'Start the daemon orchestration engine, then try again.',
  emptyTitle: 'No modules installed',
  emptyDescription: 'Compute modules will appear here once the runtime registers them.',
  selectTitle: 'Select a module',
  selectDescription: 'Choose a module from the list to inspect its manifest.',
};

const DETAIL_COPY: ModuleDetailCopy = {
  title: 'Module manifest',
  version: (version) => `v${version}`,
  loading: 'Loading module detail…',
  errorTitle: 'Could not load module detail',
  errorDescription: 'Could not load module detail.',
  engineUnavailableTitle: 'Orchestration engine not running',
  engineUnavailableDescription: 'Start the daemon orchestration engine, then try again.',
  abiVersion: 'ABI version',
  author: 'Author',
  computeExport: 'Compute export',
  initExport: 'Init export',
  maxFuel: 'Max fuel',
  maxMemory: 'Max memory (MiB)',
  maxWallTime: 'Max wall time (ms)',
  requiredKeyBlockTypes: 'Required knowledge entry types',
  hostFunctions: 'Host functions',
  battleReportKind: 'Battle report kind',
  schemas: 'Schemas',
  keyBlockAttributes: 'Knowledge entry attributes',
  keyBlockState: 'Knowledge entry state',
  invocation: 'Invocation',
  battleReport: 'Battle report',
};

const FORM_COPY: RunFormCopy = {
  emptyTitle: 'Can’t run this module',
  emptyDescription:
    'Manifest is missing fields needed to build a form; use another module or fix the install.',
  unsupportedFieldNote:
    'Not available in the guided form — open Advanced: edit invocation JSON below.',
  entityPlaceholder: 'Choose a character',
  selectPlaceholder: 'Choose a value',
  entityEmptyTitle: 'No characters to run',
  entityEmptyDescription: 'Add character knowledge entries in this World, then return.',
};

const PROPOSAL_COPY: ProposalSectionsCopy = {
  reportTitle: 'Report',
  knowledgeUpdatesTitle: 'Knowledge updates',
  timelineEventsTitle: 'Timeline events',
  newKnowledgeTitle: 'New knowledge',
  truncatedNote: 'This preview is shortened — open the Run for the full result.',
  untitledEventLabel: 'Untitled event',
  affectedEntriesLabel: (count) => `Affects ${count} ${count === 1 ? 'entry' : 'entries'}`,
  newEntryLabel: 'New entry',
};

const RUNS_COPY: RunsTableCopy = {
  moduleColumn: 'Module',
  worldColumn: 'World',
  statusColumn: 'Status',
  startedColumn: 'Started',
  finishedColumn: 'Finished',
  runIdColumn: 'Run ID',
  openRunLabel: 'Open',
  copyIdLabel: 'Copy',
  emptyTitle: 'No runs yet',
  emptyDescription: 'Run this module to see history here.',
};

const STATUS_LABELS: Record<RunStatus, string> = {
  running: 'Running',
  succeeded: 'Needs review',
  failed: 'Failed',
  applied: 'Applied',
  discarded: 'Discarded',
};

const RUN_COPY: RunStudioCopy = {
  title: 'Run Studio',
  worldLabel: 'World',
  worldPlaceholder: 'Choose a World',
  branchNote: 'Branch: root (default).',
  noWorldHint: 'Choose a World to run this module.',
  worldsErrorTitle: 'Could not load Worlds',
  worldsErrorDescription: 'Worlds are unavailable right now. Try again.',
  graphErrorTitle: 'Could not load World characters',
  graphErrorDescription: 'Character entries are unavailable right now. Try again.',
  graphLoading: 'Loading characters…',
  advancedJson: 'Advanced: edit invocation JSON',
  advancedJsonWarning: 'Form fields sync best-effort when you edit the raw JSON.',
  runButton: 'Run',
  runPending: 'Running…',
  runLoadingDetail: 'Loading Run…',
  runErrorTitle: 'Could not load this Run',
  runErrorDescription:
    'Run detail is unavailable right now — the Run may have been cleared, or the daemon is unreachable. Try again.',
  runsTitle: 'Runs',
  statusFilter: 'Filter by status',
  allStatuses: 'All statuses',
  worldFilter: 'Filter by World',
  allWorlds: 'All Worlds',
  clearHistory: 'Clear history',
  clearDisabledHint: 'Choose a World to clear history.',
  runsLoading: 'Loading Runs…',
  runsErrorTitle: 'Could not load Runs',
  runsErrorDescription: 'Runs history is unavailable right now. Try again.',
  loadMore: 'Load more',
  loadingMore: 'Loading…',
  statusOptions: [
    { value: 'running', label: STATUS_LABELS.running },
    { value: 'succeeded', label: STATUS_LABELS.succeeded },
    { value: 'applied', label: STATUS_LABELS.applied },
    { value: 'discarded', label: STATUS_LABELS.discarded },
    { value: 'failed', label: STATUS_LABELS.failed },
  ],
  statusLabel: (status) => STATUS_LABELS[status],
  accept: 'Accept',
  discard: 'Discard',
  acceptAria: 'Accept all proposals for this Run',
  discardAria: 'Discard this Run’s proposals',
  acceptNote: 'Accept commits all proposals from this Run atomically.',
  stoppedTitle: 'Run stopped (limit)',
  failedTitle: 'Run failed',
  stoppedDescription: 'The module hit a safety limit; simplify inputs or retry. World unchanged.',
  failedDescription: (reason) =>
    `The module returned an error: ${reason}. World unchanged. Open Run for details.`,
  failedCodeLabel: 'Error code',
  appliedNote: 'Proposals applied to the World.',
  discardedNote: 'Proposals discarded. World unchanged.',
  runningNote: 'This Run is still executing.',
  formCopy: FORM_COPY,
  proposalsCopy: PROPOSAL_COPY,
  runsTableCopy: RUNS_COPY,
};

/* ------------------------------------------------------------------ */
/*  Fake data                                                          */
/* ------------------------------------------------------------------ */

const CHARACTERS: EntityPickerEntry[] = [
  { id: 'char-aria', title: 'Aria', subtitle: 'Level 3 · ATK 12' },
  { id: 'char-brann', title: 'Brann', subtitle: 'Level 2 · ATK 9' },
];

/** Mirrors modules/basic-combat/manifest.json schemas.invocation. */
const BASIC_COMBAT_SCHEMA = {
  type: 'object',
  properties: {
    attacker_id: { type: 'string' },
    defender_id: { type: 'string' },
  },
};

const BASIC_COMBAT_MANIFEST: ModuleManifestView = {
  module_id: 'basic-combat',
  name: 'Basic Combat',
  version: '1.0.0',
  nexus_abi_version: 3,
  compute_export: 'compute',
  init_export: 'init',
  description: 'Simple combat resolution between two characters.',
  author: 'Nexus Core',
  host_functions: ['read_key_block', 'write_key_block', 'emit_timeline_event'],
  required_key_block_types: ['character'],
  battle_report_kind: 'combat',
  max_fuel: 1_000_000,
  max_memory_mib: 64,
  max_wall_time_ms: 5_000,
  schemas: {
    key_block_attributes: { type: 'object', properties: { hp: { type: 'integer' } } },
    key_block_state: { type: 'object', properties: { current_hp: { type: 'integer' } } },
    invocation: BASIC_COMBAT_SCHEMA,
    battle_report: { type: 'object', properties: { damage: { type: 'integer' } } },
  },
};

const MODULE_SUMMARIES: ModuleSummaryView[] = [
  {
    module_id: 'basic-combat',
    name: 'Basic Combat',
    version: '1.0.0',
    description: 'Simple combat resolution between two characters.',
    required_key_block_types: ['character'],
    battle_report_kind: 'combat',
  },
  {
    module_id: 'scouting-party',
    name: 'Scouting Party',
    version: '0.4.2',
    description: 'Reveals adjacent regions for the travelling party.',
    required_key_block_types: ['character', 'region'],
  },
];

const READY_LIST: ModulesListView = {
  items: MODULE_SUMMARIES,
  isLoading: false,
  isError: false,
  isRefreshing: false,
  onRefresh: noop,
};

const MANIFEST_DETAIL: ModuleDetailView = {
  data: BASIC_COMBAT_MANIFEST,
  isLoading: false,
  isError: false,
  onRetry: noop,
};

const SUCCESS_PROPOSALS: ComputeProposals = {
  schema_version: 1,
  state_delta: [
    {
      op: 'sub',
      path: 'character.current_hp',
      target_key_block_id: 'char-brann',
      value: { amount: 6 },
    },
  ],
  timeline_events: [
    {
      title: 'Aria strikes Brann',
      summary: 'Brann takes 6 damage and staggers back.',
      affected_key_block_ids: ['char-aria', 'char-brann'],
    },
  ],
  new_key_blocks: [{ title: 'Bruised rib', kind: 'injury', severity: 'minor' }],
  battle_report: { kind: 'combat', damage: 6, defender_hp_after: 14 },
};

const RUN_ROWS: RunTableRow[] = [
  {
    runId: 'run_9f3a2c',
    moduleName: 'Basic Combat',
    moduleVersion: '1.0.0',
    worldTitle: 'The Lost City',
    status: 'running',
    statusLabel: STATUS_LABELS.running,
    startedAt: '2026-07-31 14:02',
  },
  {
    runId: 'run_71bd04',
    moduleName: 'Basic Combat',
    moduleVersion: '1.0.0',
    worldTitle: 'The Lost City',
    status: 'succeeded',
    statusLabel: STATUS_LABELS.succeeded,
    startedAt: '2026-07-31 13:47',
    finishedAt: '2026-07-31 13:47',
  },
];

function noop() {
  /* Studio fixture only — actions have nowhere to go. */
}

/* ------------------------------------------------------------------ */
/*  View models                                                        */
/* ------------------------------------------------------------------ */

/** One Run Studio base — the three run variants differ only in run/proposal. */
function studioProps(
  run: RunStudioChromeProps['run'],
  proposal: RunStudioChromeProps['proposal'],
): RunStudioChromeProps {
  return {
    copy: RUN_COPY,
    world: {
      options: [
        { world_id: 'w-lost-city', title: 'The Lost City' },
        { world_id: 'w-echo', title: 'Echo Protocol' },
      ],
      value: 'w-lost-city',
      onChange: noop,
      isLoading: false,
      isError: false,
      onRetry: noop,
    },
    form: {
      schema: BASIC_COMBAT_SCHEMA,
      requiredKeyBlockTypes: ['character'],
      values: { attacker_id: 'char-aria', defender_id: 'char-brann' },
      onChange: noop,
      entityEntries: { attacker_id: CHARACTERS, defender_id: CHARACTERS },
      copy: FORM_COPY,
      graph: null,
    },
    advanced: {
      open: false,
      onOpenChange: noop,
      json: '{\n  "attacker_id": "char-aria",\n  "defender_id": "char-brann"\n}',
      onJsonChange: noop,
      error: null,
    },
    run,
    proposal,
    runs: {
      rows: RUN_ROWS,
      copy: RUNS_COPY,
      isLoading: false,
      isError: false,
      onRetry: noop,
      statusFilter: '',
      onStatusFilterChange: noop,
      worldFilter: 'w-lost-city',
      onWorldFilterChange: noop,
      clearScopeWorldId: 'w-lost-city',
      clearPending: false,
      onClear: noop,
      onOpenRun: noop,
      hasNextPage: false,
      isFetchingNextPage: false,
      onLoadMore: noop,
    },
  };
}

const IDLE_RUN: RunStudioChromeProps['run'] = {
  pending: false,
  canRun: true,
  onRun: noop,
  detailLoading: false,
  detailError: false,
  onDetailRetry: noop,
};

const IDLE_PROPOSAL: RunStudioChromeProps['proposal'] = {
  run: null,
  truncated: false,
  acceptPending: false,
  discardPending: false,
  onAccept: noop,
  onDiscard: noop,
};

/* ------------------------------------------------------------------ */
/*  Frames                                                             */
/* ------------------------------------------------------------------ */

function Frame({
  title,
  description,
  testId,
  children,
}: {
  title: string;
  description: string;
  testId: string;
  children: ReactNode;
}) {
  return (
    <div
      className="mb-8 rounded-card border border-gray-alpha-300 bg-background-100 p-4"
      data-testid={testId}
    >
      <h4 className="text-heading-16 font-heading text-gray-1000 mb-1">{title}</h4>
      <p className="text-copy-13 text-gray-700 mb-4">{description}</p>
      {children}
    </div>
  );
}

export function ModulesRailFixtures() {
  // Selection is real state so clicking a row exercises the chrome's own
  // selection highlight; the initial value is the fixture's "one selected".
  const [selectedId, setSelectedId] = useState<string | null>('basic-combat');

  return (
    <div data-testid="modules-rail-fixtures">
      <Frame
        title="List — ready, one selected"
        description="The page shell with both installed modules and Basic Combat selected; the detail slot hosts the manifest panel."
        testId="modules-rail-fixture-list"
      >
        <ModulesPageChrome
          copy={PAGE_COPY}
          list={READY_LIST}
          selectedId={selectedId}
          onSelect={setSelectedId}
          detail={<ModuleDetailChrome copy={DETAIL_COPY} detail={MANIFEST_DETAIL} />}
        />
      </Frame>

      <Frame
        title="Detail — manifest fields + select prompt"
        description="The manifest panel (fields, limits, schemas) beside the page shell's no-selection prompt."
        testId="modules-rail-fixture-detail"
      >
        <div className="grid gap-4 lg:grid-cols-2">
          <ModuleDetailChrome copy={DETAIL_COPY} detail={MANIFEST_DETAIL} />
          <ModulesPageChrome
            copy={PAGE_COPY}
            list={READY_LIST}
            selectedId={null}
            onSelect={noop}
            detail={null}
          />
        </div>
      </Frame>

      <Frame
        title="Run — pending"
        description="A Run is in flight: the Run control reads “Running…” and the inspector reports the Run is still executing."
        testId="modules-rail-fixture-run-pending"
      >
        <RunStudioChrome
          {...studioProps(
            { ...IDLE_RUN, pending: true, canRun: false },
            {
              ...IDLE_PROPOSAL,
              run: { runIdShort: 'run_9f3a2c', status: 'running' },
            },
          )}
        />
      </Frame>

      <Frame
        title="Run — success (proposal visible)"
        description="Needs review: the inspector shows the proposal sections plus Accept / Discard."
        testId="modules-rail-fixture-run-success"
      >
        <RunStudioChrome
          {...studioProps(IDLE_RUN, {
            ...IDLE_PROPOSAL,
            run: {
              runIdShort: 'run_71bd04',
              status: 'succeeded',
              proposals: SUCCESS_PROPOSALS,
            },
          })}
        />
      </Frame>

      <Frame
        title="Run — detail read error"
        description="A deep-linked Run whose detail read failed says so instead of leaving an empty inspector; retry is wired to the callback."
        testId="modules-rail-fixture-run-error"
      >
        <RunStudioChrome
          {...studioProps({ ...IDLE_RUN, detailError: true }, IDLE_PROPOSAL)}
        />
      </Frame>
    </div>
  );
}
