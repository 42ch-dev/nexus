/**
 * Settings Appearance section — V1.112 P0 i18n foundation.
 *
 * Language control bound to the locale preference. Theme remains in the header
 * this iteration (spec lock).
 */
import { useId } from 'react';
import { useTranslation } from 'react-i18next';
import { Languages } from 'lucide-react';

import { Card, CardContent, CardHeader, CardTitle } from '@/components/ui/card';
import { Label } from '@/components/ui/label';
import { Select } from '@/components/ui/select';
import { useLocale, type LocalePreference } from '@/components/locale-provider';

const PREFERENCES: LocalePreference[] = ['system', 'en', 'zh-CN'];

export function SettingsAppearanceSection() {
  const { t } = useTranslation('settings');
  const { preference, setPreference } = useLocale();
  const selectId = useId();
  const helperId = useId();

  return (
    <div className="flex flex-col gap-6" data-testid="settings-appearance-section">
      <div className="flex flex-col gap-2">
        <h3 className="text-heading-16 font-heading text-gray-1000">
          {t('appearance.title')}
        </h3>
      </div>

      <Card className="shadow-card" data-testid="settings-appearance-card">
        <CardHeader>
          <div className="flex items-center gap-2">
            <Languages className="h-5 w-5 text-blue-1000 dark:text-blue-700" aria-hidden="true" />
            <CardTitle>{t('appearance.language.label')}</CardTitle>
          </div>
        </CardHeader>
        <CardContent className="space-y-4">
          <div className="flex flex-col gap-1.5">
            <Label htmlFor={selectId}>{t('appearance.language.label')}</Label>
            <Select
              id={selectId}
              value={preference}
              onChange={(e) => setPreference(e.target.value as LocalePreference)}
              aria-describedby={helperId}
              data-testid="settings-appearance-language-select"
            >
              {PREFERENCES.map((value) => (
                <option key={value} value={value}>
                  {t(`appearance.language.${value}`)}
                </option>
              ))}
            </Select>
          </div>
          <p
            id={helperId}
            className="text-copy-13 text-gray-700"
            data-testid="settings-appearance-language-helper"
          >
            {t('appearance.language.helper')}
          </p>
        </CardContent>
      </Card>
    </div>
  );
}
