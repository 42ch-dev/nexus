/**
 * Studio fixtures — Outline canvas Scene/Beat authoring card + Timeline
 * World-event bind/unbind control (V1.200, Greptile issue 5 studio-first).
 *
 * Mirrors two v1.200 canvas UI surfaces before App wiring:
 *   1. `SceneBeatAuthoringPanel` (apps/web/src/components/canvas/
 *      outline-canvas.tsx) — add-scene / add-beat inputs, chapter badge,
 *      remove buttons, disabled states, EmptyState.
 *   2. The Timeline event inspector's World-event bind/unbind control
 *      (apps/web/src/components/canvas/outline-canvas/inspectors/
 *      event-inspector.tsx) — event-only picker, bound state with Unbind,
 *      disabled-when-no-bound-World (affordance disabled, never hidden).
 *
 * The web sources couple to `useTranslation`, daemon query hooks, and
 * `@42ch/nexus-contracts` wire DTOs, so the fixture is a studio-local
 * structure/token mirror on promoted primitives (`@42ch/nexus-ui`) — same
 * Card/Button/Badge APIs, same literal English copy the app ships as i18n
 * defaultValues. No behavior is re-implemented: every control renders in a
 * fixed state per variant.
 *
 * States:
 *   Authoring card — empty (no chapters, no scenes) / with-draft (scene and
 *   beat titles in progress, add buttons enabled) / bound (scenes bound to
 *   chapters, beats bound to scenes, remove affordances live).
 *   Bind control — unbound (picker enabled, Bind gated on selection) / bound
 *   (World event name + Unbind) / disabled (no bound World: hint copy +
 *   picker/Bind/Unbind all disabled).
 *
 * Boundary (apps/design-studio AGENTS.md — HARD): no `@xyflow/react`, no
 * `@42ch/nexus-contracts`, no daemon clients (`lib/nexus`), no
 * `useTranslation`, no `@tauri-apps`. Static English product vocabulary only.
 * Each variant renders as a light/dark pair — the dark specimen sits in a
 * scoped `.dark` panel so both themes are inspectable without a document
 * theme toggle (same recipe as vi-aesthetic-retune-fixtures).
 */
import { type ReactNode } from 'react';
import { CalendarPlus, Clapperboard, Plus, Trash2 } from 'lucide-react';

import {
  Badge,
  Button,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@42ch/nexus-ui';
import { EmptyState } from '@web-ui/states'; // @web-ui/states — transitional — keep-web (lucide-react asset boundary; product copy & app-composition callbacks)

/* ------------------------------------------------------------------ */
/*  Local token mirrors (studio boundary: web sources not aliased here) */
/* ------------------------------------------------------------------ */

/**
 * Shared form-control class — verbatim mirror of `INPUT_CLASS` in
 * apps/web/src/components/canvas/outline-canvas/inspectors/
 * chapter-meta-field.tsx (DESIGN.md tokens).
 */
const INPUT_CLASS =
  'rounded-control border border-gray-alpha-400 bg-background-100 px-3 py-2 text-gray-1000 focus:border-blue-1000 dark:focus:border-blue-700 disabled:bg-gray-100 disabled:text-gray-700';

/** Picker class — verbatim mirror of the World-event select in the event inspector. */
const SELECT_CLASS =
  'min-w-0 flex-1 rounded-control border border-gray-alpha-400 bg-background-100 px-2 py-1 text-label-12 text-gray-1000 focus:border-blue-1000 dark:focus:border-blue-700';

/** Label + control wrapper — mirror of `MetaField` (chapter-meta-field.tsx). */
function MetaField({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="flex flex-col gap-1 text-copy-13">
      <span className="text-gray-700">{label}</span>
      {children}
    </label>
  );
}

/* ------------------------------------------------------------------ */
/*  Shared fixture chrome                                               */
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

/**
 * Light + dark specimen pair — the plain panel follows the document theme
 * (light by default); the `-dark` panel scopes Tailwind's `.dark` class so
 * dark tokens apply regardless of the document theme.
 */
function ThemePair({
  testId,
  label,
  children,
}: {
  testId: string;
  label: string;
  children: ReactNode;
}) {
  return (
    <div className="flex flex-col gap-2">
      <span className="text-label-12 font-medium text-gray-700">{label}</span>
      <div className="grid gap-4 lg:grid-cols-2">
        <div
          data-testid={`${testId}-light`}
          className="rounded-card border border-gray-alpha-300 bg-background-100 p-6"
        >
          {children}
        </div>
        <div
          data-testid={`${testId}-dark`}
          className="dark rounded-card border border-gray-alpha-300 bg-background-100 p-6"
        >
          {children}
        </div>
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Surface 1 — SceneBeatAuthoringPanel mirror                          */
/* ------------------------------------------------------------------ */

/** Static per-variant authoring data (no behavior — fixed states). */
type AuthoringVariant = 'empty' | 'draft' | 'bound';

interface AuthoringSceneFixture {
  sceneId: string;
  title: string;
  chapter: number;
  beats: { beatId: string; title: string }[];
}

const AUTHORING_DATA: Record<
  AuthoringVariant,
  {
    chapters: number[];
    sceneTitleDraft: string;
    scenes: AuthoringSceneFixture[];
    beatTitleDraft: Record<string, string>;
  }
> = {
  empty: {
    chapters: [],
    sceneTitleDraft: '',
    scenes: [],
    beatTitleDraft: {},
  },
  draft: {
    chapters: [1],
    sceneTitleDraft: 'Arrival at the Ashen Gate',
    scenes: [{ sceneId: 'sc-draft-1', title: 'Arrival at the Ashen Gate', chapter: 1, beats: [{ beatId: 'bt-draft-1', title: 'Gate inspection' }] }],
    beatTitleDraft: { 'sc-draft-1': 'Hook' },
  },
  bound: {
    chapters: [1, 2],
    sceneTitleDraft: '',
    scenes: [
      {
        sceneId: 'sc-bound-1',
        title: 'Arrival at the Ashen Gate',
        chapter: 1,
        beats: [
          { beatId: 'bt-bound-1', title: 'Gate inspection' },
          { beatId: 'bt-bound-2', title: 'Guard captain parley' },
        ],
      },
      {
        sceneId: 'sc-bound-2',
        title: 'Ride to the Ford',
        chapter: 2,
        beats: [{ beatId: 'bt-bound-3', title: 'River crossing' }],
      },
    ],
    beatTitleDraft: {},
  },
};

function AuthoringCard({ variant }: { variant: AuthoringVariant }) {
  const data = AUTHORING_DATA[variant];
  const hasChapters = data.chapters.length > 0;
  // App rule: `add_scene` needs an existing chapter and a non-blank title.
  const canAddScene = hasChapters && data.sceneTitleDraft.trim().length > 0;

  return (
    <Card data-testid={`outline-authoring-card-${variant}`}>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <Clapperboard className="h-5 w-5 text-canvas-outline-accent" aria-hidden />
          Scenes &amp; Beats
        </CardTitle>
        <CardDescription>
          Scenes and beats are work-owned outline structure. They appear on the
          Work Timeline Moment layer.
        </CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        <div className="flex flex-wrap items-end gap-3">
          <MetaField label="Chapter">
            <select
              data-testid={`outline-scene-chapter-${variant}`}
              disabled={!hasChapters}
              className={INPUT_CLASS}
              defaultValue={hasChapters ? String(data.chapters[0]) : undefined}
              aria-label="Chapter"
            >
              {data.chapters.map((chapter) => (
                <option key={chapter} value={String(chapter)}>
                  Chapter {chapter}
                </option>
              ))}
            </select>
          </MetaField>
          <MetaField label="Scene title">
            <input
              type="text"
              data-testid={`outline-scene-title-${variant}`}
              defaultValue={data.sceneTitleDraft}
              className={INPUT_CLASS}
              aria-label="Scene title"
            />
          </MetaField>
          <Button
            type="button"
            variant="secondary"
            size="small"
            data-testid={`outline-add-scene-${variant}`}
            disabled={!canAddScene}
          >
            <Plus className="h-4 w-4" aria-hidden />
            Add scene
          </Button>
        </div>

        {!hasChapters ? (
          <p className="text-copy-13 text-gray-700" data-testid={`outline-no-chapters-${variant}`}>
            Add a chapter before authoring scenes.
          </p>
        ) : null}

        {data.scenes.length === 0 ? (
          <EmptyState
            title="No scenes yet"
            description="Scenes you add here appear on the Work Timeline Moment layer."
          />
        ) : (
          <ul className="space-y-3" data-testid={`outline-scene-list-${variant}`}>
            {data.scenes.map((scene) => {
              const beatTitle = data.beatTitleDraft[scene.sceneId] ?? '';
              const canAddBeat = beatTitle.trim().length > 0;
              return (
                <li
                  key={scene.sceneId}
                  className="rounded-card border border-gray-alpha-300 bg-background-100 p-3"
                >
                  <div className="flex flex-wrap items-center justify-between gap-2">
                    <div className="flex items-center gap-2">
                      <span className="text-label-14 font-semibold text-gray-900">
                        {scene.title}
                      </span>
                      <Badge variant="neutral" data-testid={`outline-scene-chapter-badge-${scene.sceneId}`}>
                        Ch. {scene.chapter}
                      </Badge>
                    </div>
                    <Button
                      type="button"
                      variant="tertiary"
                      size="small"
                      data-testid={`outline-remove-scene-${scene.sceneId}`}
                    >
                      <Trash2 className="h-4 w-4" aria-hidden />
                      Remove
                    </Button>
                  </div>

                  {scene.beats.length > 0 ? (
                    <ul className="mt-2 space-y-1">
                      {scene.beats.map((beat) => (
                        <li
                          key={beat.beatId}
                          className="flex items-center justify-between gap-2 text-copy-13 text-gray-900"
                        >
                          <span>{beat.title}</span>
                          <Button
                            type="button"
                            variant="tertiary"
                            size="small"
                            data-testid={`outline-remove-beat-${beat.beatId}`}
                          >
                            <Trash2 className="h-4 w-4" aria-hidden />
                            Remove
                          </Button>
                        </li>
                      ))}
                    </ul>
                  ) : null}

                  <div className="mt-2 flex flex-wrap items-center gap-2">
                    <input
                      type="text"
                      data-testid={`outline-beat-title-${scene.sceneId}`}
                      aria-label="Beat title"
                      defaultValue={beatTitle}
                      className={INPUT_CLASS}
                    />
                    <Button
                      type="button"
                      variant="secondary"
                      size="small"
                      data-testid={`outline-add-beat-${scene.sceneId}`}
                      disabled={!canAddBeat}
                    >
                      <Plus className="h-4 w-4" aria-hidden />
                      Add beat
                    </Button>
                  </div>
                </li>
              );
            })}
          </ul>
        )}
      </CardContent>
    </Card>
  );
}

function SceneBeatAuthoringFixtureFrame() {
  return (
    <FixtureFrame
      title="Outline — Scenes & Beats authoring card"
      description="Studio mirror of the v1.200 sceneBeatAuthoring panel (outline-canvas.tsx): add-scene / add-beat inputs, chapter badge, remove buttons, disabled states. Empty — no chapters, add gated off + honest empty state. With draft — scene and beat titles in progress, add buttons enabled. Bound — scenes bound to chapters and beats bound to scenes, remove affordances live. Light and dark themes per variant (dark panel is scoped .dark)."
      testId="outline-authoring-fixture-card"
    >
      <div className="flex flex-col gap-6">
        <ThemePair testId="outline-authoring-card-empty" label="Empty — no chapters, no scenes">
          <AuthoringCard variant="empty" />
        </ThemePair>
        <ThemePair testId="outline-authoring-card-draft" label="With draft — titles in progress">
          <AuthoringCard variant="draft" />
        </ThemePair>
        <ThemePair testId="outline-authoring-card-bound" label="Bound — scenes and beats authored">
          <AuthoringCard variant="bound" />
        </ThemePair>
      </div>
    </FixtureFrame>
  );
}

/* ------------------------------------------------------------------ */
/*  Surface 2 — Timeline World-event bind/unbind control mirror         */
/* ------------------------------------------------------------------ */

/** One selectable World KB `block_type=event` option (DR-26 picker source). */
const WORLD_EVENT_OPTIONS = [
  { id: 'evt-fall-ashen', name: 'The Fall of Ashen Gate' },
  { id: 'evt-ride-ford', name: 'Ride to the Ford' },
];

const WORLD_EVENT_REQUIRED = 'Bind a World to this Work to bind World events.';

type BindVariant = 'unbound' | 'bound' | 'disabled';

function WorldEventBindRow({
  variant,
  bound,
}: {
  variant: BindVariant;
  /** Whether this Narrative event carries a `world_event_id` binding. */
  bound: boolean;
}) {
  const hasBoundWorld = variant !== 'disabled';
  const title = 'The Crossing';

  return (
    <li
      className="rounded-control border border-gray-alpha-300 bg-background-100 p-2"
      data-testid={`outline-world-event-row-${bound ? 'bound' : 'unbound'}-${variant}`}
    >
      <div className="flex items-start justify-between">
        <div>
          <p className="text-copy-14 font-medium text-gray-1000">{title}</p>
        </div>
      </div>

      {bound ? (
        <div className="mt-1.5 flex items-center gap-1.5">
          <span className="min-w-0 flex-1 truncate text-label-12 text-gray-700">
            World event: The Fall of Ashen Gate
          </span>
          <Button
            variant="secondary"
            size="small"
            data-testid={`outline-world-event-unbind-${variant}`}
            disabled={!hasBoundWorld}
            title={hasBoundWorld ? 'Unbind World event' : WORLD_EVENT_REQUIRED}
          >
            Unbind
          </Button>
        </div>
      ) : (
        <div className="mt-1.5 flex items-center gap-1.5">
          <select
            data-testid={`outline-world-event-select-${variant}`}
            disabled={!hasBoundWorld}
            title={hasBoundWorld ? undefined : WORLD_EVENT_REQUIRED}
            className={SELECT_CLASS}
            aria-label={`World event for ${title}`}
            defaultValue=""
          >
            <option value="">Bind World event…</option>
            {WORLD_EVENT_OPTIONS.map((option) => (
              <option key={option.id} value={option.id}>
                {option.name}
              </option>
            ))}
          </select>
          <Button
            variant="secondary"
            size="small"
            data-testid={`outline-world-event-bind-${variant}`}
            // App rule: Bind needs a bound World AND a selected World event
            // (`!boundWorldId || !worldEventTargetByEvent[eventId]`). The
            // static unbound fixture mirrors the initial no-selection state,
            // so Bind stays gated off there too.
            disabled={!hasBoundWorld || variant === 'unbound'}
          >
            Bind
          </Button>
        </div>
      )}
    </li>
  );
}

function WorldEventBindPanel({ variant }: { variant: BindVariant }) {
  const hasBoundWorld = variant !== 'disabled';
  return (
    <Card data-testid={`outline-world-event-panel-${variant}`}>
      <CardHeader>
        <CardTitle className="flex items-center gap-2">
          <CalendarPlus className="h-5 w-5 text-canvas-outline-timeline-marker" aria-hidden />
          Timeline
        </CardTitle>
        <CardDescription>Events, beats, and foreshadow links.</CardDescription>
      </CardHeader>
      <CardContent className="space-y-4">
        {!hasBoundWorld ? (
          <p className="text-label-12 text-gray-700" data-testid="outline-world-event-required-hint">
            {WORLD_EVENT_REQUIRED}
          </p>
        ) : null}
        <ul className="space-y-2">
          <WorldEventBindRow variant={variant} bound={false} />
          <WorldEventBindRow variant={variant} bound />
        </ul>
      </CardContent>
    </Card>
  );
}

function WorldEventBindFixtureFrame() {
  return (
    <FixtureFrame
      title="Outline Timeline — World-event bind / unbind"
      description="Studio mirror of the v1.200 DR-26 bind/unbind control (event-inspector.tsx): event-only World-event picker, bound state with Unbind, and the disabled-when-no-bound-World rule — the affordance is disabled, never hidden, and carries the refusal reason as hint copy + control title. Unbound — picker enabled, Bind gated on a selected World event. Bound — canonical World-event name with a live Unbind. Disabled — no bound World: hint paragraph, picker/Bind/Unbind all disabled. Light and dark themes per variant (dark panel is scoped .dark)."
      testId="outline-world-event-fixture"
    >
      <div className="flex flex-col gap-6">
        <ThemePair testId="outline-world-event-unbound" label="Unbound — World bound, no World event selected">
          <WorldEventBindPanel variant="unbound" />
        </ThemePair>
        <ThemePair testId="outline-world-event-bound" label="Bound — World event bound to the Narrative event">
          <WorldEventBindPanel variant="bound" />
        </ThemePair>
        <ThemePair testId="outline-world-event-disabled" label="Disabled — Work has no bound World">
          <WorldEventBindPanel variant="disabled" />
        </ThemePair>
      </div>
    </FixtureFrame>
  );
}

/* ------------------------------------------------------------------ */
/*  Public fixture component                                            */
/* ------------------------------------------------------------------ */

/**
 * Outline authoring + World-event binding fixtures — presentational-only
 * studio mirrors of the v1.200 canvas surfaces (Greptile issue 5
 * studio-first). No daemon, no RF, no contracts, no i18n.
 */
export function OutlineAuthoringFixtures() {
  return (
    <div className="studio-fixture-boundary" data-testid="outline-authoring-fixtures">
      <SceneBeatAuthoringFixtureFrame />
      <WorldEventBindFixtureFrame />
    </div>
  );
}
