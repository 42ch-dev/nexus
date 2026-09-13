# nexus-service

Standalone Node HTTP service over `@42ch/nexus-native` (v1.189 P4).

## Entrypoints

- Library: `startService(options)` from `dist/index.js`
- CLI: `node dist/main.js --home <raw-home> --host 127.0.0.1 --port 8421`
- Domain-only profile: add `--domain-only` (DirectWriter; no provider execution)
- Remote: requires `--allow-remote --tls-cert <path> --tls-key <path>` and `NEXUS42_DAEMON_API_KEY`

## Scripts

- `pnpm run build` — compile TypeScript to `dist/`
- `pnpm run start` — run compiled CLI
- `pnpm run proof:open-smoke` — P2 native open smoke (not product HTTP)

## Tests

- `node --test apps/nexus-service/tests/world-kb-http.test.mjs` (PM wave; seeds fixture + real native core)

## Ownership (P4)

Service sources only. Do not edit root lockfiles, schemas, Electron, or `apps/web`.
