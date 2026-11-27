#!/usr/bin/env node
/**
 * Run one child so it cannot outlive the process that asked for it.
 *
 * `check-contract-packageability.mjs` waits with `spawnSync`, which blocks the
 * loop: a signal delivered to it while a `cargo` build runs is not dispatched to
 * JavaScript at all, so that process can neither kill the build's process group
 * nor report the interruption. Its children would keep writing into the shared
 * build tree as orphans.
 *
 * This supervisor is the process that *can* react, because it is not the blocked
 * one. It leads its own process group (`detached`), runs the command inside that
 * group, and takes the group down when either side disappears: its parent dies
 * (it is reparented, so `ppid` changes) or it is signalled itself. The caller
 * still kills the group by pid on a deadline; this covers the interruption the
 * caller cannot observe.
 *
 * Usage: node supervise-child.mjs <command> [args...]. stdin/stdout/stderr are
 * the command's own, so the caller's capture and `input` behave as without it.
 *
 * @module tooling/release/supervise-child
 */

import { spawn } from "node:child_process";

/** How often a reparented supervisor notices, in milliseconds. */
const PARENT_POLL_MS = 100;

/** Exit code the caller reads when the command never started. */
const SPAWN_FAILED = 127;

const [command, ...args] = process.argv.slice(2);
if (command === undefined) {
  process.stderr.write("supervise-child: no command given\n");
  process.exit(SPAWN_FAILED);
}

const originalParent = process.ppid;

/** @param {number} code */
function exitAfterGroupKill(code) {
  try {
    process.kill(-process.pid, "SIGKILL");
  } catch (error) {
    if (error?.code !== "ESRCH") {
      throw error;
    }
  }
  process.exit(code);
}

for (const signal of ["SIGINT", "SIGTERM", "SIGHUP"]) {
  process.once(signal, () => exitAfterGroupKill(SPAWN_FAILED));
}

const child = spawn(command, args, { stdio: "inherit" });

child.on("error", (error) => {
  process.stderr.write(`${command} could not run: ${error.message}\n`);
  exitAfterGroupKill(SPAWN_FAILED);
});

const watch = setInterval(() => {
  if (process.ppid !== originalParent) {
    clearInterval(watch);
    exitAfterGroupKill(SPAWN_FAILED);
  }
}, PARENT_POLL_MS);

child.on("exit", (code, signal) => {
  clearInterval(watch);
  if (signal !== null) {
    // Report the command's death the way `spawnSync` reports it: by signal.
    process.kill(process.pid, signal);
    return;
  }
  process.exit(code ?? 1);
});
