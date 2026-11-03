#!/bin/sh
# Offline protocol fixture: this is not a real Codex response.
cat >/dev/null
printf '%s\n' '{"type":"item.completed","item":{"type":"agent_message","text":"A gateway gives a client one controlled doorway to a coding harness."}}' '{"type":"turn.completed","usage":{"input_tokens":1,"output_tokens":1}}'
