#!/usr/bin/env python3
"""Scripted `claude` CLI (stream-json) speaker for nexus-provider-conformance (v1.180 P0 T2).

Speaks the stream-json subset that ClaudeCliProvider uses: for every `user`
frame on stdin it emits one `assistant` frame, then a `result` (success)
frame. The provider owns the actual `--print` / `--output-format
stream-json` / `--session-id` / `--resume` argv; the fixture only speaks
frames.

Behavior knobs (env vars):
- SCENARIO=happy|tool_call|malformed|cancel  (default: happy)
  - happy:     assistant(text) -> result(success)
  - tool_call: assistant(text + tool_use block) -> result(success) — the
               tool_use block is AR-1-skipped by the mapper (no host event)
  - malformed: assistant(text) -> unknown top-level `type` frame — a
               typed-decode failure in the crate (PD-3 row 2: one
               OpFailed(decode_error))
  - cancel:    assistant(text) -> hold the turn open until the provider
               kills the child (cancel/shutdown drop the crate client, so
               the stream sees EOF — the provider's backstop emits
               OpFailed(stream_closed))
- REQ_LOG=<path>  append one JSON object per received stdin frame plus
                  lifecycle markers ({"marker": "blocked"} when the cancel
                  scenario enters its hold loop).
"""

import json
import os
import sys


def send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def log_entry(entry):
    path = os.environ.get("REQ_LOG")
    if not path:
        return
    with open(path, "a", encoding="utf-8") as f:
        f.write(json.dumps(entry) + "\n")


def assistant_frame(extra_blocks=None):
    content = [{"type": "text", "text": "hello from mock claude"}]
    if extra_blocks:
        content.extend(extra_blocks)
    return {
        "type": "assistant",
        "message": {
            "id": "msg_01",
            "type": "message",
            "role": "assistant",
            "model": "mock",
            "content": content,
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
    scenario = os.environ.get("SCENARIO", "happy")
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
        except json.JSONDecodeError:
            continue
        log_entry({"frame": req.get("type")})
        if req.get("type") == "user":
            if scenario == "cancel":
                # Marker BEFORE the assistant frame: when the test pumps
                # the first event, the fixture is provably alive and
                # holding the turn (REQ_LOG receipt).
                log_entry({"marker": "blocked"})
            if scenario == "tool_call":
                send(
                    assistant_frame(
                        [
                            {
                                "type": "tool_use",
                                "id": "toolu_01",
                                "name": "Bash",
                                "input": {"command": "echo hi"},
                            }
                        ]
                    )
                )
            else:
                send(assistant_frame())
            if scenario == "malformed":
                # Unknown top-level `type` tag: a typed-decode failure in
                # the crate (PD-3 row 2: one OpFailed(decode_error)).
                send({"type": "future_frame", "payload": {"anything": True}})
            elif scenario == "cancel":
                # Hold the turn open; cancel/shutdown kill the child, so
                # EOF reaches the provider instead of this result frame.
                for hold_line in sys.stdin:
                    hold_line = hold_line.strip()
                    if not hold_line:
                        continue
                    try:
                        hold_req = json.loads(hold_line)
                    except json.JSONDecodeError:
                        continue
                    log_entry({"frame": hold_req.get("type")})
                    if hold_req.get("type") == "control_request":
                        # Interrupt ack — normally unreachable: cancel()
                        # sends the interrupt and immediately drops the
                        # client (kills the child).
                        send(
                            {
                                "type": "control_response",
                                "request_id": hold_req.get("request_id", ""),
                                "response": {"subtype": "success"},
                            }
                        )
                        send(result_frame())
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
