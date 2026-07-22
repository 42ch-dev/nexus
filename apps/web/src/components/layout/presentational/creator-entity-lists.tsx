import { Globe, Layers, type LucideIcon } from 'lucide-react';
import type { ReactNode } from 'react';

import { cn } from '@/lib/utils';

import type { CreatorEntityRef } from './creator-shell-content';

export type CreatorEntityListItem = {
  id: string;
  label: string;
};

export type CreatorEntityListsLabels = {
  worldsTitle: string;
  worksTitle: string;
};

type EntityListSectionProps = {
  title: string;
  icon: LucideIcon;
  items: CreatorEntityListItem[];
  selectedId?: string | null;
  onSelect?: (id: string) => void;
  renderRowActions?: (item: CreatorEntityListItem) => ReactNode;
  renderRowContent?: (item: CreatorEntityListItem, defaultContent: ReactNode) => ReactNode;
  testId: string;
};

function EntityListSection({
  title,
  icon: Icon,
  items,
  selectedId,
  onSelect,
  renderRowActions,
  renderRowContent,
  testId,
}: EntityListSectionProps) {
  return (
    <section data-testid={testId} className="flex flex-col gap-2">
      <h3 className="flex items-center gap-2 px-1 text-label-12 font-medium uppercase tracking-wide text-gray-600">
        <Icon className="h-3.5 w-3.5" aria-hidden />
        {title}
      </h3>
      <ul className="flex flex-col gap-1" role="list">
        {items.map((item) => {
          const selected = selectedId === item.id;
          const defaultContent = (
            <button
              type="button"
              data-testid={`${testId}-row-${item.id}`}
              aria-pressed={selected}
              onClick={() => onSelect?.(item.id)}
              className={cn(
                'flex w-full items-center rounded-control px-3 py-2 text-left text-label-14 transition-colors duration-state ease-standard motion-reduce:transition-none',
                selected
                  ? 'bg-gray-alpha-100 text-gray-1000'
                  : 'text-gray-700 hover:bg-gray-alpha-100 hover:text-gray-1000',
              )}
            >
              {item.label}
            </button>
          );

          return (
            <li key={item.id} className="group relative flex items-center gap-1">
              <div className="min-w-0 flex-1">
                {renderRowContent ? renderRowContent(item, defaultContent) : defaultContent}
              </div>
              {renderRowActions?.(item)}
            </li>
          );
        })}
      </ul>
    </section>
  );
}

export type CreatorEntityListsProps = {
  labels: CreatorEntityListsLabels;
  worlds: CreatorEntityListItem[];
  works: CreatorEntityListItem[];
  selectedEntity?: CreatorEntityRef | null;
  onSelectWorld?: (id: string) => void;
  onSelectWork?: (id: string) => void;
  renderWorldRowActions?: (item: CreatorEntityListItem) => ReactNode;
  renderWorkRowActions?: (item: CreatorEntityListItem) => ReactNode;
  renderWorldRowContent?: (item: CreatorEntityListItem, defaultContent: ReactNode) => ReactNode;
  renderWorkRowContent?: (item: CreatorEntityListItem, defaultContent: ReactNode) => ReactNode;
  'data-testid'?: string;
};

/**
 * Creator hub right-side Worlds / Works lists (V1.132 P3 AC-8).
 *
 * Presentational extract consumed by App hub routes and Design Studio
 * fixtures via `@web-layout/creator-entity-lists`. Host owns selection
 * context, row actions, and i18n labels.
 */
export function CreatorEntityLists({
  labels,
  worlds,
  works,
  selectedEntity,
  onSelectWorld,
  onSelectWork,
  renderWorldRowActions,
  renderWorkRowActions,
  renderWorldRowContent,
  renderWorkRowContent,
  'data-testid': testId = 'creator-entity-lists',
}: CreatorEntityListsProps) {
  const selectedWorldId = selectedEntity?.kind === 'world' ? selectedEntity.id : null;
  const selectedWorkId = selectedEntity?.kind === 'work' ? selectedEntity.id : null;

  return (
    <div
      data-testid={testId}
      className="flex h-full w-full flex-col gap-6 overflow-auto"
    >
      <EntityListSection
        title={labels.worldsTitle}
        icon={Globe}
        items={worlds}
        selectedId={selectedWorldId}
        onSelect={onSelectWorld}
        renderRowActions={renderWorldRowActions}
        renderRowContent={renderWorldRowContent}
        testId={`${testId}-worlds`}
      />
      <EntityListSection
        title={labels.worksTitle}
        icon={Layers}
        items={works}
        selectedId={selectedWorkId}
        onSelect={onSelectWork}
        renderRowActions={renderWorkRowActions}
        renderRowContent={renderWorkRowContent}
        testId={`${testId}-works`}
      />
    </div>
  );
}
