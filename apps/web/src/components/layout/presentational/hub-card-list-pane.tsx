import { ArrowLeft, Globe, Layers } from 'lucide-react';
import type { LucideIcon } from 'lucide-react';

import { cn } from '@/lib/utils';

import type { HubTab } from './hub-tab-bar';

export type HubCardListItem = {
  id: string;
  label: string;
};

export type HubCardListPaneLabels = {
  emptyWorlds: string;
  emptyWorks: string;
  /** Optional i18n key surfaced in Studio fixtures for author review. */
  emptyWorldsKey?: string;
  emptyWorksKey?: string;
};

export type HubCardListPaneProps = {
  activeTab: HubTab;
  worlds: HubCardListItem[];
  works: HubCardListItem[];
  labels: HubCardListPaneLabels;
  onSelectCard?: (id: string) => void;
  'data-testid'?: string;
};

function HubEntityCard({
  item,
  icon: Icon,
  onSelect,
  testId,
}: {
  item: HubCardListItem;
  icon: LucideIcon;
  onSelect?: (id: string) => void;
  testId: string;
}) {
  return (
    <button
      type="button"
      data-testid={testId}
      onClick={() => onSelect?.(item.id)}
      className={cn(
        'flex min-h-[5.5rem] w-full flex-col items-start gap-2 rounded-card border border-gray-alpha-300 bg-background-100 p-4 text-left',
        'transition-colors duration-state ease-standard motion-reduce:transition-none',
        'hover:border-gray-alpha-400 hover:bg-gray-alpha-100',
        'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2',
      )}
    >
      <Icon className="h-4 w-4 shrink-0 text-gray-600" aria-hidden />
      <span className="text-label-14 font-medium text-gray-1000">{item.label}</span>
    </button>
  );
}

function HubEmptyState({
  copy,
  i18nKey,
  testId,
}: {
  copy: string;
  i18nKey?: string;
  testId: string;
}) {
  return (
    <div
      className="flex h-full flex-col items-center justify-center gap-3 px-6 py-12 text-center"
      data-testid={testId}
    >
      <ArrowLeft className="h-6 w-6 text-gray-600" aria-hidden />
      <p className="max-w-xs text-copy-14 text-gray-900">{copy}</p>
      {i18nKey ? (
        <p
          className="text-copy-13-mono text-gray-600"
          data-testid={`${testId}-i18n-key`}
        >
          {i18nKey}
        </p>
      ) : null}
    </div>
  );
}

/**
 * Creator Hub right card list pane — single-kind list + empty state (V1.134 P3).
 *
 * Presentational extract consumed by App hub routes and Design Studio
 * fixtures via `@web-layout/hub-card-list-pane`. Host owns queries and i18n.
 */
export function HubCardListPane({
  activeTab,
  worlds,
  works,
  labels,
  onSelectCard,
  'data-testid': testId = 'hub-card-list-pane',
}: HubCardListPaneProps) {
  const isWorld = activeTab === 'world';
  const items = isWorld ? worlds : works;
  const Icon = isWorld ? Globe : Layers;
  const emptyCopy = isWorld ? labels.emptyWorlds : labels.emptyWorks;
  const emptyKey = isWorld ? labels.emptyWorldsKey : labels.emptyWorksKey;
  const listTestId = `${testId}-${activeTab}`;

  if (items.length === 0) {
    return (
      <div
        className="flex h-full min-h-0 flex-col overflow-auto bg-background-200"
        data-testid={testId}
        data-active-tab={activeTab}
      >
        <HubEmptyState
          copy={emptyCopy}
          i18nKey={emptyKey}
          testId={`${testId}-empty`}
        />
      </div>
    );
  }

  return (
    <div
      className="flex h-full min-h-0 flex-col overflow-auto bg-background-200 p-6"
      data-testid={testId}
      data-active-tab={activeTab}
    >
      <ul
        className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-3"
        role="list"
        data-testid={listTestId}
      >
        {items.map((item) => (
          <li key={item.id}>
            <HubEntityCard
              item={item}
              icon={Icon}
              onSelect={onSelectCard}
              testId={`${listTestId}-card-${item.id}`}
            />
          </li>
        ))}
      </ul>
    </div>
  );
}
