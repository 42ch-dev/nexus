import i18next from 'i18next';
import { initReactI18next } from 'react-i18next';

import enCommon from '../../locales/en/common.json';
import enShell from '../../locales/en/shell.json';
import enSettings from '../../locales/en/settings.json';
import enSetup from '../../locales/en/setup.json';
import enCanvas from '../../locales/en/canvas.json';
import enReading from '../../locales/en/reading.json';
import enFindings from '../../locales/en/findings.json';
import enMemory from '../../locales/en/memory.json';
import enCommands from '../../locales/en/commands.json';
import enWorks from '../../locales/en/works.json';
import enSchedule from '../../locales/en/schedule.json';
import enSessions from '../../locales/en/sessions.json';
import enStrategies from '../../locales/en/strategies.json';
import enCapabilities from '../../locales/en/capabilities.json';
import enModules from '../../locales/en/modules.json';
import enWorlds from '../../locales/en/worlds.json';

import zhCommon from '../../locales/zh-CN/common.json';
import zhShell from '../../locales/zh-CN/shell.json';
import zhSettings from '../../locales/zh-CN/settings.json';
import zhSetup from '../../locales/zh-CN/setup.json';
import zhCanvas from '../../locales/zh-CN/canvas.json';
import zhReading from '../../locales/zh-CN/reading.json';
import zhFindings from '../../locales/zh-CN/findings.json';
import zhMemory from '../../locales/zh-CN/memory.json';
import zhCommands from '../../locales/zh-CN/commands.json';
import zhWorks from '../../locales/zh-CN/works.json';
import zhSchedule from '../../locales/zh-CN/schedule.json';
import zhSessions from '../../locales/zh-CN/sessions.json';
import zhStrategies from '../../locales/zh-CN/strategies.json';
import zhCapabilities from '../../locales/zh-CN/capabilities.json';
import zhModules from '../../locales/zh-CN/modules.json';
import zhWorlds from '../../locales/zh-CN/worlds.json';

export type LocalePreference = 'system' | 'en' | 'zh-CN';
export type ResolvedLocale = 'en' | 'zh-CN';

const STORAGE_KEY = 'nexus-web-locale';

/**
 * Resolve the initial locale from localStorage + system before React renders.
 * This prevents a flash of English on reload when the user selected zh-CN.
 */
function resolveInitialLocale(): ResolvedLocale {
  if (typeof window === 'undefined') return 'en';
  try {
    const stored = window.localStorage.getItem(STORAGE_KEY);
    if (stored === 'en' || stored === 'zh-CN') return stored;
    if (stored === 'system' || stored === null) {
      const sys = navigator.language;
      return sys.startsWith('zh') ? 'zh-CN' : 'en';
    }
  } catch {
    // localStorage not available (test env without --localstorage-file)
  }
  return 'en';
}

const initialLocale = resolveInitialLocale();

// Set html lang attribute synchronously to avoid flash
if (typeof document !== 'undefined') {
  document.documentElement.lang = initialLocale;
}

/**
 * Namespace catalog layout. P0 populates common/shell/settings; the remaining
 * namespaces are stubbed as empty objects so P1 can add keys without touching
 * init configuration.
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
  'works',
  'schedule',
  'sessions',
  'strategies',
  'capabilities',
  'modules',
  'worlds',
] as const;

export type Namespace = (typeof namespaces)[number];

i18next
  .use(initReactI18next)
  .init({
    lng: initialLocale,
    fallbackLng: 'en',
    supportedLngs: ['en', 'zh-CN'] as const,
    defaultNS: 'common',
    ns: namespaces as readonly string[],
    resources: {
      en: {
        common: enCommon,
        shell: enShell,
        settings: enSettings,
        setup: enSetup,
        canvas: enCanvas,
        reading: enReading,
        findings: enFindings,
        memory: enMemory,
        commands: enCommands,
        works: enWorks,
        schedule: enSchedule,
        sessions: enSessions,
        strategies: enStrategies,
        capabilities: enCapabilities,
        modules: enModules,
        worlds: enWorlds,
      },
      'zh-CN': {
        common: zhCommon,
        shell: zhShell,
        settings: zhSettings,
        setup: zhSetup,
        canvas: zhCanvas,
        reading: zhReading,
        findings: zhFindings,
        memory: zhMemory,
        commands: zhCommands,
        works: zhWorks,
        schedule: zhSchedule,
        sessions: zhSessions,
        strategies: zhStrategies,
        capabilities: zhCapabilities,
        modules: zhModules,
        worlds: zhWorlds,
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
