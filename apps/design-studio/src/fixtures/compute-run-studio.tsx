import { useState, type ReactNode } from 'react';

import {
  Button,
  EntityPickerField,
  Label,
  ProposalSections,
  RunFormFields,
  RunStatusBadge,
  RunsTable,
  Select,
  type ComputeProposals,
  type EntityPickerEntry,
  type InvocationSchema,
  type ProposalSectionsCopy,
  type RunFormCopy,
  type RunStatus,
  type RunsTableCopy,
  type RunTableRow,
} from '@42ch/nexus-ui';

/**
 * Studio fixture for the Compute Run Studio primitives (V1.147 P1 T2).
 *
 * Renders the promoted `@42ch/nexus-ui` primitives across their variant
 * matrix — form derivation (basic-combat pickers, kitchen-sink controls,
 * missing-schema empty state), proposal inspector (success / truncated /
 * failed), and Runs history (all statuses / empty) — plus the fixture-level
 * chrome the app will own in Task 3 (module summary, World selector,
 * Advanced JSON disclosure, Accept/Discard row, filter selects).
 *
 * Boundary: imports only promoted primitives from `@42ch/nexus-ui`. All copy
 * is literal English caller-owned strings (studio is developer-auxiliary and
 * excluded from i18n catalogs); all data is fake. No daemon, no routing, no
 * contracts, no `react-i18next`. Toggle the shell theme to verify light +
 * dark — every class is token-backed.
 */

/* ------------------------------------------------------------------ */
/*  Caller-owned copy (product vocabulary lock — Run / Proposal /        */
/*  Accept / Discard / Compute result; no protocol jargon)               */
/* ------------------------------------------------------------------ */

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

/* ------------------------------------------------------------------ */
/*  Fake data                                                          */
/* ------------------------------------------------------------------ */

const CHARACTERS: EntityPickerEntry[] = [
  { id: 'char-aria', title: 'Aria', subtitle: 'Level 3 · ATK 12' },
  { id: 'char-brann', title: 'Brann', subtitle: 'Level 2 · ATK 9' },
  { id: 'char-sera', title: 'Sera', subtitle: 'Level 5 · ATK 15' },
];

/** Mirrors modules/basic-combat/manifest.json schemas.invocation. */
const BASIC_COMBAT_SCHEMA: InvocationSchema = {
  type: 'object',
  properties: {
    attacker_id: { type: 'string' },
    defender_id: { type: 'string' },
  },
};

const KITCHEN_SINK_SCHEMA: InvocationSchema = {
  type: 'object',
  properties: {
    mode: {
      type: 'string',
      enum: ['Skirmish', 'Campaign'],
      title: 'Mode',
      description: 'How much of the world reacts.',
    },
    rounds: { type: 'integer', minimum: 1, maximum: 20, title: 'Rounds' },
    allow_items: { type: 'boolean', title: 'Allow items' },
    note: { type: 'string', title: 'Note', description: 'Freeform note stored with the Run.' },
    tags: { type: 'array', items: { type: 'string' }, title: 'Tags' },
  },
  required: ['mode'],
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
    {
      op: 'set',
      path: 'character.is_alive',
      target_key_block_id: 'char-brann',
      value: { flag: true },
    },
  ],
  timeline_events: [
    {
      title: 'Aria strikes Brann',
      summary: 'Brann takes 6 damage and staggers back.',
      affected_key_block_ids: ['char-aria', 'char-brann'],
    },
    {
      title: 'Sera tends the wound',
      summary: 'Sera prepares a bandage for the next exchange.',
      affected_key_block_ids: ['char-sera'],
    },
  ],
  new_key_blocks: [{ title: 'Bruised rib', kind: 'injury', severity: 'minor' }],
  battle_report: {
    kind: 'combat',
    damage: 6,
    defender_hp_before: 20,
    defender_hp_after: 14,
  },
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
  {
    runId: 'run_55e8a1',
    moduleName: 'Basic Combat',
    moduleVersion: '1.0.0',
    worldTitle: 'The Lost City',
    status: 'applied',
    statusLabel: STATUS_LABELS.applied,
    startedAt: '2026-07-31 11:20',
    finishedAt: '2026-07-31 11:21',
  },
  {
    runId: 'run_30cc77',
    moduleName: 'Basic Combat',
    moduleVersion: '1.0.0',
    worldTitle: 'Echo Protocol',
    status: 'discarded',
    statusLabel: STATUS_LABELS.discarded,
    startedAt: '2026-07-30 22:10',
    finishedAt: '2026-07-30 22:10',
  },
  {
    runId: 'run_18f0b9',
    moduleName: 'Basic Combat',
    moduleVersion: '1.0.0',
    worldTitle: 'Echo Protocol',
    status: 'failed',
    statusLabel: STATUS_LABELS.failed,
    startedAt: '2026-07-30 21:58',
    finishedAt: '2026-07-30 21:58',
  },
];

function noop() {
  /* Studio fixture only — actions have nowhere to go. */
}

/* ------------------------------------------------------------------ */
/*  Fixture-level product chrome (app-owned in Task 3)                   */
/* ------------------------------------------------------------------ */

function ModuleSummaryChrome() {
  return (
    <div
      data-testid="run-studio-module-summary"
      className="rounded-card border border-gray-alpha-300 bg-background-100 p-4"
    >
      <div className="flex flex-wrap items-baseline gap-2">
        <h4 className="text-heading-16 font-heading text-gray-1000">Basic Combat</h4>
        <span className="text-copy-13 text-gray-700">v1.0.0</span>
      </div>
      <p className="mt-1 text-copy-13 text-gray-700">
        Simple combat resolution between two characters.
      </p>
    </div>
  );
}

function WorldSelectorChrome() {
  return (
    <div className="flex flex-col gap-1.5 max-w-xs">
      <Label htmlFor="run-studio-world">World</Label>
      <Select id="run-studio-world" defaultValue="world-lost-city" data-testid="run-studio-world">
        <option value="world-lost-city">The Lost City</option>
        <option value="world-echo">Echo Protocol</option>
      </Select>
      <p className="text-copy-13 text-gray-700">
        Branch: root (default). Product chrome — app-owned in Task 3.
      </p>
    </div>
  );
}

function AdvancedJsonChrome({ values }: { values: Record<string, unknown> }) {
  return (
    <details
      data-testid="run-studio-advanced-json"
      className="rounded-control border border-gray-alpha-300 bg-background-100 p-3"
    >
      <summary className="cursor-pointer text-label-14 text-gray-1000">
        Advanced: edit invocation JSON
      </summary>
      <p className="mt-2 text-copy-13 text-gray-700">
        Form fields sync back best-effort when you edit the raw JSON.
      </p>
      <pre className="mt-2 overflow-x-auto rounded-control bg-background-200 p-3 text-copy-13-mono text-gray-1000">
        {JSON.stringify(values, null, 2)}
      </pre>
    </details>
  );
}

/* ------------------------------------------------------------------ */
/*  Variant sections                                                     */
/* ------------------------------------------------------------------ */

function FormBasicCombatVariant() {
  const [values, setValues] = useState<Record<string, unknown>>({});
  const setField = (name: string, value: unknown) =>
    setValues((prev) => ({ ...prev, [name]: value }));

  return (
    <div data-testid="run-studio-form-basic-combat" className="grid max-w-xl gap-4">
      <ModuleSummaryChrome />
      <WorldSelectorChrome />
      <RunFormFields
        schema={BASIC_COMBAT_SCHEMA}
        requiredKeyBlockTypes={['character']}
        values={values}
        onChange={setField}
        entityEntries={{ attacker_id: CHARACTERS, defender_id: CHARACTERS }}
        copy={FORM_COPY}
        idPrefix="fixture-basic-combat"
      />
      <AdvancedJsonChrome values={values} />
      <div>
        <Button variant="primary" onClick={noop} data-testid="run-studio-run-button">
          Run
        </Button>
      </div>
    </div>
  );
}

function FormKitchenSinkVariant() {
  const [values, setValues] = useState<Record<string, unknown>>({ rounds: 3, allow_items: true });
  const setField = (name: string, value: unknown) =>
    setValues((prev) => ({ ...prev, [name]: value }));

  return (
    <div data-testid="run-studio-form-kitchen-sink" className="grid max-w-xl gap-4">
      <RunFormFields
        schema={KITCHEN_SINK_SCHEMA}
        values={values}
        onChange={setField}
        copy={FORM_COPY}
        idPrefix="fixture-kitchen-sink"
      />
    </div>
  );
}

function InspectorSuccessVariant() {
  const [selected, setSelected] = useState<string[]>(['evt_0', 'evt_1']);
  const toggle = (eventId: string) =>
    setSelected((prev) =>
      prev.includes(eventId) ? prev.filter((id) => id !== eventId) : [...prev, eventId],
    );

  return (
    <div data-testid="run-studio-inspector-success" className="grid max-w-2xl gap-4">
      <div className="flex flex-wrap items-center gap-3">
        <h4 className="text-heading-16 font-heading text-gray-1000">Basic Combat</h4>
        <RunStatusBadge status="succeeded" label={STATUS_LABELS.succeeded} />
      </div>
      <ProposalSections
        proposals={SUCCESS_PROPOSALS}
        copy={PROPOSAL_COPY}
        selectedEventIds={selected}
        onToggleEvent={toggle}
      />
      <div className="flex gap-3">
        <Button variant="primary" onClick={noop} data-testid="run-studio-accept">
          Accept
        </Button>
        <Button variant="secondary" onClick={noop} data-testid="run-studio-discard">
          Discard
        </Button>
      </div>
      <p className="text-copy-13 text-gray-700">
        Accept commits all proposals atomically; Discard leaves the World unchanged (app confirms).
      </p>
    </div>
  );
}

function InspectorFailedVariant() {
  return (
    <div data-testid="run-studio-inspector-failed" className="grid max-w-2xl gap-4">
      <div className="flex flex-wrap items-center gap-3">
        <h4 className="text-heading-16 font-heading text-gray-1000">Basic Combat</h4>
        <RunStatusBadge status="failed" label={STATUS_LABELS.failed} />
      </div>
      <div
        data-testid="run-studio-failed-block"
        className="rounded-card border border-error-surface-border bg-error-surface p-4"
      >
        <p className="text-label-14 font-medium text-gray-1000">Run failed</p>
        <p className="mt-1 text-copy-13 text-gray-700">
          The module stopped at a safety limit; simplify inputs or retry. World unchanged.
        </p>
        <p className="mt-2 text-copy-13-mono text-gray-700">compute_wall_time_exceeded</p>
      </div>
    </div>
  );
}

function RunsFilterChrome() {
  return (
    <div data-testid="run-studio-runs-filters" className="flex flex-wrap items-end gap-4">
      <div className="flex flex-col gap-1.5">
        <Label htmlFor="run-studio-filter-module">Module</Label>
        <Select id="run-studio-filter-module" defaultValue="all" className="w-44">
          <option value="all">All modules</option>
          <option value="basic-combat">Basic Combat</option>
        </Select>
      </div>
      <div className="flex flex-col gap-1.5">
        <Label htmlFor="run-studio-filter-world">World</Label>
        <Select id="run-studio-filter-world" defaultValue="all" className="w-44">
          <option value="all">All worlds</option>
          <option value="world-lost-city">The Lost City</option>
          <option value="world-echo">Echo Protocol</option>
        </Select>
      </div>
      <div className="flex flex-col gap-1.5">
        <Label htmlFor="run-studio-filter-status">Status</Label>
        <Select id="run-studio-filter-status" defaultValue="all" className="w-44">
          <option value="all">All statuses</option>
          <option value="succeeded">Needs review</option>
          <option value="applied">Applied</option>
          <option value="discarded">Discarded</option>
          <option value="failed">Failed</option>
          <option value="running">Running</option>
        </Select>
      </div>
      <p className="text-copy-13 text-gray-700">Filter chrome is visual only in this fixture.</p>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Fixture root                                                         */
/* ------------------------------------------------------------------ */

function VariantBlock({
  title,
  note,
  testId,
  children,
}: {
  title: string;
  note?: string;
  testId: string;
  children: ReactNode;
}) {
  return (
    <div className="grid gap-3">
      <div>
        <h4 className="text-heading-16 font-heading text-gray-1000">{title}</h4>
        {note && <p className="mt-0.5 text-copy-13 text-gray-700">{note}</p>}
      </div>
      <div data-testid={testId} className="rounded-card border border-gray-alpha-300 bg-background-100 p-6">
        {children}
      </div>
    </div>
  );
}

export function ComputeRunStudioFixtures() {
  return (
    <div data-testid="compute-run-studio-fixtures" className="grid gap-10">
      <VariantBlock
        title="Form — Basic Combat (two pickers)"
        note="Reference manifest: attacker_id / defender_id derive entity pickers over World characters. Module summary, World selector, and Advanced JSON are fixture-level product chrome (app-owned in Task 3)."
        testId="run-studio-variant-form-basic-combat"
      >
        <FormBasicCombatVariant />
      </VariantBlock>

      <VariantBlock
        title="Form — control kitchen sink"
        note="enum → select, integer → number (min/max), boolean → checkbox, string → text, array → unsupported note (Advanced JSON path)."
        testId="run-studio-variant-form-kitchen-sink"
      >
        <FormKitchenSinkVariant />
      </VariantBlock>

      <VariantBlock
        title="Form — missing invocation schema"
        note="Module manifest lacks the fields needed to build a form — caller-owned empty state per behavior spec §6."
        testId="run-studio-variant-form-empty"
      >
        <div className="max-w-xl">
          <RunFormFields schema={null} values={{}} onChange={noop} copy={FORM_COPY} />
        </div>
      </VariantBlock>

      <VariantBlock
        title="Picker — no matching entries"
        note="World has no characters for Basic Combat — picker empty state with the next step."
        testId="run-studio-variant-picker-empty"
      >
        <div className="max-w-xl">
          <EntityPickerField
            id="fixture-picker-empty"
            label="Attacker"
            entries={[]}
            value={null}
            onChange={noop}
            placeholder={FORM_COPY.entityPlaceholder}
            emptyTitle={FORM_COPY.entityEmptyTitle}
            emptyDescription={FORM_COPY.entityEmptyDescription}
          />
        </div>
      </VariantBlock>

      <VariantBlock
        title="Inspector — succeeded, all four proposal parts"
        note="Report / Knowledge updates / Timeline events / New knowledge; per-event selection is the spec §2 opt-in. Accept / Discard are explicit author actions."
        testId="run-studio-variant-inspector-success"
      >
        <InspectorSuccessVariant />
      </VariantBlock>

      <VariantBlock
        title="Inspector — truncated proposals"
        note="Full result stays available on the Run; the preview says so honestly."
        testId="run-studio-variant-inspector-truncated"
      >
        <div className="max-w-2xl">
          <ProposalSections proposals={SUCCESS_PROPOSALS} truncated copy={PROPOSAL_COPY} />
        </div>
      </VariantBlock>

      <VariantBlock
        title="Inspector — failed Run"
        note="Failure lives in the Run inspector only — never on the Timeline (direct lane). Fixture-local composition on error-surface tokens."
        testId="run-studio-variant-inspector-failed"
      >
        <InspectorFailedVariant />
      </VariantBlock>

      <VariantBlock
        title="Runs — all statuses, newest first"
        note="Needs review / Applied / Discarded / Failed / Running. Module / World / Status filter chrome is visual only here."
        testId="run-studio-variant-runs-populated"
      >
        <div className="grid gap-4">
          <RunsFilterChrome />
          <RunsTable rows={RUN_ROWS} copy={RUNS_COPY} onOpenRun={noop} />
        </div>
      </VariantBlock>

      <VariantBlock
        title="Runs — empty history"
        note="No runs yet — run this module to see history here."
        testId="run-studio-variant-runs-empty"
      >
        <RunsTable rows={[]} copy={RUNS_COPY} />
      </VariantBlock>
    </div>
  );
}
