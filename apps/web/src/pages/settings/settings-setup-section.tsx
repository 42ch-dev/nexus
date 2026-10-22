/**
 * Settings Setup section — re-run entry (plan v1219-onboarding T4, access spec
 * §6.3).
 *
 * The wizard route is always directly reachable (compass D21), so re-running
 * setup is a plain navigation: nothing is cleared, no data is deleted, and an
 * initialized service re-runs the finish path idempotently. Browser-available
 * — no desktop-only gate.
 */
import { useNavigate } from 'react-router';
import { useTranslation } from 'react-i18next';

import { SettingsSetupSectionChrome } from '@/components/settings/presentational/settings-setup-section-chrome';

export function SettingsSetupSection() {
  const { t } = useTranslation('settings');
  const navigate = useNavigate();

  return (
    <SettingsSetupSectionChrome
      data-testid="settings-setup-section"
      onReRunSetup={() => navigate('/setup')}
      title={t('setup.title')}
      helper={t('setup.helper')}
      rerunLabel={t('setup.rerun')}
    />
  );
}
