/**
 * V1.134 P2 — AgentPicker VI retune Studio acceptance fixtures.
 *
 * Dot-state matrix (documentation strip) + live `@web-setup/agent-picker` for
 * ready/interactive frames (AC-10 validates shipped component post-T3).
 *
 * SSOT: `.mstar/iterations/v1.134/guides/p2-agent-picker-vi-rca.md`
 */

import { useState, type ReactNode } from 'react';

import { cn } from '@42ch/nexus-ui';

import {
  AgentPicker,
  type AgentPickerItem,
  type AgentPickerStatus,
  type AgentVerifyStatus,
} from '@web-setup/agent-picker';

const TARGET_AGENTS: AgentPickerItem[] = [
  {
    id: 'claude-native',
    name: 'claude (native CLI)',
    version: '1.0.42',
    description: 'Anthropic coding agent via native CLI.',
    installed: true,
    installUrl: 'https://docs.anthropic.com/en/docs/claude-code',
    docsUrl: 'https://docs.anthropic.com/en/docs/claude-code',
  },
  {
    id: 'codex-native',
    name: 'codex (native CLI)',
    version: '0.12.0',
    description: 'OpenAI Codex CLI.',
    installed: true,
    installUrl: 'https://github.com/openai/codex',
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

type DotState = 'lit' | 'hollow' | 'muted';

/** Documentation strip — RCA dot semantics reference (not a component fork). */
function ViTargetStatusDot({
  installed,
  selected,
}: {
  installed: boolean;
  selected: boolean;
}) {
  const dot: DotState = !installed ? 'muted' : selected ? 'lit' : 'hollow';
  const title = !installed
    ? 'Not installed'
    : selected
      ? 'Selected'
      : 'Installed';

  return (
    <span
      className="relative mt-0.5 inline-flex h-2.5 w-2.5 shrink-0"
      title={title}
      aria-hidden
      data-testid="agent-status-dot"
      data-dot={dot}
    >
      <span
        className={cn(
          'absolute inset-0 rounded-full',
          dot === 'muted' && 'bg-gray-500',
          dot === 'lit' && 'bg-green-700',
          dot === 'hollow' && 'border-[1.5px] border-gray-500 bg-transparent',
        )}
      />
    </span>
  );
}

function ThemePair({
  testId,
  light,
  dark,
}: {
  testId: string;
  light: ReactNode;
  dark: ReactNode;
}) {
  return (
    <div data-testid={testId} className="grid grid-cols-1 gap-4 sm:grid-cols-2">
      <div
        data-testid={`${testId}-light`}
        className="rounded-card border border-gray-alpha-300 bg-background-100 p-3"
      >
        <p className="mb-3 text-label-14 font-medium text-gray-1000">Light shell</p>
        {light}
      </div>
      <div
        data-testid={`${testId}-dark`}
        className="dark rounded-card border border-gray-alpha-300 bg-[#08141C] p-3"
      >
        <p className="mb-3 text-label-14 font-medium text-brand-cyan">Dark shell</p>
        {dark}
      </div>
    </div>
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
      <h4 className="mb-1 text-heading-16 font-heading text-gray-1000">{title}</h4>
      <p className="mb-4 text-copy-13 text-gray-700">{description}</p>
      {children}
    </div>
  );
}

function DotStateMatrix() {
  const samples: { label: string; installed: boolean; selected: boolean }[] = [
    { label: 'Selected (lit)', installed: true, selected: true },
    { label: 'Installed unselected (hollow)', installed: true, selected: false },
    { label: 'Not installed (muted)', installed: false, selected: false },
  ];

  return (
    <div className="flex flex-wrap gap-6" data-testid="vi-target-dot-matrix">
      {samples.map((sample) => (
        <div key={sample.label} className="flex flex-col items-center gap-2">
          <ViTargetStatusDot installed={sample.installed} selected={sample.selected} />
          <span className="text-copy-13 text-gray-700">{sample.label}</span>
        </div>
      ))}
    </div>
  );
}

function LiveReadyPicker({
  selectedId,
  onSelect,
}: {
  selectedId: string | null;
  onSelect?: (id: string) => void;
}) {
  const [custom, setCustom] = useState('');
  return (
    <AgentPicker
      status="ready"
      defaultGrid={TARGET_AGENTS}
      selectedId={selectedId}
      onSelect={onSelect}
      customLaunchValue={custom}
      onCustomLaunchChange={setCustom}
    />
  );
}

function InteractiveAgentPickerReady() {
  const [selectedId, setSelectedId] = useState<string | null>('claude-native');

  return (
    <div className="flex flex-col gap-2" data-testid="vi-retune-interactive-root">
      <LiveReadyPicker selectedId={selectedId} onSelect={setSelectedId} />
      <div className="flex flex-wrap gap-2">
        {TARGET_AGENTS.filter((a) => a.installed).map((agent) => (
          <button
            key={agent.id}
            type="button"
            className="rounded-control border border-gray-alpha-400 px-2 py-1 text-label-14 text-gray-1000 hover:bg-gray-alpha-100"
            onClick={() => setSelectedId(agent.id)}
            data-testid={`vi-retune-select-${agent.id}`}
          >
            Select {agent.name}
          </button>
        ))}
        <button
          type="button"
          className="rounded-control border border-gray-alpha-400 px-2 py-1 text-label-14 text-gray-700 hover:bg-gray-alpha-100"
          onClick={() => setSelectedId(null)}
          data-testid="vi-retune-select-none"
        >
          Clear selection
        </button>
      </div>
    </div>
  );
}

/**
 * V1.134 P2 visual acceptance — StatusDot restored + cyan discipline.
 */
export function AgentPickerViRetuneFixtures() {
  return (
    <div data-testid="agent-picker-vi-retune-fixtures">
      <FixtureFrame
        title="V1.134 — StatusDot matrix (reference)"
        description="Lit green when selected; hollow gray when installed-unselected; muted gray when not installed. Documentation strip only — ready grids below use live AgentPicker."
        testId="vi-retune-fixture-dot-matrix"
      >
        <ThemePair
          testId="vi-retune-dot-matrix"
          light={<DotStateMatrix />}
          dark={<DotStateMatrix />}
        />
      </FixtureFrame>

      <FixtureFrame
        title="V1.134 — Ready grid with dots (live)"
        description="Live `@web-setup/agent-picker`: 2px cyan selection ring + top-right StatusDot. Light: no bg-blue-700/8 wash."
        testId="vi-retune-fixture-ready-selected"
      >
        <ThemePair
          testId="vi-retune-ready-selected"
          light={<LiveReadyPicker selectedId="claude-native" />}
          dark={<LiveReadyPicker selectedId="claude-native" />}
        />
      </FixtureFrame>

      <FixtureFrame
        title="V1.134 — Ready unselected + mixed (live)"
        description="Live AgentPicker — hollow dots on installed cards; muted dot on not-installed card."
        testId="vi-retune-fixture-ready-unselected"
      >
        <ThemePair
          testId="vi-retune-ready-unselected"
          light={<LiveReadyPicker selectedId={null} />}
          dark={<LiveReadyPicker selectedId={null} />}
        />
      </FixtureFrame>

      <FixtureFrame
        title="V1.134 — Interactive selection (live)"
        description="Live AgentPicker — toggle selection to verify lit ↔ hollow dot transition alongside ring."
        testId="vi-retune-fixture-interactive"
      >
        <InteractiveAgentPickerReady />
      </FixtureFrame>

      <FixtureFrame
        title="AgentPickerStatus — loading / empty / error"
        description="No per-card dots for non-ready statuses. Loading spinner uses cyan accent (DESIGN.md)."
        testId="vi-retune-fixture-statuses"
      >
        <ThemePair
          testId="vi-retune-status-loading"
          light={<AgentPicker status={'loading' satisfies AgentPickerStatus} />}
          dark={<AgentPicker status={'loading' satisfies AgentPickerStatus} />}
        />
        <div className="mt-4" />
        <ThemePair
          testId="vi-retune-status-empty"
          light={
            <AgentPicker
              status="empty"
              customLaunchValue=""
              onCustomLaunchChange={() => undefined}
              onVerify={() => undefined}
              verifyStatus={'idle' satisfies AgentVerifyStatus}
            />
          }
          dark={
            <AgentPicker
              status="empty"
              customLaunchValue=""
              onCustomLaunchChange={() => undefined}
              onVerify={() => undefined}
              verifyStatus={'idle' satisfies AgentVerifyStatus}
            />
          }
        />
        <div className="mt-4" />
        <ThemePair
          testId="vi-retune-status-error"
          light={
            <AgentPicker
              status="error"
              errorDescription="The daemon did not respond to the agent scan request."
              onRetry={() => undefined}
              customLaunchValue=""
              onCustomLaunchChange={() => undefined}
            />
          }
          dark={
            <AgentPicker
              status="error"
              errorDescription="The daemon did not respond to the agent scan request."
              onRetry={() => undefined}
              customLaunchValue=""
              onCustomLaunchChange={() => undefined}
            />
          }
        />
      </FixtureFrame>
    </div>
  );
}
