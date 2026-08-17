#!/usr/bin/env python3
"""Minimal mock `codex app-server` for nexus-agent-host unit tests (v1.168 P1 T2).

Speaks the JSON-RPC subset that CodexNativeProvider uses: `initialize`,
`thread/start`, `turn/start`, `turn/interrupt`. For every `turn/start` it
replies with a turn id, emits `turn/started`, one `item/agentMessage/delta`,
then `turn/completed`. Turn ids increment per `turn/start` (mock-turn-1,
mock-turn-2, ...) so cross-turn stale-frame tests can tell turns apart.

Behavior knobs (env vars):
- REQ_LOG=<path>  append one JSON object per received request
  ({"method": ..., "threadId": ...}) for thread-reuse assertions.
- BLOCK_TURN=1    after emitting the delta, wait for `turn/interrupt` before
  emitting `turn/completed` (status interrupted) — exercises cancel.
- BAD_FRAME=1     after emitting the delta, send an `item/agentMessage/delta`
  whose params fail typed decode (delta is not a string), then wait for
  `turn/interrupt` — exercises the decode-error interrupt + drain path.
- STALE_TURN_COMPLETED=1  on the SECOND `turn/start`, emit a leftover
  `turn/completed` for the FIRST turn before the new turn's frames —
  exercises the B-1 stale-terminal filter.
"""

import json
import os
import sys

THREAD_ID = "mock-thread-1"

_turn_counter = 0


def current_turn_id():
    return "mock-turn-%d" % _turn_counter


def turn_notification(method, status):
    return {
        "method": method,
        "params": {
            "threadId": THREAD_ID,
            "turn": {"id": current_turn_id(), "status": status, "items": []},
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
        if os.environ.get("STALE_TURN_COMPLETED") == "1" and _turn_counter == 2:
            # A leftover terminal from the FIRST turn, still in the pipe when
            # the second turn starts (B-1 regression knob).
            send(
                {
                    "method": "turn/completed",
                    "params": {
                        "threadId": THREAD_ID,
                        "turn": {
                            "id": "mock-turn-%d" % (_turn_counter - 1),
                            "status": "completed",
                            "items": [],
                        },
                    },
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
                    "turnId": current_turn_id(),
                },
            }
        )
        if os.environ.get("BAD_FRAME") == "1" and _turn_counter == 1:
            # delta is not a string → typed-decode failure in the crate.
            # Only the first turn so a follow-up execute sees a clean turn.
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
        elif os.environ.get("BLOCK_TURN") == "1":
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
