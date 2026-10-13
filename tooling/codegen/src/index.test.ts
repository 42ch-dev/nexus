import { describe, expect, it } from 'vitest';
import fs from 'fs';
import os from 'os';
import path from 'path';
import { runCodegen } from './index';
import { resolvePrepPaths } from './schema-prep';

describe('runCodegen run lock', () => {
  it('refuses an overlapping run before it can clear the shared schema inputs', async () => {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), 'nexus-run-lock-'));
    const deref = path.join(root, 'deref');
    fs.mkdirSync(deref);
    fs.writeFileSync(path.join(deref, 'sentinel.schema.json'), '{}');
    const previousDeref = process.env.NEXUS_DEREF_SCHEMAS_DIR;
    process.env.NEXUS_DEREF_SCHEMAS_DIR = deref;

    // The run lock covers the shared localized scratch tree, not just the TS output
    // directory — prep clears it before either generator runs.
    const lockDir = `${resolvePrepPaths().localizedDir}.lock`;
    expect(fs.existsSync(lockDir)).toBe(false);
    // A competing run already holds the whole-run lock.
    fs.mkdirSync(lockDir, { recursive: true });
    fs.writeFileSync(path.join(lockDir, 'owner'), `${process.pid}:competing:0\n`);
    try {
      await expect(runCodegen()).rejects.toThrow(/another codegen run/);
      // The refusal happens before prep, so the shared inputs a competing run would
      // clear are still intact.
      expect(fs.existsSync(path.join(deref, 'sentinel.schema.json'))).toBe(true);
    } finally {
      fs.rmSync(lockDir, { recursive: true, force: true });
      if (previousDeref === undefined) delete process.env.NEXUS_DEREF_SCHEMAS_DIR;
      else process.env.NEXUS_DEREF_SCHEMAS_DIR = previousDeref;
      fs.rmSync(root, { recursive: true, force: true });
    }
  });
});
