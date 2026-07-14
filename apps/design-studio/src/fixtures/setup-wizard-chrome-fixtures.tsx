/**
 * Studio fixtures for setup wizard chrome (V1.105 P2 portrait shell).
 *
 * Normative contract: `.mstar/iterations/v1.105/specs/portrait-wizard-shell.md`
 * Studio-local only — no product pages, no daemon client, no contracts.
 */

import { useState, type ReactNode } from 'react';
import { CheckCircle, ChevronLeft } from 'lucide-react';

import { Button, Card } from '@42ch/nexus-ui';
import {
  AgentPicker,
  type AgentPickerItem,
  type AgentPickerStatus,
} from '@web-setup/agent-picker';
import {
  TopStepIndicator,
  type WizardStep,
} from '@web-setup/top-step-indicator';
import {
  WorkspacePathField,
} from '@web-setup/workspace-path-field';

const STEP_TITLES: Record<WizardStep, string> = {
  agent: 'Choose an agent',
  workspace: 'Choose a workspace',
  done: "You're ready 🎉",
};

const OVERFLOW_AGENT_NAMES = [
  'Claude Code',
  'Codex CLI',
  'Cursor Agent',
  'Gemini CLI',
  'Aider',
  'OpenCode',
  'Continue',
  'Windsurf Cascade',
  'Cline',
  'Roo Code',
  'Amp',
  'Custom ACP',
];

// Map Studio display names to ACP registry IDs so the shared AgentPicker
// treats every overflow agent as a common agent and renders them in the grid
// without requiring the "More agents" toggle.
const OVERFLOW_AGENT_ID_BY_NAME: Record<string, string> = {
  'Claude Code': 'claude-acp',
  'Codex CLI': 'codex-acp',
  'Cursor Agent': 'cursor',
  'Gemini CLI': 'kimi',
  'Aider': 'hermes',
  'OpenCode': 'opencode',
  'Continue': 'qoder',
  'Windsurf Cascade': 'github-copilot-cli',
  'Cline': 'pi-acp',
  'Roo Code': 'kiro',
  'Amp': 'amp',
  'Custom ACP': 'custom-acp',
};

const OVERFLOW_AGENTS: AgentPickerItem[] = OVERFLOW_AGENT_NAMES.map((name) => ({
  id: OVERFLOW_AGENT_ID_BY_NAME[name] ?? name.toLowerCase().replace(/\s+/g, '-'),
  name,
  version: '1.0.0',
  description: `${name} ACP agent.`,
  installed: true,
  installUrl: null,
  docsUrl: null,
}));

const READY_AGENTS = OVERFLOW_AGENTS.slice(0, 3);

const MIXED_AGENTS: AgentPickerItem[] = [
  {
    id: 'claude-acp',
    name: 'Claude Code',
    version: '1.0.42',
    description: 'Anthropic coding agent via ACP.',
    installed: true,
    installUrl: 'https://docs.anthropic.com/en/docs/claude-code',
    docsUrl: 'https://docs.anthropic.com/en/docs/claude-code',
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

/** Normative CTA: single horizontal row — icon Back left, Continue/Finish right. */
function CtaRow({ showBack, primaryLabel }: { showBack: boolean; primaryLabel: string }) {
  return (
    <div
      className="mt-auto flex shrink-0 items-center gap-setup-wizard-surface-cta-container-gap"
      data-testid="wizard-cta-row"
      data-layout="horizontal-adjacent"
    >
      {showBack && (
        <Button variant="tertiary" type="button" aria-label="Back" className="px-2">
          <ChevronLeft className="h-4 w-4" aria-hidden="true" />
        </Button>
      )}
      <Button
        variant="primary"
        type="button"
        className="w-full max-w-setup-wizard-surface-cta-primary-max-width"
      >
        {primaryLabel}
      </Button>
    </div>
  );
}

function AgentStepBody({
  status,
  overflow = false,
  mixed = false,
}: {
  status: AgentPickerStatus;
  overflow?: boolean;
  mixed?: boolean;
}) {
  const [selectedId, setSelectedId] = useState<string | null>(
    status === 'ready'
      ? mixed
        ? MIXED_AGENTS[0]!.id
        : READY_AGENTS[0]!.id
      : null,
  );
  const [custom, setCustom] = useState('');
  const agents =
    status === 'ready'
      ? mixed
        ? MIXED_AGENTS
        : overflow
          ? OVERFLOW_AGENTS
          : READY_AGENTS
      : [];

  return (
    <AgentPicker
      status={status}
      defaultGrid={agents}
      selectedId={selectedId}
      onSelect={setSelectedId}
      customLaunchValue={custom}
      onCustomLaunchChange={setCustom}
      showCustomLaunch={false}
      emptyDescription="Install an ACP-compatible agent to continue."
      errorDescription="The daemon did not respond to the agent scan request."
      onRetry={status === 'error' ? () => undefined : undefined}
      density="compact"
    />
  );
}

function WorkspaceBody() {
  return (
    <WorkspacePathField
      id="studio-wizard-workspace-path"
      path="~/Documents/nexus/default"
      layout="wizard-stack"
      desktopAvailable={false}
      browserOnlyHelper="Workspace path changes are available on the desktop app only."
      data-testid="workspace-location-row"
    />
  );
}

function WizardChromeCard({
  currentStep,
  agentStatus = 'ready',
  agentOverflow = false,
  agentMixed = false,
}: {
  currentStep: WizardStep;
  agentStatus?: AgentPickerStatus;
  agentOverflow?: boolean;
  agentMixed?: boolean;
}) {
  const showBack = currentStep === 'workspace' || currentStep === 'done';
  const primaryLabel = currentStep === 'done' ? 'Open Nexus' : 'Continue';

  return (
    <div className="flex items-center justify-center p-2">
      <Card
        className="flex h-setup-wizard-wizard-max-height max-h-[85vh] w-full max-w-setup-wizard-step-wizard-max-width flex-col overflow-hidden rounded-popover p-0 shadow-modal"
        data-testid={`wizard-chrome-card-${currentStep}${
          currentStep === 'agent' && agentStatus !== 'ready'
            ? `-${agentStatus}`
            : agentMixed
              ? '-mixed'
              : agentOverflow
                ? '-overflow'
                : ''
        }`}
        data-current-step={currentStep}
        data-shell="portrait"
      >
        <div className="flex min-h-0 flex-1 flex-col gap-4 bg-background-100 px-setup-wizard-surface-content-panel-padding-x py-setup-wizard-surface-content-panel-padding-y">
          <TopStepIndicator currentStep={currentStep} />

          <div
            className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto"
            data-testid="wizard-step-body"
          >
            {currentStep === 'agent' && (
              <>
                <div className="flex flex-col gap-2">
                  <h3 className="text-heading-24 font-heading text-gray-1000">
                    {STEP_TITLES.agent}
                  </h3>
                <p className="text-copy-14 text-gray-900">
                  Pick a local ACP agent already on your machine.
                </p>
                </div>
                <AgentStepBody status={agentStatus} overflow={agentOverflow} mixed={agentMixed} />
              </>
            )}
            {currentStep === 'workspace' && (
              <div className="my-auto flex flex-col gap-4">
                <div className="flex flex-col gap-2">
                  <h3 className="text-heading-24 font-heading text-gray-1000">
                    {STEP_TITLES.workspace}
                  </h3>
                  <p className="text-copy-14 text-gray-900">
                    Nexus needs a workspace folder for your creative projects. We will create it if it does not exist.
                  </p>
                </div>
                <WorkspaceBody />
              </div>
            )}
            {currentStep === 'done' && (
              <div className="my-auto flex flex-col items-center gap-4 text-center">
                <CheckCircle className="h-12 w-12 text-green-800" aria-hidden />
                <div className="flex flex-col gap-2">
                  <h3 className="text-heading-24 font-heading text-gray-1000">
                    {STEP_TITLES.done}
                  </h3>
                  <p className="text-copy-14 text-gray-900">
                    Open Nexus to start writing. You can change settings anytime.
                  </p>
                </div>
              </div>
            )}
          </div>

          <CtaRow showBack={showBack} primaryLabel={primaryLabel} />
        </div>
      </Card>
    </div>
  );
}

/**
 * Visual acceptance fixtures for portrait shell + top Steps before App wiring (Task 3).
 */
export function SetupWizardChromeFixtures() {
  return (
    <div data-testid="setup-wizard-chrome-fixtures">
      <FixtureFrame
        title="Steps — agent active"
        description="Portrait card; top Steps: Agent active; Workspace/Done pending. No left rail."
        testId="wizard-chrome-steps-agent"
      >
        <WizardChromeCard currentStep="agent" />
      </FixtureFrame>

      <FixtureFrame
        title="Steps — workspace active"
        description="Portrait card; Agent complete; Workspace active. Normative CTA: Back left / Continue right."
        testId="wizard-chrome-steps-workspace"
      >
        <WizardChromeCard currentStep="workspace" />
      </FixtureFrame>

      <FixtureFrame
        title="Steps — done active"
        description="Portrait card; all prior complete; Done active. Finish CTA with Back."
        testId="wizard-chrome-steps-done"
      >
        <WizardChromeCard currentStep="done" />
      </FixtureFrame>

      <FixtureFrame
        title="Agent list — scroll overflow"
        description="Long agent list scrolls inside fixed-height portrait card; CTA stays bottom-anchored."
        testId="wizard-chrome-steps-agent-overflow"
      >
        <WizardChromeCard currentStep="agent" agentOverflow />
      </FixtureFrame>

      <FixtureFrame
        title="Agent — mixed install state"
        description="Installed badge beside title; uninstalled title muted; status dots visible."
        testId="wizard-chrome-agent-mixed"
      >
        <WizardChromeCard currentStep="agent" agentMixed />
      </FixtureFrame>

      <FixtureFrame
        title="Agent — loading"
        description="Scan in progress inside the portrait shell."
        testId="wizard-chrome-agent-loading"
      >
        <WizardChromeCard currentStep="agent" agentStatus="loading" />
      </FixtureFrame>

      <FixtureFrame
        title="Agent — empty"
        description="No agents found; custom launch escape hatch visible."
        testId="wizard-chrome-agent-empty"
      >
        <WizardChromeCard currentStep="agent" agentStatus="empty" />
      </FixtureFrame>

      <FixtureFrame
        title="Agent — error"
        description="Scan failure with retry + custom launch escape hatch."
        testId="wizard-chrome-agent-error"
      >
        <WizardChromeCard currentStep="agent" agentStatus="error" />
      </FixtureFrame>
    </div>
  );
}
