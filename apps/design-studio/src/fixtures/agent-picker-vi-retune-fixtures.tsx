/**
 * V1.134 P2 T2 — AgentPicker VI retune target fixtures (studio-local).
 *
 * Proves the restored StatusDot + cyan discipline in light + dark before Task 3
 * wires the same markup into `@web-setup/agent-picker`.
 *
 * SSOT: `.mstar/iterations/v1.134/guides/p2-agent-picker-vi-rca.md`
 */

import { Loader2, User } from 'lucide-react';
import { useState, type ReactNode } from 'react';

import { Badge, cn } from '@42ch/nexus-ui';

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

/** Target StatusDot — mirrors pre-V1.132 FB-UI-006 semantics (RCA §Target VI). */
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

function ViTargetAgentCard({
  agent,
  selected,
}: {
  agent: AgentPickerItem;
  selected: boolean;
}) {
  const selectable = agent.installed;
  const label = agent.displayName || agent.name;

  return (
    <div
      data-testid={`vi-target-agent-card-${agent.id}`}
      data-installed={selectable ? 'true' : 'false'}
      className={cn(
        'flex w-full flex-col rounded-control bg-background-100 p-3 transition-colors duration-state ease-standard',
        selectable
          ? cn(
              'border-2',
              selected ? 'border-blue-700' : 'border-gray-alpha-400 hover:bg-gray-alpha-100',
            )
          : 'border border-gray-alpha-400 bg-background-200',
      )}
    >
      <div className="flex w-full items-start justify-between gap-2">
        <div className="flex min-w-0 flex-1 flex-col gap-0.5">
          <div className="flex min-w-0 items-center gap-2">
            <span className="flex h-5 w-5 shrink-0 items-center justify-center rounded-sm bg-gray-alpha-200">
              <User className="h-3 w-3 text-gray-500" aria-hidden />
            </span>
            <span
              className={cn(
                'truncate text-copy-14 font-medium',
                agent.installed ? 'text-gray-1000' : 'text-gray-700',
              )}
            >
              {label}
            </span>
            <Badge variant={agent.installed ? 'running' : 'neutral'} tone="soft" className="shrink-0">
              {agent.installed ? 'Installed' : 'Not installed'}
            </Badge>
          </div>
          {agent.version ? (
            <span className="text-copy-13 text-gray-700">v{agent.version}</span>
          ) : null}
          {agent.description ? (
            <span className="line-clamp-2 text-copy-13 text-gray-700">{agent.description}</span>
          ) : null}
        </div>
        <ViTargetStatusDot installed={agent.installed} selected={selected} />
      </div>
    </div>
  );
}

function ViTargetReadyGrid({
  agents,
  selectedId,
}: {
  agents: AgentPickerItem[];
  selectedId: string | null;
}) {
  return (
    <div
      className="flex flex-col gap-3 rounded-card border border-gray-alpha-400 bg-background-200 p-4"
      data-testid="vi-target-agent-picker-ready"
      data-status="ready"
    >
      <ul className="grid grid-cols-1 gap-3 sm:grid-cols-2">
        {agents.map((agent) => (
          <li key={agent.id}>
            <ViTargetAgentCard agent={agent} selected={selectedId === agent.id} />
          </li>
        ))}
      </ul>
    </div>
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

function InteractiveViTargetReady() {
  const [selectedId, setSelectedId] = useState<string | null>('claude-native');

  return (
    <div className="flex flex-col gap-2">
      <ViTargetReadyGrid agents={TARGET_AGENTS} selectedId={selectedId} />
      <div className="flex flex-wrap gap-2">
        {TARGET_AGENTS.filter((a) => a.installed).map((agent) => (
          <button
            key={agent.id}
            type="button"
            className="rounded-control border border-gray-alpha-400 px-2 py-1 text-label-14 text-gray-1000 hover:bg-gray-alpha-100"
            onClick={() => setSelectedId(agent.id)}
            data-testid={`vi-target-select-${agent.id}`}
          >
            Select {agent.name}
          </button>
        ))}
        <button
          type="button"
          className="rounded-control border border-gray-alpha-400 px-2 py-1 text-label-14 text-gray-700 hover:bg-gray-alpha-100"
          onClick={() => setSelectedId(null)}
          data-testid="vi-target-select-none"
        >
          Clear selection
        </button>
      </div>
    </div>
  );
}

/**
 * V1.134 P2 visual acceptance — StatusDot restored + cyan discipline.
 * Author sign-off gate before Task 3 App wiring.
 */
export function AgentPickerViRetuneFixtures() {
  return (
    <div data-testid="agent-picker-vi-retune-fixtures">
      <FixtureFrame
        title="V1.134 — StatusDot matrix (target)"
        description="Lit green when selected; hollow gray when installed-unselected; muted gray when not installed. Cyan reserved for selection ring only (no card fill wash in Light)."
        testId="vi-retune-fixture-dot-matrix"
      >
        <ThemePair
          testId="vi-retune-dot-matrix"
          light={<DotStateMatrix />}
          dark={<DotStateMatrix />}
        />
      </FixtureFrame>

      <FixtureFrame
        title="V1.134 — Ready grid with dots (target)"
        description="2px cyan selection ring + top-right StatusDot. Light: no bg-blue-700/8 wash. Dark: cyan ring liberal per DESIGN.md."
        testId="vi-retune-fixture-ready-selected"
      >
        <ThemePair
          testId="vi-retune-ready-selected"
          light={<ViTargetReadyGrid agents={TARGET_AGENTS} selectedId="claude-native" />}
          dark={<ViTargetReadyGrid agents={TARGET_AGENTS} selectedId="claude-native" />}
        />
      </FixtureFrame>

      <FixtureFrame
        title="V1.134 — Ready unselected + mixed (target)"
        description="Hollow dots on installed cards; muted dot on not-installed card."
        testId="vi-retune-fixture-ready-unselected"
      >
        <ThemePair
          testId="vi-retune-ready-unselected"
          light={<ViTargetReadyGrid agents={TARGET_AGENTS} selectedId={null} />}
          dark={<ViTargetReadyGrid agents={TARGET_AGENTS} selectedId={null} />}
        />
      </FixtureFrame>

      <FixtureFrame
        title="V1.134 — Interactive selection (target)"
        description="Toggle selection to verify lit ↔ hollow dot transition alongside ring."
        testId="vi-retune-fixture-interactive"
      >
        <InteractiveViTargetReady />
      </FixtureFrame>

      <FixtureFrame
        title="AgentPickerStatus — loading / empty / error (current component)"
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
