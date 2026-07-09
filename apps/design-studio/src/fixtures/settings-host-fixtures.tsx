/**
 * Studio fixtures for Settings shell chrome (V1.103) + Agent (P1) +
 * Connection (P2) + Setup (P3) section bodies.
 *
 * Studio-local shell + page chrome only — no apps/web pages/, layout/, hooks,
 * or daemon client. Section nav labels locked by settings-shell-ia.md.
 * Workspace nav is absent until P4 Stretch runs.
 *
 * P1 Agent section fixture is props-driven with a preselected agent card
 * (saved-profile visual state). P2 Connection section fixture shows locked
 * helper copy + form chrome placeholder. P3 Setup section fixture shows
 * Re-run Setup CTA + confirm dialog chrome (DESIGN Voice). No App IPC /
 * Tauri in Studio.
 */

import { useState, type ReactNode } from 'react';

import {
  Bot,
  Fingerprint,
  RotateCcw,
  Settings,
  Wifi,
  type LucideIcon,
} from 'lucide-react';
import {
  Button,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  cn,
  Input,
  Label,
} from '@42ch/nexus-ui';

import {
  Dialog,
  DialogContent,
} from '@web-ui/dialog'; // transitional — keep-web (Radix portal/focus-trap beyond presentational scope)

import {
  AgentPicker,
  type AgentPickerItem,
} from '@web-setup/agent-picker';

/** P0 Must section allowlist — Workspace omitted until P4 Stretch. */
export type SettingsSectionId = 'agent' | 'connection' | 'setup';

const SETTINGS_SECTIONS: {
  id: SettingsSectionId;
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
];

const SHELL_HELPER =
  'Manage your local agent, daemon connection, and setup options from one place.';

/** Locked by settings-agent-section.md — section body helper (sentence case). */
const AGENT_SECTION_HELPER =
  'Choose which local ACP agent Nexus uses for creative work.';

/** Locked by settings-connection-section.md — section body helper (sentence case). */
const CONNECTION_SECTION_HELPER =
  'Connect this app to a remote Nexus daemon. Your local daemon stays the default until you activate a remote connection.';

const CONNECTION_FORM_DESCRIPTION =
  'Enter the remote daemon URL and API key. Local mode remains available — you can revert here at any time.';

const CONNECTION_URL_HELPER =
  'The full HTTPS address of the daemon, including port.';

const CONNECTION_API_KEY_HELPER_PREFIX =
  'The API key from the daemon machine (';
const CONNECTION_API_KEY_HELPER_COMMAND = 'nexus42 daemon api-key';
const CONNECTION_API_KEY_HELPER_SUFFIX = ' on that host).';

const CONNECTION_FINGERPRINT_HELPER =
  'Confirm the certificate fingerprint matches what you see on the daemon machine before connecting.';

/** Locked by settings-setup-section.md — section body helper (sentence case). */
const SETUP_SECTION_HELPER =
  'Return to the first-run wizard to walk through setup steps again. Your workspace and agent choices are kept.';

const SETUP_CONFIRM_TITLE = 'Re-run Setup?';

const SETUP_CONFIRM_BODY =
  'This restarts the setup wizard from the beginning. Your workspace path and agent profile are not deleted.';

const SETUP_BROWSER_HELPER =
  'Re-run setup is available on the desktop app only.';

const SETUP_BROWSER_TOOLTIP =
  'Open the Nexus desktop app to re-run setup.';

/** Fixture-only sample values — visual chrome, not live connection state. */
const FIXTURE_DAEMON_URL = 'https://192.168.1.42:8420';
const FIXTURE_API_KEY = '••••••••••••••••';
const FIXTURE_FINGERPRINT = 'SHA256:aa:bb:cc:dd:ee:ff';
/**
 * Preselected saved-profile id for the Agent section fixture.
 * Codex (not first-installed Claude) so the visual reads as G1 preselect,
 * not the V1.102 first-installed default.
 */
const PRESELECTED_AGENT_ID = 'codex';

const FIXTURE_AGENTS: AgentPickerItem[] = [
  {
    id: 'claude-code',
    name: 'Claude Code',
    version: '1.0.42',
    description: 'Anthropic coding agent via ACP.',
    installed: true,
    installUrl: 'https://docs.anthropic.com/en/docs/claude-code',
    docsUrl: 'https://docs.anthropic.com/en/docs/claude-code',
  },
  {
    id: 'codex',
    name: 'Codex',
    version: '0.12.0',
    description: 'OpenAI Codex CLI.',
    installed: true,
    installUrl: 'https://github.com/openai/codex',
    docsUrl: null,
  },
  {
    id: 'gemini-cli',
    name: 'Gemini CLI',
    version: null,
    description: 'Google Gemini agent (not on PATH).',
    installed: false,
    installUrl: 'https://github.com/google-gemini/gemini-cli',
    docsUrl: 'https://ai.google.dev/',
  },
];

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

/**
 * Secondary section nav inside Settings page chrome.
 * Not Creator/Orchestrator tabs; not a second app-wide sidebar.
 * Workspace link intentionally absent (P4 Stretch deferred).
 */
function SettingsSectionNav({
  active,
  onSelect,
}: {
  active: SettingsSectionId;
  onSelect: (id: SettingsSectionId) => void;
}) {
  return (
    <nav
      aria-label="Settings sections"
      className="flex flex-wrap gap-1 border-b border-gray-alpha-200 pb-px"
      data-testid="settings-section-nav"
    >
      {SETTINGS_SECTIONS.map(({ id, label, icon: Icon }) => {
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
  sectionId: SettingsSectionId;
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
  activeSection: SettingsSectionId;
  onSectionChange: (id: SettingsSectionId) => void;
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
  initialSelectedId = 'claude-code',
}: {
  initialSelectedId?: string | null;
}) {
  const [selectedId, setSelectedId] = useState<string | null>(initialSelectedId);
  const [custom, setCustom] = useState('');
  return (
    <AgentPicker
      status="ready"
      agents={FIXTURE_AGENTS}
      selectedId={selectedId}
      onSelect={setSelectedId}
      customLaunchValue={custom}
      onCustomLaunchChange={setCustom}
    />
  );
}

/**
 * Agent section body chrome — mirrors apps/web SettingsAgentSection layout
 * (helper + picker + Save Agent) without scan/IPC.
 */
function SettingsAgentSectionChrome({
  initialSelectedId = PRESELECTED_AGENT_ID,
}: {
  initialSelectedId?: string | null;
}) {
  const [selectedId, setSelectedId] = useState<string | null>(initialSelectedId);
  const [custom, setCustom] = useState('');
  const canSave = Boolean(selectedId || custom.trim());

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
          agents={FIXTURE_AGENTS}
          selectedId={selectedId}
          onSelect={setSelectedId}
          customLaunchValue={custom}
          onCustomLaunchChange={setCustom}
        />
      </div>
      <div className="flex items-center gap-3">
        <Button
          variant="primary"
          type="button"
          disabled={!canSave}
          data-testid="settings-save-agent"
        >
          Save Agent
        </Button>
      </div>
    </div>
  );
}

/**
 * Connection section body chrome — locked helper + Connect-to-Daemon form
 * placeholder (settings-connection-section.md). Props-driven only; no App IPC.
 */
function SettingsConnectionSectionChrome() {
  const [showKey, setShowKey] = useState(false);

  return (
    <div
      className="flex flex-col gap-6"
      data-testid="settings-connection-section"
    >
      <div className="flex flex-col gap-2">
        <h3 className="text-heading-16 font-heading text-gray-1000">
          Connection
        </h3>
        <p className="text-copy-14 text-gray-900">{CONNECTION_SECTION_HELPER}</p>
      </div>

      <Card className="shadow-card" data-testid="settings-connection-form-chrome">
        <CardHeader>
          <div className="flex items-center gap-2">
            <Wifi className="h-5 w-5 text-blue-700" aria-hidden="true" />
            <CardTitle>Connect to Daemon</CardTitle>
          </div>
          <CardDescription>{CONNECTION_FORM_DESCRIPTION}</CardDescription>
        </CardHeader>
        <CardContent className="space-y-6">
          <div className="space-y-2">
            <Label htmlFor="studio-daemon-url">Daemon URL</Label>
            <Input
              id="studio-daemon-url"
              type="url"
              defaultValue={FIXTURE_DAEMON_URL}
              placeholder="https://192.168.1.42:8420"
              data-testid="daemon-url-input"
              readOnly
            />
            <p className="text-copy-13 text-gray-700">{CONNECTION_URL_HELPER}</p>
          </div>

          <div className="space-y-2">
            <Label htmlFor="studio-api-key">API Key</Label>
            <Input
              id="studio-api-key"
              type={showKey ? 'text' : 'password'}
              defaultValue={FIXTURE_API_KEY}
              placeholder="Enter the API key from the daemon machine"
              data-testid="api-key-input"
              readOnly
            />
            <p className="text-copy-13 text-gray-700">
              {CONNECTION_API_KEY_HELPER_PREFIX}
              <code className="rounded-control bg-background-200 px-1 py-0.5 font-mono text-[13px]">
                {CONNECTION_API_KEY_HELPER_COMMAND}
              </code>
              {CONNECTION_API_KEY_HELPER_SUFFIX}
            </p>
            <div className="flex items-center gap-2">
              <Button
                type="button"
                variant="tertiary"
                size="small"
                onClick={() => setShowKey((s) => !s)}
              >
                {showKey ? 'Hide key' : 'Show key'}
              </Button>
            </div>
          </div>

          <div className="space-y-2">
            <p className="text-copy-13 text-gray-700">
              {CONNECTION_FINGERPRINT_HELPER}
            </p>
            <div
              className="rounded-control border border-gray-alpha-400 bg-background-200 p-3 font-mono text-[13px] font-normal leading-relaxed text-gray-1000"
              data-testid="fingerprint-block"
            >
              {FIXTURE_FINGERPRINT}
            </div>
          </div>

          <div className="flex flex-wrap items-center gap-3 pt-2">
            <Button
              type="button"
              variant="secondary"
              size="default"
              data-testid="fetch-fingerprint-button"
            >
              <Fingerprint className="h-4 w-4" aria-hidden="true" />
              Fetch fingerprint
            </Button>
            <Button
              type="button"
              variant="primary"
              size="default"
              data-testid="trust-connect-button"
            >
              Trust This Certificate and Connect
            </Button>
            <Button
              type="button"
              variant="tertiary"
              size="default"
              data-testid="revert-local-button"
            >
              Use Local Daemon
            </Button>
          </div>
        </CardContent>
      </Card>
    </div>
  );
}

/**
 * Setup section body chrome — locked helper + Re-run Setup CTA + confirm
 * dialog (settings-setup-section.md). Props-driven only; no App IPC.
 *
 * `desktopAvailable` toggles honest browser-only copy vs the desktop CTA.
 */
function SettingsSetupSectionChrome({
  desktopAvailable = true,
}: {
  desktopAvailable?: boolean;
}) {
  const [confirmOpen, setConfirmOpen] = useState(false);

  return (
    <div
      className="flex flex-col gap-6"
      data-testid="settings-setup-section"
      data-desktop={desktopAvailable ? 'true' : 'false'}
    >
      <div className="flex flex-col gap-2">
        <h3 className="text-heading-16 font-heading text-gray-1000">Setup</h3>
        <p className="text-copy-14 text-gray-900">{SETUP_SECTION_HELPER}</p>
      </div>

      {desktopAvailable ? (
        <div className="flex items-center gap-3">
          <Button
            type="button"
            variant="secondary"
            data-testid="settings-rerun-setup"
            onClick={() => setConfirmOpen(true)}
          >
            Re-run Setup
          </Button>
        </div>
      ) : (
        <div className="flex flex-col gap-3" data-testid="settings-setup-browser-only">
          <p className="text-copy-14 text-gray-700">{SETUP_BROWSER_HELPER}</p>
          <div className="flex items-center gap-3">
            <Button
              type="button"
              variant="secondary"
              disabled
              title={SETUP_BROWSER_TOOLTIP}
              data-testid="settings-rerun-setup"
            >
              Re-run Setup
            </Button>
          </div>
        </div>
      )}

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
              Re-run Setup
            </Button>
          </div>
        </DialogContent>
      </Dialog>
    </div>
  );
}

/**
 * Static confirm-dialog chrome for visual acceptance — mirrors DialogContent
 * layout without Radix portal/aria-hidden (keeps Surfaces page a11y tree intact).
 */
function SettingsSetupConfirmChromeStatic() {
  return (
    <div
      className="flex max-w-[560px] flex-col overflow-hidden rounded-popover border border-gray-alpha-400 bg-background-100 shadow-modal"
      data-testid="settings-rerun-setup-confirm-chrome"
      role="group"
      aria-label="Re-run Setup confirm dialog chrome"
    >
      <div className="flex flex-col gap-1 p-6 pb-4">
        <p className="text-heading-20 font-heading tracking-tight text-gray-1000">
          {SETUP_CONFIRM_TITLE}
        </p>
        <p className="text-copy-14 text-gray-900">{SETUP_CONFIRM_BODY}</p>
      </div>
      <div className="flex justify-end gap-3 px-6 pb-6">
        <Button type="button" variant="secondary" tabIndex={-1}>
          Cancel
        </Button>
        <Button type="button" variant="destructive" tabIndex={-1}>
          Re-run Setup
        </Button>
      </div>
    </div>
  );
}

function InteractiveSettingsShellPage() {
  const [active, setActive] = useState<SettingsSectionId>('agent');
  return (
    <SettingsShellPageChrome
      activeSection={active}
      onSectionChange={setActive}
    >
      {active === 'agent' ? (
        <SettingsAgentSectionChrome />
      ) : active === 'connection' ? (
        <SettingsConnectionSectionChrome />
      ) : active === 'setup' ? (
        <SettingsSetupSectionChrome />
      ) : (
        <SettingsEmptySectionFrame sectionId={active} />
      )}
    </SettingsShellPageChrome>
  );
}

/**
 * App shell slice with Settings as footer utility (above profiles), plus
 * Settings shell page chrome: section nav + empty section frame.
 */
function SettingsShellChromeFixture() {
  return (
    <div
      className="flex min-h-[440px] border border-gray-alpha-300 rounded-card bg-background-100 overflow-hidden"
      data-testid="settings-shell-chrome"
    >
      <div className="w-sidebar-nav-width shrink-0 border-r border-gray-alpha-200 bg-background-100 flex flex-col">
        <div className="flex border-b border-gray-alpha-200">
          {(['Creator', 'Orchestrator'] as const).map((tab, i) => (
            <button
              key={tab}
              type="button"
              tabIndex={-1}
              className={cn(
                'flex-1 text-center py-3 text-label-14 font-medium border-b-2 transition-colors',
                i === 0
                  ? 'text-gray-1000 border-blue-700 bg-gray-alpha-100'
                  : 'text-gray-700 border-transparent',
              )}
            >
              {tab}
            </button>
          ))}
        </div>

        <nav className="flex-1 overflow-auto p-3 space-y-1" aria-label="Creator navigation">
          {['Works', 'Worlds', 'Findings'].map((label) => (
            <div
              key={label}
              className="flex items-center h-sidebar-nav-item-height px-3 rounded-control text-label-14 text-gray-700"
            >
              <span className="truncate">{label}</span>
            </div>
          ))}
        </nav>

        <div className="border-t border-gray-alpha-200 p-3 space-y-2">
          <a
            href="#settings"
            tabIndex={-1}
            className={cn(
              'flex items-center gap-2 h-sidebar-nav-item-height px-3 rounded-control',
              'text-label-14 text-gray-1000 bg-gray-alpha-100',
            )}
            aria-current="page"
            data-testid="settings-footer-utility-link"
          >
            <Settings className="size-4 shrink-0" aria-hidden="true" />
            <span>Settings</span>
          </a>

          <div className="flex items-center gap-2 px-1 pt-1">
            <AvatarStub label="Creator" />
            <div className="flex flex-col min-w-0">
              <span className="text-label-14 text-gray-1000 truncate">
                Local Creator
              </span>
              <span className="text-copy-13 text-gray-700 truncate">Profiles</span>
            </div>
          </div>
        </div>
      </div>

      <div className="flex-1 bg-background-200 flex flex-col min-w-0 p-8 overflow-auto">
        <InteractiveSettingsShellPage />
      </div>
    </div>
  );
}

/**
 * Settings shell chrome + empty section frames for Studio visual acceptance.
 * Workspace nav item is not rendered (P4 Stretch deferred).
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
          Settings shell — title, helper, section nav (Agent / Connection /
          Setup). Default Agent outlet shows the preselected Agent section
          body; Connection and Setup outlets show their section chrome.
          Workspace nav is absent until P4.
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
          Section chrome with locked helper copy, AgentPicker, and Save Agent.
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
          <SettingsSetupSectionChrome />
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
          <SettingsSetupSectionChrome desktopAvailable={false} />
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
          Static empty outlet frames for Agent / Connection / Setup
          placeholders. Shell outlet mounts Agent, Connection, and Setup
          bodies; empty frames remain as visual reference.
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
