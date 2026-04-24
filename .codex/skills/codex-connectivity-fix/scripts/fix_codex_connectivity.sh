#!/usr/bin/env bash
set -euo pipefail

ACTION="${1:-}"
shift || true

USER_PROXY_URL="${CODEX_PROXY_URL:-}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --proxy-url)
      USER_PROXY_URL="${2:-}"
      shift 2
      ;;
    *)
      echo "Unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

usage() {
  cat <<'EOF'
Usage:
  bash codex-connectivity-fix/scripts/fix_codex_connectivity.sh diagnose
  bash codex-connectivity-fix/scripts/fix_codex_connectivity.sh apply-wrapper [--proxy-url URL]
  bash codex-connectivity-fix/scripts/fix_codex_connectivity.sh all [--proxy-url URL]
EOF
}

have_cmd() {
  command -v "$1" >/dev/null 2>&1
}

find_latest_nvm_node_dir() {
  local root="$HOME/.nvm/versions/node"
  [[ -d "$root" ]] || return 1
  find "$root" -mindepth 1 -maxdepth 1 -type d | sort -V | tail -n 1
}

find_codex_assets() {
  local node_dir
  if node_dir="$(find_latest_nvm_node_dir 2>/dev/null)"; then
    local node_bin="$node_dir/bin/node"
    local codex_js="$node_dir/lib/node_modules/@openai/codex/bin/codex.js"
    if [[ -x "$node_bin" && -f "$codex_js" ]]; then
      printf '%s\n%s\n%s\n' "$node_dir" "$node_bin" "$codex_js"
      return 0
    fi
  fi

  local node_bin_fallback=""
  local codex_bin_fallback=""
  node_bin_fallback="$(command -v node 2>/dev/null || true)"
  codex_bin_fallback="$(command -v codex 2>/dev/null || true)"
  [[ -n "$node_bin_fallback" ]] || return 1
  [[ -n "$codex_bin_fallback" ]] || return 1

  local node_bin_resolved
  local codex_js_resolved
  node_bin_resolved="$(readlink -f "$node_bin_fallback" 2>/dev/null || printf '%s\n' "$node_bin_fallback")"
  codex_js_resolved="$(readlink -f "$codex_bin_fallback" 2>/dev/null || printf '%s\n' "$codex_bin_fallback")"

  [[ -x "$node_bin_resolved" ]] || return 1
  [[ -f "$codex_js_resolved" ]] || return 1

  printf '%s\n%s\n%s\n' "$(dirname "$(dirname "$node_bin_resolved")")" "$node_bin_resolved" "$codex_js_resolved"
}

detect_proxy_url() {
  if [[ -n "${USER_PROXY_URL:-}" ]]; then
    printf '%s\n' "$USER_PROXY_URL"
    return 0
  fi

  have_cmd ss || return 1

  if ss -lnt 2>/dev/null | grep -q '127.0.0.1:10808'; then
    printf '%s\n' 'socks5://127.0.0.1:10808'
    return 0
  fi
  if ss -lnt 2>/dev/null | grep -q '127.0.0.1:7891'; then
    printf '%s\n' 'socks5://127.0.0.1:7891'
    return 0
  fi
  if ss -lnt 2>/dev/null | grep -q '127.0.0.1:7890'; then
    printf '%s\n' 'http://127.0.0.1:7890'
    return 0
  fi
  if ss -lnt 2>/dev/null | grep -q '127.0.0.1:20171'; then
    printf '%s\n' 'http://127.0.0.1:20171'
    return 0
  fi
  return 1
}

curl_with_proxy_flag() {
  local proxy_url="$1"
  case "$proxy_url" in
    socks5://*)
      printf -- '--socks5-hostname %s\n' "${proxy_url#socks5://}"
      ;;
    http://*|https://*)
      printf -- '--proxy %s\n' "$proxy_url"
      ;;
    *)
      return 1
      ;;
  esac
}

print_header() {
  printf '\n== %s ==\n' "$1"
}

diagnose() {
  print_header "codex in PATH"
  command -v codex || true
  codex --version || true

  print_header "shell node"
  command -v node || true
  node -v || true

  print_header "nvm codex assets"
  if mapfile -t assets < <(find_codex_assets); then
    printf 'node_dir=%s\nnode_bin=%s\ncodex_js=%s\n' "${assets[0]}" "${assets[1]}" "${assets[2]}"
    "${assets[1]}" "${assets[2]}" --version || true
  else
    echo "No usable Codex installation found under ~/.nvm"
  fi

  print_header "auth"
  if [[ -f "$HOME/.codex/auth.json" ]]; then
    echo "~/.codex/auth.json exists"
  else
    echo "~/.codex/auth.json missing"
  fi

  print_header "proxy listeners"
  if have_cmd ss; then
    ss -lntp 2>/dev/null | grep -E '(:10808|:7890|:7891|:20171)' || true
  else
    echo "ss not available"
  fi

  print_header "dns"
  if have_cmd getent; then
    getent hosts api.openai.com || true
    getent hosts chat.openai.com || true
  fi
  if have_cmd python3; then
    python3 - <<'PY'
import socket
for host in ("api.openai.com", "chat.openai.com", "oauth.openai.com"):
    try:
        print(host, socket.gethostbyname_ex(host))
    except Exception as exc:
        print(host, "ERR", exc)
PY
  fi

  print_header "direct curl"
  curl -I -m 10 https://api.openai.com/v1/models 2>&1 | sed -n '1,30p' || true

  print_header "proxy curl"
  if proxy_url="$(detect_proxy_url)"; then
    echo "proxy_url=$proxy_url"
    proxy_flag="$(curl_with_proxy_flag "$proxy_url")" || true
    if [[ -n "${proxy_flag:-}" ]]; then
      # shellcheck disable=SC2086
      curl $proxy_flag -I -m 15 https://api.openai.com/v1/models 2>&1 | sed -n '1,40p' || true
    fi
  else
    echo "No supported local proxy listener detected"
  fi

  print_header "diagnosis hints"
  cat <<'EOF'
- If `codex` is missing in PATH but `~/.nvm/.../bin/codex` exists, prefer a user-level wrapper.
- If `codex` crashes with JS syntax/module errors, the system Node is likely too old.
- If direct curl times out but proxy curl returns HTTP status, Codex needs proxy inheritance.
- If DNS results look suspicious, treat that as a network issue first, not an auth issue.
EOF
}

apply_wrapper() {
  mapfile -t assets < <(find_codex_assets)
  local node_dir="${assets[0]}"
  local node_bin="${assets[1]}"
  local codex_js="${assets[2]}"
  local wrapper_dir="$HOME/.local/bin"
  local wrapper_path="$wrapper_dir/codex"
  local proxy_url=""

  mkdir -p "$wrapper_dir"

  if [[ -f "$wrapper_path" || -L "$wrapper_path" ]]; then
    cp -a "$wrapper_path" "$wrapper_path.bak.$(date +%Y%m%d%H%M%S)"
  fi

  if proxy_url="$(detect_proxy_url)"; then
    :
  else
    proxy_url=""
  fi

  cat >"$wrapper_path" <<EOF
#!/usr/bin/env bash
set -euo pipefail

NODE_BIN="$node_bin"
CODEX_JS="$codex_js"
DEFAULT_PROXY_URL="$proxy_url"

if [ ! -x "\$NODE_BIN" ]; then
  echo "codex wrapper error: missing Node runtime at \$NODE_BIN" >&2
  exit 1
fi

if [ ! -f "\$CODEX_JS" ]; then
  echo "codex wrapper error: missing Codex entrypoint at \$CODEX_JS" >&2
  exit 1
fi

if [ -z "\${ALL_PROXY:-}" ] && [ -z "\${all_proxy:-}" ] && [ -n "\$DEFAULT_PROXY_URL" ]; then
  export ALL_PROXY="\$DEFAULT_PROXY_URL"
  export all_proxy="\$ALL_PROXY"
  export HTTPS_PROXY="\${HTTPS_PROXY:-\$DEFAULT_PROXY_URL}"
  export https_proxy="\${https_proxy:-\$DEFAULT_PROXY_URL}"
  export HTTP_PROXY="\${HTTP_PROXY:-\$DEFAULT_PROXY_URL}"
  export http_proxy="\${http_proxy:-\$DEFAULT_PROXY_URL}"
fi

export NO_PROXY="\${NO_PROXY:-127.0.0.1,localhost}"
export no_proxy="\${no_proxy:-127.0.0.1,localhost}"

exec "\$NODE_BIN" "\$CODEX_JS" "\$@"
EOF

  chmod 755 "$wrapper_path"

  print_header "wrapper written"
  echo "$wrapper_path"
  echo "node_dir=$node_dir"
  if [[ -n "$proxy_url" ]]; then
    echo "default_proxy=$proxy_url"
  else
    echo "default_proxy=<none detected>"
  fi
}

verify_wrapper() {
  print_header "verify wrapper"
  bash -lc 'hash -r; command -v codex; codex --version'
}

case "$ACTION" in
  diagnose)
    diagnose
    ;;
  apply-wrapper)
    apply_wrapper
    verify_wrapper
    ;;
  all)
    diagnose
    apply_wrapper
    verify_wrapper
    ;;
  *)
    usage >&2
    exit 2
    ;;
esac
