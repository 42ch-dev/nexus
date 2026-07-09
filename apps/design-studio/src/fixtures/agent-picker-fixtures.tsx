/**
 * Studio fixtures for AgentPicker visual states (V1.101 P0).
 *
 * Props-driven only — no @42ch/nexus-contracts, no daemon client.
 * Import path: @web-setup/agent-picker → apps/web/src/components/setup/agent-picker
 */

import { useState, type ReactNode } from 'react';

import {
  AgentPicker,
  type AgentPickerItem,
  type AgentPickerStatus,
} from '@web-setup/agent-picker';

const INSTALLED_ONLY: AgentPickerItem[] = [
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
];

const MIXED: AgentPickerItem[] = [
  ...INSTALLED_ONLY,
  {
    id: 'gemini-cli',
    name: 'Gemini CLI',
    version: null,
    description: 'Google Gemini agent (not on PATH).',
    installed: false,
    installUrl: 'https://github.com/google-gemini/gemini-cli',
    docsUrl: 'https://ai.google.dev/',
  },
  {
    id: 'cursor-agent',
    name: 'Cursor Agent',
    version: null,
    description: 'Known registry entry without install/docs URLs.',
    installed: false,
    // Both URLs missing → Install/Docs links hidden (acceptance path).
    installUrl: null,
    docsUrl: null,
  },
];

function FixtureFrame({
  title,
  description,
  children,
}: {
  title: string;
  description: string;
  children: ReactNode;
}) {
  return (
    <div
      className="mb-8 rounded-card border border-gray-alpha-200 bg-background-100 p-4"
      data-testid={`agent-picker-fixture-${title.toLowerCase().replace(/\s+/g, '-')}`}
    >
      <h4 className="text-heading-16 font-heading text-gray-1000 mb-1">{title}</h4>
      <p className="text-copy-13 text-gray-700 mb-4">{description}</p>
      <div className="max-w-2xl">{children}</div>
    </div>
  );
}

function InteractiveSelectedFixture() {
  const [selectedId, setSelectedId] = useState<string | null>('claude-code');
  const [custom, setCustom] = useState('');
  return (
    <AgentPicker
      status="ready"
      agents={INSTALLED_ONLY}
      selectedId={selectedId}
      onSelect={setSelectedId}
      customLaunchValue={custom}
      onCustomLaunchChange={setCustom}
    />
  );
}

function InteractiveMixedFixture() {
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [custom, setCustom] = useState('');
  return (
    <AgentPicker
      status="ready"
      agents={MIXED}
      selectedId={selectedId}
      onSelect={setSelectedId}
      customLaunchValue={custom}
      onCustomLaunchChange={setCustom}
    />
  );
}

/**
 * Six required visual states for AgentPicker Studio acceptance.
 */
export function AgentPickerFixtures() {
  const [emptyCustom, setEmptyCustom] = useState('');
  const [errorCustom, setErrorCustom] = useState('');

  return (
    <div data-testid="agent-picker-fixtures">
      <FixtureFrame
        title="Loading"
        description="Scan in progress — spinner + present participle copy."
      >
        <AgentPicker status={'loading' satisfies AgentPickerStatus} />
      </FixtureFrame>

      <FixtureFrame
        title="Installed grid"
        description="All agents installed; Install/Docs outbound links when URLs present (Codex hides Docs)."
      >
        <AgentPicker
          status="ready"
          agents={INSTALLED_ONLY}
          selectedId={null}
          onSelect={() => undefined}
          customLaunchValue=""
          onCustomLaunchChange={() => undefined}
        />
      </FixtureFrame>

      <FixtureFrame
        title="Mixed installed / not installed"
        description="Not-installed cards are non-selectable; Cursor Agent hides both outbound links."
      >
        <InteractiveMixedFixture />
      </FixtureFrame>

      <FixtureFrame
        title="Empty"
        description="No agents on PATH — custom launch affordance required."
      >
        <AgentPicker
          status="empty"
          customLaunchValue={emptyCustom}
          onCustomLaunchChange={setEmptyCustom}
        />
      </FixtureFrame>

      <FixtureFrame
        title="Error"
        description="Scan failure — retry + custom launch escape hatch."
      >
        <AgentPicker
          status="error"
          errorDescription="The daemon did not respond to the agent scan request."
          onRetry={() => undefined}
          customLaunchValue={errorCustom}
          onCustomLaunchChange={setErrorCustom}
        />
      </FixtureFrame>

      <FixtureFrame
        title="Selected"
        description="Installed agent selected (aria-pressed + status-dot ring)."
      >
        <InteractiveSelectedFixture />
      </FixtureFrame>
    </div>
  );
}
