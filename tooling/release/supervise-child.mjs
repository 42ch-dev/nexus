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
import { writeFileSync, writeSync } from "node:fs";

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

// First thing, so nothing below can skip it: a caller that wants to signal this
// process needs its pid, and it has no other way to learn one.
const supervisorPidFile = process.env.NEXUS_SUPERVISOR_PID_FILE;
if (supervisorPidFile !== undefined && supervisorPidFile.length > 0) {
  try {
    writeFileSync(supervisorPidFile, `${process.pid}\n`);
  } catch (error) {
    process.stderr.write(
      `supervise-child: could not publish ${supervisorPidFile}: ${error.message}\n`,
    );
  }
}

const originalParent = process.ppid;

/**
 * The caller may hand us an fd to publish the command's group pid on, so that a
 * supervisor killed without a chance to act (SIGKILL) still leaves the caller a
 * handle on the group it cannot reap itself.
 */
const pidChannel = Number(process.env.NEXUS_CHILD_PID_FD ?? "");

/**
 * Kill the command's group; this process is not in it, so it survives to exit.
 *
 * @param {{ assertPid?: boolean }} [options] `assertPid` belongs on paths where
 *   `spawn` has demonstrably succeeded (the command ran and exited); a signal that
 *   arrives before the pid is assigned must not turn into an uncaught exception,
 *   or the caller loses the "command never started" cause the error event carries.
 */
function killCommandGroup({ assertPid = false } = {}) {
  if (child.pid === undefined) {
    if (assertPid) {
      throw new Error(`supervise-child: ${command} has no pid after a spawn`);
    }
    process.stderr.write(
      `supervise-child: ${command} has no pid yet; the spawn error carries the cause\n`,
    );
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
// installed would swallow the re-raise instead. These go in before `spawn` so an
// early signal is handled by this process rather than the default disposition,
// which would kill the supervisor and leave the command it is about to start.
for (const signal of SIGNALS) {
  process.once(signal, () => {
    killCommandGroup();
    process.removeAllListeners(signal);
    process.kill(process.pid, signal);
  });
}

/**
 * The command's stdio is this process's own three streams plus one ignored entry
 * for every fd the caller's channel occupies, so the channel fd is never inherited
 * by the command (the caller's read would then wait for the build) and cannot
 * drift from `NEXUS_CHILD_PID_FD`.
 *
 * @param {number} channel
 * @returns {("inherit" | "ignore")[]}
 */
function commandStdio(channel) {
  const stdio = ["inherit", "inherit", "inherit"];
  if (Number.isInteger(channel) && channel >= 3) {
    while (stdio.length < channel) {
      stdio.push("ignore");
    }
    stdio.push("ignore");
  }
  return stdio;
}

const child = spawn(command, args, {
  stdio: commandStdio(pidChannel),
  detached: true,
});

if (Number.isInteger(pidChannel) && pidChannel >= 3) {
  child.on("spawn", () => {
    try {
      writeSync(pidChannel, `${child.pid}\n`);
    } catch {
      // The caller closed the channel; the group is still reaped on our signals.
    }
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
    // A command killed by a signal leaves whatever it started in its group:
    // `cargo` compiling means `rustc` children still there. Reap the group first,
    // then forward the death as a signal, as `spawnSync` reports it. The command
    // ran and exited, so a missing pid here is a broken contract, not an early
    // signal.
    killCommandGroup({ assertPid: true });
    process.removeAllListeners(signal);
    process.kill(process.pid, signal);
    return;
  }
  process.exit(code ?? 1);
});
