/**
 * Studio fixtures for Canvas surfaces (V1.108 P1 FB-UI-004).
 *
 * Presentational-only preview of the shared canvas shell chrome + context-menu
 * chrome. No `@xyflow/react`, no `@42ch/nexus-contracts`, no daemon data.
 *
 * Outline node chrome mirrors the P0 `outline-nodes.tsx` visual structure
 * (Volume / Chapter / Timeline Event) using the same `canvas-outline-*` and
 * `canvas-node-*` CSS tokens shared via `@nexus/design-tokens`. Because Studio
 * cannot import `@xyflow/react` (Handle/Position/NodeProps) or `ChapterStatus`,
 * the nodes are mirrored as static presentational markup — not the live RF
 * nodes. The token values are identical, so light/dark visual acceptance here
 * carries to the App graph.
 *
 * Context-menu chrome mirrors `path-context-menu.tsx` and
 * `world-kb-entity-context-menu.tsx` (role="menu", rounded-popover, shadow).
 */
import { type ReactNode } from 'react';
import {
  Copy,
  ExternalLink,
  FolderSearch,
  Link2,
  Maximize2,
  Minimize2,
  Plus,
  ZoomIn,
  ZoomOut,
} from 'lucide-react';

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
/*  Outline node chrome — mirrored from P0 outline-nodes.tsx            */
/*  (same canvas-outline-* / canvas-node-* tokens; no RF types)         */
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

function NodeShell({
  selected,
  children,
  className,
  style,
}: {
  selected: boolean;
  children: ReactNode;
  className?: string;
  style?: React.CSSProperties;
}) {
  return (
    <div
      className={[
        'min-w-[176px] rounded-card border bg-canvas-node-fill px-3 py-2 shadow-card transition-colors duration-state ease-standard',
        selected ? 'border-canvas-node-border-selected' : 'border-canvas-node-border',
        className ?? '',
      ].join(' ')}
      style={style}
    >
      {children}
    </div>
  );
}

/** Volume lane node — mirrors P0 OutlineVolumeNode. */
function VolumeNodeSample({ label, chapterCount }: { label: string; chapterCount: number }) {
  return (
    <NodeShell
      selected={false}
      style={{ background: 'var(--color-canvas-outline-volume-fill)' }}
    >
      <span className="font-heading text-copy-14 font-semibold text-gray-1000">{label}</span>
      <p className="mt-0.5 text-label-12 text-gray-700">
        {chapterCount} {chapterCount === 1 ? 'chapter' : 'chapters'}
      </p>
    </NodeShell>
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
    <NodeShell selected={selected}>
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
    </NodeShell>
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
    <NodeShell
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
    </NodeShell>
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
/*  Public fixture component                                            */
/* ------------------------------------------------------------------ */

/**
 * Canvas Surfaces fixtures — shell chrome + outline node chrome + context-menu
 * chrome matrices. Presentational-only; no daemon, no live graph, no contracts.
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
    </div>
  );
}
