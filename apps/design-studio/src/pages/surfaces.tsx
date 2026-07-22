import { useRef, useState, type ReactNode } from 'react';
import { NavLink, Outlet } from 'react-router-dom';

import { cn, Badge, Button } from '@42ch/nexus-ui';

import { StudioShellLogo } from '@/components/studio-shell-logo';

import {
  ShellSidebarChrome,
  type ShellSidebarTab,
} from '@web-layout/shell-sidebar-chrome';
import { FooterProfilesChrome } from '@web-layout/footer-profiles-chrome';
import { DaemonHealthIndicatorChrome } from '@web-layout/daemon-health-indicator-chrome';

import {
  SurfaceSourceBadges,
  SurfaceSourceLegend,
} from '@/components/surface-source-badge';
import { AgentPickerFixtures } from '@/fixtures/agent-picker-fixtures';
import { CanvasSurfacesFixtures } from '@/fixtures/canvas-surfaces-fixtures';
import { LaunchDaemonFixtures } from '@/fixtures/launch-daemon-fixtures';
import {
  CREATOR_NAV,
  ORCHESTRATOR_NAV,
} from '@/fixtures/shell-nav-data';
import { SettingsHostFixtures } from '@/fixtures/settings-host-fixtures';
import { SetupWizardChromeFixtures } from '@/fixtures/setup-wizard-chrome-fixtures';
import { ConflictModalFixtures } from '@/fixtures/conflict-modal-fixtures';
import { CreatorShellFixtures } from '@/fixtures/creator-shell-fixtures';
import { ChronosTitlebarFixtures } from '@/fixtures/chronos-titlebar-fixtures';
import { GlobalTimelineFixtures } from '@/fixtures/global-timeline-fixtures';
import { LayerBreadcrumbFixtures } from '@/fixtures/layer-breadcrumb-fixtures';
import { SelectionSubmenuStubFixtures } from '@/fixtures/selection-submenu-fixtures';
import { NleTimelineCanvasFixtures } from '@/fixtures/nle-timeline-canvas-fixtures';
import { TimelineCanvasFixtures } from '@/fixtures/timeline-canvas-fixtures';
import { WorkTimelineCanvasFixtures } from '@/fixtures/work-timeline-canvas-fixtures';

/* ------------------------------------------------------------------ */
/*  Data — IA guide §4.5 fixtures (canonical copy strings)              */
/* ------------------------------------------------------------------ */

const SURFACES_SECTIONS = [
  {
    label: 'Overview',
    path: '/surfaces',
    end: true,
    desc: 'Index of Studio surface chrome slices',
  },
  {
    label: 'Setup',
    path: '/surfaces/setup',
    end: false,
    desc: 'Setup wizard chrome (Steps / Back+Continue / daemon chips)',
  },
  {
    label: 'Shell',
    path: '/surfaces/shell',
    end: false,
    desc: 'App shell sidebar + Settings shell chrome',
  },
  {
    label: 'AgentPicker',
    path: '/surfaces/agent-picker',
    end: false,
    desc: 'AgentPicker visual states',
  },
  {
    label: 'Canvas',
    path: '/surfaces/canvas',
    end: false,
    desc: 'Canvas shell + context menu chrome (presentational preview)',
  },
  {
    label: 'Daemon',
    path: '/surfaces/daemon',
    end: false,
    desc: 'Daemon status strip',
  },
  {
    label: 'Launch',
    path: '/surfaces/launch',
    end: false,
    desc: 'Desktop launch splash — waiting, error, and recovery',
  },
  {
    label: 'Selection Submenu',
    path: '/surfaces/selection-submenu',
    end: false,
    desc: 'Selection submenu fixture — 6 variants (V1.126 P0 T4)',
  },
] as const;

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

function SurfacesSectionNav() {
  return (
    <nav
      aria-label="Surfaces sections"
      className="flex flex-col gap-0.5 w-44"
      data-testid="surfaces-section-nav"
      data-layout="sidebar"
    >
      {SURFACES_SECTIONS.map(({ label, path, end }) => (
        <NavLink
          key={path}
          to={path}
          end={end}
          className={({ isActive }) =>
            cn(
              'block px-3 py-2 rounded-md text-label-14 transition-colors',
              isActive
                ? 'bg-gray-alpha-200 text-gray-1000 font-medium'
                : 'text-gray-700 hover:text-gray-1000 hover:bg-gray-alpha-100',
            )
          }
        >
          {label}
        </NavLink>
      ))}
    </nav>
  );
}

/**
 * Shared Surfaces chrome: page title + persistent left section sidebar.
 * Nested routes render via Outlet beside the sidebar (V1.102 P2 Surfaces IA;
 * V1.128 P0 T1 left-sidebar IA).
 */
export function SurfacesLayout() {
  return (
    <div className="max-w-6xl mx-auto py-8 px-4">
      <h2 className="text-heading-24 font-semibold text-gray-1000 mb-2">
        Surfaces
      </h2>
      <p className="text-copy-16 text-gray-700 mb-4">
        Real product-surface slices — Setup wizard step card and App shell
        chrome, composed as studio-local fixtures. Each section below labels
        whether imports are App presentational extracts (
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
          @web-*
        </code>
        ) or promoted primitives (
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
          @42ch/nexus-ui
        </code>
        ) per IA guide §4.5. No daemon data, no live routing, and no
        product-page imports (
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
          pages/
        </code>{' '}
        or{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
          components/layout/
        </code>
        ). Studio-only deep links — not App Settings IA.
      </p>
      <p
        data-testid="surfaces-chronos-note"
        className="text-copy-14 text-gray-700 mb-4 max-w-prose"
      >
        Chronos chrome: cyan active affordances (sidebar bar, mode pills, setup step, focus rings)
        on warm-paper (light) and ink (dark) surfaces — tokens drive both themes.
      </p>
      <SurfaceSourceLegend />

      <div className="flex gap-8 items-start">
        <aside
          className="sticky top-16 shrink-0 border-r border-gray-alpha-200 pr-4"
          data-testid="surfaces-section-sidebar"
        >
          <SurfacesSectionNav />
        </aside>
        <div className="flex-1 min-w-0">
          <Outlet />
        </div>
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Fixture — App shell sidebar chrome                                  */
/* ------------------------------------------------------------------ */

function FixtureFooterProfiles() {
  const [activeId, setActiveId] = useState('local-creator');
  const [focusIndex, setFocusIndex] = useState(0);
  const itemRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const addRef = useRef<HTMLButtonElement | null>(null);

  const profiles = [
    {
      id: 'local-creator',
      displayName: 'Local Creator',
      active: activeId === 'local-creator',
    },
  ];
  const total = profiles.length + 1;

  function focusAt(index: number) {
    const next = Math.max(0, Math.min(total - 1, index));
    const el = next === profiles.length ? addRef.current : itemRefs.current[next];
    el?.focus();
    setFocusIndex(next);
  }

  function handleKeyDown(event: React.KeyboardEvent<HTMLDivElement>) {
    switch (event.key) {
      case 'ArrowRight':
        event.preventDefault();
        focusAt(focusIndex + 1);
        break;
      case 'ArrowLeft':
        event.preventDefault();
        focusAt(focusIndex - 1);
        break;
      case 'Home':
        event.preventDefault();
        focusAt(0);
        break;
      case 'End':
        event.preventDefault();
        focusAt(total - 1);
        break;
      case 'Escape':
        event.preventDefault();
        if (focusIndex === profiles.length) {
          addRef.current?.blur();
        } else {
          itemRefs.current[focusIndex]?.blur();
        }
        break;
      default:
        break;
    }
  }

  return (
    <FooterProfilesChrome
      sectionLabel="Profiles"
      addButtonLabel="Add profile"
      profiles={profiles}
      activeDisplayName="Local Creator"
      focusIndex={focusIndex}
      onSelect={setActiveId}
      onAdd={() => {}}
      onFocus={setFocusIndex}
      onKeyDown={handleKeyDown}
      onItemRef={(index, el) => {
        itemRefs.current[index] = el;
      }}
      onAddRef={(el) => {
        addRef.current = el;
      }}
    />
  );
}

function ShellSidebarFixture() {
  const [activeTab, setActiveTab] = useState<ShellSidebarTab>('creator');
  const groups = activeTab === 'creator' ? CREATOR_NAV : ORCHESTRATOR_NAV;

  return (
    <div
      className="flex min-h-[440px] border border-gray-alpha-300 rounded-card bg-background-100 overflow-hidden"
      data-testid="app-shell-fixture"
    >
      <div className="w-sidebar-nav-width shrink-0">
        <ShellSidebarChrome
          activeTab={activeTab}
          activeRoute="#works"
          navGroups={groups}
          onTabChange={setActiveTab}
          logo={<StudioShellLogo />}
          footer={<FixtureFooterProfiles />}
        />
      </div>

      {/* Main content area — V1.128 P2 shows Create page when no entity selected */}
      <div className="flex-1 bg-background-200 flex flex-col items-center justify-center min-w-0 p-8">
        <div
          className="w-full max-w-lg rounded-card border border-gray-alpha-300 bg-background-100 p-6"
          data-testid="app-shell-content-create"
        >
          <p className="text-label-14 text-gray-900 mb-4">Create page (empty selection)</p>
          <p className="text-copy-13 text-gray-700 mb-4">
            Card CTAs for World / Work — see Creator shell fixtures below for
            Controller stub + interactive toggle.
          </p>
        </div>
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Fixtures — Footer profiles + header health (V1.107 T15)             */
/* ------------------------------------------------------------------ */

function FooterProfilesFixture() {
  const states: { label: string; profiles: { id: string; displayName: string; active?: boolean }[] }[] = [
    { label: '0 profiles', profiles: [] },
    { label: '1 profile', profiles: [{ id: 'local', displayName: 'Local Creator', active: true }] },
    {
      label: 'N profiles',
      profiles: [
        { id: 'personal', displayName: 'Personal', active: false },
        { id: 'work', displayName: 'Work', active: true },
        { id: 'client', displayName: 'Client A', active: false },
      ],
    },
  ];

  return (
    <div className="grid grid-cols-1 sm:grid-cols-3 gap-4" data-testid="footer-profiles-fixture">
      {states.map(({ label, profiles }) => (
        <div
          key={label}
          className="border border-gray-alpha-300 rounded-card bg-background-100 p-4"
          data-testid={`footer-profiles-${label.replace(/\s+/g, '-')}`}
        >
          <p className="text-label-14 text-gray-900 mb-3">{label}</p>
          <FooterProfilesChrome
            sectionLabel="Profiles"
            addButtonLabel="Add profile"
            profiles={profiles}
            focusIndex={0}
            onSelect={() => {}}
            onAdd={() => {}}
            onFocus={() => {}}
            onKeyDown={() => {}}
            onItemRef={() => {}}
            onAddRef={() => {}}
          />
        </div>
      ))}
    </div>
  );
}

function DaemonHealthIndicatorFixture() {
  type HealthState = { kind: 'unknown' } | { kind: 'connected'; version: string } | { kind: 'offline'; message: string };

  const states: { label: string; state: HealthState; isRemote?: boolean }[] = [
    { label: 'Unknown', state: { kind: 'unknown' } },
    { label: 'Connected (local)', state: { kind: 'connected', version: '1.2.3' } },
    { label: 'Connected (remote)', state: { kind: 'connected', version: '1.2.3' }, isRemote: true },
    { label: 'Offline', state: { kind: 'offline', message: 'Cannot reach local daemon' } },
  ];

  return (
    <div className="grid grid-cols-1 sm:grid-cols-2 gap-4" data-testid="daemon-health-indicator-fixture">
      {states.map(({ label, state, isRemote }) => (
        <div
          key={label}
          className="border border-gray-alpha-300 rounded-card bg-background-100 p-4"
          data-testid={`daemon-health-${label.toLowerCase().replace(/[\s()]+/g, '-')}`}
        >
          <p className="text-label-14 text-gray-900 mb-3">{label}</p>
          <DaemonHealthIndicatorChrome state={state} isRemote={isRemote} />
        </div>
      ))}
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Fixture — Daemon status strip (healthy sample)                      */
/* ------------------------------------------------------------------ */

/** Single-line footer strip — left status + soft Badge; right Restart (V1.102). */
function DaemonStatusStrip() {
  return (
    <div
      className="flex items-center justify-between gap-3 border border-gray-alpha-300 rounded-card bg-background-100 px-4 py-2"
      data-testid="daemon-status-strip"
    >
      <div className="flex min-w-0 items-center gap-2">
        <span className="h-2 w-2 shrink-0 rounded-full bg-green-700" aria-hidden />
        <span className="truncate text-label-14 text-gray-1000">Daemon running</span>
        <Badge variant="running" tone="soft">
          healthy
        </Badge>
      </div>
      <Button variant="tertiary" size="small" type="button" aria-label="Restart daemon">
        Restart
      </Button>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Section pages                                                       */
/* ------------------------------------------------------------------ */

export function SurfacesIndexPage() {
  return (
    <div data-testid="surfaces-index">
      <p className="text-copy-14 text-gray-700 mb-6">
        Jump to a Surfaces section for focused chrome review. Deep links are
        Studio-only — they are not App Settings IA.
      </p>
      <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
        {SURFACES_SECTIONS.filter((s) => s.path !== '/surfaces').map(
          ({ label, path, desc }) => (
            <NavLink
              key={path}
              to={path}
              className="block p-4 rounded-lg border border-gray-alpha-200 hover:border-gray-alpha-400 transition-colors no-underline"
            >
              <h3 className="text-heading-16 font-medium mb-1 text-gray-1000">
                {label}
              </h3>
              <p className="text-copy-14 text-gray-700">{desc}</p>
            </NavLink>
          ),
        )}
      </div>
      <p className="text-copy-13 text-gray-500 mt-12 pt-8 border-t border-gray-alpha-200">
        Surface fixtures: Setup wizard chrome, App shell chrome, AgentPicker
        states, Settings shell chrome (under Shell), daemon status strip. Composed
        from{' '}
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

export function SurfacesSetupPage() {
  return (
    <section data-testid="surfaces-setup">
      <SurfaceHeading>Setup — Wizard chrome</SurfaceHeading>
      <SurfaceSourceBadges
        importPaths={[
          '@web-setup/top-step-indicator',
          '@web-setup/agent-picker',
          '@web-setup/workspace-path-field',
          '@42ch/nexus-ui',
        ]}
      />
      <p className="text-copy-14 text-gray-700 mb-6">
        Studio-local chrome fixtures for V1.105 P2 portrait shell: fixed
        480×min(720px, 85vh) card, top horizontal Steps (Agent / Workspace /
        Done), scrollable agent-list overflow, normative Back+Continue CTA.
        Tokens from{' '}
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
  );
}

export function SurfacesShellPage() {
  return (
    <div data-testid="surfaces-shell">
      <section data-testid="surfaces-chronos-titlebar">
        <SurfaceHeading>Chronos titlebar</SurfaceHeading>
        <SurfaceSourceBadges
          importPaths={['@web-layout/chronos-titlebar-chrome', '@42ch/nexus-ui']}
        />
        <p className="text-copy-14 text-gray-700 mb-6">
          Full-width ink titlebar with bright mark on ink, theme-aware labels,
          gear/settings slot, and optional macOS traffic-light safe inset —
          presentational extract via{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            @web-layout/chronos-titlebar-chrome
          </code>
          .
        </p>
        <ChronosTitlebarFixtures />
      </section>

      <section className="mt-10">
        <SurfaceHeading>App shell chrome</SurfaceHeading>
        <SurfaceSourceBadges
          importPaths={[
            '@web-layout/shell-sidebar-chrome',
            '@42ch/nexus-ui',
          ]}
        />
        <p className="text-copy-14 text-gray-700 mb-6">
          Sidebar tab strip (Creator / Orchestrator), nav groups, and Settings
          footer utility — rendered by the presentational{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            @web-layout/shell-sidebar-chrome
          </code>{' '}
          extract. Active nav bar and mode pills use cyan signal; surfaces are
          warm-paper / ink via background tokens. No live routing, no{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            NexusClient
          </code>
          , and no direct layout component imports.
        </p>
        <ShellSidebarFixture />
      </section>

      <section className="mt-10" data-testid="surfaces-creator-shell">
        <SurfaceHeading>Creator shell — Create vs Controller</SurfaceHeading>
        <SurfaceSourceBadges
          importPaths={[
            '@web-layout/creator-shell-content',
            '@web-layout/shell-sidebar-chrome',
            '@42ch/nexus-ui',
          ]}
        />
        <p className="text-copy-14 text-gray-700 mb-6">
          V1.128 P2 shell content modes via{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            @web-layout/creator-shell-content
          </code>
          . Toggle empty Create (honest{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            createWorld
          </code>{' '}
          detect + Work fallback) vs selected Controller stub (placeholder +
          Back). Worlds-first nav data matches App sidebar IA. No App context,
          no daemon client.
        </p>
        <CreatorShellFixtures />
      </section>

      {/* Settings shell chrome stays discoverable under Shell (V1.103 P0) */}
      <section className="mt-10">
        <SurfaceHeading>Settings — Shell chrome</SurfaceHeading>
        <SurfaceSourceBadges
          importPaths={[
            '@web-settings/connect-daemon-form-chrome',
            '@web-settings/settings-setup-section-chrome',
            '@web-layout/shell-sidebar-chrome',
            '@web-layout/footer-profiles-chrome',
            '@web-setup/workspace-path-field',
            '@web-ui/dialog', // transitional — badge path label (not an import)
          ]}
        />
        <p className="text-copy-14 text-gray-700 mb-6">
          Studio-local chrome for DF-70 Settings shell: footer utility{' '}
          <strong className="font-medium text-gray-1000">Settings</strong>{' '}
          (lucide) above profiles, plus section nav (
          <strong className="font-medium text-gray-1000">Agent</strong> /{' '}
          <strong className="font-medium text-gray-1000">Connection</strong> /{' '}
          <strong className="font-medium text-gray-1000">Setup</strong> /{' '}
          <strong className="font-medium text-gray-1000">Workspace</strong>). Default
          Agent outlet shows the preselected Agent section body (G1 visual);
          Connection outlet shows Connection section chrome (locked helper +
          form placeholder); Setup outlet shows Re-run Setup section chrome
          (helper + confirm dialog); Workspace outlet shows Workspace section
          chrome (path display + Change Folder action + post-persist honesty
          copy). No App IPC, no daemon, no product{' '}
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

      <section className="mt-10">
        <SurfaceHeading>Footer profiles</SurfaceHeading>
        <SurfaceSourceBadges
          importPaths={['@web-layout/footer-profiles-chrome']}
        />
        <p className="text-copy-14 text-gray-700 mb-6">
          Profile switcher chrome from{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            @web-layout/footer-profiles-chrome
          </code>{' '}
          — props-driven 0/1/N states. No creator context, no daemon client.
        </p>
        <FooterProfilesFixture />
      </section>

      <section className="mt-10">
        <SurfaceHeading>Header health indicator</SurfaceHeading>
        <SurfaceSourceBadges
          importPaths={['@web-layout/daemon-health-indicator-chrome']}
        />
        <p className="text-copy-14 text-gray-700 mb-6">
          Daemon health indicator chrome from{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            @web-layout/daemon-health-indicator-chrome
          </code>{' '}
          — unknown, connected (local/remote), and offline states. No polling,
          no daemon client.
        </p>
        <DaemonHealthIndicatorFixture />
      </section>
    </div>
  );
}

export function SurfacesAgentPickerPage() {
  return (
    <section className="mt-0" data-testid="surfaces-agent-picker">
      <SurfaceHeading>Setup — AgentPicker</SurfaceHeading>
      <SurfaceSourceBadges importPaths={['@web-setup/agent-picker']} />
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
  );
}

export function SurfacesDaemonPage() {
  return (
    <section data-testid="surfaces-daemon">
      <SurfaceHeading>Daemon status strip</SurfaceHeading>
      <SurfaceSourceBadges importPaths={['@42ch/nexus-ui']} />
      <p className="text-copy-14 text-gray-700 mb-6">
        Healthy daemon status affordance — green dot, badge, helper text.
        Composed from{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
          @42ch/nexus-ui
        </code>{' '}
        with inline markup. Per DESIGN.md{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
          components.daemon-status-indicator
        </code>{' '}
        tokens.
      </p>
      <DaemonStatusStrip />
    </section>
  );
}

export function SurfacesLaunchPage() {
  return (
    <section data-testid="surfaces-launch">
      <SurfaceHeading>Launch — Daemon splash</SurfaceHeading>
      <SurfaceSourceBadges
        importPaths={['@web-setup/daemon-ready-splash']}
      />
      <p className="text-copy-14 text-gray-700 mb-6">
        Presentational desktop launch splash from{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
          @web-setup/daemon-ready-splash
        </code>
        . Props-driven variants: waiting, error + Restart Nexus, and error +
        Reset local database. No daemon IPC, no Tauri commands.
      </p>
      <LaunchDaemonFixtures />
    </section>
  );
}

export function SurfacesSelectionSubmenuPage() {
  return (
    <section data-testid="surfaces-selection-submenu">
      <SurfaceHeading>Selection Submenu — 6 variants (V1.126 P0 T4)</SurfaceHeading>
      <SurfaceSourceBadges
        importPaths={['@web-shell/selection-submenu']}
      />
      <p className="text-copy-14 text-gray-700 mb-6">
        Full fixture matrix for the selection submenu presentational component
        from{' '}
        <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
          @web-shell/selection-submenu
        </code>
        . All six variants cover World/Work rows, light/dark themes, rename in
        progress, and the agent dialog overlay. The delete variant is
        intentionally omitted — deferred per R-V1126P0-T2-001.
      </p>

      <div className="mb-4 rounded-card border border-gray-alpha-200 bg-background-100 p-3">
        <p className="text-label-14 font-medium text-gray-1000 mb-2">Legend</p>
        <ul className="flex flex-col gap-1 text-copy-13 text-gray-700">
          <li><strong className="text-gray-1000">1–2:</strong> World row (KB item) + submenu open, light / dark</li>
          <li><strong className="text-gray-1000">3–4:</strong> Work row (Outline item) + submenu open, light / dark</li>
          <li><strong className="text-gray-1000">5:</strong> Rename in progress — inline edit active with blue focus ring</li>
          <li><strong className="text-gray-1000">6:</strong> Agent dialog overlay — submenu closed, AgentPicker dialog open with entity-name title</li>
        </ul>
      </div>

      <SelectionSubmenuStubFixtures />
    </section>
  );
}

export function SurfacesCanvasPage() {
  return (
    <div data-testid="surfaces-canvas">
      <section>
        <SurfaceHeading>Canvas — Three mirrored surfaces + shared chrome</SurfaceHeading>
        <SurfaceSourceBadges
          importPaths={['@web-canvas/node-chrome-shell']}
        />
        <p className="text-copy-14 text-gray-700 mb-6">
          Presentational preview of the canvas surface chrome. All three App
          canvas surfaces are mirrored as static markup using the same tokens
          shared via{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            @nexus/design-tokens
          </code>
          :{' '}
          <strong className="font-medium text-gray-1000">Outline</strong> (Volume
          / Chapter / Timeline Event / Scene / Beat node kinds, mirroring{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            outline-nodes.tsx
          </code>{' '}
          +{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            scene-beat-nodes.tsx
          </code>
          ),{' '}
          <strong className="font-medium text-gray-1000">Strategy</strong>{' '}
          (state-machine states, join, terminal, labeled transitions, inspector,
          and validation panel, mirroring{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            strategy-nodes.tsx
          </code>{' '}
          +{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            strategy-canvas/*
          </code>
          ), and{' '}
          <strong className="font-medium text-gray-1000">World KB</strong>{' '}
          (entity cards, source-anchor provenance, typed relationship edges, and
          relationship inspector, mirroring{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            world-kb/*
          </code>
          ). The shared shell chrome — dot-grid background, zoom controls,
          minimap swatch — and right-click context menu matrices are common to all
          three, painted from{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            canvas-outline-*
          </code>
          ,{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            canvas-node-*
          </code>
          ,{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            canvas-strategy-accent
          </code>
          , and{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            canvas-worldkb-*
          </code>{' '}
          tokens. No{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            @xyflow/react
          </code>
          , no{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            NexusClient
          </code>
          , no{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            @42ch/nexus-contracts
          </code>
          — light/dark acceptance here carries to the App graph.
        </p>
        <CanvasSurfacesFixtures />
      </section>

      {/* V1.128 P1 T1 — NLE multi-track Timeline band */}
      <section className="mt-10" data-testid="surfaces-nle-timeline">
        <SurfaceHeading>NLE Timeline</SurfaceHeading>
        <SurfaceSourceBadges
          importPaths={['@web-canvas/nle-timeline-chrome']}
        />
        <p className="text-copy-14 text-gray-700 mb-6">
          NLE-style multi-track Timeline band for dogfood visual acceptance —
          vertically centered in the canvas host, horizontally scrubbable along
          the time axis, with labeled Brief / Narrative / Moment lanes.
          Composes{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            @web-canvas/nle-timeline-chrome
          </code>{' '}
          (same extract App Timeline hosts adopt in T3). Pan the scroll region
          to scrub across eras, events, and scenes. Layer accents use existing{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            canvas-layer-*-accent
          </code>{' '}
          tokens — no{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            @xyflow/react
          </code>
          , no contracts, no i18n. The pull-off fixture (T2) detaches one item
          from a track onto the canvas area using local fixture state only — it
          does not ship in App Timeline. Card-matrix node chrome samples remain
          below for per-kind regression.
        </p>
        <NleTimelineCanvasFixtures />
      </section>

      {/* V1.124 P0 T3 — World Timeline node chrome (Brief-era / Event / KeyBlock) */}
      <section className="mt-10" data-testid="surfaces-world-timeline">
        <SurfaceHeading>World Timeline</SurfaceHeading>
        <SurfaceSourceBadges
          importPaths={[
            '@web-canvas/node-chrome-shell',
            '@web-canvas/timeline-node-chrome',
          ]}
        />
        <p className="text-copy-14 text-gray-700 mb-6">
          World Timeline node chrome for visual acceptance without the daemon —
          Brief-era markers, Narrative Events, and KeyBlock Context cluster
          cards. Composes the shared extracts{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            @web-canvas/node-chrome-shell
          </code>{' '}
          +{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            @web-canvas/timeline-node-chrome
          </code>{' '}
          (same modules App RF wrappers use). Surface spine is{' '}
          <strong className="font-medium text-gray-1000">worldkb</strong>; layer
          accents are Brief (
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            canvas-layer-brief-accent
          </code>
          ) and Narrative (
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            canvas-layer-narrative-accent
          </code>
          ). V1.126 P1 adds a layer-differentiated directed center axis:
           Brief = thick era-spanning arrow (
           <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
             canvas-layer-brief-accent
           </code>
           ), Narrative = thin discrete event-pin axis (
           <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
             canvas-layer-narrative-accent
           </code>
           ). Each layer has a distinct visual rhythm — not just token-color-different
           (ND-7). Static English product vocabulary only — no{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            @xyflow/react
          </code>
          , no contracts, no i18n. Layer breadcrumb fixtures are below (V1.124
          P2).
        </p>
        <TimelineCanvasFixtures />
      </section>

      {/* V1.124 P0 T4 — Work Timeline node chrome (Narrative + Moment scene + beat) */}
      <section className="mt-10" data-testid="surfaces-work-timeline">
        <SurfaceHeading>Work Timeline</SurfaceHeading>
        <SurfaceSourceBadges
          importPaths={[
            '@web-canvas/node-chrome-shell',
            '@web-canvas/timeline-node-chrome',
          ]}
        />
        <p className="text-copy-14 text-gray-700 mb-6">
          Work Timeline node chrome for visual acceptance without the daemon —
          Narrative events plus Moment scene and beat cards (Moment = scene +
          beat; both required). Composes the shared extracts{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            @web-canvas/node-chrome-shell
          </code>{' '}
          +{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            @web-canvas/timeline-node-chrome
          </code>{' '}
          (same modules App RF wrappers use). Narrative spine is{' '}
          <strong className="font-medium text-gray-1000">worldkb</strong> with
          layer accent{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            canvas-layer-narrative-accent
          </code>
          ; Moment scene/beat spine is{' '}
          <strong className="font-medium text-gray-1000">outline</strong> with
          layer accent{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            canvas-layer-moment-accent
          </code>
. V1.126 P1 adds the Moment layer directed center axis: chapter-scoped micro-segments (
           <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
             canvas-layer-moment-accent
           </code>
           ) with density-encoding per ND-A1 (segment length ∝ scene count). Narrative
           layer also gets the discrete pin axis treatment (cross-surface consistency
           with World Timeline). Static English product vocabulary only — no{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            @xyflow/react
          </code>
          ,           no contracts, no i18n. Layer breadcrumb and Global Timeline overview
          fixtures are below (V1.124 P2).
        </p>
        <WorkTimelineCanvasFixtures />
      </section>

      {/* V1.124 P2 T2 — Global Timeline list chrome */}
      <section className="mt-10" data-testid="surfaces-global-timeline">
        <SurfaceHeading>Global Timeline</SurfaceHeading>
        <SurfaceSourceBadges
          importPaths={['@web-global-timeline/global-timeline-list-chrome']}
        />
        <p className="text-copy-14 text-gray-700 mb-6">
          Cross-World Timeline activity list chrome for visual acceptance
          without the daemon. Composes{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            @web-global-timeline/global-timeline-list-chrome
          </code>{' '}
          (same extract App{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            GlobalTimelineView
          </code>{' '}
          uses after mapping hooks → row props). Frames: populated (≥3 Worlds),
          empty, loading, error. Static English product vocabulary — no daemon,
          no contracts, no router, no i18n.
        </p>
        <GlobalTimelineFixtures />
      </section>

      {/* V1.124 P2 T3a — Layer breadcrumb */}
      <section className="mt-10" data-testid="surfaces-layer-breadcrumb">
        <SurfaceHeading>Layer Breadcrumb</SurfaceHeading>
        <SurfaceSourceBadges
          importPaths={['@web-canvas/layer-breadcrumb']}
        />
        <p className="text-copy-14 text-gray-700 mb-6">
          Three-layer zoom affordance used on World Timeline (Brief ↔ Narrative)
          and Work Timeline (Narrative ↔ Moment). Composes{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            @web-canvas/layer-breadcrumb
          </code>
          . Parent segment is a focusable zoom-out button; active segment uses{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            aria-current=&quot;page&quot;
          </code>
          . Static English labels — no RF, no i18n.
        </p>
        <LayerBreadcrumbFixtures />
      </section>

      {/* V1.124 P2 T3b — Conflict-modal shared chrome */}
      <section className="mt-10" data-testid="surfaces-conflict-modals">
        <SurfaceHeading>Conflict Modals</SurfaceHeading>
        <SurfaceSourceBadges
          importPaths={['@web-canvas/conflict-modal-chrome']}
        />
        <p className="text-copy-14 text-gray-700 mb-6">
          One shared conflict-resolution shell used by Strategy, Outline, and
          World KB adapters. Composes{' '}
          <code className="text-copy-13-mono bg-gray-alpha-100 px-1 rounded">
            @web-canvas/conflict-modal-chrome
          </code>
          . Frames: always-open gallery preview, resolve path (Reapply enabled),
          overlap path (Reapply disabled). Domain field mapping stays App-only —
          not three parallel modal UIs.
        </p>
        <ConflictModalFixtures />
      </section>
    </div>
  );
}
