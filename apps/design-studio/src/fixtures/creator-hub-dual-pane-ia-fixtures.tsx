/**
 * Studio fixtures for V1.134 P3 Creator Hub dual-pane IA.
 *
 * Proves IA contract §7 acceptance matrix:
 * - World / Work tabs × empty / populated × light / dark (8 variants)
 * - Shared tab bar above both panes; inline create in left; card list / empty in right
 * - Empty-state i18n copy (en + zh-CN keys visible in fixture labels)
 *
 * Composes `@web-layout/hub-dual-pane-chrome` without App providers or daemon hooks.
 */
import { useState, type ReactNode } from 'react';

import {
  HubDualPaneChrome,
  type HubDualPaneChromeLabels,
} from '@web-layout/hub-dual-pane-chrome';
import type { HubCardListItem } from '@web-layout/hub-card-list-pane';
import type { HubTab } from '@web-layout/hub-tab-bar';

const HUB_LABELS_EN: HubDualPaneChromeLabels = {
  tabs: { world: 'Worlds', work: 'Works' },
  workspace: {
    createWorldTitle: 'Create World',
    createWorldDescription: 'Start a new World in the local runtime.',
    createWorkTitle: 'Create Work',
    createWorkDescription: 'Create a Work to get started — Worlds are created from your Works.',
    createWorldCompact: 'Create new World…',
    createWorkCompact: 'Create new Work…',
    titleLabel: 'Title',
    titlePlaceholder: 'Enter a title',
    submitLabel: 'Create',
  },
  cardList: {
    emptyWorlds: 'No Worlds yet — create one from the left',
    emptyWorks: 'No Works yet — create one from the left',
    emptyWorldsKey: 'hub.empty.worlds',
    emptyWorksKey: 'hub.empty.works',
  },
};

const HUB_LABELS_ZH: HubDualPaneChromeLabels = {
  tabs: { world: '世界', work: '作品' },
  workspace: {
    createWorldTitle: '创建 World',
    createWorldDescription: '在本地运行时中启动一个新世界。',
    createWorkTitle: '延续 Work',
    createWorkDescription: '创建一部作品以开始创作——世界将从你的作品中诞生。',
    createWorldCompact: '创建新世界…',
    createWorkCompact: '创建新作品…',
    titleLabel: '标题',
    titlePlaceholder: '输入标题',
    submitLabel: '创建',
  },
  cardList: {
    emptyWorlds: '暂无世界，从左边创建',
    emptyWorks: '暂无作品，从左边创建',
    emptyWorldsKey: 'hub.empty.worlds',
    emptyWorksKey: 'hub.empty.works',
  },
};

const SAMPLE_WORLDS: HubCardListItem[] = [
  { id: 'world-fantasy', label: '奇幻大陆' },
  { id: 'world-scifi', label: '近未来都市' },
];

const SAMPLE_WORKS: HubCardListItem[] = [
  { id: 'work-novel', label: '漫漫长路' },
  { id: 'work-essay', label: '随笔集' },
];

type VariantState = {
  activeTab: HubTab;
  worlds: HubCardListItem[];
  works: HubCardListItem[];
  label: string;
};

const VARIANT_STATES: VariantState[] = [
  {
    activeTab: 'world',
    worlds: [],
    works: [],
    label: 'World · empty',
  },
  {
    activeTab: 'world',
    worlds: SAMPLE_WORLDS,
    works: SAMPLE_WORKS,
    label: 'World · populated',
  },
  {
    activeTab: 'work',
    worlds: SAMPLE_WORLDS,
    works: [],
    label: 'Work · empty',
  },
  {
    activeTab: 'work',
    worlds: SAMPLE_WORLDS,
    works: SAMPLE_WORKS,
    label: 'Work · populated',
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
      <h4 className="mb-1 text-heading-16 font-heading text-gray-1000">{title}</h4>
      <p className="mb-4 text-copy-13 text-gray-700">{description}</p>
      {children}
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
    <div
      data-testid={testId}
      className="grid grid-cols-1 gap-4 sm:grid-cols-2"
    >
      <div
        data-testid={`${testId}-light`}
        className="rounded-card border border-gray-alpha-300 bg-background-100 p-2"
      >
        <p className="mb-2 px-2 pt-2 text-label-14 font-medium text-gray-1000">Light</p>
        {light}
      </div>
      <div
        data-testid={`${testId}-dark`}
        className="dark rounded-card border border-gray-alpha-300 bg-background-100 p-2"
      >
        <p className="mb-2 px-2 pt-2 text-label-14 font-medium text-brand-cyan">Dark</p>
        {dark}
      </div>
    </div>
  );
}

function HubDualPaneFrame({
  activeTab: initialTab,
  worlds,
  works,
  labels,
  testId,
  interactive = false,
}: {
  activeTab: HubTab;
  worlds: HubCardListItem[];
  works: HubCardListItem[];
  labels: HubDualPaneChromeLabels;
  testId: string;
  interactive?: boolean;
}) {
  const [activeTab, setActiveTab] = useState<HubTab>(initialTab);
  const [worldItems, setWorldItems] = useState(worlds);
  const [workItems, setWorkItems] = useState(works);

  return (
    <HubDualPaneChrome
      activeTab={activeTab}
      onTabChange={setActiveTab}
      worlds={worldItems}
      works={workItems}
      labels={labels}
      tabBarAriaLabel="Creator hub entity kind"
      onCreateSubmit={
        interactive
          ? (title) => {
              if (activeTab === 'world') {
                setWorldItems((items) => [
                  ...items,
                  { id: `world-${Date.now()}`, label: title },
                ]);
              } else {
                setWorkItems((items) => [
                  ...items,
                  { id: `work-${Date.now()}`, label: title },
                ]);
              }
            }
          : undefined
      }
      data-testid={testId}
    />
  );
}

function VariantMatrixCell({
  state,
  theme,
  labels,
}: {
  state: VariantState;
  theme: 'light' | 'dark';
  labels: HubDualPaneChromeLabels;
}) {
  const slug = state.label.toLowerCase().replace(/[^a-z0-9]+/g, '-');
  const testId = `creator-hub-dual-pane-ia-matrix-${slug}-${theme}`;

  return (
    <div
      className={theme === 'dark' ? 'dark rounded-card border border-gray-alpha-300 bg-background-100 p-2' : 'rounded-card border border-gray-alpha-300 bg-background-100 p-2'}
      data-testid={testId}
    >
      <p className="mb-2 px-2 pt-2 text-label-12 font-medium text-gray-700">
        {state.label} · {theme}
      </p>
      <HubDualPaneFrame
        activeTab={state.activeTab}
        worlds={state.worlds}
        works={state.works}
        labels={labels}
        testId={`${testId}-frame`}
      />
    </div>
  );
}

export function CreatorHubDualPaneIaFixtures() {
  return (
    <div data-testid="creator-hub-dual-pane-ia-fixtures">
      <FixtureFrame
        title="Dual-pane hub — World tab × empty / populated (light / dark)"
        description="Shared tab bar spans both panes. Left = inline create affordance; right = World cards or empty state with hub.empty.worlds copy (en + zh-CN keys below)."
        testId="creator-hub-dual-pane-ia-fixture-world-tab"
      >
        <div className="mb-6 flex flex-col gap-6">
          <div>
            <p className="mb-2 text-label-12 font-medium uppercase tracking-wide text-gray-600">
              Empty — en
            </p>
            <ThemePair
              testId="creator-hub-dual-pane-ia-world-empty-themes"
              light={
                <HubDualPaneFrame
                  activeTab="world"
                  worlds={[]}
                  works={[]}
                  labels={HUB_LABELS_EN}
                  testId="creator-hub-dual-pane-ia-world-empty-light"
                />
              }
              dark={
                <HubDualPaneFrame
                  activeTab="world"
                  worlds={[]}
                  works={[]}
                  labels={HUB_LABELS_EN}
                  testId="creator-hub-dual-pane-ia-world-empty-dark"
                />
              }
            />
          </div>
          <div>
            <p className="mb-2 text-label-12 font-medium uppercase tracking-wide text-gray-600">
              Populated — zh-CN tabs
            </p>
            <ThemePair
              testId="creator-hub-dual-pane-ia-world-populated-themes"
              light={
                <HubDualPaneFrame
                  activeTab="world"
                  worlds={SAMPLE_WORLDS}
                  works={SAMPLE_WORKS}
                  labels={HUB_LABELS_ZH}
                  testId="creator-hub-dual-pane-ia-world-populated-light"
                />
              }
              dark={
                <HubDualPaneFrame
                  activeTab="world"
                  worlds={SAMPLE_WORLDS}
                  works={SAMPLE_WORKS}
                  labels={HUB_LABELS_ZH}
                  testId="creator-hub-dual-pane-ia-world-populated-dark"
                />
              }
            />
          </div>
        </div>
      </FixtureFrame>

      <FixtureFrame
        title="Dual-pane hub — Work tab × empty / populated (light / dark)"
        description="Work tab active: left inline create targets Work; right shows Work cards only (World cards hidden). Empty copy uses hub.empty.works."
        testId="creator-hub-dual-pane-ia-fixture-work-tab"
      >
        <div className="mb-6 flex flex-col gap-6">
          <div>
            <p className="mb-2 text-label-12 font-medium uppercase tracking-wide text-gray-600">
              Empty — zh-CN
            </p>
            <ThemePair
              testId="creator-hub-dual-pane-ia-work-empty-themes"
              light={
                <HubDualPaneFrame
                  activeTab="work"
                  worlds={SAMPLE_WORLDS}
                  works={[]}
                  labels={HUB_LABELS_ZH}
                  testId="creator-hub-dual-pane-ia-work-empty-light"
                />
              }
              dark={
                <HubDualPaneFrame
                  activeTab="work"
                  worlds={SAMPLE_WORLDS}
                  works={[]}
                  labels={HUB_LABELS_ZH}
                  testId="creator-hub-dual-pane-ia-work-empty-dark"
                />
              }
            />
          </div>
          <div>
            <p className="mb-2 text-label-12 font-medium uppercase tracking-wide text-gray-600">
              Populated — en
            </p>
            <ThemePair
              testId="creator-hub-dual-pane-ia-work-populated-themes"
              light={
                <HubDualPaneFrame
                  activeTab="work"
                  worlds={SAMPLE_WORLDS}
                  works={SAMPLE_WORKS}
                  labels={HUB_LABELS_EN}
                  testId="creator-hub-dual-pane-ia-work-populated-light"
                />
              }
              dark={
                <HubDualPaneFrame
                  activeTab="work"
                  worlds={SAMPLE_WORLDS}
                  works={SAMPLE_WORKS}
                  labels={HUB_LABELS_EN}
                  testId="creator-hub-dual-pane-ia-work-populated-dark"
                />
              }
            />
          </div>
        </div>
      </FixtureFrame>

      <FixtureFrame
        title="8-variant acceptance matrix (2 tabs × 2 content × 2 themes)"
        description="Minimum IA contract §7 matrix. Each cell is a static dual-pane frame — author reviews layout density, tab indicator, inline create, and empty-state cue."
        testId="creator-hub-dual-pane-ia-fixture-matrix"
      >
        <div
          className="grid grid-cols-1 gap-4 xl:grid-cols-2"
          data-testid="creator-hub-dual-pane-ia-variant-matrix"
        >
          {VARIANT_STATES.flatMap((state) => [
            <VariantMatrixCell
              key={`${state.label}-light`}
              state={state}
              theme="light"
              labels={HUB_LABELS_EN}
            />,
            <VariantMatrixCell
              key={`${state.label}-dark`}
              state={state}
              theme="dark"
              labels={HUB_LABELS_EN}
            />,
          ])}
        </div>
      </FixtureFrame>

      <FixtureFrame
        title="Interactive — linked tabs + inline create"
        description="Tab switches update both panes. Inline create on empty tab adds a card to the right list without leaving dual-pane chrome."
        testId="creator-hub-dual-pane-ia-fixture-interactive"
      >
        <HubDualPaneFrame
          activeTab="world"
          worlds={[]}
          works={[]}
          labels={HUB_LABELS_ZH}
          interactive
          testId="creator-hub-dual-pane-ia-interactive"
        />
      </FixtureFrame>
    </div>
  );
}
