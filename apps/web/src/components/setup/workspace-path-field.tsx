/**
 * WorkspacePathField — presentational workspace folder path + change action.
 *
 * Shared between Settings (settings-row) and the setup wizard (wizard-stack).
 * No daemon client, no desktop hooks; the host owns the picker orchestration.
 * All user-facing strings are caller-owned; defaults resolve through the setup
 * namespace for the wizard path.
 */

import { FolderOpen } from 'lucide-react';
import { useTranslation } from 'react-i18next';

import { cn } from '@/lib/utils';
import { Button, Input, Label } from '@42ch/nexus-ui';

export interface WorkspacePathFieldProps {
  /** Required — `label htmlFor` ↔ readonly `Input` a11y association. */
  id: string;
  /** Display value. */
  path: string;
  /** Optional — shows resolving placeholder while true. */
  loading?: boolean;
  /** Optional — disables the change action CTA. */
  changeDisabled?: boolean;
  /** Optional — omit in Studio fixtures. */
  onChangeClick?: () => void;
  /** Layout variant. Default `settings-row`. */
  layout?: 'settings-row' | 'wizard-stack';
  /** When false, show the browser-only helper. */
  desktopAvailable?: boolean;
  /** Optional label text; defaults to setup catalog. */
  label?: string;
  /** Optional change action text; defaults to setup catalog. */
  changeAction?: string;
  /** Optional override for the browser-only helper text; defaults to setup catalog. */
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
  label,
  changeAction,
  browserOnlyHelper,
  title,
  'data-testid': dataTestId,
  inputDataTestId,
  buttonDataTestId,
}: WorkspacePathFieldProps) {
  const { t } = useTranslation('setup');
  const isWizard = layout === 'wizard-stack';
  const disabled = changeDisabled || !desktopAvailable || !onChangeClick;
  const showHelper = !desktopAvailable;
  const effectiveLabel = label ?? t('workspace.label');
  const effectiveChangeAction = changeAction ?? t('workspace.changeFolder');
  const effectivePlaceholder = loading ? t('workspace.resolving') : '';
  const effectiveBrowserOnlyHelper = browserOnlyHelper ?? t('workspace.browserOnly');

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
        {effectiveLabel}
      </Label>

      <div
        className={cn(
          'flex items-center gap-3',
          isWizard &&
            'min-h-setup-wizard-surface-input-row-min-height rounded-setup-wizard-surface-input-row-rounded border border-setup-wizard-surface-input-row-border bg-setup-wizard-surface-input-row-bg px-setup-wizard-surface-input-row-padding-x py-setup-wizard-surface-input-row-padding-y',
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
          placeholder={effectivePlaceholder}
          className={cn(
            'min-w-0 flex-1 truncate',
            isWizard &&
              'border-transparent bg-transparent px-0 focus-visible:border-transparent',
          )}
          aria-label={effectiveLabel}
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
          {effectiveChangeAction}
        </Button>
      </div>

      {showHelper ? (
        <p className="text-copy-13 text-gray-700">{effectiveBrowserOnlyHelper}</p>
      ) : null}
    </div>
  );
}
