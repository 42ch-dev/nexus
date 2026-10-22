/**
 * App registry (plan v1219 Task 2 — Interfaces) — entrance scoping and the
 * lock-model contract: 作品 alone carries `lockedUntil: 'brief-saved'`; 会话/记忆
 * carry literal § Copy reasons with NO unlock path (D17).
 */
import { describe, expect, it } from 'vitest';

import {
  appsForEntrance,
  isAppLocked,
  RAIL_LOCK_REASON,
} from '@/lib/app-registry';

const CREATOR_IDS = ['world', 'work', 'session', 'memory'] as const;
const DEVELOPER_IDS = ['hub', 'harness', 'modules', 'caps', 'session'] as const;

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

  it('ships the § Copy lock reasons verbatim on the creator locked rows', () => {
    const apps = appsForEntrance('content-creator');
    const byId = Object.fromEntries(apps.map((a) => [a.id, a]));
    expect(byId.work.lockReason).toBe('完成 Brief 后解锁「作品」');
    expect(byId.session?.lockReason).toBe('创建作品后解锁「会话」');
    expect(byId.memory?.lockReason).toBe('世界开始运转后解锁「记忆」');
    expect(RAIL_LOCK_REASON.work).toBe(byId.work.lockReason);
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
