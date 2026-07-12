import i18next from 'i18next';
import { initReactI18next } from 'react-i18next';

export type LocalePreference = 'system' | 'en' | 'zh-CN';
export type ResolvedLocale = 'en' | 'zh-CN';

/**
 * Namespace catalog layout. P0 populates common/shell/settings; T2 will replace
 * the inline empty resources below with real JSON imports from
 * src/locales/{en,zh-CN}/. All nine namespaces are registered here so P1 can
 * add keys without touching init configuration.
 */
export const namespaces = [
  'common',
  'shell',
  'settings',
  'setup',
  'canvas',
  'reading',
  'findings',
  'memory',
  'commands',
] as const;

export type Namespace = (typeof namespaces)[number];

// T2: replace these inline empty objects with imported JSON catalogs.
i18next
  .use(initReactI18next)
  .init({
    lng: 'en',
    fallbackLng: 'en',
    supportedLngs: ['en', 'zh-CN'] as const,
    defaultNS: 'common',
    ns: namespaces as readonly string[],
    resources: {
      en: {
        common: {},
        shell: {},
        settings: {},
        setup: {},
        canvas: {},
        reading: {},
        findings: {},
        memory: {},
        commands: {},
      },
      'zh-CN': {
        common: {},
        shell: {},
        settings: {},
        setup: {},
        canvas: {},
        reading: {},
        findings: {},
        memory: {},
        commands: {},
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
