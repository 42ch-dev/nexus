/**
 * Studio fixtures for Settings shell chrome (V1.103) + Agent section body (P1).
 *
 * Studio-local shell + page chrome only — no apps/web pages/, layout/, hooks,
 * or daemon client. Section nav labels locked by settings-shell-ia.md.
 * Workspace nav is absent until P4 Stretch runs.
 *
 * P1 Agent section fixture is props-driven with a preselected agent card
 * (saved-profile visual state). No App IPC / Tauri in Studio.
 */

import { useState, type ReactNode } from 'react';

import { Bot, RotateCcw, Settings, Wifi, type LucideIcon } from 'lucide-react';
import { Button, cn } from '@42ch/nexus-ui';

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
    emptyHint: 'Connection section body mounts here (P2).',
  },
  {
    id: 'setup',
    label: 'Setup',
    icon: RotateCcw,
    emptyHint: 'Setup re-run section body mounts here (P3).',
  },
];

const SHELL_HELPER =
  'Manage your local agent, daemon connection, and setup options from one place.';

/** Locked by settings-agent-section.md — section body helper (sentence case). */
const AGENT_SECTION_HELPER =
  'Choose which local ACP agent Nexus uses for creative work.';

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

function InteractiveSettingsShellPage() {
  const [active, setActive] = useState<SettingsSectionId>('agent');
  return (
    <SettingsShellPageChrome
      activeSection={active}
      onSectionChange={setActive}
    >
      {active === 'agent' ? (
        <SettingsAgentSectionChrome />
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
          body. Workspace nav is absent until P4.
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
        data-testid="settings-host-fixture-section-frames"
      >
        <h4 className="text-heading-16 font-heading text-gray-1000 mb-1">
          Empty section frames
        </h4>
        <p className="text-copy-13 text-gray-700 mb-4">
          Static empty outlet frames for Connection and Setup (and Agent
          placeholder). App Connection/Setup bodies land in later plans.
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
