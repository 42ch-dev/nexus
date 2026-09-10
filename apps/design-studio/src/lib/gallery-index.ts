/**
 * Studio-local section catalog — metadata only (no token values or fixture JSX).
 */

export type GalleryEntry = {
  path: string;
  id: string;
  label: string;
  keywords: readonly string[];
  importPaths: readonly string[];
};

function entry(
  path: string,
  id: string,
  label: string,
  keywords: readonly string[] = [],
  importPaths: readonly string[] = [],
): GalleryEntry {
  return { path, id, label, keywords, importPaths };
}

const TOKENS = '@nexus/design-tokens';
const UI = '@42ch/nexus-ui';

const TOKEN_ENTRIES: readonly GalleryEntry[] = [
  entry('/tokens', 'tokens-colors', 'Colors', ['brand', 'palette', 'chronos', 'semantic'], [TOKENS]),
  entry('/tokens', 'tokens-typography', 'Typography', ['type', 'font', 'display', 'prose'], [TOKENS]),
  entry('/tokens', 'tokens-spacing', 'Spacing', ['space', 'gap', 'padding'], [TOKENS]),
  entry('/tokens', 'tokens-radius', 'Radius', ['rounded', 'corner'], [TOKENS]),
  entry('/tokens', 'tokens-elevation', 'Elevation', ['shadow', 'lift'], [TOKENS]),
  entry('/tokens', 'tokens-motion', 'Motion', ['duration', 'transition', 'reduced'], [TOKENS]),
  entry('/tokens', 'tokens-canvas', 'Canvas', ['node', 'grid', 'ambient'], [TOKENS]),
  entry('/tokens', 'tokens-states', 'States', ['error', 'success', 'disabled'], [TOKENS]),
  entry('/tokens', 'tokens-structural', 'Structural scalars & reading chrome', ['shell', 'chrome'], [TOKENS]),
];

const BRAND_ENTRIES: readonly GalleryEntry[] = [
  entry('/brand', 'brand-logos', 'Logo variants', ['vi', 'frozen', 'primary', 'wordmark'], [UI]),
  entry('/brand', 'brand-chronos', 'Chronos shell placement', ['titlebar', 'hero', 'lockup'], [UI]),
  entry('/brand', 'brand-mark', 'Mark', ['nexusmark', 'glyph'], [UI]),
  entry('/brand', 'brand-specimens', 'Theme specimens', ['palette', 'variant'], [UI]),
  entry('/brand', 'brand-theme-css', 'Theme variables', ['theme.css', 'swatch'], [UI]),
  entry('/brand', 'brand-clear-space', 'Clear space', ['spacing', 'minimum'], [UI]),
  entry('/brand', 'brand-vi-acceptance-heading', 'VI acceptance', ['button', 'transport'], [UI]),
];

const COMPONENT_ENTRIES: readonly GalleryEntry[] = [
  entry('/components', 'comp-badge', 'Badge', ['soft', 'solid', 'variant'], [UI]),
  entry('/components', 'comp-domain-badges', 'Domain Badges', ['finding', 'memory', 'reading'], [UI]),
  entry('/components', 'comp-button', 'Button', ['primary', 'secondary', 'ghost'], [UI]),
  entry('/components', 'comp-card', 'Card', ['interactive', 'voice'], [UI]),
  entry('/components', 'comp-dialog', 'Dialog', ['modal', 'portal', 'escape'], ['@web-ui/dialog']), // @web-ui/dialog — transitional — keep-web (Radix portal/focus-trap beyond presentational scope)
  entry('/components', 'comp-input', 'Input', ['invalid', 'focus'], [UI]),
  entry('/components', 'comp-label', 'Label', ['form'], [UI]),
  entry('/components', 'comp-select', 'Select', ['native', 'invalid'], [UI]),
  entry('/components', 'comp-states', 'States', ['loading', 'empty', 'error', 'spinner'], ['@web-ui/states']), // @web-ui/states — transitional — keep-web (lucide-react asset boundary; product copy & app-composition callbacks)
  entry('/components', 'comp-table', 'Table', ['overflow', 'row'], ['@web-ui/table']), // @web-ui/table — transitional — keep-web (responsive overflow wrapper; not in V1.99 first batch)
  entry('/components', 'comp-tabs', 'Tabs', ['controlled', 'panel'], [UI]),
  entry('/components', 'comp-textarea', 'Textarea', ['invalid', 'disabled'], [UI]),
  entry('/components', 'comp-form-field', 'Form Field (composition)', ['helper', 'label'], [UI]),
  entry('/components', 'comp-toast', 'Toast', ['notification', 'portal'], [UI]),
  entry('/components', 'comp-transport-error-block', 'Transport Error Block', ['retry', 'daemon'], [UI]),
  entry('/components', 'comp-run-studio', 'Run Studio (Compute)', ['proposal', 'runs'], [UI]),
  entry('/components', 'comp-compute-timeline', 'Compute Timeline', ['node', 'inspector'], [UI]),
  entry('/components', 'comp-vi-acceptance-heading', 'VI acceptance (P2)', ['acceptance', 'theme'], [UI]),
];

const VOICE_ENTRIES: readonly GalleryEntry[] = [
  entry('/voice', 'voice-guidance', 'Voice guidance summary', ['rules', 'design'], ['DESIGN.md', '@42ch/nexus-ui']),
  entry('/voice', 'voice-writing-patterns', 'Writing Patterns', ['title case', 'toast', 'empty', 'cta'], ['DESIGN.md', '@42ch/nexus-ui']),
];

const SURFACES_OVERVIEW_ENTRIES: readonly GalleryEntry[] = [
  entry('/surfaces/setup', 'surfaces-setup', 'Setup', ['wizard', 'steps'], ['@/pages/surfaces']),
  entry('/surfaces/shell', 'surfaces-chronos-titlebar', 'Shell', ['sidebar', 'titlebar', 'settings'], ['@/pages/surfaces']),
  entry('/surfaces/agent-picker', 'surfaces-agent-picker', 'AgentPicker', ['grid', 'loading', 'empty'], ['@/pages/surfaces']),
  entry('/surfaces/canvas', 'surfaces-canvas-mirrored', 'Canvas', ['outline', 'strategy', 'timeline'], ['@/pages/surfaces']),
  entry('/surfaces/daemon', 'surfaces-daemon', 'Daemon', ['status', 'strip'], ['@/pages/surfaces']),
  entry('/surfaces/launch', 'surfaces-launch', 'Launch', ['splash', 'waiting', 'recovery'], ['@/pages/surfaces']),
  entry('/surfaces/selection-submenu', 'surfaces-selection-submenu', 'Selection Submenu', ['context', 'rename', 'agent'], ['@/pages/surfaces']),
];

const SURFACES_SETUP_ENTRIES: readonly GalleryEntry[] = [
  entry('/surfaces/setup', 'surfaces-setup', 'Setup — Wizard chrome', ['wizard', 'back', 'continue'], [
    '@web-setup/top-step-indicator',
    '@web-setup/agent-picker',
  ]),
];

const SURFACES_SHELL_ENTRIES: readonly GalleryEntry[] = [
  entry('/surfaces/shell', 'surfaces-chronos-titlebar', 'Chronos titlebar', ['ink', 'mark'], [
    '@web-layout/chronos-titlebar-chrome',
  ]),
  entry('/surfaces/shell', 'surfaces-app-shell-chrome', 'App shell chrome', ['sidebar', 'nav'], [
    '@web-layout/shell-sidebar-chrome',
  ]),
  entry('/surfaces/shell', 'surfaces-creator-hub-dual-pane-ia', 'Creator Hub — sidebar create IA', ['hub', 'cards'], [
    '@web-layout/hub-card-list-pane',
  ]),
  entry('/surfaces/shell', 'surfaces-creator-orch-gongnengqu-ia', 'Creator / Orchestrator 功能区 IA', ['create', 'browse'], [
    '@web-layout/creator-shell-content',
  ]),
  entry('/surfaces/shell', 'surfaces-creator-shell', 'Creator shell — Create vs Controller', ['empty', 'controller'], [
    '@web-layout/creator-shell-content',
  ]),
  entry('/surfaces/shell', 'surfaces-settings-shell', 'Settings — Shell chrome', ['settings', 'sections'], [
    '@web-settings/settings-host-chrome',
  ]),
  entry('/surfaces/shell', 'surfaces-footer-profiles', 'Footer profiles', ['workspace', 'profile'], [
    '@web-layout/footer-profiles-chrome',
  ]),
  entry('/surfaces/shell', 'surfaces-header-health', 'Header health indicator', ['daemon', 'health'], [
    '@web-layout/daemon-health-indicator-chrome',
  ]),
];

const SURFACES_AGENT_PICKER_ENTRIES: readonly GalleryEntry[] = [
  entry('/surfaces/agent-picker', 'surfaces-agent-picker', 'Setup — AgentPicker', [
    'loading',
    'grid',
    'mixed',
    'empty',
    'error',
    'selected',
    'verify',
  ], ['@web-setup/agent-picker']),
];

const SURFACES_CANVAS_ENTRIES: readonly GalleryEntry[] = [
  entry('/surfaces/canvas', 'surfaces-canvas-mirrored', 'Canvas — Three mirrored surfaces + shared chrome', [
    'outline',
    'strategy',
    'world kb',
  ], ['@web-canvas/node-chrome-shell']),
  entry('/surfaces/canvas', 'surfaces-mental-surfacing', 'Mental Surfacing', ['beliefs', 'observers'], [
    '@/fixtures/mental-surfacing-fixtures',
  ]),
  entry('/surfaces/canvas', 'surfaces-nle-timeline', 'NLE Timeline', ['nle', 'track'], [
    '@/fixtures/nle-timeline-canvas-fixtures',
  ]),
  entry('/surfaces/canvas', 'surfaces-world-timeline', 'World Timeline', ['world', 'events'], [
    '@/fixtures/timeline-canvas-fixtures',
  ]),
  entry('/surfaces/canvas', 'surfaces-work-timeline', 'Work Timeline', ['work', 'beats'], [
    '@/fixtures/work-timeline-canvas-fixtures',
  ]),
  entry('/surfaces/canvas', 'surfaces-global-timeline', 'Global Timeline', ['global', 'scope'], [
    '@/fixtures/global-timeline-fixtures',
  ]),
  entry('/surfaces/canvas', 'surfaces-layer-breadcrumb', 'Layer Breadcrumb', ['breadcrumb', 'layer'], [
    '@/fixtures/layer-breadcrumb-fixtures',
  ]),
  entry('/surfaces/canvas', 'surfaces-conflict-modals', 'Conflict Modals', ['modal', 'conflict'], [
    '@/fixtures/conflict-modal-fixtures',
  ]),
];

const SURFACES_DAEMON_ENTRIES: readonly GalleryEntry[] = [
  entry('/surfaces/daemon', 'surfaces-daemon', 'Daemon status strip', ['healthy', 'badge'], [UI, '@/pages/surfaces']),
];

const SURFACES_LAUNCH_ENTRIES: readonly GalleryEntry[] = [
  entry('/surfaces/launch', 'surfaces-launch', 'Launch — Daemon splash', ['waiting', 'error', 'recovery'], [
    '@web-setup/daemon-ready-splash',
  ]),
];

const SURFACES_SELECTION_ENTRIES: readonly GalleryEntry[] = [
  entry('/surfaces/selection-submenu', 'selection-submenu-world-light', 'World row + submenu open', ['world', 'menu'], [
    '@web-shell/selection-submenu',
  ]),
  entry('/surfaces/selection-submenu', 'selection-submenu-world-dark', 'World row + submenu open (document theme)', ['world', 'document theme'], [
    '@web-shell/selection-submenu',
  ]),
  entry('/surfaces/selection-submenu', 'selection-submenu-work-light', 'Work row + submenu open', ['work', 'menu'], [
    '@web-shell/selection-submenu',
  ]),
  entry('/surfaces/selection-submenu', 'selection-submenu-work-dark', 'Work row + submenu open (document theme)', ['work', 'document theme'], [
    '@web-shell/selection-submenu',
  ]),
  entry('/surfaces/selection-submenu', 'selection-submenu-rename-frame', 'Rename in progress', ['rename', 'inline'], [
    '@web-shell/selection-submenu',
  ]),
  entry('/surfaces/selection-submenu', 'selection-submenu-agent-dialog-frame', 'Agent dialog overlay', ['agent', 'dialog'], [
    '@web-shell/selection-submenu',
  ]),
];

const CATALOG_BY_PATH: Readonly<Record<string, readonly GalleryEntry[]>> = {
  '/tokens': TOKEN_ENTRIES,
  '/brand': BRAND_ENTRIES,
  '/components': COMPONENT_ENTRIES,
  '/voice': VOICE_ENTRIES,
  '/surfaces': SURFACES_OVERVIEW_ENTRIES,
  '/surfaces/setup': SURFACES_SETUP_ENTRIES,
  '/surfaces/shell': SURFACES_SHELL_ENTRIES,
  '/surfaces/agent-picker': SURFACES_AGENT_PICKER_ENTRIES,
  '/surfaces/canvas': SURFACES_CANVAS_ENTRIES,
  '/surfaces/daemon': SURFACES_DAEMON_ENTRIES,
  '/surfaces/launch': SURFACES_LAUNCH_ENTRIES,
  '/surfaces/selection-submenu': SURFACES_SELECTION_ENTRIES,
};

/** Gallery routes that expose the section index and comparison controls. */
export const GALLERY_ROUTE_PREFIXES = [
  '/tokens',
  '/brand',
  '/components',
  '/voice',
  '/surfaces',
] as const;

export function isGalleryPath(pathname: string): boolean {
  return GALLERY_ROUTE_PREFIXES.some(
    (prefix) => pathname === prefix || pathname.startsWith(`${prefix}/`),
  );
}

export function getGalleryEntries(pathname: string): readonly GalleryEntry[] {
  return CATALOG_BY_PATH[pathname] ?? [];
}

/** Frozen gallery labels keyed by route (spec §6) — used for pair-frame titles. */
const GALLERY_LABEL_BY_PATH: Readonly<Record<string, string>> = {
  '/tokens': 'Tokens',
  '/brand': 'Brand',
  '/components': 'Components',
  '/voice': 'Voice',
  '/surfaces': 'Surfaces',
  '/surfaces/setup': 'Setup',
  '/surfaces/shell': 'Shell',
  '/surfaces/agent-picker': 'AgentPicker',
  '/surfaces/canvas': 'Canvas',
  '/surfaces/daemon': 'Daemon',
  '/surfaces/launch': 'Launch',
  '/surfaces/selection-submenu': 'Selection Submenu',
};

export function getGalleryLabel(pathname: string): string {
  return GALLERY_LABEL_BY_PATH[pathname] ?? 'Gallery';
}

export function filterGalleryEntries(
  entries: readonly GalleryEntry[],
  query: string,
): readonly GalleryEntry[] {
  const needle = query.trim().toLowerCase();
  if (!needle) return entries;
  return entries.filter((item) => {
    if (item.label.toLowerCase().includes(needle)) return true;
    if (item.id.toLowerCase().includes(needle)) return true;
    return item.keywords.some((keyword) => keyword.toLowerCase().includes(needle));
  });
}

/** Focus the catalog heading below sticky chrome after route/hash navigation. */
function resolveGalleryFocusTarget(element: HTMLElement): HTMLElement {
  const tag = element.tagName.toLowerCase();
  if (/^h[1-6]$/.test(tag)) return element;

  if (tag === 'section' || tag === 'article') {
    const namedHeading = element.querySelector<HTMLElement>(
      'h1[id], h2[id], h3[id], h4[id], h5[id], h6[id]',
    );
    if (namedHeading) return namedHeading;

    const firstHeading = element.querySelector<HTMLElement>('h1, h2, h3, h4, h5, h6');
    if (firstHeading) return firstHeading;
  }

  const derivedHeading = document.getElementById(`${element.id}-heading`);
  if (derivedHeading instanceof HTMLElement) return derivedHeading;

  return element;
}

/**
 * Focus the catalog heading once the matched route has actually mounted.
 * Retries each animation frame until the target exists or the bounded
 * deadline passes — a nested lazy leaf still rendering its loading boundary
 * receives focus when it commits. Returns a cancel function; callers cancel
 * on unmount or when a newer navigation supersedes this attempt.
 */
const FOCUS_RETRY_DEADLINE_MS = 2000;

export function focusGalleryHeading(id: string): () => void {
  if (!id) return () => {};

  let cancelled = false;
  let frameHandle = 0;
  const startedAt = performance.now();

  const attempt = () => {
    if (cancelled) return;
    const target = document.getElementById(id);
    if (target) {
      const focusTarget = resolveGalleryFocusTarget(target);
      focusTarget.tabIndex = -1;
      focusTarget.focus({ preventScroll: false });
      return;
    }
    if (performance.now() - startedAt >= FOCUS_RETRY_DEADLINE_MS) return;
    frameHandle = window.requestAnimationFrame(attempt);
  };

  frameHandle = window.requestAnimationFrame(attempt);

  return () => {
    cancelled = true;
    window.cancelAnimationFrame(frameHandle);
  };
}
