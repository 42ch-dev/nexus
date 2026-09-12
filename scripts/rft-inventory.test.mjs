import { test } from 'node:test';
import assert from 'node:assert/strict';
import { identity, parseFlags, completionPaths, declaredClapPaths, expandMethods, routerRegistrations, productionSource, rustTokens, classifyTable, classifyWriter, summarize, manifestFeatures } from './rft-inventory.mjs';

const baseline = 'bbaae32d422b673576d683859b474da6bd787743';

test('callable identity keeps full path, feature set and alias relation without incidental whitespace', () => {
  const input = { kind: 'callable', source_path: 'old.rs', symbol_or_route: 'nexus42 creator world kb graph', feature_condition: 'connect-host + connect-client' };
  assert.equal(identity(input), identity({ ...input, source_path: 'moved.rs', symbol_or_route: ['nexus42', 'creator', 'world', 'kb', 'graph'], feature_condition: 'connect-client+connect-host' }));
  assert.notEqual(identity(input), identity({ ...input, symbol_or_route: 'nexus42 other graph' }));
  assert.notEqual(identity(input), identity({ ...input, feature_condition: 'all' }));
  assert.notEqual(identity(input), identity({ ...input, alias_of: 'nexus42 creator world kb old-graph' }));
});

test('clap completion walks transitions without confusing kebab commands, hidden paths or aliases', () => {
  const source = `
    nexus42,daemon)\n cmd="nexus42__subcmd__daemon"
    nexus42,daemon-run)\n cmd="nexus42__subcmd__daemon__subcmd__run"
    nexus42__subcmd__daemon,ui)\n cmd="nexus42__subcmd__daemon__subcmd__ui"
    nexus42__subcmd__daemon,web)\n cmd="nexus42__subcmd__daemon__subcmd__ui"
    nexus42,help)\n cmd="nexus42__subcmd__help"
    nexus42__subcmd__help,daemon)\n cmd="nexus42__subcmd__help__subcmd__daemon"
  `;
  assert.deepEqual(completionPaths(source).filter(item => item.leaf).map(item => item.path.join(' ')), ['nexus42 daemon ui', 'nexus42 daemon web', 'nexus42 daemon-run']);
  assert.throws(() => completionPaths('different generator format'), /format not recognized/);
});

test('source expansion follows boxed subcommands and Args without minting wrapper or test commands', () => {
  const sources = new Map([['apps/nexus42/src/cli.rs', `
    pub enum Commands {
      #[cfg(feature = "connect-host")]
      #[command(hide = true)]
      Connect { #[command(subcommand)] command: Box<ConnectCommand> },
      #[command(visible_alias = "web")]
      Ui(UiArgs),
      Run { #[command(flatten)] command: RunArgs },
    }
    pub enum ConnectCommand { Start }
    pub struct UiArgs { port: u16 }
    pub struct RunArgs { preset_id: String }
    #[cfg(test)] enum TestWrapper { Fake }
  `]]);
  const leaves = declaredClapPaths(sources).filter(item => item.leaf);
  assert.deepEqual(leaves.map(item => item.path.join(' ')), ['nexus42 connect start', 'nexus42 ui', 'nexus42 web', 'nexus42 run']);
  assert.equal(leaves[0].feature_condition, 'connect-host');
  assert.equal(leaves[0].hidden, true);
  assert.equal(leaves[2].alias_of, 'nexus42 ui');
});

test('method expansion ignores strings/comments and preserves each handler', () => {
  assert.deepEqual(expandMethods('get(handlers::read).post(handlers::write) /* .delete(fake) */ .patch(handlers::patch)'), [
    { method: 'GET', handler: 'handlers::read' },
    { method: 'POST', handler: 'handlers::write' },
    { method: 'PATCH', handler: 'handlers::patch' },
  ]);
  assert.throws(() => expandMethods('custom_methods(handler)'), /Unresolved router method set/);
});

test('router closure expands only mounted nested routes with inherited authorization', () => {
  const source = `
    fn domain_routes() -> Router {
      Router::new().nest("/worlds", Router::new().route("/{id}", get(handlers::read).post(handlers::write)))
    }
    fn dormant_routes() -> Router { Router::new().route("/dormant", get(handlers::old)) }
    pub fn create_router() -> Router {
      let runtime_routes = Router::new().route("/health", get(handlers::health));
      let protected_routes = Router::new()
        .merge(domain_routes().route_layer(from_fn(require_active_creator)))
        .route_layer(from_fn(require_api_key));
      let router = Router::new().merge(runtime_routes).merge(protected_routes);
      router
    }
  `;
  assert.deepEqual(routerRegistrations(source).map(({ method, path, auth }) => ({ method, path, auth })), [
    { method: 'GET', path: '/health', auth: 'unguarded' },
    { method: 'GET', path: '/worlds/{id}', auth: 'api-key+active-creator' },
    { method: 'POST', path: '/worlds/{id}', auth: 'api-key+active-creator' },
  ]);
  assert.throws(() => routerRegistrations(source.replace('domain_routes().route_layer', 'unknown_routes().route_layer')), /Unresolved router merge/);
});

test('test-only fields and wrappers do not consume following production code or quoted braces', () => {
  const source = `
    struct State { #[cfg(test)] hook: Hook, live: bool }
    #[cfg(test)] fn test_only() { open_pool("fake"); }
    fn production() { let _ = r#"/* { a literal } */"#; State { #[cfg(test)] hook: make(None), live: true }; }
    // open_pool("comment");
  `;
  const production = productionSource(source);
  const names = rustTokens(production).filter(token => !token.string).map(token => token.value);
  assert.ok(!names.includes('test_only'));
  assert.ok(!names.includes('hook'));
  assert.ok(names.includes('production'));
  assert.ok(names.includes('live'));
});

test('compound test cfg and manifest comments cannot masquerade as production feature registrations', () => {
  const code = productionSource('#[cfg(all(test, feature = "connect-host"))] mod interop; fn live() {}');
  assert.ok(!rustTokens(code).some(token => token.value === 'interop'));
  const alternations = productionSource(`
    #[cfg(any(test, feature = "test-hooks"))] pub mod test_hooks;
    #[cfg(any(feature = "test-hooks", all(unix, test),))] fn helper() {}
    #[cfg(feature = "test-hooks")] fn hook_only() {}
    #[cfg(all(feature = "connect-host", not(test)))] fn live_connect() {}
    #[cfg(any(test, feature = "connect-host"))] fn live_alternative() {}
    #[cfg(not(any(test, feature = "test-hooks")))] fn live_without_hooks() {}
    #[cfg(not(feature = "connect-host"))] fn live_without_connect() {}
    fn live() {}
  `);
  const declarations = rustTokens(alternations).filter(token => !token.string).map(token => token.value);
  for (const name of ['test_hooks', 'helper', 'hook_only']) assert.ok(!declarations.includes(name), name);
  for (const name of ['live_connect', 'live_alternative', 'live_without_hooks', 'live_without_connect', 'live']) assert.ok(declarations.includes(name), name);
  assert.deepEqual(manifestFeatures('# Forwarding (see [features])\n[dependencies]\none = "1"\n[features]\nconnect-host = ["dep:spoke"]\nweb-embed = []\ndefault = ["web-embed"]\n'), [
    { name: 'connect-host', definition: '["dep:spoke"]' },
    { name: 'web-embed', definition: '[]' },
    { name: 'default', definition: '["web-embed"]' },
  ]);
});

test('storage classifications distinguish authoring, engine, protocol, migration, system and unknowns', () => {
  assert.equal(classifyTable('kb_key_blocks'), 'direct-authoring');
  assert.equal(classifyTable('orchestration_sessions'), 'engine-owned');
  assert.equal(classifyTable('core_changes'), 'protocol');
  assert.equal(classifyTable('_sqlx_migrations'), 'migration');
  assert.equal(classifyTable('workspace_meta'), 'system');
  assert.equal(classifyTable('new_unreviewed_table'), null);
  assert.equal(classifyWriter('crates/nexus-local-db/src/lib.rs', 'open_pool_read_only'), 'read-only');
  assert.equal(classifyWriter('crates/nexus-daemon-runtime/src/db/pool.rs', 'new'), 'guarded');
  assert.equal(classifyWriter('crates/nexus-cloud-sync/src/pool.rs', 'new'), 'guarded');
  assert.equal(classifyWriter('crates/unreviewed/src/lib.rs', 'new'), null);
});

test('fixture closure is empty only when actual identity, storage, destination and M1 auth/schema agree', () => {
  const rows = [
    { id: 'route:1', kind: 'route', destination_rft: 'RFT-01', m1: true, gap: null, schema_refs: ['graph.schema.json'], auth: 'stored-world-owner' },
    { id: 'writer:1', kind: 'writer', writer_class: 'guarded', destination_rft: 'RFT-01' },
    { id: 'table:1', kind: 'table', table_class: 'direct-authoring', destination_rft: 'RFT-01' },
  ];
  const expected = rows.map(item => item.id);
  const clean = summarize(rows, expected);
  assert.deepEqual(Object.values(clean), [[], [], [], [], [], []]);
  assert.deepEqual(summarize(rows.slice(1), expected).missing_registrations, ['route:1']);
  const unknown = summarize([...rows, { id: 'writer:2', kind: 'writer' }], expected);
  assert.deepEqual(unknown.unknown_production_writer_table, ['writer:2']);
  assert.deepEqual(unknown.unknown_destination, ['writer:2']);
  assert.deepEqual(summarize([{ ...rows[0], auth: '' }], ['route:1']).unresolved_m1_schema_auth, ['route:1']);
  const dormant = { id: 'old:1', kind: 'dormant-route', destination_rft: 'RFT-11', retirement_gate: 'External consumer deletion block' };
  assert.deepEqual(summarize([dormant]).extra_unregistered, [{ id: 'old:1', kind: 'dormant-route', disposition: 'External consumer deletion block' }]);
});

test('summary includes source clap declarations absent from every executable cohort with their disposition', () => {
  const declarations = declaredClapPaths(new Map([['apps/nexus42/src/cli.rs', `
    enum Commands {
      Live,
      #[cfg(feature = "connect-host")] Connect,
      SourceOnly,
    }
  `]]));
  const rows = declarations.map(item => {
    const value = {
      kind: 'callable', symbol_or_route: item.path, feature_condition: item.feature_condition,
      declaration: `${item.source_path}:${item.symbol}`, destination_rft: 'RFT-11',
      retirement_gate: 'Resolve source registration and retain until RFT-11 parity decision',
    };
    return { ...value, id: identity(value) };
  });
  const defaultCohort = [rows[0].id];
  const connectCohort = [rows[0].id, rows[1].id];
  const runtime = { id: 'runtime:1', kind: 'callable', destination_rft: 'RFT-11' };
  const summary = summarize([...rows, runtime], [...defaultCohort, ...connectCohort]);
  assert.deepEqual(summary.extra_unregistered, [{
    id: rows[2].id, kind: 'clap-extra',
    disposition: 'Resolve source registration and retain until RFT-11 parity decision',
  }]);
  assert.deepEqual(summary.missing_registrations, []);
});

test('CLI accepts exactly frozen flags and rejects missing, duplicate, malformed or unknown values', () => {
  assert.deepEqual(parseFlags(['--out', 'ledger.json', '--baseline', baseline, '--inventories', 'inputs']), { out: 'ledger.json', baseline, inventoriesDir: 'inputs' });
  assert.throws(() => parseFlags(['--baseline', baseline]), /Usage/);
  assert.throws(() => parseFlags(['--baseline', baseline, '--baseline', baseline]), /duplicate/);
  assert.throws(() => parseFlags(['--inventories', '--out']), /Invalid/);
  assert.throws(() => parseFlags(['--help']), /Invalid/);
  assert.throws(() => parseFlags(['--baseline', 'HEAD', '--inventories', 'inputs', '--out', 'out']), /Usage/);
});
