#!/usr/bin/env bash
# V1.51 T-A P0 (QC3 F-001) — LLM extract echo worker fixture.
#
# Responds to worker/acp_prompt with a fixed valid extraction JSON response
# (one World KB candidate). Used by daemon_boot_llm_wiring.rs to prove the
# ProductionWorkerProvider dispatches the IPC call end-to-end through a real
# worker process and that the response is parseable by LlmExtract::run.
#
# No python3 dependency — uses bash case matching only.

while IFS= read -r req; do
  case "$req" in
    *"worker/acp_prompt"*)
      # full_text is a JSON string containing a candidates JSON object.
      # The backslash-escaped quotes are literal JSON string escapes so the
      # outer JSON parser yields a valid inner JSON object.
      printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"done":true,"full_text":"{\"candidates\":[{\"canonical_name\":\"Lin Xia\",\"block_type\":\"character\",\"summary\":\"A warrior\",\"confidence\":0.9,\"source_quote\":\"Lin Xia drew her blade.\"}]}"}}'
      ;;
    *"worker/shutdown"*)
      printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{}}'
      exit 0
      ;;
    *)
      printf '%s\n' '{"jsonrpc":"2.0","id":1,"result":{"ok":true}}'
      ;;
  esac
done
