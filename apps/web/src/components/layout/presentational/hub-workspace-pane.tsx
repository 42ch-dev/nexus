import { Globe, Plus } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';

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
  submittingLabel?: string;
};

export type HubWorkspacePaneProps = {
  activeTab: HubTab;
  labels: HubWorkspacePaneLabels;
  /** Expanded inline form when the active tab has zero items. */
  createExpanded: boolean;
  onSubmit?: (title: string) => void;
  onExpandCreate?: () => void;
  isSubmitting?: boolean;
  errorMessage?: string | null;
  canCreateWorld?: boolean;
  createWorldDisabledTitle?: string;
  'data-testid'?: string;
};

function InlineCreateForm({
  activeTab,
  labels,
  onSubmit,
  isSubmitting = false,
  errorMessage,
  canCreateWorld = true,
  createWorldDisabledTitle,
  testId,
}: {
  activeTab: HubTab;
  labels: HubWorkspacePaneLabels;
  onSubmit?: (title: string) => void;
  isSubmitting?: boolean;
  errorMessage?: string | null;
  canCreateWorld?: boolean;
  createWorldDisabledTitle?: string;
  testId: string;
}) {
  const [title, setTitle] = useState('');
  const [submitted, setSubmitted] = useState(false);
  const wasSubmittingRef = useRef(false);

  useEffect(() => {
    setTitle('');
    setSubmitted(false);
    wasSubmittingRef.current = false;
  }, [activeTab]);

  useEffect(() => {
    if (isSubmitting) {
      wasSubmittingRef.current = true;
      return;
    }
    if (!submitted || !wasSubmittingRef.current) return;
    wasSubmittingRef.current = false;
    setSubmitted(false);
    if (!errorMessage) {
      setTitle('');
    }
  }, [submitted, isSubmitting, errorMessage]);

  const isWorld = activeTab === 'world';
  const Icon = isWorld ? Globe : Plus;
  const heading = isWorld ? labels.createWorldTitle : labels.createWorkTitle;
  const description = isWorld ? labels.createWorldDescription : labels.createWorkDescription;
  const worldCreateDisabled = isWorld && !canCreateWorld;

  return (
    <form
      className="flex flex-col gap-4"
      data-testid={`${testId}-inline-form`}
      onSubmit={(event) => {
        event.preventDefault();
        if (worldCreateDisabled || isSubmitting) return;
        const trimmed = title.trim();
        if (!trimmed) return;
        onSubmit?.(trimmed);
        setSubmitted(true);
      }}
    >
      <div className="flex flex-col gap-2">
        <Icon
          className="h-8 w-8 shrink-0 text-brand-deep-blue dark:text-blue-700"
          aria-hidden
        />
        <h3 className="font-display text-display-20 tracking-tight text-gray-1000">{heading}</h3>
        <p className="text-copy-14 text-gray-700">{description}</p>
        {worldCreateDisabled && createWorldDisabledTitle ? (
          <p className="text-copy-13 text-gray-700" role="note">
            {createWorldDisabledTitle}
          </p>
        ) : null}
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
          disabled={worldCreateDisabled || isSubmitting}
          data-testid={`${testId}-title-input`}
        />
      </div>

      {errorMessage ? (
        <p className="text-copy-13 text-red-700" role="alert">
          {errorMessage}
        </p>
      ) : null}

      <Button
        type="submit"
        disabled={worldCreateDisabled || isSubmitting || title.trim().length === 0}
        data-testid={`${testId}-submit`}
      >
        {isSubmitting ? labels.submittingLabel ?? labels.submitLabel : labels.submitLabel}
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
  isSubmitting = false,
  errorMessage,
  canCreateWorld = true,
  createWorldDisabledTitle,
  'data-testid': testId = 'hub-workspace-pane',
}: HubWorkspacePaneProps) {
  return (
    <div
      className="flex h-full min-h-0 flex-col overflow-auto p-6"
      data-testid={testId}
      data-active-tab={activeTab}
    >
      {createExpanded ? (
        <InlineCreateForm
          activeTab={activeTab}
          labels={labels}
          onSubmit={onSubmit}
          isSubmitting={isSubmitting}
          errorMessage={errorMessage}
          canCreateWorld={canCreateWorld}
          createWorldDisabledTitle={createWorldDisabledTitle}
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
