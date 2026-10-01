/**
 * Studio fixture — module editing inspector states (v1.203 P2 O1/O2/O3,
 * Greptile P3 studio-first fix on PR #355). The editable visual contract
 * for the App inspector wiring shipped in v1.203 P2:
 *
 *   (1) World KB entity inspector — `modules.mental` structured editing
 *       (nine locked fields + unknown-key fallback rows)
 *   (2) World KB entity inspector — `modules.mental` raw-JSON fallback
 *       (a stored NON-object value — e.g. an array — that the structured
 *       form cannot represent; the complete stored value seeds the textarea
 *       and is repaired deliberately, never silently discarded)
 *   (3) World KB entity inspector — `modules.belief` structured row editing
 *   (4) World KB entity inspector — `modules.belief` raw-JSON fallback
 *       (a stored non-array value)
 *   (5) Timeline event inspector — `modules.observation` editing
 *       (observer selection + explicit-empty claim + access JSON)
 *   (6) Timeline event inspector — `modules.observation` explicit empty
 *       (`observers: []` — the PD-9 "explicitly nobody" claim)
 *   (7) Timeline event inspector — `modules.observation` raw-JSON fallback
 *
 * Fixture data mirrors the spoke handbook worked-example (box/basket) used
 * by the read-only mental-surfacing fixtures: `kb_bo` (Bo, harbor master),
 * `kb_ana` (Ana, the observer), `evt_transfer` (Marble transfer).
 *
 * Boundary (design-studio AGENTS.md import boundaries HARD): no
 * `@xyflow/react`, no `@42ch/nexus-contracts`, no daemon clients, no
 * `useTranslation`. Wire shapes are hand-mirrored locally; static English
 * product vocabulary only. Editing surfaces render real input controls
 * (read-only seeded values — this fixture pins the visual contract, not the
 * interaction), data-driven from the fixture records below.
 */
import { type ReactNode } from 'react';
import { ChevronDown, Eye, User } from 'lucide-react';

/* ------------------------------------------------------------------ */
/*  Local wire-shape mirrors (studio boundary: no contracts package)    */
/* ------------------------------------------------------------------ */

/** Mirror of `WorldKbEntityProjection` (subset). */
interface ModuleEditingEntity {
  key_block_id: string;
  block_type: string;
  canonical_name: string;
  status: string;
  /** Per-entry functional-dialect modules (modules.mental, modules.belief, …). */
  modules?: Record<string, unknown>;
}

/** Mirror of the Timeline event node projection (subset). */
interface ModuleEditingEvent {
  key_block_id: string;
  canonical_name: string;
  version: number;
  /** Per-event functional-dialect modules (modules.observation). */
  modules?: Record<string, unknown>;
}

/* ------------------------------------------------------------------ */
/*  Fixture data — spoke handbook worked-example (box/basket)           */
/* ------------------------------------------------------------------ */

/** (1) Holder with a populated structured `modules.mental` bag — kb_bo. */
const CHARACTER_MENTAL_STRUCTURED: ModuleEditingEntity = {
  key_block_id: 'kb_bo',
  block_type: 'character',
  canonical_name: 'Bo',
  status: 'confirmed',
  modules: {
    mental: {
      identity: { role: 'harbor_master' },
      beliefs: { ref: 'kb_bo_beliefs', count: 12 },
      attention: { target: 'kb_tw_dawn_dock', modality: 'visual' },
      goals: [{ goal: 'clear the dawn berths', status: 'active' }],
      emotions: [{ emotion: 'alert', intensity: 0.6 }],
      // `custom_model_state` is OUTSIDE the locked nine-field vocabulary —
      // the App renders it as an unknown-key raw-JSON fallback row.
      custom_model_state: { nested: { deeply: ['a', 'b'] } },
    },
  },
};

/** (2) Holder whose stored `modules.mental` is an ARRAY — raw-JSON fallback. */
const CHARACTER_MENTAL_RAW: ModuleEditingEntity = {
  key_block_id: 'kb_bo',
  block_type: 'character',
  canonical_name: 'Bo',
  status: 'confirmed',
  modules: {
    // A legacy array the nine-field form cannot represent — the App must
    // expose the complete stored value through the raw-JSON fallback rather
    // than reading it as {} and discarding it on the next edited save.
    mental: [{ goal: 'legacy row the form cannot represent' }],
  },
};

/** (3) Holder with a populated `modules.belief` row array — structured rows. */
const CHARACTER_BELIEF_STRUCTURED: ModuleEditingEntity = {
  key_block_id: 'kb_bo',
  block_type: 'character',
  canonical_name: 'Bo',
  status: 'confirmed',
  modules: {
    belief: [
      {
        holder: 'kb_bo',
        proposition: 'the marble is in the box',
        order: 1,
        truth: 'False',
        access: 'Private',
        representation: 'Explicit',
        content_type: 'Contents/Physical State',
        source: 'Perception',
        context: 'Neutral',
      },
    ],
  },
};

/** (4) Holder whose stored `modules.belief` is a STRING — raw-JSON fallback. */
const CHARACTER_BELIEF_RAW: ModuleEditingEntity = {
  key_block_id: 'kb_bo',
  block_type: 'character',
  canonical_name: 'Bo',
  status: 'confirmed',
  modules: {
    // A non-array stored dialect value — the row editor cannot represent
    // it; the App seeds the raw-JSON fallback with the complete value.
    belief: 'legacy scalar belief bag',
  },
};

/** Observer option rows already in the loaded graph (PD-18). */
const OBSERVER_OPTIONS: Array<{ id: string; name: string }> = [
  { id: 'kb_ana', name: 'Ana' },
  { id: 'kb_bo', name: 'Bo' },
];

/** (5) Event with a representable stored observation — structured editing. */
const EVENT_OBSERVATION_STRUCTURED: ModuleEditingEvent = {
  key_block_id: 'evt_transfer',
  canonical_name: 'Marble transfer',
  version: 3,
  modules: {
    observation: {
      observers: ['kb_ana'],
      access: { line_of_sight: true, hearing_range: true, modality: ['visual', 'auditory'] },
    },
  },
};

/** (6) Event whose stored observation is a non-object — raw-JSON fallback. */
const EVENT_OBSERVATION_RAW: ModuleEditingEvent = {
  key_block_id: 'evt_transfer',
  canonical_name: 'Marble transfer',
  version: 3,
  modules: {
    // `observation: 42` is not an object the structured editor can
    // represent — the App seeds the raw-JSON fallback verbatim.
    observation: 42,
  },
};

/** (7) Event with an explicit empty observation — observers: [] (PD-9 nobody). */
const EVENT_OBSERVATION_EMPTY: ModuleEditingEvent = {
  key_block_id: 'evt_empty_watch',
  canonical_name: 'Empty watch',
  version: 2,
  modules: {
    observation: { observers: [] },
  },
};

/* ------------------------------------------------------------------ */
/*  Shared chrome (mirrors the App inspector markup/classes)            */
/* ------------------------------------------------------------------ */

function FixtureFrame({
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
      className="mb-8 rounded-card border border-gray-alpha-200 bg-background-100 p-4"
      data-testid={testId}
    >
      <h4 className="text-heading-16 font-heading text-gray-1000 mb-1">{title}</h4>
      <p className="text-copy-13 text-gray-700 mb-4">{description}</p>
      {children}
    </div>
  );
}

/** App-parity seeded input: editable control chrome, read-only fixture value. */
function SeededInput({
  id,
  label,
  value,
  mono = true,
  rows,
}: {
  id: string;
  label: string;
  value: string;
  mono?: boolean;
  rows?: number;
}) {
  return (
    <div className="flex flex-col gap-0.5">
      <label htmlFor={id} className="text-label-14 font-semibold text-gray-900">
        {label}
      </label>
      {rows !== undefined ? (
        <textarea
          id={id}
          rows={rows}
          readOnly
          value={value}
          spellCheck={false}
          className={`w-full rounded-input border border-gray-alpha-400 bg-background-100 px-2 py-1.5 text-copy-13 text-gray-1000 ${
            mono ? 'font-mono' : ''
          }`}
        />
      ) : (
        <input
          id={id}
          readOnly
          value={value}
          className={`w-full rounded-input border border-gray-alpha-400 bg-background-100 px-2 py-1.5 text-copy-13 text-gray-1000 ${
            mono ? 'font-mono' : ''
          }`}
        />
      )}
    </div>
  );
}

/** App-parity tertiary small button (static chrome — no fixture wiring). */
function ChromeButton({ children, testId }: { children: ReactNode; testId: string }) {
  return (
    <button
      type="button"
      disabled
      data-testid={testId}
      className="rounded-pill border border-gray-alpha-400 px-2 py-1 text-label-12 font-semibold text-gray-700 disabled:opacity-60"
    >
      {children}
    </button>
  );
}

function InspectorAside({
  title,
  icon,
  testId,
  children,
}: {
  title: string;
  icon: ReactNode;
  testId: string;
  children: ReactNode;
}) {
  return (
    <aside
      className="w-[320px] rounded-card border border-gray-alpha-400 bg-background-100 p-4 shadow-card"
      aria-label={`${title} — inspector`}
      data-testid={testId}
    >
      <div className="flex items-center gap-2">
        {icon}
        <h3 className="font-heading text-heading-16 text-gray-1000">{title}</h3>
      </div>
      {children}
    </aside>
  );
}

function SectionHeader({ title }: { title: string }) {
  return (
    <button
      type="button"
      aria-expanded="true"
      className="mt-3 flex w-full items-center gap-1.5 border-t border-gray-alpha-300 pt-2 text-left text-label-14 font-semibold text-gray-900"
    >
      <ChevronDown className="h-4 w-4 text-gray-700" aria-hidden />
      {title}
    </button>
  );
}

/* ------------------------------------------------------------------ */
/*  (1)(2) World KB entity inspector — modules.mental editing           */
/* ------------------------------------------------------------------ */

/** Handbook order for the nine-field mental table (product locks §Mental field vocabulary). */
const MENTAL_FIELD_ORDER = [
  'identity',
  'beliefs',
  'attention',
  'goals',
  'intentions',
  'emotions',
  'dispositions',
  'norms',
  'constraints',
] as const;

const MENTAL_FIELD_LABELS: Record<(typeof MENTAL_FIELD_ORDER)[number], string> = {
  identity: 'Identity',
  beliefs: 'Beliefs',
  attention: 'Attention',
  goals: 'Goals',
  intentions: 'Intentions',
  emotions: 'Emotions',
  dispositions: 'Dispositions',
  norms: 'Norms',
  constraints: 'Constraints',
};

/**
 * Editable "Mental State" section — structured mode (O1). Nine locked
 * fields seed from the stored bag (JSON text areas); unknown inner keys
 * render as extra raw-JSON rows so they round-trip verbatim (STOP
 * condition — never silently dropped).
 */
function MentalEditingSection({ entity }: { entity: ModuleEditingEntity }) {
  const mental = entity.modules?.mental as Record<string, unknown>;
  return (
    <section data-testid="mental-editing-section" aria-label="Mental State">
      <SectionHeader title="Mental State" />
      <div className="mt-1.5 flex flex-col gap-2">
        {MENTAL_FIELD_ORDER.filter((key) => mental[key] !== undefined).map((key) => (
          <SeededInput
            key={key}
            id={`fixture-mental-${key}`}
            label={MENTAL_FIELD_LABELS[key]}
            value={JSON.stringify(mental[key], null, 2)}
            rows={2}
          />
        ))}
        {Object.keys(mental)
          .filter((key) => !(MENTAL_FIELD_ORDER as readonly string[]).includes(key))
          .map((key, i) => (
            <SeededInput
              key={key}
              id={`fixture-mental-extra-${i}`}
              label={key}
              value={JSON.stringify(mental[key], null, 2)}
              rows={2}
            />
          ))}
      </div>
    </section>
  );
}

/**
 * Raw-JSON fallback "Mental State" section (Greptile P1 fix): the stored
 * member is a NON-object value the structured form cannot represent — the
 * complete stored value seeds one JSON textarea (untouched seed = not
 * dirty); the author repairs it deliberately as a JSON object.
 */
function MentalRawSection({ entity }: { entity: ModuleEditingEntity }) {
  return (
    <section data-testid="mental-editing-section" aria-label="Mental State">
      <SectionHeader title="Mental State" />
      <div className="mt-1.5 flex flex-col gap-1">
        <label htmlFor="fixture-mental-raw" className="text-label-14 font-semibold text-gray-900">
          Raw mental JSON
        </label>
        <textarea
          id="fixture-mental-raw"
          rows={6}
          readOnly
          value={JSON.stringify(entity.modules?.mental, null, 2)}
          spellCheck={false}
          data-testid="fixture-mental-raw-json"
          className="w-full rounded-input border border-gray-alpha-400 bg-background-100 px-2 py-1.5 font-mono text-copy-13 text-gray-1000"
        />
      </div>
    </section>
  );
}

/** Entity inspector chrome (title/version) + the mental editing section. */
function EntityInspectorMentalSample({ entity, raw }: { entity: ModuleEditingEntity; raw: boolean }) {
  return (
    <InspectorAside
      title={entity.canonical_name}
      icon={<User className="h-4 w-4 text-purple-700" aria-hidden />}
      testId="entity-inspector-mental-sample"
    >
      <dl className="mt-2 flex flex-col gap-1 text-copy-13">
        <div className="flex justify-between">
          <dt className="text-gray-700">Kind</dt>
          <dd className="font-mono text-gray-1000">{entity.block_type}</dd>
        </div>
        <div className="flex justify-between">
          <dt className="text-gray-700">Entry id</dt>
          <dd className="font-mono text-gray-1000">{entity.key_block_id}</dd>
        </div>
      </dl>
      {raw ? <MentalRawSection entity={entity} /> : <MentalEditingSection entity={entity} />}
    </InspectorAside>
  );
}

/* ------------------------------------------------------------------ */
/*  (3)(4) World KB entity inspector — modules.belief editing           */
/* ------------------------------------------------------------------ */

const BELIEF_FIELDS: Array<{ key: string; label: string }> = [
  { key: 'holder', label: 'Holder' },
  { key: 'proposition', label: 'Proposition' },
  { key: 'order', label: 'Order' },
  { key: 'truth', label: 'Truth Status' },
  { key: 'access', label: 'Knowledge Access' },
  { key: 'representation', label: 'Representation' },
  { key: 'content_type', label: 'Content Type' },
  { key: 'source', label: 'Mental Source' },
  { key: 'context', label: 'Context' },
];

/**
 * Editable "Belief Propositions" section — structured mode (O2): one
 * fieldset per stored row, every editable member as a seeded input.
 */
function BeliefEditingSection({ entity }: { entity: ModuleEditingEntity }) {
  const rows = entity.modules?.belief as Array<Record<string, unknown>>;
  return (
    <section data-testid="belief-editing-section" aria-label="Belief Propositions">
      <SectionHeader title="Belief Propositions" />
      <div className="mt-1.5 flex flex-col gap-3">
        {rows.map((row, index) => (
          <fieldset
            key={index}
            className="flex flex-col gap-1.5 rounded-card border border-gray-alpha-300 p-2"
          >
            {BELIEF_FIELDS.map((field) => (
              <SeededInput
                key={field.key}
                id={`fixture-belief-${index}-${field.key}`}
                label={field.label}
                value={row[field.key] === undefined || row[field.key] === null ? '' : String(row[field.key])}
                mono={false}
                rows={field.key === 'proposition' ? 2 : undefined}
              />
            ))}
            <ChromeButton testId="fixture-belief-remove">Remove belief {index + 1}</ChromeButton>
          </fieldset>
        ))}
        <ChromeButton testId="fixture-belief-add">Add belief</ChromeButton>
      </div>
    </section>
  );
}

/** Raw-JSON fallback "Belief Propositions" section (QC1-F001): a stored
 *  non-array value seeds one JSON textarea; the author repairs deliberately. */
function BeliefRawSection({ entity }: { entity: ModuleEditingEntity }) {
  return (
    <section data-testid="belief-editing-section" aria-label="Belief Propositions">
      <SectionHeader title="Belief Propositions" />
      <div className="mt-1.5 flex flex-col gap-1">
        <label htmlFor="fixture-belief-raw" className="text-label-14 font-semibold text-gray-900">
          Raw belief JSON
        </label>
        <textarea
          id="fixture-belief-raw"
          rows={6}
          readOnly
          value={JSON.stringify(entity.modules?.belief, null, 2)}
          spellCheck={false}
          data-testid="fixture-belief-raw-json"
          className="w-full rounded-input border border-gray-alpha-400 bg-background-100 px-2 py-1.5 font-mono text-copy-13 text-gray-1000"
        />
      </div>
    </section>
  );
}

/** Entity inspector chrome + the belief editing section. */
function EntityInspectorBeliefSample({ entity, raw }: { entity: ModuleEditingEntity; raw: boolean }) {
  return (
    <InspectorAside
      title={entity.canonical_name}
      icon={<User className="h-4 w-4 text-purple-700" aria-hidden />}
      testId="entity-inspector-belief-sample"
    >
      <dl className="mt-2 flex flex-col gap-1 text-copy-13">
        <div className="flex justify-between">
          <dt className="text-gray-700">Kind</dt>
          <dd className="font-mono text-gray-1000">{entity.block_type}</dd>
        </div>
        <div className="flex justify-between">
          <dt className="text-gray-700">Entry id</dt>
          <dd className="font-mono text-gray-1000">{entity.key_block_id}</dd>
        </div>
      </dl>
      {raw ? <BeliefRawSection entity={entity} /> : <BeliefEditingSection entity={entity} />}
    </InspectorAside>
  );
}

/* ------------------------------------------------------------------ */
/*  (5)(6) Timeline event inspector — modules.observation editing       */
/* ------------------------------------------------------------------ */

/**
 * Editable "Observation" section (O3): observer checkboxes from entities
 * already in the loaded graph (PD-18), the explicit "no observers" claim
 * (PD-9), a raw-id entry row, and the access JSON text area.
 */
function ObservationEditingSection({ event }: { event: ModuleEditingEvent }) {
  const observation = event.modules?.observation as {
    observers?: string[];
    access?: Record<string, unknown>;
  };
  const selected = new Set(observation.observers ?? []);
  // PD-9: `observers: []` is the explicit "no observers" claim, distinct
  // from an absent key (unrecorded) — the claim checkbox renders checked.
  const claimNone = observation.observers?.length === 0;
  return (
    <section data-testid="observation-editing-section" aria-label="Observation">
      <h4 className="mt-3 border-t border-gray-alpha-300 pt-2 text-label-14 font-semibold text-gray-900">
        Observation
      </h4>
      <div className="mt-1.5 flex flex-col gap-1">
        <span className="text-label-14 font-semibold text-gray-900">Observers</span>
        {OBSERVER_OPTIONS.map((option) => (
          <label
            key={option.id}
            className="flex items-center gap-2 text-copy-13 text-gray-1000"
          >
            <input type="checkbox" readOnly checked={selected.has(option.id)} />
            {option.name} ({option.id})
          </label>
        ))}
        <label className="flex items-center gap-2 text-copy-13 text-gray-1000">
          <input type="checkbox" readOnly checked={claimNone} data-testid="fixture-observation-claim-none" />
          Explicitly no observers
        </label>
        <div className="flex items-center gap-2">
          <input
            readOnly
            value=""
            placeholder="entry_id, e.g. kb_ana"
            aria-label="entry_id, e.g. kb_ana"
            className="w-full rounded-input border border-gray-alpha-400 bg-background-100 px-2 py-1.5 font-mono text-copy-13 text-gray-1000"
          />
          <ChromeButton testId="fixture-observation-add">Add observer</ChromeButton>
          <ChromeButton testId="fixture-observation-clear">Clear</ChromeButton>
        </div>
      </div>
      <div className="mt-2 flex flex-col gap-1">
        <SeededInput
          id="fixture-observation-access"
          label="Access"
          value={observation.access !== undefined ? JSON.stringify(observation.access, null, 2) : ''}
          rows={3}
        />
      </div>
    </section>
  );
}

/** Raw-JSON fallback "Observation" section (C1): a stored non-object
 *  observation seeds one JSON textarea; the author repairs deliberately. */
function ObservationRawSection({ event }: { event: ModuleEditingEvent }) {
  return (
    <section data-testid="observation-editing-section" aria-label="Observation">
      <h4 className="mt-3 border-t border-gray-alpha-300 pt-2 text-label-14 font-semibold text-gray-900">
        Observation
      </h4>
      <div className="mt-1.5 flex flex-col gap-1">
        <label htmlFor="fixture-observation-raw" className="text-label-14 font-semibold text-gray-900">
          Raw observation JSON
        </label>
        <textarea
          id="fixture-observation-raw"
          rows={6}
          readOnly
          value={JSON.stringify(event.modules?.observation, null, 2)}
          spellCheck={false}
          data-testid="fixture-observation-raw-json"
          className="w-full rounded-input border border-gray-alpha-400 bg-background-100 px-2 py-1.5 font-mono text-copy-13 text-gray-1000"
        />
      </div>
    </section>
  );
}

/** Timeline event inspector chrome + the observation editing section. */
function TimelineInspectorObservationSample({ event, raw }: { event: ModuleEditingEvent; raw: boolean }) {
  return (
    <InspectorAside
      title={event.canonical_name}
      icon={<Eye className="h-4 w-4 text-purple-700" aria-hidden />}
      testId="timeline-inspector-observation-sample"
    >
      <dl className="mt-2 flex flex-col gap-1 text-copy-13">
        <div className="flex justify-between">
          <dt className="text-gray-700">Event id</dt>
          <dd className="font-mono text-gray-1000">{event.key_block_id}</dd>
        </div>
        <div className="flex justify-between">
          <dt className="text-gray-700">Version</dt>
          <dd className="font-mono text-gray-1000">v{event.version}</dd>
        </div>
      </dl>
      {raw ? <ObservationRawSection event={event} /> : <ObservationEditingSection event={event} />}
    </InspectorAside>
  );
}

/* ------------------------------------------------------------------ */
/*  Public fixture component                                            */
/* ------------------------------------------------------------------ */

/**
 * Module editing fixtures — seven editable inspector states (light + dark),
 * the visual contract for the v1.203 P2 module editors and their raw-JSON
 * fallbacks. Seeded inputs are read-only: this fixture pins chrome, layout
 * and copy; interaction lands in the App.
 */
export function ModuleEditingFixtures() {
  return (
    <div className="studio-fixture-boundary" data-testid="module-editing-fixtures">
      <FixtureFrame
        title="Entity inspector — modules.mental structured editing (O1)"
        description="Bo (kb_bo) carries a populated mental bag: the nine locked fields seed as JSON text areas, and the unknown own key custom_model_state renders as an extra raw-JSON row so it round-trips verbatim (STOP condition — never silently dropped)."
        testId="module-fixture-mental-structured"
      >
        <div className="rounded-card bg-canvas-surface p-6" data-testid="module-mental-structured-host">
          <EntityInspectorMentalSample entity={CHARACTER_MENTAL_STRUCTURED} raw={false} />
        </div>
      </FixtureFrame>

      <FixtureFrame
        title="Entity inspector — modules.mental raw-JSON fallback"
        description="A stored array-valued modules.mental (a legacy shape the nine-field form cannot represent) seeds the raw-JSON fallback with the complete stored value — untouched seeds are never dirty, and an unrelated save never rewrites or discards the dialect (Greptile P1). The author repairs it deliberately as a JSON object."
        testId="module-fixture-mental-raw"
      >
        <div className="rounded-card bg-canvas-surface p-6" data-testid="module-mental-raw-host">
          <EntityInspectorMentalSample entity={CHARACTER_MENTAL_RAW} raw />
        </div>
      </FixtureFrame>

      <FixtureFrame
        title="Entity inspector — modules.belief structured editing (O2)"
        description="Bo holds one stored belief row: every editable member (holder, proposition, closed labels like Truth / Access / Source, order) seeds as a flat fieldset; Add / Remove row affordances render below."
        testId="module-fixture-belief-structured"
      >
        <div className="rounded-card bg-canvas-surface p-6" data-testid="module-belief-structured-host">
          <EntityInspectorBeliefSample entity={CHARACTER_BELIEF_STRUCTURED} raw={false} />
        </div>
      </FixtureFrame>

      <FixtureFrame
        title="Entity inspector — modules.belief raw-JSON fallback"
        description="A stored non-array modules.belief (here a legacy scalar string) seeds the raw-JSON fallback with the complete stored value — structured rows would silently shorten or drop it (QC1-F001)."
        testId="module-fixture-belief-raw"
      >
        <div className="rounded-card bg-canvas-surface p-6" data-testid="module-belief-raw-host">
          <EntityInspectorBeliefSample entity={CHARACTER_BELIEF_RAW} raw />
        </div>
      </FixtureFrame>

      <FixtureFrame
        title="Timeline event inspector — modules.observation editing (O3)"
        description="Marble transfer (evt_transfer) records observation: observer checkboxes resolve from entities already in the loaded graph (PD-18 — 'Ana (kb_ana)' is checked), the explicit 'no observers' claim stays a separate checkbox (PD-9), a raw-id entry row adds ids absent from the graph, and the access object seeds as a JSON text area."
        testId="module-fixture-observation-structured"
      >
        <div className="rounded-card bg-canvas-surface p-6" data-testid="module-observation-structured-host">
          <TimelineInspectorObservationSample event={EVENT_OBSERVATION_STRUCTURED} raw={false} />
        </div>
      </FixtureFrame>

      <FixtureFrame
        title="Timeline event inspector — modules.observation explicit empty"
        description="Empty watch records observation with an explicit empty observers list: PD-9 treats empty as 'explicitly nobody' — the 'Explicitly no observers' claim checkbox renders checked (distinct from an absent observation, which is unrecorded)."
        testId="module-fixture-observation-empty"
      >
        <div className="rounded-card bg-canvas-surface p-6" data-testid="module-observation-empty-host">
          <TimelineInspectorObservationSample event={EVENT_OBSERVATION_EMPTY} raw={false} />
        </div>
      </FixtureFrame>

      <FixtureFrame
        title="Timeline event inspector — modules.observation raw-JSON fallback"
        description="A stored observation member the structured editor cannot represent verbatim (here a bare number) seeds the raw-JSON fallback with the complete stored value — never coerced or dropped (C1); the author repairs it deliberately as a JSON object."
        testId="module-fixture-observation-raw"
      >
        <div className="rounded-card bg-canvas-surface p-6" data-testid="module-observation-raw-host">
          <TimelineInspectorObservationSample event={EVENT_OBSERVATION_RAW} raw />
        </div>
      </FixtureFrame>
    </div>
  );
}
