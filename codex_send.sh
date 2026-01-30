#!/usr/bin/env bash
set -euo pipefail

BASE_URL="${BASE_URL:-http://127.0.0.1:7000}"
PRETTY=0
THREAD_ID=""

while [ $# -gt 0 ]; do
  case "$1" in
    --pretty)
      PRETTY=1
      shift
      ;;
    -h|--help)
      echo "用法: $0 [--pretty] <thread_id>"
      exit 0
      ;;
    *)
      THREAD_ID="$1"
      shift
      ;;
  esac
done

if [ -z "$THREAD_ID" ]; then
  echo "用法: $0 [--pretty] <thread_id>"
  exit 1
fi

echo "请输入要发送的内容，结束请按 Ctrl+D："
MESSAGE=$(cat)

REQUEST_BODY="$(jq -n --arg threadId "$THREAD_ID" --arg message "$MESSAGE" \
  '{threadId:$threadId, message:$message}')"

if [ "$PRETTY" -eq 1 ]; then
  curl -sS -N -X POST "${BASE_URL}/turn/start" \
    -H "Content-Type: application/json" \
    -d "$REQUEST_BODY" \
  | sed -n 's/^data: //p' \
  | jq -rj '
      if .method=="item/agentMessage/delta" then .params.delta
      elif .method=="codex/event/task_complete" then "\n\n" + .params.msg.last_agent_message + "\n"
      else empty end'
else
  curl -sS -N -X POST "${BASE_URL}/turn/start" \
    -H "Content-Type: application/json" \
    -d "$REQUEST_BODY"
fi
