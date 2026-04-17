#!/usr/bin/env bash
# Dummy worker — reads one line, echoes a JSON-RPC success reply, exits.
read -r _request
printf '{"jsonrpc":"2.0","id":1,"result":{"ok":true,"uptime_ms":0,"acp_session_state":"idle"}}\n'
