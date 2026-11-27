#!/usr/bin/env node
/**
 * Run one child so it cannot outlive the process that asked for it.
 *
 * The caller MUST spawn this with `detached: true`. The caller's deadline kill
 * addresses a process group by pid, so this process has to lead one: spawned
 * without the flag it inherits the caller's group, and that kill would land in the
 * caller's own group instead of here. The command then leads a second group of its
 * own (`detached` below), so this process stays outside the group it has to reap.
 *
 * `check-contract-packageability.mjs` waits with `spawnSync`, which blocks the
 * loop: a signal delivered to it while a `cargo` build runs is not dispatched to
 * JavaScript at all, so that process can neither kill the build's process group
 * nor report the interruption. Its children would keep writing into the shared
 * build tree as orphans. This supervisor is the process that *can* react, because
 * it is not the blocked one.
 *
 * The command leads its own group (`detached` below), so `kill(-child.pid)`
 * reaches the compilers it started while this process stays alive to forward the
 * command's exit code. The group is taken down when this process is signalled, or
 * when it is reparented because its caller died.
 *
 * Usage: `spawn(process.execPath, [thisFile, command, ...args], { detached: true })`.
 * stdin/stdout/stderr are the command's own, so the caller's capture and `input`
 * behave as without it.
 *
 * @module tooling/release/supervise-child
 */

import { spawn } from "node:child_process";

/** How often a reparented supervisor notices, in milliseconds. */
const PARENT_POLL_MS = 100;

/** Exit code the caller reads when the command never started. */
const SPAWN_FAILED = 127;

const SIGNALS = ["SIGINT", "SIGTERM", "SIGHUP"];

const [command, ...args] = process.argv.slice(2);
if (command === undefined) {
  process.stderr.write("supervise-child: no command given\n");
  process.exit(SPAWN_FAILED);
}

const originalParent = process.ppid;

const child = spawn(command, args, { stdio: "inherit", detached: true });

/** Kill the command's group; this process is not in it, so it survives to exit. */
function killCommandGroup() {
  if (child.pid === undefined) {
    return;
  }
  try {
    process.kill(-child.pid, "SIGKILL");
  } catch (error) {
    if (error?.code !== "ESRCH") {
      throw error;
    }
  }
}

// A signal must both stop the build and be the reason this process dies: the
// caller reads a timeout kill from the child's signal, and leaving a listener
// installed would swallow the re-raise instead.
for (const signal of SIGNALS) {
  process.once(signal, () => {
    killCommandGroup();
    process.removeAllListeners(signal);
    process.kill(process.pid, signal);
  });
}

const watch = setInterval(() => {
  if (process.ppid !== originalParent) {
    clearInterval(watch);
    killCommandGroup();
    process.exit(SPAWN_FAILED);
  }
}, PARENT_POLL_MS);

child.on("error", (error) => {
  clearInterval(watch);
  process.stderr.write(`${command} could not run: ${error.message}\n`);
  process.exit(SPAWN_FAILED);
});

child.on("exit", (code, signal) => {
  clearInterval(watch);
  if (signal !== null) {
    // Forward the command's death as a signal, as `spawnSync` reports it.
    process.removeAllListeners(signal);
    process.kill(process.pid, signal);
    return;
  }
  process.exit(code ?? 1);
});
