import { afterEach, describe, expect, it } from 'vitest';
import fs from 'fs';
import os from 'os';
import path from 'path';
import { buildDereferencedSchemaTree } from './schema-prep';

const tempDirs: string[] = [];

afterEach(() => {
  for (const dir of tempDirs.splice(0)) {
    fs.rmSync(dir, { recursive: true, force: true });
  }
});

function makeLocalizedTree(
  files: Record<string, Record<string, unknown>>,
): { localizedDir: string; derefDir: string; schemaPaths: string[] } {
  const localizedDir = fs.mkdtempSync(path.join(os.tmpdir(), 'nexus-schema-local-'));
  const derefDir = fs.mkdtempSync(path.join(os.tmpdir(), 'nexus-schema-deref-'));
  tempDirs.push(localizedDir, derefDir);

  for (const [relPath, schema] of Object.entries(files)) {
    const outPath = path.join(localizedDir, relPath);
    fs.mkdirSync(path.dirname(outPath), { recursive: true });
    fs.writeFileSync(outPath, JSON.stringify(schema, null, 2));
  }

  return { localizedDir, derefDir, schemaPaths: Object.keys(files).sort() };
}

describe('buildDereferencedSchemaTree', () => {
  it('dereferences legitimate cross-file refs inside localizedDir', async () => {
    const { localizedDir, derefDir, schemaPaths } = makeLocalizedTree({
      'child.schema.json': {
        type: 'object',
        properties: { id: { type: 'string' } },
      },
      'parent.schema.json': {
        type: 'object',
        properties: {
          child: { $ref: './child.schema.json' },
        },
      },
    });

    await buildDereferencedSchemaTree(schemaPaths, localizedDir, derefDir);

    const parent = JSON.parse(
      fs.readFileSync(path.join(derefDir, 'parent.schema.json'), 'utf8'),
    ) as Record<string, unknown>;
    const childProp = (parent.properties as Record<string, unknown>).child as Record<
      string,
      unknown
    >;
    expect(childProp).not.toHaveProperty('$ref');
    expect(childProp).toMatchObject({ type: 'object' });
  });

  it('rejects HTTP $ref without network fetch', async () => {
    const { localizedDir, derefDir, schemaPaths } = makeLocalizedTree({
      'remote.schema.json': {
        type: 'object',
        properties: {
          leaked: { $ref: 'https://example.com/evil.schema.json' },
        },
      },
    });

    await expect(
      buildDereferencedSchemaTree(schemaPaths, localizedDir, derefDir),
    ).rejects.toThrow();
  });

  it('rejects relative $ref escaping localizedDir', async () => {
    const localizedDir = fs.mkdtempSync(path.join(os.tmpdir(), 'nexus-schema-local-'));
    const derefDir = fs.mkdtempSync(path.join(os.tmpdir(), 'nexus-schema-deref-'));
    const outsideDir = fs.mkdtempSync(path.join(os.tmpdir(), 'nexus-schema-out-'));
    tempDirs.push(localizedDir, derefDir, outsideDir);

    const outsideSchema = path.join(outsideDir, 'outside.schema.json');
    fs.writeFileSync(outsideSchema, JSON.stringify({ type: 'string' }));

    const relSchema = 'nested/escape.schema.json';
    const nestedPath = path.join(localizedDir, relSchema);
    fs.mkdirSync(path.dirname(nestedPath), { recursive: true });
    const escapeRef = path.relative(path.dirname(nestedPath), outsideSchema).split(path.sep).join('/');
    fs.writeFileSync(
      nestedPath,
      JSON.stringify({
        type: 'object',
        properties: {
          escaped: { $ref: escapeRef },
        },
      }),
    );

    await expect(
      buildDereferencedSchemaTree([relSchema], localizedDir, derefDir),
    ).rejects.toThrow();
  });

  it('rejects absolute file $ref outside localizedDir', async () => {
    // mkdtempSync — avoids CodeQL js/insecure-temporary-file (predictable tmp paths).
    const outsideDir = fs.mkdtempSync(path.join(os.tmpdir(), 'nexus-schema-abs-'));
    tempDirs.push(outsideDir);
    const outsideFile = path.join(outsideDir, 'outside.schema.json');
    fs.writeFileSync(outsideFile, JSON.stringify({ type: 'string' }));

    const { localizedDir, derefDir, schemaPaths } = makeLocalizedTree({
      'absolute.schema.json': {
        type: 'object',
        properties: {
          escaped: { $ref: outsideFile },
        },
      },
    });

    await expect(
      buildDereferencedSchemaTree(schemaPaths, localizedDir, derefDir),
    ).rejects.toThrow();
  });
});
