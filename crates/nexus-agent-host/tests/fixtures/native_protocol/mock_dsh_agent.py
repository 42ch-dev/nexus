#!/usr/bin/env python3
"""Minimal mock `dsh` runtime for nexus-agent-host tests (v1.188 P0 T2 cutover).

Speaks the newline-delimited JSON-RPC 2.0 stdio subset that the
deepseek-harness-sdk uses (client/core.rs `request` / api.rs
`Session::run`): `initialize` (the result must carry the wire-stable
server identity `deepseek-harness-sdk-runtime`; the reported protocol
version is `0.0.1`, NOT the crate version `0.2.0`), `session/prompt`
(result carries a durable message id, then the notifications
`Session::run` waits for: the `agent/inbox/spliced` inbox receipt, an
`assistant/message`, a `turn/end`, and root `session.status == "idle"`),
and `shutdown` (respond, then exit on stdin EOF so the SDK close ladder
completes fast).

The `turn/end` reason kind uses the SDK 0.2 vocabulary: `completed` is
the only successful finish reason (v1.188 P0 T1).

Behavior knobs (env vars):
- REQ_LOG=<path>  append one JSON object per received request
  ({"method": ..., "sessionId": ...}) for session-rotation assertions,
  plus one startup `_spawn` record ({"argv": ..., "dsh_home": ...}) so
  tests can assert the provider's launch identity (exact argv, DSH_HOME)
  without touching the wire protocol.
- HOLD_TURN=1     after the `session/prompt` response, emit the inbox
  receipt but never the root idle — the SDK run hangs and the provider's
  turn timeout fires (the zombie-turn arm: the runtime keeps the turn
  open under the old session id).
- SHUTDOWN_DELAY_MS=<ms>  delay the `shutdown` reply, so the provider's
  close-wait timeout fires while the retained cleanup owner still runs
  (unconfirmed-close lifecycle arm).
- INIT_DELAY_MS=<ms>  delay the `initialize` reply, so a probe deadline
  can fire while the sealed runtime START is still in flight (retained
  init-ownership arm).
- INIT_FAIL_SEALED=1  fail the `initialize` reply for sealed spawns
  (`--patch` present) after planting a removal blocker inside DSH_HOME
  (switch start+delete failure arm).
"""

import json
import os
import sys
import time

_msg_counter = 0


def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def log_request(req):
    path = os.environ.get("REQ_LOG")
    if not path:
        return
    entry = {"method": req.get("method")}
    params = req.get("params")
    if isinstance(params, dict) and params.get("sessionId"):
        entry["sessionId"] = params["sessionId"]
    with open(path, "a") as f:
        f.write(json.dumps(entry) + "\n")



def log_spawn():
    path = os.environ.get("REQ_LOG")
    if not path:
        return
    entry = {
        "method": "_spawn",
        "argv": sys.argv[1:],
        "dsh_home": os.environ.get("DSH_HOME", ""),
    }
    with open(path, "a") as f:
        f.write(json.dumps(entry) + "\n")

def reply(req, result):
    send({"jsonrpc": "2.0", "id": req["id"], "result": result})


def session_event(session_id, event):
    send({
        "jsonrpc": "2.0",
        "method": "session.event",
        "params": {"sessionId": session_id, "event": event},
    })


def session_status(session_id, status):
    send({
        "jsonrpc": "2.0",
        "method": "session.status",
        "params": {"sessionId": session_id, "status": status},
    })


def handle_request(req):
    global _msg_counter
    method = req.get("method")
    params = req.get("params") or {}
    log_request(req)

    if method == "initialize":
        if os.environ.get("INIT_FAIL_SEALED") and "--patch" in sys.argv:
            dsh_home = os.environ.get("DSH_HOME", "")
            if dsh_home:
                blocker = os.path.join(dsh_home, "blocker")
                os.makedirs(blocker, exist_ok=True)
                os.chmod(blocker, 0o500)
                # Prevent anchored removal from unlinking the leaf entry.
                os.chmod(dsh_home, 0o555)
            send({
                "jsonrpc": "2.0",
                "id": req["id"],
                "error": {"code": -32603, "message": "sealed init failed"},
            })
            return
        delay_ms = int(os.environ.get("INIT_DELAY_MS", "0"))
        if delay_ms > 0:
            time.sleep(delay_ms / 1000.0)
        reply(req, {
            "serverInfo": {
                "name": "deepseek-harness-sdk-runtime",
                "version": "0.0.1",
            }
        })
        return

    if method == "session/prompt":
        _msg_counter += 1
        message_id = "mock-msg-%d" % _msg_counter
        session_id = params.get("sessionId", "unknown")
        reply(req, {"messageId": message_id})
        session_event(session_id, {
            "type": "agent/inbox/spliced",
            "data": {"inserted": [{"id": message_id}]},
        })
        if os.environ.get("HOLD_TURN"):
            # The turn stays open: never emit the root idle, so the SDK
            # run hangs and the provider's turn timeout fires.
            return
        session_event(session_id, {
            "type": "assistant/message",
            "data": {"message": {"content": [{"type": "text", "text": "mock dsh reply"}]}},
        })
        session_event(session_id, {
            "type": "turn/end",
            "data": {"reason": {"kind": "completed"}},
        })
        session_status(session_id, "idle")
        return

    if method == "shutdown":
        delay_ms = int(os.environ.get("SHUTDOWN_DELAY_MS", "0"))
        if delay_ms > 0:
            time.sleep(delay_ms / 1000.0)
        reply(req, None)
        return

    reply(req, {"error": {"code": -32601, "message": "method not found"}})


def main():
    log_spawn()
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
        except json.JSONDecodeError:
            continue
        if req.get("method"):
            handle_request(req)


if __name__ == "__main__":
    main()
