import { describe, expect, it } from 'vitest';

import { cn } from './cn';

/**
 * tailwind-merge registration regression — DESIGN.md v0.4 (V1.121).
 *
 * Threat model: the V1.94 silent-strip class of bug. Custom token-backed
 * utilities that tailwind-merge does not know are either misparsed
 * (`text-display-24` as text-color → a later `text-white` silently drops the
 * font-size) or never conflict (unknown `duration-enter` + `duration-200`
 * both survive → two duration declarations on one element). Every new v0.4
 * class group registered in `cn.ts` gets representative coverage here:
 * font-size (display tier), font-family (`font-display`), shadow
 * (`shadow-elevation-*` + legacy aliases), transition-duration
 * (`duration-enter/exit`), and min-width (`min-w-canvas-node-*`).
 */
describe('cn — v0.4 display typography tier (font-size group)', () => {
  it('keeps both the display size and a real text color (V1.94 threat)', () => {
    // Must NOT collapse to 'text-white' — the display size is not a color.
    expect(cn('text-display-24', 'text-white')).toBe('text-display-24 text-white');
  });

  it('merges display sizes against the heading tier as one font-size group', () => {
    expect(cn('text-display-24', 'text-heading-16')).toBe('text-heading-16');
    expect(cn('text-heading-16', 'text-display-24')).toBe('text-display-24');
  });

  it('merges two display sizes keeping the later one', () => {
    expect(cn('text-display-32', 'text-display-20')).toBe('text-display-20');
  });
});

describe('cn — v0.4 font-display (font-family group)', () => {
  it('merges font-display against font-sans keeping the later one', () => {
    expect(cn('font-display', 'font-sans')).toBe('font-sans');
    expect(cn('font-sans', 'font-display')).toBe('font-display');
  });

  it('does not drop unrelated classes when merging families', () => {
    expect(cn('font-display', 'text-display-24')).toBe('font-display text-display-24');
  });
});

describe('cn — v0.4 elevation scale (shadow group)', () => {
  it('collapses elevation tokens against legacy alias shadows to one class', () => {
    expect(cn('shadow-elevation-2', 'shadow-card')).toBe('shadow-card');
    expect(cn('shadow-card', 'shadow-elevation-2')).toBe('shadow-elevation-2');
  });

  it('collapses elevation tokens against the default shadow scale', () => {
    expect(cn('shadow-elevation-2', 'shadow-md')).toBe('shadow-md');
    expect(cn('shadow-sm', 'shadow-elevation-4')).toBe('shadow-elevation-4');
  });

  it('keeps shadow-elevation-* alongside unrelated utilities', () => {
    expect(cn('shadow-elevation-2', 'text-white')).toBe('shadow-elevation-2 text-white');
    expect(cn('shadow-elevation-0', 'shadow-none')).toBe('shadow-none');
  });
});

describe('cn — v0.4 motion pair (transition-duration group)', () => {
  it('collapses duration-enter/exit against default durations', () => {
    expect(cn('duration-enter', 'duration-200')).toBe('duration-200');
    expect(cn('duration-200', 'duration-enter')).toBe('duration-enter');
  });

  it('collapses duration-enter against duration-exit', () => {
    expect(cn('duration-enter', 'duration-exit')).toBe('duration-exit');
  });

  it('keeps duration-enter alongside unrelated utilities', () => {
    expect(cn('duration-enter', 'text-white')).toBe('duration-enter text-white');
  });
});

describe('cn — v0.4 canvas node width family (min-width group)', () => {
  it('collapses canvas-node widths against default min-w utilities', () => {
    expect(cn('min-w-canvas-node-default', 'min-w-0')).toBe('min-w-0');
    expect(cn('min-w-0', 'min-w-canvas-node-strategy-root')).toBe('min-w-canvas-node-strategy-root');
  });

  it('merges two canvas-node widths keeping the later one', () => {
    expect(cn('min-w-canvas-node-strategy-root', 'min-w-canvas-node-outline-scene-beat')).toBe(
      'min-w-canvas-node-outline-scene-beat',
    );
  });

  it('keeps min-w-canvas-node-* alongside unrelated utilities', () => {
    expect(cn('min-w-canvas-node-default', 'text-white')).toBe('min-w-canvas-node-default text-white');
  });
});

describe('cn — P1 dialog/sheet sizing tokens (w / max-w / max-h groups)', () => {
  it('merges w-dialog against a default width, later wins', () => {
    expect(cn('w-dialog', 'w-4')).toBe('w-4');
    expect(cn('w-4', 'w-dialog')).toBe('w-dialog');
  });

  it('merges w-dialog against w-sheet as one width group', () => {
    expect(cn('w-dialog', 'w-sheet')).toBe('w-sheet');
  });

  it('merges max-w-dialog against default max widths', () => {
    expect(cn('max-w-dialog', 'max-w-md')).toBe('max-w-md');
    expect(cn('max-w-md', 'max-w-dialog')).toBe('max-w-dialog');
  });

  it('merges max-h-dialog against default max heights (alongside max-h-listbox)', () => {
    expect(cn('max-h-dialog', 'max-h-40')).toBe('max-h-40');
    expect(cn('max-h-listbox', 'max-h-dialog')).toBe('max-h-dialog');
  });

  it('keeps dialog sizing tokens alongside unrelated utilities', () => {
    expect(cn('w-dialog', 'text-white')).toBe('w-dialog text-white');
  });
});

describe('cn — P1 motion duration completion (transition-duration group)', () => {
  it('collapses duration-state against duration-popover', () => {
    expect(cn('duration-state', 'duration-popover')).toBe('duration-popover');
  });

  it('collapses duration-popover against duration-enter', () => {
    expect(cn('duration-popover', 'duration-enter')).toBe('duration-enter');
  });

  it('collapses duration-state against default durations', () => {
    expect(cn('duration-state', 'duration-150')).toBe('duration-150');
  });
});

describe('cn — pre-existing registrations unchanged', () => {
  it('keeps the existing font-size registrations working', () => {
    expect(cn('text-heading-32', 'text-white')).toBe('text-heading-32 text-white');
    expect(cn('text-copy-14', 'text-button-12')).toBe('text-button-12');
  });

  it('keeps the V1.113 opacity/max-h registrations working', () => {
    expect(cn('opacity-disabled', 'opacity-50')).toBe('opacity-50');
    expect(cn('max-h-listbox', 'max-h-40')).toBe('max-h-40');
  });
});
