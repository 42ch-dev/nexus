#!/usr/bin/env node
/**
 * Actor/Character/ActorWorldBinding closed-schema fixtures (v1.184 P0 Task 1).
 *
 * Validates rejection of unknown discriminants, dual ids, extra properties,
 * malformed id prefixes/length, display-name bounds, and invalid metadata.
 */

const fs = require('fs');
const path = require('path');

const ROOT = path.join(__dirname, '..', '..');
const HEX32 = 'aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa';
const CHR = `chr_${HEX32}`;
const CTR = `ctr_${HEX32}`;
const AWB = `awb_${HEX32}`;
const WLD = `wld_${HEX32}`;
const TS = '2026-09-05T00:00:00Z';

function loadSchema(rel) {
  const abs = path.join(ROOT, rel);
  if (!fs.existsSync(abs)) {
    throw new Error(`missing schema: ${rel}`);
  }
  return JSON.parse(fs.readFileSync(abs, 'utf8'));
}

function resolveRef(ref, schemaCache) {
  const [id, fragment] = ref.split('#');
  const schema = schemaCache.get(id) || schemaCache.get(id.replace(/\/$/, ''));
  if (!schema) {
    throw new Error(`unresolved $ref: ${ref}`);
  }
  if (!fragment) {
    return schema;
  }
  const parts = fragment.replace(/^\//, '').split('/');
  let node = schema;
  for (const part of parts) {
    node = node[part];
    if (node === undefined) {
      throw new Error(`unresolved $ref fragment: ${ref}`);
    }
  }
  return node;
}

function matches(schema, data, cache) {
  if (schema.$ref) {
    return matches(resolveRef(schema.$ref, cache), data, cache);
  }
  if (schema.oneOf) {
    const hits = schema.oneOf.filter((arm) => matches(arm, data, cache));
    return hits.length === 1;
  }
  if (schema.const !== undefined) {
    return data === schema.const;
  }
  if (schema.enum) {
    return schema.enum.includes(data);
  }
  const types = Array.isArray(schema.type) ? schema.type : schema.type ? [schema.type] : [];
  if (types.includes('object')) {
    if (data === null || typeof data !== 'object' || Array.isArray(data)) {
      return false;
    }
    const required = schema.required || [];
    for (const key of required) {
      if (!(key in data)) {
        return false;
      }
    }
    if (schema.additionalProperties === false) {
      const allowed = new Set(Object.keys(schema.properties || {}));
      for (const key of Object.keys(data)) {
        if (!allowed.has(key)) {
          return false;
        }
      }
    }
    for (const [key, value] of Object.entries(data)) {
      const prop = schema.properties && schema.properties[key];
      if (prop && !matches(prop, value, cache)) {
        return false;
      }
    }
    return true;
  }
  if (types.includes('string')) {
    if (typeof data !== 'string') {
      return false;
    }
    const scalarCount = Array.from(data).length;
    if (schema.minLength !== undefined && scalarCount < schema.minLength) {
      return false;
    }
    if (schema.maxLength !== undefined && scalarCount > schema.maxLength) {
      return false;
    }
    if (Object.prototype.hasOwnProperty.call(schema, 'minLength') && data.trim() !== data) {
      return false;
    }
    if (schema.pattern && !new RegExp(schema.pattern).test(data)) {
      return false;
    }
    return true;
  }
  if (types.includes('integer')) {
    return Number.isInteger(data) && (schema.minimum === undefined || data >= schema.minimum);
  }
  return true;
}

function compile(rel, cache) {
  const schema = loadSchema(rel);
  if (schema.$id) {
    cache.set(schema.$id, schema);
  }
  return (data) => matches(schema, data, cache);
}

function assertReject(validate, data, label) {
  if (validate(data)) {
    throw new Error(`expected rejection: ${label}`);
  }
}

function assertAccept(validate, data, label) {
  if (!validate(data)) {
    throw new Error(`expected accept: ${label}`);
  }
}

function main() {
  const cache = new Map();
  const common = loadSchema('schemas/common/common.schema.json');
  cache.set(common.$id, common);

  const actor = compile('schemas/domain/actor-ref.schema.json', cache);
  const character = compile('schemas/domain/character.schema.json', cache);
  const binding = compile('schemas/domain/actor-world-binding.schema.json', cache);
  const createReq = compile('schemas/daemon-api/characters/create-character-request.schema.json', cache);

  assertAccept(actor, { actor_kind: 'creator', creator_id: CTR }, 'creator actor');
  assertAccept(actor, { actor_kind: 'character', character_id: CHR }, 'character actor');
  assertReject(actor, { actor_kind: 'npc', creator_id: CTR }, 'unknown discriminant');
  assertReject(actor, { actor_kind: 'creator', creator_id: CTR, character_id: CHR }, 'dual ids');
  assertReject(actor, { actor_kind: 'character', character_id: CHR, extra: true }, 'actor extra properties');
  assertReject(actor, { actor_kind: 'character', character_id: 'chr_nothex' }, 'malformed character id');
  assertReject(actor, { actor_kind: 'creator', creator_id: `CTR_${HEX32}` }, 'uppercase creator prefix');
  assertReject(actor, { actor_kind: 'character', character_id: `chr_${HEX32.slice(0, 31)}` }, 'short character id');

  const validCharacter = {
    schema_version: 1,
    character_id: CHR,
    owner_creator_id: CTR,
    display_name: 'Ada',
    status: 'active',
    persona: {},
    created_at: TS,
    updated_at: TS,
  };
  assertAccept(character, validCharacter, 'character');
  assertReject(character, { ...validCharacter, extra: 1 }, 'character extra properties');
  assertReject(character, { ...validCharacter, display_name: '' }, 'empty display name');
  assertReject(character, { ...validCharacter, display_name: 'a'.repeat(121) }, 'display name too long');
  assertAccept(character, { ...validCharacter, display_name: '你'.repeat(120) }, '120 CJK scalars');
  assertReject(character, { ...validCharacter, display_name: '你'.repeat(121) }, '121 CJK scalars');
  assertReject(character, { ...validCharacter, display_name: ' Ada' }, 'leading whitespace');
  assertReject(character, { ...validCharacter, display_name: 'Ada ' }, 'trailing whitespace');
  assertReject(character, { ...validCharacter, display_name: '   ' }, 'whitespace only');
  assertReject(character, { ...validCharacter, persona: 'not-an-object' }, 'invalid persona metadata');
  assertReject(character, { ...validCharacter, character_id: 'chr_ABCDEF' }, 'uppercase hex id');

  const validBinding = {
    schema_version: 1,
    binding_id: AWB,
    character_id: CHR,
    world_id: WLD,
    status: 'active',
    created_at: TS,
    updated_at: TS,
  };
  assertAccept(binding, validBinding, 'binding');
  assertReject(binding, { ...validBinding, extra: true }, 'binding extra properties');
  assertReject(binding, { ...validBinding, binding_id: 'awb_short' }, 'malformed binding id');
  assertReject(binding, { ...validBinding, status: 'archived' }, 'invalid binding status');

  assertAccept(createReq, { display_name: 'Ada', world_id: WLD }, 'create request');
  assertReject(
    createReq,
    { display_name: 'Ada', world_id: WLD, owner_creator_id: CTR },
    'create request ownership leak',
  );
  assertReject(createReq, { display_name: '', world_id: WLD }, 'create empty name');
  assertReject(createReq, { display_name: 'Ada', world_id: WLD, persona: [] }, 'create invalid persona');

  console.log('actor-contract-fixtures: all assertions passed');
}

try {
  main();
} catch (err) {
  console.error(`actor-contract-fixtures FAILED: ${err.message}`);
  process.exit(1);
}
