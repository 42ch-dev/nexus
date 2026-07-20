/**
 * Shared app-shell sidebar nav data — IA guide §4.5 canonical copy strings.
 *
 * Single source for the Creator + Orchestrator nav groups consumed by both
 * the Surfaces gallery fixture and the Settings host fixture. Extracted in
 * V1.109 P2 (R-V1108P1QC1-S001) to eliminate the duplicated constant literals
 * that risked fixture drift.
 *
 * V1.128 P2 T1 — Worlds-first Creator IA (matches apps/web sidebar.tsx).
 */
import {
  Boxes,
  BrainCircuit,
  CalendarClock,
  Globe,
  Layers,
  ListChecks,
  Sparkles,
} from 'lucide-react';

import type { ShellNavGroup } from '@web-layout/shell-sidebar-chrome';

export const CREATOR_NAV: ShellNavGroup[] = [
  {
    id: 'worlds',
    label: 'Worlds',
    items: [{ to: '#worlds', label: 'Worlds', icon: Globe }],
  },
  {
    id: 'works',
    label: 'Works',
    items: [{ to: '#works', label: 'All Works', icon: Layers }],
  },
];

export const ORCHESTRATOR_NAV: ShellNavGroup[] = [
  {
    id: 'memory',
    label: 'Memory',
    items: [{ to: '#memory', label: 'Memory', icon: BrainCircuit }],
  },
  {
    id: 'runtime',
    label: 'Runtime',
    items: [
      { to: '#sessions', label: 'Sessions', icon: ListChecks },
      { to: '#schedule', label: 'Schedule', icon: CalendarClock },
      { to: '#capabilities', label: 'Capabilities', icon: Boxes },
    ],
  },
  {
    id: 'strategies',
    label: 'Strategies',
    items: [{ to: '#strategies', label: 'Strategies', icon: Sparkles }],
  },
];
