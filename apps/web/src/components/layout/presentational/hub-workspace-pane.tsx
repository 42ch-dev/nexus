import { Globe, Plus } from 'lucide-react';
import { useState } from 'react';

import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { cn } from '@/lib/utils';

import type { HubTab } from './hub-tab-bar';

export type HubWorkspacePaneLabels = {
  createWorldTitle: string;
  createWorldDescription: string;
  createWorkTitle: string;
  createWorkDescription: string;
  createWorldCompact: string;
  createWorkCompact: string;
  titleLabel: string;
  titlePlaceholder: string;
  submitLabel: string;
};

export type HubWorkspacePaneProps = {
  activeTab: HubTab;
  labels: HubWorkspacePaneLabels;
  /** Expanded inline form when the active tab has zero items. */
  createExpanded: boolean;
  onSubmit?: (title: string) => void;
  onExpandCreate?: () => void;
  'data-testid'?: string;
};

function InlineCreateForm({
  activeTab,
  labels,
  onSubmit,
  testId,
}: {
  activeTab: HubTab;
  labels: HubWorkspacePaneLabels;
  onSubmit?: (title: string) => void;
  testId: string;
}) {
  const [title, setTitle] = useState('');
  const isWorld = activeTab === 'world';
  const Icon = isWorld ? Globe : Plus;
  const heading = isWorld ? labels.createWorldTitle : labels.createWorkTitle;
  const description = isWorld ? labels.createWorldDescription : labels.createWorkDescription;

  return (
    <form
      className="flex flex-col gap-4"
      data-testid={`${testId}-inline-form`}
      onSubmit={(event) => {
        event.preventDefault();
        const trimmed = title.trim();
        if (!trimmed) return;
        onSubmit?.(trimmed);
        setTitle('');
      }}
    >
      <div className="flex flex-col gap-2">
        <Icon
          className="h-8 w-8 shrink-0 text-brand-deep-blue dark:text-blue-700"
          aria-hidden
        />
        <h3 className="font-display text-display-20 tracking-tight text-gray-1000">{heading}</h3>
        <p className="text-copy-14 text-gray-700">{description}</p>
      </div>

      <div className="flex flex-col gap-2">
        <Label htmlFor={`${testId}-title`} className="text-label-14 text-gray-1000">
          {labels.titleLabel}
        </Label>
        <Input
          id={`${testId}-title`}
          value={title}
          onChange={(event) => setTitle(event.target.value)}
          placeholder={labels.titlePlaceholder}
          aria-required
          data-testid={`${testId}-title-input`}
        />
      </div>

      <Button type="submit" data-testid={`${testId}-submit`}>
        {labels.submitLabel}
      </Button>
    </form>
  );
}

function CompactCreateAffordance({
  activeTab,
  labels,
  onExpandCreate,
  testId,
}: {
  activeTab: HubTab;
  labels: HubWorkspacePaneLabels;
  onExpandCreate?: () => void;
  testId: string;
}) {
  const isWorld = activeTab === 'world';
  const label = isWorld ? labels.createWorldCompact : labels.createWorkCompact;
  const Icon = isWorld ? Globe : Plus;

  return (
    <button
      type="button"
      onClick={onExpandCreate}
      data-testid={`${testId}-compact-create`}
      className={cn(
        'flex w-full items-center gap-3 rounded-card border border-dashed border-gray-alpha-400 px-4 py-3 text-left',
        'transition-colors duration-state ease-standard motion-reduce:transition-none',
        'hover:bg-gray-alpha-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-700 focus-visible:ring-offset-2',
      )}
    >
      <Icon className="h-5 w-5 shrink-0 text-brand-deep-blue dark:text-blue-700" aria-hidden />
      <span className="text-label-14 font-medium text-gray-1000">{label}</span>
    </button>
  );
}

/**
 * Creator Hub left workspace pane — tab-aware inline create (V1.134 P3).
 *
 * Presentational extract consumed by App hub routes and Design Studio
 * fixtures via `@web-layout/hub-workspace-pane`. Host owns mutations and i18n.
 */
export function HubWorkspacePane({
  activeTab,
  labels,
  createExpanded,
  onSubmit,
  onExpandCreate,
  'data-testid': testId = 'hub-workspace-pane',
}: HubWorkspacePaneProps) {
  return (
    <div
      className="flex h-full min-h-0 flex-col overflow-auto p-6"
      data-testid={testId}
      data-active-tab={activeTab}
      role="tabpanel"
      id={`hub-tabpanel-${activeTab}`}
      aria-labelledby={`hub-tab-${activeTab}`}
    >
      {createExpanded ? (
        <InlineCreateForm
          activeTab={activeTab}
          labels={labels}
          onSubmit={onSubmit}
          testId={testId}
        />
      ) : (
        <CompactCreateAffordance
          activeTab={activeTab}
          labels={labels}
          onExpandCreate={onExpandCreate}
          testId={testId}
        />
      )}
    </div>
  );
}
