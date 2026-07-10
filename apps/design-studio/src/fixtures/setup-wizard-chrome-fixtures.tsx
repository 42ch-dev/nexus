/**
 * Studio fixtures for setup wizard chrome (V1.105 P2 portrait shell).
 *
 * Normative contract: `.mstar/iterations/v1.105/specs/portrait-wizard-shell.md`
 * Studio-local only — no product pages, no daemon client, no contracts.
 */

import type { ReactNode } from 'react';
import { ChevronLeft } from 'lucide-react';

import { cn, Button, Card } from '@42ch/nexus-ui';

export type WizardStepId = 'agent' | 'workspace' | 'done';
export type StepStatus = 'complete' | 'active' | 'pending';

const STEP_DEFS: { id: WizardStepId; label: string }[] = [
  { id: 'agent', label: 'Agent' },
  { id: 'workspace', label: 'Workspace' },
  { id: 'done', label: 'Done' },
];

const STEP_TITLES: Record<WizardStepId, string> = {
  agent: 'Choose an agent',
  workspace: 'Choose a workspace',
  done: 'You are ready',
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

function stepStatus(currentStep: WizardStepId, index: number): StepStatus {
  const currentIndex = STEP_DEFS.findIndex((s) => s.id === currentStep);
  if (index < currentIndex) return 'complete';
  if (index === currentIndex) return 'active';
  return 'pending';
}

/**
 * Top horizontal Steps (V1.105 N1) — replaces left rail StepIndicator.
 * Optional short horizontal connectors reuse setup-wizard-step-connector color.
 */
function TopStepIndicator({ currentStep }: { currentStep: WizardStepId }) {
  return (
    <nav aria-label="Setup progress" className="w-full shrink-0" data-testid="top-step-indicator">
      <ol className="flex w-full items-center justify-between gap-2">
        {STEP_DEFS.map((s, index) => {
          const status = stepStatus(currentStep, index);
          return (
            <li
              key={s.id}
              className="relative flex min-w-0 flex-1 flex-col items-center gap-2"
              aria-current={status === 'active' ? 'step' : undefined}
              data-step-id={s.id}
              data-step-status={status}
            >
              {index < STEP_DEFS.length - 1 && (
                <div
                  className="absolute top-[calc(var(--color-setup-wizard-step-circle-size)/2)] left-[calc(50%+var(--color-setup-wizard-step-circle-size)/2+4px)] right-[calc(-50%+var(--color-setup-wizard-step-circle-size)/2+4px)] h-px bg-setup-wizard-step-connector"
                  aria-hidden
                  data-testid="step-connector"
                />
              )}
              <span
                className={cn(
                  'z-10 flex h-setup-wizard-step-circle-size w-setup-wizard-step-circle-size items-center justify-center rounded-full text-button-14 font-button',
                  status === 'active' &&
                    'bg-setup-wizard-step-circle-active-bg text-setup-wizard-step-circle-active-text',
                  status === 'complete' &&
                    'bg-setup-wizard-step-circle-complete-bg text-setup-wizard-step-circle-complete-text',
                  status === 'pending' &&
                    'bg-setup-wizard-step-circle-pending-bg text-setup-wizard-step-circle-pending-text',
                )}
              >
                {index + 1}
              </span>
              <span
                className={cn(
                  'truncate text-center text-setup-wizard-step-label-typography',
                  status === 'pending'
                    ? 'text-setup-wizard-step-label-pending-color'
                    : 'text-setup-wizard-step-label-active-color',
                )}
              >
                {s.label}
              </span>
            </li>
          );
        })}
      </ol>
    </nav>
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

function AgentListBody({ overflow }: { overflow: boolean }) {
  const names = overflow ? OVERFLOW_AGENT_NAMES : OVERFLOW_AGENT_NAMES.slice(0, 3);
  return (
    <ul
      className="flex flex-col gap-2"
      data-testid={overflow ? 'wizard-agent-list-overflow' : 'wizard-agent-list'}
    >
      {names.map((name, i) => (
        <li
          key={name}
          className={cn(
            'flex min-h-setup-wizard-surface-input-row-min-height items-center rounded-control border px-setup-wizard-surface-input-row-padding-x py-setup-wizard-surface-input-row-padding-y',
            i === 0
              ? 'border-blue-700 bg-background-200'
              : 'border-setup-wizard-surface-input-row-border bg-setup-wizard-surface-input-row-bg',
          )}
        >
          <span className="text-copy-14 text-setup-wizard-surface-input-row-path-color">{name}</span>
        </li>
      ))}
    </ul>
  );
}

function WorkspaceBody() {
  return (
    <div
      className="flex min-h-setup-wizard-surface-input-row-min-height items-center gap-setup-wizard-surface-input-row-gap rounded-control border border-setup-wizard-surface-input-row-border bg-setup-wizard-surface-input-row-bg px-setup-wizard-surface-input-row-padding-x py-setup-wizard-surface-input-row-padding-y"
      data-testid="workspace-location-row"
    >
      <span className="text-label-12 text-setup-wizard-surface-input-row-label-color">
        Workspace location
      </span>
      <span className="truncate text-copy-14 text-setup-wizard-surface-input-row-path-color">
        ~/Documents/nexus/default
      </span>
      <Button variant="secondary" size="small" type="button" className="ml-auto shrink-0">
        Browse…
      </Button>
    </div>
  );
}

function WizardChromeCard({
  currentStep,
  agentOverflow = false,
}: {
  currentStep: WizardStepId;
  agentOverflow?: boolean;
}) {
  const showBack = currentStep === 'workspace' || currentStep === 'done';
  const primaryLabel = currentStep === 'done' ? 'Open Nexus' : 'Continue';

  return (
    <div className="flex items-center justify-center p-2">
      <Card
        className="flex h-setup-wizard-wizard-max-height max-h-[85vh] w-full max-w-setup-wizard-step-wizard-max-width flex-col overflow-hidden rounded-popover p-0 shadow-modal"
        data-testid={`wizard-chrome-card-${currentStep}${agentOverflow ? '-overflow' : ''}`}
        data-current-step={currentStep}
        data-shell="portrait"
      >
        <div className="flex min-h-0 flex-1 flex-col gap-4 bg-background-100 px-setup-wizard-surface-content-panel-padding-x py-setup-wizard-surface-content-panel-padding-y">
          <TopStepIndicator currentStep={currentStep} />

          <div
            className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto"
            data-testid="wizard-step-body"
          >
            <div className="flex flex-col gap-2">
              <h3 className="text-heading-24 font-heading text-gray-1000">
                {STEP_TITLES[currentStep]}
              </h3>
              <p className="text-copy-14 text-gray-900">
                {currentStep === 'agent' &&
                  'Pick a local ACP agent already on your machine, or provide a custom launch command.'}
                {currentStep === 'workspace' &&
                  'Nexus needs a workspace folder for your creative projects. We will create it if it does not exist.'}
                {currentStep === 'done' &&
                  'Nexus is set up and the daemon is running. You can change these settings later from the app menu.'}
              </p>
            </div>

            {currentStep === 'agent' && <AgentListBody overflow={agentOverflow} />}
            {currentStep === 'workspace' && <WorkspaceBody />}
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
    </div>
  );
}
