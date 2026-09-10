/**
 * @nexus/design-tokens — DESIGN pair compiler (single shared projection).
 *
 * Sole build-time authority that turns the repo-root DESIGN pair
 * (DESIGN.md / DESIGN.dark.md) into the three checked-in derived outputs:
 *   - tooling/design-tokens/src/tokens.css
 *   - packages/nexus-ui/theme.css
 *   - packages/nexus-ui/src/generated-brand.ts
 *
 * The same compiler is also consumed by the Studio Vite plugin to transform
 * the two shared CSS modules in memory during `vite dev`, so build outputs
 * and dev CSS always share one projection and never bundle raw YAML.
 *
 * Internal tooling API (not a package public surface):
 *   loadDesignPair(repoRoot) -> Promise<DesignPair>
 *   projectDesign(pair)      -> TokenProjection
 *
 *   type DesignPair      = { light: Record<string, unknown>; dark: Record<string, unknown> }
 *   type TokenProjection = { css: string; brandCss: string; brandTokens: string }
 *
 * Projection semantics follow .mstar/specs/design-studio.md §3.5:
 *   - Reject duplicate YAML keys, light/dark leaf-path mismatches, unresolved
 *     {path} references, and reference cycles.
 *   - Resolve references recursively, including refs embedded in color-mix
 *     and border strings, and whole-role typography references.
 *   - Compound SOUL values "{typography.X} @ {colors.Y}" project their color
 *     member to their existing --color-* property.
 *   - Preserve every existing CSS variable and Tailwind utility name.
 */
import { readFile } from 'node:fs/promises';
import { join, dirname, resolve } from 'node:path';
import { parseDocument } from 'yaml';

/** @typedef {Record<string, unknown>} ThemeDoc */

/**
 * Walk a parsed theme document and collect the full dot-paths of every
 * scalar leaf together with its parent path (leaf path set for parity).
 * @param {ThemeDoc} doc
 * @returns {Set<string>}
 */
export function collectLeafPaths(doc, prefix = '', out = new Set()) {
  for (const [key, value] of Object.entries(doc)) {
    const path = prefix ? `${prefix}.${key}` : key;
    if (value !== null && typeof value === 'object' && !Array.isArray(value)) {
      collectLeafPaths(value, path, out);
    } else {
      out.add(path);
    }
  }
  return out;
}

/**
 * Build a flat map of every path → its raw value (leaf or object) so
 * references to intermediate nodes (e.g. a whole typography role) resolve.
 * @param {ThemeDoc} doc
 * @param {string} [prefix]
 * @param {Map<string, unknown>} [out]
 */
export function indexPaths(doc, prefix = '', out = new Map()) {
  for (const [key, value] of Object.entries(doc)) {
    const path = prefix ? `${prefix}.${key}` : key;
    out.set(path, value);
    if (value !== null && typeof value === 'object' && !Array.isArray(value)) {
      indexPaths(value, path, out);
    }
  }
  return out;
}

/** Detect duplicate YAML keys in the first document block. */
function assertNoDuplicateKeys(source, doc) {
  const errors = doc.errors
    .filter((e) => /duplicate|not unique|Map keys must be unique/i.test(e.message))
    .map((e) => e.message);
  if (errors.length > 0) {
    throw new Error(`[${source}] duplicate YAML key(s): ${errors.join('; ')}`);
  }
  const warnings = doc.warnings;
  if (warnings.length > 0) {
    const dup = warnings
      .filter((w) => /duplicate|not unique|Map keys must be unique/i.test(w.message))
      .map((w) => w.message);
    if (dup.length > 0) {
      throw new Error(`[${source}] duplicate YAML key(s): ${dup.join('; ')}`);
    }
  }
}

/**
 * Parse a DESIGN file's YAML frontmatter (between the leading --- delimiters)
 * as a plain object, rejecting duplicate keys and any other YAML errors.
 * @param {string} file - absolute path
 * @param {string} sourceLabel
 * @returns {ThemeDoc}
 */
export async function parseFrontmatter(file, sourceLabel) {
  const text = await readFile(file, 'utf8');
  const match = /^---\r?\n([\s\S]*?)\r?\n---\r?\n/.exec(text);
  if (!match) {
    throw new Error(`[${sourceLabel}] no YAML frontmatter delimited by "---" found in ${file}`);
  }
  const doc = parseDocument(match[1], { uniqueKeys: true, strict: true, maxAliasCount: 100 });
  assertNoDuplicateKeys(sourceLabel, doc);
  const other = doc.errors.filter((e) => !/duplicate|not unique|Map keys must be unique/i.test(e.message));
  if (other.length > 0) {
    throw new Error(`[${sourceLabel}] YAML parse error(s) in ${file}: ${other.map((e) => e.message).join('; ')}`);
  }
  const value = doc.toJS();
  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`[${sourceLabel}] frontmatter in ${file} must be a YAML map`);
  }
  return value;
}

/**
 * Read both DESIGN files and parse their frontmatter, enforcing light/dark
 * leaf-path parity.
 * @param {string} repoRoot - repository root (where DESIGN.md lives)
 * @returns {Promise<{ light: ThemeDoc; dark: ThemeDoc }>}
 */
export async function loadDesignPair(repoRoot) {
  const light = await parseFrontmatter(join(repoRoot, 'DESIGN.md'), 'DESIGN.md');
  const dark = await parseFrontmatter(join(repoRoot, 'DESIGN.dark.md'), 'DESIGN.dark.md');
  const lightLeaves = collectLeafPaths(light);
  const darkLeaves = collectLeafPaths(dark);
  const onlyLight = [...lightLeaves].filter((p) => !darkLeaves.has(p));
  const onlyDark = [...darkLeaves].filter((p) => !lightLeaves.has(p));
  if (onlyLight.length || onlyDark.length) {
    throw new Error(
      `leaf-path parity mismatch: light-only=[${onlyLight.join(', ')}] dark-only=[${onlyDark.join(', ')}]`,
    );
  }
  return { light, dark };
}

/**
 * Resolve a raw value into a CSS scalar string, expanding {ref} references
 * (including refs embedded in color-mix / border strings) and compound
 * "{typography.X} @ {colors.Y}" values (returns the color member).
 *
 * @param {unknown} value
 * @param {ThemeDoc} doc
 * @param {Map<string, unknown>} index
 * @param {string} sourcePath - for diagnostics
 * @param {Set<string>} [seen]
 * @param {Array<string>} [trace]
 * @returns {string}
 */
export function resolveScalar(value, doc, index, sourcePath, seen = new Set(), trace = []) {
  if (value === null || value === undefined) {
    throw new Error(
      `[projectDesign] empty value at ${sourcePath} — a mapped token must resolve to a non-empty scalar (fail closed, no last-good).`,
    );
  }
  if (typeof value === 'number' || typeof value === 'boolean') return String(value);

  if (typeof value === 'string') {
    if (value === '') {
      throw new Error(
        `[projectDesign] empty scalar at ${sourcePath} — a mapped token must resolve to a non-empty value (fail closed, no silent omission).`,
      );
    }
    // Whole-value reference to a recipe/role (e.g. "{typography.label-14}") —
    // resolve the target; callers that need a sub-member pass the member path.
    if (/^\{[^}]+\}$/.test(value)) {
      const ref = value.slice(1, -1).trim();
      return resolveRef(ref, doc, index, sourcePath, seen, trace);
    }
    // Compound SOUL recipe "{typography.X} @ {colors.Y}" -> color member.
    // Fresh (non-shared) regex each call so `lastIndex` never leaks across
    // invocations of this resolver; iterate matchAll (resets lastIndex) for
    // detection so a prior .test() cannot poison it.
    const parts = [...value.matchAll(/@\s*\{([^}]+)\}/g)];
    if (parts.length > 0) {
      const colorRef = parts.at(-1)?.[1];
      if (colorRef) {
        return resolveRef(colorRef, doc, index, `${sourcePath} (compound color)`, seen, trace);
      }
    }
    // Embedded references (color-mix, borders, etc) — replace every {ref}.
    const matches = [...value.matchAll(/\{([^}]+)\}/g)];
    if (matches.length > 0) {
      let result = value;
      for (const m of matches) {
        const resolved = resolveRef(m[1], doc, index, `${sourcePath} (embedded)`, seen, trace);
        result = result.replace(m[0], resolved);
      }
      return result;
    }
    return value;
  }
  // Unexpected object leaf that is not a compound/string ref: reject rather
  // than serializing "[object Object]".
  throw new Error(
    `[projectDesign] non-scalar value at ${sourcePath}: ${JSON.stringify(value).slice(0, 120)} — ` +
      `expected a scalar string/number or a "{ref}" / compound recipe.`,
  );
}

/** Resolve a bare "{a.b.c}" reference path to its scalar CSS text. */
function resolveRef(refPath, doc, index, sourcePath, seen, trace) {
  if (trace.includes(refPath)) {
    throw new Error(
      `[projectDesign] cyclic reference at ${sourcePath}: ${[...trace, refPath].join(' -> ')}`,
    );
  }
  if (!index.has(refPath)) {
    throw new Error(`[projectDesign] unresolved reference at ${sourcePath}: {${refPath}} (missing from source)`);
  }
  const target = index.get(refPath);
  return resolveScalar(
    target,
    doc,
    index,
    refPath.startsWith(sourcePath) ? sourcePath : refPath,
    seen,
    [...trace, refPath],
  );
}

/**
 * Projection registry: a single declarative source-path mapping carrying
 * DESIGN paths only (never a copied palette). Each entry:
 *   - cssVar : the exact existing CSS custom-property name to emit
 *   - source : dot-path into the parsed theme document
 *   - get    : extraction rule:
 *                'scalar'        — value is a scalar (refs/compounds resolved)
 *                {member}        — value is a recipe; use that member field
 *                'fontSize'       — whole-role ref; use .fontSize
 *                'color-of'       — compound "{typography.X} @ {colors.Y}" -> color member
 * Returns the CSS value via resolveScalar.
 */
function buildProjection(doc) {
  const index = indexPaths(doc);

  /** @param {string} cssVar @param {string} source @param {object} [rule] */
  const scalar = (cssVar, source, rule = {}) => ({ cssVar, source, ...rule });

  const projections = [];

  // ── Typography: family tokens ──
  projections.push(scalar('--font-sans', 'typography.heading-16.fontFamily'));
  projections.push(scalar('--font-mono', 'typography.copy-13-mono.fontFamily'));
  projections.push(scalar('--font-display', 'typography.font-display'));

  // ── Typography roles → --text-* + metric tuples ──
  const TYPO_ROLES = [
    'display-32', 'display-24', 'display-20',
    'heading-32', 'heading-24', 'heading-20', 'heading-16',
    'label-14', 'label-12',
    'copy-16', 'copy-14', 'copy-13', 'copy-12',
    'button-14', 'button-12',
    'label-12-mono', 'copy-13-mono',
  ];
  for (const role of TYPO_ROLES) {
    const base = `typography.${role}`;
    projections.push(
      scalar(`--text-${role}`, `${base}.fontSize`),
      scalar(`--text-${role}--line-height`, `${base}.lineHeight`),
      scalar(`--text-${role}--letter-spacing`, `${base}.letterSpacing`),
      scalar(`--text-${role}--font-weight`, `${base}.fontWeight`),
      scalar(`--text-${role}--font-family`, `${base}.fontFamily`),
    );
  }

  // ── Reading prose ──
  projections.push(scalar('--reading-prose-measure', 'typography.reading-prose-measure'));
  projections.push(scalar('--reading-prose-line-height', 'typography.reading-prose-line-height'));
  projections.push(scalar('--reading-prose-paragraph-spacing', 'typography.reading-prose-paragraph-spacing'));

  // ── Spacing / radius / motion ──
  const SPACING_STEPS = ['space-1', 'space-2', 'space-3', 'space-4', 'space-6', 'space-8', 'space-10', 'space-16', 'space-24'];
  for (const s of SPACING_STEPS) projections.push(scalar(`--${s}`, `spacing.${s}`));

  const RADII = ['control', 'card', 'popover', 'fullscreen', 'pill'];
  for (const r of RADII) projections.push(scalar(`--radius-${r}`, `rounded.${r}`));

  const MOTIONS = ['duration-instant', 'duration-state', 'duration-popover', 'duration-modal', 'duration-enter', 'duration-exit'];
  for (const m of MOTIONS) projections.push(scalar(`--${m}`, `motion.${m}`));
  projections.push(scalar('--ease-standard', 'motion.ease-standard'));
  projections.push(scalar('--ease-emphasized', 'motion.ease-emphasized'));

  // ── Elevation scale + legacy aliases ──
  for (let i = 0; i <= 4; i++) projections.push(scalar(`--shadow-elevation-${i}`, `elevation.elevation-${i}`));
  for (const alias of ['card', 'popover', 'modal']) {
    projections.push(scalar(`--shadow-${alias}`, `elevation.shadow-${alias}`));
  }

  // ── Colors → --color-<name> ──
  const colors = doc.colors ?? {};
  for (const name of Object.keys(colors)) {
    projections.push(scalar(`--color-${name}`, `colors.${name}`));
  }

  // ── Brand-specific numeric snapshot + theme.css handled separately ──

  // ── Canvas surface ──
  const canvasFlat = [
    'canvas-surface', 'canvas-grid', 'canvas-grid-gap', 'canvas-grid-dot-size',
    'canvas-node-fill', 'canvas-node-fill-hover', 'canvas-node-border', 'canvas-node-border-selected',
    'canvas-edge', 'canvas-edge-hover', 'canvas-port', 'canvas-minimap',
    'canvas-strategy-accent', 'canvas-outline-accent', 'canvas-worldkb-accent', 'canvas-timeline-accent',
    'canvas-layer-brief-accent', 'canvas-layer-narrative-accent', 'canvas-layer-moment-accent',
    'canvas-write-dirty', 'canvas-write-conflict', 'canvas-write-success', 'canvas-write-stale-bg',
    'canvas-outline-volume-fill',
    'canvas-outline-chapter-card-status-pending', 'canvas-outline-chapter-card-status-drafted', 'canvas-outline-chapter-card-status-completed',
    'canvas-outline-timeline-event-pin', 'canvas-outline-foreshadow-edge', 'canvas-outline-timeline-marker', 'canvas-outline-conflict-marker',
    'canvas-outline-scene-fill', 'canvas-outline-scene-border',
    'canvas-outline-scene-status-drafted', 'canvas-outline-scene-status-completed',
    'canvas-outline-beat-fill', 'canvas-outline-beat-border',
    'canvas-worldkb-entity-card-fill-default', 'canvas-worldkb-entity-card-fill-hover', 'canvas-worldkb-entity-card-fill-selected',
    'canvas-worldkb-entity-card-stroke-default', 'canvas-worldkb-entity-card-stroke-selected',
    'canvas-worldkb-promotion-pending', 'canvas-worldkb-promotion-confirmed', 'canvas-worldkb-promotion-rejected', 'canvas-worldkb-promotion-merged',
    'canvas-worldkb-source-anchor-edge', 'canvas-worldkb-source-anchor-node',
    'canvas-worldkb-computable-badge', 'canvas-worldkb-conflict-marker', 'canvas-worldkb-conflict-marker-fill',
    'canvas-worldkb-nonspatial-row-highlight', 'canvas-worldkb-focus-ring',
    'canvas-worldkb-relationship-edge', 'canvas-worldkb-relationship-edge-default', 'canvas-worldkb-relationship-edge-symmetric',
    'canvas-worldkb-relationship-edge-custom', 'canvas-worldkb-relationship-confidence-low', 'canvas-worldkb-relationship-confidence-mid',
    'canvas-worldkb-relationship-confidence-high', 'canvas-worldkb-relationship-grounded-badge',
    'canvas-worldkb-relationship-asserted-badge', 'canvas-worldkb-relationship-inspector-fill',
  ];
  for (const k of canvasFlat) {
    projections.push(scalar(`--color-${k}`, `components.canvas.${k}`));
  }
  // canvas.node-width -> structural --canvas-node-width-*
  const canvasWidths = ['strategy-root', 'strategy-primary', 'strategy-secondary', 'outline-scene-beat', 'default'];
  for (const w of canvasWidths) {
    projections.push(scalar(`--canvas-node-width-${w}`, `components.canvas.node-width.${w}`));
  }

  // ── Dialog / sheet / sidebar / listbox structural ──
  projections.push(scalar('--color-dialog-max-width', 'components.dialog.maxWidth'));
  projections.push(scalar('--dialog-width', 'components.dialog.width'));
  projections.push(scalar('--dialog-max-height', 'components.dialog.maxHeight'));
  projections.push(scalar('--sheet-width', 'components.sheet.width'));
  projections.push(scalar('--sidebar-nav-width', 'components.sidebar-nav.width'));
  projections.push(scalar('--sidebar-nav-item-height', 'components.sidebar-nav.itemHeight'));
  projections.push(scalar('--color-listbox-max-height', 'components.listbox.maxHeight'));
  projections.push(scalar('--color-states-disabled-opacity', 'components.states.disabled.opacity'));

  // ── Status surface family ──
  for (const role of ['error', 'success', 'warning', 'info']) {
    projections.push(scalar(`--color-${role}-surface`, `components.states.${role}.backgroundColor`));
    projections.push(scalar(`--color-${role}-surface-border`, `components.states.${role}.borderColor`));
  }
  projections.push(scalar('--color-data-table-row-protected', 'components.data-table.row-protected'));
  projections.push(scalar('--color-main-banner-background', 'components.launch-daemon.main-banner.backgroundColor'));

  // ── Finding status pill: state(bg|text|border), underscore -> hyphen ──
  const FINDING_STATES = ['open', 'triaged', 'in_review', 'resolved', 'wont_fix', 'duplicate'];
  for (const state of FINDING_STATES) {
    const key = state.replace(/_/g, '-');
    projections.push(scalar(`--color-finding-status-${key}-bg`, `components.finding-status-pill.${state}.backgroundColor`));
    projections.push(scalar(`--color-finding-status-${key}-text`, `components.finding-status-pill.${state}.textColor`));
    projections.push(scalar(`--color-finding-status-${key}-border`, `components.finding-status-pill.${state}.borderColor`));
  }

  // ── Memory task-kind chips ──
  const TASK_KINDS = ['brainstorm', 'outline', 'chapter', 'research', 'unknown'];
  for (const kind of TASK_KINDS) {
    const c = `components.memory-task-kind-${kind}`;
    projections.push(scalar(`--color-memory-task-kind-${kind}-bg`, `${c}.backgroundColor`));
    projections.push(scalar(`--color-memory-task-kind-${kind}-text`, `${c}.textColor`));
    projections.push(scalar(`--color-memory-task-kind-${kind}-border`, `${c}.borderColor`));
  }

  // ── Reading maturation count badges ──
  for (const [src, out] of [
    ['world-kb-density-count', 'kb-density'],
    ['open-findings-count', 'open-findings'],
  ]) {
    const c = `components.reading-maturation-badge.${src}`;
    projections.push(scalar(`--color-reading-maturation-${out}-bg`, `${c}.backgroundColor`));
    projections.push(scalar(`--color-reading-maturation-${out}-text`, `${c}.textColor`));
    projections.push(scalar(`--color-reading-maturation-${out}-border`, `${c}.borderColor`));
  }

  // ── nexus-ui Badge soft variants ──
  const SOFT_VARIANTS = ['running', 'queued', 'warning', 'error', 'preset'];
  for (const v of SOFT_VARIANTS) {
    const c = `components.badge-status-pill.soft.${v}`;
    projections.push(scalar(`--color-nexus-ui-badge-soft-${v}-bg`, `${c}.backgroundColor`));
    projections.push(scalar(`--color-nexus-ui-badge-soft-${v}-text`, `${c}.textColor`));
    projections.push(scalar(`--color-nexus-ui-badge-soft-${v}-border`, `${c}.borderColor`));
  }

  // ── SOUL viz + narrative ──
  projections.push(scalar('--color-soul-viz-keyword-cluster-node-fill', 'components.soul-viz-keyword-cluster-node.fill'));
  projections.push(scalar('--color-soul-viz-keyword-cluster-node-stroke', 'components.soul-viz-keyword-cluster-node.stroke'));
  projections.push(scalar('--color-soul-viz-keyword-cluster-node-label', 'components.soul-viz-keyword-cluster-node.label'));
  projections.push(scalar('--color-soul-viz-timeline-axis-line', 'components.soul-viz-timeline-axis.line'));
  projections.push(scalar('--color-soul-viz-timeline-axis-tick', 'components.soul-viz-timeline-axis.tick'));
  projections.push(scalar('--color-soul-viz-timeline-axis-label', 'components.soul-viz-timeline-axis.label', { compound: true }));
  projections.push(scalar('--color-soul-viz-drift-band-fill', 'components.soul-viz-drift-band.fill'));
  projections.push(scalar('--color-soul-viz-drift-band-fill-2', 'components.soul-viz-drift-band.fill-2'));
  projections.push(scalar('--color-soul-viz-drift-band-fill-3', 'components.soul-viz-drift-band.fill-3'));
  projections.push(scalar('--color-soul-viz-drift-band-fill-4', 'components.soul-viz-drift-band.fill-4'));
  projections.push(scalar('--color-soul-viz-drift-band-fill-5', 'components.soul-viz-drift-band.fill-5'));
  projections.push(scalar('--color-soul-viz-drift-band-fill-6', 'components.soul-viz-drift-band.fill-6'));
  projections.push(scalar('--color-soul-viz-drift-band-step-stroke', 'components.soul-viz-drift-band.step-stroke'));
  projections.push(scalar('--color-soul-viz-drift-band-label', 'components.soul-viz-drift-band.label', { compound: true }));
  projections.push(scalar('--color-soul-narrative-prose', 'components.soul-narrative-prose', { compound: true }));
  projections.push(scalar('--color-soul-growth-curve-stroke', 'components.soul-growth-curve-stroke'));

  // ── Reading annotation highlights ──
  for (const [color, suffix] of [
    ['yellow', 'yellow'], ['blue', 'blue'], ['green', 'green'], ['pink', 'pink'],
  ]) {
    const c = `components.reading-annotation-highlight-${color}`;
    projections.push(scalar(`--color-reading-annotation-highlight-${suffix}-background`, `${c}.backgroundColor`));
    projections.push(scalar(`--color-reading-annotation-highlight-${suffix}-text`, `${c}.textColor`));
  }
  projections.push(scalar('--color-reading-annotation-inspector-background', 'components.reading-annotation-inspector.backgroundColor'));
  projections.push(scalar('--color-reading-annotation-inspector-border', 'components.reading-annotation-inspector.borderColor'));
  projections.push(scalar('--color-reading-annotation-inspector-text', 'components.reading-annotation-inspector.textColor'));
  projections.push(scalar('--color-reading-selection-toolbar-background', 'components.reading-selection-toolbar.backgroundColor'));
  projections.push(scalar('--color-reading-selection-toolbar-border', 'components.reading-selection-toolbar.borderColor'));
  projections.push(scalar('--color-reading-selection-toolbar-text', 'components.reading-selection-toolbar.textColor'));
  projections.push(scalar('--color-reading-selection-toolbar-shadow', 'components.reading-selection-toolbar.shadow'));

  // ── Reading chrome (flatten component + nested member) ──
  // Explicit map: cssVar leaf -> DESIGN member
  const readingChrome = [
    ['novel', 'chapter-title', 'font-family', 'fontFamily'],
    ['novel', 'chapter-title', 'font-size', 'fontSize'],
    ['novel', 'chapter-title', 'font-weight', 'fontWeight'],
    ['novel', 'chapter-title', 'line-height', 'lineHeight'],
    ['novel', 'chapter-title', 'letter-spacing', 'letterSpacing'],
    ['novel', 'chapter-title', 'color', 'color'],
    ['novel', 'scene-separator', 'color', 'color'],
    ['novel', 'scene-separator', 'font-size', 'fontSize'],
    ['novel', 'scene-separator', 'text-align', 'textAlign'],
    ['novel', 'scene-separator', 'padding-block', 'paddingBlock'],
    ['novel', 'epigraph', 'font-style', 'fontStyle'],
    ['novel', 'epigraph', 'text-align', 'textAlign'],
    ['novel', 'epigraph', 'color', 'color'],
    ['novel', 'epigraph', 'padding-left', 'paddingLeft'],
    ['essay', 'section-heading', 'font-family', 'fontFamily'],
    ['essay', 'section-heading', 'font-size', 'fontSize'],
    ['essay', 'section-heading', 'font-weight', 'fontWeight'],
    ['essay', 'section-heading', 'line-height', 'lineHeight'],
    ['essay', 'section-heading', 'letter-spacing', 'letterSpacing'],
    ['essay', 'section-heading', 'color', 'color'],
    ['essay', 'section-heading', 'margin-top', 'marginTop'],
    ['essay', 'blockquote', 'border-left', 'borderLeft'],
    ['essay', 'blockquote', 'padding-left', 'paddingLeft'],
    ['essay', 'blockquote', 'font-style', 'fontStyle'],
    ['essay', 'blockquote', 'color', 'color'],
    ['essay', 'footnote-marker', 'vertical-align', 'verticalAlign'],
    ['essay', 'footnote-marker', 'font-size', 'fontSize'],
    ['essay', 'footnote-marker', 'color', 'color'],
    ['game-bible', 'term-link', 'color', 'color'],
    ['game-bible', 'term-link', 'text-decoration', 'textDecoration'],
    ['game-bible', 'term-link', 'cursor', 'cursor'],
    ['game-bible', 'definition-callout', 'background-color', 'backgroundColor'],
    ['game-bible', 'definition-callout', 'border-left', 'borderLeft'],
    ['game-bible', 'definition-callout', 'padding', 'padding'],
    ['game-bible', 'definition-callout', 'label-color', 'labelColor'],
    ['game-bible', 'definition-callout', 'label-font-weight', 'labelFontWeight'],
    ['game-bible', 'category-badge', 'background-color', 'backgroundColor'],
    ['game-bible', 'category-badge', 'color', 'textColor'],
    ['script', 'character-name', 'text-align', 'textAlign'],
    ['script', 'character-name', 'text-transform', 'textTransform'],
    ['script', 'character-name', 'font-weight', 'fontWeight'],
    ['script', 'character-name', 'font-size', 'fontSize'],
    ['script', 'character-name', 'letter-spacing', 'letterSpacing'],
    ['script', 'character-name', 'margin-top', 'marginTop'],
    ['script', 'character-name', 'color', 'color'],
    ['script', 'parenthetical', 'font-style', 'fontStyle'],
    ['script', 'parenthetical', 'color', 'color'],
    ['script', 'parenthetical', 'padding-left', 'paddingLeft'],
    ['script', 'scene-heading', 'font-weight', 'fontWeight'],
    ['script', 'scene-heading', 'text-transform', 'textTransform'],
    ['script', 'scene-heading', 'font-size', 'fontSize'],
    ['script', 'scene-heading', 'letter-spacing', 'letterSpacing'],
    ['script', 'scene-heading', 'color', 'color'],
    ['script', 'scene-heading', 'margin-top', 'marginTop'],
  ];
  for (const [area, group, cssLeaf, member] of readingChrome) {
    const source = `components.reading-chrome-${area}.${group}.${member}`;
    const cssVar = `--reading-chrome-${area}-${group}-${cssLeaf}`;
    projections.push(scalar(cssVar, source));
  }

  // ── Footer profile ──
  const FP = {
    'avatar-size': 'avatar-size', 'avatar-rounded': 'avatar-rounded',
    'avatar-bg': 'avatar-bg', 'avatar-bg-hover': 'avatar-bg-hover', 'avatar-bg-active': 'avatar-bg-active',
    'avatar-text': 'avatar-text', 'avatar-text-active': 'avatar-text-active',
    'avatar-fallback-bg': 'avatar-fallback-bg', 'avatar-fallback-text': 'avatar-fallback-text',
    'add-button-bg': 'add-button-bg', 'add-button-border': 'add-button-border',
    'add-button-text': 'add-button-text', 'add-button-hover-bg': 'add-button-hover-bg',
    'add-button-hover-border': 'add-button-hover-border', 'add-button-hover-text': 'add-button-hover-text',
    gap: 'gap',
  };
  for (const [css, src] of Object.entries(FP)) {
    projections.push(scalar(`--color-footer-profile-${css}`, `components.footer-profile.${src}`));
  }

  // ── Setup wizard step ──
  const SW_STEP = {
    'step-circle-size': 'step-circle-size',
    'step-circle-active-bg': 'step-circle-active-bg', 'step-circle-active-text': 'step-circle-active-text',
    'step-circle-complete-bg': 'step-circle-complete-bg', 'step-circle-complete-text': 'step-circle-complete-text',
    'step-circle-pending-bg': 'step-circle-pending-bg', 'step-circle-pending-text': 'step-circle-pending-text',
    'step-connector': 'step-connector',
    'step-label-typography': 'step-label-typography',
    'step-label-active-color': 'step-label-active-color', 'step-label-pending-color': 'step-label-pending-color',
    'wizard-max-width': 'wizard-max-width', 'wizard-max-height': 'wizard-max-height',
    'wizard-padding': 'wizard-padding', 'step-row-height': 'step-row-height',
  };
  for (const [css, src] of Object.entries(SW_STEP)) {
    projections.push({
      cssVar: `--color-setup-wizard-${css}`,
      source: `components.setup-wizard-step.${src}`,
      ...(css === 'step-label-typography' ? { fontSizeOf: true } : {}),
    });
  }

  // ── Setup wizard surface ──
  const SW_SURFACE = new Map([
    ['card-bg', 'card-bg'], ['card-border', 'card-border'],
    ['step-panel-right-divider', 'step-panel-right-divider'], ['step-panel-width', 'step-panel-width'],
    ['step-panel-padding-x', 'step-panel-padding-x'], ['step-panel-padding-y', 'step-panel-padding-y'],
    ['content-panel-padding-x', 'content-panel-padding-x'], ['content-panel-padding-y', 'content-panel-padding-y'],
    ['input-row-bg', 'input-row-bg'], ['input-row-border', 'input-row-border'],
    ['input-row-min-height', 'input-row-min-height'], ['input-row-gap', 'input-row-gap'],
    ['input-row-padding-x', 'input-row-padding-x'], ['input-row-padding-y', 'input-row-padding-y'],
    ['input-row-label-color', 'input-row-label-color'], ['input-row-path-color', 'input-row-path-color'],
    ['input-row-icon-color', 'input-row-icon-color'], ['input-row-rounded', 'input-row-rounded'],
    ['cta-primary-max-width', 'cta-primary-max-width'], ['cta-container-gap', 'cta-container-gap'],
  ]);
  for (const [css, src] of SW_SURFACE) {
    projections.push(scalar(`--color-setup-wizard-surface-${css}`, `components.setup-wizard-surface.${src}`));
  }

  return { projections, index };
}

/**
 * Resolve a single projection entry to its final CSS value string.
 */
function projectValue(doc, entry, index = indexPaths(doc)) {
  const { source } = entry;
  if (!index.has(source)) {
    throw new Error(`[projectDesign] missing source path for ${entry.cssVar}: ${source}`);
  }
  const raw = index.get(source);

  // fontSizeOf: whole-role ref -> .fontSize of the referenced role.
  if (entry.fontSizeOf) {
    if (typeof raw === 'string' && /^\{[^}]+\}$/.test(raw)) {
      const ref = raw.slice(1, -1).trim();
      const target = index.get(ref);
      if (target && typeof target === 'object' && typeof target.fontSize !== 'undefined') {
        return resolveScalar(target.fontSize, doc, index, `${ref}.fontSize`);
      }
      throw new Error(
        `[projectDesign] fontSizeOf at ${entry.cssVar}: {${ref}} does not resolve to a typography role with fontSize`,
      );
    }
    throw new Error(`[projectDesign] fontSizeOf at ${entry.cssVar}: expected a "{ref}" value, got ${JSON.stringify(raw).slice(0, 80)}`);
  }

  return resolveScalar(raw, doc, index, source, new Set(), []);
}

/**
 * Emit `:root { --name: value; ... }` block from a theme document.
 * Brand color entries in tokens.css forward to the package theme.css
 * `--nexus-brand-*` layer (public contract: tokens.css imports theme.css).
 */
function emitBlock(doc, entries, index, indent = '  ', brandAlias = false) {
  const lines = [];
  for (const entry of entries) {
    const cssVar = entry.cssVar;
    if (brandAlias && cssVar.startsWith('--color-brand-')) {
      // Forward to theme.css brand variable: --color-brand-<n> -> var(--nexus-brand-<n>)
      const rest = cssVar.slice('--color-brand-'.length);
      lines.push(`${indent}${cssVar}: var(--nexus-brand-${rest});`);
      continue;
    }
    const value = projectValue(doc, entry, index);
    if (value === '') {
      // resolveScalar already rejects empty/null; this is an unreachable
      // defensive guard so a future regression cannot silently drop a var.
      throw new Error(
        `[projectDesign] ${entry.cssVar} resolved to an empty value (${entry.source}) — fail closed, no silent omission.`,
      );
    }
    lines.push(`${indent}${cssVar}: ${value};`);
  }
  return lines.join('\n');
}

/**
 * Project the DESIGN pair into the three derived output strings.
 * @param {{ light: ThemeDoc; dark: ThemeDoc }} pair
 * @returns {{ css: string; brandCss: string; brandTokens: string }}
 */
export function projectDesign(pair) {
  const { light, dark } = pair;
  // leaf parity already enforced by loadDesignPair; re-check defensively.
  const ll = collectLeafPaths(light);
  const dl = collectLeafPaths(dark);
  const onlyL = [...ll].filter((p) => !dl.has(p));
  const onlyD = [...dl].filter((p) => !ll.has(p));
  if (onlyL.length || onlyD.length) {
    throw new Error(
      `[projectDesign] leaf-path parity mismatch: light-only=[${onlyL.join(', ')}] dark-only=[${onlyD.join(', ')}]`,
    );
  }

  const lightRegistry = buildProjection(light);
  const darkRegistry = buildProjection(dark);
  const lightEntries = lightRegistry.projections;
  const darkEntries = darkRegistry.projections;
  const lightIndex = lightRegistry.index;
  const darkIndex = darkRegistry.index;

  // Sanity: both themes must project the identical CSS var surface.
  const lightVars = new Set(lightEntries.map((e) => e.cssVar));
  const darkVars = new Set(darkEntries.map((e) => e.cssVar));
  const onlyLV = [...lightVars].filter((v) => !darkVars.has(v));
  const onlyDV = [...darkVars].filter((v) => !lightVars.has(v));
  if (onlyLV.length || onlyDV.length) {
    throw new Error(
      `[projectDesign] projected CSS var surfaces differ: light-only=[${onlyLV.join(', ')}] dark-only=[${onlyDV.join(', ')}]`,
    );
  }

  // ── tokens.css ──
  const cssHeader = [
    '/*',
    ' * @nexus/design-tokens — generated CSS variable layers.',
    ' *',
    ' * DERIVED OUTPUT — do not edit by hand. Regenerate from the sole token',
    ' * authority: repo-root DESIGN.md (light, :root) and DESIGN.dark.md (dark, .dark).',
    ' * Source sections: DESIGN.md §Typography, §Colors, §Spacing & Layout,',
    ' * §Elevation, §Motion, §Shapes, §Focus, §Component Primitives, §Canvas Surface,',
    ' * §Product-surface Recipes, §Implementation Mapping.',
    ' *',
    ' * Generate: pnpm --filter @nexus/design-tokens generate',
    ' * Check:    pnpm --filter @nexus/design-tokens check',
    ' */',
    '',
    "@import '@42ch/nexus-ui/theme.css';",
    '',
    ':root {',
    emitBlock(light, lightEntries, lightIndex, '  ', true),
    '}',
    '',
    '.dark {',
    emitBlock(dark, darkEntries, darkIndex, '  ', true),
    '}',
    '',
  ].join('\n');

  // ── theme.css (brand CSS custom properties, both themes) ──
  // Brand vars are the raw DESIGN color values under their public
  // --nexus-brand-<name> contract. tokens.css forwards --color-brand-* onto
  // these (see emitBlock brandAlias). Reuse the shared registry so the
  // projection walk happens once per theme (S-001).
  const brandLight = lightEntries
    .filter((e) => e.cssVar.startsWith('--color-brand-'))
    .map((e) => ({ ...e, cssVar: `--nexus-brand-${e.cssVar.slice('--color-brand-'.length)}` }));
  const brandDark = darkEntries
    .filter((e) => e.cssVar.startsWith('--color-brand-'))
    .map((e) => ({ ...e, cssVar: `--nexus-brand-${e.cssVar.slice('--color-brand-'.length)}` }));
  const brandCss = [
    '/*',
    ' * @42ch/nexus-ui — generated brand CSS custom properties.',
    ' *',
    ' * DERIVED OUTPUT — do not edit by hand. Source: repo-root DESIGN.md',
    ' * (light, :root) and DESIGN.dark.md (dark, .dark), §Brand Colors.',
    ' *',
    ' * Consumers import via `@42ch/nexus-ui/theme.css`; app-specific tokens',
    ' * live in @nexus/design-tokens/src/tokens.css (also generated).',
    ' */',
    ':root {',
    emitBlock(light, brandLight, lightIndex),
    '}',
    '',
    '.dark {',
    emitBlock(dark, brandDark, darkIndex),
    '}',
    '',
  ].join('\n');

  // ── generated-brand.ts (numeric light/default snapshot) ──
  // Snapshot must consume resolved, validated light scalars (never an empty
  // or unresolved literal): resolve each brand key through the shared light
  // projection index, which fails closed on null/undefined/empty/missing.
  const brandScalars = ['brand-deep-blue', 'brand-cyan', 'brand-white'];
  const colors = light.colors ?? {};
  for (const key of brandScalars) {
    if (colors[key] === undefined || colors[key] === null) {
      throw new Error(
        `[projectDesign] brand snapshot missing colors.${key} in DESIGN.md — fail closed, no empty generated literal.`,
      );
    }
  }
  const brandTokens = [
    '// Generated by @nexus/design-tokens — DERIVED OUTPUT, do not edit by hand.',
    '// Light/default numeric snapshot from repo-root DESIGN.md §Brand Colors.',
    '// Theme-aware UI reads CSS (--nexus-brand-* / --color-brand-*) rather than',
    '// these constants. `cyan` is the historical property name for the',
    '// recalibrated cobalt signal. Regenerate: pnpm --filter @nexus/design-tokens generate.',
    'export const brandColors = {',
    `  deepBlue: '${resolveScalar(colors['brand-deep-blue'], light, lightIndex, 'colors.brand-deep-blue')}' as const,`,
    `  cyan: '${resolveScalar(colors['brand-cyan'], light, lightIndex, 'colors.brand-cyan')}' as const,`,
    `  white: '${resolveScalar(colors['brand-white'], light, lightIndex, 'colors.brand-white')}' as const,`,
    '} as const;',
    '',
  ].join('\n');

  return { css: cssHeader, brandCss, brandTokens };
}

/** Locate the repository root starting from a given directory. */
export async function findRepoRoot(startDir) {
  let dir = resolve(startDir);
  for (;;) {
    try {
      await readFile(join(dir, 'DESIGN.md'), 'utf8');
      return dir;
    } catch {
      const parent = dirname(dir);
      if (parent === dir) throw new Error('repo root with DESIGN.md not found');
      dir = parent;
    }
  }
}
