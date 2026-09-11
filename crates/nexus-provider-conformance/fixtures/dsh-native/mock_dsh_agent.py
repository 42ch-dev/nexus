#!/usr/bin/env python3
"""Scripted `dsh` runtime speaker for nexus-provider-conformance (v1.188 P0 T2 cutover).

Speaks the newline-delimited JSON-RPC 2.0 stdio subset that the
deepseek-harness-sdk uses (client/core.rs `request` / api.rs
`Session::run`): `initialize` (the result must carry the wire-stable
server identity `deepseek-harness-sdk-runtime`; the reported protocol
version is `0.0.1`, NOT the crate version `0.2.0`), `session/prompt`
(result carries a durable message id, then the
notifications `Session::run` waits for: the `agent/inbox/spliced` inbox
receipt, an `assistant/message`, a `turn/end`, and root
`session.status == "idle"`), and `shutdown` (respond, then exit on stdin
EOF so the SDK close ladder completes fast).

Behavior knobs (env vars):
- SCENARIO=happy|tool_call|malformed|cancel  (default: happy)
  - happy:     assistant/message -> turn/end(completed) -> idle
  - tool_call: assistant/message -> tool/call event -> turn/end(completed) ->
               idle (the SDK collects the tool event as raw noise; the
               normalized surface never surfaces tool calls — AR-6)
  - malformed: assistant/message -> turn/end WITHOUT data.reason.kind ->
               idle — the SDK fails the run with SdkProtocol (the dsh
               decode-error surface) -> one OpFailed(decode_error)
  - cancel:    same as happy — the dsh adapter's cancel is an honest no-op
               (AR-6), so the turn runs to completion
- REQ_LOG=<path>  append one JSON object per received request
  ({"method": ..., "sessionId": ...}), plus one startup `_spawn` record
  ({"argv": ..., "dsh_home": ...}) for launch-identity assertions.
"""

import json
import os
import time
import sys

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
        elif scenario == "partial_then_fail":
            session_event(session_id, {
                "type": "assistant/message",
                "data": {"content": [{"type": "text", "text": "partial"}]},
            })
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
