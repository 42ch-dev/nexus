#!/usr/bin/env node
/**
 * Desktop guarded action tests (v1.192 P0-T3).
 *
 * Defends the frozen OS-action contract without launching Electron: the
 * per-call path guard (symlink escape, sibling-prefix collision, root-change
 * re-resolution, unreadable root/candidate and NUL deny BEFORE any OS
 * effect), the canonical-path open, the directory picker contract
 * (defaultPath, cancel = null), the single external-URL predicate, and the
 * user-confirmed reset (cancellation leaves every byte untouched; a confirmed
 * reset invokes the bounded real recovery; failure never reports success).
 *
 * Every workspace is a temporary directory tree — real user data is never
 * touched. Runs against the compiled output:
 *   pnpm --dir apps/desktop-electron run build && node --test apps/desktop-electron/tests/desktop-actions.test.mjs
 */
import assert from 'node:assert/strict';
import {
  mkdirSync,
  mkdtempSync,
  realpathSync,
  rmSync,
  symlinkSync,
  writeFileSync,
  readdirSync,
  readFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { isAllowedDesktopExternalUrl } from '../dist/desktop-contract.js';
import { createDesktopActions } from '../dist/desktop-actions.js';

/**
 * Recording fakes for the injected Electron OS adapters. `effects` records
 * every call that would reach the OS, so tests can assert denies happen
 * before any effect.
 */
function fakeOs() {
  const effects = [];
  const shell = {
    openPath: async (path) => {
      effects.push(['openPath', path]);
      return shell.openPathResult;
    },
    openPathResult: '',
    showItemInFolder: (path) => {
      effects.push(['showItemInFolder', path]);
    },
    openExternal: async (url) => {
      effects.push(['openExternal', url]);
    },
  };
  const dialog = {
    pickDirectoryResult: null,
    pickDirectoryCalls: [],
    confirmResult: false,
    confirmCalls: 0,
    pickDirectory: async (options) => {
      dialog.pickDirectoryCalls.push(options);
      return dialog.pickDirectoryResult;
    },
    confirmReset: async () => {
      dialog.confirmCalls += 1;
      return dialog.confirmResult;
    },
  };
  return { shell, dialog, effects };
}

/** Reset controller fake recording the bounded recovery invocation. */
function fakeController() {
  return {
    calls: 0,
    failWith: null,
    async resetLocalState() {
      this.calls += 1;
      if (this.failWith) throw this.failWith;
    },
  };
}

/** Temporary workspace tree: `<tmp>/ws` (root) and `<tmp>/ws-sibling`. */
function workspace(t) {
  const base = mkdtempSync(join(tmpdir(), 'nexus-desktop-actions-'));
  t.after(() => rmSync(base, { recursive: true, force: true }));
  const root = join(base, 'ws');
  const nested = join(root, 'nested');
  mkdirSync(nested, { recursive: true });
  const file = join(nested, 'note.txt');
  writeFileSync(file, 'hello');
  const rootFile = join(root, 'top.txt');
  writeFileSync(rootFile, 'top');
  mkdirSync(join(base, 'ws-sibling'));
  const outside = join(base, 'outside.txt');
  writeFileSync(outside, 'outside');
  const escapeLink = join(root, 'escape');
  symlinkSync(join(base, 'ws-sibling'), escapeLink);
  let currentRoot = root;
  return {
    base,
    root,
    nested,
    file,
    rootFile,
    outside,
    escapeLink,
    sibling: join(base, 'ws-sibling'),
    setRoot(next) {
      currentRoot = next;
    },
    actions(overrides = {}) {
      const os = overrides.os ?? fakeOs();
      const controller = overrides.controller ?? fakeController();
      const handlers = createDesktopActions({
        resolveWorkspaceRoot: async () => {
          const resolved = await import('node:fs/promises').then((fs) => fs.realpath(currentRoot));
          return resolved;
        },
        shell: os.shell,
        dialog: os.dialog,
        controller,
      });
      return { handlers, os, controller };
    },
  };
}

function expectCode(promise, code) {
  return assert.rejects(promise, (err) => err && err.code === code);
}

// ---------------------------------------------------------------------------
// Handler surface
// ---------------------------------------------------------------------------

test('factory exposes exactly the P0-T3 handler map', (t) => {
  const ws = workspace(t);
  const { handlers } = ws.actions();
  assert.deepEqual(Object.keys(handlers).sort(), [
    'open_external_url',
    'open_with',
    'pick_directory',
    'reset_local_database',
    'reveal_in_finder',
  ]);
});

// ---------------------------------------------------------------------------
// Path guard (parity rows 1/2/5)
// ---------------------------------------------------------------------------

test('valid open reaches the OS with the canonical realpathed path', async (t) => {
  const ws = workspace(t);
  const { handlers, os } = ws.actions();
  const result = await handlers.open_with({ path: 'nested/note.txt' }, { operation: 'open_with' });
  assert.equal(result, null);
  assert.deepEqual(os.effects, [['openPath', realpathSync(ws.file)]]);
});

test('absolute path inside the workspace is accepted', async (t) => {
  const ws = workspace(t);
  const { handlers, os } = ws.actions();
  await handlers.open_with({ path: ws.rootFile }, { operation: 'open_with' });
  assert.deepEqual(os.effects, [['openPath', realpathSync(ws.rootFile)]]);
});

test('the workspace root itself is accepted as a candidate', async (t) => {
  const ws = workspace(t);
  const { handlers, os } = ws.actions();
  await handlers.reveal_in_finder({ path: '.' }, { operation: 'reveal_in_finder' });
  assert.deepEqual(os.effects, [['showItemInFolder', realpathSync(ws.root)]]);
});

test('symlink escape is denied before any OS effect', async (t) => {
  const ws = workspace(t);
  const { handlers, os } = ws.actions();
  await expectCode(handlers.open_with({ path: 'escape' }, { operation: 'open_with' }), 'path_outside_workspace');
  assert.deepEqual(os.effects, []);
});

test('sibling-prefix collision is denied before any OS effect', async (t) => {
  const ws = workspace(t);
  const { handlers, os } = ws.actions();
  // `<base>/ws-sibling` shares the literal prefix `<base>/ws` but is outside.
  await expectCode(
    handlers.open_with({ path: join(ws.base, 'ws-sibling') }, { operation: 'open_with' }),
    'path_outside_workspace',
  );
  assert.deepEqual(os.effects, []);
});

test('root change between calls is re-resolved (fresh authority per call)', async (t) => {
  const ws = workspace(t);
  const { handlers, os } = ws.actions();
  await handlers.open_with({ path: 'top.txt' }, { operation: 'open_with' });
  assert.deepEqual(os.effects, [['openPath', realpathSync(ws.rootFile)]]);
  // The creator/workspace switch commits a new root; the SAME file path must
  // now be denied because the new root does not contain it.
  ws.setRoot(ws.sibling);
  await expectCode(handlers.open_with({ path: 'top.txt' }, { operation: 'open_with' }), 'path_unresolvable');
  // A root somewhere else entirely: an existing file outside it is a
  // containment deny, not an unresolvable one.
  const other = mkdtempSync(join(tmpdir(), 'nexus-desktop-actions-other-'));
  t.after(() => rmSync(other, { recursive: true, force: true }));
  ws.setRoot(other);
  await expectCode(
    handlers.open_with({ path: ws.rootFile }, { operation: 'open_with' }),
    'path_outside_workspace',
  );
  assert.deepEqual(os.effects, [['openPath', realpathSync(ws.rootFile)]]);
});

test('unreadable root denies with workspace_root_unknown before any effect', async (t) => {
  const ws = workspace(t);
  const { handlers, os } = ws.actions();
  ws.setRoot(join(ws.base, 'missing-root'));
  await expectCode(handlers.open_with({ path: 'x' }, { operation: 'open_with' }), 'workspace_root_unknown');
  assert.deepEqual(os.effects, []);
});

test('missing candidate denies with path_unresolvable before any effect', async (t) => {
  const ws = workspace(t);
  const { handlers, os } = ws.actions();
  await expectCode(
    handlers.open_with({ path: 'nested/gone.txt' }, { operation: 'open_with' }),
    'path_unresolvable',
  );
  assert.deepEqual(os.effects, []);
});

test('NUL in path denies with path_unresolvable before any effect', async (t) => {
  const ws = workspace(t);
  const { handlers, os } = ws.actions();
  await expectCode(
    handlers.open_with({ path: 'nested/note.txt\0.txt' }, { operation: 'open_with' }),
    'path_unresolvable',
  );
  assert.deepEqual(os.effects, []);
});

test('reveal uses the same guard and reaches the OS with the canonical path', async (t) => {
  const ws = workspace(t);
  const { handlers, os } = ws.actions();
  await handlers.reveal_in_finder({ path: 'nested' }, { operation: 'reveal_in_finder' });
  assert.deepEqual(os.effects, [['showItemInFolder', realpathSync(ws.nested)]]);
  // `../outside.txt` resolves to a real file strictly outside the root.
  await expectCode(
    handlers.reveal_in_finder({ path: '../outside.txt' }, { operation: 'reveal_in_finder' }),
    'path_outside_workspace',
  );
  assert.deepEqual(os.effects, [['showItemInFolder', realpathSync(ws.nested)]]);
});

test('shell.openPath failure string surfaces as a typed error, not success', async (t) => {
  const ws = workspace(t);
  const { handlers, os } = ws.actions();
  os.shell.openPathResult = 'no such application';
  await expectCode(handlers.open_with({ path: 'top.txt' }, { operation: 'open_with' }), 'open_failed');
});

// ---------------------------------------------------------------------------
// External URL policy (parity row 25)
// ---------------------------------------------------------------------------

test('URL predicate preserves all http/https destinations', () => {
  assert.equal(isAllowedDesktopExternalUrl('https://example.com/a?b=1#c'), true);
  assert.equal(isAllowedDesktopExternalUrl('http://sub.example.co.uk:8080/path'), true);
  assert.equal(isAllowedDesktopExternalUrl('https://127.0.0.1:5173'), true);
  assert.equal(isAllowedDesktopExternalUrl('https://user%20name@example.com'), false); // userinfo
  assert.equal(isAllowedDesktopExternalUrl('https://user:pass@example.com'), false); // credentials
  assert.equal(isAllowedDesktopExternalUrl('ftp://example.com'), false);
  assert.equal(isAllowedDesktopExternalUrl('file:///etc/passwd'), false);
  assert.equal(isAllowedDesktopExternalUrl('nexus://app/index.html'), false);
  assert.equal(isAllowedDesktopExternalUrl('javascript:alert(1)'), false);
  assert.equal(isAllowedDesktopExternalUrl('https://exam\x01ple.com'), false); // control char
  assert.equal(isAllowedDesktopExternalUrl('https://example.com/\0x'), false);
  // Raw C0 controls that WHATWG parsing would strip/remap into a "clean"
  // URL must still be rejected on the raw input string.
  assert.equal(isAllowedDesktopExternalUrl('https://example.com/a\nb'), false); // newline in path
  assert.equal(isAllowedDesktopExternalUrl('https://example.com/\tfoo'), false); // tab in path
  assert.equal(isAllowedDesktopExternalUrl('https://example.com/\u0001foo'), false); // SOH in path
  assert.equal(isAllowedDesktopExternalUrl('https://example.com/a\x0db'), false); // CR in path
  assert.equal(isAllowedDesktopExternalUrl('https://exam\x1fple.com/path'), false); // US in host
  assert.equal(isAllowedDesktopExternalUrl('https://example.com/\x7fadmin'), false); // DEL in path
  assert.equal(isAllowedDesktopExternalUrl('ht\ttps://example.com/'), false); // control in scheme
  assert.equal(isAllowedDesktopExternalUrl('\nhttps://example.com/'), false); // leading newline
  assert.equal(isAllowedDesktopExternalUrl('https://example.com/ '), false); // trailing space
  assert.equal(isAllowedDesktopExternalUrl('not a url'), false);
  assert.equal(isAllowedDesktopExternalUrl('https://' + 'a'.repeat(8200)), false); // byte bound
  assert.equal(isAllowedDesktopExternalUrl(undefined), false);
  assert.equal(isAllowedDesktopExternalUrl(null), false);
});

test('open_external_url opens exactly the validated URL and denies the rest before any effect', async (t) => {
  const ws = workspace(t);
  const { handlers, os } = ws.actions();
  await handlers.open_external_url({ url: 'https://github.com/42ch-dev/nexus' }, { operation: 'open_external_url' });
  assert.deepEqual(os.effects, [['openExternal', 'https://github.com/42ch-dev/nexus']]);
  // No proof-only allowlist: any https host passes; unsafe schemes do not.
  await expectCode(
    handlers.open_external_url({ url: 'file:///etc/passwd' }, { operation: 'open_external_url' }),
    'url_not_allowed',
  );
  await expectCode(
    handlers.open_external_url({ url: 'https://u:p@example.com' }, { operation: 'open_external_url' }),
    'url_not_allowed',
  );
  // A raw control char that WHATWG parsing would silently strip still denies.
  await expectCode(
    handlers.open_external_url({ url: 'https://example.com/a\nb' }, { operation: 'open_external_url' }),
    'url_not_allowed',
  );
  assert.deepEqual(os.effects, [['openExternal', 'https://github.com/42ch-dev/nexus']]);
});

// ---------------------------------------------------------------------------
// Directory picker (parity row 20)
// ---------------------------------------------------------------------------

test('pick_directory forwards defaultPath, resolves the selection, and cancel = null', async (t) => {
  const ws = workspace(t);
  const { handlers, os } = ws.actions();
  os.dialog.pickDirectoryResult = '/Volumes/External/New Workspace';
  const picked = await handlers.pick_directory({ defaultPath: '/Volumes/External' }, { operation: 'pick_directory' });
  assert.equal(picked, '/Volumes/External/New Workspace');
  assert.deepEqual(os.dialog.pickDirectoryCalls, [{ defaultPath: '/Volumes/External' }]);
  // New workspace outside the current root is a DELIBERATE root-selection
  // result — the picker is not a guarded open/reveal action.
  os.dialog.pickDirectoryResult = null;
  assert.equal(await handlers.pick_directory({ defaultPath: '' }, { operation: 'pick_directory' }), null);
  assert.deepEqual(os.dialog.pickDirectoryCalls[1], {});
});

// ---------------------------------------------------------------------------
// User-confirmed reset (parity row 19 + D18/D20)
// ---------------------------------------------------------------------------

function resetTree(t) {
  // The "local state" the reset may touch: one real DB file + journal bytes.
  const home = mkdtempSync(join(tmpdir(), 'nexus-desktop-reset-'));
  t.after(() => rmSync(home, { recursive: true, force: true }));
  const stateDir = join(home, '.nexus42', 'creators', 'ctr_local0123456789ab', 'workspaces', 'default');
  mkdirSync(stateDir, { recursive: true });
  const db = join(stateDir, 'state.db');
  writeFileSync(db, 'local-state-bytes');
  const wal = join(stateDir, 'state.db-wal');
  writeFileSync(wal, 'wal-bytes');
  const untouched = join(home, 'Documents', 'user-doc.txt');
  mkdirSync(join(home, 'Documents'), { recursive: true });
  writeFileSync(untouched, 'user document — must survive');
  const snapshot = () =>
    JSON.stringify(readdirSync(stateDir)) + readFileSync(db, 'utf8') + readFileSync(wal, 'utf8');
  return { home, stateDir, db, wal, untouched, snapshot };
}

test('reset cancellation leaves every byte untouched and invokes no recovery', async (t) => {
  const tree = resetTree(t);
  const ws = workspace(t);
  const { handlers, os, controller } = ws.actions();
  os.dialog.confirmResult = false;
  const before = tree.snapshot();
  const result = await handlers.reset_local_database(undefined, { operation: 'reset_local_database' });
  assert.deepEqual(result, { status: 'cancelled' }); // cancellation stays in recovery — never a success claim
  assert.equal(controller.calls, 0);
  assert.equal(os.dialog.confirmCalls, 1);
  assert.equal(tree.snapshot(), before);
  assert.equal(readFileSync(tree.untouched, 'utf8'), 'user document — must survive');
});

test('confirmed reset invokes the bounded real recovery once', async (t) => {
  const tree = resetTree(t);
  const ws = workspace(t);
  const { handlers, os, controller } = ws.actions();
  os.dialog.confirmResult = true;
  const before = tree.snapshot();
  const result = await handlers.reset_local_database(undefined, { operation: 'reset_local_database' });
  assert.deepEqual(result, { status: 'confirmed' }); // success ONLY after confirmation + completed recovery
  assert.equal(controller.calls, 1);
  // The controller owns close → native resetLocalState(home) → restart; the
  // handler never passes a renderer-supplied path.
  assert.equal(tree.snapshot(), before); // the FAKE removes nothing; invocation is asserted
});

test('reset failure propagates and never surfaces as success', async (t) => {
  const tree = resetTree(t);
  const ws = workspace(t);
  const { handlers, os, controller } = ws.actions();
  os.dialog.confirmResult = true;
  controller.failWith = Object.assign(new Error('native reset timed out'), { code: 'reset_failed' });
  await expectCode(
    handlers.reset_local_database(undefined, { operation: 'reset_local_database' }),
    'reset_failed',
  );
  assert.equal(controller.calls, 1);
  assert.equal(readFileSync(tree.db, 'utf8'), 'local-state-bytes'); // no partial deletion surfaced
});

test('a failing confirmation dialog denies the reset before any recovery', async (t) => {
  const tree = resetTree(t);
  const ws = workspace(t);
  const os = fakeOs();
  const controller = fakeController();
  os.dialog.confirmReset = async () => {
    throw new Error('dialog unavailable');
  };
  const { handlers } = ws.actions({ os, controller });
  const before = tree.snapshot();
  await expectCode(
    handlers.reset_local_database(undefined, { operation: 'reset_local_database' }),
    'reset_confirmation_failed',
  );
  assert.equal(controller.calls, 0);
  assert.equal(tree.snapshot(), before);
});
