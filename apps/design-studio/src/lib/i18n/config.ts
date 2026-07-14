import i18next from 'i18next';
import { initReactI18next } from 'react-i18next';

import enCommon from '@web-locales/en/common.json';
import enShell from '@web-locales/en/shell.json';
import enSettings from '@web-locales/en/settings.json';
import enSetup from '@web-locales/en/setup.json';
import enCanvas from '@web-locales/en/canvas.json';

/**
 * Studio-local i18next instance.
 *
 * Design Studio is a separate Vite bundle, so importing i18next here gives a
 * distinct instance from apps/web. Only English is loaded (grill-me #7: en only;
 * web remains the locale SSOT). The included namespaces match the web
 * components Studio imports through @web-setup/*, @web-layout/*,
 * @web-settings/*, and @web-canvas/*.
 */
const namespaces = ['common', 'shell', 'settings', 'setup', 'canvas'] as const;

i18next
  .use(initReactI18next)
  .init({
    lng: 'en',
    fallbackLng: 'en',
    supportedLngs: ['en'],
    defaultNS: 'common',
    ns: namespaces,
    resources: {
      en: {
        common: enCommon,
        shell: enShell,
        settings: enSettings,
        setup: enSetup,
        canvas: enCanvas,
      },
    },
    interpolation: {
      escapeValue: false,
    },
    react: {
      useSuspense: false,
    },
  });

export const i18n = i18next;
