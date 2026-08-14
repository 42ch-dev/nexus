/**
 * Studio fixture — mental surfacing inspector states (V1.164 P3 Task 2, AR-6
 * studio-first). The read-only visual contract for the App inspector wiring
 * (Tasks 3–4): character KnowledgeEntry `modules.mental` fields and Timeline
 * Narrative event `modules.observation.observers`.
 *
 * Five states:
 *   (a) character inspector with populated nine-field keys (beliefs / goals /
 *       emotions proof, PD-16 shows every populated key)
 *   (b) character inspector WITHOUT `modules.mental` — no mental section, no
 *       empty panel
 *   (c) event inspector with observers list (id + resolved name when the
 *       fixture graph already has names, PD-18)
 *   (d) event inspector with `observers: []` — explicit "No observers" line
 *       (PD-9: empty = explicitly nobody)
 *   (e) event inspector without observation — section hidden (PD-9: absent =
 *       unrecorded)
 *
 * Fixture data mirrors the spoke handbook worked-example (box/basket):
 * `kb_bo` (Bo) holds the false belief, `evt_transfer` (Marble transfer) is
 * observed by `kb_ana` (Ana) only.
 *
 * Boundary (design-studio AGENTS.md import boundaries HARD): no
 * `@xyflow/react`, no `@42ch/nexus-contracts`, no daemon clients, no
 * `useTranslation`. The wire shapes consumed by the App inspectors are
 * hand-mirrored locally (see `MentalSurfacingEntity` / `MentalSurfacingEvent`
 * below) — same `modules` shape Task 1 landed on
 * `WorldKbEntityProjection` / `TimelineEventInfo`. Static English product
 * vocabulary only. No input controls anywhere — the "Mental State" chevron is
 * a static collapsible affordance; toggle wiring lands with the App inspector
 * (Task 3).
 */
import { type ReactNode } from 'react';
import { ChevronDown, Eye, User } from 'lucide-react';

/* ------------------------------------------------------------------ */
/*  Local wire-shape mirrors (studio boundary: no contracts package)    */
/*  Source shapes: packages/nexus-contracts/src/generated/daemon-api/   */
/*    canvas/world-kb/world-kb-entity-projection.ts                    */
/*    timeline/timeline-event-info.ts                                  */
/*  Mirrored subset = fields the fixture renders; `modules` kept in     */
/*  the exact additive-optional Task 1 shape.                           */
/* ------------------------------------------------------------------ */

/** Mirror of `WorldKbEntityProjection` (subset). */
interface MentalSurfacingEntity {
  key_block_id: string;
  block_type: string;
  canonical_name: string;
  status: string;
  /** Per-entry functional-dialect modules (modules.mental, ...). Absent when no modules data (V1.164 P3, AR-2). */
  modules?: Record<string, unknown>;
}

/** Mirror of `TimelineEventInfo` (subset). */
interface MentalSurfacingEvent {
  id: string;
  event_type: string;
  status: string;
  title?: string | null;
  summary?: string | null;
  metadata: Record<string, unknown>;
  created_at: string;
  /** Per-event functional-dialect modules (modules.observation). Null when unrecorded (V1.164 P3, AR-2). */
  modules?: Record<string, unknown> | null;
}

/** `modules.observation` inner shape (handbook-exact). */
interface MentalObservation {
  observers?: unknown[];
  access?: Record<string, unknown>;
}

/* ------------------------------------------------------------------ */
/*  Fixture data — spoke handbook worked-example (box/basket)           */
/* ------------------------------------------------------------------ */

/** (a) Character with populated `modules.mental` — kb_bo (Bo, harbor master). */
const CHARACTER_WITH_MENTAL: MentalSurfacingEntity = {
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
      norms: ['greet arriving captains'],
      constraints: ['cannot waive dockside law'],
    },
  },
};

/** (b) Character WITHOUT `modules.mental` — kb_ana (Ana, the observer). */
const CHARACTER_WITHOUT_MENTAL: MentalSurfacingEntity = {
  key_block_id: 'kb_ana',
  block_type: 'character',
  canonical_name: 'Ana',
  status: 'confirmed',
};

/** Base event row shared by the three event states. */
const EVENT_BASE: Omit<MentalSurfacingEvent, 'modules'> = {
  id: 'evt_transfer',
  event_type: 'narrative',
  status: 'canon',
  title: 'Marble transfer',
  summary: 'Ana moves the marble from the box to the basket while Bo is away.',
  metadata: {},
  created_at: '2026-08-14T10:00:00Z',
};

/** (c) Event with recorded observation — observers: [kb_ana]. */
const EVENT_WITH_OBSERVERS: MentalSurfacingEvent = {
  ...EVENT_BASE,
  modules: {
    observation: {
      observers: ['kb_ana'],
      access: {
        line_of_sight: true,
        hearing_range: true,
        modality: ['visual', 'auditory'],
      },
    },
  },
};

/** (d) Event with explicit empty observation — observers: [] (PD-9 nobody). */
const EVENT_EMPTY_OBSERVERS: MentalSurfacingEvent = {
  ...EVENT_BASE,
  id: 'evt_empty_watch',
  title: 'Empty watch',
  modules: {
    observation: {
      observers: [],
    },
  },
};

/** (e) Event without observation — no modules key at all (PD-9 unrecorded). */
const EVENT_NO_OBSERVATION: MentalSurfacingEvent = {
  ...EVENT_BASE,
  id: 'evt_unrecorded',
  title: 'Unrecorded passage',
};

/**
 * entry_id → canonical_name for entities already in the fixture graph.
 * PD-18: observers resolve to name + id when the graph has names in memory.
 */
const OBSERVER_NAMES: Record<string, string> = {
  kb_ana: 'Ana',
  kb_bo: 'Bo',
};

/* ------------------------------------------------------------------ */
/*  Shared fixture frame                                                */
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

/* ------------------------------------------------------------------ */
/*  Character inspector — modules.mental section chrome                 */
/*  Mirrors the App entity-inspector section pattern (bold field label  */
/*  + value rows; read-only — no input controls).                       */
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

/** One bold field label + value row (read-only). Structured values render as pretty JSON. */
function MentalFieldRow({ label, value }: { label: string; value: unknown }) {
  return (
    <div className="flex flex-col gap-0.5">
      <dt className="text-label-14 font-semibold text-gray-900">{label}</dt>
      <dd className="text-copy-13 font-mono text-gray-1000 whitespace-pre-wrap break-words">
        {JSON.stringify(value, null, 2)}
      </dd>
    </div>
  );
}

/**
 * "Mental State" section — collapsible-style header (static chevron
 * affordance; toggle wiring is App Task 3) + every populated nine-field key
 * (PD-16). Returns null when `modules.mental` is absent — no empty panel.
 */
function MentalStateSection({
  mental,
}: {
  mental: Record<string, unknown>;
}) {
  const fields = MENTAL_FIELD_ORDER.filter((key) => mental[key] !== undefined);
  if (fields.length === 0) {
    return null;
  }
  return (
    <section
      className="mt-3 border-t border-gray-alpha-300 pt-2"
      data-testid="mental-state-section"
      aria-label="Mental State"
    >
      <h4 className="flex items-center gap-1.5 text-label-14 font-semibold text-gray-900">
        <ChevronDown className="h-4 w-4 text-gray-700" aria-hidden />
        Mental State
      </h4>
      <dl className="mt-1.5 flex flex-col gap-2">
        {fields.map((key) => (
          <MentalFieldRow
            key={key}
            label={MENTAL_FIELD_LABELS[key]}
            value={mental[key]}
          />
        ))}
      </dl>
    </section>
  );
}

/** Character inspector aside — identity chrome + optional mental section. */
function CharacterInspectorSample({ entity }: { entity: MentalSurfacingEntity }) {
  const mental =
    entity.modules?.mental !== undefined && typeof entity.modules.mental === 'object'
      ? (entity.modules.mental as Record<string, unknown>)
      : undefined;
  return (
    <aside
      className="w-[300px] rounded-card border border-gray-alpha-400 bg-background-100 p-4 shadow-card"
      aria-label={`${entity.canonical_name} — entity details`}
      data-testid="character-inspector-sample"
    >
      <div className="flex items-center gap-2">
        <User className="h-4 w-4 text-purple-700" aria-hidden />
        <h3 className="font-heading text-heading-16 text-gray-1000">
          {entity.canonical_name}
        </h3>
      </div>
      <dl className="mt-2 flex flex-col gap-1 text-copy-13">
        <div className="flex justify-between">
          <dt className="text-gray-700">Kind</dt>
          <dd className="font-mono text-gray-1000">{entity.block_type}</dd>
        </div>
        <div className="flex justify-between">
          <dt className="text-gray-700">Entry id</dt>
          <dd className="font-mono text-gray-1000">{entity.key_block_id}</dd>
        </div>
        <div className="flex justify-between">
          <dt className="text-gray-700">Status</dt>
          <dd className="font-mono text-gray-1000">{entity.status}</dd>
        </div>
      </dl>
      {mental ? <MentalStateSection mental={mental} /> : null}
    </aside>
  );
}

/* ------------------------------------------------------------------ */
/*  Timeline event inspector — modules.observation.observers chrome     */
/*  Observers render as a metadata line (PD-9 / PD-18).                 */
/* ------------------------------------------------------------------ */

/**
 * Observers metadata line. `observers: []` renders the explicit "No
 * observers" claim (PD-9 — empty = explicitly nobody, distinct from
 * absent). Non-array / missing `observers` on a present observation object
 * hides the line — lenient like the P2 checker (skip on malformed).
 */
function ObserversLine({
  observers,
  resolveName,
}: {
  observers: unknown[];
  resolveName: (id: string) => string | undefined;
}) {
  const names = observers.map((observer) => {
    const id = String(observer);
    const name = resolveName(id);
    return name !== undefined && name !== id ? `${name} (${id})` : id;
  });
  return (
    <p className="mt-2 text-copy-13" data-testid="event-observers-line">
      <span className="font-semibold text-gray-900">Observers:</span>{' '}
      {names.length === 0 ? (
        <span className="text-gray-1000">No observers</span>
      ) : (
        <span className="font-mono text-gray-1000">{names.join(', ')}</span>
      )}
    </p>
  );
}

/** Timeline Narrative event inspector aside — chrome + optional observers line. */
function EventInspectorSample({ event }: { event: MentalSurfacingEvent }) {
  const observation =
    event.modules?.observation !== undefined &&
    event.modules.observation !== null &&
    typeof event.modules.observation === 'object'
      ? (event.modules.observation as MentalObservation)
      : undefined;
  const observers = observation?.observers;
  return (
    <aside
      className="w-[300px] rounded-card border border-gray-alpha-400 bg-background-100 p-4 shadow-card"
      aria-label={`${event.title ?? event.id} — event details`}
      data-testid="event-inspector-sample"
    >
      <div className="flex items-center gap-2">
        <Eye className="h-4 w-4 text-purple-700" aria-hidden />
        <h3 className="font-heading text-heading-16 text-gray-1000">
          {event.title ?? event.id}
        </h3>
      </div>
      <dl className="mt-2 flex flex-col gap-1 text-copy-13">
        <div className="flex justify-between">
          <dt className="text-gray-700">Event id</dt>
          <dd className="font-mono text-gray-1000">{event.id}</dd>
        </div>
        <div className="flex justify-between">
          <dt className="text-gray-700">Type</dt>
          <dd className="font-mono text-gray-1000">{event.event_type}</dd>
        </div>
        <div className="flex justify-between">
          <dt className="text-gray-700">Status</dt>
          <dd className="font-mono text-gray-1000">{event.status}</dd>
        </div>
      </dl>
      {event.summary ? <p className="mt-2 text-gray-900">{event.summary}</p> : null}
      {Array.isArray(observers) ? (
        <ObserversLine observers={observers} resolveName={(id) => OBSERVER_NAMES[id]} />
      ) : null}
    </aside>
  );
}

/* ------------------------------------------------------------------ */
/*  Public fixture component                                            */
/* ------------------------------------------------------------------ */

/**
 * Mental surfacing fixtures — five read-only inspector states (light + dark),
 * the visual contract for App Tasks 3–4. Presentational-only: no daemon, no
 * contracts, no input controls.
 */
export function MentalSurfacingFixtures() {
  return (
    <div data-testid="mental-surfacing-fixtures">
      <FixtureFrame
        title="Character inspector — modules.mental populated"
        description="Bo (kb_bo) carries a populated mental bag: every populated nine-field key renders as a bold label + JSON value row (PD-16) under the collapsible-style 'Mental State' header. Beliefs / goals / emotions are the AC proof. The chevron is a static affordance — collapse wiring lands with the App inspector (Task 3)."
        testId="mental-fixture-character-populated"
      >
        <div
          className="rounded-card bg-canvas-surface p-6"
          data-testid="mental-character-populated-host"
        >
          <CharacterInspectorSample entity={CHARACTER_WITH_MENTAL} />
        </div>
      </FixtureFrame>

      <FixtureFrame
        title="Character inspector — no modules.mental"
        description="Ana (kb_ana) has no modules bag: the 'Mental State' section is omitted entirely — no empty panel, no placeholder rows (PD-16)."
        testId="mental-fixture-character-absent"
      >
        <div
          className="rounded-card bg-canvas-surface p-6"
          data-testid="mental-character-absent-host"
        >
          <CharacterInspectorSample entity={CHARACTER_WITHOUT_MENTAL} />
        </div>
      </FixtureFrame>

      <FixtureFrame
        title="Event inspector — observers recorded"
        description="Marble transfer (evt_transfer) records observation: observers resolve to name + id when the graph already has canonical names (PD-18) — 'Ana (kb_ana)'. Access metadata stays on the data; the inspector surfaces the observers line only."
        testId="mental-fixture-event-observers"
      >
        <div
          className="rounded-card bg-canvas-surface p-6"
          data-testid="mental-event-observers-host"
        >
          <EventInspectorSample event={EVENT_WITH_OBSERVERS} />
        </div>
      </FixtureFrame>

      <FixtureFrame
        title="Event inspector — observers: [] (explicit empty)"
        description="Empty watch records observation with an explicit empty observers list: PD-9 treats empty as 'explicitly nobody' — the inspector renders an explicit 'No observers' line instead of hiding the section."
        testId="mental-fixture-event-empty"
      >
        <div
          className="rounded-card bg-canvas-surface p-6"
          data-testid="mental-event-empty-host"
        >
          <EventInspectorSample event={EVENT_EMPTY_OBSERVERS} />
        </div>
      </FixtureFrame>

      <FixtureFrame
        title="Event inspector — observation absent"
        description="Unrecorded passage has no modules at all: absent observation = unrecorded (PD-9) — the observers section is hidden entirely."
        testId="mental-fixture-event-absent"
      >
        <div
          className="rounded-card bg-canvas-surface p-6"
          data-testid="mental-event-absent-host"
        >
          <EventInspectorSample event={EVENT_NO_OBSERVATION} />
        </div>
      </FixtureFrame>
    </div>
  );
}
