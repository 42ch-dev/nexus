/**
 * Locale key parity — en ↔ zh-CN (V1.170 P1 — AR-21).
 *
 * Project convention: every namespace ships en + zh-CN with full key parity.
 * The one sanctioned divergence is i18next pluralization: English uses
 * `key_one` / `key_other` forms while zh-CN (a single-plural-form locale)
 * collapses them to the base key or a single `_other` form. The parity check
 * therefore compares NORMALIZED key sets (`_one`/`_other` suffixes stripped),
 * which is exactly the set of keys i18next can resolve in each locale.
 *
 * The entrance block is pinned explicitly (page, wizard step, bounce toast,
 * switch control, layout labels, hub cards) so a dropped key fails with a
 * readable message instead of a generic set diff.
 */
import { describe, expect, it } from 'vitest';

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
import enInspector from '../../locales/en/inspector.json';
import enPack from '../../locales/en/pack.json';
import enWorldFindings from '../../locales/en/world-findings.json';
import enWorldRules from '../../locales/en/world-rules.json';

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
import zhInspector from '../../locales/zh-CN/inspector.json';
import zhPack from '../../locales/zh-CN/pack.json';
import zhWorldFindings from '../../locales/zh-CN/world-findings.json';
import zhWorldRules from '../../locales/zh-CN/world-rules.json';

type JsonRecord = Record<string, unknown>;

const NAMESPACE_PAIRS: ReadonlyArray<{ name: string; en: JsonRecord; zh: JsonRecord }> = [
  { name: 'common', en: enCommon, zh: zhCommon },
  { name: 'shell', en: enShell, zh: zhShell },
  { name: 'settings', en: enSettings, zh: zhSettings },
  { name: 'setup', en: enSetup, zh: zhSetup },
  { name: 'canvas', en: enCanvas, zh: zhCanvas },
  { name: 'reading', en: enReading, zh: zhReading },
  { name: 'findings', en: enFindings, zh: zhFindings },
  { name: 'memory', en: enMemory, zh: zhMemory },
  { name: 'commands', en: enCommands, zh: zhCommands },
  { name: 'works', en: enWorks, zh: zhWorks },
  { name: 'schedule', en: enSchedule, zh: zhSchedule },
  { name: 'sessions', en: enSessions, zh: zhSessions },
  { name: 'strategies', en: enStrategies, zh: zhStrategies },
  { name: 'capabilities', en: enCapabilities, zh: zhCapabilities },
  { name: 'modules', en: enModules, zh: zhModules },
  { name: 'worlds', en: enWorlds, zh: zhWorlds },
  { name: 'inspector', en: enInspector, zh: zhInspector },
  { name: 'pack', en: enPack, zh: zhPack },
  { name: 'worldFindings', en: enWorldFindings, zh: zhWorldFindings },
  { name: 'worldRules', en: enWorldRules, zh: zhWorldRules },
];

/** Flatten a nested namespace object into dotted leaf keys. */
function flattenKeys(obj: JsonRecord, prefix = ''): string[] {
  const keys: string[] = [];
  for (const [key, value] of Object.entries(obj)) {
    const path = prefix ? `${prefix}.${key}` : key;
    if (value !== null && typeof value === 'object' && !Array.isArray(value)) {
      keys.push(...flattenKeys(value as JsonRecord, path));
    } else {
      keys.push(path);
    }
  }
  return keys;
}

/** Entrance keys that must exist in BOTH locales (AR-21 + EL §2/§4/§5). */
const ENTRANCE_KEYS: readonly string[] = [
  // shell — switch control + bounce toast + layout labels
  'entrance.switchLabel',
  'entrance.bounceToast',
  'entrance.layout.content-creator',
  'entrance.layout.developer',
  'nav.develop',
  // shell — identity page (EL §2 locked copy)
  'entrance.page.title',
  'entrance.page.subtitle',
  'entrance.page.optionsLabel',
  'entrance.page.option.contentCreator.title',
  'entrance.page.option.contentCreator.description',
  'entrance.page.option.developer.title',
  'entrance.page.option.developer.description',
  'entrance.page.continue',
  'entrance.page.persistFailed',
  // shell — Develop hub v1 cards (EL §4)
  'hub.develop.title',
  'hub.develop.description',
  'hub.develop.presets.title',
  'hub.develop.presets.description',
  'hub.develop.presets.count_one',
  'hub.develop.presets.count_other',
  'hub.develop.capabilities.title',
  'hub.develop.capabilities.description',
  'hub.develop.modules.title',
  'hub.develop.modules.description',
  'hub.develop.strategyCanvas.title',
  'hub.develop.strategyCanvas.description',
  'hub.develop.runStudio.title',
  'hub.develop.runStudio.description',
  'hub.develop.connect.title',
  'hub.develop.connect.description',
  // setup — wizard step + progress labels (AR-17)
  'step.entrance.title',
  'step.entrance.description',
  'step.entrance.optionsLabel',
  'step.entrance.option.contentCreator.title',
  'step.entrance.option.contentCreator.description',
  'step.entrance.option.developer.title',
  'step.entrance.option.developer.description',
  'progress.entrance',
  'progress.agent',
  'progress.workspace',
  'progress.done',
];

describe('locale key parity (en ↔ zh-CN)', () => {
  it.each(NAMESPACE_PAIRS)(
    '$name has equal normalized key sets in en and zh-CN',
    ({ en, zh }) => {
      // Strip i18next plural suffixes — zh-CN collapses English `_one`/`_other`.
      const enKeys = new Set(flattenKeys(en).map((key) => key.replace(/_(one|other)$/, '')));
      const zhKeys = new Set(flattenKeys(zh).map((key) => key.replace(/_(one|other)$/, '')));
      expect(zhKeys).toEqual(enKeys);
    },
  );

  it('keeps the 20-namespace catalog unchanged (AR-21: no new namespace)', () => {
    expect(NAMESPACE_PAIRS).toHaveLength(20);
  });

  it('ships every entrance key in BOTH en and zh-CN (AR-21 + EL §2/§4/§5)', () => {
    const enKeys = new Set(flattenKeys(enShell));
    const zhKeys = new Set(flattenKeys(zhShell));
    const enSetupKeys = new Set(flattenKeys(enSetup));
    const zhSetupKeys = new Set(flattenKeys(zhSetup));

    for (const key of ENTRANCE_KEYS) {
      const inShell = enKeys.has(key) && zhKeys.has(key);
      const inSetup = enSetupKeys.has(key) && zhSetupKeys.has(key);
      expect(inShell || inSetup, `entrance key "${key}" missing in en or zh-CN`).toBe(true);
    }
  });
});
