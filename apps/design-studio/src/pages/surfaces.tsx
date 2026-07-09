import type { ReactNode } from 'react';

import { cn, Badge } from '@42ch/nexus-ui';

import { AgentPickerFixtures } from '@/fixtures/agent-picker-fixtures';
import { SettingsHostFixtures } from '@/fixtures/settings-host-fixtures';
import { SetupWizardChromeFixtures } from '@/fixtures/setup-wizard-chrome-fixtures';

/* ------------------------------------------------------------------ */
/*  Data — IA guide §4.5 fixtures (canonical copy strings)              */
/* ------------------------------------------------------------------ */

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

/* ------------------------------------------------------------------ */
/*  Fixture — App shell chrome                                          */
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
      {/* ── Sidebar — DESIGN.md §Sidebar Nav ── */}
      <div className="w-sidebar-nav-width shrink-0 border-r border-gray-alpha-200 bg-background-100 flex flex-col">
        {/* Top tabs — DESIGN.md §shell-nav */}
        <div className="flex border-b border-gray-alpha-200">
          {SHELL_TABS.map((tab) => (
            <button
              key={tab.label}
              type="button"
              tabIndex={-1}
              className={cn(
                'flex-1 text-center py-3 text-label-14 font-medium border-b-2 transition-colors',
                tab.active
                  ? 'text-gray-1000 border-blue-700 bg-gray-alpha-100'
                  : 'text-gray-700 border-transparent hover:text-gray-900 hover:bg-gray-alpha-100',
              )}
            >
              {tab.label}
            </button>
          ))}
        </div>

        {/* Nav groups — DESIGN.md §Sidebar Nav */}
        <nav className="flex-1 overflow-auto p-3 space-y-1">
          {activeNav.map((item) => (
            <div key={item.label}>
              {/* Group label */}
              <div
                className={cn(
                  'flex items-center h-sidebar-nav-item-height px-3 rounded-control text-label-14 transition-colors',
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
                    className="ml-auto shrink-0 text-gray-600"
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
                      'flex items-center h-sidebar-nav-item-height pl-6 pr-3 ml-3 rounded-control text-label-14',
                      'text-gray-1000 bg-gray-alpha-100 border-l-2 border-l-blue-700',
                    )}
                  >
                    <span className="truncate">{child.label}</span>
                  </div>
                ))}
            </div>
          ))}
        </nav>

        {/* Footer — profile avatar row stub */}
        <div className="border-t border-gray-alpha-200 p-3 flex items-center gap-2">
          <AvatarStub label="Creator" />
          <div className="flex flex-col min-w-0">
            <span className="text-label-14 text-gray-1000 truncate">
              Local Creator
            </span>
            <span className="text-copy-13 text-gray-700 truncate">
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

      {/* ── Main content area — recessed background, placeholder indicates active workspace ── */}
      <div className="flex-1 bg-background-200 flex flex-col items-center justify-center min-w-0 p-8">
        <div className="border-2 border-dashed border-gray-alpha-300 rounded-card w-full max-w-md p-8 text-center">
          <p className="text-copy-14 text-gray-700 mb-1">
            Content panel
          </p>
          <p className="text-copy-13 text-gray-500">
            Active workspace — editor, canvas, or dashboard — rendered by the
            product shell at runtime. Not part of this fixture.
          </p>
        </div>
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
          @42ch/nexus-ui
        </code>{' '}
        (promoted) and{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
          @web-ui/*
        </code>{' '}
        (transitional) primitives per IA guide §4.5. No daemon data, no live routing, and no
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

      {/* 1. Setup wizard chrome polish (V1.101 P1) */}
      <section>
        <SurfaceHeading>Setup — Wizard chrome</SurfaceHeading>
        <p className="text-copy-14 text-gray-700 mb-6">
          Studio-local chrome fixtures for V1.101 P1 polish contract §8: Steps
          matrices (welcome / daemon / agent / done) with numbered
          complete/active/pending circles, normative Back+Continue horizontal
          CTA row, and daemon status chips (starting / running / error). Tokens
          from{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            components.setup-wizard-step
          </code>{' '}
          and{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            components.setup-wizard-surface
          </code>
          . Static — no Tauri IPC, no daemon wiring, no App page imports.
        </p>
        <SetupWizardChromeFixtures />
      </section>

      {/* 2. App shell chrome */}
      <section>
        <SurfaceHeading>App shell chrome</SurfaceHeading>
        <p className="text-copy-14 text-gray-700 mb-6">
          Sidebar tab strip (Creator / Orchestrator), one expanded nav group
          (Works → All Works), footer profile avatar row stub, and daemon
          status strip — studio-local chrome fixtures built with inline HTML/SVG
          and Badge from{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            @42ch/nexus-ui
          </code>{' '}
          for the daemon status strip. No live routing, no{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            NexusClient
          </code>
          , and no layout component imports.
        </p>
        <AppShellFixture />
      </section>

      {/* 3. AgentPicker visual states (V1.101 P0) */}
      <section className="mt-10">
        <SurfaceHeading>Setup — AgentPicker</SurfaceHeading>
        <p className="text-copy-14 text-gray-700 mb-6">
          Presentational card grid from{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            @web-setup/agent-picker
          </code>{' '}
          (apps/web setup composition — not{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            @42ch/nexus-ui
          </code>
          ). Props-driven fixtures: loading, installed grid, mixed, empty, error,
          selected. No contracts, no daemon client.
        </p>
        <AgentPickerFixtures />
      </section>

      {/* 4. Thin Settings host (V1.102 P1) */}
      <section className="mt-10">
        <SurfaceHeading>Settings — Thin host</SurfaceHeading>
        <p className="text-copy-14 text-gray-700 mb-6">
          Studio-local chrome for DF-70 slice A: footer utility{' '}
          <strong className="font-medium text-gray-1000">Settings</strong>{' '}
          (lucide) above profiles, plus a thin host page mounting{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            @web-setup/agent-picker
          </code>{' '}
          with fixture props. Not a wizard re-run — no Steps / Back+Continue.
          No daemon, no product{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            pages/
          </code>{' '}
          or{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            components/layout/
          </code>{' '}
          imports.
        </p>
        <SettingsHostFixtures />
      </section>

      {/* Daemon status strip */}
      <section className="mt-6">
        <SurfaceHeading>Daemon status strip</SurfaceHeading>
        <p className="text-copy-14 text-gray-700 mb-6">
          Healthy daemon status affordance — green dot, badge, helper text.
        Composed from{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
          @42ch/nexus-ui
        </code>{' '}
        +{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
          @web-ui/*
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
        Surface fixtures: Setup wizard chrome, App shell chrome, AgentPicker
        states, Settings thin host, daemon status strip. Composed from{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
          @42ch/nexus-ui
        </code>
        ,{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
          @web-setup/*
        </code>
        , and transitional{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
          @web-ui/*
        </code>
        . No live product pages, no{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
          components/layout/
        </code>{' '}
        imports, no daemon wiring.
      </p>
    </div>
  );
}
