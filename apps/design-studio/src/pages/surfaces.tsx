import type { ReactNode } from 'react';

import { cn } from '@web-lib/utils';

import { Badge } from '@web-ui/badge';
import { Button } from '@web-ui/button';
import { Card } from '@web-ui/card';
import { Label } from '@web-ui/label';

/* ------------------------------------------------------------------ */
/*  Data — IA guide §4.5 fixtures (canonical copy strings)              */
/* ------------------------------------------------------------------ */

const SETUP_STEPS = [
  { label: 'Welcome', state: 'active' as const },
  { label: 'Daemon', state: 'pending' as const },
  { label: 'Agent', state: 'pending' as const },
  { label: 'Done', state: 'pending' as const },
];

interface ShellNavItem {
  label: string;
  active?: boolean;
  children?: { label: string }[];
}

const SHELL_TABS: { label: string; active: boolean }[] = [
  { label: 'Creator', active: true },
  { label: 'Orchestrator', active: false },
];

const CREATOR_NAV: ShellNavItem[] = [
  { label: 'Works', active: true, children: [{ label: 'All Works' }] },
  { label: 'Worlds' },
  { label: 'Findings' },
];

// Kept for reference — IA guide §4.5 "Orchestrator" nav group fixture copy.
// const ORCHESTRATOR_NAV: ShellNavItem[] = [
//   { label: 'Runtime', active: true, children: [{ label: 'Sessions' }] },
//   { label: 'Schedules' },
//   { label: 'Capabilities' },
// ];

/* ------------------------------------------------------------------ */
/*  Sub-components — shared                                            */
/* ------------------------------------------------------------------ */

function SurfaceHeading({ children }: { children: ReactNode }) {
  return (
    <h3 className="text-heading-20 font-semibold text-gray-1000 mb-2 scroll-mt-16">
      {children}
    </h3>
  );
}

function SurfaceLabel({
  children,
}: {
  children: ReactNode;
}) {
  return (
    <section>
      <SurfaceHeading>{children}</SurfaceHeading>
      <p className="text-copy-14 text-gray-700 mb-6">
        Studio-local fixture composed from{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
          @web-ui/*
        </code>{' '}
        primitives — static copy per IA guide §4.5. No daemon data, no live
        routing, no product-page imports.
      </p>
    </section>
  );
}

/* ------------------------------------------------------------------ */
/*  Fixture 1 — Setup wizard step card                                  */
/* ------------------------------------------------------------------ */

function StepCircle({
  label,
  state,
}: {
  label: string;
  state: 'active' | 'pending';
}) {
  return (
    <div className="flex items-center gap-3">
      {/* Step circle */}
      <div
        className={cn(
          'w-8 h-8 rounded-full flex items-center justify-center shrink-0',
          'text-label-14 font-semibold',
          state === 'active'
            ? 'bg-blue-700 text-white'
            : 'bg-gray-alpha-100 text-gray-700',
        )}
        aria-current={state === 'active' ? 'step' : undefined}
      >
        {state === 'active' && (
          <svg
            width="16"
            height="16"
            viewBox="0 0 16 16"
            fill="none"
            aria-hidden="true"
          >
            <path
              d="M13.3 4.3L6 11.6L2.7 8.3"
              stroke="currentColor"
              strokeWidth="1.5"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
        )}
      </div>
      {/* Step label */}
      <span
        className={cn(
          'text-label-14',
          state === 'active' ? 'text-gray-1000 font-medium' : 'text-gray-700',
        )}
      >
        {label}
      </span>
    </div>
  );
}

function StepConnector() {
  return (
    <div className="flex items-center pl-[15px] h-6">
      <div className="w-[2px] h-full bg-gray-alpha-400 rounded-full" />
    </div>
  );
}

function SetupWizardFixture() {
  return (
    <div className="flex items-center justify-center min-h-[420px] p-4">
      {/* Outer card — integrated wizard surface per DESIGN.md §Setup Wizard Surface */}
      <Card className="flex flex-col sm:flex-row w-full max-w-[640px] p-0 shadow-modal rounded-popover overflow-hidden">
        {/* ── Left panel: step indicator list ── */}
        <div className="w-full sm:w-[208px] shrink-0 border-b sm:border-b-0 sm:border-r border-gray-alpha-200 bg-background-100 p-6">
          <div className="space-y-1">
            {SETUP_STEPS.map((step, idx) => (
              <div key={step.label}>
                <StepCircle label={step.label} state={step.state} />
                {idx < SETUP_STEPS.length - 1 && <StepConnector />}
              </div>
            ))}
          </div>
        </div>

        {/* ── Right panel: current step content (Welcome) ── */}
        <div className="flex-1 p-8 sm:p-10 bg-background-100 min-w-0">
          <h3 className="text-heading-24 font-semibold text-gray-1000 mb-2">
            Welcome to Nexus
          </h3>
          <p className="text-copy-16 text-gray-700 mb-8 leading-relaxed">
            Nexus needs a workspace folder for your creative projects. We will
            create it if it does not exist.
          </p>

          {/* Inline input row */}
          <Label className="block mb-1.5 text-label-14 text-gray-700">
            Workspace location
          </Label>
          <div className="flex items-center gap-2 mb-8">
            <div className="flex-1 flex items-center gap-3 h-12 px-4 rounded-control bg-background-200 border border-gray-alpha-400 text-copy-14">
              <svg
                width="16"
                height="16"
                viewBox="0 0 16 16"
                fill="none"
                className="text-blue-700 shrink-0"
                aria-hidden="true"
              >
                <path
                  d="M2 4.5C2 3.67157 2.67157 3 3.5 3H5.58579C5.851 3 6.10536 3.10536 6.29289 3.29289L7.70711 4.70711C7.89464 4.89464 8.149 5 8.41421 5H12.5C13.3284 5 14 5.67157 14 6.5V11.5C14 12.3284 13.3284 13 12.5 13H3.5C2.67157 13 2 12.3284 2 11.5V4.5Z"
                  stroke="currentColor"
                  strokeWidth="1.25"
                  fill="none"
                />
              </svg>
              <span className="text-copy-14 text-gray-1000 truncate">
                ~/Documents/nexus/default
              </span>
            </div>
            <Button variant="secondary" size="small">
              Browse…
            </Button>
          </div>

          {/* Primary CTA */}
          <div className="flex items-center gap-3">
            <Button variant="primary" className="max-w-[400px] w-full">
              Continue
            </Button>
          </div>
        </div>
      </Card>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Fixture 2 — App shell chrome                                        */
/* ------------------------------------------------------------------ */

function AvatarStub({ label }: { label: string }) {
  return (
    <div
      className={cn(
        'w-8 h-8 rounded-pill flex items-center justify-center shrink-0',
        'bg-gray-alpha-200 text-gray-700',
        'text-label-12 font-semibold select-none',
      )}
      aria-hidden="true"
    >
      {label.slice(0, 2).toUpperCase()}
    </div>
  );
}

function AppShellFixture() {
  const activeNav = CREATOR_NAV;

  return (
    <div className="flex min-h-[440px] border border-gray-alpha-300 rounded-card bg-background-100 overflow-hidden">
      {/* ── Sidebar ── */}
      <div className="w-[248px] shrink-0 border-r border-gray-alpha-200 bg-background-100 flex flex-col">
        {/* Top tabs */}
        <div className="flex border-b border-gray-alpha-200">
          {SHELL_TABS.map((tab) => (
            <button
              key={tab.label}
              type="button"
              tabIndex={-1}
              className={cn(
                'flex-1 text-center py-3 text-label-14 font-medium border-b-2 transition-colors',
                tab.active
                  ? 'text-gray-1000 border-blue-700'
                  : 'text-gray-700 border-transparent hover:text-gray-900',
              )}
            >
              {tab.label}
            </button>
          ))}
        </div>

        {/* Nav groups */}
        <div className="flex-1 overflow-auto p-3 space-y-1">
          {activeNav.map((item) => (
            <div key={item.label}>
              {/* Group label */}
              <div
                className={cn(
                  'flex items-center h-9 px-3 rounded-control text-label-14 transition-colors',
                  item.active
                    ? 'bg-gray-alpha-100 text-gray-1000'
                    : 'text-gray-700 hover:bg-gray-alpha-100 hover:text-gray-1000',
                )}
              >
                <span className="truncate">{item.label}</span>
                {item.children && item.children.length > 0 && (
                  <svg
                    width="12"
                    height="12"
                    viewBox="0 0 12 12"
                    fill="none"
                    className="ml-auto shrink-0 text-gray-500"
                    aria-hidden="true"
                  >
                    <path
                      d="M4 2L8 6L4 10"
                      stroke="currentColor"
                      strokeWidth="1.5"
                      strokeLinecap="round"
                      strokeLinejoin="round"
                    />
                  </svg>
                )}
              </div>

              {/* Nested items */}
              {item.children &&
                item.active &&
                item.children.map((child) => (
                  <div
                    key={child.label}
                    className={cn(
                      'flex items-center h-9 pl-6 pr-3 ml-3 rounded-control text-label-14',
                      'text-gray-1000 bg-gray-alpha-100 border-l-2 border-l-blue-700',
                    )}
                  >
                    <span className="truncate">{child.label}</span>
                  </div>
                ))}
            </div>
          ))}
        </div>

        {/* Footer — profile avatar row stub */}
        <div className="border-t border-gray-alpha-200 p-3 flex items-center gap-2">
          <AvatarStub label="Creator" />
          <div className="flex flex-col min-w-0">
            <span className="text-label-14 text-gray-1000 truncate">
              Local Creator
            </span>
            <span className="text-copy-13 text-gray-500 truncate">
              Profiles
            </span>
          </div>
          <div className="ml-auto flex items-center gap-1">
            <button
              type="button"
              tabIndex={-1}
              className="w-8 h-8 rounded-control flex items-center justify-center text-gray-700 hover:bg-gray-alpha-100 hover:text-gray-1000 transition-colors"
              aria-label="Add profile"
            >
              <svg
                width="14"
                height="14"
                viewBox="0 0 14 14"
                fill="none"
                aria-hidden="true"
              >
                <path
                  d="M7 1V13M1 7H13"
                  stroke="currentColor"
                  strokeWidth="1.5"
                  strokeLinecap="round"
                />
              </svg>
            </button>
          </div>
        </div>
      </div>

      {/* ── Main content area (empty — fixture shows chrome only) ── */}
      <div className="flex-1 bg-background-200 flex flex-col items-center justify-center min-w-0">
        <p className="text-copy-14 text-gray-500 mb-1">
          Content area
        </p>
        <p className="text-copy-13 text-gray-400">
          This fixture shows app-shell chrome only
        </p>
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Fixture 3 — Daemon status strip (healthy sample)                    */
/* ------------------------------------------------------------------ */

function DaemonStatusStrip() {
  return (
    <div className="border border-gray-alpha-300 rounded-card bg-background-100 p-4">
      <div className="flex items-start gap-3">
        {/* Status dot */}
        <div className="mt-1 w-2.5 h-2.5 rounded-full bg-green-700 shrink-0" />
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2 mb-1">
            <span className="text-label-14 text-gray-1000">Daemon running</span>
            <Badge variant="running">healthy</Badge>
          </div>
          <p className="text-copy-14 text-gray-700">
            Daemon API is reachable on the configured port.
          </p>
        </div>
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Page                                                                */
/* ------------------------------------------------------------------ */

export function SurfacesPage() {
  return (
    <div className="max-w-6xl mx-auto py-8 px-4">
      <h2 className="text-heading-24 font-semibold text-gray-1000 mb-2">
        Surfaces
      </h2>
      <p className="text-copy-16 text-gray-700 mb-6">
        Real product-surface slices — Setup wizard step card and App shell
        chrome, composed as studio-local fixtures from{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
          @web-ui/*
        </code>{' '}
        primitives per IA guide §4.5. No daemon data, no live routing, and no
        product-page imports (
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
          pages/
        </code>{' '}
        or{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
          components/layout/
        </code>
        ).
      </p>

      {/* 1. Setup wizard step card */}
      <section>
        <SurfaceHeading>Setup — Step card</SurfaceHeading>
        <p className="text-copy-14 text-gray-700 mb-6">
          A studio-local fixture reproducing the V1.96 integrated wizard card
          appearance. Left panel: step indicator list (Welcome → Daemon → Agent
          → Done) per{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            components.setup-wizard-step
          </code>{' '}
          tokens. Right panel: Welcome step body with workspace-location
          affordance per{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            components.setup-wizard-surface
          </code>
          . Static fixture — no Tauri IPC, no daemon wiring.
        </p>
        <SetupWizardFixture />
      </section>

      {/* 2. App shell chrome */}
      <SurfaceLabel>
        App shell chrome
      </SurfaceLabel>

      <p className="text-copy-14 text-gray-700 mb-6">
        Sidebar tab strip (Creator / Orchestrator), one expanded nav group
        (Works → All Works), footer profile avatar row stub, and slim daemon
        status strip — chrome only, no live routing or{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
          NexusClient
        </code>
        . Composed from{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
          @web-ui/*
        </code>{' '}
        primitives + inline SVG icons; no layout component imports.
      </p>

      <AppShellFixture />

      {/* Daemon status strip */}
      <section className="mt-6">
        <SurfaceHeading>Daemon status strip</SurfaceHeading>
        <p className="text-copy-14 text-gray-700 mb-6">
          Healthy daemon status affordance — green dot, badge, helper text.
          Composed from{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            @web-ui/badge
          </code>{' '}
          with inline markup. Per DESIGN.md{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            components.daemon-status-indicator
          </code>{' '}
          tokens.
        </p>
        <DaemonStatusStrip />
      </section>

      <p className="text-copy-13 text-gray-500 mt-12 pt-8 border-t border-gray-alpha-200">
        2 surface fixtures (Setup step card + App shell chrome) composed from{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
          @web-ui/*
        </code>{' '}
        primitives + layout CSS. No live product pages, no{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
          components/layout/
        </code>{' '}
        imports, no daemon wiring. Copy strings from IA guide §4.5 (canonical
        fixtures).
      </p>
    </div>
  );
}
