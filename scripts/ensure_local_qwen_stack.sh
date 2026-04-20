#!/usr/bin/env bash

set -euo pipefail

STATE_DIR="${HOME}/.codex/local-qwen"
BACKEND_PORT="${BACKEND_PORT:-12434}"
PROXY_PORT="${PROXY_PORT:-8080}"
BACKEND_PID_FILE="${STATE_DIR}/llama-server.pid"
PROXY_PID_FILE="${STATE_DIR}/alias-proxy.pid"
BACKEND_LOG="${STATE_DIR}/llama-server.log"
PROXY_LOG="${STATE_DIR}/alias-proxy.log"
MODEL_PATH="${HOME}/.lmstudio/models/HauhauCS/Qwen3.6-35B-A3B-Uncensored-HauhauCS-Aggressive/Qwen3.6-35B-A3B-Uncensored-HauhauCS-Aggressive-Q8_K_P.gguf"
LLAMA_SERVER="/home/dgx/github/llama.cpp/build-cuda-release/bin/llama-server"
PROXY_SCRIPT="/home/dgx/github/codex/scripts/local_qwen_alias_proxy.py"
ALIASES="qwen36-think-general,qwen36-think-code,qwen36-nothink-general,qwen36-nothink-reason"

mkdir -p "${STATE_DIR}"

is_backend_ready() {
    curl -fsS "http://127.0.0.1:${BACKEND_PORT}/health" >/dev/null 2>&1
}

is_proxy_ready() {
    curl -fsS "http://127.0.0.1:${PROXY_PORT}/healthz" >/dev/null 2>&1
}

cleanup_pid_file() {
    local pid_file="$1"
    if [[ -f "${pid_file}" ]]; then
        local pid
        pid="$(cat "${pid_file}" 2>/dev/null || true)"
        if [[ -n "${pid}" ]] && kill -0 "${pid}" >/dev/null 2>&1; then
            return
        fi
        rm -f "${pid_file}"
    fi
}

wait_ready() {
    local name="$1"
    local check_cmd="$2"
    local log_file="$3"

    for _ in $(seq 1 180); do
        if eval "${check_cmd}"; then
            return 0
        fi
        sleep 1
    done

    echo "[local-qwen] ${name} failed to become ready; recent log:" >&2
    tail -n 80 "${log_file}" >&2 || true
    return 1
}

start_backend() {
    cleanup_pid_file "${BACKEND_PID_FILE}"

    if is_backend_ready; then
        pgrep -n -f "llama-server.*--port ${BACKEND_PORT}" >"${BACKEND_PID_FILE}" || true
        return 0
    fi

    if [[ ! -x "${LLAMA_SERVER}" ]]; then
        echo "[local-qwen] missing llama-server binary: ${LLAMA_SERVER}" >&2
        return 1
    fi

    if [[ ! -f "${MODEL_PATH}" ]]; then
        echo "[local-qwen] missing model file: ${MODEL_PATH}" >&2
        return 1
    fi

    setsid "${LLAMA_SERVER}" \
        --offline \
        -m "${MODEL_PATH}" \
        --host 127.0.0.1 \
        --port "${BACKEND_PORT}" \
        -c 131072 \
        -b 1024 \
        -ub 1024 \
        -ngl all \
        --jinja \
        --reasoning auto \
        --reasoning-format deepseek \
        -a "${ALIASES}" \
        --no-webui \
        >"${BACKEND_LOG}" 2>&1 </dev/null &
    sleep 1
    pgrep -n -f "llama-server.*--port ${BACKEND_PORT}" >"${BACKEND_PID_FILE}"

    wait_ready "llama-server" "is_backend_ready" "${BACKEND_LOG}"
}

start_proxy() {
    cleanup_pid_file "${PROXY_PID_FILE}"

    if is_proxy_ready; then
        pgrep -n -f "local_qwen_alias_proxy.py --listen-host 127.0.0.1 --listen-port ${PROXY_PORT}" >"${PROXY_PID_FILE}" || true
        return 0
    fi

    setsid python3 "${PROXY_SCRIPT}" \
        --listen-host 127.0.0.1 \
        --listen-port "${PROXY_PORT}" \
        --upstream-base "http://127.0.0.1:${BACKEND_PORT}" \
        >"${PROXY_LOG}" 2>&1 </dev/null &
    sleep 1
    pgrep -n -f "local_qwen_alias_proxy.py --listen-host 127.0.0.1 --listen-port ${PROXY_PORT}" >"${PROXY_PID_FILE}"

    wait_ready "alias proxy" "is_proxy_ready" "${PROXY_LOG}"
}

restart_proxy() {
    local pattern="local_qwen_alias_proxy.py --listen-host 127.0.0.1 --listen-port ${PROXY_PORT}"
    if pgrep -f "${pattern}" >/dev/null 2>&1; then
        pkill -f "${pattern}" || true
        for _ in $(seq 1 20); do
            if ! pgrep -f "${pattern}" >/dev/null 2>&1; then
                break
            fi
            sleep 1
        done
    fi
    rm -f "${PROXY_PID_FILE}"
    start_proxy
}

show_status() {
    echo "backend_port=${BACKEND_PORT}"
    echo "proxy_port=${PROXY_PORT}"
    echo "backend_ready=$(is_backend_ready && echo yes || echo no)"
    echo "proxy_ready=$(is_proxy_ready && echo yes || echo no)"
    echo "backend_pid=$(cat "${BACKEND_PID_FILE}" 2>/dev/null || echo -)"
    echo "proxy_pid=$(cat "${PROXY_PID_FILE}" 2>/dev/null || echo -)"
}

case "${1:-ensure}" in
    ensure)
        start_backend
        restart_proxy
        show_status
        ;;
    status)
        show_status
        ;;
    *)
        echo "usage: $0 [ensure|status]" >&2
        exit 2
        ;;
esac
