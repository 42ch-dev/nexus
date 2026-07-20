/**
 * Studio fixtures for Creator Create vs Controller shell chrome (V1.128 P2 T1).
 *
 * Prop-driven frames mirror App `CreatorEntitySelectionContext` modes without
 * importing App layout providers. Composes `@web-layout/creator-shell-content`.
 */
import { useState, type ReactNode } from 'react';

import logoPrimary from '@42ch/nexus-ui/assets/logos/logo-primary.svg';
import { NexusLogo } from '@42ch/nexus-ui';

import {
  CreatorShellContent,
  type CreatorEntityRef,
} from '@web-layout/creator-shell-content';
import {
  ShellSidebarChrome,
  type ShellSidebarTab,
} from '@web-layout/shell-sidebar-chrome';

import { CREATOR_NAV } from '@/fixtures/shell-nav-data';

const CREATE_LABELS = {
  createWorldTitle: 'Create',
  createWorldDescription: 'Start a new World in the local runtime.',
  createWorkTitle: 'Create',
  createWorkDescription: 'Create a Work to get started — Worlds are created from your Works.',
  createWorldDisabledTitle: 'Create World is available on the Nexus desktop app only.',
} as const;

const CONTROLLER_LABELS_BASE = {
  title: 'Controller Panel',
  description: 'Controller Panel — coming soon',
  back: 'Back',
} as const;

function controllerLabels(entity: CreatorEntityRef) {
  const kind = entity.kind === 'world' ? 'World' : 'Work';
  return {
    ...CONTROLLER_LABELS_BASE,
    selectedSummary: `Selected ${kind}: ${entity.label}`,
  };
}

const SAMPLE_WORLD: CreatorEntityRef = {
  kind: 'world',
  id: 'world-fantasy',
  label: 'My Fantasy World',
};

const SAMPLE_WORK: CreatorEntityRef = {
  kind: 'work',
  id: 'work-novel',
  label: 'The Long Road',
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

function ShellWithContent({
  content,
  activeRoute = '#worlds',
}: {
  content: ReactNode;
  activeRoute?: string;
}) {
  const [activeTab, setActiveTab] = useState<ShellSidebarTab>('creator');

  return (
    <div
      className="flex min-h-[440px] border border-gray-alpha-300 rounded-card bg-background-100 overflow-hidden"
      data-testid="creator-shell-frame"
    >
      <div className="w-sidebar-nav-width shrink-0">
        <ShellSidebarChrome
          activeTab={activeTab}
          activeRoute={activeRoute}
          navGroups={CREATOR_NAV}
          onTabChange={setActiveTab}
          logo={
            <NexusLogo
              variant="primary"
              src={logoPrimary}
              label="Nexus"
              size={32}
              className="h-8 w-auto shrink-0"
            />
          }
        />
      </div>
      <div className="flex flex-1 flex-col justify-center bg-background-200 min-w-0 p-8">
        {content}
      </div>
    </div>
  );
}

export function CreatorShellFixtures() {
  const [interactiveEntity, setInteractiveEntity] = useState<CreatorEntityRef | null>(null);

  return (
    <div data-testid="creator-shell-fixtures">
      <FixtureFrame
        title="Interactive — toggle Create ↔ Controller"
        description="Prop-driven selectedEntity mirror. Empty → Create page CTAs; selected → Controller stub + Back clears selection."
        testId="creator-shell-fixture-interactive"
      >
        <div className="mb-4 flex flex-wrap gap-2">
          <button
            type="button"
            className="rounded-control border border-gray-alpha-400 px-3 py-1.5 text-label-14 text-gray-1000 hover:bg-gray-alpha-100"
            data-testid="creator-shell-toggle-empty"
            onClick={() => setInteractiveEntity(null)}
          >
            Empty (Create)
          </button>
          <button
            type="button"
            className="rounded-control border border-gray-alpha-400 px-3 py-1.5 text-label-14 text-gray-1000 hover:bg-gray-alpha-100"
            data-testid="creator-shell-toggle-world"
            onClick={() => setInteractiveEntity(SAMPLE_WORLD)}
          >
            Select World
          </button>
          <button
            type="button"
            className="rounded-control border border-gray-alpha-400 px-3 py-1.5 text-label-14 text-gray-1000 hover:bg-gray-alpha-100"
            data-testid="creator-shell-toggle-work"
            onClick={() => setInteractiveEntity(SAMPLE_WORK)}
          >
            Select Work
          </button>
        </div>
        <ShellWithContent
          activeRoute={interactiveEntity ? '#works' : '#worlds'}
          content={
            interactiveEntity ? (
              <CreatorShellContent
                mode="controller"
                selectedEntity={interactiveEntity}
                labels={controllerLabels(interactiveEntity)}
                onBack={() => setInteractiveEntity(null)}
                data-testid="creator-shell-interactive-content"
              />
            ) : (
              <CreatorShellContent
                mode="create"
                canCreateWorld={false}
                labels={CREATE_LABELS}
                onCreateWork={() => {}}
                data-testid="creator-shell-interactive-content"
              />
            )
          }
        />
      </FixtureFrame>

      <FixtureFrame
        title="Create — honest Work fallback (createWorld absent)"
        description="When createWorld is absent on the bridge, World CTA is disabled with tooltip; Work CTA opens Create Work."
        testId="creator-shell-fixture-create-fallback"
      >
        <ShellWithContent
          content={
            <CreatorShellContent
              mode="create"
              canCreateWorld={false}
              labels={CREATE_LABELS}
              onCreateWork={() => {}}
              data-testid="creator-shell-create-fallback"
            />
          }
        />
      </FixtureFrame>

      <FixtureFrame
        title="Create — createWorld present"
        description="Both World and Work card CTAs are active when the client exposes createWorld."
        testId="creator-shell-fixture-create-world"
      >
        <ShellWithContent
          content={
            <CreatorShellContent
              mode="create"
              canCreateWorld
              labels={CREATE_LABELS}
              onCreateWorld={() => {}}
              onCreateWork={() => {}}
              data-testid="creator-shell-create-world"
            />
          }
        />
      </FixtureFrame>

      <FixtureFrame
        title="Controller — selected World stub"
        description="Placeholder copy + Back only — no business widgets."
        testId="creator-shell-fixture-controller-world"
      >
        <ShellWithContent
          activeRoute="#worlds"
          content={
            <CreatorShellContent
              mode="controller"
              selectedEntity={SAMPLE_WORLD}
              labels={controllerLabels(SAMPLE_WORLD)}
              onBack={() => {}}
              data-testid="creator-shell-controller-world"
            />
          }
        />
      </FixtureFrame>

      <FixtureFrame
        title="Controller — selected Work stub"
        description="Same stub shell for Work selection — Back returns to Create page in App wiring."
        testId="creator-shell-fixture-controller-work"
      >
        <ShellWithContent
          activeRoute="#works"
          content={
            <CreatorShellContent
              mode="controller"
              selectedEntity={SAMPLE_WORK}
              labels={controllerLabels(SAMPLE_WORLD)}
              onBack={() => {}}
              data-testid="creator-shell-controller-work"
            />
          }
        />
      </FixtureFrame>
    </div>
  );
}
