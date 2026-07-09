/**
 * Studio fixtures for thin Settings host chrome (V1.102 P1 Task 2).
 *
 * Studio-local shell + page chrome only — no apps/web pages/, layout/, hooks,
 * or daemon client. AgentPicker via @web-setup (props-driven fixture data).
 */

import { useState, type ReactNode } from 'react';

import { Settings } from 'lucide-react';
import { cn } from '@42ch/nexus-ui';

import {
  AgentPicker,
  type AgentPickerItem,
} from '@web-setup/agent-picker';

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
 * Thin Settings host page chrome: title + helper + AgentPicker region.
 * Not a wizard — no Steps, Back/Continue, or Welcome/Daemon/Done.
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

function InteractiveSettingsPicker() {
  const [selectedId, setSelectedId] = useState<string | null>('claude-code');
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
 * App shell slice with Settings as footer utility (above profiles), matching
 * architect lock: lucide Settings, outside Creator/Orchestrator tab groups.
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
          {/* Footer utility — Settings (above profiles) */}
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
        <SettingsHostPageChrome>
          <InteractiveSettingsPicker />
        </SettingsHostPageChrome>
      </div>
    </div>
  );
}

/**
 * Thin Settings host chrome + Agent page for Studio visual acceptance.
 */
export function SettingsHostFixtures() {
  return (
    <div data-testid="settings-host-fixtures" className="space-y-8">
      <div
        className="rounded-card border border-gray-alpha-200 bg-background-100 p-4"
        data-testid="settings-host-fixture-shell"
      >
        <h4 className="text-heading-16 font-heading text-gray-1000 mb-1">
          Shell + Agent page
        </h4>
        <p className="text-copy-13 text-gray-700 mb-4">
          Footer utility Settings (lucide) above profiles; main panel is a thin
          host — title, helper, and AgentPicker — not a multi-step wizard.
        </p>
        <SettingsShellChromeFixture />
      </div>

      <div
        className="rounded-card border border-gray-alpha-200 bg-background-100 p-4"
        data-testid="settings-host-fixture-page-only"
      >
        <h4 className="text-heading-16 font-heading text-gray-1000 mb-1">
          Page chrome only
        </h4>
        <p className="text-copy-13 text-gray-700 mb-4">
          Isolated host content for App wiring reference (Task 3).
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
