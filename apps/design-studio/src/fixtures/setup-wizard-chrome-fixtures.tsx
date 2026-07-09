/**
 * Studio fixtures for setup wizard chrome polish (V1.101 P1 Task 2).
 *
 * Normative contract: `.mstar/iterations/v1.101/specs/setup-wizard-ui-polish.md` §8
 * Studio-local only — no product pages, no daemon client, no contracts.
 */

import type { ReactNode } from 'react';

import { cn, Button, Card } from '@42ch/nexus-ui';

export type WizardStepId = 'welcome' | 'daemon' | 'agent' | 'done';
export type StepStatus = 'complete' | 'active' | 'pending';
export type DaemonChipState = 'starting' | 'running' | 'error';

const STEP_DEFS: { id: WizardStepId; label: string }[] = [
  { id: 'welcome', label: 'Welcome' },
  { id: 'daemon', label: 'Daemon' },
  { id: 'agent', label: 'Agent' },
  { id: 'done', label: 'Done' },
];

const STEP_TITLES: Record<WizardStepId, string> = {
  welcome: 'Welcome to Nexus',
  daemon: 'Start the daemon',
  agent: 'Choose an agent',
  done: 'You are ready',
};

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

function StepIndicator({ currentStep }: { currentStep: WizardStepId }) {
  return (
    <nav aria-label="Setup progress">
      <ol className="flex flex-col">
        {STEP_DEFS.map((s, index) => {
          const status = stepStatus(currentStep, index);
          return (
            <li
              key={s.id}
              className="relative flex h-setup-wizard-step-row-height items-center gap-3"
              aria-current={status === 'active' ? 'step' : undefined}
              data-step-id={s.id}
              data-step-status={status}
            >
              {index < STEP_DEFS.length - 1 && (
                <div
                  className="absolute top-1/2 h-setup-wizard-step-row-height w-px bg-setup-wizard-step-connector"
                  aria-hidden
                  style={{ left: 'calc(var(--color-setup-wizard-step-circle-size) / 2)' }}
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
                  'text-setup-wizard-step-label-typography',
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

/** Normative CTA: single horizontal row — Back left, Continue right (§8.1 + progress note). */
function CtaRow({ showBack, primaryLabel }: { showBack: boolean; primaryLabel: string }) {
  return (
    <div
      className="mt-auto flex items-center gap-setup-wizard-surface-cta-container-gap"
      data-testid="wizard-cta-row"
      data-layout="horizontal-adjacent"
    >
      {showBack && (
        <Button variant="tertiary" type="button">
          Back
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

function DaemonStatusRegion({ state }: { state: DaemonChipState }) {
  return (
    <div
      className="flex min-h-[120px] flex-col items-center justify-center gap-3 rounded-card border border-gray-alpha-400 bg-background-200 p-6 text-center"
      data-testid={`daemon-chip-${state}`}
      data-daemon-state={state}
    >
      {state === 'starting' && (
        <>
          <span
            className="h-6 w-6 animate-spin rounded-full border-2 border-blue-700 border-t-transparent"
            aria-hidden
          />
          <p className="text-copy-14 text-gray-900">Starting daemon…</p>
        </>
      )}
      {state === 'running' && (
        <p className="text-copy-14 text-green-800">Daemon is running.</p>
      )}
      {state === 'error' && (
        <>
          <p className="text-copy-14 text-red-800">
            Daemon is taking longer than expected to start. You can retry or reset the local
            database.
          </p>
          <Button variant="secondary" type="button">
            Retry
          </Button>
        </>
      )}
    </div>
  );
}

function WizardChromeCard({
  currentStep,
  daemonState,
}: {
  currentStep: WizardStepId;
  daemonState?: DaemonChipState;
}) {
  const showBack = currentStep === 'daemon' || currentStep === 'agent';
  const primaryLabel = currentStep === 'done' ? 'Open Nexus' : 'Continue';

  return (
    <div className="flex items-center justify-center p-2">
      <Card
        className="flex w-full max-w-setup-wizard-step-wizard-max-width flex-col overflow-hidden rounded-popover p-0 shadow-modal sm:flex-row"
        data-testid={`wizard-chrome-card-${currentStep}`}
        data-current-step={currentStep}
      >
        <div className="w-full shrink-0 border-b border-gray-alpha-200 bg-background-100 px-setup-wizard-surface-step-panel-padding-x py-setup-wizard-surface-step-panel-padding-y sm:w-setup-wizard-surface-step-panel-width sm:border-b-0 sm:border-r">
          <StepIndicator currentStep={currentStep} />
        </div>

        <div className="flex min-h-[280px] min-w-0 flex-1 flex-col gap-6 bg-background-100 px-setup-wizard-surface-content-panel-padding-x py-setup-wizard-surface-content-panel-padding-y">
          <div className="flex flex-col gap-2">
            <h3 className="text-heading-24 font-heading text-gray-1000">
              {STEP_TITLES[currentStep]}
            </h3>
            <p className="text-copy-14 text-gray-900">
              {currentStep === 'welcome' &&
                'Nexus needs a workspace folder for your creative projects. We will create it if it does not exist.'}
              {currentStep === 'daemon' &&
                'Nexus runs a local daemon that manages your workspace, agents, and creative projects.'}
              {currentStep === 'agent' &&
                'Pick a local ACP agent already on your machine, or provide a custom launch command.'}
              {currentStep === 'done' &&
                'Nexus is set up and the daemon is running. You can change these settings later from the app menu.'}
            </p>
          </div>

          {currentStep === 'welcome' && (
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
          )}

          {currentStep === 'daemon' && daemonState && <DaemonStatusRegion state={daemonState} />}

          <CtaRow showBack={showBack} primaryLabel={primaryLabel} />
        </div>
      </Card>
    </div>
  );
}

/**
 * Visual acceptance fixtures for Back / Steps / daemon chips before App wiring (T3).
 */
export function SetupWizardChromeFixtures() {
  return (
    <div data-testid="setup-wizard-chrome-fixtures">
      <FixtureFrame
        title="Steps — welcome active"
        description="Matrix: Welcome active; Daemon/Agent/Done pending. Numbered circles + absolute connectors."
        testId="wizard-chrome-steps-welcome"
      >
        <WizardChromeCard currentStep="welcome" />
      </FixtureFrame>

      <FixtureFrame
        title="Steps — daemon active (Back + running)"
        description="Matrix: Welcome complete; Daemon active. Normative CTA row: Back left / Continue right. Daemon chip: running."
        testId="wizard-chrome-steps-daemon"
      >
        <WizardChromeCard currentStep="daemon" daemonState="running" />
      </FixtureFrame>

      <FixtureFrame
        title="Steps — agent active"
        description="Matrix: Welcome+Daemon complete; Agent active; Done pending. Back adjacent to Continue."
        testId="wizard-chrome-steps-agent"
      >
        <WizardChromeCard currentStep="agent" />
      </FixtureFrame>

      <FixtureFrame
        title="Steps — done active"
        description="Matrix: all prior complete; Done active. Finish CTA only (no Back)."
        testId="wizard-chrome-steps-done"
      >
        <WizardChromeCard currentStep="done" />
      </FixtureFrame>

      <FixtureFrame
        title="Daemon chip — starting"
        description="Spinner + “Starting daemon…”; Continue present (disabled in App — visual only here)."
        testId="wizard-chrome-daemon-starting"
      >
        <WizardChromeCard currentStep="daemon" daemonState="starting" />
      </FixtureFrame>

      <FixtureFrame
        title="Daemon chip — error"
        description="Error copy + Retry affordance (Reset local database is desktop-only in App)."
        testId="wizard-chrome-daemon-error"
      >
        <WizardChromeCard currentStep="daemon" daemonState="error" />
      </FixtureFrame>
    </div>
  );
}
