#!/usr/bin/env python3
"""Scripted `dsh-jsonrpc-agent` runtime speaker for nexus-provider-conformance (v1.180 P0 T2).

Speaks the newline-delimited JSON-RPC 2.0 stdio subset that the
deepseek-harness-sdk uses (client/core.rs `request` / api.rs
`Session::run`): `initialize` (the result must carry the wire-stable
server identity `deepseek-harness-sdk-runtime` plus a version),
`session/prompt` (result carries a durable message id, then the
notifications `Session::run` waits for: the `agent/inbox/spliced` inbox
receipt, an `assistant/message`, a `turn/end`, and root
`session.status == "idle"`), and `shutdown` (respond, then exit on stdin
EOF so the SDK close ladder completes fast).

Behavior knobs (env vars):
- SCENARIO=happy|tool_call|malformed|cancel  (default: happy)
  - happy:     assistant/message -> turn/end(stop) -> idle
  - tool_call: assistant/message -> tool/call event -> turn/end(stop) ->
               idle (the SDK collects the tool event as raw noise; the
               normalized surface never surfaces tool calls — AR-6)
  - malformed: assistant/message -> turn/end WITHOUT data.reason.kind ->
               idle — the SDK fails the run with SdkProtocol (the dsh
               decode-error surface) -> one OpFailed(decode_error)
  - cancel:    same as happy — the dsh adapter's cancel is an honest no-op
               (AR-6), so the turn runs to completion
- REQ_LOG=<path>  append one JSON object per received request
  ({"method": ..., "sessionId": ...}).
"""

import json
import os
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
                "version": "0.1.0-mock",
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
        session_event(session_id, {
            "type": "assistant/message",
            "data": {"message": {"content": [{"type": "text", "text": "mock dsh reply"}]}},
        })
        if scenario == "tool_call":
            # A tool-call-shaped event: the SDK collects it as raw noise
            # (AR-6 — no tool surface on this adapter).
            session_event(session_id, {
                "type": "tool/call",
                "data": {"tool": "bash", "input": {"command": "echo hi"}},
            })
        if scenario == "malformed":
            # turn/end without a string data.reason.kind -> the SDK fails
            # the run with SdkProtocol (the dsh decode-error surface).
            session_event(session_id, {
                "type": "turn/end",
                "data": {"reason": {}},
            })
        else:
            session_event(session_id, {
                "type": "turn/end",
                "data": {"reason": {"kind": "stop"}},
            })
        session_status(session_id, "idle")
        return

    if method == "shutdown":
        reply(req, None)
        return

    reply(req, {"error": {"code": -32601, "message": "method not found"}})


def main():
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
