#!/usr/bin/env python3
"""Minimal mock `claude` CLI (stream-json) for nexus-agent-host unit tests (v1.168 P1 T3).

Speaks the stream-json subset that ClaudeCliProvider uses: for every `user`
frame on stdin it emits one `assistant` text frame, then a `result`
(success) frame. The crate owns the actual `--print` / `--output-format
stream-json` / `--session-id` / `--resume` argv; the mock records its argv
per spawn so tests can assert session-continuity flags (AR-5).

Behavior knobs (env vars):
- REQ_LOG=<path>  append one JSON array (the child's argv) per spawned CLI.
- BLOCK_TURN=1    after the assistant frame, hold the turn open until the
                  provider kills the child (cancel/shutdown drop the crate
                  client, so the stream sees EOF — the provider's backstop
                  emits OpFailed(stream_closed)).
- BAD_FRAME=1     after the assistant frame, emit a frame with an unknown
                  top-level `type` tag — a typed-decode failure in the crate
                  (PD-3 row 2: one OpFailed(decode_error)).
"""

import json
import os
import sys


def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def log_argv():
    path = os.environ.get("REQ_LOG")
    if not path:
        return
    with open(path, "a", encoding="utf-8") as f:
        f.write(json.dumps(sys.argv[1:]) + "\n")


def assistant_frame():
    return {
        "type": "assistant",
        "message": {
            "id": "msg_01",
            "type": "message",
            "role": "assistant",
            "model": "mock",
            "content": [{"type": "text", "text": "hello from mock claude"}],
            "stop_reason": "end_turn",
        },
        "session_id": "sess_01",
        "uuid": "00000000-0000-0000-0000-000000000001",
    }


def result_frame():
    return {
        "type": "result",
        "subtype": "success",
        "is_error": False,
        "duration_ms": 1,
        "duration_api_ms": 1,
        "num_turns": 1,
        "session_id": "sess_01",
        "total_cost_usd": 0.01,
        "result": "turn completed",
    }


def main():
    log_argv()
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
        except json.JSONDecodeError:
            continue
        if req.get("type") == "user":
            send(assistant_frame())
            if os.environ.get("BAD_FRAME") == "1":
                send({"type": "future_frame", "payload": {"anything": True}})
            elif os.environ.get("BLOCK_TURN") == "1":
                # Hold the turn open; cancel/shutdown kill the child, so EOF
                # reaches the provider instead of this result frame.
                for _ in sys.stdin:
                    pass
            else:
                send(result_frame())
        elif req.get("type") == "control_request":
            # Interrupt ack — normally unreachable: cancel() sends the
            # interrupt and immediately drops the client (kills the child).
            send(
                {
                    "type": "control_response",
                    "request_id": req.get("request_id", ""),
                    "response": {"subtype": "success"},
                }
            )
            send(result_frame())


if __name__ == "__main__":
    main()
