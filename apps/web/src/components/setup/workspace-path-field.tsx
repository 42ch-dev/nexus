/**
 * WorkspacePathField — presentational workspace folder path + change action.
 *
 * Shared between Settings (settings-row) and the setup wizard (wizard-stack).
 * No daemon client, no desktop hooks; the host owns the picker orchestration.
 */

import { FolderOpen } from 'lucide-react';

import { cn } from '@/lib/utils';
import { Button, Input, Label } from '@42ch/nexus-ui';

export const WORKSPACE_PATH_FIELD_LABEL = 'Workspace folder';

export const WORKSPACE_PATH_CHANGE_ACTION = 'Change Folder…';

const DEFAULT_BROWSER_ONLY_HELPER =
  'Workspace path changes are available on the desktop app only.';

export interface WorkspacePathFieldProps {
  /** Required — `label htmlFor` ↔ readonly `Input` a11y association. */
  id: string;
  /** Display value. */
  path: string;
  /** Optional — shows `Resolving…` placeholder while true. */
  loading?: boolean;
  /** Optional — disables the change action CTA. */
  changeDisabled?: boolean;
  /** Optional — omit in Studio fixtures. */
  onChangeClick?: () => void;
  /** Layout variant. Default `settings-row`. */
  layout?: 'settings-row' | 'wizard-stack';
  /** When false, show the browser-only helper. */
  desktopAvailable?: boolean;
  /** Optional override for the browser-only helper text. */
  browserOnlyHelper?: string;
  /** Optional tooltip for the disabled change action (e.g. desktop-only explanation). */
  title?: string;
  /** Optional root test id. */
  'data-testid'?: string;
  /** Optional test id for the readonly Input. */
  inputDataTestId?: string;
  /** Optional test id for the Change Folder button. */
  buttonDataTestId?: string;
}

export function WorkspacePathField({
  id,
  path,
  loading = false,
  changeDisabled = false,
  onChangeClick,
  layout = 'settings-row',
  desktopAvailable = true,
  browserOnlyHelper = DEFAULT_BROWSER_ONLY_HELPER,
  title,
  'data-testid': dataTestId,
  inputDataTestId,
  buttonDataTestId,
}: WorkspacePathFieldProps) {
  const isWizard = layout === 'wizard-stack';
  const disabled = changeDisabled || !desktopAvailable || !onChangeClick;
  const showHelper = !desktopAvailable;

  return (
    <div
      className={cn('flex flex-col', isWizard ? 'gap-2' : 'gap-3')}
      data-testid={dataTestId}
    >
      <Label
        htmlFor={id}
        className={cn(
          'text-label-14 font-medium text-gray-1000',
          !isWizard && 'sr-only',
        )}
      >
        {WORKSPACE_PATH_FIELD_LABEL}
      </Label>

      <div
        className={cn(
          'flex items-center gap-3',
          isWizard &&
            'min-h-setup-wizard-surface-input-row-min-height rounded-control border border-setup-wizard-surface-input-row-border bg-setup-wizard-surface-input-row-bg px-setup-wizard-surface-input-row-padding-x py-setup-wizard-surface-input-row-padding-y',
        )}
      >
        {isWizard && (
          <FolderOpen
            className="h-5 w-5 shrink-0 text-setup-wizard-surface-input-row-icon-color"
            aria-hidden
          />
        )}
        <Input
          id={id}
          type="text"
          readOnly
          value={path}
          placeholder={loading ? 'Resolving…' : ''}
          className={cn(
            'min-w-0 flex-1 truncate',
            isWizard &&
              'border-transparent bg-transparent px-0 focus-visible:border-transparent',
          )}
          aria-label={WORKSPACE_PATH_FIELD_LABEL}
          data-testid={inputDataTestId}
        />
        <Button
          type="button"
          variant="secondary"
          disabled={disabled}
          onClick={onChangeClick}
          className="shrink-0"
          title={disabled ? title : undefined}
          data-testid={buttonDataTestId}
        >
          {WORKSPACE_PATH_CHANGE_ACTION}
        </Button>
      </div>

      {showHelper ? (
        <p className="text-copy-13 text-gray-700">{browserOnlyHelper}</p>
      ) : null}
    </div>
  );
}
