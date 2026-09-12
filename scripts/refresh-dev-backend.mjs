#!/usr/bin/env node
/** Explicit backend refresh entry — the only ordinary DX path that may run Cargo build/codegen. */
import { resolve } from 'node:path';
import {
  defaultArtifactPath,
  getRepoRoot,
  refreshBackend,
  resolveTargetDir,
} from './dev-backend-manifest.mjs';

function parseArgs(argv) {
  const options = { profile: 'debug', targetDir: undefined };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--profile') {
      options.profile = argv[++i];
      if (!options.profile) throw new Error('Missing value for --profile');
    } else if (arg === '--target-dir') {
      options.targetDir = argv[++i];
      if (!options.targetDir) throw new Error('Missing value for --target-dir');
    } else if (arg === '--help' || arg === '-h') {
      console.log(`Usage: node scripts/refresh-dev-backend.mjs [--profile debug|release] [--target-dir <path>]`);
      process.exit(0);
    } else {
      throw new Error(`Unknown argument: ${arg}`);
    }
  }
  if (options.profile !== 'debug' && options.profile !== 'release') {
    throw new Error(`Unsupported profile: ${options.profile}`);
  }
  return options;
}

async function main() {
  const { profile, targetDir: targetDirArg } = parseArgs(process.argv.slice(2));
  const targetDir = targetDirArg ? resolve(targetDirArg) : await resolveTargetDir();
  const artifactPath = defaultArtifactPath({ profile, targetDir });
  console.log(`==> refreshing backend artifact (${profile})`);
  console.log(`    target dir: ${targetDir}`);
  console.log(`    artifact:   ${artifactPath}`);
  const manifest = await refreshBackend({ profile, targetDir, repoRoot: getRepoRoot() });
  console.log(`==> manifest written (${manifest.sha256.slice(0, 12)}…)`);
}

main().catch(err => {
  console.error(err.message ?? err);
  process.exit(1);
});
