import { useState } from 'react';
import { useNavigate } from 'react-router';
import { useTranslation } from 'react-i18next';

import { Button } from '@/components/ui/button';
import {
  ENTRANCE_BY_ID,
  type EntranceId,
} from '@/components/layout/entrance-registry';
import { useEntrance } from '@/lib/entrance-context';
import { useToast } from '@/lib/use-toast';
import { errorMessage } from '@/lib/error-message';
import { cn } from '@/lib/utils';

/**
 * Entrance identity page (V1.170 P1 — AR-16/AR-20, product EL §2).
 *
 * First-class User-layer choice — NOT a sidebar tab rename. Shown once on the
 * first SPA land when no entrance is stored (browser first-run; the desktop
 * first-run is the wizard step, AR-17) and reachable from the footer
 * "Switch entrance" control. Copy is the EL §2 locked English source.
 *
 * Persists ONLY on Continue (`setEntrance`): the `?entrance=` URL override
 * only pre-highlights the matching option (session-only, AR-20); selecting an
 * option without continuing writes nothing.
 */
const OPTIONS: readonly { id: EntranceId; titleKey: string; descriptionKey: string }[] = [
  {
    id: 'content-creator',
    titleKey: 'entrance.page.option.contentCreator.title',
    descriptionKey: 'entrance.page.option.contentCreator.description',
  },
  {
    id: 'developer',
    titleKey: 'entrance.page.option.developer.title',
    descriptionKey: 'entrance.page.option.developer.description',
  },
] as const;

export function EntrancePage() {
  const { t } = useTranslation('shell');
  const { entrance, setEntrance } = useEntrance();
  const navigate = useNavigate();
  const { toast } = useToast();
  // The provider already applies the URL override (precedence URL > stored >
  // default), so the initial highlight follows it (AR-20).
  const [selected, setSelected] = useState<EntranceId>(entrance);
  const [saving, setSaving] = useState(false);

  async function handleContinue() {
    setSaving(true);
    try {
      await setEntrance(selected);
      navigate(ENTRANCE_BY_ID[selected].landRoute, { replace: true });
    } catch (err) {
      toast({
        variant: 'error',
        title: t('entrance.page.persistFailed'),
        description: errorMessage(err) || undefined,
      });
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="flex min-h-screen items-center justify-center bg-background-200 p-6">
      <div
        className="flex w-full max-w-xl flex-col gap-6 rounded-popover border border-gray-alpha-400 bg-background-100 p-8 shadow-modal"
        data-testid="entrance-page"
      >
        <div className="flex flex-col gap-2">
          <h1 className="font-display text-display-24 text-gray-1000">
            {t('entrance.page.title')}
          </h1>
          <p className="text-copy-14 text-gray-900">{t('entrance.page.subtitle')}</p>
        </div>

        <div className="grid grid-cols-1 gap-4 sm:grid-cols-2" role="radiogroup" aria-label={t('entrance.page.optionsLabel')}>
          {OPTIONS.map((option) => {
            const isSelected = selected === option.id;
            return (
              <button
                key={option.id}
                type="button"
                role="radio"
                aria-checked={isSelected}
                onClick={() => setSelected(option.id)}
                data-testid={`entrance-option-${option.id}`}
                className={cn(
                  'flex flex-col gap-2 rounded-card border-2 p-4 text-left transition-colors duration-state ease-standard motion-reduce:transition-none',
                  'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-blue-1000 dark:focus-visible:ring-blue-700',
                  isSelected
                    ? 'border-blue-1000 dark:border-blue-700'
                    : 'border-gray-alpha-400 hover:bg-gray-alpha-100',
                )}
              >
                <span className="text-heading-16 font-heading text-gray-1000">
                  {t(option.titleKey)}
                </span>
                <span className="text-copy-14 text-gray-900">{t(option.descriptionKey)}</span>
              </button>
            );
          })}
        </div>

        <Button
          variant="primary"
          onClick={handleContinue}
          disabled={saving}
          className="w-full"
          data-testid="entrance-continue"
        >
          {t('entrance.page.continue')}
        </Button>
      </div>
    </div>
  );
}
