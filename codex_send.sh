#!/usr/bin/env bash
set -euo pipefail

BASE_URL="${BASE_URL:-http://127.0.0.1:7000}"
DEBUG=0
PRETTY=0
END_MARKER=""
THREAD_ID=""

while [ $# -gt 0 ]; do
  case "$1" in
    --pretty)
      PRETTY=1
      shift
      ;;
    --debug)
      DEBUG=1
      shift
      ;;
    --end-marker)
      END_MARKER="${2:-}"
      shift 2
      ;;
    -h|--help)
      echo "用法: $0 [--pretty] [--debug] [--end-marker STR] <thread_id>"
      exit 0
      ;;
    *)
      THREAD_ID="$1"
      shift
      ;;
  esac
done

if [ -z "$THREAD_ID" ]; then
  echo "用法: $0 [--pretty] [--debug] [--end-marker STR] <thread_id>"
  exit 1
fi

if [ -n "$END_MARKER" ]; then
  echo "请输入要发送的内容，单独一行输入 ${END_MARKER} 结束："
  MESSAGE=""
  while IFS= read -r line; do
    if [ "$line" = "$END_MARKER" ]; then
      break
    fi
    MESSAGE="${MESSAGE}${line}"$'\n'
  done
else
  echo "请输入要发送的内容，结束请按 Ctrl+D："
  MESSAGE=$(cat)
fi

if [ "$DEBUG" -eq 1 ]; then
  LEN=$(printf '%s' "$MESSAGE" | wc -c | tr -d ' ')
  echo "[debug] message bytes: $LEN" >&2
  echo "[debug] first 200 chars:" >&2
  printf '%s' "$MESSAGE" | head -c 200 | sed 's/^/[debug] /' >&2
  echo >&2
fi


REQUEST_BODY="$(jq -n --arg threadId "$THREAD_ID" --arg message "$MESSAGE" \
  '{threadId:$threadId, message:$message}')"

if [ "$PRETTY" -eq 1 ]; then
  curl -sS -N -X POST "${BASE_URL}/turn/start" \
    -H "Content-Type: application/json" \
    -d "$REQUEST_BODY" \
  | while IFS= read -r line; do
      case "$line" in
        data:\ *)
          payload="${line#data: }"
          method=$(printf '%s' "$payload" | jq -r '.method // empty')
          if [ "$method" = "item/agentMessage/delta" ]; then
            printf '%s' "$(printf '%s' "$payload" | jq -r '.params.delta // ""')"
          elif [ "$method" = "codex/event/task_complete" ]; then
            printf '\n\n%s\n' "$(printf '%s' "$payload" | jq -r '.params.msg.last_agent_message // ""')"
          elif [ "$method" = "item/tool/requestUserInput" ]; then
            request_id_json=$(printf '%s' "$payload" | jq -c '.id')
            printf '\n\n[INPUT REQUIRED]\nrequestId: %s\n' "$request_id_json"

            questions=$(printf '%s' "$payload" | jq -c '.params.questions[]')
            answers='{}'
            while IFS= read -r q; do
              qid=$(printf '%s' "$q" | jq -r '.id')
              header=$(printf '%s' "$q" | jq -r '.header')
              question=$(printf '%s' "$q" | jq -r '.question')
              options=$(printf '%s' "$q" | jq -r '.options // empty')

              printf '[%s] %s\n%s\n' "$qid" "$header" "$question"
              if [ -n "$options" ]; then
                printf 'Options:\n'
                printf '%s' "$q" | jq -r '.options[] | "- " + .label + ": " + .description'
              fi
              printf 'Answer (comma-separated if multiple): '
              IFS= read -r input < /dev/tty

              arr=$(printf '%s\n' "$input" | tr ',' '\n' | sed 's/^ *//;s/ *$//' \
                | jq -R . | jq -s .)
              answers=$(jq -n --argjson obj "$answers" --arg id "$qid" --argjson arr "$arr" \
                '$obj + {($id): $arr}')
            done <<< "$questions"

            jq -n --argjson requestId "$request_id_json" --argjson answers "$answers" \
              '{requestId:$requestId, answers:$answers}' \
            | curl -sS -X POST "${BASE_URL}/tool/answer" \
                -H "Content-Type: application/json" \
                -d @- >/dev/null
          fi
          ;;
      esac
    done
else
  curl -sS -N -X POST "${BASE_URL}/turn/start" \
    -H "Content-Type: application/json" \
    -d "$REQUEST_BODY"
fi
