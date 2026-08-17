#!/usr/bin/env python3
"""Minimal mock `codex app-server` for nexus-agent-host unit tests (v1.168 P1 T2).

Speaks the JSON-RPC subset that CodexNativeProvider uses: `initialize`,
`thread/start`, `turn/start`, `turn/interrupt`. For every `turn/start` it
replies with a turn id, emits `turn/started`, one `item/agentMessage/delta`,
then `turn/completed`.

Behavior knobs (env vars):
- REQ_LOG=<path>  append one JSON object per received request
  ({"method": ..., "threadId": ...}) for thread-reuse assertions.
- BLOCK_TURN=1    after emitting the delta, wait for `turn/interrupt` before
  emitting `turn/completed` (status interrupted) — exercises cancel.
"""

import json
import os
import sys

THREAD_ID = "mock-thread-1"
TURN_ID = "mock-turn-1"


def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def turn_notification(method, status):
    return {
        "method": method,
        "params": {
            "threadId": THREAD_ID,
            "turn": {"id": TURN_ID, "status": status, "items": []},
        },
    }


def log_request(req):
    path = os.environ.get("REQ_LOG")
    if not path:
        return
    params = req.get("params") or {}
    with open(path, "a", encoding="utf-8") as f:
        f.write(
            json.dumps(
                {"method": req.get("method"), "threadId": params.get("threadId")}
            )
            + "\n"
        )


def handle_request(req):
    method = req.get("method")
    msg_id = req.get("id")
    log_request(req)
    if method == "initialize":
        send({"id": msg_id, "result": {}})
    elif method == "thread/start":
        send(
            {
                "id": msg_id,
                "result": {
                    "approvalPolicy": "never",
                    "cwd": os.getcwd(),
                    "thread": {"id": THREAD_ID},
                },
            }
        )
    elif method == "turn/start":
        send(
            {
                "id": msg_id,
                "result": {"turn": {"id": TURN_ID, "status": "inProgress", "items": []}},
            }
        )
        send(turn_notification("turn/started", "inProgress"))
        send(
            {
                "method": "item/agentMessage/delta",
                "params": {
                    "delta": "hello from mock codex",
                    "itemId": "it_1",
                    "threadId": THREAD_ID,
                    "turnId": TURN_ID,
                },
            }
        )
        if os.environ.get("BLOCK_TURN") == "1":
            # Hold the turn open until turn/interrupt arrives.
            for line in sys.stdin:
                line = line.strip()
                if not line:
                    continue
                try:
                    req2 = json.loads(line)
                except json.JSONDecodeError:
                    continue
                log_request(req2)
                if req2.get("method") == "turn/interrupt":
                    send({"id": req2.get("id"), "result": {}})
                    send(turn_notification("turn/completed", "interrupted"))
                    break
        else:
            send(turn_notification("turn/completed", "completed"))
    elif method == "turn/interrupt":
        send({"id": msg_id, "result": {}})
        send(turn_notification("turn/completed", "interrupted"))


def main():
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
        except json.JSONDecodeError:
            continue
        handle_request(req)


if __name__ == "__main__":
    main()
