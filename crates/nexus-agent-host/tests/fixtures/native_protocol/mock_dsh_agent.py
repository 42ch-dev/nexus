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
  plus one startup `_spawn` record ({"argv": ..., "dsh_home": ...,
  "pid": ...}) so tests can assert the provider's launch identity (exact
  argv, DSH_HOME) and CONFIRMED child exit (pid liveness after close)
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
- WRONG_IDENTITY=1  answer `initialize` with a foreign server identity —
  the SDK rejects the handshake (hard protocol error) and the launch
  must fail closed (wrong-identity arm).
- CLOSE_ERROR=1   answer the `shutdown` request with a JSON-RPC error.
  The SDK treats a failed cooperative shutdown as diagnostic only; the
  EOF/TERM/KILL ladder still reaps the child (close-error tolerance arm).
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
        "pid": os.getpid(),
        # The child's own working directory, so a test can prove the bounded
        # probe ran in the verified owner workspace (never the ambient cwd).
        "cwd": os.getcwd(),
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
        name = "deepseek-harness-sdk-runtime"
        if os.environ.get("WRONG_IDENTITY"):
            # A foreign runtime identity: the SDK rejects the handshake
            # with a hard protocol error.
            name = "not-the-dsh-sdk-runtime"
        reply(req, {
            "serverInfo": {
                "name": name,
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
        scenario = os.environ.get("SCENARIO", "happy")
        if scenario == "hold_turn" or os.environ.get("HOLD_TURN") == "1":
            return
        if scenario == "lag":
            time.sleep(int(os.environ.get("LAG_MS", "150")) / 1000.0)
        if scenario == "nested":
            session_event("child-session", {
                "type": "assistant/message",
                "data": {"content": [{"type": "text", "text": "nested-only"}]},
            })
        if scenario == "two_messages":
            session_event(session_id, {
                "type": "assistant/message",
                "data": {"content": [{"type": "text", "text": "A"}]},
            })
            session_event(session_id, {
                "type": "assistant/message",
                "data": {"content": [{"type": "text", "text": "B"}]},
            })
        elif scenario == "empty_only":
            session_event(session_id, {
                "type": "assistant/message",
                "data": {"content": [{"type": "text", "text": ""}]},
            })
        elif scenario == "malformed_text":
            session_event(session_id, {
                "type": "assistant/message",
                "data": {"content": [{"type": "text", "text": 1}]},
            })
        elif scenario == "malformed_root_event_type":
            session_event(session_id, {
                "data": {"content": [{"type": "text", "text": "would-be-streamed"}]},
            })
        elif scenario == "oversize":
            big = "x" * (int(os.environ.get("OVERSIZE_BYTES", str(256 * 1024 + 1))))
            session_event(session_id, {
                "type": "assistant/message",
                "data": {"content": [{"type": "text", "text": big}]},
            })
        elif scenario == "partial_then_fail":
            session_event(session_id, {
                "type": "assistant/message",
                "data": {"content": [{"type": "text", "text": "partial"}]},
            })
        elif scenario == "malformed":
            pass  # malformed turn/end below; no assistant prose
        else:
            session_event(session_id, {
                "type": "assistant/message",
                "data": {"message": {"content": [{"type": "text", "text": "mock dsh reply"}]}},
            })
        if scenario == "tool_call":
            session_event(session_id, {
                "type": "tool/call",
                "data": {"tool": "bash", "input": {"command": "echo hi"}},
            })
        finish_kind = "completed"
        if scenario == "partial_then_fail":
            finish_kind = "max-tokens"
        if scenario == "malformed":
            session_event(session_id, {
                "type": "turn/end",
                "data": {"reason": {}},
            })
            # Root idle ends the SDK activity interval even when turn/end is
            # malformed; without idle Session::run waits forever.
            session_status(session_id, "idle")
        else:
            session_event(session_id, {
                "type": "turn/end",
                "data": {"reason": {"kind": finish_kind}},
            })
            session_status(session_id, "idle")
        return

    if method == "shutdown":
        delay_ms = int(os.environ.get("SHUTDOWN_DELAY_MS", "0"))
        if delay_ms > 0:
            time.sleep(delay_ms / 1000.0)
        if os.environ.get("CLOSE_ERROR"):
            # A failed cooperative shutdown is diagnostic only for the
            # SDK; the EOF/TERM/KILL ladder still reaps this process.
            send({
                "jsonrpc": "2.0",
                "id": req["id"],
                "error": {"code": -32603, "message": "cooperative shutdown failed"},
            })
            return
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
