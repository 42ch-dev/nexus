/**
 * Studio fixtures for Chronos titlebar chrome (V1.131 P0 T2).
 *
 * Imports presentational extract only via `@web-layout/chronos-titlebar-chrome`.
 */
import type { ReactNode } from 'react';

import logoWhite from '@42ch/nexus-ui/assets/logos/logo-white.svg';
import { NexusLogo, logoShellHeightPx } from '@42ch/nexus-ui';
import { Moon, Settings, Sun } from 'lucide-react';

import {
  CHRONOS_TITLEBAR_DESKTOP_INSET_PX,
  ChronosTitlebarChrome,
} from '@web-layout/chronos-titlebar-chrome';
import { ShellSidebarChrome } from '@web-layout/shell-sidebar-chrome';

import { CREATOR_NAV } from '@/fixtures/shell-nav-data';

function InkLogo() {
  return (
    <NexusLogo
      variant="white"
      src={logoWhite}
      label="Nexus"
      size={logoShellHeightPx}
      className="h-5 w-auto max-w-full shrink-0"
    />
  );
}

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

function TitlebarSpecimen({
  isDark,
  desktopSafeInset,
  testId,
}: {
  isDark: boolean;
  desktopSafeInset?: boolean;
  testId: string;
}) {
  const inkControlClass = isDark
    ? 'inline-flex h-8 w-8 items-center justify-center rounded-control text-brand-cyan'
    : 'inline-flex h-8 w-8 items-center justify-center rounded-control text-white';

  return (
    <ChronosTitlebarChrome
      data-testid={testId}
      title="Works"
      isDark={isDark}
      desktopSafeInset={desktopSafeInset}
      logo={<InkLogo />}
      healthIndicator={
        <span className="rounded-pill bg-white/10 px-2 py-0.5 text-label-12 text-white">
          Daemon v0.1.0
        </span>
      }
      settingsControl={
        <button type="button" aria-label="Settings" className={inkControlClass}>
          <Settings className="h-4 w-4" aria-hidden />
        </button>
      }
      themeToggle={
        <button type="button" aria-label="Theme" className={inkControlClass}>
          {isDark ? <Sun className="h-4 w-4" aria-hidden /> : <Moon className="h-4 w-4" aria-hidden />}
        </button>
      }
    />
  );
}

function DualPaneShellFixture({
  isDark,
  desktopSafeInset,
  testId,
}: {
  isDark: boolean;
  desktopSafeInset?: boolean;
  testId: string;
}) {
  return (
    <div
      className="flex min-h-[440px] flex-col overflow-hidden rounded-card border border-gray-alpha-300 bg-background-100"
      data-testid={testId}
    >
      <TitlebarSpecimen
        isDark={isDark}
        desktopSafeInset={desktopSafeInset}
        testId={`${testId}-titlebar`}
      />
      <div className="flex min-h-0 flex-1">
        <div className="w-sidebar-nav-width shrink-0">
          <ShellSidebarChrome
            activeTab="creator"
            activeRoute="#works"
            navGroups={CREATOR_NAV}
            onTabChange={() => {}}
          />
        </div>
        <div className="flex flex-1 items-center justify-center bg-background-200 p-8">
          <p className="text-copy-14 text-gray-700">Main content</p>
        </div>
      </div>
    </div>
  );
}

export function ChronosTitlebarFixtures() {
  return (
    <div data-testid="chronos-titlebar-fixtures">
      <FixtureFrame
        title="Light + dark titlebar (browser)"
        description="White labels on ink (light shell) and cyan labels (dark shell). Logo uses bright mark on ink — no primary plate on the light sidebar."
        testId="chronos-titlebar-fixture-themes"
      >
        <div className="grid gap-4">
          <div className="rounded-card border border-gray-alpha-200 overflow-hidden">
            <TitlebarSpecimen isDark={false} testId="chronos-titlebar-light" />
          </div>
          <div className="dark rounded-card border border-gray-alpha-200 overflow-hidden">
            <TitlebarSpecimen isDark testId="chronos-titlebar-dark" />
          </div>
        </div>
      </FixtureFrame>

      <FixtureFrame
        title="Desktop safe inset + dual-pane shell"
        description={`Native traffic-light inset (${CHRONOS_TITLEBAR_DESKTOP_INSET_PX}px) with drag region on empty paint only; sidebar has no logo row.`}
        testId="chronos-titlebar-fixture-desktop-inset"
      >
        <DualPaneShellFixture
          isDark={false}
          desktopSafeInset
          testId="chronos-titlebar-dual-pane-light"
        />
      </FixtureFrame>

      <FixtureFrame
        title="Dark dual-pane with desktop inset"
        description="Cyan title labels on ink with the same inset + interactive-slot separation."
        testId="chronos-titlebar-fixture-dark-dual-pane"
      >
        <div className="dark">
          <DualPaneShellFixture
            isDark
            desktopSafeInset
            testId="chronos-titlebar-dual-pane-dark"
          />
        </div>
      </FixtureFrame>
    </div>
  );
}
