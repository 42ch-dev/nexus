/**
 * Settings Connection section — V1.103 P2.
 *
 * Hosts ConnectDaemonForm under SettingsShellLayout. Legacy `/connect`
 * permanently redirects here (C1).
 *
 * Author-facing copy: settings-connection-section.md.
 */

import { useTranslation } from 'react-i18next';

import { ConnectDaemonForm } from '@/components/settings/connect-daemon-form';

export function SettingsConnectionSection() {
  const { t } = useTranslation('settings');
  return (
    <div className="flex flex-col gap-6" data-testid="settings-connection-section" id="connection">
      <div className="flex flex-col gap-2">
        <h3 className="text-heading-16 font-heading text-gray-1000">{t('connection.sectionTitle')}</h3>
        <p className="text-copy-14 text-gray-900">{t('connection.sectionHelper')}</p>
      </div>
      <ConnectDaemonForm />
    </div>
  );
}
