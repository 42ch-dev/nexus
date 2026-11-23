import { describe, expect, it } from 'vitest';

import { i18n } from '@/lib/i18n/config';
import {
  appsForEntrance,
  isAppLocked,
  RAIL_LOCK_REASON,
} from '@/lib/app-registry';

const CREATOR_IDS = ['world', 'work', 'session', 'memory'] as const;
const DEVELOPER_IDS = ['hub', 'harness', 'modules', 'caps', 'characters', 'session'] as const;

describe('appsForEntrance', () => {
  it('scopes the creator rail to 世界/作品/会话/记忆', () => {
    const apps = appsForEntrance('content-creator');
    expect(apps.map((a) => a.id)).toEqual([...CREATOR_IDS]);
    expect(apps.every((a) => a.route.startsWith('/'))).toBe(true);
    expect(apps.every((a) => typeof a.label === 'string')).toBe(true);
  });

  it('scopes the developer rail to 操作台/Harness/模块/能力/会话', () => {
    const apps = appsForEntrance('developer');
    expect(apps.map((a) => a.id)).toEqual([...DEVELOPER_IDS]);
  });

  it('ships `shell` i18n keys as lock reasons on the creator locked rows', () => {
    const apps = appsForEntrance('content-creator');
    const byId = Object.fromEntries(apps.map((a) => [a.id, a]));
    expect(byId.work.lockReason).toBe('rail.lockReason.work');
    expect(byId.session?.lockReason).toBe('rail.lockReason.session');
    expect(byId.memory?.lockReason).toBe('rail.lockReason.memory');
    expect(RAIL_LOCK_REASON.work).toBe(byId.work.lockReason);
  });

  it('resolves every rail lock reason in the en and zh-CN catalogs', async () => {
    await i18n.changeLanguage('en');
    expect(i18n.t(RAIL_LOCK_REASON.work, { ns: 'shell' })).toBe(
      'Unlocks once you complete the Brief',
    );
    expect(i18n.t(RAIL_LOCK_REASON.session, { ns: 'shell' })).toBe(
      'Unlocks once you create a Work',
    );
    expect(i18n.t(RAIL_LOCK_REASON.memory, { ns: 'shell' })).toBe(
      'Unlocks once the World is running',
    );
    await i18n.changeLanguage('zh-CN');
    expect(i18n.t(RAIL_LOCK_REASON.work, { ns: 'shell' })).toBe('完成 Brief 后解锁「作品」');
    expect(i18n.t(RAIL_LOCK_REASON.session, { ns: 'shell' })).toBe('创建作品后解锁「会话」');
    expect(i18n.t(RAIL_LOCK_REASON.memory, { ns: 'shell' })).toBe('世界开始运转后解锁「记忆」');
  });

  it('locks 作品 via brief-saved only; 会话/记忆 have no unlock member (D17)', () => {
    const apps = appsForEntrance('content-creator');
    const byId = Object.fromEntries(apps.map((a) => [a.id, a]));
    expect(byId.work.lockedUntil).toBe('brief-saved');
    expect(byId.session?.lockedUntil).toBeUndefined();
    expect(byId.memory?.lockedUntil).toBeUndefined();
    expect(byId.world.lockReason).toBeUndefined();
  });

  it('locks nothing on the developer rail', () => {
    expect(appsForEntrance('developer').every((a) => a.lockReason === undefined)).toBe(true);
  });
});

describe('isAppLocked', () => {
  const unlockedAll = () => true;
  const lockedAll = () => false;

  it('brief-saved apps follow the activation store', () => {
    const work = appsForEntrance('content-creator').find((a) => a.id === 'work')!;
    expect(isAppLocked(work, lockedAll)).toBe(true);
    expect(isAppLocked(work, unlockedAll)).toBe(false);
  });

  it('literal-copy apps stay locked even when the store says unlocked (D17)', () => {
    const session = appsForEntrance('content-creator').find((a) => a.id === 'session')!;
    expect(isAppLocked(session, unlockedAll)).toBe(true);
  });

  it('apps without a lock reason are never locked', () => {
    const world = appsForEntrance('content-creator').find((a) => a.id === 'world')!;
    expect(isAppLocked(world, lockedAll)).toBe(false);
  });
});
