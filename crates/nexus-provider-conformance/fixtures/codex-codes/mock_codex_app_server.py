#!/usr/bin/env python3
"""Scripted `codex app-server` (JSON-RPC) speaker for nexus-provider-conformance (v1.180 P0 T2).

Speaks the JSON-RPC subset that CodexNativeProvider uses: `initialize`,
`thread/start`, `turn/start`, `turn/interrupt`. For every `turn/start` it
replies with a turn id, emits `turn/started`, one `item/agentMessage/delta`,
then `turn/completed`. Turn ids increment per `turn/start` (mock-turn-1,
mock-turn-2, ...) so cross-turn stale-frame tests can tell turns apart.

Behavior knobs (env vars):
- SCENARIO=happy|tool_call|malformed|cancel|mutate  (default: happy)
  - happy:     delta -> turn/completed(completed)
  - tool_call: delta -> item/toolUse (unknown method in codex-codes
               0.146.4; mapper-skipped) -> turn/completed(completed)
  - malformed: delta with non-string `delta` (typed-decode failure) ->
               wait for turn/interrupt -> turn/completed(interrupted)
  - cancel:    delta -> wait for turn/interrupt -> turn/completed(interrupted)
  - mutate:    turn/started carries a STALE turn id (mock-turn-0) — the
               provider's B-1 stale filter skips it, so the normalized
               stream has no OpStarted and the runner goes red (mutation
               probe)
- REQ_LOG=<path>  append one JSON object per received request
  ({"method": ..., "threadId": ...}).
"""

import json
import os
import sys

THREAD_ID = "mock-thread-1"

_turn_counter = 0


def current_turn_id():
    return "mock-turn-%d" % _turn_counter


def turn_notification(method, status, turn_id=None):
    return {
        "method": method,
        "params": {
            "threadId": THREAD_ID,
            "turn": {"id": turn_id or current_turn_id(), "status": status, "items": []},
        },
    }


def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


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


def wait_for_interrupt():
    """Hold the turn open until `turn/interrupt` arrives, then reply."""
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


def handle_request(req):
    global _turn_counter
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
        _turn_counter += 1
        send(
            {
                "id": msg_id,
                "result": {
                    "turn": {
                        "id": current_turn_id(),
                        "status": "inProgress",
                        "items": [],
                    }
                },
            }
        )
        scenario = os.environ.get("SCENARIO", "happy")
        if scenario == "mutate":
            # Mutation probe: the turn/started notification carries a
            # STALE turn id, so the provider's B-1 filter skips it and the
            # normalized stream has no OpStarted — the runner must go red.
            send(turn_notification("turn/started", "inProgress", turn_id="mock-turn-0"))
        else:
            send(turn_notification("turn/started", "inProgress"))
        send(
            {
                "method": "item/agentMessage/delta",
                "params": {
                    "delta": "hello from mock codex",
                    "itemId": "it_1",
                    "threadId": THREAD_ID,
                    "turnId": current_turn_id(),
                },
            }
        )
        if scenario == "tool_call":
            # A tool-use-shaped notification: unknown method in
            # codex-codes 0.146.4 -> Notification::Unknown -> mapper-skipped
            # (AR-1: no host event for tool calls this iteration).
            send(
                {
                    "method": "item/toolUse",
                    "params": {
                        "threadId": THREAD_ID,
                        "turnId": current_turn_id(),
                        "itemId": "it_tool",
                        "toolUse": {
                            "id": "toolu_01",
                            "name": "Bash",
                            "input": {"command": "echo hi"},
                        },
                    },
                }
            )
        if scenario == "malformed":
            # delta is not a string -> typed-decode failure in the crate.
            send(
                {
                    "method": "item/agentMessage/delta",
                    "params": {
                        "delta": 12345,
                        "itemId": "it_bad",
                        "threadId": THREAD_ID,
                        "turnId": current_turn_id(),
                    },
                }
            )
            wait_for_interrupt()
        elif scenario == "cancel":
            # Hold the turn open until turn/interrupt arrives.
            wait_for_interrupt()
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
