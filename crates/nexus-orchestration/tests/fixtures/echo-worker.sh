#!/usr/bin/env bash
# Echo worker — reads JSON-RPC requests, responds with echo.
# For worker/acp_prompt: extracts prompt, sends chunk + final result.
# For worker/health: replies with health status.
# For worker/shutdown: replies and exits.

while IFS= read -r req; do
  method=$(echo "$req" | python3 -c "import json,sys; d=json.loads(sys.stdin.read()); print(d.get('method',''), end='')" 2>/dev/null)

  case "$method" in
    worker/health)
      printf '{"jsonrpc":"2.0","id":1,"result":{"uptime_ms":0,"acp_session_state":"ready"}}\n'
      ;;
    worker/acp_prompt)
      prompt=$(echo "$req" | python3 -c "import json,sys; d=json.loads(sys.stdin.read()); print(d.get('params',{}).get('prompt',''), end='')" 2>/dev/null)
      # Send final result only (no intermediate chunk notification).
      # The IPC client reads one line per request.
      printf '{"jsonrpc":"2.0","id":1,"result":{"done":true,"full_text":"%s"}}\n' "$prompt"
      ;;
    worker/shutdown)
      printf '{"jsonrpc":"2.0","id":1,"result":{}}\n'
      exit 0
      ;;
    *)
      printf '{"jsonrpc":"2.0","id":1,"result":{"ok":true}}\n'
      ;;
  esac
done
