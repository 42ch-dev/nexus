#!/usr/bin/env node
/**
 * @nexus/design-tokens — v0.4 token contract gate.
 *
 * Validates that the DESIGN.md v0.4 contract is actually projected through
 * this package: expected CSS custom properties exist in src/tokens.css
 * (:root + .dark) and the matching preset keys exist in tailwind.preset.ts.
 * Text-based assertions (dependency-light); type-checking of the preset is
 * handled by `tsc --noEmit` in the `build` script before this runs.
 *
 * Exit 0 = all checks pass; exit 1 = at least one missing projection.
 */
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const tokens = readFileSync(join(root, 'src/tokens.css'), 'utf8');
const preset = readFileSync(join(root, 'tailwind.preset.ts'), 'utf8');

/** @type {Array<{ label: string, haystack: string, needle: string }>} */
const required = [
  // ── Display tier (V1.121 v0.4 T1) ──
  { label: 'tokens: display family', haystack: tokens, needle: '--font-display:' },
  { label: 'tokens: display-32 size var', haystack: tokens, needle: '--text-display-32:' },
  { label: 'tokens: display-32 line-height var', haystack: tokens, needle: '--text-display-32--line-height:' },
  { label: 'tokens: display-32 letter-spacing var', haystack: tokens, needle: '--text-display-32--letter-spacing:' },
  { label: 'tokens: display-32 font-weight var', haystack: tokens, needle: '--text-display-32--font-weight:' },
  { label: 'tokens: display-24 size var', haystack: tokens, needle: '--text-display-24:' },
  { label: 'tokens: display-20 size var', haystack: tokens, needle: '--text-display-20:' },
  { label: 'tokens: serif 400 @font-face', haystack: tokens, needle: 'source-serif-4-latin-400-normal.woff2' },
  { label: 'tokens: serif 600 @font-face', haystack: tokens, needle: 'source-serif-4-latin-600-normal.woff2' },
  { label: 'preset: fontFamily.display consumes var', haystack: preset, needle: "display: 'var(--font-display)'" },
  { label: 'preset: fontSize display-32 key', haystack: preset, needle: "'display-32':" },
  { label: 'preset: fontSize display-24 key', haystack: preset, needle: "'display-24':" },
  { label: 'preset: fontSize display-20 key', haystack: preset, needle: "'display-20':" },
  { label: 'preset: display-32 consumes vars (no literal fork)', haystack: preset, needle: "sv('text-display-32')" },

  // ── Spacing / radius scales (V1.121 v0.4 scalar projection) ──
  { label: 'tokens: --space-1', haystack: tokens, needle: '--space-1:' },
  { label: 'tokens: --space-24', haystack: tokens, needle: '--space-24:' },
  { label: 'tokens: --radius-control', haystack: tokens, needle: '--radius-control:' },
  { label: 'tokens: --radius-pill', haystack: tokens, needle: '--radius-pill:' },
  { label: 'preset: spacing step consumes var', haystack: preset, needle: "sv('space-1')" },
  { label: 'preset: borderRadius consumes var', haystack: preset, needle: "control: sv('radius-control')" },

  // ── Motion scale (V1.121 v0.4 T4) ──
  { label: 'tokens: --duration-enter', haystack: tokens, needle: '--duration-enter:' },
  { label: 'tokens: --duration-exit', haystack: tokens, needle: '--duration-exit:' },
  { label: 'tokens: --duration-instant', haystack: tokens, needle: '--duration-instant:' },
  { label: 'tokens: --ease-standard', haystack: tokens, needle: '--ease-standard:' },
  { label: 'tokens: --ease-emphasized', haystack: tokens, needle: '--ease-emphasized:' },
  { label: 'preset: transitionDuration consumes var', haystack: preset, needle: "state: sv('duration-state')" },
  { label: 'preset: transitionTimingFunction consumes var', haystack: preset, needle: "standard: sv('ease-standard')" },

  // ── Elevation scale + alias chain (V1.121 v0.4 T3) ──
  { label: 'tokens: --shadow-elevation-0', haystack: tokens, needle: '--shadow-elevation-0:' },
  { label: 'tokens: --shadow-elevation-1', haystack: tokens, needle: '--shadow-elevation-1:' },
  { label: 'tokens: --shadow-elevation-2', haystack: tokens, needle: '--shadow-elevation-2:' },
  { label: 'tokens: --shadow-elevation-3', haystack: tokens, needle: '--shadow-elevation-3:' },
  { label: 'tokens: --shadow-elevation-4', haystack: tokens, needle: '--shadow-elevation-4:' },
  { label: 'tokens: alias shadow-card → elevation-1', haystack: tokens, needle: '--shadow-card: var(--shadow-elevation-1)' },
  { label: 'tokens: alias shadow-popover → elevation-3', haystack: tokens, needle: '--shadow-popover: var(--shadow-elevation-3)' },
  { label: 'tokens: alias shadow-modal → elevation-4', haystack: tokens, needle: '--shadow-modal: var(--shadow-elevation-4)' },
  { label: 'preset: boxShadow elevation-4 key', haystack: preset, needle: "'elevation-4':" },

  // ── Canvas node width family (structural namespace, V1.121 v0.4 T5) ──
  { label: 'tokens: --canvas-node-width-strategy-root', haystack: tokens, needle: '--canvas-node-width-strategy-root:' },
  { label: 'tokens: --canvas-node-width-default', haystack: tokens, needle: '--canvas-node-width-default:' },
  { label: 'preset: minWidth canvas-node key', haystack: preset, needle: "'canvas-node-strategy-root':" },
  { label: 'preset: minWidth consumes structural var', haystack: preset, needle: "sv('canvas-node-width-strategy-root')" },

  // ── Dialog/Sheet layout metrics (structural namespace, V1.121 P1) ──
  { label: 'tokens: --dialog-width', haystack: tokens, needle: '--dialog-width:' },
  { label: 'tokens: --dialog-max-height', haystack: tokens, needle: '--dialog-max-height:' },
  { label: 'tokens: --sheet-width', haystack: tokens, needle: '--sheet-width:' },
  { label: 'preset: width dialog consumes structural var', haystack: preset, needle: "dialog: sv('dialog-width')" },
  { label: 'preset: width sheet consumes structural var', haystack: preset, needle: "sheet: sv('sheet-width')" },
  { label: 'preset: maxHeight dialog consumes structural var', haystack: preset, needle: "dialog: sv('dialog-max-height')" },

  // ── Sidebar nav sizing (structural namespace, v1.183 P0 AR-2) ──
  { label: 'tokens: --sidebar-nav-width', haystack: tokens, needle: '--sidebar-nav-width:' },
  { label: 'tokens: --sidebar-nav-item-height', haystack: tokens, needle: '--sidebar-nav-item-height:' },
  { label: 'preset: spacing sidebar-nav width consumes structural var', haystack: preset, needle: "sv('sidebar-nav-width')" },
  { label: 'preset: spacing sidebar-nav item-height consumes structural var', haystack: preset, needle: "sv('sidebar-nav-item-height')" },

  // ── Reading chrome projection (V1.121 v0.4 T6) ──
  { label: 'tokens: reading-chrome title family → display', haystack: tokens, needle: '--reading-chrome-novel-chapter-title-font-family: var(--font-display)' },

  // ── Badge family tint projection (V1.121 P1 T3) ──
  { label: 'tokens: finding-status open bg', haystack: tokens, needle: '--color-finding-status-open-bg:' },
  { label: 'tokens: finding-status wont-fix text', haystack: tokens, needle: '--color-finding-status-wont-fix-text:' },
  { label: 'tokens: finding-status duplicate border', haystack: tokens, needle: '--color-finding-status-duplicate-border:' },
  { label: 'tokens: memory-task-kind brainstorm bg', haystack: tokens, needle: '--color-memory-task-kind-brainstorm-bg:' },
  { label: 'tokens: memory-task-kind unknown border', haystack: tokens, needle: '--color-memory-task-kind-unknown-border:' },
  { label: 'tokens: reading-maturation kb-density bg', haystack: tokens, needle: '--color-reading-maturation-kb-density-bg:' },
  { label: 'tokens: reading-maturation open-findings text', haystack: tokens, needle: '--color-reading-maturation-open-findings-text:' },
  { label: 'preset: finding-status color group', haystack: preset, needle: "'finding-status':" },
  { label: 'preset: memory-task-kind color group', haystack: preset, needle: "'memory-task-kind':" },
  { label: 'preset: reading-maturation color group', haystack: preset, needle: "'reading-maturation':" },
  { label: 'preset: finding-status consumes var', haystack: preset, needle: "cv('finding-status-open-bg')" },
  { label: 'tokens: data-table row-protected', haystack: tokens, needle: '--color-data-table-row-protected:' },
  { label: 'tokens: main-banner background', haystack: tokens, needle: '--color-main-banner-background:' },
  { label: 'preset: data-table-row-protected', haystack: preset, needle: "cv('data-table-row-protected')" },
  { label: 'preset: main-banner-background', haystack: preset, needle: "cv('main-banner-background')" },
  { label: 'preset: fontSize copy-12 key', haystack: preset, needle: "'copy-12':" },
  // ── Disabled-state wash (V1.113 P1 token; preset promotion v1.183 P0 AR-1) ──
  { label: 'tokens: --color-states-disabled-opacity', haystack: tokens, needle: '--color-states-disabled-opacity:' },
  { label: 'preset: opacity disabled consumes var', haystack: preset, needle: "cv('states-disabled-opacity')" },
];

/** Banned legacy namespace, assembled so a repo-wide grep for the literal
 *  string stays zero — this script is the mechanical enforcer instead. */
const BANNED_NODE_WIDTH_NS = '--color-canvas-' + 'node-width';
const BANNED_NODE_WIDTH_HELPER = "cv('canvas-node-" + 'width';
const BANNED_DIALOG_WIDTH_NS = '--color-dialog-' + 'width';
const BANNED_DIALOG_MAX_HEIGHT_NS = '--color-dialog-max-' + 'height';
const BANNED_SHEET_WIDTH_NS = '--color-sheet-' + 'width';
const BANNED_DIALOG_WIDTH_HELPER = "cv('dialog-" + "width')";
const BANNED_DIALOG_MAX_HEIGHT_HELPER = "cv('dialog-max-" + "height')";
const BANNED_SHEET_WIDTH_HELPER = "cv('sheet-" + "width')";
const BANNED_SIDEBAR_NAV_NS = '--color-sidebar-' + 'nav-';
const BANNED_SIDEBAR_NAV_HELPER = "cv('sidebar-" + "nav-";

/** @type {Array<{ label: string, haystack: string, needle: string }>} */
const forbidden = [
  { label: 'tokens: node widths must not use the color namespace', haystack: tokens, needle: BANNED_NODE_WIDTH_NS },
  { label: 'preset: node widths must not use the color-var helper', haystack: preset, needle: BANNED_NODE_WIDTH_HELPER },
  { label: 'tokens: dialog width must not use the color namespace', haystack: tokens, needle: BANNED_DIALOG_WIDTH_NS },
  { label: 'tokens: dialog max-height must not use the color namespace', haystack: tokens, needle: BANNED_DIALOG_MAX_HEIGHT_NS },
  { label: 'tokens: sheet width must not use the color namespace', haystack: tokens, needle: BANNED_SHEET_WIDTH_NS },
  { label: 'preset: dialog width must not use the color-var helper', haystack: preset, needle: BANNED_DIALOG_WIDTH_HELPER },
  { label: 'preset: dialog max-height must not use the color-var helper', haystack: preset, needle: BANNED_DIALOG_MAX_HEIGHT_HELPER },
  { label: 'preset: sheet width must not use the color-var helper', haystack: preset, needle: BANNED_SHEET_WIDTH_HELPER },
  { label: 'tokens: sidebar-nav sizing must not use the color namespace', haystack: tokens, needle: BANNED_SIDEBAR_NAV_NS },
  { label: 'preset: sidebar-nav sizing must not use the color-var helper', haystack: preset, needle: BANNED_SIDEBAR_NAV_HELPER },
];

/** Light (:root) and dark blocks — theme-split assertions below. */
const [lightBlock, darkBlock] = tokens.split(/\n\.dark \{/);
if (!darkBlock) {
  console.error('FAIL tokens: no .dark block found');
  process.exit(1);
}

/** V1.137 P0: light semantic *-active* pairs on opaque blue-1000 → brand-white label. */
const lightBlue1000ActivePairs = [
  {
    label: 'setup wizard step circle',
    bg: '--color-setup-wizard-step-circle-active-bg: var(--color-blue-1000)',
    text: '--color-setup-wizard-step-circle-active-text: var(--color-brand-white)',
  },
  {
    label: 'footer profile avatar',
    bg: '--color-footer-profile-avatar-bg-active: var(--color-blue-1000)',
    text: '--color-footer-profile-avatar-text-active: var(--color-brand-white)',
  },
];
for (const { label, bg, text } of lightBlue1000ActivePairs) {
  if (!lightBlock.includes(bg) || !lightBlock.includes(text)) {
    console.error(
      `FAIL tokens :root: light ${label} active pair must be blue-1000 fill + brand-white text`,
    );
    process.exit(1);
  }
}

/** V1.137 P0: dark active pairs on blue-700 keep brand-deep-blue label (Q2). */
const darkBlue700ActivePairs = [
  {
    label: 'setup wizard step circle',
    bg: '--color-setup-wizard-step-circle-active-bg: var(--color-blue-700)',
    text: '--color-setup-wizard-step-circle-active-text: var(--color-brand-deep-blue)',
  },
  {
    label: 'footer profile avatar',
    bg: '--color-footer-profile-avatar-bg-active: var(--color-blue-700)',
    text: '--color-footer-profile-avatar-text-active: var(--color-brand-deep-blue)',
  },
];
for (const { label, bg, text } of darkBlue700ActivePairs) {
  if (!darkBlock.includes(bg) || !darkBlock.includes(text)) {
    console.error(
      `FAIL tokens .dark: ${label} active pair must stay blue-700 fill + brand-deep-blue text`,
    );
    process.exit(1);
  }
}
for (const needle of ['--color-background-100:', '--color-gray-100:', '--shadow-elevation-1:', '--color-finding-status-open-bg:', '--color-memory-task-kind-brainstorm-bg:', '--color-reading-maturation-kb-density-bg:']) {
  if (!darkBlock.includes(needle)) {
    console.error(`FAIL tokens .dark: missing ${needle}`);
    process.exit(1);
  }
}

let failed = 0;
for (const { label, haystack, needle } of required) {
  if (!haystack.includes(needle)) {
    console.error(`FAIL ${label} — expected to find: ${needle}`);
    failed += 1;
  }
}
for (const { label, haystack, needle } of forbidden) {
  if (haystack.includes(needle)) {
    console.error(`FAIL ${label} — must not contain: ${needle}`);
    failed += 1;
  }
}

if (failed > 0) {
  console.error(`\n${failed} token projection check(s) failed.`);
  process.exit(1);
}
console.log(`design-tokens v0.4 contract: ${required.length} projections OK, ${forbidden.length} namespace guards OK`);
