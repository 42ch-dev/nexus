/**
 * Studio fixtures for Settings shell chrome (V1.103) + Agent (P1) +
 * Connection (P2) + Setup (P3) + Workspace (V1.104 P0) section bodies.
 *
 * Studio-local shell + page chrome only — no apps/web pages/, layout/, hooks,
 * or daemon client. Section nav labels locked by settings-shell-ia.md.
 * Workspace nav added in V1.104 P0 (Must).
 *
 * V1.107 refactored to import presentational chrome from
 * `@web-settings/connect-daemon-form-chrome`,
 * `@web-settings/settings-setup-section-chrome`, and
 * `@web-setup/workspace-path-field` so the fixture file stays a thin gallery
 * wrapper with no duplicated app markup.
 *
 * V1.108 (FB-UI-001..003, 005) replaced the stale inline SettingsShellChromeFixture
 * (underline tabs, plain nav, profile-name dual track) with ShellSidebarChrome +
 * FooterProfilesChrome SSOT via `@web-layout/*` — same component tree as the App
 * shell and surfaces.tsx ShellSidebarFixture. Profiles are icon-only (no
 * `activeDisplayName`).
 */

import { useState, type ReactNode } from 'react';

import {
  Bot,
  FolderOpen,
  RotateCcw,
  Settings,
  Wifi,
  type LucideIcon,
} from 'lucide-react';
import {
  Button,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  cn,
} from '@42ch/nexus-ui';

import { StudioShellLogo } from '@/components/studio-shell-logo';

import { Dialog, DialogContent } from '@web-ui/dialog'; // transitional — keep-web (Radix portal/focus-trap beyond presentational scope)

import { WorkspacePathField } from '@web-setup/workspace-path-field';
import {
  AgentPicker,
  type AgentPickerItem,
} from '@web-setup/agent-picker';
import {
  ShellSidebarChrome,
  type ShellSidebarTab,
} from '@web-layout/shell-sidebar-chrome';
import { FooterProfilesChrome } from '@web-layout/footer-profiles-chrome';
import { ConnectDaemonFormChrome } from '@web-settings/connect-daemon-form-chrome';
import {
  SettingsSetupSectionChrome,
  SettingsSetupConfirmChromeStatic,
} from '@web-settings/settings-setup-section-chrome';

import {
  CREATOR_NAV,
  ORCHESTRATOR_NAV,
} from '@/fixtures/shell-nav-data';

/** Three-tab top nav — Agent / Workspace / Advanced (V1.106 P2). */
export type SettingsNavSectionId = 'agent' | 'workspace' | 'advanced';

/** Section body IDs for chrome fixtures; Connection and Setup live inside Advanced. */
export type SettingsBodySectionId = 'agent' | 'connection' | 'setup' | 'workspace';

const SETTINGS_NAV_SECTIONS: {
  id: SettingsNavSectionId;
  label: string;
  icon: LucideIcon;
}[] = [
  { id: 'agent', label: 'Agent', icon: Bot },
  { id: 'workspace', label: 'Workspace', icon: FolderOpen },
  { id: 'advanced', label: 'Advanced', icon: Settings },
];

const SETTINGS_SECTIONS: {
  id: SettingsBodySectionId;
  label: string;
  icon: LucideIcon;
  emptyHint: string;
}[] = [
  {
    id: 'agent',
    label: 'Agent',
    icon: Bot,
    emptyHint: 'Agent section body mounts in the shell outlet (see Agent section fixture).',
  },
  {
    id: 'connection',
    label: 'Connection',
    icon: Wifi,
    emptyHint:
      'Connection section body mounts in the shell outlet (see Connection section fixture).',
  },
  {
    id: 'setup',
    label: 'Setup',
    icon: RotateCcw,
    emptyHint:
      'Setup section body mounts in the shell outlet (see Setup section fixture).',
  },
  {
    id: 'workspace',
    label: 'Workspace',
    icon: FolderOpen,
    emptyHint:
      'Workspace section body mounts in the shell outlet (see Workspace section fixture).',
  },
];

const SHELL_HELPER =
  'Manage your local agent, workspace, daemon connection, and setup options from one place.';

// CREATOR_NAV / ORCHESTRATOR_NAV imported from shell-nav-data.ts (V1.109 P2,
// R-V1108P1QC1-S001) — shared with surfaces.tsx ShellSidebarFixture.

/** Locked by settings-agent-section.md — section body helper (sentence case). */
const AGENT_SECTION_HELPER =
  'Choose which local ACP agent Nexus uses for creative work.';

/** Locked by settings-workspace-section.md — section body helper (sentence case). */
const WORKSPACE_SECTION_HELPER =
  'View or change where Nexus stores your creative files on this machine.';

const WORKSPACE_POST_PERSIST_SUCCESS =
  'Workspace path saved. Restart or reload the app so the running daemon uses the new location.';

/** Copy-only label — no wired app restart orchestration. */
const WORKSPACE_RESTART_LABEL = 'Quit and reopen Nexus';

const SETUP_CONFIRM_TITLE = 'Re-run Setup?';

const SETUP_CONFIRM_BODY =
  'This restarts the setup wizard from the beginning. Your workspace path and agent profile are not deleted.';

/** Fixture-only sample paths — visual chrome, not live workspace state. */
const FIXTURE_WORKSPACE_PATH = '/Users/creator/Documents/Nexus';
const FIXTURE_WORKSPACE_PATH_UPDATED = '/Volumes/Studio/Nexus';

/**
 * Preselected saved-profile id for the Agent section fixture.
 * Codex (not first-installed Claude) so the visual reads as G1 preselect,
 * not the V1.102 first-installed default.
 */
const PRESELECTED_AGENT_ID = 'codex-native';

const FIXTURE_AGENTS: AgentPickerItem[] = [
  {
    id: 'claude-native',
    name: 'claude (native CLI)',
    displayName: 'Claude',
    version: 'claude 1.0.42',
    description: "Anthropic's agent for local coding with Claude.",
    installed: true,
    installUrl: 'https://docs.anthropic.com/en/docs/claude-code',
    docsUrl: 'https://docs.anthropic.com/en/docs/claude-code',
  },
  {
    id: 'codex-native',
    name: 'codex (native CLI)',
    displayName: 'Codex',
    version: 'codex 0.12.0',
    description: "OpenAI's agent for local coding with Codex.",
    installed: true,
    installUrl: 'https://openai.com/codex/',
    docsUrl: null,
  },
  {
    id: 'kimi',
    name: 'Gemini CLI',
    version: null,
    description: 'Google Gemini agent (not on PATH).',
    installed: false,
    installUrl: 'https://github.com/google-gemini/gemini-cli',
    docsUrl: 'https://ai.google.dev/',
  },
];

/**
 * Secondary section nav inside Settings page chrome.
 * Not Creator/Orchestrator tabs; not a second app-wide sidebar.
 * V1.106 P2: three tabs — Agent / Workspace / Advanced.
 */
function SettingsSectionNav({
  active,
  onSelect,
}: {
  active: SettingsNavSectionId;
  onSelect: (id: SettingsNavSectionId) => void;
}) {
  return (
    <nav
      aria-label="Settings sections"
      className="flex flex-wrap gap-1 border-b border-gray-alpha-200 pb-px"
      data-testid="settings-section-nav"
    >
      {SETTINGS_NAV_SECTIONS.map(({ id, label, icon: Icon }) => {
        const selected = active === id;
        return (
          <button
            key={id}
            type="button"
            onClick={() => onSelect(id)}
            aria-current={selected ? 'page' : undefined}
            data-testid={`settings-section-nav-${id}`}
            className={cn(
              'inline-flex items-center gap-2 px-3 py-2 text-label-14 font-medium',
              'border-b-2 -mb-px transition-colors',
              selected
                ? 'text-gray-1000 border-blue-700'
                : 'text-gray-700 border-transparent hover:text-gray-1000 hover:border-gray-alpha-400',
            )}
          >
            <Icon className="size-4 shrink-0" aria-hidden="true" />
            <span>{label}</span>
          </button>
        );
      })}
    </nav>
  );
}

/** Empty section outlet frame — P0 scaffold before section body product work. */
function SettingsEmptySectionFrame({
  sectionId,
}: {
  sectionId: SettingsBodySectionId;
}) {
  const section = SETTINGS_SECTIONS.find((s) => s.id === sectionId);
  if (!section) return null;
  const Icon = section.icon;
  return (
    <div
      className={cn(
        'flex flex-col items-center justify-center gap-3 min-h-[200px]',
        'rounded-card border border-dashed border-gray-alpha-400',
        'bg-background-100 px-6 py-10 text-center',
      )}
      data-testid={`settings-section-frame-${sectionId}`}
      data-section={sectionId}
    >
      <Icon className="size-8 text-gray-500" aria-hidden="true" />
      <p className="text-heading-16 font-heading text-gray-1000">
        {section.label}
      </p>
      <p className="text-copy-13 text-gray-700 max-w-sm">{section.emptyHint}</p>
    </div>
  );
}

/**
 * Settings shell page chrome: title + helper + section nav + outlet region.
 * Matches settings-shell-ia.md author-facing copy (DESIGN Voice).
 */
function SettingsShellPageChrome({
  activeSection,
  onSectionChange,
  children,
}: {
  activeSection: SettingsNavSectionId;
  onSectionChange: (id: SettingsNavSectionId) => void;
  children: ReactNode;
}) {
  return (
    <div
      className="flex flex-col gap-6 max-w-2xl w-full"
      data-testid="settings-shell-page-chrome"
    >
      <div className="flex flex-col gap-2">
        <h2 className="text-heading-24 font-heading text-gray-1000">Settings</h2>
        <p className="text-copy-14 text-gray-900">{SHELL_HELPER}</p>
      </div>
      <SettingsSectionNav
        active={activeSection}
        onSelect={onSectionChange}
      />
      <div data-testid="settings-shell-outlet">{children}</div>
    </div>
  );
}

/**
 * V1.102 thin host page chrome (AgentPicker region) — retained for P1
 * Agent section body visual reference.
 */
function SettingsHostPageChrome({ children }: { children: ReactNode }) {
  return (
    <div
      className="flex flex-col gap-6 max-w-2xl w-full"
      data-testid="settings-host-page-chrome"
    >
      <div className="flex flex-col gap-2">
        <h2 className="text-heading-24 font-heading text-gray-1000">Settings</h2>
        <p className="text-copy-14 text-gray-900">
          Change the local agent Nexus uses after setup. Select a discovered
          agent or provide a custom launch command.
        </p>
      </div>
      <div data-testid="settings-host-picker-region">{children}</div>
    </div>
  );
}

function InteractiveSettingsPicker({
  initialSelectedId = 'claude-native',
}: {
  initialSelectedId?: string | null;
}) {
  const [selectedId, setSelectedId] = useState<string | null>(initialSelectedId);
  const [custom, setCustom] = useState('');
  return (
    <AgentPicker
      status="ready"
      defaultGrid={FIXTURE_AGENTS}
      selectedId={selectedId}
      onSelect={setSelectedId}
      customLaunchValue={custom}
      onCustomLaunchChange={setCustom}
    />
  );
}

/**
 * Agent section body chrome — mirrors apps/web SettingsAgentSection layout
 * (helper + picker; instant-apply on select) without scan/IPC.
 */
function SettingsAgentSectionChrome({
  initialSelectedId = PRESELECTED_AGENT_ID,
}: {
  initialSelectedId?: string | null;
}) {
  const [selectedId, setSelectedId] = useState<string | null>(initialSelectedId);
  const [custom, setCustom] = useState('');

  return (
    <div
      className="flex flex-col gap-6"
      data-testid="settings-agent-section"
      data-preselected={initialSelectedId ?? undefined}
    >
      <div className="flex flex-col gap-2">
        <h3 className="text-heading-16 font-heading text-gray-1000">Agent</h3>
        <p className="text-copy-14 text-gray-900">{AGENT_SECTION_HELPER}</p>
      </div>
      <div data-testid="settings-host-picker-region">
        <AgentPicker
          status="ready"
          defaultGrid={FIXTURE_AGENTS}
          selectedId={selectedId}
          onSelect={setSelectedId}
          customLaunchValue={custom}
          onCustomLaunchChange={setCustom}
        />
      </div>
    </div>
  );
}

/**
 * Connection section body chrome — presentational extract from
 * `@web-settings/connect-daemon-form-chrome`.
 */
function SettingsConnectionSectionChrome() {
  return (
    <div
      className="flex flex-col gap-6"
      data-testid="settings-connection-section"
      id="connection"
    >
      <div className="flex flex-col gap-2">
        <h3 className="text-heading-16 font-heading text-gray-1000">
          Connection
        </h3>
        <p className="text-copy-14 text-gray-900">
          Connect this app to a remote Nexus daemon. Your local daemon stays the
          default until you activate a remote connection.
        </p>
      </div>

      <ConnectDaemonFormChrome
        state="reconnectMatch"
        data-testid="settings-connection-form-chrome"
      />
    </div>
  );
}

/**
 * Connection four-state matrix — presentational chrome for each
 * author-visible state so Studio can accept every branch.
 */
function SettingsConnectionMatrixChrome() {
  return (
    <div
      className="grid grid-cols-1 gap-4"
      data-testid="settings-connection-matrix"
    >
      <ConnectDaemonFormChrome
        state="firstUse"
        data-testid="settings-connection-form-first-use"
      />
      <ConnectDaemonFormChrome
        state="reconnectMatch"
        data-testid="settings-connection-form-reconnect"
      />
      <ConnectDaemonFormChrome
        state="fingerprintMismatch"
        data-testid="settings-connection-form-mismatch"
      />
      <ConnectDaemonFormChrome
        state="loopbackOnly"
        data-testid="settings-connection-form-loopback"
      />
    </div>
  );
}

/**
 * Workspace section body chrome — presentational extract from
 * `@web-setup/workspace-path-field`.
 */
function SettingsWorkspaceSectionChrome({
  desktopAvailable = true,
  saved = false,
  path = FIXTURE_WORKSPACE_PATH,
}: {
  desktopAvailable?: boolean;
  saved?: boolean;
  path?: string;
}) {
  return (
    <div
      className="flex flex-col gap-6"
      data-testid="settings-workspace-section"
      data-desktop={desktopAvailable ? 'true' : 'false'}
    >
      <div className="flex flex-col gap-2">
        <h3 className="text-heading-16 font-heading text-gray-1000">Workspace</h3>
        <p className="text-copy-14 text-gray-900">{WORKSPACE_SECTION_HELPER}</p>
      </div>

      <Card className="shadow-card" data-testid="settings-workspace-card">
        <CardHeader>
          <div className="flex items-center gap-2">
            <FolderOpen
              className="h-5 w-5 text-blue-700"
              aria-hidden="true"
            />
            <CardTitle>Workspace folder</CardTitle>
          </div>
        </CardHeader>
        <CardContent className="space-y-6">
          <WorkspacePathField
            id="studio-workspace-path"
            path={path}
            desktopAvailable={desktopAvailable}
            onChangeClick={() => {}}
            inputDataTestId="settings-workspace-path"
            buttonDataTestId="settings-change-folder"
            title={
              desktopAvailable
                ? undefined
                : 'Open the Nexus desktop app to change your workspace folder.'
            }
          />

          {saved && (
            <div
              className="rounded-control border border-gray-alpha-400 bg-background-200 p-4 space-y-1"
              data-testid="settings-workspace-saved-honesty"
            >
              <p className="text-copy-14 text-gray-900">
                {WORKSPACE_POST_PERSIST_SUCCESS}
              </p>
              <p className="text-copy-13 text-gray-700">
                {WORKSPACE_RESTART_LABEL}
              </p>
            </div>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

/**
 * Setup section host — owns the Radix confirm dialog; chrome is purely
 * presentational. Browser-only variant needs no dialog (CTA is disabled).
 */
function SettingsSetupSectionHost({
  desktopAvailable = true,
  'data-testid': dataTestId,
}: {
  desktopAvailable?: boolean;
  'data-testid'?: string;
}) {
  const [confirmOpen, setConfirmOpen] = useState(false);
  return (
    <>
      <SettingsSetupSectionChrome
        desktopAvailable={desktopAvailable}
        data-testid={dataTestId}
        onReRunSetup={() => setConfirmOpen(true)}
      />
      {desktopAvailable && (
        <Dialog open={confirmOpen} onOpenChange={setConfirmOpen}>
          <DialogContent
            title={SETUP_CONFIRM_TITLE}
            description={SETUP_CONFIRM_BODY}
          >
            <div
              className="flex justify-end gap-3"
              data-testid="settings-rerun-setup-confirm"
            >
              <Button
                type="button"
                variant="secondary"
                data-testid="settings-rerun-setup-cancel"
                onClick={() => setConfirmOpen(false)}
              >
                Cancel
              </Button>
              <Button
                type="button"
                variant="destructive"
                data-testid="settings-rerun-setup-confirm-action"
                onClick={() => setConfirmOpen(false)}
              >
                Re-run
              </Button>
            </div>
          </DialogContent>
        </Dialog>
      )}
    </>
  );
}

/**
 * Advanced section body chrome — stacks Connection and Setup on one page
 * with hash anchors (V1.106 P2).
 */
function SettingsAdvancedSectionChrome() {
  return (
    <div className="flex flex-col gap-10" data-testid="settings-advanced-section">
      <SettingsConnectionSectionChrome />
      <SettingsSetupSectionHost data-testid="settings-setup-section" />
    </div>
  );
}

function InteractiveSettingsShellPage() {
  const [active, setActive] = useState<SettingsNavSectionId>('agent');
  return (
    <SettingsShellPageChrome
      activeSection={active}
      onSectionChange={setActive}
    >
      {active === 'agent' ? (
        <SettingsAgentSectionChrome />
      ) : active === 'workspace' ? (
        <SettingsWorkspaceSectionChrome />
      ) : (
        <SettingsAdvancedSectionChrome />
      )}
    </SettingsShellPageChrome>
  );
}

/**
 * Icon-only profile footer for the Settings fixture — same FooterProfilesChrome
 * SSOT as the App shell, but without `activeDisplayName` (FB-UI-001: no name
 * text under avatars in the Settings context).
 */
function SettingsFooterProfiles() {
  return (
    <FooterProfilesChrome
      sectionLabel="Profiles"
      addButtonLabel="Add profile"
      profiles={[
        { id: 'local-creator', displayName: 'Local Creator', active: true },
      ]}
      focusIndex={0}
      onSelect={() => {}}
      onAdd={() => {}}
      onFocus={() => {}}
      onKeyDown={() => {}}
      onItemRef={() => {}}
      onAddRef={() => {}}
    />
  );
}

/**
 * App shell slice with Settings footer utility active — uses ShellSidebarChrome
 * + FooterProfilesChrome SSOT (V1.108 FB-UI-001..003, 005), not stale inline
 * underline/plain-nav/profile-name markup. Same component tree as the App
 * shell and surfaces.tsx ShellSidebarFixture.
 */
function SettingsShellChromeFixture() {
  const [activeTab, setActiveTab] = useState<ShellSidebarTab>('creator');
  const groups = activeTab === 'creator' ? CREATOR_NAV : ORCHESTRATOR_NAV;

  return (
    <div
      className="flex min-h-[440px] border border-gray-alpha-300 rounded-card bg-background-100 overflow-hidden"
      data-testid="settings-shell-chrome"
    >
      <div className="w-sidebar-nav-width shrink-0">
        <ShellSidebarChrome
          activeTab={activeTab}
          activeRoute="#works"
          navGroups={groups}
          onTabChange={setActiveTab}
          logo={<StudioShellLogo />}
          footer={<SettingsFooterProfiles />}
        />
      </div>

      <div className="flex-1 bg-background-200 flex flex-col min-w-0 p-8 overflow-auto">
        <InteractiveSettingsShellPage />
      </div>
    </div>
  );
}

/**
 * Settings shell chrome + empty section frames for Studio visual acceptance.
 * V1.106 P2: section nav shows Agent / Workspace / Advanced; Connection and
 * Setup live inside Advanced.
 */
export function SettingsHostFixtures() {
  return (
    <div data-testid="settings-host-fixtures" className="space-y-8">
      <div
        className="rounded-card border border-gray-alpha-200 bg-background-100 p-4"
        data-testid="settings-host-fixture-shell"
      >
        <h4 className="text-heading-16 font-heading text-gray-1000 mb-1">
          Shell + section nav
        </h4>
        <p className="text-copy-13 text-gray-700 mb-4">
          Footer utility Settings (lucide) above profiles; main panel is the
          Settings shell — title, helper, section nav (Agent / Workspace /
          Advanced). Default Agent outlet shows the preselected Agent section
          body; Advanced outlet shows Connection and Setup stacked, and the
          Workspace outlet shows the Workspace section chrome.
        </p>
        <SettingsShellChromeFixture />
      </div>

      <div
        className="rounded-card border border-gray-alpha-200 bg-background-100 p-4"
        data-testid="settings-host-fixture-agent-section"
      >
        <h4 className="text-heading-16 font-heading text-gray-1000 mb-1">
          Agent section (preselected)
        </h4>
        <p className="text-copy-13 text-gray-700 mb-4">
          Section chrome with locked helper copy and AgentPicker (instant-apply).
          Codex starts selected to show saved-profile preselect (G1 visual) —
          props-driven only; no App IPC.
        </p>
        <div className="bg-background-200 rounded-card p-6">
          <SettingsAgentSectionChrome />
        </div>
      </div>

      <div
        className="rounded-card border border-gray-alpha-200 bg-background-100 p-4"
        data-testid="settings-host-fixture-connection-section"
      >
        <h4 className="text-heading-16 font-heading text-gray-1000 mb-1">
          Connection section
        </h4>
        <p className="text-copy-13 text-gray-700 mb-4">
          Section chrome with locked helper copy and Connect-to-Daemon form
          placeholder (URL / API key / fingerprint helpers + Title Case CTAs).
          Fixture-driven only; no App IPC.
        </p>
        <div className="bg-background-200 rounded-card p-6">
          <SettingsConnectionSectionChrome />
        </div>
      </div>

      <div
        className="rounded-card border border-gray-alpha-200 bg-background-100 p-4"
        data-testid="settings-host-fixture-connection-matrix"
      >
        <h4 className="text-heading-16 font-heading text-gray-1000 mb-1">
          Connection matrix — four states
        </h4>
        <p className="text-copy-13 text-gray-700 mb-4">
          Presentational chrome for each Connect-to-Daemon author-visible state:
          first-use TOFU, reconnect match, fingerprint mismatch, and loopback
          only. Props-driven only; no App IPC.
        </p>
        <div className="bg-background-200 rounded-card p-6">
          <SettingsConnectionMatrixChrome />
        </div>
      </div>

      <div
        className="rounded-card border border-gray-alpha-200 bg-background-100 p-4"
        data-testid="settings-host-fixture-setup-section"
      >
        <h4 className="text-heading-16 font-heading text-gray-1000 mb-1">
          Setup section
        </h4>
        <p className="text-copy-13 text-gray-700 mb-4">
          Section chrome with locked helper copy and Re-run Setup CTA. Opens
          the confirm dialog (destructive-adjacent Title Case primary).
          Fixture-driven only; no App IPC.
        </p>
        <div className="bg-background-200 rounded-card p-6">
          <SettingsSetupSectionHost data-testid="settings-setup-section" />
        </div>
      </div>

      <div
        className="rounded-card border border-gray-alpha-200 bg-background-100 p-4"
        data-testid="settings-host-fixture-setup-confirm"
      >
        <h4 className="text-heading-16 font-heading text-gray-1000 mb-1">
          Setup — confirm dialog
        </h4>
        <p className="text-copy-13 text-gray-700 mb-4">
          Static confirm dialog chrome for visual acceptance: title, body,
          Cancel, and destructive Re-run Setup. No Radix portal (avoids
          aria-hiding the Surfaces page); interactive open is on the Setup
          section CTA above.
        </p>
        <div className="bg-background-200 rounded-card p-6">
          <SettingsSetupConfirmChromeStatic />
        </div>
      </div>

      <div
        className="rounded-card border border-gray-alpha-200 bg-background-100 p-4"
        data-testid="settings-host-fixture-setup-browser"
      >
        <h4 className="text-heading-16 font-heading text-gray-1000 mb-1">
          Setup — browser-only
        </h4>
        <p className="text-copy-13 text-gray-700 mb-4">
          Honest desktop-only helper with disabled Re-run Setup CTA (optional
          tooltip). No invented HTTP setup-marker API.
        </p>
        <div className="bg-background-200 rounded-card p-6">
          <SettingsSetupSectionChrome
            desktopAvailable={false}
            data-testid="settings-setup-section"
          />
        </div>
      </div>

      <div
        className="rounded-card border border-gray-alpha-200 bg-background-100 p-4"
        data-testid="settings-host-fixture-workspace-section"
      >
        <h4 className="text-heading-16 font-heading text-gray-1000 mb-1">
          Workspace section (desktop)
        </h4>
        <p className="text-copy-13 text-gray-700 mb-4">
          Section chrome with locked helper copy, current path display, and
          enabled Change Folder CTA. Props-driven only; no App IPC.
        </p>
        <div className="bg-background-200 rounded-card p-6">
          <SettingsWorkspaceSectionChrome />
        </div>
      </div>

      <div
        className="rounded-card border border-gray-alpha-200 bg-background-100 p-4"
        data-testid="settings-host-fixture-workspace-saved"
      >
        <h4 className="text-heading-16 font-heading text-gray-1000 mb-1">
          Workspace section — post-persist
        </h4>
        <p className="text-copy-13 text-gray-700 mb-4">
          Honesty copy after persist: the running app and daemon may still use
          the previous workspace root until restart or reload. No wired restart
          orchestration.
        </p>
        <div className="bg-background-200 rounded-card p-6">
          <SettingsWorkspaceSectionChrome
            saved
            path={FIXTURE_WORKSPACE_PATH_UPDATED}
          />
        </div>
      </div>

      <div
        className="rounded-card border border-gray-alpha-200 bg-background-100 p-4"
        data-testid="settings-host-fixture-workspace-browser"
      >
        <h4 className="text-heading-16 font-heading text-gray-1000 mb-1">
          Workspace section — browser-only
        </h4>
        <p className="text-copy-13 text-gray-700 mb-4">
          Honest desktop-only helper with disabled Change Folder CTA (title
          tooltip). No invented HTTP workspace API.
        </p>
        <div className="bg-background-200 rounded-card p-6">
          <SettingsWorkspaceSectionChrome desktopAvailable={false} />
        </div>
      </div>

      <div
        className="rounded-card border border-gray-alpha-200 bg-background-100 p-4"
        data-testid="settings-host-fixture-section-frames"
      >
        <h4 className="text-heading-16 font-heading text-gray-1000 mb-1">
          Empty section frames
        </h4>
        <p className="text-copy-13 text-gray-700 mb-4">
          Static empty outlet frames for Agent / Connection / Setup / Workspace
          placeholders. Shell outlet mounts Agent, Connection, Setup, and
          Workspace bodies; empty frames remain as visual reference.
        </p>
        <div className="grid grid-cols-1 gap-4">
          {SETTINGS_SECTIONS.map(({ id }) => (
            <SettingsEmptySectionFrame key={id} sectionId={id} />
          ))}
        </div>
      </div>

      <div
        className="rounded-card border border-gray-alpha-200 bg-background-100 p-4"
        data-testid="settings-host-fixture-page-only"
      >
        <h4 className="text-heading-16 font-heading text-gray-1000 mb-1">
          Agent body reference (thin host)
        </h4>
        <p className="text-copy-13 text-gray-700 mb-4">
          V1.102 thin-host AgentPicker chrome retained as a secondary
          reference — not the P1 Agent section claim.
        </p>
        <div className="bg-background-200 rounded-card p-6">
          <SettingsHostPageChrome>
            <InteractiveSettingsPicker />
          </SettingsHostPageChrome>
        </div>
      </div>
    </div>
  );
}
