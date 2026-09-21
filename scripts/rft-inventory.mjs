/** Repository-specific RFT disposition collector. No product effects or network access. */
import { createHash } from 'node:crypto';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { readFile, readdir, writeFile, mkdir, mkdtemp, rm } from 'node:fs/promises';
import { dirname, join, resolve, relative } from 'node:path';
import { homedir, tmpdir } from 'node:os';
import { pathToFileURL } from 'node:url';

const exec = promisify(execFile);
const sorted = values => [...new Set(values)].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
const hash = value => createHash('sha256').update(value).digest('hex');
const text = path => readFile(path, 'utf8');

export function parseFlags(args) {
  const result = {};
  const names = { '--baseline': 'baseline', '--inventories': 'inventoriesDir', '--out': 'out' };
  for (let i = 0; i < args.length; i += 2) {
    const key = names[args[i]];
    if (!key || result[key] || args[i + 1]?.startsWith('--')) {
      throw new Error(`Invalid or duplicate argument: ${args[i]}`);
    }
    result[key] = args[i + 1];
  }
  if (Object.keys(result).length !== 3 || !/^[a-f0-9]{40}$/.test(result.baseline)) {
    throw new Error('Usage: node scripts/rft-inventory.mjs --baseline <40-character sha> --inventories <dir> --out <file>');
  }
  return result;
}

export function identity({ kind, source_path = '', symbol_or_route, feature_condition = 'all', alias_of = null }) {
  const symbol = Array.isArray(symbol_or_route) ? symbol_or_route.join(' ') : symbol_or_route.trim().replace(/\s+/g, ' ');
  const features = sorted(feature_condition.split('+').map(value => value.trim())).join('+');
  // Callable identity is a full clap path, never a variant name or leaf count.
  const source = ['callable', 'route'].includes(kind) ? '' : source_path;
  return `${kind}:${hash(JSON.stringify([source, symbol, features, alias_of])).slice(0, 24)}`;
}

async function filesUnder(directory, extension) {
  const files = [];
  for (const item of await readdir(directory, { withFileTypes: true })) {
    const path = join(directory, item.name);
    if (item.isDirectory()) files.push(...await filesUnder(path, extension));
    else if (path.endsWith(extension)) files.push(path);
  }
  return files.sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
}

/** Small lexical reader: balanced Rust groups, comments and raw strings are not regex-delimited blocks. */
export function rustTokens(source) {
  const tokens = [];
  let at = 0;
  while (at < source.length) {
    if (/\s/.test(source[at])) { at++; continue; }
    if (source.startsWith('//', at)) { const end = source.indexOf('\n', at); at = end < 0 ? source.length : end; continue; }
    if (source.startsWith('/*', at)) {
      let depth = 1; at += 2;
      while (depth && at < source.length) {
        if (source.startsWith('/*', at)) { depth++; at += 2; }
        else if (source.startsWith('*/', at)) { depth--; at += 2; }
        else at++;
      }
      if (depth) throw new Error('Unclosed Rust comment');
      continue;
    }
    const start = at;
    const raw = /^(?:br|r)(#*)"/.exec(source.slice(at));
    if (raw) {
      const end = source.indexOf(`"${raw[1]}`, at + raw[0].length);
      if (end < 0) throw new Error('Unclosed Rust raw string');
      at = end + raw[1].length + 1;
      tokens.push({ value: source.slice(start + raw[0].length, end), string: true, start, end: at });
      continue;
    }
    if (source[at] === '"' || (source[at] === "'" && /^(?:'[^'\\\n]'|'\\(?:.|u\{[^}]+\})')/.test(source.slice(at)))) {
      const quote = source[at++];
      while (at < source.length && source[at] !== quote) { if (source[at] === '\\') at++; at++; }
      if (at === source.length) throw new Error('Unclosed Rust string');
      at++;
      tokens.push({ value: source.slice(start + 1, at - 1), string: true, start, end: at });
      continue;
    }
    const word = /^[A-Za-z_][A-Za-z_0-9]*/.exec(source.slice(at));
    at += word ? word[0].length : source.startsWith('::', at) ? 2 : 1;
    tokens.push({ value: source.slice(start, at), start, end: at });
  }
  const stack = [];
  const pairs = { ')': '(', ']': '[', '}': '{' };
  for (let i = 0; i < tokens.length; i++) {
    const token = tokens[i];
    if (token.string) continue;
    if (['(', '[', '{'].includes(token.value)) stack.push(i);
    else if (pairs[token.value]) {
      const open = stack.pop();
      if (open === undefined || tokens[open].value !== pairs[token.value]) throw new Error(`Unbalanced Rust group at ${token.start}`);
      tokens[open].close = i;
    }
  }
  if (stack.length) throw new Error('Unclosed Rust group');
  return tokens;
}

const values = tokens => tokens.map(token => token.string ? JSON.stringify(token.value) : token.value).join(' ');

/** Evaluate cfg with test seams disabled; other build predicates remain unknown. */
function productionCfg(tokens, start, end) {
  const name = tokens[start]?.value;
  if (end - start === 1 && name === 'test') return false;
  if (end - start === 3 && name === 'feature' && tokens[start + 1].value === '=' && tokens[start + 2].string && tokens[start + 2].value === 'test-hooks') return false;
  if (tokens[start + 1]?.value !== '(' || tokens[start + 1].close !== end - 1) return undefined;
  if (name === 'not') {
    const value = productionCfg(tokens, start + 2, tokens[end - 2]?.value === ',' ? end - 2 : end - 1);
    return value === undefined ? undefined : !value;
  }
  if (name !== 'all' && name !== 'any') return undefined;
  let result = name === 'all';
  for (let i = start + 2; i < end - 1;) {
    let next = i;
    while (next < end - 1 && tokens[next].value !== ',') next = (tokens[next].close ?? next) + 1;
    const value = productionCfg(tokens, i, next);
    if ((name === 'all' && value === false) || (name === 'any' && value === true)) return value;
    if (value === undefined) result = undefined;
    i = next + 1;
  }
  return result;
}

/** Exclude test-only cfg items without dropping any possible production cohort. */
export function productionSource(source) {
  const tokens = rustTokens(source);
  const cuts = [];
  for (let i = 0; i < tokens.length; i++) {
    if (tokens[i].value !== '#' || tokens[i + 1]?.value !== '[') continue;
    const end = tokens[i + 1].close;
    if (tokens[i + 2]?.value !== 'cfg' || tokens[i + 3]?.value !== '(' || productionCfg(tokens, i + 4, end - 1) !== false) continue;
    let j = end + 1;
    while (tokens[j]?.value === '#') j = tokens[j + 1].close + 1;
    while (tokens[j] && !['{', ';', ','].includes(tokens[j].value)) j = (tokens[j].close ?? j) + 1;
    if (!tokens[j]) throw new Error('Cannot delimit test-only cfg item');
    const last = tokens[j].close ?? j;
    cuts.push([tokens[i].start, tokens[last].end]);
    i = last;
  }
  for (const [start, end] of cuts.reverse()) source = source.slice(0, start) + source.slice(start, end).replace(/[^\n]/g, ' ') + source.slice(end);
  return source;
}

function functions(source) {
  const tokens = rustTokens(source);
  const result = [];
  const owners = [];
  for (let i = 0; i < tokens.length; i++) {
    if (tokens[i].value !== 'impl') continue;
    let j = i + 1;
    while (tokens[j] && tokens[j].value !== '{') j++;
    if (!tokens[j]) continue;
    const header = source.slice(tokens[i].start, tokens[j].start);
    const owner = header.match(/^impl\s+(?:[\w:]+\s+for\s+)?([A-Z]\w*)/)?.[1];
    if (owner) owners.push({ owner, start: tokens[j].start, end: tokens[tokens[j].close].end });
  }
  for (let i = 0; i < tokens.length; i++) {
    if (tokens[i].value !== 'fn' || !/^[a-zA-Z_]\w*$/.test(tokens[i + 1]?.value)) continue;
    const name = tokens[i + 1].value;
    let j = i + 2;
    while (tokens[j] && !['{', ';'].includes(tokens[j].value)) j++;
    if (tokens[j]?.value !== '{') continue;
    const end = tokens[j].close;
    const owner = owners.find(item => item.start < tokens[i].start && item.end > tokens[i].start)?.owner;
    result.push({ name, symbol: owner ? `${owner}::${name}` : name, start: tokens[i].start, end: tokens[end].end, body: source.slice(tokens[j].start, tokens[end].end) });
  }
  return result;
}

/** Decode the actual clap_complete bash transition graph; never split encoded command keys on underscores. */
export function completionPaths(completion) {
  const edges = new Map();
  for (const match of completion.matchAll(/^\s+(nexus42[^,\s]*),([^\s)]+)\)\s*\n\s*cmd="([^"]+)"/gm)) {
    const list = edges.get(match[1]) ?? [];
    list.push({ name: match[2], key: match[3] });
    edges.set(match[1], list);
  }
  if (!edges.has('nexus42')) throw new Error('Clap completion transition format not recognized');
  const result = [];
  const walk = (key, path, ancestry) => {
    if (ancestry.includes(key)) throw new Error(`Cyclic clap completion key: ${key}`);
    for (const child of edges.get(key) ?? []) {
      // clap-generated help mirrors the entire tree, not another product command.
      if (child.name === 'help') continue;
      const next = [...path, child.name];
      result.push({ path: next, key: child.key, leaf: !(edges.get(child.key) ?? []).some(edge => edge.name !== 'help') });
      walk(child.key, next, [...ancestry, key]);
    }
  };
  walk('nexus42', ['nexus42'], []);
  return result;
}

export function expandMethods(expression) {
  const tokens = rustTokens(expression);
  const methods = [];
  for (let i = 0; i < tokens.length; i++) {
    if (!['get', 'post', 'put', 'patch', 'delete', 'head', 'options', 'trace'].includes(tokens[i].value) || tokens[i + 1]?.value !== '(') continue;
    const end = tokens[i + 1].close;
    methods.push({ method: tokens[i].value.toUpperCase(), handler: values(tokens.slice(i + 2, end)).replace(/\s*::\s*/g, '::') });
    i = end;
  }
  if (!methods.length) throw new Error(`Unresolved router method set: ${expression}`);
  return methods;
}

/** Production create_router is the root; only reachable merges and local router bindings count. */
export function routerRegistrations(source) {
  source = productionSource(source);
  const declared = new Map(functions(source).map(fn => [fn.name, fn]));
  const root = declared.get('create_router');
  if (!root) throw new Error('Missing create_router');
  const bindings = new Map();
  const tokens = rustTokens(root.body);
  for (let i = 0; i < tokens.length; i++) {
    if (tokens[i].value !== 'let' || tokens[i + 2]?.value !== '=') continue;
    let j = i + 3;
    while (tokens[j] && tokens[j].value !== ';') j = (tokens[j].close ?? j) + 1;
    bindings.set(tokens[i + 1].value, root.body.slice(tokens[i + 3].start, tokens[j].start));
    i = j;
  }
  const routes = [];
  const seen = new Set();
  const walk = (expression, caller, auth, prefix = '') => {
    if (expression.includes('require_active_creator')) auth = 'api-key+active-creator';
    else if (expression.includes('require_api_key')) auth = 'api-key';
    const ts = rustTokens(expression);
    for (let i = 0; i < ts.length; i++) {
      if (ts[i].value !== '.' || ts[i + 2]?.value !== '(') continue;
      const method = ts[i + 1].value;
      const end = ts[i + 2].close;
      const inner = ts.slice(i + 3, end);
      if (method === 'route') {
        if (!inner[0]?.string || inner[1]?.value !== ',') throw new Error(`Dynamic route in ${caller}`);
        const methodExpression = expression.slice(inner[2].start, ts[end].start);
        for (const entry of expandMethods(methodExpression)) routes.push({ ...entry, path: prefix + inner[0].value, caller, auth });
      } else if (method === 'merge' || method === 'nest') {
        let child = inner;
        let childPrefix = prefix;
        if (method === 'nest') {
          if (!inner[0]?.string || inner[1]?.value !== ',') throw new Error(`Dynamic nest in ${caller}`);
          childPrefix += inner[0].value;
          child = inner.slice(2);
        }
        const name = child[0]?.value;
        const childExpression = expression.slice(child[0].start, ts[end].start);
        const target = name === 'Router' ? childExpression : bindings.get(name) ?? declared.get(name)?.body;
        if (!target) throw new Error(`Unresolved router merge: ${name}`);
        const key = `${name}:${childPrefix}:${auth}:${values(child)}`;
        if (!seen.has(key)) {
          seen.add(key);
          const childAuth = childExpression.includes('require_active_creator') ? 'api-key+active-creator' : auth;
          walk(target, name, childAuth, childPrefix);
        }
      }
      i = end;
    }
  };
  // The final binding composes runtime_routes and protected_routes. The release
  // fallback shadows it later but does not unregister either lane.
  const composition = root.body.match(/let router = Router::new\(\)[\s\S]*?;/)?.[0];
  if (!composition) throw new Error('Unresolved final router composition');
  walk(composition, 'create_router', 'unguarded');
  return routes;
}

function splitItems(source) {
  const tokens = rustTokens(source);
  const result = [];
  let start = 0;
  for (let i = 0; i < tokens.length; i++) {
    if (tokens[i].close !== undefined) i = tokens[i].close;
    else if (tokens[i].value === ',') {
      result.push(source.slice(start, tokens[i].start));
      start = tokens[i].end;
    }
  }
  if (source.slice(start).trim()) result.push(source.slice(start));
  return result;
}

function attributes(item) {
  const tokens = rustTokens(item);
  const attrs = [];
  let i = 0;
  while (tokens[i]?.value === '#' && tokens[i + 1]?.value === '[') {
    const end = tokens[i + 1].close;
    attrs.push(values(tokens.slice(i + 2, end)));
    i = end + 1;
  }
  return { attrs: attrs.join(' '), body: item.slice(tokens[i]?.start ?? item.length) };
}

const kebab = name => name.replace(/([a-z0-9])([A-Z])/g, '$1-$2').replace(/([A-Z])([A-Z][a-z])/g, '$1-$2').toLowerCase();

/** Source annotations are reconciled with the executable clap tree, not used as a replacement tree. */
export function declaredClapPaths(sources) {
  const types = [];
  for (const [path, original] of sources) {
    const source = productionSource(original);
    const tokens = rustTokens(source);
    for (let i = 0; i < tokens.length; i++) {
      if (!['enum', 'struct'].includes(tokens[i].value)) continue;
      const name = tokens[i + 1]?.value;
      let j = i + 2;
      while (tokens[j] && !['{', ';'].includes(tokens[j].value)) j++;
      if (tokens[j]?.value !== '{') continue;
      const end = tokens[j].close;
      types.push({ name, kind: tokens[i].value, path, body: source.slice(tokens[j].end, tokens[end].start), offset: tokens[i].start });
      i = end;
    }
  }
  const resolveType = (ref, parent) => {
    const name = ref.split('::').at(-1);
    let candidates = types.filter(type => type.name === name);
    const sameFile = candidates.filter(type => type.path === parent.path);
    if (sameFile.length === 1) return sameFile[0];
    const qualifier = ref.split('::').slice(0, -1).join('/');
    if (qualifier) {
      const moduleDir = parent.path.endsWith('/mod.rs') ? dirname(parent.path) : parent.path.replace(/\.rs$/, '');
      const local = candidates.filter(type => type.path === `${moduleDir}/${qualifier}.rs` || type.path === `${moduleDir}/${qualifier}/mod.rs`);
      if (local.length === 1) return local[0];
      const matched = candidates.filter(type => Boolean(type.path.endsWith(`/${qualifier}.rs`)) || type.path.endsWith(`/${qualifier}/mod.rs`));
      if (matched.length) candidates = matched;
    }
    if (candidates.length !== 1) throw new Error(`Unresolved clap type ${parent.path}:${ref} (${candidates.map(type => type.path).join(', ')})`);
    return candidates[0];
  };
  const root = types.find(type => type.path.endsWith('/cli.rs') && type.name === 'Commands');
  if (!root) throw new Error('Missing Commands declaration');
  const result = [];
  const descend = (type, path, inherited, seen) => {
    if (seen.includes(type)) throw new Error(`Cyclic clap type ${type.name}`);
    const fields = splitItems(type.body);
    if (type.kind === 'struct') {
      for (const field of fields) {
        const { attrs, body } = attributes(field);
        if (!/command \( (?:subcommand|flatten)/.test(attrs)) continue;
        const ref = body.match(/:\s*(?:(?:Option|Box)<)*([\w:]+)/)?.[1];
        if (!ref) throw new Error(`Unresolved clap field ${type.path}:${body}`);
        descend(resolveType(ref, type), path, inherited, [...seen, type]);
      }
      return;
    }
    for (const field of fields) {
      const { attrs, body } = attributes(field);
      if (/command \( skip/.test(attrs)) continue;
      const name = body.match(/^\s*(\w+)/)?.[1];
      if (!name) throw new Error(`Unresolved clap variant ${type.path}:${body}`);
      const command = attrs.match(/\bname = "([^"]+)"/)?.[1] ?? kebab(name);
      const aliases = [...attrs.matchAll(/\b(?:visible_alias|alias) = "([^"]+)"/g)].map(match => match[1]);
      const features = [...attrs.matchAll(/feature = "([^"]+)"/g)].map(match => match[1]);
      const condition = sorted([...inherited.features, ...features]);
      const hidden = Boolean(inherited.hidden) || /hide = true/.test(attrs);
      const variants = [{ name: command, alias: null }, ...aliases.map(alias => ({ name: alias, alias: [...path, command].join(' ') }))];
      for (const variant of variants) {
        const next = [...path, variant.name];
        const item = { path: next, source_path: type.path, symbol: `${type.name}::${name}`, feature_condition: condition.join('+') || 'all', hidden, alias_of: variant.alias ?? inherited.alias_of, leaf: true };
        result.push(item);
        const brace = body.indexOf('{');
        const tuple = body.match(/^\s*\w+\s*\(\s*([\w:]+)\s*\)/);
        const before = result.length;
        if (tuple) {
          descend(resolveType(tuple[1], type), next, { features: condition, hidden, alias_of: item.alias_of }, [...seen, type]);
        } else if (brace >= 0) {
          const inner = body.slice(brace + 1, body.lastIndexOf('}'));
          for (const nested of splitItems(inner)) {
            const spec = attributes(nested);
            if (!/command \( (?:subcommand|flatten)/.test(spec.attrs)) continue;
            const ref = spec.body.match(/:\s*(?:(?:Option|Box)<)*([\w:]+)/)?.[1];
            if (!ref) throw new Error(`Unresolved clap field ${type.path}:${nested}`);
            descend(resolveType(ref, type), next, { features: condition, hidden, alias_of: item.alias_of }, [...seen, type]);
          }
        }
        item.leaf = result.length === before;
      }
    }
  };
  descend(root, ['nexus42'], { features: [], hidden: false, alias_of: null }, []);
  return result;
}

const TABLE_CLASSES = {
  'direct-authoring': `actor_world_bindings auth_tokens character_memory_fragments character_memory_pending_review character_soul_meta character_soul_narratives characters creators findings inspiration_items kb_key_blocks kb_relationships kb_source_anchors knowledge_entries local_identities memory_fragments memory_pending_review memory_soul_narratives mind_states moment_directive_chapter_anchors moment_directives narrative_timeline_events narrative_worlds novel_pool_entries outbox outbox_entries partial_apply_states peer_hosts reading_annotations reading_progress reference_sources soul_meta spoke_rules work_chapters works world_findings world_stories`,
  'engine-owned': `acp_sessions acp_tool_audit_log character_run_captures compute_sessions core_context_versions creator_prompt_injections creator_schedules force_gates_audit kb_extract_jobs orchestration_sessions schedule_dependencies works_idempotency workspace_commit_intents workspace_sessions`,
  system: 'workspace_meta sqlite_sequence sqlite_schema sqlite_master',
  migration: '_sqlx_migrations',
  protocol: 'core_workspace_gate core_writer_registration core_changes',
};

export function classifyTable(name) {
  return Object.entries(TABLE_CLASSES).find(([, names]) => names.split(/\s+/).includes(name))?.[0] ?? null;
}

export function classifyWriter(path, symbol) {
  if (['open_pool_read_only', 'open_workspace_pool_read_only', 'preflight_existing_workspace'].includes(symbol)) return 'read-only';
  if (path === 'crates/nexus-local-db/src/lib.rs' && ['open_pool', 'init_pool', 'run_migrations', 'apply_pending_migrations', 'apply_fk_suspension_migration', 'apply_fk_suspension_tx'].includes(symbol)) return 'guarded';
  if (path === 'crates/nexus-cloud-sync/src/pool.rs' && symbol === 'new') return 'guarded';
  return null;
}

function destination(kind, path, symbol) {
  symbol = symbol.replace('/v1/daemon/', '/');
  if (kind === 'writer' || kind === 'table' || kind === 'migration') return 'RFT-01';
  if (kind === 'external-consumer' || kind === 'dormant-route') return 'RFT-11';
  if (kind === 'delivery' || kind === 'report-evidence') {
    if (/release|publish/.test(path + symbol)) return 'RFT-10';
    if (/desktop/.test(path + symbol)) return 'RFT-09';
    return 'RFT-00';
  }
  if (kind === 'provider' || /nexus-acp-host/.test(path)) return 'RFT-02';
  if (/nexus42 creator world kb (graph|entity patch)$/.test(symbol) || /\/kb\/(graph|patch-entity|candidates)$/.test(symbol)) return 'RFT-01';
  if (symbol.startsWith('nexus42 creator works ') && /\b(list|status|use)$/.test(symbol)) return 'RFT-08';
  if (/character|actor|memory|soul|moment.directive|context.assemble/.test(path + symbol)) return 'RFT-06';
  if (/nexus42 (system|connect|platform|sync)|nexus-runtime/.test(symbol)) return 'RFT-08';
  if (/desktop/.test(path + symbol)) return 'RFT-09';
  if (/daemon|schedule|host|acp|mcp|ops|preset|capabilit|compute|orchestration|bootstrap|creator run/.test(symbol) || ['builtin', 'capability', 'boot', 'task', 'middleware', 'stream'].includes(kind)) return 'RFT-07';
  if (/creator|world|work|kb|knowledge|narrative|reading|reference|timeline|strateg/.test(symbol)) return 'RFT-05';
  if (kind === 'feature') return 'RFT-08';
  if (kind === 'route' || kind === 'schema-operation') return 'RFT-07';
  return null;
}

function callableAlias(symbol, alias) {
  if (alias) return alias;
  if (/^nexus42 sync(?: |$)/.test(symbol)) return symbol.replace('nexus42 sync', 'nexus42 platform sync');
  return null;
}

export function manifestFeatures(manifest) {
  const block = manifest.split(/^\[features\][ \t]*$/m)[1]?.split(/^\[/m)[0];
  if (!block) throw new Error('Missing CLI feature declaration');
  return [...block.matchAll(/^([\w-]+)\s*=\s*(\[[\s\S]*?\])/gm)].map(match => ({ name: match[1], definition: match[2] }));
}

function row(kind, source_path, symbol_or_route, extra = {}) {
  const destination_rft = extra.destination_rft ?? destination(kind, source_path, symbol_or_route);
  const current_host = source_path.startsWith('apps/nexus42/') ? 'nexus42' : source_path.split('/')[1] ?? 'repository';
  const value = {
    kind, source_path, symbol_or_route, feature_condition: 'all',
    support_state: 'retained', callers: [], schema_refs: [], storage_tables: [],
    effect_owner: current_host, current_host, destination_rft,
    implementation_owner: `${destination_rft ?? 'UNCLASSIFIED'} family implementer`,
    parity_scenario: `Preserve ${symbol_or_route} inputs, outputs, authorization, errors and effects`,
    retirement_owner: 'RFT-11',
    retirement_gate: `Keep current surface until ${destination_rft ?? 'classified replacement'} parity and all caller migration are proven; RFT-11 removal decision required`,
    evidence_class: 'source-registration', gap: null,
    ...extra,
  };
  value.id = identity(value);
  return value;
}

/** Follow declared modules, not every .rs file (standalone test modules are not production). */
async function moduleClosure(root, entries) {
  const sources = new Map();
  const visit = async (path, moduleDir) => {
    if (sources.has(path)) return;
    const source = productionSource(await text(join(root, path)));
    sources.set(path, source);
    const tokens = rustTokens(source);
    for (let i = 0; i < tokens.length; i++) {
      if (tokens[i].value !== 'mod' || tokens[i + 2]?.value !== ';') continue;
      const name = tokens[i + 1].value;
      const explicit = source.slice(Math.max(0, tokens[i].start - 180), tokens[i].start).match(/#\[path\s*=\s*"([^"]+)"\]\s*(?:pub\s*)?$/)?.[1];
      const candidates = explicit ? [join(dirname(path), explicit)] : [join(moduleDir, `${name}.rs`), join(moduleDir, name, 'mod.rs')];
      let selected;
      for (const candidate of candidates) {
        try { await readFile(join(root, candidate)); selected = candidate; break; }
        catch (error) { if (error.code !== 'ENOENT') throw error; }
      }
      if (!selected) throw new Error(`Missing registered module ${path}:${name}`);
      await visit(selected, selected.endsWith('/mod.rs') ? dirname(selected) : selected.replace(/\.rs$/, ''));
    }
  };
  for (const entry of entries) await visit(entry, dirname(entry));
  return sources;
}

function sqlTables(source) {
  const result = [];
  for (const token of rustTokens(source)) {
    if (!token.string) continue;
    const sql = token.value.replace(/--[^\n]*/g, '');
    for (const match of sql.matchAll(/\b(?:INSERT(?:\s+OR\s+\w+)?\s+INTO|REPLACE\s+INTO|UPDATE|DELETE\s+FROM|FROM|JOIN)\s+["`]?([a-z_][a-z_0-9]*)/gi)) {
      // A SQL string, not arbitrary diagnostic prose containing “from …”.
      if (/^\s*(?:SELECT|INSERT|UPDATE|DELETE|REPLACE|WITH|CREATE|ALTER|DROP)\b/i.test(sql)) result.push(match[1]);
    }
  }
  return sorted(result);
}

export function summarize(rows, expected = [], discrepancies = []) {
  const ids = new Set(rows.map(value => value.id));
  const registered = new Set(expected);
  return {
    missing_registrations: sorted([...registered].filter(id => !ids.has(id))),
    extra_unregistered: rows
      .filter(value => value.kind === 'dormant-route' || (value.kind === 'callable' && value.declaration && !registered.has(value.id)))
      .map(value => ({ id: value.id, kind: value.kind === 'dormant-route' ? 'dormant-route' : 'clap-extra', disposition: value.retirement_gate })),
    unknown_production_writer_table: rows.filter(value => (value.kind === 'writer' && !value.writer_class) || (value.kind === 'table' && !value.table_class)).map(value => value.id),
    unknown_destination: rows.filter(value => !/^RFT-(?:0[0-9]|1[01])$/.test(value.destination_rft)).map(value => value.id),
    unresolved_m1_schema_auth: rows.filter(value => value.m1 && (value.gap !== null || !value.schema_refs.length || !value.auth)).map(value => value.id),
    blockers: discrepancies.filter(value => value.severity === 'STOP'),
  };
}

async function executableCli(root, declarations, provenance) {
  const target = process.env.CARGO_TARGET_DIR ?? join(process.env.XDG_CACHE_HOME ?? join(homedir(), '.cache'), 'nexus-target');
  const tempHome = await mkdtemp(join(tmpdir(), 'rft-inventory-'));
  const cohorts = [];
  try {
    for (const features of [[], ['connect-host', 'connect-client']]) {
      const args = ['build', '--offline', '--locked', '-p', 'nexus42', '--bin', 'nexus42'];
      if (features.length) args.push('--features', features.join(','));
      const build = await exec('cargo', args, { cwd: root, env: { ...process.env, CARGO_TARGET_DIR: target, SQLX_OFFLINE: 'true' }, maxBuffer: 16 * 1024 * 1024 });
      const binary = join(target, 'debug', process.platform === 'win32' ? 'nexus42.exe' : 'nexus42');
      const result = await exec(binary, ['system', 'completion', 'bash'], {
        cwd: root, env: { ...process.env, NEXUS42_HOME: tempHome, NO_COLOR: '1' }, maxBuffer: 16 * 1024 * 1024,
      });
      const paths = completionPaths(result.stdout);
      const declared = declarations.filter(item => item.feature_condition === 'all' || item.feature_condition.split('+').every(feature => features.includes(feature)));
      const actualSet = new Set(paths.map(item => item.path.join(' ')));
      const declaredSet = new Set(declared.map(item => item.path.join(' ')));
      const registered_ids = paths.map(item => {
        const annotation = declared.find(candidate => candidate.path.join(' ') === item.path.join(' '));
        return identity({ kind: 'callable', symbol_or_route: item.path, feature_condition: annotation?.feature_condition ?? features.join('+'), alias_of: callableAlias(item.path.join(' '), annotation?.alias_of) });
      });
      cohorts.push({
        features: features.length ? features.join('+') : 'default',
        registered_ids,
        missing: sorted([...actualSet].filter(key => !declaredSet.has(key))),
        extra: sorted([...declaredSet].filter(key => !actualSet.has(key))),
        leaf_count_including_aliases: paths.filter(item => item.leaf).length,
        command_count: paths.length, completion_sha256: hash(result.stdout),
      });
      provenance.push({ command: ['cargo', ...args], binary_sha256: hash(await readFile(binary)), completion_command: ['nexus42', 'system', 'completion', 'bash'], build_warnings: build.stderr.split('\n').filter(line => /^warning:/.test(line)) });
    }
  } finally {
    await rm(tempHome, { recursive: true, force: true });
  }
  return cohorts;
}

export async function collectInventory({ baseline, inventoriesDir }) {
  const root = process.cwd();
  if (!/^[a-f0-9]{40}$/.test(baseline)) throw new Error('baseline must be a full commit SHA');
  await exec('git', ['merge-base', '--is-ancestor', baseline, 'HEAD'], { cwd: root });
  const provenance = [];
  const discrepancies = [];
  const rows = [];
  const expected = [];
  const add = value => rows.push(value);
  const note = (id, detail, severity = 'classified') => discrepancies.push({ id, severity, detail });
  for (const name of ['RftCliInventory', 'RftServiceInventory', 'RftDeliveryInventory']) {
    const path = join(inventoriesDir, `${name}.json`);
    const source = await text(path);
    const report = JSON.parse(source);
    if (!Array.isArray(report.files) || typeof report.report !== 'string') throw new Error(`Invalid static inventory: ${path}`);
    provenance.push({ input: `${name}.json`, sha256: hash(source), file_rows: report.files.length });
    // Preserve every prose row. Historical destinations are evidence, not the locked vocabulary.
    for (const [index, claim] of report.report.split(/\n\n+/).entries()) {
      add(row('report-evidence', `${name}.json`, `paragraph:${index + 1}`, { evidence_class: 'scout-snapshot-not-exhaustive', support_state: 'historical-evidence', claim, retirement_gate: 'Evidence only; current declarations and rust-core-service-boundary §7.5 override historical counts/destinations' }));
    }
    for (const entry of report.files) add(row('delivery', entry.path, entry.description, { evidence_class: 'scout-source-reference', support_state: 'retained-evidence' }));
  }

  const metadata = JSON.parse((await exec('cargo', ['metadata', '--offline', '--locked', '--no-deps', '--format-version', '1'], { cwd: root, maxBuffer: 16 * 1024 * 1024 })).stdout);
  const packages = new Map(metadata.packages.map(pkg => [pkg.name, pkg]));
  const dependencyClosure = new Map();
  const follow = name => {
    if (dependencyClosure.has(name) || !packages.has(name)) return;
    const pkg = packages.get(name);
    dependencyClosure.set(name, pkg);
    for (const dep of pkg.dependencies.filter(dep => dep.kind !== 'dev' && dep.path)) follow(dep.name);
  };
  follow('nexus42');
  const entries = [...dependencyClosure.values()].flatMap(pkg => pkg.targets.filter(target => target.kind.includes('lib')).map(target => relative(root, target.src_path)));
  entries.push('apps/nexus42/src/main.rs', 'apps/nexus42/src/bin/nexus-runtime.rs');
  const sources = await moduleClosure(root, entries);
  const declarations = declaredClapPaths(new Map([...sources].filter(([path]) => path.startsWith('apps/nexus42/'))));
  const cohorts = await executableCli(root, declarations, provenance);
  for (const cohort of cohorts) {
    expected.push(...cohort.registered_ids);
    for (const missing of cohort.missing) note(`clap-missing:${cohort.features}:${missing}`, 'Executable clap registration lacks source annotation', 'STOP');
    for (const extra of cohort.extra) note(`clap-extra:${cohort.features}:${extra}`, 'Source declaration absent from executable clap cohort', 'STOP');
  }
  const deferred = /nexus42 (?:creator workspace (?:clone|link|unlink|status)|creator works (?:start|create)|platform (?:explore (?:browse|search)|context assemble|publish)|(?:platform )?sync retry)$/;
  for (const item of declarations) {
    const symbol = item.path.join(' ');
    const compatibility = callableAlias(symbol, item.alias_of);
    add(row('callable', item.source_path, symbol, {
      feature_condition: item.feature_condition, alias_of: compatibility,
      declaration: `${item.source_path}:${item.symbol}`, leaf: item.leaf,
      support_state: deferred.test(symbol) ? 'deferred-or-rejected' : compatibility ? 'retained-alias' : item.hidden ? 'hidden-callable' : item.feature_condition !== 'all' ? 'feature-gated' : 'supported',
      callers: ['apps/nexus42/src/main.rs:main', ...(compatibility ? [compatibility] : [])],
      evidence_class: 'executable-clap+source-declaration',
      retirement_gate: deferred.test(symbol) ? 'RFT-11 explicit stub/deprecation decision and help/docs parity; keep current rejection behavior' : `Preserve exact clap path, flags, feature and alias relation; ${destination('callable', item.source_path, symbol)} parity before RFT-11 deletion`,
    }));
  }
  add(row('callable', 'apps/nexus42/src/bin/nexus-runtime.rs', 'nexus-runtime', { feature_condition: 'connect-host', callers: ['headless-integrators'], support_state: 'feature-gated', parity_scenario: 'Preserve --listen/--allow-peer/--home, stdout readiness and Connect-only SIGINT shutdown; no HTTP/SPA/Host' }));
  add(row('feature', 'apps/nexus42/src/commands/creator/run.rs', 'creator run <preset_id>: runtime-resolved preset manifest', { destination_rft: 'RFT-07', support_state: 'dynamic-registry', retirement_gate: 'Keep embedded/user/system manifest resolution and dynamic args; never invent a finite preset allowlist' }));
  note('creator-identity-count', 'Scout labels six but lists eight identity actions; declarations retain register/use/list/status/pair/unpair/credentials rotate/logout. demo-seed is a separate maintenance leaf.');
  note('schedule-count', `The 13-leaf schedule prose described the retired nexus42 daemon group; that group and its declarations were deleted in v1.193 P2 (current declarations: ${declarations.filter(item => item.leaf && item.path.slice(0, 3).join(' ') === 'nexus42 daemon schedule').length}). Exact paths, not 252/247 totals, determine coverage.`);
  note('wrappers-and-presets', 'Args wrappers and unregistered KbDaemonCommand do not add public paths; cfg(test) parser wrappers are excluded. creator run is one dynamic manifest entry, not a hardcoded preset tree.');
  note('historical-destinations', 'Scouts assign contradictory historical RFT keys. All current rows use rust-core-service-boundary §7.5; Works list/status/use remain RFT-08 basic reads.');

  // v1.193 P2 retired the entire Rust daemon composition: the
  // `nexus-daemon-runtime` crate (its `create_router` route table, handlers,
  // middleware, capability registry and boot tasks), the app-only
  // `basic-cli` / `legacy-cli` / `web-embed` / `connect-client` /
  // `embedded-mcp` selectors and the `nexus42 daemon` command group. Those
  // rows can no longer be collected from this tree; the retained surfaces are
  // the direct-core CLI, the Connect-only `nexus-runtime` and the core
  // library MCP/peer features. Historical route/handler rows stay in the
  // baseline inventories' evidence.
  note('daemon-surface-retired', 'Rust daemon route/handler/middleware/capability/boot rows were removed with the nexus-daemon-runtime crate in v1.193 P2; the executable clap cohorts and the retained schema declarations are the current boundary SSOT.');
  add(row('external-consumer', 'packages/nexus-contracts/package.json', 'external-consumer-unknown', { support_state: 'external-consumer-unknown', retirement_gate: 'Third-party consumers of @42ch/nexus-contracts must be identified before any wire/semver break; the local HTTP host has been the Electron/TS service since v1.193 P2' }));

  const migrations = await filesUnder(join(root, 'crates/nexus-local-db/migrations'), '.sql');
  const tables = new Map();
  const historical = new Map();
  for (const path of migrations) {
    const source = (await text(path)).replace(/--[^\n]*/g, '');
    for (const match of source.matchAll(/\b(CREATE\s+(?:VIRTUAL\s+)?TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?|DROP\s+TABLE\s+(?:IF\s+EXISTS\s+)?|ALTER\s+TABLE\s+)["`]?(\w+)(?:["`]?\s+RENAME\s+TO\s+["`]?(\w+))?/gi)) {
      const [, op, name, rename] = match;
      if (/^CREATE/i.test(op)) { tables.set(name, relative(root, path)); historical.set(name, relative(root, path)); }
      else if (/^DROP/i.test(op)) tables.delete(name);
      else if (rename) { tables.delete(name); tables.set(rename, relative(root, path)); }
    }
  }
  const tableCallers = new Map([...tables].map(([name]) => [name, []]));
  const constructors = [];
  for (const [path, source] of sources) {
    const fns = functions(source);
    for (const fn of fns) {
      for (const name of sqlTables(fn.body)) if (tableCallers.has(name)) tableCallers.get(name).push(`${path}:${fn.symbol}`);
      const tokens = rustTokens(fn.body);
      const code = values(tokens.filter(token => !token.string));
      const rawFactory = /SqlitePoolOptions\s*::\s*new|SqliteConnection\s*::\s*(?:connect|connect_with)|SqlitePool\s*::\s*(?:connect|connect_with)/.test(code);
      const factoryCalls = [...code.matchAll(/\b(open_pool_read_only|open_pool|init_pool|run_migrations|(?:Schema\s*::\s*init)|(?:DbPool\s*::\s*(?:new|with_defaults))|(?:OutboxPool\s*::\s*new))\s*\(/g)].map(match => match[1].replace(/\s*::\s*/g, '::'));
      if (rawFactory) constructors.push({ path, fn });
      if (factoryCalls.length) add(row('migration', path, fn.symbol, { calls: factoryCalls, callers: [`${path}:${fn.symbol}`], storage_tables: [...tables.keys()].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0)), evidence_class: 'source-connection-and-migration-callsite', writer_class: factoryCalls.every(name => name === 'open_pool_read_only') ? 'read-only' : 'guarded', parity_scenario: 'P1 registers this existing connection/migration path; global nonworkspace DB is distinct from canonical workspace activation' }));
      for (const token of tokens.filter(item => item.string)) {
        for (const match of token.value.matchAll(/\bCREATE\s+(?:VIRTUAL\s+)?TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?["`]?(\w+)/gi)) {
          if (!tables.has(match[1])) {
            tables.set(match[1], `${path}:${fn.symbol}`);
            tableCallers.set(match[1], [`${path}:${fn.symbol}`]);
          }
        }
      }
    }
  }
  expected.push(...constructors.map(({ path, fn }) => identity({ kind: 'writer', source_path: path, symbol_or_route: fn.symbol })));
  expected.push(...[...tables].map(([name, path]) => identity({ kind: 'table', source_path: path.split(':')[0], symbol_or_route: name })));
  for (const { path, fn } of constructors) {
    const writer_class = classifyWriter(path, fn.name);
    const callers = rows.filter(value => value.kind === 'migration' && value.calls?.includes(fn.symbol)).map(value => `${value.source_path}:${value.symbol_or_route}`);
    add(row('writer', path, fn.symbol, { writer_class, support_state: writer_class === 'read-only' ? 'read-only' : 'current-raw-connection', storage_tables: [...tables.keys()].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0)), callers: sorted(callers), evidence_class: 'source-constructor', parity_scenario: writer_class === 'read-only' ? 'Read-only open performs no migration or registration upgrade' : 'P1 must install common guards on this existing raw pool factory, not only new core callers', gap: writer_class ? null : 'Unclassified production connection constructor' }));
    if (!writer_class) note(`writer:${path}:${fn.symbol}`, 'Activated writer cannot be classified; P1 activation must STOP', 'STOP');
  }
  for (const [name, path] of tables) {
    const table_class = classifyTable(name);
    add(row('table', path.split(':')[0], name, { table_class, guard_required: !name.startsWith('sqlite_') && table_class !== 'migration', storage_tables: [name], callers: sorted(tableCallers.get(name) ?? []), evidence_class: 'migration+production-SQL', support_state: 'persistent', effect_owner: table_class === 'engine-owned' ? 'single workspace engine owner' : 'authorized stored-context author / existing storage owner', parity_scenario: `${table_class ?? 'UNCLASSIFIED'} admission; preserve existing direct callers and migration semantics` }));
    if (!table_class) note(`table:${name}`, `Persistent production table from ${path} is unclassified; P1 activation must STOP`, 'STOP');
  }
  add(row('table', 'crates/nexus-local-db/src/lib.rs', '_sqlx_migrations', { table_class: 'migration', storage_tables: ['_sqlx_migrations'], callers: ['apply_pending_migrations:ensure_migrations_table'], evidence_class: 'sqlx-migration-protocol', effect_owner: 'exclusive migration owner' }));
  note('cloud-outbox-writer', 'Dependency closure reveals nexus-cloud-sync::OutboxPool::new (pool.rs), called by Outbox::init_pool_with_schema (outbox.rs). It accepts a caller-selected DB and runs shared local-db migrations; classify guarded/workspace-capable, never exempt as automatically nonworkspace.');
  note('migration-rebuild-tables', `Historical rebuild names are not persistent final tables: ${[...historical.keys()].filter(name => !tables.has(name)).sort((a, b) => (a < b ? -1 : a > b ? 1 : 0)).join(', ')}. Applied CREATE/DROP/RENAME order, not a CREATE count.`);
  note('writer-guard-status', 'guarded is the required P1 admission classification, NOT a claim guards already exist. open_pool and DbPool::new are raw today; read-only constructors stay read-only. No production writer may be exempted on P1 activation.');

  const builtinPath = 'crates/nexus-orchestration/src/capability/mod.rs';
  const builtinSource = functions(sources.get(builtinPath)).find(fn => fn.name === 'with_builtins')?.body;
  if (!builtinSource) throw new Error('Missing with_builtins');
  for (const name of sorted([...builtinSource.matchAll(/Box::new\(builtins::(\w+)/g)].map(match => match[1]))) add(row('builtin', builtinPath, name, { callers: ['with_builtins', 'build_with_narrative_compute'], support_state: 'retained-pool-bound-or-explicit-unavailable', effect_owner: 'single orchestration coordinator', retirement_gate: 'Preserve pool/runtime dependency injection; pool-less validation is not execution success' }));
  const providerPath = 'crates/nexus-agent-host/src/providers/mod.rs';
  const providerSource = sources.get(providerPath);
  for (const name of sorted([...providerSource.matchAll(/"(dsh-native|codex-native|claude-native|acp)"/g)].map(match => match[1]))) add(row('provider', providerPath, name, { callers: ['adapter_from_catalog_entry'], effect_owner: 'HostManager + selected ProviderAdapter', parity_scenario: name === 'dsh-native' ? 'Complete-message streaming; cancellation:false; retained cleanup, no fabricated cancel support' : 'Actual provider streaming/cancel/terminal/close parity with no paid calls' }));
  const manifestPath = 'apps/nexus42/Cargo.toml';
  const manifest = await text(join(root, manifestPath));
  for (const feature of manifestFeatures(manifest)) add(row('feature', manifestPath, feature.name, { feature_condition: feature.name, definition: feature.definition, support_state: 'retained-feature-cohort', parity_scenario: 'Preserve additive feature implications of the final `cli`/`connect-host` cohorts; Connect-only runtime does not imply `cli` or any host composition' }));

  const unique = new Map();
  for (const value of rows) {
    if (unique.has(value.id)) {
      const previous = unique.get(value.id);
      if (JSON.stringify(previous) !== JSON.stringify(value)) note(`duplicate:${value.id}`, `Conflicting identity ${value.symbol_or_route}`, 'STOP');
    } else unique.set(value.id, value);
  }
  const finalRows = [...unique.values()].sort((a, b) => a.id.localeCompare(b.id));
  const summary = summarize(finalRows, expected, discrepancies);
  summary.baseline = baseline;
  summary.clap_cohorts = cohorts.map(({ registered_ids, ...cohort }) => ({ ...cohort, registered_identity_sha256: hash(sorted(registered_ids).join('\n')) }));
  summary.counts = Object.fromEntries(sorted(finalRows.map(value => value.kind)).map(kind => [kind, finalRows.filter(value => value.kind === kind).length]));
  summary.source_sha256 = hash([...sources].sort(([a], [b]) => a.localeCompare(b)).map(([path, source]) => `${path}\0${hash(source)}`).join('\n'));
  summary.registered_identity_sha256 = hash(sorted(expected).join('\n'));
  summary.local_dependency_closure = [...dependencyClosure.keys()].sort((a, b) => (a < b ? -1 : a > b ? 1 : 0));
  summary.provenance = provenance;
  return { rows: finalRows, discrepancies: discrepancies.sort((a, b) => a.id.localeCompare(b.id)), summary };
}

export function discrepancyReport(result) {
  return `# RFT disposition discrepancies\n\nBaseline: ${result.summary.baseline}\n\nStatic source closure and executable clap proof only; no business operation, migration, network or paid call executed.\n\n## Summary sets\n\n\`\`\`json\n${JSON.stringify(result.summary, null, 2)}\n\`\`\`\n\n## Classified contradictions and blockers\n\n${result.discrepancies.map(item => `- **${item.severity}: ${item.id}** — ${item.detail}`).join('\n')}\n\n## Explicit nonregistrations\n\n${result.summary.extra_unregistered.map(item => `- ${item.id}: ${item.disposition}`).join('\n')}\n`;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  try {
    const { out, ...options } = parseFlags(process.argv.slice(2));
    const result = await collectInventory(options);
    await mkdir(dirname(resolve(out)), { recursive: true });
    await writeFile(out, `${JSON.stringify(result, null, 2)}\n`);
    await writeFile(join(dirname(resolve(out)), 'disposition-discrepancies.md'), discrepancyReport(result));
    console.log(JSON.stringify(result.summary, null, 2));
    if (result.summary.blockers.length || ['missing_registrations', 'unknown_production_writer_table', 'unknown_destination', 'unresolved_m1_schema_auth'].some(key => result.summary[key].length)) process.exitCode = 1;
  } catch (error) {
    console.error(`RFT inventory BLOCKED: ${error.message}`);
    process.exitCode = 1;
  }
}
