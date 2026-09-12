#!/usr/bin/env python3
"""Hermetic ACP stdio agent fixture for nexus-agent-host lifecycle tests (v1.186 P1 T1).

Speaks the real ACP wire protocol (newline-delimited JSON-RPC 2.0) over
stdio: `initialize`, `session/new`, `session/prompt`, `session/cancel`.
It is NOT an echo: every prompt returns a deterministic non-echo
transformation (`echo:<prompt>` would be an echo; this returns
`transformed:<prompt>`), so tests can prove real agent output flowed.

Modes (env vars, all optional):
  - BLOCK_PROMPT=1   after receiving session/prompt, never respond (the
                     host's streaming timeout / cancel path is exercised).
  - EOF_AFTER_INIT=1 respond to initialize, then exit 0 immediately —
                     the host must surface EOF as a typed failure, not
                     success.
  - EOF_AFTER_INIT_FROM_RUN=n (n >= 1) as EOF_AFTER_INIT, but only from the
                     n-th fixture start recorded in ACP_FIXTURE_LOG. Lets a
                     test separate a PASSING bounded readiness probe (run 1)
                     from a FAILING later session launch (run n), so
                     post-ready launch failure is distinguished from a broken
                     recipe.
  - DESCENDANT=1     spawn a child process that outlives the fixture; the
                     host's owned process-tree shutdown must reap the exact
                     child (never a reused/unowned PID).

The fixture writes sanitized evidence (cwd, pid, request log) to the path
in ACP_FIXTURE_LOG (one JSON object per line). It never writes secrets.
"""

import json
import os
import subprocess
import sys

LOG_PATH = os.environ.get("ACP_FIXTURE_LOG")
BLOCK_PROMPT = os.environ.get("BLOCK_PROMPT") == "1"
DESCENDANT = os.environ.get("DESCENDANT") == "1"


def _prior_run_count():
    """Fixture starts already recorded in the shared log (0 for the first)."""
    if not LOG_PATH or not os.path.exists(LOG_PATH):
        return 0
    count = 0
    with open(LOG_PATH, encoding="utf-8") as fh:
        for line in fh:
            try:
                if json.loads(line).get("event") == "start":
                    count += 1
            except json.JSONDecodeError:
                continue
    return count


def _eof_after_init_enabled():
    if os.environ.get("EOF_AFTER_INIT") == "1":
        return True
    from_run = os.environ.get("EOF_AFTER_INIT_FROM_RUN")
    if not from_run:
        return False
    try:
        threshold = int(from_run)
    except ValueError:
        return False
    # Runs are 1-indexed: this start is run number (_prior_run_count() + 1).
    return (_prior_run_count() + 1) >= threshold


EOF_AFTER_INIT = _eof_after_init_enabled()

_descendant = None


def log(entry):
    if LOG_PATH:
        with open(LOG_PATH, "a", encoding="utf-8") as fh:
            fh.write(json.dumps(entry) + "\n")


def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def reply(req, result):
    send({"jsonrpc": "2.0", "id": req.get("id"), "result": result})


def reply_error(req, code, message):
    send({"jsonrpc": "2.0", "id": req.get("id"), "error": {"code": code, "message": message}})


def notify(method, params):
    send({"jsonrpc": "2.0", "method": method, "params": params})


def main():
    global _descendant
    log({"event": "start", "pid": os.getpid(), "cwd": os.getcwd()})

    if DESCENDANT:
        # Spawn a child that outlives us; the host must reap the exact
        # owned process tree on shutdown.
        _descendant = subprocess.Popen(
            [sys.executable, "-c", "import time; time.sleep(3600)"],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        log({"event": "descendant_spawned", "child_pid": _descendant.pid})

    session_id = None
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
        except json.JSONDecodeError:
            continue
        method = req.get("method")
        params = req.get("params") or {}
        log({"event": "request", "method": method, "id": req.get("id")})

        if method == "initialize":
            send({
                "jsonrpc": "2.0",
                "id": req.get("id"),
                "result": {
                    "protocolVersion": 1,
                    "agentCapabilities": {"loadSession": False},
                    "agentInfo": {"name": "mock-acp-workflow", "version": "1.0.0"},
                },
            })
            if EOF_AFTER_INIT:
                log({"event": "eof_after_init"})
                return
        elif method == "session/new":
            session_id = "mock-session-%d" % os.getpid()
            log({"event": "session_new", "session_id": session_id, "cwd": params.get("cwd")})
            reply(req, {"sessionId": session_id})
        elif method == "session/prompt":
            prompt = ""
            for block in params.get("prompt", []):
                if block.get("type") == "text":
                    prompt += block.get("text", "")
            log({"event": "prompt", "session_id": params.get("sessionId"), "prompt": prompt})
            if BLOCK_PROMPT:
                # Never respond to the prompt, but keep reading stdin so a
                # session/cancel notification is observed and recorded.
                log({"event": "prompt_blocked"})
                for raw_line in sys.stdin:
                    line = raw_line.strip()
                    if not line:
                        continue
                    try:
                        cancel_req = json.loads(line)
                    except json.JSONDecodeError:
                        continue
                    if cancel_req.get("method") == "session/cancel":
                        log({"event": "cancel", "session_id": cancel_req.get("params", {}).get("sessionId")})
                        # Acknowledge cancellation, then exit cooperatively so
                        # the host's bounded shutdown observes a clean exit.
                        return
                return
            # Deterministic non-echo transformation.
            notify("session/update", {
                "sessionId": params.get("sessionId"),
                "update": {
                    "sessionUpdate": "agent_message_chunk",
                    "content": {"type": "text", "text": "transformed:" + prompt},
                },
            })
            reply(req, {"stopReason": "end_turn"})
        elif method == "session/cancel":
            log({"event": "cancel", "session_id": params.get("sessionId")})
            # Acknowledge cancellation; the prompt loop above is blocked so
            # the host proceeds to owned process-tree termination.
            send({"jsonrpc": "2.0", "method": "session/update", "params": {
                "sessionId": params.get("sessionId"),
                "update": {"sessionUpdate": "agent_message_chunk",
                           "content": {"type": "text", "text": "cancelled"}},
            }})
        else:
            reply_error(req, -32601, "method not found: %s" % method)

    log({"event": "eof"})
    if _descendant is not None:
        try:
            _descendant.kill()
        except OSError:
            # The descendant already exited; nothing to reap.
            pass


if __name__ == "__main__":
    try:
        main()
    except KeyboardInterrupt:
        # Interactive Ctrl-C during a manual run is a normal exit.
        pass
