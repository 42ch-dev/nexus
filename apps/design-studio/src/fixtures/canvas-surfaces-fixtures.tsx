/**
 * Studio fixtures for Canvas surfaces (V1.108 P1 FB-UI-004).
 *
 * Presentational-only preview of the shared canvas shell chrome + context-menu
 * chrome. No `@xyflow/react`, no `@42ch/nexus-contracts`, no daemon data.
 *
 * Outline / Strategy / Scene-Beat node chrome consumes the shared
 * `NodeChromeShell` presentational extract (`@web-canvas/node-chrome-shell`)
 * — the same source the App RF node wrappers use. Because the extract is
 * free of `@xyflow/react`, Studio imports it directly without pulling RF
 * into the gallery. The token values are identical, so light/dark visual
 * acceptance here carries to the App graph.
 *
 * Context-menu chrome mirrors `path-context-menu.tsx` and
 * `world-kb-entity-context-menu.tsx` (role="menu", rounded-popover, shadow).
 */
import { type ReactNode } from 'react';
import {
  AlertTriangle,
  Copy,
  ExternalLink,
  FolderSearch,
  Info,
  Link2,
  Maximize2,
  Minimize2,
  Plus,
  ZoomIn,
  ZoomOut,
} from 'lucide-react';

import {
  NodeChromeShell,
  NODE_STATUS_DOT,
  type NodeStatus,
} from '@web-canvas/node-chrome-shell';

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
/*  Outline node chrome — consumes the shared NodeChromeShell extract     */
/*  (@web-canvas/node-chrome-shell). Same canvas-outline-* / canvas-node-* */
/*  tokens as the App; no RF types.                                      */
/* ------------------------------------------------------------------ */

/**
 * Status → `canvas-outline-chapter-card-status-*` token name.
 * Mirrors the P0 STATUS_TOKEN_VAR mapping (V1.72 compass alias):
 *   not_started/outlined → pending, draft → drafted, finalized/published → completed.
 */
const CHAPTER_STATUS_TOKENS = {
  pending: '--color-canvas-outline-chapter-card-status-pending',
  drafted: '--color-canvas-outline-chapter-card-status-drafted',
  completed: '--color-canvas-outline-chapter-card-status-completed',
} as const;

const CHAPTER_STATUS_LABELS = {
  pending: 'Not started',
  drafted: 'Draft',
  completed: 'Finalized',
} as const;

type ChapterStatusKey = keyof typeof CHAPTER_STATUS_TOKENS;

/** Volume lane node — mirrors P0 OutlineVolumeNode. */
function VolumeNodeSample({ label, chapterCount }: { label: string; chapterCount: number }) {
  return (
    <NodeChromeShell
      selected={false}
      style={{ background: 'var(--color-canvas-outline-volume-fill)' }}
    >
      <span className="font-heading text-copy-14 font-semibold text-gray-1000">{label}</span>
      <p className="mt-0.5 text-label-12 text-gray-700">
        {chapterCount} {chapterCount === 1 ? 'chapter' : 'chapters'}
      </p>
    </NodeChromeShell>
  );
}

/** Chapter card node — mirrors P0 OutlineChapterNode (status paint + slug + words). */
function ChapterNodeSample({
  title,
  status,
  slug,
  actualWords,
  plannedWords,
  selected = false,
}: {
  title: string;
  status: ChapterStatusKey;
  slug: string | null;
  actualWords: number;
  plannedWords: number;
  selected?: boolean;
}) {
  const tokenVar = `var(${CHAPTER_STATUS_TOKENS[status]})`;
  return (
    <NodeChromeShell selected={selected}>
      <div className="flex items-center justify-between gap-2">
        <span
          className="truncate font-heading text-copy-14 font-semibold text-gray-1000"
          title={title}
        >
          {title}
        </span>
      </div>
      <div className="mt-1 flex flex-wrap items-center gap-1">
        <span
          className="flex items-center gap-1 rounded-pill bg-gray-alpha-100 px-1.5 py-0.5 text-label-12"
          style={{
            color: tokenVar,
            background: `color-mix(in srgb, ${tokenVar} 12%, transparent)`,
          }}
        >
          <span
            className="inline-block h-2 w-2 rounded-pill"
            style={{ background: tokenVar }}
            aria-hidden
          />
          {CHAPTER_STATUS_LABELS[status]}
        </span>
        {slug ? (
          <span className="rounded-pill bg-gray-alpha-100 px-1.5 py-0.5 font-mono text-label-12 text-gray-700">
            {slug}
          </span>
        ) : null}
      </div>
      <p className="mt-1 text-label-12 text-gray-700">
        {actualWords}/{plannedWords} words
      </p>
    </NodeChromeShell>
  );
}

/** Timeline event node — mirrors P0 OutlineTimelineEventNode (pin border-left). */
function TimelineEventNodeSample({
  title,
  description,
  realizesLabel,
}: {
  title: string;
  description: string | null;
  realizesLabel: string;
}) {
  return (
    <NodeChromeShell
      selected={false}
      style={{
        borderLeftColor: 'var(--color-canvas-outline-timeline-event-pin)',
        borderLeftWidth: '3px',
      }}
    >
      <span
        className="truncate font-heading text-copy-14 font-semibold text-gray-1000"
        title={title}
      >
        {title}
      </span>
      {description ? (
        <p className="mt-0.5 line-clamp-2 text-copy-13 text-gray-900">{description}</p>
      ) : null}
      <p className="mt-0.5 text-label-12 text-gray-700">{realizesLabel}</p>
    </NodeChromeShell>
  );
}

/* ------------------------------------------------------------------ */
/*  Scene/Beat node chrome — consumes the shared NodeChromeShell extract  */
/*  (@web-canvas/node-chrome-shell). Same canvas-outline-scene-* /        */
/*  canvas-outline-beat-* tokens as the App.                             */
/* ------------------------------------------------------------------ */

const SCENE_STATUS_TOKENS = {
  drafted: '--color-canvas-outline-scene-status-drafted',
  completed: '--color-canvas-outline-scene-status-completed',
} as const;

const SCENE_STATUS_LABELS = {
  drafted: 'Drafted',
  completed: 'Completed',
} as const;

type SceneStatusKey = keyof typeof SCENE_STATUS_TOKENS;

/**
 * Scene node sample — mirrors V1.109 C2 OutlineSceneNode (title + status chip).
 * Consumes `canvas-outline-scene-fill` / `-border` / `-status-*` tokens.
 */
function SceneNodeSample({
  title,
  status,
  selected = false,
}: {
  title: string;
  status: SceneStatusKey | null;
  selected?: boolean;
}) {
  const tokenVar = status ? `var(${SCENE_STATUS_TOKENS[status]})` : null;
  return (
    <NodeChromeShell
      selected={selected}
      className="min-w-[160px]"
      style={{
        background: 'var(--color-canvas-outline-scene-fill)',
        borderColor: selected ? undefined : 'var(--color-canvas-outline-scene-border)',
      }}
    >
      <span
        className="truncate font-heading text-copy-14 font-semibold text-gray-1000"
        title={title}
      >
        {title}
      </span>
      {tokenVar ? (
        <div className="mt-1 flex flex-wrap items-center gap-1">
          <span
            className="flex items-center gap-1 rounded-pill bg-gray-alpha-100 px-1.5 py-0.5 text-label-12"
            style={{
              color: tokenVar,
              background: `color-mix(in srgb, ${tokenVar} 12%, transparent)`,
            }}
          >
            <span
              className="inline-block h-2 w-2 rounded-pill"
              style={{ background: tokenVar }}
              aria-hidden
            />
            {SCENE_STATUS_LABELS[status!]}
          </span>
        </div>
      ) : null}
    </NodeChromeShell>
  );
}

/**
 * Beat node sample — mirrors V1.109 C2 OutlineBeatNode (title only, no status).
 * Consumes `canvas-outline-beat-fill` / `-border` tokens.
 */
function BeatNodeSample({ title, selected = false }: { title: string; selected?: boolean }) {
  return (
    <NodeChromeShell
      selected={selected}
      className="min-w-[160px]"
      style={{
        background: 'var(--color-canvas-outline-beat-fill)',
        borderColor: selected ? undefined : 'var(--color-canvas-outline-beat-border)',
      }}
    >
      <span
        className="truncate font-heading text-copy-14 font-semibold text-gray-1000"
        title={title}
      >
        {title}
      </span>
    </NodeChromeShell>
  );
}

/** Connection port — mirrors the RF Handle visual (canvas-port token). */
function PortSample() {
  return (
    <span
      className="inline-block h-2.5 w-2.5 rounded-pill border-canvas-port bg-canvas-port"
      aria-hidden
    />
  );
}

/* ------------------------------------------------------------------ */
/*  Context menu chrome — mirrored from path-context-menu.tsx           */
/*  and world-kb-entity-context-menu.tsx (static; no click-away logic)   */
/* ------------------------------------------------------------------ */

function ContextMenuItem({
  icon,
  children,
}: {
  icon: ReactNode;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      role="menuitem"
      className="flex h-9 w-full items-center gap-2 rounded-control px-3 text-copy-14 text-gray-1000 hover:bg-gray-alpha-100 focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-700/40"
    >
      <span className="text-gray-900" aria-hidden>
        {icon}
      </span>
      {children}
    </button>
  );
}

function ContextMenuShell({
  label,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  return (
    <div
      role="menu"
      aria-label={label}
      className="min-w-[200px] rounded-popover border border-gray-alpha-400 bg-background-100 p-1 shadow-popover"
    >
      {children}
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Canvas shell chrome — mirrored from canvas-shell.tsx (presentational) */
/* ------------------------------------------------------------------ */

function CanvasShellChrome({
  selectedNodeId,
}: {
  selectedNodeId: string | null;
}) {
  return (
    <div
      className="relative h-[420px] w-full overflow-hidden rounded-card border border-gray-alpha-400 bg-canvas-surface"
      data-testid="canvas-shell-chrome"
    >
      {/* Screen-reader graph summary (A8) — mirrors CanvasShell sr-only region. */}
      <div className="sr-only" role="status" aria-live="polite" aria-atomic="true">
        Canvas preview — 1 volume, 3 chapters (not started, draft, finalized), 1 timeline event.
      </div>

      {/* Dot-grid background — mirrors CanvasShell Background variant=Dots. */}
      <div
        className="pointer-events-none absolute inset-0"
        style={{
          backgroundImage:
            'radial-gradient(var(--color-canvas-grid) 1.5px, transparent 1.5px)',
          backgroundSize: '20px 20px',
        }}
        aria-hidden
      />

      {/* Sample outline nodes — laid out statically to mirror the RF graph. */}
      <div className="relative flex h-full flex-wrap items-start gap-6 p-10">
        <div className="flex flex-col gap-3">
          <span className="text-label-12 font-medium text-gray-500">Volume lane</span>
          <div className="relative">
            <div className="absolute -right-3 top-1/2 -translate-y-1/2">
              <PortSample />
            </div>
            <VolumeNodeSample label="Volume I — Origins" chapterCount={3} />
          </div>
        </div>

        <div className="flex flex-col gap-3">
          <span className="text-label-12 font-medium text-gray-500">Chapter cards</span>
          <div className="relative">
            <div className="absolute -left-3 top-1/2 -translate-y-1/2">
              <PortSample />
            </div>
            <ChapterNodeSample
              title="Chapter 1 — The Call"
              status="pending"
              slug="ch-01"
              actualWords={0}
              plannedWords={3500}
            />
          </div>
          <div className="relative">
            <div className="absolute -left-3 top-1/2 -translate-y-1/2">
              <PortSample />
            </div>
            <ChapterNodeSample
              title="Chapter 2 — Crossing"
              status="drafted"
              slug="ch-02"
              actualWords={2100}
              plannedWords={4000}
            />
          </div>
          <div className="relative">
            <div className="absolute -left-3 top-1/2 -translate-y-1/2">
              <PortSample />
            </div>
            <ChapterNodeSample
              title="Chapter 3 — Threshold"
              status="completed"
              slug="ch-03"
              actualWords={3800}
              plannedWords={3800}
              selected={selectedNodeId === 'ch-03'}
            />
          </div>
        </div>

        <div className="flex flex-col gap-3">
          <span className="text-label-12 font-medium text-gray-500">Timeline lane</span>
          <div className="relative">
            <div className="absolute -left-3 top-1/2 -translate-y-1/2">
              <PortSample />
            </div>
            <TimelineEventNodeSample
              title="Inciting Incident"
              description="The protagonist receives the letter."
              realizesLabel="Realizes chapter 1"
            />
          </div>
        </div>
      </div>

      {/* Controls — mirrors CanvasShell Controls (presentational; no RF state). */}
      <div
        className="absolute bottom-4 left-4 flex flex-col gap-1 rounded-card border border-gray-alpha-400 bg-background-100 p-1 shadow-popover"
        data-testid="canvas-shell-controls"
      >
        <button
          type="button"
          aria-label="Zoom in"
          className="flex h-8 w-8 items-center justify-center rounded-control text-gray-900 hover:bg-gray-alpha-100 focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-700/40"
        >
          <ZoomIn className="h-4 w-4" aria-hidden />
        </button>
        <button
          type="button"
          aria-label="Zoom out"
          className="flex h-8 w-8 items-center justify-center rounded-control text-gray-900 hover:bg-gray-alpha-100 focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-700/40"
        >
          <ZoomOut className="h-4 w-4" aria-hidden />
        </button>
        <button
          type="button"
          aria-label="Fit view"
          className="flex h-8 w-8 items-center justify-center rounded-control text-gray-900 hover:bg-gray-alpha-100 focus:outline-none focus-visible:ring-2 focus-visible:ring-blue-700/40"
        >
          <Maximize2 className="h-4 w-4" aria-hidden />
        </button>
      </div>

      {/* MiniMap — mirrors CanvasShell MiniMap (presentational swatch). */}
      <div
        className="absolute bottom-4 right-4 h-28 w-44 rounded-card border border-gray-alpha-400 bg-background-100 p-2 shadow-popover"
        data-testid="canvas-shell-minimap"
        aria-hidden
      >
        <div
          className="h-full w-full rounded-control"
          style={{
            backgroundColor: 'var(--color-canvas-minimap)',
            backgroundImage:
              'radial-gradient(var(--color-canvas-strategy-accent) 2px, transparent 2px)',
            backgroundSize: '12px 12px',
            backgroundPosition: 'center',
            backgroundRepeat: 'no-repeat',
          }}
        />
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Strategy surface chrome — consumes NodeChromeShell (@web-canvas) +    */
/*  mirrors strategy-canvas/state-machine + strategy-canvas/inspector-    */
/*  panel. Static props into the shared extract (no RF types, no App      */
/*  canvas import). Tokens: canvas-strategy-accent (purple) + generic     */
/*  canvas-node-*. Status overlay uses semantic colors (Draft §3.6:       */
/*  canvas tokens cover shared primitives only).                         */
/* ------------------------------------------------------------------ */

/**
 * Status → label. The ring + dot classes come from the shared
 * `NodeChromeShell` extract (`NODE_STATUS_RING` / `NODE_STATUS_DOT`); only the
 * human-readable label is Studio-owned (static — the App resolves the same
 * statuses via i18n `t()` at render time).
 */
const STRATEGY_STATUS_LABEL = {
  current: 'Current',
  running: 'Running',
  waiting: 'Waiting',
  error: 'Error',
  completed: 'Completed',
} as const;

type StrategyStatus = NodeStatus;

/** Header row — label + status chip. Consumes the shared NODE_STATUS_DOT. */
function StrategyNodeHeader({
  label,
  status,
}: {
  label: string;
  status: StrategyStatus | null;
}) {
  return (
    <div className="flex items-center justify-between gap-2">
      <span
        className="truncate font-heading text-copy-14 font-semibold text-gray-1000"
        title={label}
      >
        {label}
      </span>
      {status ? (
        <span className="flex items-center gap-1 text-label-12 text-gray-700">
          <span
            className={`inline-block h-2 w-2 rounded-pill ${NODE_STATUS_DOT[status]}`}
            aria-hidden
          />
          {STRATEGY_STATUS_LABEL[status]}
        </span>
      ) : null}
    </div>
  );
}

/** State-kind mono tag — mirrors KindTag in strategy-nodes.tsx. */
function StrategyKindTag({ kind }: { kind: string }) {
  return (
    <span className="mt-0.5 inline-block rounded-pill bg-gray-alpha-100 px-1.5 py-0.5 font-mono text-label-12 text-gray-700">
      {kind}
    </span>
  );
}

/** Outer state-machine state — mirrors StrategyStateNode (accent + Start tag). */
function StrategyStateSample({
  label,
  stateKind,
  description,
  isInitial,
  status = null,
  selected = false,
}: {
  label: string;
  stateKind: string;
  description: string | null;
  isInitial: boolean;
  status?: StrategyStatus | null;
  selected?: boolean;
}) {
  return (
    <NodeChromeShell selected={selected} status={status ?? undefined} accent>
      <StrategyNodeHeader label={label} status={status} />
      <StrategyKindTag kind={stateKind} />
      {description ? (
        <p className="mt-1 line-clamp-2 text-copy-13 text-gray-900">{description}</p>
      ) : null}
      {isInitial ? (
        <span className="mt-1 inline-block text-label-12 text-purple-700">Start</span>
      ) : null}
    </NodeChromeShell>
  );
}

/** Converge merge-point join — mirrors StrategyJoinNode (Join · strategy chip). */
function StrategyJoinSample({
  label,
  convergeStrategy = 'wait_for_all',
  status = null,
  selected = false,
}: {
  label: string;
  convergeStrategy?: string;
  status?: StrategyStatus | null;
  selected?: boolean;
}) {
  return (
    <NodeChromeShell selected={selected} status={status ?? undefined}>
      <StrategyNodeHeader label={label} status={status} />
      <span className="mt-0.5 inline-block rounded-pill bg-[color-mix(in_srgb,var(--color-purple-700)_12%,transparent)] px-1.5 py-0.5 text-label-12 text-purple-1000">
        Join · {convergeStrategy}
      </span>
    </NodeChromeShell>
  );
}

/** Terminal state — mirrors StrategyTerminalNode (End). */
function StrategyTerminalSample({
  label,
  status = null,
  selected = false,
}: {
  label: string;
  status?: StrategyStatus | null;
  selected?: boolean;
}) {
  return (
    <NodeChromeShell selected={selected} status={status ?? undefined} className="min-w-[140px]">
      <StrategyNodeHeader label={label} status={status} />
      <span className="mt-0.5 inline-block text-label-12 text-gray-700">End</span>
    </NodeChromeShell>
  );
}

/**
 * Labeled transition edge — static stand-in for the RF `strategy-edge` with a
 * `label: condition`. The App renders bezier paths with a label badge; here a
 * horizontal connector with a centered condition pill captures the same
 * visual contract. Accent stripe uses `canvas-strategy-accent`.
 */
function StrategyEdgeSample({
  label,
  kind = 'next',
}: {
  label: string | null;
  kind?: 'next' | 'branch' | 'default' | 'converge' | 'depends_on';
}) {
  return (
    <div
      className="flex items-center gap-1.5"
      data-testid="strategy-edge-sample"
      aria-label={`Transition: ${kind}${label ? ` · ${label}` : ''}`}
    >
      <span
        className="inline-block h-px w-8"
        style={{
          backgroundImage:
            'linear-gradient(to right, var(--color-canvas-strategy-accent) 60%, transparent 60%)',
          backgroundSize: '6px 1px',
        }}
        aria-hidden
      />
      <span
        className="inline-flex items-center rounded-pill px-1.5 py-0.5 text-label-12"
        style={{
          color: 'var(--color-canvas-strategy-accent)',
          background:
            'color-mix(in srgb, var(--color-canvas-strategy-accent) 12%, transparent)',
        }}
      >
        {label ?? kind}
      </span>
      <span
        className="inline-block h-px w-8"
        style={{
          backgroundImage:
            'linear-gradient(to right, var(--color-canvas-strategy-accent) 60%, transparent 60%)',
          backgroundSize: '6px 1px',
        }}
        aria-hidden
      />
      <span
        className="inline-block h-0 w-0"
        style={{
          borderTop: '3px solid transparent',
          borderBottom: '3px solid transparent',
          borderLeft: '5px solid var(--color-canvas-strategy-accent)',
        }}
        aria-hidden
      />
    </div>
  );
}

/**
 * Inspector panel chrome — mirrors the read-only `ReadOnlyDetails` in
 * `inspector-panel.tsx` (top-right aside: Info icon + label + dl of Kind /
 * State id / Initial / Status / description / prompt ref). Static — no edit
 * toggle wiring, no conflict modal.
 */
function StrategyInspectorSample() {
  return (
    <aside
      className="absolute right-3 top-3 w-[280px] rounded-card border border-gray-alpha-400 bg-background-100 p-3 shadow-popover"
      aria-label="Selected node details"
      data-testid="strategy-inspector-chrome"
    >
      <div className="flex items-center gap-2">
        <Info className="h-4 w-4 text-purple-700" aria-hidden />
        <h3 className="font-heading text-heading-16 text-gray-1000">Drafting</h3>
      </div>
      <dl className="mt-2 flex flex-col gap-1 text-copy-13">
        <div className="flex justify-between">
          <dt className="text-gray-700">Kind</dt>
          <dd className="font-mono text-gray-1000">standard</dd>
        </div>
        <div className="flex justify-between">
          <dt className="text-gray-700">State id</dt>
          <dd className="font-mono text-gray-1000">drafting</dd>
        </div>
        <div className="text-purple-700">Initial state</div>
        <div className="flex justify-between">
          <dt className="text-gray-700">Status</dt>
          <dd className="text-blue-700">current</dd>
        </div>
        <p className="mt-2 text-gray-900">Author writes the first draft of the chapter.</p>
        <p className="mt-2 text-gray-700">
          Prompt: <span className="font-mono">prompts/draft-chapter.md</span>
        </p>
      </dl>
    </aside>
  );
}

/**
 * Validation panel chrome — mirrors `ValidationPanel` in `state-machine.tsx`
 * (bottom-left, amber AlertTriangle + "Validation notes" + problem list +
 * dangling transitions). Static.
 */
function StrategyValidationSample() {
  return (
    <div
      className="absolute bottom-3 left-3 max-w-[360px] rounded-card border border-amber-700/40 bg-background-100 p-2 text-copy-13 shadow-popover"
      role="status"
      data-testid="strategy-validation-chrome"
    >
      <div className="flex items-center gap-1.5 text-amber-1000">
        <AlertTriangle className="h-4 w-4" aria-hidden />
        <span className="font-semibold">Validation notes</span>
      </div>
      <ul className="mt-1 flex flex-col gap-0.5 text-gray-900">
        <li className="text-amber-1000">Dangling transition: drafting → unknown_target</li>
      </ul>
    </div>
  );
}

/**
 * Strategy shell chrome — mirrors `StrategyCanvas` + `CanvasShell` layout:
 * dot-grid surface, state-machine nodes laid out in layers with labeled
 * transition edges, a top-right inspector aside, and a bottom-left validation
 * panel. No live graph — nodes and edges are static markup.
 */
function StrategyShellChrome() {
  return (
    <div
      className="relative h-[460px] w-full overflow-hidden rounded-card border border-gray-alpha-400 bg-canvas-surface"
      data-testid="strategy-shell-chrome"
    >
      {/* sr-only graph summary — mirrors CanvasShell aria-live region. */}
      <div className="sr-only" role="status" aria-live="polite" aria-atomic="true">
        Strategy preview — 4 states (drafting, revising, done), 1 join, 1 labeled transition.
      </div>

      {/* Dot-grid background — mirrors CanvasShell Background variant=Dots. */}
      <div
        className="pointer-events-none absolute inset-0"
        style={{
          backgroundImage:
            'radial-gradient(var(--color-canvas-grid) 1.5px, transparent 1.5px)',
          backgroundSize: '20px 20px',
        }}
        aria-hidden
      />

      {/* Static state-machine graph — initial → join → terminal. */}
      <div className="relative flex h-full flex-col items-start justify-center gap-4 p-10">
        <div className="flex flex-col gap-2">
          <span className="text-label-12 font-medium text-gray-500">Initial</span>
          <StrategyStateSample
            label="Drafting"
            stateKind="standard"
            description="Author writes the first draft of the chapter."
            isInitial
            status="current"
          />
        </div>

        <div className="pl-6">
          <StrategyEdgeSample label="draft_ready" kind="next" />
        </div>

        <div className="flex flex-col gap-2">
          <span className="text-label-12 font-medium text-gray-500">Join</span>
          <StrategyJoinSample label="Converge" convergeStrategy="wait_for_all" />
        </div>

        <div className="pl-6">
          <StrategyEdgeSample label="all_done" kind="converge" />
        </div>

        <div className="flex flex-col gap-2">
          <span className="text-label-12 font-medium text-gray-500">Terminal</span>
          <StrategyTerminalSample label="Done" status="completed" />
        </div>
      </div>

      <StrategyInspectorSample />
      <StrategyValidationSample />
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  World KB surface chrome — mirrored from world-kb/entity-node.tsx +    */
/*  world-kb/relationship-projection.ts + relationship-confidence.ts +    */
/*  relationship-inspector.tsx. Hand-mirrored static markup (no RF types, */
/*  no App canvas import, no contracts).                                 */
/*  Tokens: canvas-worldkb-* family (entity-card-fill/stroke, promotion-*, */
/*  relationship-edge-*, confidence-*, source-anchor-*, inspector-fill).  */
/* ------------------------------------------------------------------ */

/**
 * Lifecycle → promotion-state badge. Mirrors the LIFECYCLE_BADGE map in
 * `entity-node.tsx` verbatim. The promotion-* token IS the lifecycle color
 * (state is never color-only — Draft §4.4 #6).
 */
const WORLDKB_LIFECYCLE_BADGE = {
  pending: {
    label: 'Pending',
    tokenVar: 'var(--color-canvas-worldkb-promotion-pending)',
  },
  confirmed: {
    label: 'Confirmed',
    tokenVar: 'var(--color-canvas-worldkb-promotion-confirmed)',
  },
  rejected: {
    label: 'Rejected',
    tokenVar: 'var(--color-canvas-worldkb-promotion-rejected)',
  },
  merged: {
    label: 'Merged',
    tokenVar: 'var(--color-canvas-worldkb-promotion-merged)',
  },
} as const;

type WorldKbLifecycle = keyof typeof WORLDKB_LIFECYCLE_BADGE;

/** Entity node card — mirrors WorldKbEntityNode (entity-node.tsx). */
function WorldKbEntityNodeSample({
  name,
  entityKind,
  lifecycle,
  sourceAnchorCount,
  version,
  computable = false,
  selected = false,
}: {
  name: string;
  entityKind: string;
  lifecycle: WorldKbLifecycle;
  sourceAnchorCount: number;
  version: number;
  computable?: boolean;
  selected?: boolean;
}) {
  const badge = WORLDKB_LIFECYCLE_BADGE[lifecycle];
  return (
    <div
      className={[
        'min-w-[200px] max-w-[240px] rounded-card border px-3 py-2 shadow-card transition-colors duration-state ease-standard',
        selected
          ? 'border-canvas-worldkb-entity-card-stroke-selected bg-canvas-worldkb-entity-card-fill-selected'
          : 'border-canvas-worldkb-entity-card-stroke-default bg-canvas-worldkb-entity-card-fill-default',
      ].join(' ')}
      style={selected ? { outline: '2px solid var(--color-canvas-worldkb-focus-ring)' } : undefined}
    >
      <div className="flex items-center justify-between gap-2">
        <span
          className="truncate font-heading text-copy-14 font-semibold text-gray-1000"
          title={name}
        >
          {name || '(unnamed)'}
        </span>
      </div>
      <div className="mt-1 flex flex-wrap items-center gap-1">
        <span className="rounded-pill bg-gray-alpha-100 px-1.5 py-0.5 font-mono text-label-12 text-gray-700">
          {entityKind}
        </span>
        <span
          className="flex items-center gap-1 rounded-pill border px-1.5 py-0.5 text-label-12"
          style={{
            color: badge.tokenVar,
            background: `color-mix(in srgb, ${badge.tokenVar} 15%, transparent)`,
            borderColor: `color-mix(in srgb, ${badge.tokenVar} 30%, transparent)`,
          }}
        >
          <span
            className="inline-block h-2 w-2 rounded-pill"
            style={{ background: badge.tokenVar }}
            aria-hidden
          />
          {badge.label}
        </span>
        {computable ? (
          <span
            className="rounded-pill border px-1.5 py-0.5 text-label-12"
            style={{
              color: 'var(--color-canvas-worldkb-computable-badge)',
              borderColor: 'color-mix(in srgb, var(--color-canvas-worldkb-computable-badge) 30%, transparent)',
              background:
                'color-mix(in srgb, var(--color-canvas-worldkb-computable-badge) 15%, transparent)',
            }}
          >
            Computable
          </span>
        ) : null}
      </div>
      <p className="mt-1 text-label-12 text-gray-700">
        {sourceAnchorCount} {sourceAnchorCount === 1 ? 'source anchor' : 'source anchors'} · v{version}
      </p>
    </div>
  );
}

/** Read-only source-anchor provenance origin — mirrors WorldKbSourceAnchorNode. */
function WorldKbSourceAnchorNodeSample({
  sourceType,
  reference,
}: {
  sourceType: string;
  reference: string;
}) {
  return (
    <div
      className="min-w-[140px] max-w-[180px] rounded-card border border-canvas-worldkb-source-anchor-edge/40 bg-canvas-worldkb-source-anchor-node px-2 py-1 shadow-card"
      aria-label={`Source anchor: ${reference}`}
    >
      <p
        className="truncate font-mono text-label-12 text-gray-700"
        title={reference}
      >
        {sourceType}
      </p>
      <p className="truncate text-label-12 text-gray-900" title={reference}>
        {reference}
      </p>
    </div>
  );
}

/**
 * Relationship confidence band → stroke-width / opacity / badge token.
 * Mirrors the PM-locked stepped bands in relationship-confidence.ts:
 * low (1px / 0.3), mid (2px / 0.6), high (3px / 1.0).
 */
const WORLDKB_CONFIDENCE_BAND = {
  low: {
    label: 'Low',
    strokeWidth: 1,
    opacity: 0.3,
    tokenVar: 'var(--color-canvas-worldkb-relationship-confidence-low)',
  },
  mid: {
    label: 'Medium',
    strokeWidth: 2,
    opacity: 0.6,
    tokenVar: 'var(--color-canvas-worldkb-relationship-confidence-mid)',
  },
  high: {
    label: 'High',
    strokeWidth: 3,
    opacity: 1.0,
    tokenVar: 'var(--color-canvas-worldkb-relationship-confidence-high)',
  },
} as const;

type WorldKbConfidenceBand = keyof typeof WORLDKB_CONFIDENCE_BAND;

/** Relationship edge kind → base stroke token. Mirrors deriveRelationshipEdges. */
const WORLDKB_RELATIONSHIP_EDGE_TOKEN = {
  default: 'var(--color-canvas-worldkb-relationship-edge-default)',
  symmetric: 'var(--color-canvas-worldkb-relationship-edge-symmetric)',
  custom: 'var(--color-canvas-worldkb-relationship-edge-custom)',
} as const;

type WorldKbRelationshipKind = keyof typeof WORLDKB_RELATIONSHIP_EDGE_TOKEN;

/**
 * Labeled relationship edge — static stand-in for the RF
 * `world-kb-relationship-edge`. Mirrors deriveRelationshipEdges +
 * confidenceEdgeStyle: base stroke color by kind, then the confidence band
 * modulates stroke-width + opacity. Suggested (`needs_review`) edges render
 * dashed regardless of band. Label pill shows the relation kind + "· suggested"
 * suffix.
 */
function WorldKbRelationshipEdgeSample({
  label,
  kind = 'default',
  band = 'high',
  suggested = false,
}: {
  label: string;
  kind?: WorldKbRelationshipKind;
  band?: WorldKbConfidenceBand;
  suggested?: boolean;
}) {
  const cfg = WORLDKB_CONFIDENCE_BAND[band];
  const stroke = WORLDKB_RELATIONSHIP_EDGE_TOKEN[kind];
  const labelText = suggested ? `${label} · suggested` : label;
  const dashStyle = suggested
    ? {
        backgroundImage: `linear-gradient(to right, ${stroke} 60%, transparent 60%)`,
        backgroundSize: '6px 1px',
      }
    : { background: stroke };
  return (
    <div
      className="flex items-center gap-1.5"
      data-testid="worldkb-relationship-edge-sample"
      aria-label={`Relationship: ${labelText} (${kind}, ${cfg.label.toLowerCase()} confidence)`}
    >
      <span
        className="inline-block w-10"
        style={{
          height: `${cfg.strokeWidth}px`,
          opacity: cfg.opacity,
          ...dashStyle,
        }}
        aria-hidden
      />
      <span
        className="inline-flex items-center gap-1 rounded-pill px-1.5 py-0.5 text-label-12"
        style={{
          color: stroke,
          background: `color-mix(in srgb, ${stroke} 12%, transparent)`,
        }}
      >
        {labelText}
      </span>
      <span
        className="inline-block w-10"
        style={{
          height: `${cfg.strokeWidth}px`,
          opacity: cfg.opacity,
          ...dashStyle,
        }}
        aria-hidden
      />
      <span
        className="inline-block h-0 w-0"
        style={{
          borderTop: '3px solid transparent',
          borderBottom: '3px solid transparent',
          borderLeft: `5px solid ${stroke}`,
          opacity: cfg.opacity,
        }}
        aria-hidden
      />
      <span
        className="inline-flex items-center gap-1 rounded-pill px-1.5 py-0.5 text-label-12"
        style={{
          color: cfg.tokenVar,
          background: `color-mix(in srgb, ${cfg.tokenVar} 12%, transparent)`,
        }}
      >
        <span
          className="inline-block h-2 w-2 rounded-pill"
          style={{ background: cfg.tokenVar }}
          aria-hidden
        />
        {cfg.label}
      </span>
    </div>
  );
}

/**
 * Source-anchor provenance edge — static stand-in for the RF `anchor:*` edge
 * (graph-projection.ts deriveEdges). Rendered as an undirected-style solid
 * connector in `canvas-worldkb-source-anchor-edge`. Read-only provenance — no
 * label, no confidence band.
 */
function WorldKbSourceAnchorEdgeSample() {
  return (
    <div
      className="flex items-center"
      data-testid="worldkb-source-anchor-edge-sample"
      aria-label="Source-anchor provenance edge"
    >
      <span
        className="inline-block h-px w-8"
        style={{ background: 'var(--color-canvas-worldkb-source-anchor-edge)' }}
        aria-hidden
      />
      <span
        className="inline-block h-0 w-0"
        style={{
          borderTop: '3px solid transparent',
          borderBottom: '3px solid transparent',
          borderLeft: '5px solid var(--color-canvas-worldkb-source-anchor-edge)',
        }}
        aria-hidden
      />
    </div>
  );
}

/**
 * Relationship inspector aside — mirrors `relationship-inspector.tsx`
 * (top-right aside: relation label + dl of Kind / Confidence / Symmetric /
 * Source-anchor grounding). Uses `canvas-worldkb-relationship-inspector-fill`
 * + grounded/asserted badge tokens. Static — no edit affordances.
 */
function WorldKbRelationshipInspectorSample() {
  return (
    <aside
      className="absolute right-3 top-3 w-[300px] rounded-card border border-gray-alpha-400 bg-canvas-worldkb-relationship-inspector-fill p-4 shadow-card"
      aria-label="Selected relationship details"
      data-testid="worldkb-inspector-chrome"
    >
      <div className="flex items-center gap-2">
        <Link2 className="h-4 w-4 text-purple-700" aria-hidden />
        <h3 className="font-heading text-heading-16 text-gray-1000">Allied With</h3>
      </div>
      <dl className="mt-2 flex flex-col gap-1 text-copy-13">
        <div className="flex justify-between">
          <dt className="text-gray-700">Kind</dt>
          <dd className="font-mono text-gray-1000">allied_with</dd>
        </div>
        <div className="flex justify-between">
          <dt className="text-gray-700">Confidence</dt>
          <dd className="text-gray-1000">0.82</dd>
        </div>
        <div className="flex justify-between">
          <dt className="text-gray-700">Symmetric</dt>
          <dd className="text-gray-1000">Yes</dd>
        </div>
        <div className="flex justify-between">
          <dt className="text-gray-700">Source</dt>
          <dd>
            <span
              className="rounded-pill px-1.5 py-0.5 text-label-12"
              style={{
                color: 'var(--color-blue-700)',
                background:
                  'var(--color-canvas-worldkb-relationship-grounded-badge)',
              }}
            >
              Grounded
            </span>
          </dd>
        </div>
      </dl>
      <p className="mt-2 text-gray-700">
        Backed by <span className="font-mono">2</span> source anchor(s).
      </p>
    </aside>
  );
}

/**
 * World KB shell chrome — mirrors `WorldKbCanvas` + `CanvasShell` layout:
 * dot-grid surface, source-anchor provenance nodes on the left, entity cards
 * in a lane with typed relationship edges between them, and a top-right
 * relationship inspector aside. No live graph — nodes and edges are static
 * markup. Covers all four lifecycle states, the three relationship kinds
 * (default / symmetric / custom), the three confidence bands, a suggested
 * (dashed) edge, and a source-anchor provenance chain.
 */
function WorldKbShellChrome() {
  return (
    <div
      className="relative h-[480px] w-full overflow-hidden rounded-card border border-gray-alpha-400 bg-canvas-surface"
      data-testid="worldkb-shell-chrome"
    >
      {/* sr-only graph summary — mirrors CanvasShell aria-live region + graphSummary. */}
      <div className="sr-only" role="status" aria-live="polite" aria-atomic="true">
        World KB preview — 3 entities (confirmed, confirmed selected, pending),
        2 relationships (default high-confidence, custom suggested), 1 source
        anchor with provenance edge, 1 merged lifecycle badge.
      </div>

      {/* Dot-grid background — mirrors CanvasShell Background variant=Dots. */}
      <div
        className="pointer-events-none absolute inset-0"
        style={{
          backgroundImage:
            'radial-gradient(var(--color-canvas-grid) 1.5px, transparent 1.5px)',
          backgroundSize: '20px 20px',
        }}
        aria-hidden
      />

      {/* Static World KB graph — anchors → entities → relationships. */}
      <div className="relative flex h-full flex-col items-start justify-center gap-4 p-10">
        {/* Source-anchor provenance chain: anchor → edge → confirmed entity. */}
        <div className="flex flex-col gap-2">
          <span className="text-label-12 font-medium text-gray-500">Source-anchor provenance</span>
          <div className="flex items-center gap-3">
            <WorldKbSourceAnchorNodeSample
              sourceType="manuscript"
              reference="ch-03¶7"
            />
            <WorldKbSourceAnchorEdgeSample />
            <WorldKbEntityNodeSample
              name="Kael Veynor"
              entityKind="Character"
              lifecycle="confirmed"
              sourceAnchorCount={2}
              version={3}
              computable={false}
              selected
            />
          </div>
        </div>

        {/* Relationship: confirmed ↔ confirmed (default kind, high confidence). */}
        <div className="pl-[200px]">
          <WorldKbRelationshipEdgeSample
            label="Allied With"
            kind="default"
            band="high"
          />
        </div>

        <div className="flex flex-col gap-2">
          <span className="text-label-12 font-medium text-gray-500">Entity lane · Character</span>
          <div className="flex items-center gap-6">
            {/* Merged entity — demonstrates the merged lifecycle badge. */}
            <WorldKbEntityNodeSample
              name="The Hearthstone Covenant"
              entityKind="Organization"
              lifecycle="merged"
              sourceAnchorCount={4}
              version={7}
            />
            {/* Suggested relationship: symmetric kind, mid confidence, dashed. */}
            <WorldKbRelationshipEdgeSample
              label="Rival Of"
              kind="symmetric"
              band="mid"
              suggested
            />
            <WorldKbEntityNodeSample
              name="Iron Vow"
              entityKind="Organization"
              lifecycle="pending"
              sourceAnchorCount={1}
              version={1}
            />
          </div>
        </div>

        {/* Custom-label relationship: rejected → confirmed, low confidence. */}
        <div className="pl-[200px]">
          <WorldKbRelationshipEdgeSample
            label="Sworn Enemy"
            kind="custom"
            band="low"
          />
        </div>

        <div className="flex flex-col gap-2">
          <span className="text-label-12 font-medium text-gray-500">Rejected entity · computable</span>
          <WorldKbEntityNodeSample
            name="Act II — Siege"
            entityKind="Act"
            lifecycle="rejected"
            sourceAnchorCount={0}
            version={1}
            computable
          />
        </div>
      </div>

      <WorldKbRelationshipInspectorSample />
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Public fixture component                                            */
/* ------------------------------------------------------------------ */

/**
 * Canvas Surfaces fixtures — shell chrome + outline node chrome + context-menu
 * chrome matrices + Strategy surface chrome + World KB surface chrome.
 * Presentational-only; no daemon, no live graph, no contracts.
 */
export function CanvasSurfacesFixtures() {
  return (
    <div data-testid="canvas-surfaces-fixtures">
      <FixtureFrame
        title="Canvas shell chrome"
        description="Shared canvas surface chrome — dot-grid background, controls, minimap, and sample outline nodes (Volume / Chapter / Timeline Event). Mirrors the App CanvasShell presentational structure using canvas-* tokens. No live graph — nodes are static markup."
        testId="canvas-fixture-shell"
      >
        <CanvasShellChrome selectedNodeId="ch-03" />
      </FixtureFrame>

      <FixtureFrame
        title="Outline node chrome matrix"
        description="Outline node kinds aligned with P0 outline-nodes.tsx — same canvas-outline-* tokens. Light/dark acceptance here carries to the App graph. Selection pairs canvas-node-border-selected with the focus ring (Draft §4.4 #6)."
        testId="canvas-fixture-node-matrix"
      >
        <div
          className="flex flex-wrap gap-6 rounded-card bg-canvas-surface p-6"
          data-testid="canvas-node-matrix"
        >
          <div className="flex flex-col gap-2">
            <span className="text-label-12 font-medium text-gray-500">Volume</span>
            <VolumeNodeSample label="Volume II — Journeys" chapterCount={5} />
          </div>

          <div className="flex flex-col gap-2">
            <span className="text-label-12 font-medium text-gray-500">Not started</span>
            <ChapterNodeSample
              title="Untitled chapter"
              status="pending"
              slug={null}
              actualWords={0}
              plannedWords={3000}
            />
          </div>

          <div className="flex flex-col gap-2">
            <span className="text-label-12 font-medium text-gray-500">Draft</span>
            <ChapterNodeSample
              title="Chapter 5 — The Climb"
              status="drafted"
              slug="ch-05"
              actualWords={1800}
              plannedWords={3500}
            />
          </div>

          <div className="flex flex-col gap-2">
            <span className="text-label-12 font-medium text-gray-500">Finalized (selected)</span>
            <ChapterNodeSample
              title="Chapter 6 — Descent"
              status="completed"
              slug="ch-06"
              actualWords={3600}
              plannedWords={3600}
              selected
            />
          </div>

          <div className="flex flex-col gap-2">
            <span className="text-label-12 font-medium text-gray-500">Timeline event</span>
            <TimelineEventNodeSample
              title="Midpoint Reversal"
              description={null}
              realizesLabel="Unattached event"
            />
          </div>

          {/* V1.109 C2 — Scene/Beat node chrome (FB-C2-001/004). Mirrors
              scene-beat-nodes.tsx using canvas-outline-scene-* / -beat-*
              tokens. Light/dark acceptance here carries to the App graph. */}
          <div className="flex flex-col gap-2">
            <span className="text-label-12 font-medium text-gray-500">Scene (drafted)</span>
            <SceneNodeSample title="Opening Scene" status="drafted" />
          </div>

          <div className="flex flex-col gap-2">
            <span className="text-label-12 font-medium text-gray-500">Scene (completed)</span>
            <SceneNodeSample title="Closing Scene" status="completed" />
          </div>

          <div className="flex flex-col gap-2">
            <span className="text-label-12 font-medium text-gray-500">Scene (no status)</span>
            <SceneNodeSample title="Untitled Scene" status={null} />
          </div>

          <div className="flex flex-col gap-2">
            <span className="text-label-12 font-medium text-gray-500">Beat</span>
            <BeatNodeSample title="Inciting Moment" />
          </div>

          <div className="flex flex-col gap-2">
            <span className="text-label-12 font-medium text-gray-500">Beat (selected)</span>
            <BeatNodeSample title="Turning Point" selected />
          </div>
        </div>
      </FixtureFrame>

      <FixtureFrame
        title="Context menu chrome matrix"
        description="Right-click menu chrome mirrored from path-context-menu.tsx and world-kb-entity-context-menu.tsx — role='menu', rounded-popover, shadow. Static (no click-away wiring); Title Case actions per Voice & Content."
        testId="canvas-fixture-context-menus"
      >
        <div
          className="flex flex-wrap gap-6"
          data-testid="canvas-context-menu-matrix"
        >
          <div className="flex flex-col gap-2">
            <span className="text-label-12 font-medium text-gray-500">Entity (World KB)</span>
            <ContextMenuShell label="Actions for Lore Fragment">
              <ContextMenuItem icon={<Link2 className="h-4 w-4" aria-hidden />}>
                Connect to…
              </ContextMenuItem>
            </ContextMenuShell>
          </div>

          <div className="flex flex-col gap-2">
            <span className="text-label-12 font-medium text-gray-500">Path (browser)</span>
            <ContextMenuShell label="Path context menu">
              <ContextMenuItem icon={<Copy className="h-4 w-4" aria-hidden />}>
                Copy Path
              </ContextMenuItem>
            </ContextMenuShell>
          </div>

          <div className="flex flex-col gap-2">
            <span className="text-label-12 font-medium text-gray-500">Path (desktop)</span>
            <ContextMenuShell label="Path context menu (desktop)">
              <ContextMenuItem icon={<Copy className="h-4 w-4" aria-hidden />}>
                Copy Path
              </ContextMenuItem>
              <ContextMenuItem icon={<ExternalLink className="h-4 w-4" aria-hidden />}>
                Open With…
              </ContextMenuItem>
              <ContextMenuItem icon={<FolderSearch className="h-4 w-4" aria-hidden />}>
                Reveal in Finder
              </ContextMenuItem>
            </ContextMenuShell>
          </div>

          <div className="flex flex-col gap-2">
            <span className="text-label-12 font-medium text-gray-500">Canvas (future)</span>
            <ContextMenuShell label="Canvas node context menu">
              <ContextMenuItem icon={<Plus className="h-4 w-4" aria-hidden />}>
                Add Chapter
              </ContextMenuItem>
              <ContextMenuItem icon={<Link2 className="h-4 w-4" aria-hidden />}>
                Connect to…
              </ContextMenuItem>
              <ContextMenuItem icon={<Minimize2 className="h-4 w-4" aria-hidden />}>
                Collapse
              </ContextMenuItem>
            </ContextMenuShell>
          </div>
        </div>
      </FixtureFrame>

      <FixtureFrame
        title="Strategy surface chrome"
        description="Strategy state-machine surface mirrored from strategy-nodes.tsx + strategy-canvas/state-machine + strategy-canvas/inspector-panel — outer states (accent stripe + stateKind tag + status ring), a join merge-point, a terminal, labeled transition edges (condition pill in canvas-strategy-accent), a read-only inspector aside, and an amber validation panel. Same canvas-node-* + canvas-strategy-accent tokens as the App; status overlay uses semantic colors (Draft §3.6). No @xyflow/react, no App canvas import — static markup."
        testId="canvas-fixture-strategy"
      >
        <StrategyShellChrome />
      </FixtureFrame>

      <FixtureFrame
        title="World KB surface chrome"
        description="World KB graph surface mirrored from world-kb/entity-node.tsx + relationship-projection.ts + relationship-confidence.ts + relationship-inspector.tsx — entity cards (name + BlockType tag + promotion-state lifecycle badge + computable chip + source-anchor count), source-anchor provenance origin nodes, typed relationship edges (default / symmetric / custom kind × low / mid / high confidence band × suggested-dashed), read-only source-anchor provenance edges, and a top-right relationship inspector aside with grounded-badge. Same canvas-worldkb-* tokens as the App. No @xyflow/react, no App canvas import, no contracts — static markup."
        testId="canvas-fixture-worldkb"
      >
        <WorldKbShellChrome />
      </FixtureFrame>
    </div>
  );
}
